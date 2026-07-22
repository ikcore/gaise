use crate::contracts::*;
use async_trait::async_trait;
use base64::Engine;
use futures_util::{Stream, StreamExt};
use gaise_core::GaiseClient;
use gaise_core::contracts::{
    GaiseContent, GaiseEmbeddingsRequest, GaiseEmbeddingsResponse, GaiseFunctionCall,
    GaiseInstructRequest, GaiseInstructResponse, GaiseInstructStreamResponse, GaiseMessage,
    GaiseStreamChunk, GaiseTool, GaiseToolCall, GaiseToolParameter, GaiseUsage, OneOrMany,
    image_media_type,
};
use std::collections::HashMap;
use std::pin::Pin;

fn map_usage(usage: &OpenAIUsage) -> GaiseUsage {
    let mut input = HashMap::new();
    input.insert("prompt_tokens".to_string(), usage.prompt_tokens);
    if let Some(details) = &usage.prompt_tokens_details {
        if let Some(tokens) = details.audio_tokens {
            input.insert("audio_tokens".to_string(), tokens);
        }
        if let Some(tokens) = details.cached_tokens {
            input.insert("cached_tokens".to_string(), tokens);
        }
        if let Some(tokens) = details.cache_write_tokens {
            input.insert("cache_write_tokens".to_string(), tokens);
        }
    }

    let mut output = HashMap::new();
    output.insert("completion_tokens".to_string(), usage.completion_tokens);
    if let Some(details) = &usage.completion_tokens_details {
        if let Some(tokens) = details.audio_tokens {
            output.insert("audio_tokens".to_string(), tokens);
        }
        if let Some(tokens) = details.reasoning_tokens {
            output.insert("reasoning_tokens".to_string(), tokens);
        }
        if let Some(tokens) = details.accepted_prediction_tokens {
            output.insert("accepted_prediction_tokens".to_string(), tokens);
        }
        if let Some(tokens) = details.rejected_prediction_tokens {
            output.insert("rejected_prediction_tokens".to_string(), tokens);
        }
    }

    GaiseUsage {
        input: Some(input),
        output: Some(output),
        total: Some(HashMap::from([(
            "total_tokens".to_string(),
            usage.total_tokens,
        )])),
    }
}

/// Map one parsed OpenAI streaming chunk to zero or more GAISe events. OpenAI can
/// place deltas for several parallel tool calls in a single SSE event, so returning
/// a vector is required to avoid silently dropping every call after the first.
fn map_stream_chunk(chunk: OpenAIChatStreamResponse) -> Vec<GaiseInstructStreamResponse> {
    // The final usage-only chunk (sent when `stream_options.include_usage` is set)
    // carries an empty `choices` array, so handle it before the per-choice logic.
    if let Some(usage) = &chunk.usage {
        return vec![GaiseInstructStreamResponse {
            chunk: GaiseStreamChunk::Usage(map_usage(usage)),
            external_id: Some(chunk.id.clone()),
        }];
    }
    let Some(choice) = chunk.choices.first() else {
        return Vec::new();
    };
    let mut events = Vec::new();
    if let Some(tool_calls) = &choice.delta.tool_calls {
        events.extend(tool_calls.iter().map(|tc| GaiseInstructStreamResponse {
            chunk: GaiseStreamChunk::ToolCall {
                index: tc.index,
                id: tc.id.clone(),
                name: tc.function.as_ref().and_then(|f| f.name.clone()),
                arguments: tc.function.as_ref().and_then(|f| f.arguments.clone()),
                thought_signature: None,
            },
            external_id: Some(chunk.id.clone()),
        }));
    }
    if let Some(content) = &choice.delta.content {
        events.push(GaiseInstructStreamResponse {
            chunk: GaiseStreamChunk::Text(content.clone()),
            external_id: Some(chunk.id.clone()),
        });
    }
    events
}

