use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures_util::StreamExt;
use gaise_core::{
    GaiseClient,
    contracts::{
        GaiseEmbeddingsRequest,
        GaiseEmbeddingsResponse,
        //GaiseRepository,
        GaiseInstructRequest,
        GaiseInstructResponse,
        GaiseInstructStreamResponse
    }
};
use super::contracts::google_claims::GoogleClaims;
use crate::contracts::{GoogleAccessToken, GoogleChatCompletionResponse, GoogleInstructRequest};
use crate::contracts::models::{GoogleEmbeddingsResponse, GoogleEmbeddingsRequest};
use super::contracts::ServiceAccount;

// const SERVICE_PROVIDER_ID: &str = "google";

#[derive(Clone)]pub struct GaiseClientVertexAI {
    pub account: ServiceAccount,
    pub api_url: String,
    token_state: Arc<tokio::sync::Mutex<TokenState>>,
}

struct TokenState {
    access_token: String,
    token_type: String,
    expires_at: Option<DateTime<Utc>>,
}

impl GaiseClientVertexAI {
    pub async fn new(sa: &ServiceAccount, api_url: String) -> Self {
        Self {
            account: sa.clone(),
            api_url,
            token_state: Arc::new(tokio::sync::Mutex::new(TokenState {
                access_token: String::new(),
                token_type: "Bearer".to_string(),
                expires_at: None,
            })),
        }
    }

    pub async fn get_token(
        &self,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let refresh_skew = Duration::from_secs(5 * 60);
        let now = Utc::now();

        let mut state = self.token_state.lock().await;
        let needs_refresh = match state.expires_at {
            None => state.access_token.is_empty(),
            Some(exp) => state.access_token.is_empty() || (now + refresh_skew) >= exp,
        };
        if needs_refresh {
            println!("Google: Refreshing access token (lazy)");

            let response = self.fetch_new_token().await?;
            let expires_at = now + Duration::from_secs(response.expires_in as u64);

            state.access_token = response.access_token;
            state.token_type = response.token_type;
            state.expires_at = Some(expires_at);

            println!("Google: New access token created; expires at {}", expires_at);
        }
        Ok(state.access_token.clone())
    }

    pub async fn fetch_new_token(
        &self,
    ) -> Result<GoogleAccessToken, Box<dyn std::error::Error + Send + Sync>> {
        println!("Google: Creating new access token");

        let now = Utc::now();
        let claims = GoogleClaims {
            iss: self.account.client_email.to_string(),
            scope: "https://www.googleapis.com/auth/cloud-platform".to_owned(),
            aud: "https://oauth2.googleapis.com/token".to_owned(),
            iat: now.timestamp(),
            exp: (now + std::time::Duration::from_secs(60 * 60)).timestamp(),
        };
        let jwt = jsonwebtoken::encode(
            &jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256),
            &claims,
            &jsonwebtoken::EncodingKey::from_rsa_pem(self.account.private_key.as_bytes())?,
        )?;
        let params = [
            ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
            ("assertion", &jwt),
        ];
        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .build()?;
        let res = client
            .post("https://oauth2.googleapis.com/token")
            .form(&params)
            .send()
            .await?;

        let body = res.text().await?;
        let response: GoogleAccessToken = serde_json::from_str(&body)?;
        Ok(response)
    }

    pub async fn get_auth_header_value(
        &self,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let token = self.get_token().await?;
        Ok(format!("Bearer {}", token))
    }
}

