use async_trait::async_trait;
use aws_sdk_bedrockruntime::Client as BedrockClient;
use futures_util::Stream;
use gaise_core::GaiseClient;
use gaise_core::contracts::{
    GaiseContent, GaiseEmbeddingsRequest, GaiseEmbeddingsResponse, GaiseFunctionCall,
    GaiseInstructRequest, GaiseInstructResponse, GaiseInstructStreamResponse,
    GaiseListModelsRequest, GaiseListModelsResponse, GaiseMessage, GaiseStreamChunk, GaiseToolCall,
    GaiseToolParameter, OneOrMany,
};

use crate::catalog::{
    BedrockInferenceProfile, BedrockModelSummary, map_foundation_model, map_inference_profiles,
};
use std::error::Error;
use std::path::Path;
use std::pin::Pin;

fn map_bedrock_usage(
    usage: &aws_sdk_bedrockruntime::types::TokenUsage,
) -> gaise_core::contracts::GaiseUsage {
    let raw_input = usage.input_tokens.max(0) as usize;
    let mut effective_input = raw_input;
    let mut input = std::collections::HashMap::from([("input_tokens".to_string(), raw_input)]);
    if let Some(value) = usage.cache_read_input_tokens {
        let value = value.max(0) as usize;
        effective_input += value;
        input.insert("cache_read_input_tokens".to_string(), value);
    }
    if let Some(value) = usage.cache_write_input_tokens {
        let value = value.max(0) as usize;
        effective_input += value;
        input.insert("cache_write_input_tokens".to_string(), value);
    }
    for detail in usage.cache_details() {
        input.insert(
            format!("cache_write_{}_input_tokens", detail.ttl.as_str()),
            detail.input_tokens.max(0) as usize,
        );
    }
    if effective_input != raw_input {
        input.insert("effective_input_tokens".to_string(), effective_input);
    }
    let output = std::collections::HashMap::from([(
        "output_tokens".to_string(),
        usage.output_tokens.max(0) as usize,
    )]);
    gaise_core::contracts::GaiseUsage {
        input: Some(input),
        output: Some(output),
        total: Some(std::collections::HashMap::from([(
            "total_tokens".to_string(),
            usage.total_tokens.max(0) as usize,
        )])),
    }
}

fn bedrock_embedding_input_tokens(response: &serde_json::Value) -> Option<usize> {
    response
        .get("inputTextTokenCount")
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
}

fn bedrock_image_format(format: Option<&str>) -> aws_sdk_bedrockruntime::types::ImageFormat {
    match format {
        Some("png") | Some("image/png") => aws_sdk_bedrockruntime::types::ImageFormat::Png,
        Some("jpeg") | Some("jpg") | Some("image/jpeg") | Some("image/jpg") => {
            aws_sdk_bedrockruntime::types::ImageFormat::Jpeg
        }
        Some("webp") | Some("image/webp") => aws_sdk_bedrockruntime::types::ImageFormat::Webp,
        Some("gif") | Some("image/gif") => aws_sdk_bedrockruntime::types::ImageFormat::Gif,
        _ => aws_sdk_bedrockruntime::types::ImageFormat::Jpeg,
    }
}

fn bedrock_audio_format(format: Option<&str>) -> aws_sdk_bedrockruntime::types::AudioFormat {
    match format
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "wav" | "wave" | "audio/wav" | "audio/wave" => {
            aws_sdk_bedrockruntime::types::AudioFormat::Wav
        }
        "flac" | "audio/flac" => aws_sdk_bedrockruntime::types::AudioFormat::Flac,
        "ogg" | "audio/ogg" => aws_sdk_bedrockruntime::types::AudioFormat::Ogg,
        "opus" | "audio/opus" => aws_sdk_bedrockruntime::types::AudioFormat::Opus,
        "webm" | "audio/webm" => aws_sdk_bedrockruntime::types::AudioFormat::Webm,
        "m4a" | "audio/m4a" => aws_sdk_bedrockruntime::types::AudioFormat::M4A,
        "mp4" | "audio/mp4" => aws_sdk_bedrockruntime::types::AudioFormat::Mp4,
        _ => aws_sdk_bedrockruntime::types::AudioFormat::Mp3,
    }
}

fn bedrock_document_format(name: Option<&str>) -> aws_sdk_bedrockruntime::types::DocumentFormat {
    let lower = name.unwrap_or_default().to_ascii_lowercase();
    if lower.ends_with(".pdf") {
        aws_sdk_bedrockruntime::types::DocumentFormat::Pdf
    } else if lower.ends_with(".csv") {
        aws_sdk_bedrockruntime::types::DocumentFormat::Csv
    } else if lower.ends_with(".docx") {
        aws_sdk_bedrockruntime::types::DocumentFormat::Docx
    } else if lower.ends_with(".doc") {
        aws_sdk_bedrockruntime::types::DocumentFormat::Doc
    } else if lower.ends_with(".xlsx") {
        aws_sdk_bedrockruntime::types::DocumentFormat::Xlsx
    } else if lower.ends_with(".xls") {
        aws_sdk_bedrockruntime::types::DocumentFormat::Xls
    } else if lower.ends_with(".html") || lower.ends_with(".htm") {
        aws_sdk_bedrockruntime::types::DocumentFormat::Html
    } else if lower.ends_with(".md") || lower.ends_with(".markdown") {
        aws_sdk_bedrockruntime::types::DocumentFormat::Md
    } else {
        aws_sdk_bedrockruntime::types::DocumentFormat::Txt
    }
}

/// Bedrock document names exclude dots and several punctuation characters. Use
/// the stem and sanitize it before building the Converse request.
fn bedrock_document_name(name: Option<&str>) -> String {
    let source = name
        .and_then(|name| Path::new(name).file_stem())
        .and_then(|stem| stem.to_str())
        .unwrap_or("document");
    let cleaned: String = source
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric()
                || character.is_ascii_whitespace()
                || matches!(character, '-' | '(' | ')' | '[' | ']')
            {
                character
            } else {
                '-'
            }
        })
        .take(200)
        .collect();
    let cleaned = cleaned.trim();
    if cleaned.is_empty()
        || !cleaned
            .chars()
            .any(|character| character.is_ascii_alphanumeric())
    {
        "document".to_string()
    } else {
        cleaned.to_string()
    }
}

pub struct GaiseClientBedrock {
    client: BedrockClient,
    /// Control-plane client for `ListFoundationModels` / `ListInferenceProfiles`.
    /// Absent when the struct was built from a bare runtime client.
    control: Option<aws_sdk_bedrock::Client>,
}

impl GaiseClientBedrock {
    pub async fn new() -> Self {
        Self::new_with_region(None).await
    }