pub struct GaiseClientOpenAI {
    api_url: String,
    api_key: String,
    client: reqwest::Client,
    /// Optional processing tier ("flex", "priority", …) read once from the
    /// `OPENAI_API_TIER` env var at construction. Stamped onto every chat request
    /// when set; left off entirely when unset so OpenAI applies its own default.
    service_tier: Option<String>,
}

impl From<GaiseTool> for OpenAITool {
    fn from(t: GaiseTool) -> Self {
        fn map_param(p: &GaiseToolParameter) -> OpenAIParameterProperty {
            let mut prop_type = p.r#type.clone().unwrap_or_else(|| "string".to_string());
            if prop_type == "text" {
                prop_type = "string".to_string();
            }
            OpenAIParameterProperty {
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

        OpenAITool {
            r#type: "function".to_string(),
            function: OpenAIFunction {
                name: t.name,
                description: t.description,
                parameters: OpenAIParameters {
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
            },
        }
    }
}

fn openai_audio_format(format: Option<&str>) -> String {
    match format
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "wav" | "wave" | "audio/wav" | "audio/wave" => "wav".to_string(),
        _ => "mp3".to_string(),
    }
}

fn map_content_parts(content: GaiseContent, image_detail: Option<&str>) -> Vec<OpenAIContentPart> {
    match content {
        GaiseContent::Text { text } => vec![OpenAIContentPart::Text { text }],
        GaiseContent::Image { data, format } => {
            let encoded = base64::prelude::BASE64_STANDARD.encode(data);
            let media_type = image_media_type(format.as_deref());
            vec![OpenAIContentPart::ImageUrl {
                image_url: OpenAIImageUrl {
                    url: format!("data:{media_type};base64,{encoded}"),
                    detail: image_detail.map(str::to_ascii_lowercase),
                },
            }]
        }
        GaiseContent::Audio { data, format } => vec![OpenAIContentPart::InputAudio {
            input_audio: OpenAIInputAudio {
                data: base64::prelude::BASE64_STANDARD.encode(data),
                format: openai_audio_format(format.as_deref()),
            },
        }],
        GaiseContent::File { data, name } => {
            let name = name.unwrap_or_else(|| "document".to_string());
            let text = match String::from_utf8(data) {
                Ok(text) => {
                    format!("<attached_document name=\"{name}\">\n{text}\n</attached_document>")
                }
                Err(_) => format!(
                    "[Unsupported binary document for OpenAI Chat Completions; use the Responses API input_file feature: {name}]"
                ),
            };
            vec![OpenAIContentPart::Text { text }]
        }
        GaiseContent::Reasoning { text, .. } => vec![OpenAIContentPart::Text {
            text: format!("<reasoning_summary>\n{text}\n</reasoning_summary>"),
        }],
        GaiseContent::RedactedReasoning { .. } => vec![OpenAIContentPart::Text {
            text: "[Encrypted reasoning retained only on its source provider]".to_string(),
        }],
        GaiseContent::Parts { parts } => parts
            .into_iter()
            .flat_map(|part| map_content_parts(part, image_detail))
            .collect(),
    }
}

impl From<&GaiseInstructRequest> for OpenAIChatRequest {
    fn from(request: &GaiseInstructRequest) -> Self {
        let messages = match &request.input {
            OneOrMany::One(m) => vec![m.clone()],
            OneOrMany::Many(ms) => ms.clone(),
        };
        let image_detail = request
            .generation_config
            .as_ref()
            .and_then(|config| config.input_image_detail.as_deref());

        let openai_messages = messages
            .into_iter()
            .map(|m| {
                let content = m.content.map(|c| {
                    let items = match c {
                        OneOrMany::One(item) => vec![item],
                        OneOrMany::Many(items) => items,
                    };

                    let parts: Vec<OpenAIContentPart> = items
                        .into_iter()
                        .flat_map(|item| map_content_parts(item, image_detail))
                        .collect();

                    if parts.len() == 1
                        && let Some(OpenAIContentPart::Text { text }) = parts.first()
                    {
                        return OpenAIContent::Text(text.clone());
                    }
                    OpenAIContent::Parts(parts)
                });

                let tool_calls = m.tool_calls.map(|tcs| {
                    tcs.into_iter()
                        .map(|tc| OpenAIToolCall {
                            id: tc.id,
                            r#type: tc.r#type,
                            function: OpenAIFunctionCall {
                                name: tc.function.name,
                                arguments: tc.function.arguments.unwrap_or_default(),
                            },
                        })
                        .collect()
                });

                OpenAIMessage {
                    role: m.role,
                    content,
                    tool_calls,
                    tool_call_id: m.tool_call_id,
                }
            })
            .collect();

        OpenAIChatRequest {
            model: request.model.clone(),
            messages: openai_messages,
            stream: false,
            stream_options: None,
            temperature: request
                .generation_config
                .as_ref()
                .and_then(|c| c.temperature),
            top_p: request.generation_config.as_ref().and_then(|c| c.top_p),
            max_completion_tokens: request
                .generation_config
                .as_ref()
                .and_then(|c| c.max_tokens),
            reasoning_effort: request
                .generation_config
                .as_ref()
                .and_then(|c| c.thinking_effort.clone()),
            prompt_cache_key: request
                .generation_config
                .as_ref()
                .and_then(|c| c.cache_key.clone()),
            // Defaulted here; the client stamps the resolved tier in `instruct`/`instruct_stream`,
            // which is the only place that has access to the env-sourced value.
            service_tier: None,
            tools: request
                .tools
                .as_ref()
                .map(|ts| ts.iter().map(|t| OpenAITool::from(t.clone())).collect()),
        }
    }
}

impl GaiseClientOpenAI {
    pub fn new(api_url: String, api_key: String) -> Self {
        // Resolve the processing tier once. An empty/whitespace value is treated as
        // unset so a blank `OPENAI_API_TIER=` doesn't send `service_tier: ""`.
        let service_tier = std::env::var("OPENAI_API_TIER")
            .ok()
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty());
        Self {
            api_url,
            api_key,
            client: reqwest::Client::new(),
            service_tier,
        }
    }

    fn map_from_openai_message(&self, msg: OpenAIMessage) -> GaiseMessage {
        let content = msg.content.map(|c| match c {
            OpenAIContent::Text(text) => OneOrMany::One(GaiseContent::Text { text }),
            OpenAIContent::Parts(parts) => OneOrMany::Many(
                parts
                    .into_iter()
                    .filter_map(|p| match p {
                        OpenAIContentPart::Text { text } => Some(GaiseContent::Text { text }),
                        OpenAIContentPart::ImageUrl { .. } => {
                            // This is lossy as we don't easily get back raw bytes from URL here if it's external,
                            // but if it's data URI we could. For now, just placeholder or skip.
                            None
                        }
                        OpenAIContentPart::InputAudio { .. } => None,
                    })
                    .collect(),
            ),
        });

        let tool_calls = msg.tool_calls.map(|tcs| {
            tcs.into_iter()
                .map(|tc| GaiseToolCall {
                    id: tc.id,
                    r#type: tc.r#type,
                    function: GaiseFunctionCall {
                        name: tc.function.name,
                        arguments: Some(tc.function.arguments),
                    },
                    thought_signature: None,
                })
                .collect()
        });

        GaiseMessage {
            role: msg.role,
            content,
            tool_calls,
            tool_call_id: msg.tool_call_id,
            tool_name: None,
        }
    }
}

/// Total attempts (1 initial + retries) for a transient failure.
const MAX_ATTEMPTS: u32 = 4;

/// Transient HTTP statuses worth retrying: 429 (rate limit) and any 5xx
/// (OpenAI's `server_error`, plus gateway/timeout codes). Other 4xx are caller
/// errors and must not be retried.
fn is_transient_status(status: reqwest::StatusCode) -> bool {
    status.as_u16() == 429 || status.is_server_error()
}

impl GaiseClientOpenAI {
    /// Send `builder`, retrying transient failures — 429/5xx responses and
    /// network errors — with exponential backoff. OpenAI's 500 `server_error` is
    /// explicitly retryable and is common under the `flex` service tier; without
    /// this a single random 500 aborts the whole chat turn or enrich item. The
    /// last attempt's outcome is returned verbatim (success or not), so a genuine
    /// persistent failure still surfaces through the caller's existing handling.
    async fn send_with_retry(
        &self,
        builder: reqwest::RequestBuilder,
    ) -> Result<reqwest::Response, Box<dyn std::error::Error + Send + Sync>> {
        let mut attempt = 0u32;
        loop {
            attempt += 1;
            let outcome = match builder.try_clone() {
                Some(rb) => rb.send().await,
                // JSON bodies always clone; if one somehow can't, send the original
                // (no retry) rather than failing to send at all.
                None => return builder.send().await.map_err(Into::into),
            };
            let retryable = match &outcome {
                Ok(resp) => is_transient_status(resp.status()),
                Err(_) => true, // network/timeout — worth another attempt
            };
            if !retryable || attempt >= MAX_ATTEMPTS {
                return outcome.map_err(Into::into);
            }
            match outcome {
                // Drain the body to free the connection before retrying.
                Ok(resp) => {
                    let status = resp.status();
                    let _ = resp.bytes().await;
                    eprintln!(
                        "⚠️  OpenAI {status} — retrying ({attempt}/{})",
                        MAX_ATTEMPTS - 1
                    );
                }
                Err(_) => {
                    eprintln!(
                        "⚠️  OpenAI request error — retrying ({attempt}/{})",
                        MAX_ATTEMPTS - 1
                    );
                }
            }
            // Exponential backoff: 400ms, 800ms, 1600ms.
            let delay = std::time::Duration::from_millis(400u64 * (1u64 << (attempt - 1)));
            tokio::time::sleep(delay).await;
        }
    }
}

#[async_trait]
impl GaiseClient for GaiseClientOpenAI {
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
        let url = format!("{}/chat/completions", self.api_url);

