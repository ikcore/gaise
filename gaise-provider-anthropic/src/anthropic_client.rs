use crate::contracts::*;
use async_trait::async_trait;
use base64::Engine;
use futures_util::{Stream, StreamExt};
use gaise_core::GaiseClient;
use gaise_core::contracts::{
    GaiseContent, GaiseEmbeddingsRequest, GaiseEmbeddingsResponse, GaiseFunctionCall,
    GaiseInstructRequest, GaiseInstructResponse, GaiseInstructStreamResponse, GaiseMessage,
    GaiseStreamChunk, GaiseTool, GaiseToolCall, GaiseToolParameter, GaiseUsage, OneOrMany,
    file_media_type, image_media_type,
};
use std::collections::HashMap;
use std::pin::Pin;

fn map_input_usage(usage: &AnthropicUsage) -> HashMap<String, usize> {
    let mut input = HashMap::new();
    let effective_input =
        usage.input_tokens + usage.cache_read_input_tokens + usage.cache_creation_input_tokens;
    input.insert("input_tokens".to_string(), usage.input_tokens);
    if usage.cache_read_input_tokens > 0 {
        input.insert(
            "cache_read_input_tokens".to_string(),
            usage.cache_read_input_tokens,
        );
    }
    if usage.cache_creation_input_tokens > 0 {
        input.insert(
            "cache_creation_input_tokens".to_string(),
            usage.cache_creation_input_tokens,
        );
    }
    if usage.cache_read_input_tokens > 0 || usage.cache_creation_input_tokens > 0 {
        input.insert("effective_input_tokens".to_string(), effective_input);
    }
    if let Some(cache) = &usage.cache_creation {
        input.insert(
            "cache_creation_1h_input_tokens".to_string(),
            cache.ephemeral_1h_input_tokens,
        );
        input.insert(
            "cache_creation_5m_input_tokens".to_string(),
            cache.ephemeral_5m_input_tokens,
        );
    }
    input
}

fn map_output_usage(usage: &AnthropicUsage) -> HashMap<String, usize> {
    let mut output = HashMap::new();
    output.insert("output_tokens".to_string(), usage.output_tokens);
    if let Some(details) = &usage.output_tokens_details {
        output.insert("reasoning_tokens".to_string(), details.thinking_tokens);
    }
    output
}

fn map_request_usage(usage: &AnthropicUsage) -> Option<HashMap<String, usize>> {
    let mut request = HashMap::new();
    if let Some(tools) = &usage.server_tool_use {
        request.insert("web_fetch_requests".to_string(), tools.web_fetch_requests);
        request.insert("web_search_requests".to_string(), tools.web_search_requests);
    }
    (!request.is_empty()).then_some(request)
}

pub struct GaiseClientAnthropic {
    api_url: String,
    api_key: String,
    api_version: String,
    client: reqwest::Client,
}

