use crate::contracts::*;
use async_trait::async_trait;
use base64::Engine;
use futures_util::{Stream, StreamExt};
use gaise_core::contracts::{
    GaiseContent, GaiseEmbeddingsRequest, GaiseEmbeddingsResponse, GaiseFunctionCall,
    GaiseInstructRequest, GaiseInstructResponse, GaiseInstructStreamResponse,
    GaiseListModelsRequest, GaiseListModelsResponse, GaiseMessage, GaiseReasoningEffort,
    GaiseSpeechChunk, GaiseSpeechRequest, GaiseSpeechResponse, GaiseSpeechStreamResponse,
    GaiseStreamChunk, GaiseTool, GaiseToolCall, GaiseToolParameter, GaiseUsage, OneOrMany,
    image_media_type,
};
use gaise_core::{GaiseClient, GaiseSpeechClient};
use std::collections::HashMap;
use std::pin::Pin;

/// Default API root (the global endpoint; `https://us.api.x.ai/v1` serves
/// only grok-4.7 and grok-4.6, and no voice, image, or video APIs).
pub const DEFAULT_API_URL: &str = "https://api.x.ai/v1";

type BoxError = Box<dyn std::error::Error + Send + Sync>;
type InstructStream =
    Pin<Box<dyn Stream<Item = Result<GaiseInstructStreamResponse, BoxError>> + Send>>;
type SpeechStream = Pin<Box<dyn Stream<Item = Result<GaiseSpeechStreamResponse, BoxError>> + Send>>;

/// Chat Completions rules per Grok family, from the xAI model pages and the
/// reasoning guide (audited 2026-10-17; see `wiki/vendor-xai.md`).
///
/// Unknown `grok-*` ids get a pass-through profile so new releases keep
/// working; known families get exact effort sets so invalid values are
/// never sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct XaiChatRules {
    /// Served by Chat Completions (the multi-agent model, Imagine, and voice
    /// models are not).
    pub chat_supported: bool,
    /// The model reasons.
    pub reasoning: bool,
    /// `reasoning_effort` may be sent at all.
    pub effort_control: bool,
    /// Accepted `reasoning_effort` values, lowest to highest. Empty with
    /// `effort_control` = forward as given.
    pub effort_levels: &'static [&'static str],
    /// Effort applied by the API when none is sent.
    pub default_effort: Option<&'static str>,
}

const EFFORT_47: &[&str] = &["low", "medium", "high", "xhigh"];
const EFFORT_45: &[&str] = &["low", "medium", "high"];
const EFFORT_43: &[&str] = &["none", "low", "medium", "high", "xhigh"];

const PASS_THROUGH: XaiChatRules = XaiChatRules {
    chat_supported: true,
    reasoning: true,
    effort_control: true,
    effort_levels: &[],
    default_effort: None,
};

/// Reasoning without a documented effort control: the effort is never sent.
const FIXED_REASONING: XaiChatRules = XaiChatRules {
    chat_supported: true,
    reasoning: true,
    effort_control: false,
    effort_levels: &[],
    default_effort: None,
};

const NON_REASONING: XaiChatRules = XaiChatRules {
    chat_supported: true,
    reasoning: false,
    effort_control: false,
    effort_levels: &[],
    default_effort: None,
};

const NOT_CHAT: XaiChatRules = XaiChatRules {
    chat_supported: false,
    ..PASS_THROUGH
};

fn effort_rules(levels: &'static [&'static str], default: &'static str) -> XaiChatRules {
    XaiChatRules {
        chat_supported: true,
        reasoning: true,
        effort_control: true,
        effort_levels: levels,
        default_effort: Some(default),
    }
}

/// `grok-4.N` minor version, if the id has that shape.
fn grok4_minor(model: &str) -> Option<u32> {
    let rest = model.strip_prefix("grok-4.")?;
    rest.split(|c: char| !c.is_ascii_digit())
        .next()
        .and_then(|v| v.parse().ok())
}

