//! Responses API (`POST /responses`) mapping for models whose function tools
//! are not served by Chat Completions (GPT-6 Astra, GPT-6.1 Sol).
//!
//! The mapping is stateless: every call resends the conversation with
//! `store: false`, and reasoning items travel between turns as
//! [`GaiseContent::Reasoning`] whose `signature` carries the encrypted item.

use crate::contracts::{OpenAIParameters, OpenAITool};
use crate::openai_client::{
    chat_tools_require_responses, normalize_chat_effort, openai_chat_rules,
};
use base64::Engine;
use gaise_core::contracts::{
    GaiseContent, GaiseFunctionCall, GaiseInstructRequest, GaiseInstructResponse,
    GaiseInstructStreamResponse, GaiseMessage, GaiseStreamChunk, GaiseToolCall, GaiseUsage,
    OneOrMany, file_media_type, image_media_type,
};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::HashMap;

/// Prefix of a [`GaiseContent::Reasoning`] signature produced by this module.
/// The remainder is the Responses `reasoning` output item, replayed verbatim.
pub const REASONING_SIGNATURE_PREFIX: &str = "openai-responses:";

#[derive(Debug, Serialize)]
pub struct OpenAIResponsesRequest {
    pub model: String,
    pub input: Vec<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<OpenAIResponsesTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_cache_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<String>,
    /// Always `false`: GAISe resends the conversation instead of relying on
    /// server-side state.
    pub store: bool,
    /// `reasoning.encrypted_content`, so reasoning can be replayed statelessly.
    pub include: Vec<String>,
    pub stream: bool,
}

/// Responses function tools are flat (no `function` wrapper).
#[derive(Debug, Serialize)]
pub struct OpenAIResponsesTool {
    pub r#type: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub parameters: OpenAIParameters,
    /// Sent as `false`: strict mode requires closed schemas with every
    /// property required, which GAISe tool definitions do not guarantee.
    pub strict: bool,
}

impl From<OpenAITool> for OpenAIResponsesTool {
    fn from(tool: OpenAITool) -> Self {
        Self {
            r#type: tool.r#type,
            name: tool.function.name,
            description: tool.function.description,
            parameters: tool.function.parameters,
            strict: false,
        }
    }
}

fn messages(request: &GaiseInstructRequest) -> Vec<&GaiseMessage> {
    match &request.input {
        OneOrMany::One(message) => vec![message],
        OneOrMany::Many(messages) => messages.iter().collect(),
    }
}

fn content_items(message: &GaiseMessage) -> Vec<&GaiseContent> {
    fn flatten<'a>(content: &'a GaiseContent, out: &mut Vec<&'a GaiseContent>) {
        match content {
            GaiseContent::Parts { parts } => parts.iter().for_each(|part| flatten(part, out)),
            other => out.push(other),
        }
    }
    let mut out = Vec::new();
    match &message.content {
        Some(OneOrMany::One(item)) => flatten(item, &mut out),
        Some(OneOrMany::Many(items)) => items.iter().for_each(|item| flatten(item, &mut out)),
        None => {}
    }
    out
}

fn replayable_reasoning(content: &GaiseContent) -> Option<Value> {
    let GaiseContent::Reasoning {
        signature: Some(signature),
        ..
    } = content
    else {
        return None;
    };
    let item = signature.strip_prefix(REASONING_SIGNATURE_PREFIX)?;
    serde_json::from_str(item).ok()
}

/// Whether `instruct` / `instruct_stream` send this request to the Responses
/// API. True for a model whose Chat Completions surface cannot call tools
/// when the request offers tools or continues a conversation that already
/// used them (or carries reasoning this module produced).
pub fn uses_responses_api(request: &GaiseInstructRequest) -> bool {
    if !chat_tools_require_responses(&request.model) {
        return false;
    }
    request
        .tools
        .as_ref()
        .is_some_and(|tools| !tools.is_empty())
        || messages(request).into_iter().any(|message| {
            message.role == "tool"
                || message.tool_call_id.is_some()
                || message.tool_calls.as_ref().is_some_and(|c| !c.is_empty())
                || content_items(message)
                    .into_iter()
                    .any(|content| replayable_reasoning(content).is_some())
        })
}