        let mut openai_request = OpenAIChatRequest::from(request);
        openai_request.stream = true;
        openai_request.stream_options = Some(OpenAIStreamOptions {
            include_usage: true,
        });
        openai_request.service_tier = self.service_tier.clone();

        let builder = self
            .client
            .post(url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&openai_request);
        let response = self.send_with_retry(builder).await?;

        if !response.status().is_success() {
            let err_text = response.text().await?;
            return Err(format!("OpenAI API error: {}", err_text).into());
        }

        let stream = response.bytes_stream();

        // `bytes_stream()` yields arbitrary network chunks, NOT line-aligned SSE events:
        // a single chunk may hold several `data:` lines, or split one line (and its JSON)
        // across reads. So buffer the raw bytes and only parse complete, `\n`-terminated
        // lines. This reassembles a large tool-call argument payload that arrives across
        // several reads, instead of parsing it truncated ("EOF while parsing a string") or
        // parsing two concatenated events as one ("trailing characters").
        let mapped_stream = stream
            .scan(Vec::<u8>::new(), |buf, res| {
                let events: Vec<
                    Result<GaiseInstructStreamResponse, Box<dyn std::error::Error + Send + Sync>>,
                > = match res {
                    Err(e) => vec![Err(Box::new(e) as Box<dyn std::error::Error + Send + Sync>)],
                    Ok(bytes) => {
                        buf.extend_from_slice(&bytes);
                        let mut out = Vec::new();
                        // Drain every complete line now in the buffer; leave any partial
                        // trailing line for the next chunk to complete.
                        while let Some(pos) = buf.iter().position(|&b| b == b'\n') {
                            let line_bytes: Vec<u8> = buf.drain(..=pos).collect();
                            let line = String::from_utf8_lossy(&line_bytes);
                            let line = line.trim();
                            let json_str = match line.strip_prefix("data:").map(str::trim_start) {
                                Some(j) => j,
                                None => continue, // blank line, SSE comment, or other field
                            };
                            if json_str == "[DONE]" {
                                continue; // end-of-stream marker — stream ends naturally
                            }
                            match serde_json::from_str::<OpenAIChatStreamResponse>(json_str) {
                                Ok(chunk) => {
                                    out.extend(map_stream_chunk(chunk).into_iter().map(Ok));
                                }
                                // A *complete* line that still won't parse is provider
                                // metadata we don't model; skip it rather than abort the
                                // whole stream.
                                Err(_) => continue,
                            }
                        }
                        out
                    }
                };
                futures_util::future::ready(Some(futures_util::stream::iter(events)))
            })
            .flatten();

