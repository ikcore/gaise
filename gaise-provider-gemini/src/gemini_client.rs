use async_trait::async_trait;
use gaise_core::GaiseClient;
use gaise_core::contracts::{
    GaiseContent, GaiseEmbeddingsRequest, GaiseEmbeddingsResponse, GaiseInstructRequest,
    GaiseInstructResponse, GaiseInstructStreamResponse, GaiseMessage, GaiseStreamChunk,
    GaiseUsage, OneOrMany, GaiseToolCall, GaiseFunctionCall, GaiseTool, GaiseToolParameter,
};
use crate::contracts::*;
use futures_util::{Stream, StreamExt};
use std::collections::HashMap;
use std::pin::Pin;
use base64::Engine;

pub struct GaiseClientGemini {
    api_url: String,
    api_key: String,
    client: reqwest::Client,
}

/// Gemini doesn't allow hyphens in function names — sanitise on the way out.
fn sanitize_tool_name(name: &str) -> String {
    name.replace('-', "_")
}

/// Reverse the sanitisation when returning tool calls to the caller.
fn unsanitize_tool_name(name: &str) -> String {
    name.replace('_', "-")
}

fn map_gaise_role_to_gemini(role: &str) -> String {
    match role {
        "assistant" => "model".to_string(),
        "tool" => "user".to_string(),
        _ => role.to_string(), // "user" stays "user", "system" handled separately
    }
}

fn map_gemini_role_to_gaise(role: &str) -> String {
    match role {
        "model" => "assistant".to_string(),
        _ => role.to_string(),
    }
}

fn map_tool_parameter(param: &GaiseToolParameter) -> serde_json::Value {
    let mut obj = serde_json::Map::new();
    if let Some(t) = &param.r#type {
        let mapped = if t == "text" { "string" } else { t.as_str() };
        obj.insert("type".into(), serde_json::Value::String(mapped.to_string()));
    }
    if let Some(desc) = &param.description {
        obj.insert("description".into(), serde_json::Value::String(desc.clone()));
    }
    if let Some(props) = &param.properties {
        let mut properties = serde_json::Map::new();
        for (k, v) in props {
            properties.insert(k.clone(), map_tool_parameter(v));
        }
        obj.insert("properties".into(), serde_json::Value::Object(properties));
    }
    if let Some(items) = &param.items {
        obj.insert("items".into(), map_tool_parameter(items));
    }
    if let Some(req) = &param.required {
        obj.insert("required".into(), serde_json::Value::Array(
            req.iter().map(|r| serde_json::Value::String(r.clone())).collect()
        ));
    }
    serde_json::Value::Object(obj)
}

impl From<&Vec<GaiseTool>> for GeminiToolSet {
    fn from(tools: &Vec<GaiseTool>) -> Self {
        GeminiToolSet {
            function_declarations: tools.iter().map(|t| {
                GeminiFunctionDeclaration {
                    name: sanitize_tool_name(&t.name),
                    description: t.description.clone(),
                    parameters: t.parameters.as_ref().map(map_tool_parameter),
                }
            }).collect(),
        }
    }
}