fn append_anthropic_content(blocks: &mut Vec<AnthropicContentBlock>, content: GaiseContent) {
    match content {
        GaiseContent::Text { text } => blocks.push(AnthropicContentBlock::Text {
            text,
            cache_control: None,
        }),
        GaiseContent::Image { data, format } => {
            let media_type = image_media_type(format.as_deref());
            if matches!(
                media_type.as_str(),
                "image/jpeg" | "image/png" | "image/gif" | "image/webp"
            ) {
                blocks.push(AnthropicContentBlock::Image {
                    source: AnthropicImageSource {
                        r#type: "base64".to_string(),
                        media_type,
                        data: base64::prelude::BASE64_STANDARD.encode(data),
                    },
                    cache_control: None,
                });
            } else {
                blocks.push(AnthropicContentBlock::Text {
                    text: format!("[Unsupported image type for Anthropic Messages: {media_type}]"),
                    cache_control: None,
                });
            }
        }
        GaiseContent::File { data, name } => {
            let inferred_media_type = file_media_type(name.as_deref());
            if inferred_media_type == "application/pdf" {
                blocks.push(AnthropicContentBlock::Document {
                    source: AnthropicDocumentSource {
                        r#type: "base64".to_string(),
                        media_type: "application/pdf".to_string(),
                        data: base64::prelude::BASE64_STANDARD.encode(data),
                    },
                    title: name,
                    cache_control: None,
                });
            } else if let Ok(text) = String::from_utf8(data) {
                blocks.push(AnthropicContentBlock::Document {
                    source: AnthropicDocumentSource {
                        r#type: "text".to_string(),
                        // Anthropic's plain-text document source only accepts this
                        // MIME type, including for CSV and Markdown input.
                        media_type: "text/plain".to_string(),
                        data: text,
                    },
                    title: name,
                    cache_control: None,
                });
            } else {
                let name = name.unwrap_or_else(|| "document".to_string());
                blocks.push(AnthropicContentBlock::Text {
                    text: format!(
                        "[Unsupported binary document for Anthropic Messages; convert to PDF or text: {name}]"
                    ),
                    cache_control: None,
                });
            }
        }
        GaiseContent::Audio { format, .. } => {
            let format = format.unwrap_or_else(|| "unknown".to_string());
            blocks.push(AnthropicContentBlock::Text {
                text: format!("[Unsupported audio input for Anthropic Messages: {format}]"),
                cache_control: None,
            });
        }
        GaiseContent::Reasoning { text, signature } => {
            blocks.push(AnthropicContentBlock::Thinking {
                thinking: text,
                signature,
            });
        }
        GaiseContent::RedactedReasoning { data } => {
            let data = String::from_utf8(data).unwrap_or_else(|error| {
                base64::prelude::BASE64_STANDARD.encode(error.into_bytes())
            });
            blocks.push(AnthropicContentBlock::RedactedThinking { data });
        }
        GaiseContent::Parts { parts } => {
            for part in parts {
                append_anthropic_content(blocks, part);
            }
        }
    }
}

fn collect_text(content: &GaiseContent, output: &mut Vec<String>) {
    match content {
        GaiseContent::Text { text } => output.push(text.clone()),
        GaiseContent::Parts { parts } => {
            for part in parts {
                collect_text(part, output);
            }
        }
        _ => {}
    }
}

fn anthropic_reasoning_config(
    model: &str,
    config: Option<&gaise_core::contracts::GaiseGenerationConfig>,
) -> (Option<AnthropicThinking>, Option<AnthropicOutputConfig>) {
    let Some(config) = config else {
        return (None, None);
    };
    let model = model.to_ascii_lowercase();
    let adaptive_only = [
        "claude-fable-5",
        "claude-mythos-5",
        "claude-opus-4-8",
        "claude-opus-4-7",
        "claude-sonnet-5",
        "claude-mythos-preview",
    ]
    .iter()
    .any(|family| model.contains(family));
    let adaptive = adaptive_only
        || ["claude-opus-4-6", "claude-sonnet-4-6"]
            .iter()
            .any(|family| model.contains(family));
    let supports_effort =
        adaptive || model.contains("claude-opus-4-5") || model.contains("claude-mythos-preview");
    let display = config.include_thoughts.map(|include| {
        if include {
            "summarized".to_string()
        } else {
            "omitted".to_string()
        }
    });

    let output_config = config
        .thinking_effort
        .as_ref()
        .filter(|_| supports_effort)
        .map(|effort| AnthropicOutputConfig {
            effort: effort.to_ascii_lowercase(),
        });

    let thinking = if adaptive
        && (config.thinking_effort.is_some()
            || config.thinking_tokens.is_some()
            || display.is_some())
    {
        Some(AnthropicThinking {
            r#type: "adaptive".to_string(),
            budget_tokens: None,
            display,
        })
    } else if model.contains("claude") && !adaptive_only {
        config.thinking_tokens.map(|tokens| AnthropicThinking {
            r#type: "enabled".to_string(),
            budget_tokens: Some(tokens),
            display,
        })
    } else {
        None
    };

    (thinking, output_config)
}