        Ok(Box::pin(mapped_stream))
    }

    async fn instruct(
        &self,
        request: &GaiseInstructRequest,
    ) -> Result<GaiseInstructResponse, Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("{}/chat/completions", self.api_url);

        let mut openai_request = OpenAIChatRequest::from(request);
        openai_request.service_tier = self.service_tier.clone();

        let builder = self
            .client
            .post(url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&openai_request);
        let response = self.send_with_retry(builder).await?;

        if !response.status().is_success() {
            let err_text = response.text().await?;
            return Err(format!("OpenAI API error: {}", err_text).into());
        }

        let openai_response: OpenAIChatResponse = response.json().await?;

        let usage = openai_response.usage.as_ref().map(map_usage);

        Ok(GaiseInstructResponse {
            output: OneOrMany::Many(
                openai_response
                    .choices
                    .into_iter()
                    .map(|c| self.map_from_openai_message(c.message))
                    .collect(),
            ),
            external_id: Some(openai_response.id),
            usage,
        })
    }

    async fn embeddings(
        &self,
        request: &GaiseEmbeddingsRequest,
    ) -> Result<GaiseEmbeddingsResponse, Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("{}/embeddings", self.api_url);

        let input = match &request.input {
            OneOrMany::One(s) => OpenAIEmbedInput::String(s.clone()),
            OneOrMany::Many(ss) => OpenAIEmbedInput::Array(ss.clone()),
        };

        let openai_request = OpenAIEmbedRequest {
            model: request.model.clone(),
            input,
        };

        let builder = self
            .client
            .post(url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&openai_request);
        let response = self.send_with_retry(builder).await?;

        if !response.status().is_success() {
            let err_text = response.text().await?;
            return Err(format!("OpenAI API error: {}", err_text).into());
        }

        // Parse from the raw body so a schema/shape mismatch reports the offending
        // field and a body snippet, instead of reqwest's opaque "error decoding
        // response body". (The body is embedding vectors, not secrets.)
        let body = response.text().await?;
        let openai_response: OpenAIEmbedResponse = serde_json::from_str(&body).map_err(|e| {
            let snippet: String = body.chars().take(400).collect();
            format!("failed to parse OpenAI embeddings response: {e}; body starts: {snippet}")
        })?;

        let mut input_usage = HashMap::new();
        input_usage.insert(
            "prompt_tokens".to_string(),
            openai_response.usage.prompt_tokens,
        );

        Ok(GaiseEmbeddingsResponse {
            external_id: Some(openai_response.object),
            output: openai_response
                .data
                .into_iter()
                .map(|d| d.embedding)
                .collect(),
            usage: Some(GaiseUsage {
                input: Some(input_usage),
                output: None,
                total: Some(HashMap::from([(
                    "total_tokens".to_string(),
                    openai_response.usage.total_tokens,
                )])),
            }),
        })
    }
}

