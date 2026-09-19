//! HTTP mapping audited against https://docs.typesafe.ai/api and the official
//! TypeScript SDK on 2026-09-19. Routing metadata never leaves GAISe.
use gaise_core::contracts::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

#[derive(Serialize)]
pub struct SystemOneRequest<'a> {
    pub model: &'a str,
    pub state: &'a serde_json::Value,
    pub questions: &'a BTreeMap<String, GaiseQuestion>,
}

impl<'a> From<&'a GaiseSystemOneRequest> for SystemOneRequest<'a> {
    fn from(request: &'a GaiseSystemOneRequest) -> Self {
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

impl From<SystemOneResponse> for GaiseSystemOneResponse {
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
    // This adapter maps every model on the dedicated System One surface.
    model.capabilities.operations = vec![GaiseOperation::SystemOne];
    if include_raw {
        model.raw = Some(raw);
    }
    Ok(model)
}