impl From<GaiseTool> for AnthropicTool {
    fn from(t: GaiseTool) -> Self {
        fn map_param(p: &GaiseToolParameter) -> AnthropicProperty {
            let mut prop_type = p.r#type.clone().unwrap_or_else(|| "string".to_string());
            if prop_type == "text" {
                prop_type = "string".to_string();
            }
            AnthropicProperty {
                r#type: prop_type,
                description: p.description.clone().unwrap_or_default(),
                items: p.items.as_ref().map(|i| Box::new(map_param(i))),
                properties: p.properties.as_ref().map(|properties| {
                    properties
                        .iter()
                        .map(|(name, property)| (name.clone(), map_param(property)))
                        .collect()
                }),
                required: p.required.clone(),
            }
        }

        AnthropicTool {
            name: t.name,
            description: t.description,
            input_schema: AnthropicInputSchema {
                r#type: "object".to_string(),
                properties: t
                    .parameters
                    .as_ref()
                    .and_then(|p| p.properties.as_ref())
                    .map(|props| {
                        props
                            .iter()
                            .map(|(k, v)| (k.clone(), map_param(v)))
                            .collect()
                    })
                    .unwrap_or_default(),
                required: t
                    .parameters
                    .as_ref()
                    .and_then(|p| p.required.clone())
                    .unwrap_or_default(),
            },
            cache_control: None,
        }
    }
}

/// Convert the Gaise tool list to Anthropic tools, attaching a single cache
/// breakpoint to the *last* tool. Tool definitions are stable across turns and
/// across mode/system-prompt changes, so caching them at their own breakpoint
/// keeps the whole tool block warm even when the system prompt is rebuilt — the
/// system-prompt marker then only has to (re)cache the system block itself.
fn map_tools(tools: Option<&Vec<GaiseTool>>) -> Option<Vec<AnthropicTool>> {
    tools.map(|ts| {
        let mut mapped: Vec<AnthropicTool> =
            ts.iter().map(|t| AnthropicTool::from(t.clone())).collect();
        if let Some(last) = mapped.last_mut() {
            last.cache_control = Some(AnthropicCacheControl::ephemeral());
        }
        mapped
    })
}

