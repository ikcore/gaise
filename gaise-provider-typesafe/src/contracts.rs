//! HTTP mapping audited against https://docs.typesafe.ai/api and the official
//! TypeScript SDK on 2026-09-19. Routing metadata never leaves GAISe.
use base64::Engine;
use gaise_core::contracts::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

#[derive(Serialize)]
pub struct SystemOneRequest<'a> {
    pub model: &'a str,
    pub state: &'a serde_json::Value,
    pub questions: &'a BTreeMap<String, GaiseQuestion>,
}

impl<'a> From<&'a GaiseDecisionRequest> for SystemOneRequest<'a> {
    fn from(request: &'a GaiseDecisionRequest) -> Self {
        Self {
            model: if request.model == "jev" {
                "jev-latest"
            } else {
                &request.model
            },
            state: &request.state,
            questions: &request.questions,
        }
    }
}

/// One entry of System One's top-level `assets` map (TypeSafe, 2026-10-14):
/// a PNG or JPEG image sent as standard base64, which replaces every
/// occurrence of its reference string in the state, instructions, or criteria.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ImageAsset {
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub format: &'static str,
    pub data_base64: String,
}

impl ImageAsset {
    /// Encodes `GaiseContent::Image` for TypeSafe. The format comes from the
    /// caller (`png`, `jpg`, `jpeg`, or the matching MIME type) or, when
    /// absent, from the file signature; anything but PNG or JPEG is rejected.
    pub fn from_content(content: &GaiseContent) -> Result<Self, String> {
        let GaiseContent::Image { data, format } = content else {
            return Err("TypeSafe assets accept only image content".into());
        };
        let declared = format
            .as_deref()
            .map(|f| f.trim().to_ascii_lowercase())
            .filter(|f| !f.is_empty());
        let format = match declared.as_deref() {
            Some("png" | "image/png") => "png",
            Some("jpg" | "jpeg" | "image/jpg" | "image/jpeg") => "jpg",
            Some(other) => {
                return Err(format!(
                    "TypeSafe accepts PNG or JPEG images only, not {other}"
                ));
            }
            None if data.starts_with(&[0x89, b'P', b'N', b'G']) => "png",
            None if data.starts_with(&[0xFF, 0xD8, 0xFF]) => "jpg",
            None => return Err("TypeSafe accepts PNG or JPEG images only".into()),
        };
        Ok(Self {
            kind: "image",
            format,
            data_base64: base64::prelude::BASE64_STANDARD.encode(data),
        })
    }
}

#[derive(Deserialize)]
pub struct SystemOneResponse {
    pub model: String,
    pub answers: BTreeMap<String, GaiseAnswer>,
    pub usage: Usage,
}

#[derive(Deserialize)]
pub struct Usage {
    pub input_tokens: usize,
    pub output_tokens: usize,
}

impl From<SystemOneResponse> for GaiseDecisionResponse {
    fn from(response: SystemOneResponse) -> Self {
        Self {
            model: response.model,
            answers: response.answers,
            usage: Some(GaiseUsage {
                input: Some(HashMap::from([(
                    "input_tokens".into(),
                    response.usage.input_tokens,
                )])),
                output: Some(HashMap::from([(
                    "output_tokens".into(),
                    response.usage.output_tokens,
                )])),
                total: None,
            }),
        }
    }
}

#[derive(Deserialize)]
pub struct ModelsResponse {
    pub models: Vec<serde_json::Value>,
}

#[derive(Deserialize)]
struct ModelCard {
    name: String,
    description: String,
    release_date: String,
}

pub fn map_model(
    raw: serde_json::Value,
    include_raw: bool,
) -> Result<GaiseModel, serde_json::Error> {
    let card: ModelCard = serde_json::from_value(raw.clone())?;
    let id = if card.name == "jev-latest" {
        "jev".to_string()
    } else {
        card.name
    };
    let mut model = GaiseModel::new("typesafe", id);
    model.description = Some(card.description);
    model.created_at = Some(if card.release_date.len() == 10 {
        format!("{}T00:00:00Z", card.release_date)
    } else {
        card.release_date
    });
    // Every TypeSafe model is served by System One, GAISe's decision operation.
    model.capabilities.operations = vec![GaiseOperation::Decision];
    if include_raw {
        model.raw = Some(raw);
    }
    Ok(model)
}
