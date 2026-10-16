use crate::contracts::{
    ImageAsset, ModelsResponse, SystemOneRequest, SystemOneResponse, map_model,
};
use async_trait::async_trait;
use futures_util::Stream;
use gaise_core::{GaiseClient, contracts::*};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    hash::{BuildHasher, Hasher},
    pin::Pin,
    time::Duration,
};

pub const DEFAULT_API_URL: &str = "https://api.typesafe.ai";
/// Opt-in header for image assets and Box questions (TypeSafe, 2026-10-14).
pub const IMAGES_HEADER: &str = "TypeSafe-Danger-Zone-Images";
/// Opt-in header that raises Jev's context to 262k tokens per request and per
/// question (TypeSafe, 2026-10-14).
pub const LONG_CONTEXT_HEADER: &str = "TypeSafe-Danger-Zone-Long-Context";
/// TypeSafe's limit on one asset reference, in UTF-8 bytes.
const MAX_REFERENCE_BYTES: usize = 1024;
type BoxError = Box<dyn std::error::Error + Send + Sync>;

pub struct GaiseClientTypeSafe {
    api_url: String,
    api_key: String,
    client: reqwest::Client,
    long_context: bool,
}

impl GaiseClientTypeSafe {
    /// `api_url` is the API root (without `/v1`), just like TypeSafe's baseURL.
    pub fn new(api_url: String, api_key: String) -> Self {
        Self {
            api_url: api_url.trim_end_matches('/').into(),
            api_key,
            client: reqwest::Client::new(),
            long_context: false,
        }
    }

    /// Sends `TypeSafe-Danger-Zone-Long-Context: true` with every decision,
    /// which TypeSafe documents as slower and less accurate. Off by default.
    pub fn with_long_context(mut self, enabled: bool) -> Self {
        self.long_context = enabled;
        self
    }

    /// System One with caller-placed images. Each key of `assets` is a
    /// reference string that must appear in the request's state, question
    /// instructions, or criteria (TypeSafe returns 400 otherwise); TypeSafe
    /// puts the image wherever its reference appears. Images are PNG or JPEG,
    /// sent as base64 with the `TypeSafe-Danger-Zone-Images: true` header.
    /// TypeSafe labels images Dreamer-ready, so their contract may change.
    /// An empty map sends a plain decision.
    pub async fn decision_with_assets(
        &self,
        request: &GaiseDecisionRequest,
        assets: &BTreeMap<String, GaiseContent>,
    ) -> Result<GaiseDecisionResponse, BoxError> {
        request.validate()?;
        let mut body = serde_json::to_value(SystemOneRequest::from(request))?;
        let mut headers = Vec::new();
        if !assets.is_empty() {
            let mut encoded = serde_json::Map::new();
            for (reference, content) in assets {
                if reference.trim().is_empty() || reference.len() > MAX_REFERENCE_BYTES {
                    return Err(format!(
                        "TypeSafe asset references must be non-blank and at most {MAX_REFERENCE_BYTES} bytes"
                    )
                    .into());
                }
                let asset = ImageAsset::from_content(content)?;
                encoded.insert(reference.clone(), serde_json::to_value(asset)?);
            }
            body["assets"] = Value::Object(encoded);
            headers.push((IMAGES_HEADER, "true"));
        }
        if self.long_context {
            headers.push((LONG_CONTEXT_HEADER, "true"));
        }
        let response: SystemOneResponse = self
            .send(reqwest::Method::POST, "/v1/systemone", Some(body), &headers)
            .await?;
        request
            .check_answers(&response.answers)
            .map_err(|e| format!("TypeSafe response {e}"))?;
        Ok(response.into())
    }