impl From<&GaiseInstructRequest> for AnthropicRequest {
    fn from(request: &GaiseInstructRequest) -> Self {
        let messages = match &request.input {
            OneOrMany::One(m) => vec![m.clone()],
            OneOrMany::Many(ms) => ms.clone(),
        };

        let mut system_prompts = Vec::new();
        let mut anthropic_messages = Vec::new();

        for m in messages {
            // Extract system message
            if m.role == "system" {
                if let Some(content) = &m.content {
                    match content {
                        OneOrMany::One(content) => collect_text(content, &mut system_prompts),
                        OneOrMany::Many(contents) => {
                            for content in contents {
                                collect_text(content, &mut system_prompts);
                            }
                        }
                    }
                }
                continue;
            }

            let content = match &m.content {
                Some(c) => {
                    let items = match c {
                        OneOrMany::One(item) => vec![item.clone()],
                        OneOrMany::Many(items) => items.clone(),
                    };

                    let mut blocks = Vec::new();
                    for item in items {
                        append_anthropic_content(&mut blocks, item);
                    }

                    if blocks.len() == 1
                        && matches!(blocks.first(), Some(AnthropicContentBlock::Text { .. }))
                    {
                        if let Some(AnthropicContentBlock::Text { text, .. }) = blocks.first() {
                            AnthropicContent::Text(text.clone())
                        } else {
                            AnthropicContent::Blocks(blocks)
                        }
                    } else {
                        AnthropicContent::Blocks(blocks)
                    }
                }
                None => AnthropicContent::Text(String::new()),
            };

            // Handle tool calls - convert to tool_use blocks
            let final_content = if let Some(tool_calls) = &m.tool_calls {
                let mut blocks = match content {
                    AnthropicContent::Text(t) => vec![AnthropicContentBlock::Text {
                        text: t,
                        cache_control: None,
                    }],
                    AnthropicContent::Blocks(b) => b,
                };

                for tc in tool_calls {
                    let input: serde_json::Value = if let Some(args) = &tc.function.arguments {
                        serde_json::from_str(args)
                            .unwrap_or(serde_json::Value::Object(serde_json::Map::new()))
                    } else {
                        serde_json::Value::Object(serde_json::Map::new())
                    };

                    blocks.push(AnthropicContentBlock::ToolUse {
                        id: tc.id.clone(),
                        name: tc.function.name.clone(),
                        input,
                        cache_control: None,
                    });
                }
                AnthropicContent::Blocks(blocks)
            } else if let Some(tool_call_id) = &m.tool_call_id {
                // This is a tool result message
                let result_content = match content {
                    AnthropicContent::Text(text) => AnthropicToolResultContent::Text(text),
                    AnthropicContent::Blocks(blocks) => AnthropicToolResultContent::Blocks(blocks),
                };
                AnthropicContent::Blocks(vec![AnthropicContentBlock::ToolResult {
                    tool_use_id: tool_call_id.clone(),
                    content: result_content,
                    cache_control: None,
                }])
            } else {
                content
            };

            anthropic_messages.push(AnthropicMessage {
                role: if m.tool_call_id.is_some() || m.role == "tool" {
                    "user".to_string()
                } else {
                    m.role.clone()
                },
                content: final_content,
            });
        }

        let (thinking, output_config) =
            anthropic_reasoning_config(&request.model, request.generation_config.as_ref());
        let thinking_enabled = thinking.is_some();

        // --- Prompt caching breakpoints ---
        // 1) Tool definitions carry their own breakpoint (see `map_tools`), so they
        //    stay cached independently of the system prompt.
        // 2) System prompt: a single text block carrying an ephemeral cache marker.
        //    This caches the system block on top of the already-cached tool prefix.
        let system = (!system_prompts.is_empty()).then(|| {
            vec![AnthropicSystemBlock {
                r#type: "text".to_string(),
                text: system_prompts.join("\n\n"),
                cache_control: Some(AnthropicCacheControl::ephemeral()),
            }]
        });

        // 2) Conversation: roll the whole history into the cache each turn by marking
        //    the last block of the last message. A bare-text content is promoted to a
        //    single text block so the marker has a block to attach to.
        if let Some(last_msg) = anthropic_messages.last_mut() {
            match &mut last_msg.content {
                AnthropicContent::Text(t) => {
                    last_msg.content =
                        AnthropicContent::Blocks(vec![AnthropicContentBlock::Text {
                            text: std::mem::take(t),
                            cache_control: Some(AnthropicCacheControl::ephemeral()),
                        }]);
                }
                AnthropicContent::Blocks(blocks) => {
                    if let Some(last) = blocks.last_mut() {
                        last.set_cache_control(Some(AnthropicCacheControl::ephemeral()));
                    }
                }
            }
        }

        let model = request.model.to_ascii_lowercase();
        let fixed_sampling = model.contains("claude-opus-4-7")
            || model.contains("claude-opus-4-8")
            || model.contains("claude-sonnet-5")
            || model.contains("claude-fable-5")
            || model.contains("claude-mythos-5")
            || model.contains("claude-mythos-preview");
        let exclusive_sampling = model.contains("claude-opus-4-5")
            || model.contains("claude-sonnet-4-5")
            || model.contains("claude-haiku-4-5");
        let requested_temperature = request
            .generation_config
            .as_ref()
            .and_then(|config| config.temperature);
        let requested_top_p = request
            .generation_config
            .as_ref()
            .and_then(|config| config.top_p);
        let top_p = if fixed_sampling {
            None
        } else if thinking_enabled {
            requested_top_p.filter(|value| (0.95..=1.0).contains(value))
        } else if exclusive_sampling && requested_temperature.is_some() {
            None
        } else {
            requested_top_p
        };

        AnthropicRequest {
            model: request.model.clone(),
            messages: anthropic_messages,
            max_tokens: request
                .generation_config
                .as_ref()
                .and_then(|c| c.max_tokens)
                .unwrap_or(4096),
            system,
            temperature: (!fixed_sampling && !thinking_enabled)
                .then_some(requested_temperature)
                .flatten(),
            top_p,
            top_k: (!fixed_sampling && !thinking_enabled)
                .then(|| {
                    request
                        .generation_config
                        .as_ref()
                        .and_then(|config| config.top_k)
                })
                .flatten(),
            thinking,
            output_config,
            tools: map_tools(request.tools.as_ref()),
            stream: Some(false),
        }
    }
}

