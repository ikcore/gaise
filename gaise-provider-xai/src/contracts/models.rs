//! xAI Chat Completions wire contract (`POST /v1/chat/completions`).
//!
//! The shape is OpenAI-compatible; the differences that matter to GAISe are
//! `reasoning_content` on assistant messages and stream deltas, per-family
//! `reasoning_effort` values, and the extra usage counters
//! (`cost_in_usd_ticks`, image/text prompt tokens).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct XaiChatRequest {
    pub model: String,
    pub messages: Vec<XaiMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<XaiTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    /// Visible output tokens only (reasoning is not counted); xAI defaults to
    /// 128,000 when unset. `max_tokens` is deprecated upstream.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_completion_tokens: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<String>,
    /// Routed to the `x-grok-conv-id` cache key upstream.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_cache_key: Option<String>,
    pub stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream_options: Option<XaiStreamOptions>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct XaiStreamOptions {
    pub include_usage: bool,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct XaiMessage {
    pub role: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<XaiContent>,
    /// The reasoning trace, returned by reasoning models. Never sent back.
    #[serde(default, skip_serializing)]
    pub reasoning_content: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refusal: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<XaiToolCall>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum XaiContent {
    Text(String),
    Parts(Vec<XaiContentPart>),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum XaiContentPart {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image_url")]
    ImageUrl { image_url: XaiImageUrl },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct XaiImageUrl {
    pub url: String,
    /// `auto`, `low`, or `high`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct XaiTool {
    pub r#type: String,
    pub function: XaiFunction,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct XaiFunction {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub parameters: XaiParameters,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct XaiParameters {
    /// Always `object`: xAI rejects non-object roots with a 400.
    pub r#type: String,
    // BTreeMap keeps the tools block byte-stable across turns for caching.
    pub properties: BTreeMap<String, XaiParameterProperty>,
    pub required: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct XaiParameterProperty {
    pub r#type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Box<XaiParameterProperty>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<BTreeMap<String, XaiParameterProperty>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct XaiToolCall {
    pub id: String,
    #[serde(default = "function_type")]
    pub r#type: String,
    pub function: XaiFunctionCall,
}

fn function_type() -> String {
    "function".to_string()
}

#[derive(Debug, Serialize, Deserialize)]
pub struct XaiFunctionCall {
    pub name: String,
    #[serde(default)]
    pub arguments: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct XaiChatResponse {
    pub id: String,
    #[serde(default)]
    pub model: Option<String>,
    pub choices: Vec<XaiChoice>,
    #[serde(default)]
    pub usage: Option<XaiUsage>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct XaiChoice {
    #[serde(default)]
    pub index: usize,
    pub message: XaiMessage,
    #[serde(default)]
    pub finish_reason: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct XaiUsage {
    #[serde(default)]
    pub prompt_tokens: usize,
    #[serde(default)]
    pub completion_tokens: usize,
    #[serde(default)]
    pub total_tokens: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_tokens_details: Option<XaiPromptTokensDetails>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completion_tokens_details: Option<XaiCompletionTokensDetails>,
    /// Request cost; 1 USD = 10,000,000,000 ticks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_in_usd_ticks: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct XaiPromptTokensDetails {
    #[serde(default)]
    pub text_tokens: Option<usize>,
    #[serde(default)]
    pub audio_tokens: Option<usize>,
    #[serde(default)]
    pub image_tokens: Option<usize>,
    #[serde(default)]
    pub cached_tokens: Option<usize>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct XaiCompletionTokensDetails {
    #[serde(default)]
    pub reasoning_tokens: Option<usize>,
    #[serde(default)]
    pub audio_tokens: Option<usize>,
    #[serde(default)]
    pub accepted_prediction_tokens: Option<usize>,
    #[serde(default)]
    pub rejected_prediction_tokens: Option<usize>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct XaiChatStreamResponse {
    pub id: String,
    #[serde(default)]
    pub choices: Vec<XaiStreamChoice>,
    #[serde(default)]
    pub usage: Option<XaiUsage>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct XaiStreamChoice {
    #[serde(default)]
    pub index: usize,
    #[serde(default)]
    pub delta: XaiStreamDelta,
    #[serde(default)]
    pub finish_reason: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct XaiStreamDelta {
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub reasoning_content: Option<String>,
    #[serde(default)]
    pub tool_calls: Option<Vec<XaiStreamToolCall>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct XaiStreamToolCall {
    #[serde(default)]
    pub index: usize,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub r#type: Option<String>,
    #[serde(default)]
    pub function: Option<XaiStreamFunctionCall>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct XaiStreamFunctionCall {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub arguments: Option<String>,
}