#[async_trait]
impl GaiseClient for GaiseClientVertexAI {
    async fn instruct_stream(
        &self,
        request: &GaiseInstructRequest,
    ) -> Result<
        std::pin::Pin<
            Box<
                dyn futures_util::Stream<
                        Item = Result<GaiseInstructStreamResponse, Box<dyn std::error::Error + Send + Sync>>,
                    > + Send,
            >,
        >,
        Box<dyn std::error::Error + Send + Sync>,
    > {
        let url = self.api_url.replace("{{MODEL}}", &request.model) + ":streamGenerateContent?alt=sse";
        let json = serde_json::to_string(&GoogleInstructRequest::from(request))?;

        let token = self
            .get_token()
            .await
            .map_err(|e| format!("no google access token: {e}"))?;

        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .build()?;
        let res = client
            .post(&url)
            .header("Authorization", "Bearer ".to_owned() + &token)
            .header("Content-type", "application/json")
            .body(json)
            .send()
            .await?;

        if !res.status().is_success() {
            let err_text = res.text().await.unwrap_or_default();
            return Err(format!("Vertex AI error: {}", err_text).into());
        }

        let stream = res.bytes_stream();

        // Buffer SSE data across TCP chunks — a single `data: {...}` line
        // can be split across multiple chunks.
        let buffered_stream = futures_util::stream::unfold(
            (stream, String::new()),
            |(mut stream, mut buf)| async move {
                use futures_util::StreamExt as _;
                loop {
                    // Try to extract complete lines from the buffer
                    let mut results = Vec::new();
                    while let Some(newline_pos) = buf.find('\n') {
                        let line = buf[..newline_pos].trim_end_matches('\r').to_string();
                        buf = buf[newline_pos + 1..].to_string();

                        if let Some(json_str) = line.strip_prefix("data: ") {
                            match serde_json::from_str::<GoogleChatCompletionResponse>(json_str) {
                                Ok(response) => {
                                    for r in response.to_stream_view() {
                                        results.push(Ok(r));
                                    }
                                }
                                Err(e) => {
                                    eprintln!("[vertexai-stream] parse failed: {e}; json preview: {}...", &json_str[..json_str.len().min(200)]);
                                    results.push(Err(Box::new(e) as Box<dyn std::error::Error + Send + Sync>));
                                }
                            }
                        }
                    }

                    if !results.is_empty() {
                        return Some((futures_util::stream::iter(results), (stream, buf)));
                    }

                    // Need more data
                    match stream.next().await {
                        Some(Ok(chunk)) => {
                            let text = String::from_utf8_lossy(&chunk);
                            buf.push_str(&text);
                        }
                        Some(Err(e)) => {
                            let err: Box<dyn std::error::Error + Send + Sync> = Box::new(e);
                            return Some((
                                futures_util::stream::iter(vec![Err(err)]),
                                (stream, buf),
                            ));
                        }
                        None => {
                            // Stream ended — flush any remaining buffered data
                            let trimmed = buf.trim();
                            if let Some(json_str) = trimmed.strip_prefix("data: ") {
                                if let Ok(response) = serde_json::from_str::<GoogleChatCompletionResponse>(json_str) {
                                    let results: Vec<Result<GaiseInstructStreamResponse, Box<dyn std::error::Error + Send + Sync>>> =
                                        response.to_stream_view().into_iter().map(Ok).collect();
                                    if !results.is_empty() {
                                        return Some((futures_util::stream::iter(results), (stream, String::new())));
                                    }
                                }
                            }
                            return None;
                        }
                    }
                }
            },
        )
        .flatten();

        Ok(Box::pin(buffered_stream))
    }

    async fn instruct(&self, request:&GaiseInstructRequest) -> Result<GaiseInstructResponse, Box<dyn std::error::Error + Send + Sync>> {

        let url = self.api_url.replace("{{MODEL}}", &request.model) + ":generateContent";
        let json = serde_json::to_string(&GoogleInstructRequest::from(request))?;     

       let token = self
            .get_token()
            .await
            .map_err(|e| format!("no google access token: {e}"))?;

        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .build()?;
        let res = client.post(&url)
            .header("Authorization", "Bearer ".to_owned() + &token)
            .header("Content-type", "application/json")
            .body(json)
            .send()
            .await
            .expect("failed to get response");

        let res_json = res.text().await.expect("failed to get payload");

        println!("{}", res_json);
        let response:GoogleChatCompletionResponse = serde_json::from_str(&res_json)?;
        let response_view = response.to_view();

        Ok(response_view)
    }

    async fn embeddings(&self, request:&GaiseEmbeddingsRequest) -> Result<GaiseEmbeddingsResponse, Box<dyn std::error::Error + Send + Sync>> {
        
        let url = self.api_url.replace("{{MODEL}}", &request.model) + ":predict";
        let json = serde_json::to_string(&GoogleEmbeddingsRequest::from(request))?;

        let token = self
            .get_token()
            .await
            .map_err(|e| format!("no google access token: {e}"))?;

        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .build()?;
        let res = client.post(&url)
            .header("Authorization", "Bearer ".to_owned() + &token)
            .header("Content-type", "application/json")
            .body(json)
            .send()
            .await
            .map_err(|e| format!("embeddings request failed: {e}"))?;

        let res_json = res.text().await
            .map_err(|e| format!("embeddings response body read failed: {e}"))?;
        let response: GoogleEmbeddingsResponse = serde_json::from_str(&res_json)
            .map_err(|e| format!("embeddings response parse failed: {e} — body: {}", &res_json[..res_json.len().min(500)]))?;
        let response_view = response.to_view();

        Ok(response_view)
    }
}