pub fn xai_chat_rules(model: &str) -> XaiChatRules {
    let m = model.to_ascii_lowercase();
    let starts = |p: &str| m.starts_with(p);

    // The multi-agent model "does not work with the OpenAI Chat Completions
    // API"; Imagine and voice models have their own endpoints.
    if m.contains("multi-agent")
        || starts("grok-imagine")
        || starts("grok-2-image")
        || starts("grok-voice")
        || m == "grok-transcribe"
    {
        return NOT_CHAT;
    }
    // Retired slugs (2026-05-15) still resolve and redirect to grok-4.3 at a
    // fixed effort (low for reasoning slugs, none otherwise), or to
    // grok-build-0.1 for grok-code-fast-1; never override that choice.
    if starts("grok-4-1-fast-reasoning") || starts("grok-4-fast-reasoning") || starts("grok-4-0709")
    {
        return FIXED_REASONING;
    }
    if starts("grok-4-1-fast-non-reasoning") || starts("grok-4-fast-non-reasoning") {
        return NON_REASONING;
    }
    if starts("grok-build") || starts("grok-code-fast") {
        // grok-build-latest is an alias of grok-4.5.
        if m == "grok-build-latest" {
            return effort_rules(EFFORT_45, "high");
        }
        return FIXED_REASONING;
    }
    if starts("grok-4.20") {
        if m.contains("non-reasoning") {
            return NON_REASONING;
        }
        // No effort values are documented for the 4.20 reasoning model.
        return FIXED_REASONING;
    }
    if starts("grok-3") || starts("grok-2") || m == "grok-beta" || m == "grok-vision-beta" {
        // Retired generations: grok-3 redirects to grok-4.3 at none; the rest
        // are no longer in the catalog. Send no effort.
        return NON_REASONING;
    }
    match grok4_minor(&m) {
        Some(3) => effort_rules(EFFORT_43, "low"),
        // "xhigh is available on grok-4.6 and later"; grok-4.5 treats it as high.
        Some(5) => effort_rules(EFFORT_45, "high"),
        // grok-4.6, grok-4.7, and later 4.x releases until documented otherwise.
        Some(n) if (6..20).contains(&n) => effort_rules(EFFORT_47, "high"),
        _ => PASS_THROUGH,
    }
}

/// Resolve a provider-neutral effort onto the family's accepted values:
/// `auto` omits the field, `none` on a model that cannot disable reasoning
/// becomes its lowest level, `ultra`/`max` become the highest, and custom
/// strings pass through.
pub fn normalize_xai_effort(rules: &XaiChatRules, effort: &str) -> Option<String> {
    if !rules.reasoning || !rules.effort_control {
        return None;
    }
    match GaiseReasoningEffort::parse(effort) {
        GaiseReasoningEffort::Auto => None,
        GaiseReasoningEffort::Custom(raw) => Some(raw),
        level => {
            let accepted = GaiseReasoningEffort::levels(rules.effort_levels);
            Some(level.clamp_to(&accepted).as_str().to_string())
        }
    }
}

fn ensure_chat_supported(model: &str) -> Result<(), BoxError> {
    if xai_chat_rules(model).chat_supported {
        Ok(())
    } else {
        Err(format!(
            "xAI model '{model}' is not served by Chat Completions (multi-agent models need the Responses API; Imagine and voice models use their own endpoints: speech through GaiseSpeechClient, speech-to-speech through live_connect)"
        )
        .into())
    }
}