    /// Creates a Bedrock client with an optional explicit AWS region.
    ///
    /// The bundled WebPKI trust roots avoid platform certificate-store failures
    /// on minimal Windows and container installations. Credentials still come
    /// from the standard AWS SDK provider chain.
    #[allow(deprecated)]
    pub async fn new_with_region(region: Option<String>) -> Self {
        let https_connector = hyper_rustls::HttpsConnectorBuilder::new()
            .with_webpki_roots()
            .https_only()
            .enable_http1()
            .enable_http2()
            .build();
        let http_client = aws_smithy_runtime::client::http::hyper_014::HyperClientBuilder::new()
            .build(https_connector);

        let mut loader =
            aws_config::defaults(aws_config::BehaviorVersion::latest()).http_client(http_client);
        if let Some(region) = region {
            loader = loader.region(aws_sdk_bedrockruntime::config::Region::new(region));
        }
        let config = loader.load().await;
        let client = BedrockClient::new(&config);
        let control = aws_sdk_bedrock::Client::new(&config);
        Self {
            client,
            control: Some(control),
        }
    }

    /// Wrap an existing runtime client. Model listing is unavailable unless a
    /// control-plane client is also supplied via [`Self::with_clients`].
    pub fn with_client(client: BedrockClient) -> Self {
        Self {
            client,
            control: None,
        }
    }

    pub fn with_clients(client: BedrockClient, control: aws_sdk_bedrock::Client) -> Self {
        Self {
            client,
            control: Some(control),
        }
    }

    /// `ListFoundationModels` for the configured region.
    pub async fn list_foundation_models(
        &self,
    ) -> Result<Vec<BedrockModelSummary>, Box<dyn Error + Send + Sync>> {
        let control = self.control.as_ref().ok_or(
            "Bedrock model listing requires a control-plane client; construct with new_with_region or with_clients",
        )?;
        let output = control
            .list_foundation_models()
            .send()
            .await
            .map_err(|e| format!("ListFoundationModels failed: {}", e.into_service_error()))?;
        Ok(output
            .model_summaries()
            .iter()
            .map(BedrockModelSummary::from)
            .collect())
    }

    /// `ListInferenceProfiles` (system-defined cross-region profiles).
    pub async fn list_inference_profiles(
        &self,
    ) -> Result<Vec<BedrockInferenceProfile>, Box<dyn Error + Send + Sync>> {
        let control = self.control.as_ref().ok_or(
            "Bedrock model listing requires a control-plane client; construct with new_with_region or with_clients",
        )?;
        let mut profiles = Vec::new();
        let mut next_token: Option<String> = None;
        loop {
            let mut req = control
                .list_inference_profiles()
                .type_equals(aws_sdk_bedrock::types::InferenceProfileType::SystemDefined)
                .max_results(1000);
            if let Some(token) = &next_token {
                req = req.next_token(token);
            }
            let output = req
                .send()
                .await
                .map_err(|e| format!("ListInferenceProfiles failed: {}", e.into_service_error()))?;
            profiles.extend(
                output
                    .inference_profile_summaries()
                    .iter()
                    .map(BedrockInferenceProfile::from),
            );
            match output.next_token() {
                Some(token) if next_token.as_deref() != Some(token) => {
                    next_token = Some(token.to_string())
                }
                _ => break,
            }
        }
        Ok(profiles)
    }