#[cfg(test)]
mod retry_tests {
    use super::{is_transient_status, map_stream_chunk, map_usage};
    use crate::contracts::{OpenAIChatStreamResponse, OpenAIUsage};
    use gaise_core::contracts::GaiseStreamChunk;
    use reqwest::StatusCode;

    #[test]
    fn retries_429_and_5xx_only() {
        // OpenAI's intermittent 500 (and gateway/timeout 5xx) and 429 are retried.
        assert!(is_transient_status(StatusCode::INTERNAL_SERVER_ERROR)); // 500
        assert!(is_transient_status(StatusCode::BAD_GATEWAY)); // 502
        assert!(is_transient_status(StatusCode::SERVICE_UNAVAILABLE)); // 503
        assert!(is_transient_status(StatusCode::GATEWAY_TIMEOUT)); // 504
        assert!(is_transient_status(StatusCode::TOO_MANY_REQUESTS)); // 429
        // Caller errors and success are never retried.
        assert!(!is_transient_status(StatusCode::BAD_REQUEST)); // 400
        assert!(!is_transient_status(StatusCode::UNAUTHORIZED)); // 401
        assert!(!is_transient_status(StatusCode::OK)); // 200
    }

    #[test]
    fn maps_every_parallel_tool_delta_without_http() {
        let chunk: OpenAIChatStreamResponse = serde_json::from_value(serde_json::json!({
            "id": "response-1",
            "object": "chat.completion.chunk",
            "created": 1,
            "model": "gpt-5.6",
            "choices": [{
                "index": 0,
                "delta": {
                    "role": null,
                    "content": null,
                    "tool_calls": [
                        {
                            "index": 0,
                            "id": "call-1",
                            "type": "function",
                            "function": {"name": "first", "arguments": "{}"}
                        },
                        {
                            "index": 1,
                            "id": "call-2",
                            "type": "function",
                            "function": {"name": "second", "arguments": "{}"}
                        }
                    ]
                },
                "finish_reason": null
            }]
        }))
        .unwrap();

        let events = map_stream_chunk(chunk);
        assert_eq!(events.len(), 2);
        assert!(matches!(
            &events[0].chunk,
            GaiseStreamChunk::ToolCall { index: 0, name, .. }
                if name.as_deref() == Some("first")
        ));
        assert!(matches!(
            &events[1].chunk,
            GaiseStreamChunk::ToolCall { index: 1, name, .. }
                if name.as_deref() == Some("second")
        ));
    }

