use crate::contracts::{ModelsResponse, SystemOneRequest, SystemOneResponse, map_model};
use async_trait::async_trait;
use futures_util::Stream;
use gaise_core::{GaiseClient, contracts::*};
use std::{pin::Pin, time::Duration};

pub const DEFAULT_API_URL: &str = "https://api.typesafe.ai";
type BoxError = Box<dyn std::error::Error + Send + Sync>;

pub struct GaiseClientTypeSafe {
    api_url: String,
    api_key: String,
    client: reqwest::Client,
}

impl GaiseClientTypeSafe {
    /// `api_url` is the API root (without `/v1`), just like TypeSafe's baseURL.
    pub fn new(api_url: String, api_key: String) -> Self {
        Self {
            api_url: api_url.trim_end_matches('/').into(),
            api_key,
            client: reqwest::Client::new(),
        }
    }

    async fn send<T: serde::de::DeserializeOwned>(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> Result<T, BoxError> {
        for attempt in 0..=2 {
            let mut request = self
                .client
                .request(method.clone(), format!("{}{path}", self.api_url))
                .bearer_auth(&self.api_key)
                .header(reqwest::header::ACCEPT, "application/json")
                .timeout(Duration::from_secs(30));
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
    async fn system_one(
        &self,
        request: &GaiseSystemOneRequest,
    ) -> Result<GaiseSystemOneResponse, BoxError> {
        request.validate()?;
        let response: SystemOneResponse = self
            .send(
                reqwest::Method::POST,
                "/v1/systemone",
                Some(serde_json::to_value(SystemOneRequest::from(request))?),
            )
            .await?;
        if response.answers.len() != request.questions.len()
            || request.questions.iter().any(|(id, q)| {
                !matches!(
                    (q, response.answers.get(id)),
                    (GaiseQuestion::Noul { .. }, Some(GaiseAnswer::Noul { .. }))
                        | (
                            GaiseQuestion::Choice { .. },
                            Some(GaiseAnswer::Choice { .. })
                        )
                        | (GaiseQuestion::Score { .. }, Some(GaiseAnswer::Score { .. }))
                )
            })
        {
            return Err("TypeSafe response answer IDs or types do not match the questions".into());
        }
        Ok(response.into())
    }

    async fn list_models(
        &self,
        request: &GaiseListModelsRequest,
    ) -> Result<GaiseListModelsResponse, BoxError> {
        let response: ModelsResponse = self.send(reqwest::Method::GET, "/v1/models", None).await?;
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
        Err("TypeSafe does not support instruct; use system_one with typed questions".into())
    }
    async fn instruct_stream(
        &self,
        _: &GaiseInstructRequest,
    ) -> Result<
        Pin<Box<dyn Stream<Item = Result<GaiseInstructStreamResponse, BoxError>> + Send>>,
        BoxError,
    > {
        Err("TypeSafe does not support streaming; use system_one with typed questions".into())
    }
    async fn embeddings(
        &self,
        _: &GaiseEmbeddingsRequest,
    ) -> Result<GaiseEmbeddingsResponse, BoxError> {
        Err("TypeSafe does not support embeddings; use system_one with typed questions".into())
    }
}