    fn to_document(value: &serde_json::Value) -> aws_smithy_types::Document {
        match value {
            serde_json::Value::Null => aws_smithy_types::Document::Null,
            serde_json::Value::Bool(b) => aws_smithy_types::Document::Bool(*b),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    aws_smithy_types::Document::Number(aws_smithy_types::Number::NegInt(i))
                } else if let Some(u) = n.as_u64() {
                    aws_smithy_types::Document::Number(aws_smithy_types::Number::PosInt(u))
                } else {
                    aws_smithy_types::Document::Number(aws_smithy_types::Number::Float(
                        n.as_f64().unwrap_or(0.0),
                    ))
                }
            }
            serde_json::Value::String(s) => aws_smithy_types::Document::String(s.clone()),
            serde_json::Value::Array(a) => {
                aws_smithy_types::Document::Array(a.iter().map(Self::to_document).collect())
            }
            serde_json::Value::Object(o) => aws_smithy_types::Document::Object(
                o.iter()
                    .map(|(k, v)| (k.clone(), Self::to_document(v)))
                    .collect(),
            ),
        }
    }

    fn from_document(value: &aws_smithy_types::Document) -> serde_json::Value {
        match value {
            aws_smithy_types::Document::Null => serde_json::Value::Null,
            aws_smithy_types::Document::Bool(value) => serde_json::Value::Bool(*value),
            aws_smithy_types::Document::String(value) => serde_json::Value::String(value.clone()),
            aws_smithy_types::Document::Number(number) => match number {
                aws_smithy_types::Number::NegInt(value) => (*value).into(),
                aws_smithy_types::Number::PosInt(value) => (*value).into(),
                aws_smithy_types::Number::Float(value) => serde_json::Number::from_f64(*value)
                    .map(serde_json::Value::Number)
                    .unwrap_or(serde_json::Value::Null),
            },
            aws_smithy_types::Document::Array(values) => {
                serde_json::Value::Array(values.iter().map(Self::from_document).collect())
            }
            aws_smithy_types::Document::Object(values) => serde_json::Value::Object(
                values
                    .iter()
                    .map(|(key, value)| (key.clone(), Self::from_document(value)))
                    .collect(),
            ),
        }
    }

    /// Build the provider-specific reasoning fields used by Bedrock's Converse
    /// APIs. Bedrock has no portable effort member in `inferenceConfig`: Claude
    /// and Nova expose different shapes through `additionalModelRequestFields`.
    /// Keeping this mapping pure makes the wire contract testable without
    /// constructing an AWS client or loading credentials.
    fn reasoning_request_fields(request: &GaiseInstructRequest) -> Option<serde_json::Value> {
        let config = request.generation_config.as_ref()?;
        let effort = config.thinking_effort.as_deref();
        let display = config
            .include_thoughts
            .map(|include| if include { "summarized" } else { "omitted" });
        let model = request.model.to_ascii_lowercase();

        let adaptive_only_claude = [
            "anthropic.claude-mythos-5",
            "anthropic.claude-fable-5",
            "anthropic.claude-opus-5",
            "anthropic.claude-opus-4-8",
            "anthropic.claude-opus-4-7",
            "anthropic.claude-sonnet-5",
            "anthropic.claude-mythos-preview",
        ]
        .iter()
        .any(|family| model.contains(family));
        let adaptive_claude = adaptive_only_claude
            || ["anthropic.claude-opus-4-6", "anthropic.claude-sonnet-4-6"]
                .iter()
                .any(|family| model.contains(family));

        if adaptive_claude
            && (effort.is_some() || config.thinking_tokens.is_some() || display.is_some())
        {
            let mut thinking = serde_json::json!({ "type": "adaptive" });
            if let Some(display) = display {
                thinking["display"] = serde_json::Value::String(display.to_string());
            }
            let mut fields = serde_json::json!({ "thinking": thinking });
            if let Some(effort) = effort {
                fields["output_config"] = serde_json::json!({ "effort": effort });
            }
            return Some(fields);
        }

        if model.contains("anthropic.claude-opus-4-5")
            && let Some(effort) = effort
        {
            let mut fields = serde_json::json!({
                "output_config": { "effort": effort },
            });
            if let Some(tokens) = config.thinking_tokens {
                fields["thinking"] = serde_json::json!({
                    "type": "enabled",
                    "budget_tokens": tokens,
                });
                if let Some(display) = display {
                    fields["thinking"]["display"] = serde_json::Value::String(display.to_string());
                }
            }
            return Some(fields);
        }

        // Older reasoning-capable Claude models use a manual token budget.
        // Direct callers can still opt into that path through thinking_tokens.
        if model.contains("anthropic.claude")
            && !adaptive_only_claude
            && let Some(tokens) = config.thinking_tokens
        {
            let mut fields = serde_json::json!({
                "thinking": {
                    "type": "enabled",
                    "budget_tokens": tokens,
                },
            });
            if let Some(display) = display {
                fields["thinking"]["display"] = serde_json::Value::String(display.to_string());
            }
            return Some(fields);
        }

        let reasoning_nova = model.contains("amazon.nova-2")
            || model.contains("amazon.nova-lite-1-5")
            || model.contains("amazon.nova-pro-1-5");
        if reasoning_nova && effort.is_some() {
            return Some(serde_json::json!({
                "reasoningConfig": {
                    "type": "enabled",
                    "maxReasoningEffort": effort,
                },
            }));
        }

        None
    }

    fn omit_sampling_for_reasoning(request: &GaiseInstructRequest) -> bool {
        let model = request.model.to_ascii_lowercase();
        let effort = request
            .generation_config
            .as_ref()
            .and_then(|config| config.thinking_effort.as_deref());
        let fixed_sampling_claude = [
            "anthropic.claude-fable-5",
            "anthropic.claude-mythos-5",
            "anthropic.claude-mythos-preview",
            "anthropic.claude-opus-5",
            "anthropic.claude-opus-4-7",
            "anthropic.claude-opus-4-8",
            "anthropic.claude-sonnet-5",
        ]
        .iter()
        .any(|family| model.contains(family));
        let claude_thinking_enabled = model.contains("anthropic.claude")
            && Self::reasoning_request_fields(request)
                .is_some_and(|fields| !fields["thinking"].is_null());

        fixed_sampling_claude
            || claude_thinking_enabled
            || (model.contains("amazon.nova") && effort == Some("high"))
    }

    fn append_system_content(
        system: &mut Vec<aws_sdk_bedrockruntime::types::SystemContentBlock>,
        content: &GaiseContent,
    ) {
        match content {
            GaiseContent::Text { text } => system.push(
                aws_sdk_bedrockruntime::types::SystemContentBlock::Text(text.clone()),
            ),
            GaiseContent::Parts { parts } => {
                for part in parts {
                    Self::append_system_content(system, part);
                }
            }
            _ => {}
        }
    }

    fn tool_input_schema(parameter: Option<&GaiseToolParameter>) -> aws_smithy_types::Document {
        let mut schema = parameter
            .and_then(|parameter| serde_json::to_value(parameter).ok())
            .unwrap_or_else(|| serde_json::json!({}));
        if let Some(object) = schema.as_object_mut() {
            object
                .entry("type".to_string())
                .or_insert_with(|| serde_json::json!("object"));
            object
                .entry("properties".to_string())
                .or_insert_with(|| serde_json::json!({}));
        }
        Self::to_document(&schema)
    }

    fn map_gaise_content_to_bedrock(
        content: &GaiseContent,
    ) -> Vec<aws_sdk_bedrockruntime::types::ContentBlock> {
        match content {
            GaiseContent::Text { text } => vec![aws_sdk_bedrockruntime::types::ContentBlock::Text(
                text.clone(),
            )],
            GaiseContent::Image { data, format } => {
                let format = bedrock_image_format(format.as_deref());
                vec![aws_sdk_bedrockruntime::types::ContentBlock::Image(
                    aws_sdk_bedrockruntime::types::ImageBlock::builder()
                        .format(format)
                        .source(aws_sdk_bedrockruntime::types::ImageSource::Bytes(
                            aws_smithy_types::Blob::new(data.clone()),
                        ))
                        .build()
                        .expect("Failed to build ImageBlock"),
                )]
            }
            GaiseContent::Audio { data, format } => {
                vec![aws_sdk_bedrockruntime::types::ContentBlock::Audio(
                    aws_sdk_bedrockruntime::types::AudioBlock::builder()
                        .format(bedrock_audio_format(format.as_deref()))
                        .source(aws_sdk_bedrockruntime::types::AudioSource::Bytes(
                            aws_smithy_types::Blob::new(data.clone()),
                        ))
                        .build()
                        .expect("Failed to build AudioBlock"),
                )]
            }
            GaiseContent::Parts { parts } => parts
                .iter()
                .flat_map(Self::map_gaise_content_to_bedrock)
                .collect(),
            GaiseContent::File { data, name } => {
                let format = bedrock_document_format(name.as_deref());

                vec![aws_sdk_bedrockruntime::types::ContentBlock::Document(
                    aws_sdk_bedrockruntime::types::DocumentBlock::builder()
                        .name(bedrock_document_name(name.as_deref()))
                        .format(format)
                        .source(aws_sdk_bedrockruntime::types::DocumentSource::Bytes(
                            aws_smithy_types::Blob::new(data.clone()),
                        ))
                        .build()
                        .expect("Failed to build DocumentBlock"),
                )]
            }
            GaiseContent::Reasoning { text, signature } => vec![
                aws_sdk_bedrockruntime::types::ContentBlock::ReasoningContent(
                    aws_sdk_bedrockruntime::types::ReasoningContentBlock::ReasoningText(
                        aws_sdk_bedrockruntime::types::ReasoningTextBlock::builder()
                            .text(text)
                            .set_signature(signature.clone())
                            .build()
                            .expect("Failed to build ReasoningTextBlock"),
                    ),
                ),
            ],
            GaiseContent::RedactedReasoning { data } => vec![
                aws_sdk_bedrockruntime::types::ContentBlock::ReasoningContent(
                    aws_sdk_bedrockruntime::types::ReasoningContentBlock::RedactedContent(
                        aws_smithy_types::Blob::new(data.clone()),
                    ),
                ),
            ],
        }
    }

    fn map_tool_result_content(
        content: &GaiseContent,
    ) -> Vec<aws_sdk_bedrockruntime::types::ToolResultContentBlock> {
        match content {
            GaiseContent::Text { text } => {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(text) {
                    vec![aws_sdk_bedrockruntime::types::ToolResultContentBlock::Json(
                        Self::to_document(&json),
                    )]
                } else {
                    vec![aws_sdk_bedrockruntime::types::ToolResultContentBlock::Text(
                        text.clone(),
                    )]
                }
            }
            GaiseContent::Image { data, format } => vec![
                aws_sdk_bedrockruntime::types::ToolResultContentBlock::Image(
                    aws_sdk_bedrockruntime::types::ImageBlock::builder()
                        .format(bedrock_image_format(format.as_deref()))
                        .source(aws_sdk_bedrockruntime::types::ImageSource::Bytes(
                            aws_smithy_types::Blob::new(data.clone()),
                        ))
                        .build()
                        .expect("Failed to build tool-result ImageBlock"),
                ),
            ],
            GaiseContent::File { data, name } => vec![
                aws_sdk_bedrockruntime::types::ToolResultContentBlock::Document(
                    aws_sdk_bedrockruntime::types::DocumentBlock::builder()
                        .name(bedrock_document_name(name.as_deref()))
                        .format(bedrock_document_format(name.as_deref()))
                        .source(aws_sdk_bedrockruntime::types::DocumentSource::Bytes(
                            aws_smithy_types::Blob::new(data.clone()),
                        ))
                        .build()
                        .expect("Failed to build tool-result DocumentBlock"),
                ),
            ],
            GaiseContent::Reasoning { text, .. } => {
                vec![aws_sdk_bedrockruntime::types::ToolResultContentBlock::Text(
                    text.clone(),
                )]
            }
            GaiseContent::RedactedReasoning { .. } => {
                vec![aws_sdk_bedrockruntime::types::ToolResultContentBlock::Text(
                    "[Redacted reasoning cannot be used as a tool result]".to_string(),
                )]
            }
            GaiseContent::Parts { parts } => parts
                .iter()
                .flat_map(Self::map_tool_result_content)
                .collect(),
            GaiseContent::Audio { .. } => {
                vec![aws_sdk_bedrockruntime::types::ToolResultContentBlock::Text(
                    "[Audio tool results are not supported by Bedrock Converse]".to_string(),
                )]
            }
        }
    }

    fn map_gaise_message_to_bedrock(
        msg: &GaiseMessage,
    ) -> Option<aws_sdk_bedrockruntime::types::Message> {
        if let Some(tool_use_id) = &msg.tool_call_id {
            let mut result_content = Vec::new();
            if let Some(content) = &msg.content {
                match content {
                    OneOrMany::One(content) => {
                        result_content.extend(Self::map_tool_result_content(content));
                    }
                    OneOrMany::Many(contents) => {
                        for content in contents {
                            result_content.extend(Self::map_tool_result_content(content));
                        }
                    }
                }
            }
            if result_content.is_empty() {
                result_content.push(aws_sdk_bedrockruntime::types::ToolResultContentBlock::Text(
                    String::new(),
                ));
            }
            let has_document = result_content.iter().any(|block| {
                matches!(
                    block,
                    aws_sdk_bedrockruntime::types::ToolResultContentBlock::Document(_)
                )
            });
            let has_text = result_content.iter().any(|block| {
                matches!(
                    block,
                    aws_sdk_bedrockruntime::types::ToolResultContentBlock::Text(_)
                )
            });
            if has_document && !has_text {
                result_content.insert(
                    0,
                    aws_sdk_bedrockruntime::types::ToolResultContentBlock::Text(
                        "Tool result document attached.".to_string(),
                    ),
                );
            }
            let tool_result = aws_sdk_bedrockruntime::types::ToolResultBlock::builder()
                .tool_use_id(tool_use_id)
                .set_content(Some(result_content))
                .build()
                .expect("Failed to build ToolResultBlock");
            return Some(
                aws_sdk_bedrockruntime::types::Message::builder()
                    .role(aws_sdk_bedrockruntime::types::ConversationRole::User)
                    .content(aws_sdk_bedrockruntime::types::ContentBlock::ToolResult(
                        tool_result,
                    ))
                    .build()
                    .expect("Failed to build tool-result Message"),
            );
        }

        let role = match msg.role.as_str() {
            "user" => aws_sdk_bedrockruntime::types::ConversationRole::User,
            "assistant" => aws_sdk_bedrockruntime::types::ConversationRole::Assistant,
            _ => return None,
        };

        let mut content_blocks = Vec::new();

        if let Some(content) = &msg.content {
            match content {
                OneOrMany::One(c) => content_blocks.extend(Self::map_gaise_content_to_bedrock(c)),
                OneOrMany::Many(v) => {
                    for c in v {
                        content_blocks.extend(Self::map_gaise_content_to_bedrock(c));
                    }
                }
            }
        }

        if let Some(tool_calls) = &msg.tool_calls {
            for tool_call in tool_calls {
                let input = tool_call
                    .function
                    .arguments
                    .as_deref()
                    .and_then(|arguments| serde_json::from_str(arguments).ok())
                    .unwrap_or_else(|| serde_json::json!({}));
                let tool_use = aws_sdk_bedrockruntime::types::ToolUseBlock::builder()
                    .tool_use_id(&tool_call.id)
                    .name(&tool_call.function.name)
                    .input(Self::to_document(&input))
                    .build()
                    .expect("Failed to build ToolUseBlock");
                content_blocks.push(aws_sdk_bedrockruntime::types::ContentBlock::ToolUse(
                    tool_use,
                ));
            }
        }

        // Converse requires a text block whenever a document is present in a
        // message. Supply a neutral instruction when the caller sent only files.
        let has_document = content_blocks.iter().any(|block| {
            matches!(
                block,
                aws_sdk_bedrockruntime::types::ContentBlock::Document(_)
            )
        });
        let has_text = content_blocks
            .iter()
            .any(|block| matches!(block, aws_sdk_bedrockruntime::types::ContentBlock::Text(_)));
        if has_document && !has_text {
            content_blocks.insert(
                0,
                aws_sdk_bedrockruntime::types::ContentBlock::Text(
                    "Process the attached document.".to_string(),
                ),
            );
        }

        if content_blocks.is_empty() {
            return None;
        }

        Some(
            aws_sdk_bedrockruntime::types::Message::builder()
                .role(role)
                .set_content(Some(content_blocks))
                .build()
                .expect("Failed to build Message"),
        )
    }
}