fn plain_text(message: &GaiseMessage) -> String {
    content_items(message)
        .into_iter()
        .filter_map(|content| match content {
            GaiseContent::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn input_parts(message: &GaiseMessage, image_detail: Option<&str>) -> Vec<Value> {
    let encode = |data: &[u8]| base64::prelude::BASE64_STANDARD.encode(data);
    content_items(message)
        .into_iter()
        .filter_map(|content| match content {
            GaiseContent::Text { text } => Some(json!({"type": "input_text", "text": text})),
            GaiseContent::Image { data, format } => {
                let media_type = image_media_type(format.as_deref());
                let mut part = json!({
                    "type": "input_image",
                    "image_url": format!("data:{media_type};base64,{}", encode(data)),
                });
                if let Some(detail) = image_detail {
                    part["detail"] = json!(detail);
                }
                Some(part)
            }
            GaiseContent::File { data, name } => {
                let filename = name.as_deref().unwrap_or("document");
                Some(match std::str::from_utf8(data) {
                    Ok(text) => json!({
                        "type": "input_text",
                        "text": format!(
                            "<attached_document name=\"{filename}\">\n{text}\n</attached_document>"
                        ),
                    }),
                    Err(_) => json!({
                        "type": "input_file",
                        "filename": filename,
                        "file_data": format!(
                            "data:{};base64,{}",
                            file_media_type(name.as_deref()),
                            encode(data)
                        ),
                    }),
                })
            }
            GaiseContent::Audio { .. } => Some(json!({
                "type": "input_text",
                "text": "[Audio input is not supported by the OpenAI Responses mapping]",
            })),
            // Reasoning from another provider or turn is not replayable here.
            GaiseContent::Reasoning { .. }
            | GaiseContent::RedactedReasoning { .. }
            | GaiseContent::Parts { .. } => None,
        })
        .collect()
}

fn input_items(request: &GaiseInstructRequest, image_detail: Option<&str>) -> Vec<Value> {
    let mut items = Vec::new();
    for message in messages(request) {
        if message.role == "tool" || (message.tool_call_id.is_some() && message.role != "assistant")
        {
            items.push(json!({
                "type": "function_call_output",
                "call_id": message.tool_call_id.clone().unwrap_or_default(),
                "output": plain_text(message),
            }));
            continue;
        }
        if message.role == "assistant" {
            // Output order of a reasoning model: reasoning, text, tool calls.
            items.extend(
                content_items(message)
                    .into_iter()
                    .filter_map(replayable_reasoning),
            );
            let text = plain_text(message);
            if !text.is_empty() {
                items.push(json!({"role": "assistant", "content": text}));
            }
            for call in message.tool_calls.iter().flatten() {
                // No item `id`: only `call_id` links a call to its output, and
                // an id would bind the call to a stored reasoning item.
                items.push(json!({
                    "type": "function_call",
                    "call_id": call.id,
                    "name": call.function.name,
                    "arguments": call.function.arguments.clone().unwrap_or_else(|| "{}".into()),
                }));
            }
            continue;
        }
        let parts = input_parts(message, image_detail);
        if !parts.is_empty() {
            items.push(json!({"role": message.role, "content": parts}));
        }
    }
    items
}

/// Build the Responses request. Effort follows the same family rules as Chat
/// Completions; sampling parameters are never sent because every model routed
/// here rejects them.
pub fn responses_request(request: &GaiseInstructRequest) -> OpenAIResponsesRequest {
    let rules = openai_chat_rules(&request.model);
    let config = request.generation_config.as_ref();
    let image_detail = config
        .and_then(|c| c.input_image_detail.as_deref())
        .map(str::to_ascii_lowercase)
        .map(|detail| {
            if detail == "original" && !rules.original_image_detail {
                "high".to_string()
            } else {
                detail
            }
        });
    let effort = config
        .and_then(|c| c.thinking_effort.as_deref())
        .and_then(|effort| normalize_chat_effort(&rules, effort));
    let summary = config.and_then(|c| c.include_thoughts).unwrap_or(false);
    let mut reasoning = serde_json::Map::new();
    if let Some(effort) = effort {
        reasoning.insert("effort".into(), json!(effort));
    }
    if summary {
        reasoning.insert("summary".into(), json!("auto"));
    }
    OpenAIResponsesRequest {
        model: request.model.clone(),
        input: input_items(request, image_detail.as_deref()),
        tools: request
            .tools
            .as_ref()
            .filter(|t| !t.is_empty())
            .map(|tools| {
                tools
                    .iter()
                    .map(|tool| OpenAITool::from(tool.clone()).into())
                    .collect()
            }),
        max_output_tokens: config.and_then(|c| c.max_tokens),
        reasoning: (!reasoning.is_empty()).then_some(Value::Object(reasoning)),
        prompt_cache_key: config.and_then(|c| c.cache_key.clone()),
        service_tier: None,
        store: false,
        include: vec!["reasoning.encrypted_content".into()],
        stream: false,
    }
}

fn count(value: &Value) -> Option<usize> {
    value.as_u64().map(|n| n as usize)
}

/// Map Responses usage onto the same keys the Chat Completions path reports
/// (`prompt_tokens`, `completion_tokens`, ...), so callers see one vocabulary
/// for OpenAI whichever endpoint served the request.
pub fn map_responses_usage(usage: &Value) -> Option<GaiseUsage> {
    if !usage.is_object() {
        return None;
    }
    let mut input = HashMap::new();
    let mut output = HashMap::new();
    if let Some(n) = count(&usage["input_tokens"]) {
        input.insert("prompt_tokens".to_string(), n);
    }
    if let Some(n) = count(&usage["input_tokens_details"]["cached_tokens"]) {
        input.insert("cached_tokens".to_string(), n);
    }
    if let Some(n) = count(&usage["input_tokens_details"]["cache_write_tokens"]) {
        input.insert("cache_write_tokens".to_string(), n);
    }
    if let Some(n) = count(&usage["output_tokens"]) {
        output.insert("completion_tokens".to_string(), n);
    }
    if let Some(n) = count(&usage["output_tokens_details"]["reasoning_tokens"]) {
        output.insert("reasoning_tokens".to_string(), n);
    }
    Some(GaiseUsage {
        input: Some(input),
        output: Some(output),
        total: count(&usage["total_tokens"])
            .map(|n| HashMap::from([("total_tokens".to_string(), n)])),
    })
}

/// A `reasoning` output item as replayable content, or `None` when it carries
/// neither a summary nor encrypted content.
fn reasoning_content(item: &Value) -> Option<GaiseContent> {
    let text = item["summary"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|part| part["text"].as_str())
        .collect::<Vec<_>>()
        .join("\n\n");
    let encrypted = item["encrypted_content"].is_string();
    if text.is_empty() && !encrypted {
        return None;
    }
    Some(GaiseContent::Reasoning {
        text,
        signature: encrypted.then(|| format!("{REASONING_SIGNATURE_PREFIX}{item}")),
    })
}

fn message_text(item: &Value) -> String {
    item["content"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|part| match part["type"].as_str() {
            Some("output_text") => part["text"].as_str(),
            Some("refusal") => part["refusal"].as_str(),
            _ => None,
        })
        .collect()
}

fn failure(response: &Value) -> Option<String> {
    (response["status"] == "failed" || response["error"].is_object()).then(|| {
        let error = &response["error"];
        format!(
            "OpenAI API error: {}",
            error["message"]
                .as_str()
                .map(str::to_string)
                .unwrap_or_else(|| error.to_string())
        )
    })
}

/// Map a complete Responses body to one assistant message.
pub fn map_responses_response(response: &Value) -> Result<GaiseInstructResponse, String> {
    if let Some(error) = failure(response) {
        return Err(error);
    }
    let mut content = Vec::new();
    let mut tool_calls = Vec::new();
    for item in response["output"].as_array().into_iter().flatten() {
        match item["type"].as_str() {
            Some("reasoning") => content.extend(reasoning_content(item)),
            Some("message") => {
                let text = message_text(item);
                if !text.is_empty() {
                    content.push(GaiseContent::Text { text });
                }
            }
            Some("function_call") => tool_calls.push(GaiseToolCall {
                id: item["call_id"].as_str().unwrap_or_default().to_string(),
                r#type: "function".to_string(),
                function: GaiseFunctionCall {
                    name: item["name"].as_str().unwrap_or_default().to_string(),
                    arguments: Some(item["arguments"].as_str().unwrap_or("{}").to_string()),
                },
                thought_signature: None,
            }),
            _ => {}
        }
    }
    let message = GaiseMessage {
        role: "assistant".to_string(),
        content: match content.len() {
            0 => None,
            1 => content.pop().map(OneOrMany::One),
            _ => Some(OneOrMany::Many(content)),
        },
        tool_calls: (!tool_calls.is_empty()).then_some(tool_calls),
        tool_call_id: None,
        tool_name: None,
    };
    Ok(GaiseInstructResponse {
        output: OneOrMany::Many(vec![message]),
        external_id: response["id"].as_str().map(str::to_string),
        usage: map_responses_usage(&response["usage"]),
    })
}

/// Streaming state: the response id and the GAISe tool-call index assigned to
/// each `output_index` that holds a function call.
#[derive(Debug, Default)]
pub struct ResponsesStreamState {
    response_id: Option<String>,
    tool_indexes: HashMap<u64, usize>,
}

/// Map one Responses SSE event (the JSON after `data:`) to GAISe stream
/// events. Unmodelled event types map to nothing.
pub fn map_responses_event(
    state: &mut ResponsesStreamState,
    event: &Value,
) -> Vec<Result<GaiseInstructStreamResponse, String>> {
    if let Some(id) = event["response"]["id"].as_str() {
        state.response_id = Some(id.to_string());
    }
    let emit = |chunk| {
        Ok(GaiseInstructStreamResponse {
            chunk,
            external_id: state.response_id.clone(),
        })
    };
    let output_index = event["output_index"].as_u64().unwrap_or_default();
    match event["type"].as_str().unwrap_or_default() {
        "response.output_text.delta" | "response.refusal.delta" => event["delta"]
            .as_str()
            .map(|delta| vec![emit(GaiseStreamChunk::Text(delta.to_string()))])
            .unwrap_or_default(),
        "response.output_item.added" if event["item"]["type"] == "function_call" => {
            let index = state.tool_indexes.len();
            state.tool_indexes.insert(output_index, index);
            let item = &event["item"];
            vec![emit(GaiseStreamChunk::ToolCall {
                index,
                id: item["call_id"].as_str().map(str::to_string),
                name: item["name"].as_str().map(str::to_string),
                arguments: item["arguments"]
                    .as_str()
                    .filter(|a| !a.is_empty())
                    .map(str::to_string),
                thought_signature: None,
            })]
        }
        "response.function_call_arguments.delta" => {
            match (
                state.tool_indexes.get(&output_index),
                event["delta"].as_str(),
            ) {
                (Some(&index), Some(delta)) => vec![emit(GaiseStreamChunk::ToolCall {
                    index,
                    id: None,
                    name: None,
                    arguments: Some(delta.to_string()),
                    thought_signature: None,
                })],
                _ => Vec::new(),
            }
        }
        "response.output_item.done" if event["item"]["type"] == "reasoning" => {
            reasoning_content(&event["item"])
                .map(|content| vec![emit(GaiseStreamChunk::Content(content))])
                .unwrap_or_default()
        }
        "response.completed" | "response.incomplete" => {
            map_responses_usage(&event["response"]["usage"])
                .map(|usage| vec![emit(GaiseStreamChunk::Usage(usage))])
                .unwrap_or_default()
        }
        "response.failed" => vec![Err(failure(&event["response"])
            .unwrap_or_else(|| "OpenAI API error: response failed".to_string()))],
        "error" => vec![Err(format!(
            "OpenAI API error: {}",
            event["message"]
                .as_str()
                .or_else(|| event["error"]["message"].as_str())
                .unwrap_or("stream error")
        ))],
        _ => Vec::new(),
    }
}