impl GaiseClientAnthropic {
    pub fn new(api_url: String, api_key: String) -> Self {
        Self {
            api_url,
            api_key,
            api_version: "2023-06-01".to_string(),
            client: reqwest::Client::new(),
        }
    }

    pub fn with_version(mut self, version: String) -> Self {
        self.api_version = version;
        self
    }

    fn map_from_anthropic_content(
        &self,
        content: Vec<AnthropicContentBlock>,
    ) -> (Option<OneOrMany<GaiseContent>>, Option<Vec<GaiseToolCall>>) {
        let mut text_parts = Vec::new();
        let mut tool_calls = Vec::new();

        for block in content {
            match block {
                AnthropicContentBlock::Text { text, .. } => {
                    text_parts.push(GaiseContent::Text { text });
                }
                AnthropicContentBlock::ToolUse {
                    id, name, input, ..
                } => {
                    tool_calls.push(GaiseToolCall {
                        id,
                        r#type: "function".to_string(),
                        function: GaiseFunctionCall {
                            name,
                            arguments: Some(input.to_string()),
                        },
                        thought_signature: None,
                    });
                }
                AnthropicContentBlock::Thinking {
                    thinking,
                    signature,
                } => {
                    text_parts.push(GaiseContent::Reasoning {
                        text: thinking,
                        signature,
                    });
                }
                AnthropicContentBlock::RedactedThinking { data } => {
                    text_parts.push(GaiseContent::RedactedReasoning {
                        data: data.into_bytes(),
                    });
                }
                _ => {}
            }
        }

        let content_result = if !text_parts.is_empty() {
            if text_parts.len() == 1 {
                Some(OneOrMany::One(text_parts.into_iter().next().unwrap()))
            } else {
                Some(OneOrMany::Many(text_parts))
            }
        } else {
            None
        };

        let tool_calls_result = if !tool_calls.is_empty() {
            Some(tool_calls)
        } else {
            None
        };

        (content_result, tool_calls_result)
    }
}