#[async_trait]
impl GaiseClient for GaiseClientBedrock {
    async fn list_models(
        &self,
        request: &GaiseListModelsRequest,
    ) -> Result<GaiseListModelsResponse, Box<dyn Error + Send + Sync>> {
        let summaries = self.list_foundation_models().await?;
        let mut models: Vec<gaise_core::contracts::GaiseModel> = summaries
            .iter()
            .map(|s| map_foundation_model(s, request.include_raw))
            .collect();
        let mut errors = Vec::new();
        // Profiles are what callers invoke across regions; a failure here
        // should not hide the foundation-model list.
        match self.list_inference_profiles().await {
            Ok(profiles) => {
                models.extend(map_inference_profiles(
                    &profiles,
                    &models,
                    request.include_raw,
                ));
            }
            Err(e) => errors.push(gaise_core::contracts::GaiseProviderError {
                provider: "bedrock".to_string(),
                message: format!("inference profiles unavailable: {e}"),
            }),
        }
        let mut response = GaiseListModelsResponse { models, errors };
        response.retain_operation(request.operation);
        Ok(response)
    }

    async fn instruct(
        &self,
        request: &GaiseInstructRequest,
    ) -> Result<GaiseInstructResponse, Box<dyn Error + Send + Sync>> {
        let mut messages = Vec::new();
        let mut system_messages = Vec::new();

        let inputs = match &request.input {
            OneOrMany::One(m) => vec![m],
            OneOrMany::Many(v) => v.iter().collect(),
        };

        for msg in inputs {
            if msg.role == "system" {
                if let Some(content) = &msg.content {
                    match content {
                        OneOrMany::One(content) => {
                            Self::append_system_content(&mut system_messages, content)
                        }
                        OneOrMany::Many(v) => {
                            for content in v {
                                Self::append_system_content(&mut system_messages, content);
                            }
                        }
                    }
                }
            } else if let Some(m) = Self::map_gaise_message_to_bedrock(msg) {
                messages.push(m);
            }
        }

        let mut builder = self
            .client
            .converse()
            .model_id(&request.model)
            .set_messages(Some(messages));

        if !system_messages.is_empty() {
            builder = builder.set_system(Some(system_messages));
        }

        if let Some(config) = &request.generation_config {
            let mut inf_cfg = aws_sdk_bedrockruntime::types::InferenceConfiguration::builder();
            if !Self::omit_sampling_for_reasoning(request) {
                if let Some(t) = config.temperature {
                    inf_cfg = inf_cfg.temperature(t);
                }
                if let Some(p) = config.top_p {
                    inf_cfg = inf_cfg.top_p(p);
                }
            }
            if let Some(m) = config.max_tokens {
                inf_cfg = inf_cfg.max_tokens(m as i32);
            }
            builder = builder.inference_config(inf_cfg.build());
        }

        if let Some(fields) = Self::reasoning_request_fields(request) {
            builder = builder.additional_model_request_fields(Self::to_document(&fields));
        }

        if let Some(tools) = &request.tools {
            let mut tool_list = Vec::new();
            for t in tools {
                let tool_spec = aws_sdk_bedrockruntime::types::ToolSpecification::builder()
                    .name(&t.name)
                    .set_description(t.description.clone())
                    .input_schema(aws_sdk_bedrockruntime::types::ToolInputSchema::Json(
                        Self::tool_input_schema(t.parameters.as_ref()),
                    ))
                    .build()
                    .expect("Failed to build ToolSpec");
                tool_list.push(aws_sdk_bedrockruntime::types::Tool::ToolSpec(tool_spec));
            }
            builder = builder.tool_config(
                aws_sdk_bedrockruntime::types::ToolConfiguration::builder()
                    .set_tools(Some(tool_list))
                    .build()
                    .expect("Failed to build ToolConfiguration"),
            );
        }

        let response = builder.send().await?;

        let usage = response.usage.as_ref().map(map_bedrock_usage);

        let output = response.output.ok_or("No output from Bedrock")?;
        let message = match output {
            aws_sdk_bedrockruntime::types::ConverseOutput::Message(m) => m,
            _ => return Err("Unexpected output type from Bedrock".into()),
        };

        let mut gaise_content = Vec::new();
        let mut tool_calls = Vec::new();

        for block in message.content {
            match block {
                aws_sdk_bedrockruntime::types::ContentBlock::Text(t) => {
                    gaise_content.push(GaiseContent::Text { text: t })
                }
                aws_sdk_bedrockruntime::types::ContentBlock::ToolUse(tu) => {
                    tool_calls.push(GaiseToolCall {
                        id: tu.tool_use_id,
                        r#type: "function".to_string(),
                        function: GaiseFunctionCall {
                            name: tu.name,
                            arguments: Some(Self::from_document(&tu.input).to_string()),
                        },
                        thought_signature: None,
                    });
                }
                aws_sdk_bedrockruntime::types::ContentBlock::Image(image) => {
                    if let Some(aws_sdk_bedrockruntime::types::ImageSource::Bytes(data)) =
                        image.source
                    {
                        gaise_content.push(GaiseContent::Image {
                            data: data.into_inner(),
                            format: Some(format!("image/{}", image.format.as_str())),
                        });
                    }
                }
                aws_sdk_bedrockruntime::types::ContentBlock::Audio(audio) => {
                    if let Some(aws_sdk_bedrockruntime::types::AudioSource::Bytes(data)) =
                        audio.source
                    {
                        gaise_content.push(GaiseContent::Audio {
                            data: data.into_inner(),
                            format: Some(format!("audio/{}", audio.format.as_str())),
                        });
                    }
                }
                aws_sdk_bedrockruntime::types::ContentBlock::ReasoningContent(
                    aws_sdk_bedrockruntime::types::ReasoningContentBlock::ReasoningText(reasoning),
                ) => gaise_content.push(GaiseContent::Reasoning {
                    text: reasoning.text,
                    signature: reasoning.signature,
                }),
                aws_sdk_bedrockruntime::types::ContentBlock::ReasoningContent(
                    aws_sdk_bedrockruntime::types::ReasoningContentBlock::RedactedContent(data),
                ) => gaise_content.push(GaiseContent::RedactedReasoning {
                    data: data.into_inner(),
                }),
                _ => {}
            }
        }

        Ok(GaiseInstructResponse {
            output: OneOrMany::One(GaiseMessage {
                role: "assistant".to_string(),
                content: if gaise_content.is_empty() {
                    None
                } else {
                    Some(OneOrMany::Many(gaise_content))
                },
                tool_calls: if tool_calls.is_empty() {
                    None
                } else {
                    Some(tool_calls)
                },
                tool_call_id: None,
                tool_name: None,
            }),
            external_id: None,
            usage,
        })
    }