    async fn send<T: serde::de::DeserializeOwned>(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<serde_json::Value>,
        headers: &[(&str, &str)],
    ) -> Result<T, BoxError> {
        for attempt in 0..=2 {
            let mut request = self
                .client
                .request(method.clone(), format!("{}{path}", self.api_url))
                .bearer_auth(&self.api_key)
                .header(reqwest::header::ACCEPT, "application/json")
                .timeout(Duration::from_secs(30));
            for (name, value) in headers {
                request = request.header(*name, *value);
            }
            if let Some(body) = &body {
                request = request.json(body);
            }
            let response = request.send().await?;
            let status = response.status();
            if status.is_success() {
                return Ok(response.json().await?);
            }
            let retry_after = response
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok());
            let retry = status.as_u16() == 429 || status.is_server_error();
            let mut detail = response.text().await?;
            if !self.api_key.is_empty() {
                detail = detail.replace(&self.api_key, "***");
            }
            if retry && attempt < 2 {
                let delay = retry_after
                    .map(|s| Duration::from_secs(s.min(60)))
                    .unwrap_or_else(|| Duration::from_millis(500 * (1 << attempt)));
                tokio::time::sleep(delay).await;
                continue;
            }
            return Err(format!("TypeSafe API error ({status}): {detail}").into());
        }
        unreachable!()
    }
}

#[async_trait]
impl GaiseClient for GaiseClientTypeSafe {
    async fn decision(
        &self,
        request: &GaiseDecisionRequest,
    ) -> Result<GaiseDecisionResponse, BoxError> {
        self.decision_with_assets(request, &BTreeMap::new()).await
    }

    /// Images shared by all questions, like Ollama's System One `images`.
    /// GAISe gives each image a fresh reference and lists the references
    /// under `images` in the state, so questions can name `images` or
    /// `images[0]`: an object or null state gains an `images` field, and any
    /// other state (or one that already has `images`) is wrapped as
    /// `{"state": ..., "images": [...]}`. Use
    /// [`GaiseClientTypeSafe::decision_with_assets`] to place images yourself.
    async fn decision_with_images(
        &self,
        request: &GaiseDecisionRequest,
        images: &[GaiseContent],
    ) -> Result<GaiseDecisionResponse, BoxError> {
        if images.is_empty() {
            return self.decision(request).await;
        }
        let nonce = image_reference_nonce();
        let assets: BTreeMap<String, GaiseContent> = images
            .iter()
            .enumerate()
            .map(|(i, image)| (format!("gaise-image-{nonce:016x}-{i}"), image.clone()))
            .collect();
        let mut request = request.clone();
        request.state = state_with_images(&request.state, assets.keys().cloned().collect());
        self.decision_with_assets(&request, &assets).await
    }

    async fn list_models(
        &self,
        request: &GaiseListModelsRequest,
    ) -> Result<GaiseListModelsResponse, BoxError> {
        let response: ModelsResponse = self
            .send(reqwest::Method::GET, "/v1/models", None, &[])
            .await?;
        let models = response
            .models
            .into_iter()
            .map(|m| map_model(m, request.include_raw))
            .collect::<Result<Vec<_>, _>>()?;
        let mut response = GaiseListModelsResponse::from_models(models);
        response.retain_operation(request.operation);
        Ok(response)
    }

    async fn instruct(&self, _: &GaiseInstructRequest) -> Result<GaiseInstructResponse, BoxError> {
        Err("TypeSafe does not support instruct; use decision with typed questions".into())
    }
    async fn instruct_stream(
        &self,
        _: &GaiseInstructRequest,
    ) -> Result<
        Pin<Box<dyn Stream<Item = Result<GaiseInstructStreamResponse, BoxError>> + Send>>,
        BoxError,
    > {
        Err("TypeSafe does not support streaming; use decision with typed questions".into())
    }
    async fn embeddings(
        &self,
        _: &GaiseEmbeddingsRequest,
    ) -> Result<GaiseEmbeddingsResponse, BoxError> {
        Err("TypeSafe does not support embeddings; use decision with typed questions".into())
    }
}

/// A per-request random value so image references cannot collide with text.
fn image_reference_nonce() -> u64 {
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write_u128(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default(),
    );
    hasher.finish()
}

fn state_with_images(state: &Value, references: Vec<String>) -> Value {
    match state {
        Value::Object(fields) if !fields.contains_key("images") => {
            let mut fields = fields.clone();
            fields.insert("images".into(), references.into());
            Value::Object(fields)
        }
        Value::Null => serde_json::json!({"images": references}),
        other => serde_json::json!({"state": other, "images": references}),
    }
}