impl From<&GaiseInstructRequest> for GeminiRequest {
    fn from(request: &GaiseInstructRequest) -> Self {
        let messages = match &request.input {
            OneOrMany::One(m) => vec![m.clone()],
            OneOrMany::Many(ms) => ms.clone(),
        };

        // Extract system messages → systemInstruction, rest → contents
        let mut system_parts: Vec<GeminiPart> = Vec::new();
        let mut contents: Vec<GeminiContent> = Vec::new();

        for msg in messages {
            if msg.role == "system" {
                // System messages become top-level systemInstruction
                if let Some(c) = &msg.content {
                    let items = match c {
                        OneOrMany::One(item) => vec![item.clone()],
                        OneOrMany::Many(items) => items.clone(),
                    };
                    for item in items {
                        if let GaiseContent::Text { text } = item {
                            system_parts.push(GeminiPart {
                                text: Some(text),
                                ..Default::default()
                            });
                        }
                    }
                }
                continue;
            }

            let role = map_gaise_role_to_gemini(&msg.role);
            let mut parts: Vec<GeminiPart> = Vec::new();

            // Handle tool results: tool role messages become functionResponse parts
            if msg.role == "tool" {
                if let Some(c) = &msg.content {
                    let items = match c {
                        OneOrMany::One(item) => vec![item.clone()],
                        OneOrMany::Many(items) => items.clone(),
                    };
                    let text_content = items.into_iter().filter_map(|item| {
                        if let GaiseContent::Text { text } = item { Some(text) } else { None }
                    }).collect::<Vec<_>>().join("");

                    let response_value: serde_json::Value = serde_json::from_str(&text_content)
                        .unwrap_or(serde_json::Value::String(text_content));

                    // Use tool_call_id as function name if available, but typically
                    // the caller tracks the name. We use a placeholder if needed.
                    let name = msg.tool_call_id.clone().unwrap_or_default();
                    parts.push(GeminiPart {
                        function_response: Some(GeminiFunctionResponse {
                            name: sanitize_tool_name(&name),
                            response: response_value,
                        }),
                        ..Default::default()
                    });
                }
                contents.push(GeminiContent { role: Some(role), parts });
                continue;
            }

            // Handle regular content
            if let Some(c) = &msg.content {
                let items = match c {
                    OneOrMany::One(item) => vec![item.clone()],
                    OneOrMany::Many(items) => items.clone(),
                };
                for item in items {
                    match item {
                        GaiseContent::Text { text } => {
                            parts.push(GeminiPart {
                                text: Some(text),
                                ..Default::default()
                            });
                        }
                        GaiseContent::Image { data, format } => {
                            let base64_data = base64::prelude::BASE64_STANDARD.encode(data);
                            let mime_type = format.unwrap_or_else(|| "image/jpeg".to_string());
                            parts.push(GeminiPart {
                                inline_data: Some(GeminiInlineData {
                                    mime_type,
                                    data: base64_data,
                                }),
                                ..Default::default()
                            });
                        }
                        GaiseContent::Audio { data, format } => {
                            let base64_data = base64::prelude::BASE64_STANDARD.encode(data);
                            let mime_type = format.unwrap_or_else(|| "audio/mp3".to_string());
                            parts.push(GeminiPart {
                                inline_data: Some(GeminiInlineData {
                                    mime_type,
                                    data: base64_data,
                                }),
                                ..Default::default()
                            });
                        }
                        GaiseContent::Parts { parts: inner } => {
                            for p in inner {
                                if let GaiseContent::Text { text } = p {
                                    parts.push(GeminiPart {
                                        text: Some(text),
                                        ..Default::default()
                                    });
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }

            // Handle assistant tool calls → functionCall parts
            if let Some(tcs) = &msg.tool_calls {
                for tc in tcs {
                    let args: Option<serde_json::Value> = tc.function.arguments.as_ref()
                        .and_then(|a| serde_json::from_str(a).ok());
                    parts.push(GeminiPart {
                        function_call: Some(GeminiFunctionCall {
                            name: sanitize_tool_name(&tc.function.name),
                            args,
                        }),
                        ..Default::default()
                    });
                }
            }

            contents.push(GeminiContent { role: Some(role), parts });
        }

        let system_instruction = if system_parts.is_empty() {
            None
        } else {
            Some(GeminiSystemInstruction { parts: system_parts })
        };

        let generation_config = request.generation_config.as_ref().map(|gc| {
            let thinking_config = gc.thinking_effort.as_ref().map(|effort| {
                GeminiThinkingConfig {
                    thinking_level: Some(effort.to_uppercase()),
                    thinking_budget: gc.thinking_tokens.map(|t| t as i64),
                    include_thoughts: Some(true),
                }
            }).or_else(|| {
                gc.thinking_tokens.map(|tokens| {
                    GeminiThinkingConfig {
                        thinking_budget: Some(tokens as i64),
                        thinking_level: None,
                        include_thoughts: Some(true),
                    }
                })
            });

            GeminiGenerationConfig {
                temperature: gc.temperature,
                top_p: gc.top_p,
                top_k: gc.top_k,
                max_output_tokens: gc.max_tokens,
                candidate_count: None,
                thinking_config,
            }
        });

        let tools = request.tools.as_ref().map(|ts| vec![GeminiToolSet::from(ts)]);

        GeminiRequest {
            contents,
            system_instruction,
            generation_config,
            tools,
            safety_settings: Some(GeminiSafetySetting::defaults()),
        }
    }
}

impl GaiseClientGemini {
    pub fn new(api_url: String, api_key: String) -> Self {
        Self {
            api_url,
            api_key,
            client: reqwest::Client::new(),
        }
    }

    fn map_from_gemini_content(&self, content: &GeminiContent) -> GaiseMessage {
        let role = content.role.as_deref().map(map_gemini_role_to_gaise)
            .unwrap_or_else(|| "assistant".to_string());

        let mut text_parts: Vec<GaiseContent> = Vec::new();
        let mut tool_calls: Vec<GaiseToolCall> = Vec::new();

        for part in &content.parts {
            if let Some(text) = &part.text {
                text_parts.push(GaiseContent::Text { text: text.clone() });
            }
            if let Some(fc) = &part.function_call {
                tool_calls.push(GaiseToolCall {
                    id: format!("call_{}", tool_calls.len()),
                    r#type: "function".to_string(),
                    function: GaiseFunctionCall {
                        name: unsanitize_tool_name(&fc.name),
                        arguments: fc.args.as_ref().map(|a| a.to_string()),
                    },
                });
            }
        }

        let content = if text_parts.is_empty() {
            None
        } else if text_parts.len() == 1 {
            Some(OneOrMany::One(text_parts.remove(0)))
        } else {
            Some(OneOrMany::Many(text_parts))
        };

        let tool_calls = if tool_calls.is_empty() { None } else { Some(tool_calls) };

        GaiseMessage {
            role,
            content,
            tool_calls,
            tool_call_id: None,
        }
    }

    fn map_usage(&self, usage: &GeminiUsageMetadata) -> GaiseUsage {
        let mut input = HashMap::new();
        if let Some(n) = usage.prompt_token_count {
            input.insert("prompt_tokens".to_string(), n);
        }
        let mut output = HashMap::new();
        if let Some(n) = usage.candidates_token_count {
            output.insert("candidates_tokens".to_string(), n);
        }
        GaiseUsage {
            input: if input.is_empty() { None } else { Some(input) },
            output: if output.is_empty() { None } else { Some(output) },
        }
    }
}

#[async_trait]
impl GaiseClient for GaiseClientGemini {
    async fn instruct_stream(
        &self,
        request: &GaiseInstructRequest,
    ) -> Result<
        Pin<Box<dyn Stream<Item = Result<GaiseInstructStreamResponse, Box<dyn std::error::Error + Send + Sync>>> + Send>>,
        Box<dyn std::error::Error + Send + Sync>,
    > {
        let url = format!(
            "{}/models/{}:streamGenerateContent?alt=sse&key={}",
            self.api_url, request.model, self.api_key
        );

        let gemini_request = GeminiRequest::from(request);

        let response = self.client.post(&url)
            .json(&gemini_request)
            .send()
            .await?;

        if !response.status().is_success() {
            let err_text = response.text().await?;
            return Err(format!("Gemini API error: {}", err_text).into());
        }

        let stream = response.bytes_stream();

        let mapped_stream = stream.map(|res| {
            res.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>).and_then(|bytes| {
                let text = std::str::from_utf8(&bytes)?;

                // SSE format: lines starting with "data: "
                for line in text.lines() {
                    let line = line.trim();
                    if !line.starts_with("data: ") {
                        continue;
                    }
                    let json_str = &line[6..];
                    let chunk: GeminiResponse = serde_json::from_str(json_str)?;

                    if let Some(candidate) = chunk.candidates.first()
                        && let Some(content) = &candidate.content
                    {
                        for part in &content.parts {
                            if let Some(fc) = &part.function_call {
                                return Ok(GaiseInstructStreamResponse {
                                    chunk: GaiseStreamChunk::ToolCall {
                                        index: 0,
                                        id: None,
                                        name: Some(unsanitize_tool_name(&fc.name)),
                                        arguments: fc.args.as_ref().map(|a| a.to_string()),
                                    },
                                    external_id: None,
                                });
                            }
                            if let Some(text) = &part.text {
                                return Ok(GaiseInstructStreamResponse {
                                    chunk: GaiseStreamChunk::Text(text.clone()),
                                    external_id: None,
                                });
                            }
                        }
                    }

                    // Check for usage in final chunk
                    if let Some(usage) = &chunk.usage_metadata {
                        let mut input = HashMap::new();
                        if let Some(n) = usage.prompt_token_count {
                            input.insert("prompt_tokens".to_string(), n);
                        }
                        let mut output = HashMap::new();
                        if let Some(n) = usage.candidates_token_count {
                            output.insert("candidates_tokens".to_string(), n);
                        }
                        return Ok(GaiseInstructStreamResponse {
                            chunk: GaiseStreamChunk::Usage(GaiseUsage {
                                input: if input.is_empty() { None } else { Some(input) },
                                output: if output.is_empty() { None } else { Some(output) },
                            }),
                            external_id: None,
                        });
                    }
                }

                Err("Empty chunk".into())
            })
        })
        .filter(|res| {
            match res {
                Err(e) if e.to_string() == "Empty chunk" => futures_util::future::ready(false),
                _ => futures_util::future::ready(true),
            }
        });

        Ok(Box::pin(mapped_stream))
    }

    async fn instruct(&self, request: &GaiseInstructRequest) -> Result<GaiseInstructResponse, Box<dyn std::error::Error + Send + Sync>> {
        let url = format!(
            "{}/models/{}:generateContent?key={}",
            self.api_url, request.model, self.api_key
        );

        let gemini_request = GeminiRequest::from(request);

        let response = self.client.post(&url)
            .json(&gemini_request)
            .send()
            .await?;

        if !response.status().is_success() {
            let err_text = response.text().await?;
            return Err(format!("Gemini API error: {}", err_text).into());
        }

        let gemini_response: GeminiResponse = response.json().await?;

        let usage = gemini_response.usage_metadata.as_ref().map(|u| self.map_usage(u));

        let messages: Vec<GaiseMessage> = gemini_response.candidates.iter()
            .filter_map(|c| c.content.as_ref())
            .map(|content| self.map_from_gemini_content(content))
            .collect();

        let output = if messages.len() == 1 {
            OneOrMany::One(messages.into_iter().next().unwrap())
        } else {
            OneOrMany::Many(messages)
        };

        Ok(GaiseInstructResponse {
            output,
            external_id: None,
            usage,
        })
    }

    async fn embeddings(&self, request: &GaiseEmbeddingsRequest) -> Result<GaiseEmbeddingsResponse, Box<dyn std::error::Error + Send + Sync>> {
        let url = format!(
            "{}/models/{}:batchEmbedContents?key={}",
            self.api_url, request.model, self.api_key
        );

        let inputs = match &request.input {
            OneOrMany::One(s) => vec![s.clone()],
            OneOrMany::Many(ss) => ss.clone(),
        };

        let batch_request = GeminiBatchEmbedRequest {
            requests: inputs.into_iter().map(|text| {
                GeminiEmbedRequest {
                    model: format!("models/{}", request.model),
                    content: GeminiContent {
                        role: None,
                        parts: vec![GeminiPart {
                            text: Some(text),
                            ..Default::default()
                        }],
                    },
                }
            }).collect(),
        };

        let response = self.client.post(&url)
            .json(&batch_request)
            .send()
            .await?;

        if !response.status().is_success() {
            let err_text = response.text().await?;
            return Err(format!("Gemini API error: {}", err_text).into());
        }

        let gemini_response: GeminiBatchEmbedResponse = response.json().await?;

        Ok(GaiseEmbeddingsResponse {
            external_id: None,
            output: gemini_response.embeddings.into_iter().map(|e| e.values).collect(),
            usage: None,
        })
    }
}