    async fn instruct_stream(
        &self,
        request: &GaiseInstructRequest,
    ) -> Result<
        Pin<
            Box<
                dyn Stream<Item = Result<GaiseInstructStreamResponse, Box<dyn Error + Send + Sync>>>
                    + Send,
            >,
        >,
        Box<dyn Error + Send + Sync>,
    > {
        let mut messages = Vec::new();
        let mut system_messages = Vec::new();

        let inputs = match &request.input {
            OneOrMany::One(m) => vec![m],
            OneOrMany::Many(v) => v.iter().collect(),
        };

        for msg in inputs {
            if msg.role == "system" {
                if let Some(content) = &msg.content {
                    match content {
                        OneOrMany::One(content) => {
                            Self::append_system_content(&mut system_messages, content)
                        }
                        OneOrMany::Many(v) => {
                            for content in v {
                                Self::append_system_content(&mut system_messages, content);
                            }
                        }
                    }
                }
            } else if let Some(m) = Self::map_gaise_message_to_bedrock(msg) {
                messages.push(m);
            }
        }

        let mut builder = self
            .client
            .converse_stream()
            .model_id(&request.model)
            .set_messages(Some(messages));

        if !system_messages.is_empty() {
            builder = builder.set_system(Some(system_messages));
        }

        if let Some(config) = &request.generation_config {
            let mut inf_cfg = aws_sdk_bedrockruntime::types::InferenceConfiguration::builder();
            if !Self::omit_sampling_for_reasoning(request) {
                if let Some(t) = config.temperature {
                    inf_cfg = inf_cfg.temperature(t);
                }
                if let Some(p) = config.top_p {
                    inf_cfg = inf_cfg.top_p(p);
                }
            }
            if let Some(m) = config.max_tokens {
                inf_cfg = inf_cfg.max_tokens(m as i32);
            }
            builder = builder.inference_config(inf_cfg.build());
        }

        if let Some(fields) = Self::reasoning_request_fields(request) {
            builder = builder.additional_model_request_fields(Self::to_document(&fields));
        }

        if let Some(tools) = &request.tools {
            let mut tool_list = Vec::new();
            for t in tools {
                let tool_spec = aws_sdk_bedrockruntime::types::ToolSpecification::builder()
                    .name(&t.name)
                    .set_description(t.description.clone())
                    .input_schema(aws_sdk_bedrockruntime::types::ToolInputSchema::Json(
                        Self::tool_input_schema(t.parameters.as_ref()),
                    ))
                    .build()
                    .expect("Failed to build ToolSpec");
                tool_list.push(aws_sdk_bedrockruntime::types::Tool::ToolSpec(tool_spec));
            }
            builder = builder.tool_config(
                aws_sdk_bedrockruntime::types::ToolConfiguration::builder()
                    .set_tools(Some(tool_list))
                    .build()
                    .expect("Failed to build ToolConfiguration"),
            );
        }

        let response = builder.send().await?;
        let mut stream = response.stream;

        let gaise_stream = async_stream::stream! {
            let mut images: std::collections::HashMap<i32, (String, Vec<u8>)> =
                std::collections::HashMap::new();
            loop {
                let event = match stream.recv().await {
                    Ok(Some(event)) => event,
                    Ok(None) => break,
                    Err(error) => {
                        yield Err(Box::new(error) as Box<dyn Error + Send + Sync>);
                        break;
                    }
                };

                match event {
                    aws_sdk_bedrockruntime::types::ConverseStreamOutput::ContentBlockStart(start) => {
                        match start.start {
                            Some(aws_sdk_bedrockruntime::types::ContentBlockStart::ToolUse(tool)) => {
                                yield Ok(GaiseInstructStreamResponse {
                                    chunk: GaiseStreamChunk::ToolCall {
                                        index: start.content_block_index.max(0) as usize,
                                        id: Some(tool.tool_use_id),
                                        name: Some(tool.name),
                                        arguments: None,
                                        thought_signature: None,
                                    },
                                    external_id: None,
                                });
                            }
                            Some(aws_sdk_bedrockruntime::types::ContentBlockStart::Image(image)) => {
                                images.insert(
                                    start.content_block_index,
                                    (format!("image/{}", image.format.as_str()), Vec::new()),
                                );
                            }
                            _ => {}
                        }
                    }
                    aws_sdk_bedrockruntime::types::ConverseStreamOutput::ContentBlockDelta(delta) => {
                        match delta.delta {
                            Some(aws_sdk_bedrockruntime::types::ContentBlockDelta::Text(text)) => {
                                yield Ok(GaiseInstructStreamResponse {
                                    chunk: GaiseStreamChunk::Text(text),
                                    external_id: None,
                                });
                            }
                            Some(aws_sdk_bedrockruntime::types::ContentBlockDelta::ToolUse(tool)) => {
                                yield Ok(GaiseInstructStreamResponse {
                                    chunk: GaiseStreamChunk::ToolCall {
                                        index: delta.content_block_index.max(0) as usize,
                                        id: None,
                                        name: None,
                                        arguments: Some(tool.input),
                                        thought_signature: None,
                                    },
                                    external_id: None,
                                });
                            }
                            Some(aws_sdk_bedrockruntime::types::ContentBlockDelta::ReasoningContent(reasoning)) => {
                                match reasoning {
                                    aws_sdk_bedrockruntime::types::ReasoningContentBlockDelta::Text(text) => {
                                        if !text.is_empty() {
                                            yield Ok(GaiseInstructStreamResponse {
                                                chunk: GaiseStreamChunk::Content(GaiseContent::Reasoning {
                                                    text,
                                                    signature: None,
                                                }),
                                                external_id: None,
                                            });
                                        }
                                    }
                                    aws_sdk_bedrockruntime::types::ReasoningContentBlockDelta::Signature(signature) => {
                                        yield Ok(GaiseInstructStreamResponse {
                                            chunk: GaiseStreamChunk::Content(GaiseContent::Reasoning {
                                                text: String::new(),
                                                signature: Some(signature),
                                            }),
                                            external_id: None,
                                        });
                                    }
                                    aws_sdk_bedrockruntime::types::ReasoningContentBlockDelta::RedactedContent(data) => {
                                        yield Ok(GaiseInstructStreamResponse {
                                            chunk: GaiseStreamChunk::Content(GaiseContent::RedactedReasoning {
                                                data: data.into_inner(),
                                            }),
                                            external_id: None,
                                        });
                                    }
                                    _ => {}
                                }
                            }
                            Some(aws_sdk_bedrockruntime::types::ContentBlockDelta::Image(image)) => {
                                if let Some(aws_sdk_bedrockruntime::types::ImageSource::Bytes(data)) = image.source
                                    && let Some((_, bytes)) = images.get_mut(&delta.content_block_index)
                                {
                                    bytes.extend_from_slice(data.as_ref());
                                }
                            }
                            _ => {}
                        }
                    }
                    aws_sdk_bedrockruntime::types::ConverseStreamOutput::ContentBlockStop(stop) => {
                        if let Some((format, data)) = images.remove(&stop.content_block_index) {
                            yield Ok(GaiseInstructStreamResponse {
                                chunk: GaiseStreamChunk::Content(GaiseContent::Image {
                                    data,
                                    format: Some(format),
                                }),
                                external_id: None,
                            });
                        }
                    }
                    aws_sdk_bedrockruntime::types::ConverseStreamOutput::Metadata(metadata) => {
                        if let Some(usage) = metadata.usage {
                            yield Ok(GaiseInstructStreamResponse {
                                chunk: GaiseStreamChunk::Usage(map_bedrock_usage(&usage)),
                                external_id: None,
                            });
                        }
                    }
                    _ => {}
                }
            }
        };

        Ok(Box::pin(gaise_stream))
    }