fn map_anthropic_stream_response(
    chunk: AnthropicStreamResponse,
) -> Vec<GaiseInstructStreamResponse> {
    let external_id = chunk.message.as_ref().map(|message| message.id.clone());
    let mut events = Vec::new();

    match chunk.r#type.as_str() {
        "content_block_delta" => {
            if let Some(delta) = chunk.delta {
                if let Some(text) = delta.text {
                    events.push(GaiseInstructStreamResponse {
                        chunk: GaiseStreamChunk::Text(text),
                        external_id: external_id.clone(),
                    });
                }
                if delta.thinking.is_some() || delta.signature.is_some() {
                    events.push(GaiseInstructStreamResponse {
                        chunk: GaiseStreamChunk::Content(GaiseContent::Reasoning {
                            text: delta.thinking.unwrap_or_default(),
                            signature: delta.signature,
                        }),
                        external_id: external_id.clone(),
                    });
                }
                if let Some(arguments) = delta.partial_json {
                    events.push(GaiseInstructStreamResponse {
                        chunk: GaiseStreamChunk::ToolCall {
                            index: chunk.index.unwrap_or(0),
                            id: None,
                            name: None,
                            arguments: Some(arguments),
                            thought_signature: None,
                        },
                        external_id: external_id.clone(),
                    });
                }
            }
        }
        "content_block_start" => match chunk.content_block {
            Some(AnthropicContentBlock::ToolUse { id, name, .. }) => {
                events.push(GaiseInstructStreamResponse {
                    chunk: GaiseStreamChunk::ToolCall {
                        index: chunk.index.unwrap_or(0),
                        id: Some(id),
                        name: Some(name),
                        arguments: None,
                        thought_signature: None,
                    },
                    external_id: external_id.clone(),
                });
            }
            Some(AnthropicContentBlock::Thinking {
                thinking,
                signature,
            }) if !thinking.is_empty() || signature.is_some() => {
                events.push(GaiseInstructStreamResponse {
                    chunk: GaiseStreamChunk::Content(GaiseContent::Reasoning {
                        text: thinking,
                        signature,
                    }),
                    external_id: external_id.clone(),
                });
            }
            Some(AnthropicContentBlock::RedactedThinking { data }) => {
                events.push(GaiseInstructStreamResponse {
                    chunk: GaiseStreamChunk::Content(GaiseContent::RedactedReasoning {
                        data: data.into_bytes(),
                    }),
                    external_id: external_id.clone(),
                });
            }
            _ => {}
        },
        "message_start" => {
            if let Some(message) = &chunk.message {
                let usage = &message.usage;
                events.push(GaiseInstructStreamResponse {
                    chunk: GaiseStreamChunk::Usage(GaiseUsage {
                        input: Some(map_input_usage(usage)),
                        output: None,
                        total: map_request_usage(usage),
                    }),
                    external_id: Some(message.id.clone()),
                });
            }
        }
        "message_delta" => {
            if let Some(usage) = &chunk.usage {
                events.push(GaiseInstructStreamResponse {
                    chunk: GaiseStreamChunk::Usage(GaiseUsage {
                        input: None,
                        output: Some(map_output_usage(usage)),
                        total: map_request_usage(usage),
                    }),
                    external_id,
                });
            }
        }
        _ => {}
    }

    events
}

