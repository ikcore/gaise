//! xAI `GET /v1/models` contract and mapping.
//!
//! Unlike OpenAI, the xAI model object reports aliases, the context length,
//! and the accepted `reasoning_effort` values. Modalities are not reported,
//! so they come from a conservative id heuristic and the registry overlay.

use gaise_core::contracts::{
    GaiseMetadataSource, GaiseModality, GaiseModel, GaiseOperation, GaiseSupport, rfc3339_from_unix,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XaiModelList {
    #[serde(default)]
    pub object: Option<String>,
    #[serde(default)]
    pub data: Vec<XaiModelObject>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XaiModelObject {
    pub id: String,
    #[serde(default)]
    pub object: Option<String>,
    #[serde(default)]
    pub created: Option<i64>,
    #[serde(default)]
    pub owned_by: Option<String>,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub context_length: Option<u64>,
    #[serde(default)]
    pub capabilities: Option<XaiModelCapabilities>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct XaiModelCapabilities {
    #[serde(default)]
    pub reasoning_effort: Vec<String>,
    #[serde(default)]
    pub default_reasoning_effort: Option<String>,
}

/// Rough surface classification from the identifier alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XaiModelKind {
    /// Grok text models served by Chat Completions.
    Chat,
    /// Served only by the Responses API (the multi-agent model).
    ResponsesOnly,
    /// Image generation and editing (`/v1/images/*`).
    Image,
    /// Video generation (`/v1/videos/*`).
    Video,
    /// Speech-to-speech over `wss://…/v1/realtime`.
    Realtime,
    /// Speech-to-text (`/v1/stt`), which no GAISe surface drives.
    Transcription,
    Other,
}

/// Classify an xAI model id. Only patterns with a single well-known meaning
/// are recognised; anything else is `Other` and left for the registry overlay.
pub fn classify_xai_model_id(id: &str) -> XaiModelKind {
    let id = id.to_ascii_lowercase();
    if id.starts_with("grok-imagine-image") || id.starts_with("grok-2-image") {
        return XaiModelKind::Image;
    }
    if id.starts_with("grok-imagine-video") {
        return XaiModelKind::Video;
    }
    if id.starts_with("grok-voice-transcribe") || id == "grok-transcribe" {
        return XaiModelKind::Transcription;
    }
    if id.starts_with("grok-voice") {
        return XaiModelKind::Realtime;
    }
    if id.contains("multi-agent") {
        return XaiModelKind::ResponsesOnly;
    }
    if id.starts_with("grok-") {
        return XaiModelKind::Chat;
    }
    XaiModelKind::Other
}

/// Map one model object to the common record.
pub fn map_xai_model(model: &XaiModelObject, include_raw: bool) -> GaiseModel {
    let mut out = GaiseModel::new("xai", model.id.clone());
    out.created_at = model.created.map(rfc3339_from_unix);
    if !model.aliases.is_empty() {
        out.notes = Some(format!("aliases: {}", model.aliases.join(", ")));
    }
    out.limits.context_window = model.context_length.filter(|n| *n > 0);
    out.capabilities.add_source(GaiseMetadataSource::Provider);

    let efforts = model
        .capabilities
        .as_ref()
        .map(|c| c.reasoning_effort.clone())
        .unwrap_or_default();
    let caps = &mut out.capabilities;
    if !efforts.is_empty() {
        caps.reasoning = GaiseSupport::Supported;
        caps.reasoning_values = Some(efforts);
    }

    let kind = classify_xai_model_id(&model.id);
    match kind {
        XaiModelKind::Chat | XaiModelKind::ResponsesOnly => {
            caps.add_input(GaiseModality::Text);
            caps.add_input(GaiseModality::Image);
            caps.add_output(GaiseModality::Text);
            if kind == XaiModelKind::Chat {
                caps.add_operation(GaiseOperation::Instruct);
                caps.add_operation(GaiseOperation::InstructStream);
            }
        }
        XaiModelKind::Image => {
            caps.add_input(GaiseModality::Text);
            caps.add_input(GaiseModality::Image);
            caps.add_output(GaiseModality::Image);
        }
        XaiModelKind::Video => {
            caps.add_input(GaiseModality::Text);
            caps.add_input(GaiseModality::Image);
            caps.add_output(GaiseModality::Video);
        }
        XaiModelKind::Realtime => {
            caps.add_input(GaiseModality::Text);
            caps.add_input(GaiseModality::Audio);
            caps.add_output(GaiseModality::Text);
            caps.add_output(GaiseModality::Audio);
            caps.add_operation(GaiseOperation::Live);
        }
        XaiModelKind::Transcription => {
            caps.add_input(GaiseModality::Audio);
            caps.add_output(GaiseModality::Text);
        }
        XaiModelKind::Other => {}
    }
    if !caps.input.is_empty() {
        caps.add_source(GaiseMetadataSource::Heuristic);
    }

    if include_raw {
        out.raw = serde_json::to_value(model).ok();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = r#"{
      "object": "list",
      "data": [
        {"id": "grok-4.7", "object": "model", "created": 1790000000, "owned_by": "xai",
         "aliases": [], "context_length": 500000,
         "capabilities": {"reasoning_effort": ["low", "medium", "high", "xhigh"], "default_reasoning_effort": "high"}},
        {"id": "grok-4.20-0309-non-reasoning", "object": "model", "created": 1773000000, "owned_by": "xai",
         "aliases": ["grok-4.20-non-reasoning"], "context_length": 1000000},
        {"id": "grok-4.20-multi-agent-0309", "object": "model", "created": 1773000000, "owned_by": "xai", "aliases": []},
        {"id": "grok-imagine-image-2.0", "object": "model", "created": 1790000000, "owned_by": "xai", "aliases": []},
        {"id": "grok-imagine-video-1.5", "object": "model", "created": 1790000000, "owned_by": "xai", "aliases": []},
        {"id": "grok-voice-3.0", "object": "model", "created": 1790000000, "owned_by": "xai", "aliases": ["grok-voice-latest"]}
      ]
    }"#;

    #[test]
    fn parses_list_and_maps_reported_metadata() {
        let list: XaiModelList = serde_json::from_str(FIXTURE).unwrap();
        let models: Vec<GaiseModel> = list.data.iter().map(|m| map_xai_model(m, false)).collect();

        let grok = &models[0];
        assert_eq!(grok.provider, "xai");
        assert_eq!(grok.limits.context_window, Some(500_000));
        assert_eq!(grok.capabilities.reasoning, GaiseSupport::Supported);
        assert_eq!(
            grok.capabilities.reasoning_values.as_deref(),
            Some(&["low", "medium", "high", "xhigh"].map(String::from)[..])
        );
        assert_eq!(
            grok.capabilities.operations,
            vec![GaiseOperation::Instruct, GaiseOperation::InstructStream]
        );
        assert!(grok.capabilities.input.contains(&GaiseModality::Image));

        let non_reasoning = &models[1];
        assert_eq!(non_reasoning.capabilities.reasoning, GaiseSupport::Unknown);
        assert_eq!(
            non_reasoning.notes.as_deref(),
            Some("aliases: grok-4.20-non-reasoning")
        );

        assert!(
            models[2].capabilities.operations.is_empty(),
            "multi-agent is Responses-only"
        );
        assert_eq!(models[3].capabilities.output, vec![GaiseModality::Image]);
        assert_eq!(models[4].capabilities.output, vec![GaiseModality::Video]);
        assert_eq!(
            models[5].capabilities.operations,
            vec![GaiseOperation::Live]
        );
    }

    #[test]
    fn classification_is_conservative() {
        assert_eq!(classify_xai_model_id("grok-build-0.1"), XaiModelKind::Chat);
        assert_eq!(
            classify_xai_model_id("grok-voice-latest"),
            XaiModelKind::Realtime
        );
        assert_eq!(
            classify_xai_model_id("grok-voice-transcribe-2.0"),
            XaiModelKind::Transcription
        );
        assert_eq!(
            classify_xai_model_id("grok-2-image-1212"),
            XaiModelKind::Image
        );
        assert_eq!(classify_xai_model_id("mystery"), XaiModelKind::Other);
    }
}