    async fn embeddings(
        &self,
        request: &GaiseEmbeddingsRequest,
    ) -> Result<GaiseEmbeddingsResponse, Box<dyn Error + Send + Sync>> {
        let inputs = match &request.input {
            OneOrMany::One(s) => vec![s.clone()],
            OneOrMany::Many(v) => v.clone(),
        };

        let mut embeddings = Vec::new();
        // Titan reports this counter for every InvokeModel response. Cohere's
        // Bedrock embedding response does not expose token usage, so preserve
        // `None` instead of estimating it locally.
        let mut input_tokens = request.model.contains("titan").then_some(0usize);

        for input in inputs {
            let body = if request.model.contains("titan") {
                serde_json::json!({
                    "inputText": input
                })
            } else if request.model.contains("cohere") {
                serde_json::json!({
                    "texts": [input],
                    "input_type": "search_document"
                })
            } else {
                return Err(format!("Unsupported embedding model: {}", request.model).into());
            };

            let response = self
                .client
                .invoke_model()
                .model_id(&request.model)
                .content_type("application/json")
                .body(aws_smithy_types::Blob::new(serde_json::to_vec(&body)?))
                .send()
                .await?;

            let response_body: serde_json::Value = serde_json::from_slice(response.body.as_ref())?;

            if request.model.contains("titan") {
                input_tokens = input_tokens.and_then(|total| {
                    bedrock_embedding_input_tokens(&response_body)
                        .and_then(|value| total.checked_add(value))
                });
                if let Some(embedding) = response_body["embedding"].as_array() {
                    let vec: Vec<f32> = embedding
                        .iter()
                        .map(|v| v.as_f64().unwrap_or(0.0) as f32)
                        .collect();
                    embeddings.push(vec);
                }
            } else if request.model.contains("cohere")
                && let Some(embeddings_arr) = response_body["embeddings"].as_array()
                && let Some(first) = embeddings_arr.first()
                && let Some(embedding) = first.as_array()
            {
                let vec: Vec<f32> = embedding
                    .iter()
                    .map(|v| v.as_f64().unwrap_or(0.0) as f32)
                    .collect();
                embeddings.push(vec);
            }
        }

        let usage = input_tokens.map(|input_tokens| gaise_core::contracts::GaiseUsage {
            input: Some(std::collections::HashMap::from([(
                "input_tokens".to_string(),
                input_tokens,
            )])),
            output: None,
            total: Some(std::collections::HashMap::from([(
                "total_tokens".to_string(),
                input_tokens,
            )])),
        });

        Ok(GaiseEmbeddingsResponse {
            output: embeddings,
            usage,
            ..Default::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gaise_core::contracts::GaiseGenerationConfig;

    fn request(
        model: &str,
        effort: Option<&str>,
        thinking_tokens: Option<usize>,
    ) -> GaiseInstructRequest {
        GaiseInstructRequest {
            model: model.to_string(),
            generation_config: Some(GaiseGenerationConfig {
                thinking_effort: effort.map(str::to_string),
                thinking_tokens,
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[test]
    fn maps_adaptive_claude_effort_without_aws_client() {
        for effort in ["low", "medium", "high", "max"] {
            let fields = GaiseClientBedrock::reasoning_request_fields(&request(
                "us.anthropic.claude-opus-4-6-v1:0",
                Some(effort),
                None,
            ))
            .unwrap();
            assert_eq!(fields["thinking"]["type"], "adaptive");
            assert_eq!(fields["output_config"]["effort"], effort);
        }
    }

    #[test]
    fn maps_nova_effort_without_aws_client() {
        for effort in ["low", "medium", "high"] {
            let fields = GaiseClientBedrock::reasoning_request_fields(&request(
                "us.amazon.nova-2-lite-v1:0",
                Some(effort),
                None,
            ))
            .unwrap();
            assert_eq!(fields["reasoningConfig"]["type"], "enabled");
            assert_eq!(fields["reasoningConfig"]["maxReasoningEffort"], effort);
        }
    }

    #[test]
    fn maps_manual_claude_budget_and_omits_unsupported_models() {
        let fields = GaiseClientBedrock::reasoning_request_fields(&request(
            "anthropic.claude-sonnet-4-5-20250929-v1:0",
            None,
            Some(4096),
        ))
        .unwrap();
        assert_eq!(fields["thinking"]["type"], "enabled");
        assert_eq!(fields["thinking"]["budget_tokens"], 4096);

        assert!(
            GaiseClientBedrock::reasoning_request_fields(&request(
                "amazon.titan-text-express-v1",
                Some("high"),
                Some(4096),
            ))
            .is_none()
        );
        let adaptive_fallback = GaiseClientBedrock::reasoning_request_fields(&request(
            "us.anthropic.claude-fable-5-v1:0",
            None,
            Some(4096),
        ))
        .unwrap();
        assert_eq!(adaptive_fallback["thinking"]["type"], "adaptive");
        assert!(adaptive_fallback["thinking"]["budget_tokens"].is_null());
        assert!(
            GaiseClientBedrock::reasoning_request_fields(&GaiseInstructRequest::default())
                .is_none()
        );
    }

    #[test]
    fn maps_thought_display_and_sampling_rules_without_aws_client() {
        let mut fable = request("anthropic.claude-fable-5", None, None);
        let config = fable.generation_config.as_mut().unwrap();
        config.include_thoughts = Some(true);
        config.temperature = Some(0.2);
        config.top_p = Some(0.8);
        let fields = GaiseClientBedrock::reasoning_request_fields(&fable).unwrap();
        assert_eq!(fields["thinking"]["type"], "adaptive");
        assert_eq!(fields["thinking"]["display"], "summarized");
        assert!(GaiseClientBedrock::omit_sampling_for_reasoning(&fable));

        // Opus 5 (2026-07-24) follows the Opus 4.7/4.8 rules on Bedrock too.
        let mut opus5 = request("global.anthropic.claude-opus-5", Some("high"), None);
        opus5.generation_config.as_mut().unwrap().temperature = Some(0.2);
        let fields = GaiseClientBedrock::reasoning_request_fields(&opus5).unwrap();
        assert_eq!(fields["thinking"]["type"], "adaptive");
        assert!(GaiseClientBedrock::omit_sampling_for_reasoning(&opus5));

        let mut manual = request(
            "anthropic.claude-sonnet-4-5-20250929-v1:0",
            None,
            Some(4096),
        );
        manual.generation_config.as_mut().unwrap().include_thoughts = Some(false);
        let fields = GaiseClientBedrock::reasoning_request_fields(&manual).unwrap();
        assert_eq!(fields["thinking"]["display"], "omitted");
        assert!(GaiseClientBedrock::omit_sampling_for_reasoning(&manual));

        assert!(GaiseClientBedrock::omit_sampling_for_reasoning(&request(
            "amazon.nova-2-lite-v1:0",
            Some("high"),
            None,
        )));
        assert!(!GaiseClientBedrock::omit_sampling_for_reasoning(&request(
            "amazon.nova-2-lite-v1:0",
            Some("low"),
            None,
        )));
    }

    #[test]
    fn creates_valid_empty_tool_schema_and_flattens_nested_system_text() {
        let schema = GaiseClientBedrock::tool_input_schema(None);
        let schema = GaiseClientBedrock::from_document(&schema);
        assert_eq!(schema["type"], "object");
        assert_eq!(schema["properties"], serde_json::json!({}));

        let mut system = Vec::new();
        GaiseClientBedrock::append_system_content(
            &mut system,
            &GaiseContent::Parts {
                parts: vec![
                    GaiseContent::Text {
                        text: "first".to_string(),
                    },
                    GaiseContent::Parts {
                        parts: vec![GaiseContent::Text {
                            text: "second".to_string(),
                        }],
                    },
                ],
            },
        );
        assert_eq!(system.len(), 2);
        assert!(matches!(
            &system[0],
            aws_sdk_bedrockruntime::types::SystemContentBlock::Text(text) if text == "first"
        ));
        assert!(matches!(
            &system[1],
            aws_sdk_bedrockruntime::types::SystemContentBlock::Text(text) if text == "second"
        ));
    }

    #[test]
    fn maps_image_mime_types_used_by_gaise_without_aws_client() {
        assert_eq!(bedrock_image_format(Some("image/png")).as_str(), "png");
        assert_eq!(bedrock_image_format(Some("image/jpeg")).as_str(), "jpeg");
        assert_eq!(bedrock_image_format(Some("image/webp")).as_str(), "webp");
        assert_eq!(bedrock_image_format(Some("image/gif")).as_str(), "gif");
        assert_eq!(bedrock_image_format(None).as_str(), "jpeg");
    }

    #[test]
    fn maps_redacted_reasoning_bytes_without_aws_client() {
        let blocks =
            GaiseClientBedrock::map_gaise_content_to_bedrock(&GaiseContent::RedactedReasoning {
                data: vec![1, 2, 3],
            });
        assert!(matches!(
            &blocks[0],
            aws_sdk_bedrockruntime::types::ContentBlock::ReasoningContent(
                aws_sdk_bedrockruntime::types::ReasoningContentBlock::RedactedContent(data)
            ) if data.as_ref() == [1, 2, 3]
        ));
    }

    #[test]
    fn maps_document_formats_case_insensitively_and_sanitizes_names() {
        assert_eq!(bedrock_document_format(Some("REPORT.PDF")).as_str(), "pdf");
        assert_eq!(bedrock_document_format(Some("brief.DOCX")).as_str(), "docx");
        assert_eq!(bedrock_document_format(Some("sheet.XLSX")).as_str(), "xlsx");
        assert_eq!(bedrock_document_format(None).as_str(), "txt");
        assert_eq!(
            bedrock_document_name(Some("quarterly.report (final).pdf")),
            "quarterly-report (final)"
        );
        assert_eq!(bedrock_document_name(Some("...pdf")), "document");
        assert_eq!(bedrock_document_name(None), "document");
    }

    #[test]
    fn maps_bedrock_usage_cache_and_total_without_fabricated_modalities() {
        let cache_detail = aws_sdk_bedrockruntime::types::CacheDetail::builder()
            .ttl(aws_sdk_bedrockruntime::types::CacheTtl::OneHour)
            .input_tokens(4)
            .build()
            .unwrap();
        let usage = aws_sdk_bedrockruntime::types::TokenUsage::builder()
            .input_tokens(20)
            .output_tokens(30)
            .total_tokens(50)
            .cache_read_input_tokens(5)
            .cache_write_input_tokens(7)
            .cache_details(cache_detail)
            .build()
            .unwrap();
        let mapped = map_bedrock_usage(&usage);
        let input = mapped.input.unwrap();
        assert_eq!(input.get("input_tokens"), Some(&20));
        assert_eq!(input.get("effective_input_tokens"), Some(&32));
        assert_eq!(input.get("cache_read_input_tokens"), Some(&5));
        assert_eq!(input.get("cache_write_1h_input_tokens"), Some(&4));
        assert!(!input.contains_key("image_tokens"));
        assert!(!input.contains_key("audio_tokens"));
        assert_eq!(mapped.output.unwrap().get("output_tokens"), Some(&30));
        assert_eq!(mapped.total.unwrap().get("total_tokens"), Some(&50));
    }

    #[test]
    fn maps_titan_embedding_input_usage_without_estimation() {
        let response = serde_json::json!({
            "embedding": [0.1, 0.2],
            "inputTextTokenCount": 17
        });
        assert_eq!(bedrock_embedding_input_tokens(&response), Some(17));
        assert_eq!(
            bedrock_embedding_input_tokens(&serde_json::json!({"embedding": [0.1]})),
            None
        );
    }
}