fn map_param(p: &GaiseToolParameter) -> XaiParameterProperty {
    let mut prop_type = p.r#type.clone().unwrap_or_else(|| "string".to_string());
    if prop_type == "text" {
        prop_type = "string".to_string();
    }
    XaiParameterProperty {
        r#type: prop_type,
        description: p.description.clone(),
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

impl From<&GaiseTool> for XaiTool {
    fn from(t: &GaiseTool) -> Self {
        XaiTool {
            r#type: "function".to_string(),
            function: XaiFunction {
                name: t.name.clone(),
                description: t.description.clone(),
                parameters: XaiParameters {
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

/// Image detail: xAI accepts `auto`, `low`, and `high`; `original` becomes `high`.
fn image_detail(request: &GaiseInstructRequest) -> Option<String> {
    let detail = request
        .generation_config
        .as_ref()?
        .input_image_detail
        .as_deref()?
        .to_ascii_lowercase();
    Some(if detail == "original" {
        "high".to_string()
    } else {
        detail
    })
}

fn map_content_parts(
    content: GaiseContent,
    detail: Option<&str>,
    out: &mut Vec<XaiContentPart>,
) -> Result<(), String> {
    match content {
        GaiseContent::Text { text } => out.push(XaiContentPart::Text { text }),
        GaiseContent::Image { data, format } => {
            let encoded = base64::prelude::BASE64_STANDARD.encode(data);
            let media_type = image_media_type(format.as_deref());
            out.push(XaiContentPart::ImageUrl {
                image_url: XaiImageUrl {
                    url: format!("data:{media_type};base64,{encoded}"),
                    detail: detail.map(str::to_string),
                },
            });
        }
        GaiseContent::Audio { .. } => {
            return Err(
                "xAI Chat Completions does not accept audio input; use live_connect with a grok-voice model"
                    .to_string(),
            );
        }
        GaiseContent::File { data, name } => {
            let name = name.unwrap_or_else(|| "document".to_string());
            let text = String::from_utf8(data).map_err(|_| {
                format!("xAI Chat Completions cannot take binary document '{name}'; send text")
            })?;
            out.push(XaiContentPart::Text {
                text: format!("<attached_document name=\"{name}\">\n{text}\n</attached_document>"),
            });
        }
        // Chat Completions has no field for replaying reasoning; xAI keeps the
        // reasoning state server-side only on the Responses API. Drop it.
        GaiseContent::Reasoning { .. } | GaiseContent::RedactedReasoning { .. } => {}
        GaiseContent::Parts { parts } => {
            for part in parts {
                map_content_parts(part, detail, out)?;
            }
        }
    }
    Ok(())
}

fn map_message(message: &GaiseMessage, detail: Option<&str>) -> Result<XaiMessage, String> {
    let content = match &message.content {
        None => None,
        Some(c) => {
            let items = match c {
                OneOrMany::One(item) => vec![item.clone()],
                OneOrMany::Many(items) => items.clone(),
            };
            let mut parts = Vec::new();
            for item in items {
                map_content_parts(item, detail, &mut parts)?;
            }
            match parts.as_slice() {
                [] => None,
                [XaiContentPart::Text { text }] => Some(XaiContent::Text(text.clone())),
                _ => Some(XaiContent::Parts(parts)),
            }
        }
    };
    let tool_calls = message.tool_calls.as_ref().map(|calls| {
        calls
            .iter()
            .map(|tc| XaiToolCall {
                id: tc.id.clone(),
                r#type: tc.r#type.clone(),
                function: XaiFunctionCall {
                    name: tc.function.name.clone(),
                    arguments: tc.function.arguments.clone().unwrap_or_default(),
                },
            })
            .collect()
    });
    Ok(XaiMessage {
        role: message.role.clone(),
        content,
        tool_calls,
        tool_call_id: message.tool_call_id.clone(),
        ..Default::default()
    })
}

/// `tool_choice` from the GAISe tool mode: `any`/`required`, `none`, or `auto`.
fn tool_choice(request: &GaiseInstructRequest) -> Option<serde_json::Value> {
    let mode = request.tool_config.as_ref()?.mode.as_deref()?;
    Some(match mode.to_ascii_lowercase().as_str() {
        "any" | "required" => serde_json::json!("required"),
        "none" => serde_json::json!("none"),
        _ => serde_json::json!("auto"),
    })
}

/// Build the Chat Completions body for a GAISe request.
pub fn xai_chat_request(request: &GaiseInstructRequest) -> Result<XaiChatRequest, String> {
    let rules = xai_chat_rules(&request.model);
    let detail = image_detail(request);
    let messages = match &request.input {
        OneOrMany::One(m) => vec![map_message(m, detail.as_deref())?],
        OneOrMany::Many(ms) => ms
            .iter()
            .map(|m| map_message(m, detail.as_deref()))
            .collect::<Result<_, _>>()?,
    };
    let config = request.generation_config.as_ref();
    let tools: Option<Vec<XaiTool>> = request
        .tools
        .as_ref()
        .filter(|t| !t.is_empty())
        .map(|ts| ts.iter().map(XaiTool::from).collect());
    Ok(XaiChatRequest {
        model: request.model.clone(),
        messages,
        tool_choice: tools.as_ref().and_then(|_| tool_choice(request)),
        tools,
        temperature: config.and_then(|c| c.temperature),
        top_p: config.and_then(|c| c.top_p),
        max_completion_tokens: config.and_then(|c| c.max_tokens),
        reasoning_effort: config
            .and_then(|c| c.thinking_effort.as_deref())
            .and_then(|effort| normalize_xai_effort(&rules, effort)),
        prompt_cache_key: config.and_then(|c| c.cache_key.clone()),
        stream: false,
        stream_options: None,
    })
}

pub fn map_xai_usage(usage: &XaiUsage) -> GaiseUsage {
    let mut input = HashMap::from([("prompt_tokens".to_string(), usage.prompt_tokens)]);
    if let Some(d) = &usage.prompt_tokens_details {
        for (key, value) in [
            ("text_tokens", d.text_tokens),
            ("audio_tokens", d.audio_tokens),
            ("image_tokens", d.image_tokens),
            ("cached_tokens", d.cached_tokens),
        ] {
            if let Some(v) = value {
                input.insert(key.to_string(), v);
            }
        }
    }
    let mut output = HashMap::from([("completion_tokens".to_string(), usage.completion_tokens)]);
    if let Some(d) = &usage.completion_tokens_details {
        for (key, value) in [
            ("reasoning_tokens", d.reasoning_tokens),
            ("audio_tokens", d.audio_tokens),
            ("accepted_prediction_tokens", d.accepted_prediction_tokens),
            ("rejected_prediction_tokens", d.rejected_prediction_tokens),
        ] {
            if let Some(v) = value {
                output.insert(key.to_string(), v);
            }
        }
    }
    let mut total = HashMap::from([("total_tokens".to_string(), usage.total_tokens)]);
    if let Some(ticks) = usage.cost_in_usd_ticks {
        total.insert("cost_in_usd_ticks".to_string(), ticks as usize);
    }
    GaiseUsage {
        input: Some(input),
        output: Some(output),
        total: Some(total),
    }
}

/// Map an assistant message, putting returned reasoning before the answer.
pub fn map_xai_message(message: XaiMessage) -> GaiseMessage {
    let mut items = Vec::new();
    if let Some(reasoning) = message.reasoning_content.filter(|r| !r.is_empty()) {
        items.push(GaiseContent::Reasoning {
            text: reasoning,
            signature: None,
        });
    }
    match message.content {
        Some(XaiContent::Text(text)) => items.push(GaiseContent::Text { text }),
        Some(XaiContent::Parts(parts)) => items.extend(parts.into_iter().filter_map(|p| match p {
            XaiContentPart::Text { text } => Some(GaiseContent::Text { text }),
            XaiContentPart::ImageUrl { .. } => None,
        })),
        None => {}
    }
    if let Some(refusal) = message.refusal.filter(|r| !r.is_empty())
        && items
            .iter()
            .all(|i| !matches!(i, GaiseContent::Text { .. }))
    {
        items.push(GaiseContent::Text { text: refusal });
    }
    let content = match items.len() {
        0 => None,
        1 => Some(OneOrMany::One(items.remove(0))),
        _ => Some(OneOrMany::Many(items)),
    };
    let tool_calls = message.tool_calls.map(|calls| {
        calls
            .into_iter()
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
        role: message.role,
        content,
        tool_calls,
        tool_call_id: message.tool_call_id,
        tool_name: None,
    }
}

/// Map one streaming chunk. xAI sends each tool call whole in one chunk, and
/// may attach `usage` to the final content chunk, so content is emitted first.
pub fn map_xai_stream_chunk(chunk: XaiChatStreamResponse) -> Vec<GaiseInstructStreamResponse> {
    let id = chunk.id;
    let event = |chunk| GaiseInstructStreamResponse {
        chunk,
        external_id: Some(id.clone()),
    };
    let mut events = Vec::new();
    if let Some(choice) = chunk.choices.into_iter().next() {
        let delta = choice.delta;
        if let Some(reasoning) = delta.reasoning_content.filter(|r| !r.is_empty()) {
            events.push(event(GaiseStreamChunk::Content(GaiseContent::Reasoning {
                text: reasoning,
                signature: None,
            })));
        }
        if let Some(tool_calls) = delta.tool_calls {
            events.extend(tool_calls.into_iter().map(|tc| {
                event(GaiseStreamChunk::ToolCall {
                    index: tc.index,
                    id: tc.id,
                    name: tc.function.as_ref().and_then(|f| f.name.clone()),
                    arguments: tc.function.and_then(|f| f.arguments),
                    thought_signature: None,
                })
            }));
        }
        if let Some(content) = delta.content.filter(|c| !c.is_empty()) {
            events.push(event(GaiseStreamChunk::Text(content)));
        }
    }
    if let Some(usage) = &chunk.usage {
        events.push(event(GaiseStreamChunk::Usage(map_xai_usage(usage))));
    }
    events
}

/// Total attempts (1 initial + retries) for a transient failure.
const MAX_ATTEMPTS: u32 = 4;

/// 429 (rate limit) and 5xx are retried; other 4xx are caller errors.
fn is_transient_status(status: reqwest::StatusCode) -> bool {
    status.as_u16() == 429 || status.is_server_error()
}

pub struct GaiseClientXai {
    api_url: String,
    api_key: String,
    client: reqwest::Client,
}

impl GaiseClientXai {
    pub fn new(api_url: String, api_key: String) -> Self {
        Self {
            api_url: api_url.trim_end_matches('/').to_string(),
            api_key,
            client: reqwest::Client::new(),
        }
    }

    /// Send `builder`, retrying 429/5xx responses and network errors with
    /// exponential backoff (400 ms, 800 ms, 1600 ms).
    async fn send_with_retry(
        &self,
        builder: reqwest::RequestBuilder,
    ) -> Result<reqwest::Response, BoxError> {
        let mut attempt = 0u32;
        loop {
            attempt += 1;
            let outcome = match builder.try_clone() {
                Some(rb) => rb.send().await,
                None => return builder.send().await.map_err(Into::into),
            };
            let retryable = match &outcome {
                Ok(resp) => is_transient_status(resp.status()),
                Err(_) => true,
            };
            if !retryable || attempt >= MAX_ATTEMPTS {
                return outcome.map_err(Into::into);
            }
            if let Ok(resp) = outcome {
                let _ = resp.bytes().await;
            }
            let delay = std::time::Duration::from_millis(400u64 * (1u64 << (attempt - 1)));
            tokio::time::sleep(delay).await;
        }
    }

    async fn post_json<T: serde::Serialize>(
        &self,
        path: &str,
        body: &T,
    ) -> Result<reqwest::Response, BoxError> {
        let builder = self
            .client
            .post(format!("{}/{path}", self.api_url))
            .bearer_auth(&self.api_key)
            .json(body);
        let response = self.send_with_retry(builder).await?;
        if !response.status().is_success() {
            let status = response.status();
            let err_text = response.text().await?;
            return Err(format!("xAI API error ({status}): {err_text}").into());
        }
        Ok(response)
    }

    /// `GET /v1/models`: ids, aliases, context length, and reasoning efforts.
    pub async fn list_models_raw(&self) -> Result<XaiModelList, BoxError> {
        let builder = self
            .client
            .get(format!("{}/models", self.api_url))
            .bearer_auth(&self.api_key);
        let response = self.send_with_retry(builder).await?;
        if !response.status().is_success() {
            let status = response.status();
            let err_text = response.text().await?;
            return Err(format!("xAI API error ({status}): {err_text}").into());
        }
        let body = response.text().await?;
        serde_json::from_str(&body).map_err(|e| {
            let snippet: String = body.chars().take(400).collect();
            format!("failed to parse xAI models response: {e}; body starts: {snippet}").into()
        })
    }

    /// `GET /v1/tts/voices`: the built-in voice roster (`voice_id`, `name`,
    /// `language`), shared by text-to-speech and speech-to-speech.
    pub async fn list_voices(&self) -> Result<serde_json::Value, BoxError> {
        let builder = self
            .client
            .get(format!("{}/tts/voices", self.api_url))
            .bearer_auth(&self.api_key);
        let response = self.send_with_retry(builder).await?;
        if !response.status().is_success() {
            let status = response.status();
            let err_text = response.text().await?;
            return Err(format!("xAI API error ({status}): {err_text}").into());
        }
        Ok(response.json().await?)
    }
}

#[async_trait]
impl GaiseClient for GaiseClientXai {
    async fn instruct(
        &self,
        request: &GaiseInstructRequest,
    ) -> Result<GaiseInstructResponse, BoxError> {
        ensure_chat_supported(&request.model)?;
        let body = xai_chat_request(request)?;
        let response = self.post_json("chat/completions", &body).await?;
        let text = response.text().await?;
        let parsed: XaiChatResponse = serde_json::from_str(&text).map_err(|e| {
            let snippet: String = text.chars().take(400).collect();
            format!("failed to parse xAI chat response: {e}; body starts: {snippet}")
        })?;
        Ok(GaiseInstructResponse {
            usage: parsed.usage.as_ref().map(map_xai_usage),
            output: OneOrMany::Many(
                parsed
                    .choices
                    .into_iter()
                    .map(|c| map_xai_message(c.message))
                    .collect(),
            ),
            external_id: Some(parsed.id),
        })
    }

    async fn instruct_stream(
        &self,
        request: &GaiseInstructRequest,
    ) -> Result<InstructStream, BoxError> {
        ensure_chat_supported(&request.model)?;
        let mut body = xai_chat_request(request)?;
        body.stream = true;
        body.stream_options = Some(XaiStreamOptions {
            include_usage: true,
        });
        let response = self.post_json("chat/completions", &body).await?;

        // Buffer raw bytes and parse only complete `\n`-terminated SSE lines:
        // network chunks are not aligned to events.
        let mapped = response
            .bytes_stream()
            .scan(Vec::<u8>::new(), |buf, res| {
                let events: Vec<Result<GaiseInstructStreamResponse, BoxError>> = match res {
                    Err(e) => vec![Err(e.into())],
                    Ok(bytes) => {
                        buf.extend_from_slice(&bytes);
                        let mut out = Vec::new();
                        while let Some(pos) = buf.iter().position(|&b| b == b'\n') {
                            let line_bytes: Vec<u8> = buf.drain(..=pos).collect();
                            let line = String::from_utf8_lossy(&line_bytes);
                            let Some(json) = line.trim().strip_prefix("data:").map(str::trim_start)
                            else {
                                continue;
                            };
                            if json == "[DONE]" {
                                continue;
                            }
                            if let Ok(chunk) = serde_json::from_str::<XaiChatStreamResponse>(json) {
                                out.extend(map_xai_stream_chunk(chunk).into_iter().map(Ok));
                            }
                        }
                        out
                    }
                };
                futures_util::future::ready(Some(futures_util::stream::iter(events)))
            })
            .flatten();
        Ok(Box::pin(mapped))
    }

    /// xAI publishes no embedding model (`/v1/embeddings` exists in the
    /// OpenAPI schema, but neither the catalog nor pricing lists a model).
    async fn embeddings(
        &self,
        _request: &GaiseEmbeddingsRequest,
    ) -> Result<GaiseEmbeddingsResponse, BoxError> {
        Err("xAI does not offer a public text-embedding model".into())
    }

    async fn list_models(
        &self,
        request: &GaiseListModelsRequest,
    ) -> Result<GaiseListModelsResponse, BoxError> {
        let list = self.list_models_raw().await?;
        let models = list
            .data
            .iter()
            .map(|m| map_xai_model(m, request.include_raw))
            .collect();
        let mut response = GaiseListModelsResponse::from_models(models);
        response.retain_operation(request.operation);
        Ok(response)
    }
}

/// Decode a `/v1/tts` response: raw audio bytes by default, or the JSON
/// envelope (base64 audio plus timestamps) when `with_timestamps` was set.
pub fn decode_tts_body(
    content_type: Option<&str>,
    body: Vec<u8>,
    format: &XaiTtsFormat,
) -> Result<GaiseSpeechResponse, String> {
    let is_json = content_type.is_some_and(|t| t.to_ascii_lowercase().contains("json"));
    if !is_json {
        return Ok(GaiseSpeechResponse {
            audio: body,
            format: format.mime.to_string(),
            sample_rate: Some(format.sample_rate),
            ..Default::default()
        });
    }
    let envelope: XaiTtsJsonResponse = serde_json::from_slice(&body)
        .map_err(|e| format!("failed to parse xAI TTS response: {e}"))?;
    let audio = base64::prelude::BASE64_STANDARD
        .decode(envelope.audio.as_bytes())
        .map_err(|e| format!("xAI TTS audio is not valid base64: {e}"))?;
    Ok(GaiseSpeechResponse {
        audio,
        format: envelope
            .content_type
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| format.mime.to_string()),
        sample_rate: Some(format.sample_rate),
        alignment: envelope.audio_timestamps.as_ref().map(Into::into),
        ..Default::default()
    })
}

#[async_trait]
impl GaiseSpeechClient for GaiseClientXai {
    /// `POST /v1/tts`. The model name is ignored: the endpoint has none.
    async fn speech(&self, request: &GaiseSpeechRequest) -> Result<GaiseSpeechResponse, BoxError> {
        let (body, format) = build_tts_request(request)?;
        let response = self.post_json("tts", &body).await?;
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let bytes = response.bytes().await?.to_vec();
        Ok(decode_tts_body(content_type.as_deref(), bytes, &format)?)
    }

    /// Streams the `/v1/tts` body as it arrives. With alignment requested
    /// the endpoint returns one JSON envelope, so the clip is emitted whole
    /// followed by its alignment.
    async fn speech_stream(&self, request: &GaiseSpeechRequest) -> Result<SpeechStream, BoxError> {
        if request.include_alignment {
            let response = self.speech(request).await?;
            let mut chunks = vec![Ok(GaiseSpeechStreamResponse {
                chunk: GaiseSpeechChunk::Audio {
                    data: response.audio,
                    format: response.format,
                    sample_rate: response.sample_rate,
                },
                external_id: None,
            })];
            if let Some(alignment) = response.alignment {
                chunks.push(Ok(GaiseSpeechStreamResponse {
                    chunk: GaiseSpeechChunk::Alignment(alignment),
                    external_id: None,
                }));
            }
            return Ok(Box::pin(futures_util::stream::iter(chunks)));
        }
        let (body, format) = build_tts_request(request)?;
        let response = self.post_json("tts", &body).await?;
        let stream = response.bytes_stream().map(move |res| {
            res.map(|bytes| GaiseSpeechStreamResponse {
                chunk: GaiseSpeechChunk::Audio {
                    data: bytes.to_vec(),
                    format: format.mime.to_string(),
                    sample_rate: Some(format.sample_rate),
                },
                external_id: None,
            })
            .map_err(Into::into)
        });
        Ok(Box::pin(stream))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retries_429_and_5xx_only() {
        assert!(is_transient_status(reqwest::StatusCode::TOO_MANY_REQUESTS));
        assert!(is_transient_status(reqwest::StatusCode::BAD_GATEWAY));
        assert!(!is_transient_status(reqwest::StatusCode::BAD_REQUEST));
        assert!(!is_transient_status(reqwest::StatusCode::OK));
    }

    #[test]
    fn grok4_minor_parses_versions() {
        assert_eq!(grok4_minor("grok-4.7"), Some(7));
        assert_eq!(grok4_minor("grok-4.20-0309-reasoning"), Some(20));
        assert_eq!(grok4_minor("grok-4.5-latest"), Some(5));
        assert_eq!(grok4_minor("grok-4-0709"), None);
    }
}