#[async_trait]
impl GaiseClient for GaiseClientAnthropic {
    async fn instruct_stream(
        &self,
        request: &GaiseInstructRequest,
    ) -> Result<
        Pin<
            Box<
                dyn Stream<
                        Item = Result<
                            GaiseInstructStreamResponse,
                            Box<dyn std::error::Error + Send + Sync>,
                        >,
                    > + Send,
            >,
        >,
        Box<dyn std::error::Error + Send + Sync>,
    > {
        let url = format!("{}/messages", self.api_url);

        let mut anthropic_request = AnthropicRequest::from(request);
        anthropic_request.stream = Some(true);

        let response = self
            .client
            .post(url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", &self.api_version)
            .header("content-type", "application/json")
            .json(&anthropic_request)
            .send()
            .await?;

        if !response.status().is_success() {
            let err_text = response.text().await?;
            return Err(format!("Anthropic API error: {}", err_text).into());
        }

        let stream = response.bytes_stream();

        let mapped_stream = stream
            .scan(Vec::<u8>::new(), |buffer, result| {
                let mut events: Vec<
                    Result<GaiseInstructStreamResponse, Box<dyn std::error::Error + Send + Sync>>,
                > = Vec::new();
                match result {
                    Err(error) => events.push(Err(Box::new(error))),
                    Ok(bytes) => {
                        buffer.extend_from_slice(&bytes);
                        while let Some(position) = buffer.iter().position(|byte| *byte == b'\n') {
                            let line = buffer.drain(..=position).collect::<Vec<_>>();
                            let line = String::from_utf8_lossy(&line);
                            let line = line.trim();
                            let Some(json) = line.strip_prefix("data:").map(str::trim_start) else {
                                continue;
                            };
                            match serde_json::from_str::<AnthropicStreamResponse>(json) {
                                Ok(chunk) => events.extend(
                                    map_anthropic_stream_response(chunk).into_iter().map(Ok),
                                ),
                                Err(error) => events.push(Err(Box::new(error))),
                            }
                        }
                    }
                }
                futures_util::future::ready(Some(futures_util::stream::iter(events)))
            })
            .flatten();

        Ok(Box::pin(mapped_stream))
    }

    async fn instruct(
        &self,
        request: &GaiseInstructRequest,
    ) -> Result<GaiseInstructResponse, Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("{}/messages", self.api_url);

        let anthropic_request = AnthropicRequest::from(request);

        let response = self
            .client
            .post(url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", &self.api_version)
            .header("content-type", "application/json")
            .json(&anthropic_request)
            .send()
            .await?;

        if !response.status().is_success() {
            let err_text = response.text().await?;
            return Err(format!("Anthropic API error: {}", err_text).into());
        }

        let anthropic_response: AnthropicResponse = response.json().await?;

        let (content, tool_calls) = self.map_from_anthropic_content(anthropic_response.content);

        let message = GaiseMessage {
            role: anthropic_response.role,
            content,
            tool_calls,
            tool_call_id: None,
            tool_name: None,
        };

        let u = &anthropic_response.usage;

        Ok(GaiseInstructResponse {
            output: OneOrMany::One(message),
            external_id: Some(anthropic_response.id),
            usage: Some(GaiseUsage {
                input: Some(map_input_usage(u)),
                output: Some(map_output_usage(u)),
                total: map_request_usage(u),
            }),
        })
    }

    async fn embeddings(
        &self,
        _request: &GaiseEmbeddingsRequest,
    ) -> Result<GaiseEmbeddingsResponse, Box<dyn std::error::Error + Send + Sync>> {
        Err("Anthropic does not support embeddings API".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_redacted_reasoning_response_without_http() {
        let client = GaiseClientAnthropic::new(String::new(), String::new());
        let (content, tool_calls) =
            client.map_from_anthropic_content(vec![AnthropicContentBlock::RedactedThinking {
                data: "opaque-redacted-block".to_string(),
            }]);

        assert!(tool_calls.is_none());
        assert!(matches!(
            content,
            Some(OneOrMany::One(GaiseContent::RedactedReasoning { data }))
                if data == b"opaque-redacted-block"
        ));
    }

    #[test]
    fn maps_cache_reasoning_and_server_tool_usage_without_fabricated_modalities() {
        let usage: AnthropicUsage = serde_json::from_value(serde_json::json!({
            "input_tokens": 20,
            "output_tokens": 30,
            "cache_creation_input_tokens": 7,
            "cache_read_input_tokens": 11,
            "cache_creation": {
                "ephemeral_1h_input_tokens": 2,
                "ephemeral_5m_input_tokens": 5
            },
            "output_tokens_details": {"thinking_tokens": 9},
            "server_tool_use": {"web_fetch_requests": 1, "web_search_requests": 2}
        }))
        .unwrap();

        let input = map_input_usage(&usage);
        let output = map_output_usage(&usage);
        let request = map_request_usage(&usage).unwrap();
        assert_eq!(input.get("input_tokens"), Some(&20));
        assert_eq!(input.get("effective_input_tokens"), Some(&38));
        assert_eq!(input.get("cache_read_input_tokens"), Some(&11));
        assert_eq!(input.get("cache_creation_1h_input_tokens"), Some(&2));
        assert_eq!(input.get("cache_creation_5m_input_tokens"), Some(&5));
        assert!(!input.contains_key("image_tokens"));
        assert!(!input.contains_key("audio_tokens"));
        assert_eq!(output.get("output_tokens"), Some(&30));
        assert_eq!(output.get("reasoning_tokens"), Some(&9));
        assert!(!output.contains_key("web_search_requests"));
        assert_eq!(request.get("web_fetch_requests"), Some(&1));
        assert_eq!(request.get("web_search_requests"), Some(&2));
    }
}