    #[test]
    fn maps_chat_usage_to_the_correct_side_without_inventing_modalities() {
        let usage: OpenAIUsage = serde_json::from_value(serde_json::json!({
            "prompt_tokens": 50,
            "completion_tokens": 100,
            "total_tokens": 150,
            "prompt_tokens_details": {
                "audio_tokens": 7,
                "cached_tokens": 10,
                "cache_write_tokens": 4
            },
            "completion_tokens_details": {
                "audio_tokens": 30,
                "reasoning_tokens": 20,
                "accepted_prediction_tokens": 3,
                "rejected_prediction_tokens": 2
            }
        }))
        .unwrap();

        let mapped = map_usage(&usage);
        let input = mapped.input.unwrap();
        let output = mapped.output.unwrap();
        assert_eq!(input.get("prompt_tokens"), Some(&50));
        assert_eq!(input.get("audio_tokens"), Some(&7));
        assert_eq!(input.get("cached_tokens"), Some(&10));
        assert_eq!(input.get("cache_write_tokens"), Some(&4));
        assert!(!input.contains_key("text_tokens"));
        assert!(!input.contains_key("image_tokens"));
        assert_eq!(output.get("completion_tokens"), Some(&100));
        assert_eq!(output.get("audio_tokens"), Some(&30));
        assert_eq!(output.get("reasoning_tokens"), Some(&20));
        assert!(!output.contains_key("total_tokens"));
        assert_eq!(mapped.total.unwrap().get("total_tokens"), Some(&150));
    }
}
