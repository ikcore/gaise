//! `/v1/transcriptions`, `/v1/images`, and `/v1/videos` through a fake
//! provider registered on the router; no provider traffic.

use async_trait::async_trait;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use futures_util::Stream;
use gaise_api::{AppState, create_app};
use gaise_client::{GaiseClientConfig, GaiseClientService};
use gaise_core::{GaiseClient, contracts::*, logging::IGaiseLogger};
use serde_json::{Value, json};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use tower::ServiceExt;

type BoxErr = Box<dyn std::error::Error + Send + Sync>;

struct FakeMedia;

#[async_trait]
impl GaiseClient for FakeMedia {
    async fn instruct_stream(
        &self,
        _: &GaiseInstructRequest,
    ) -> Result<
        Pin<Box<dyn Stream<Item = Result<GaiseInstructStreamResponse, BoxErr>> + Send>>,
        BoxErr,
    > {
        Err("unused".into())
    }
    async fn instruct(&self, _: &GaiseInstructRequest) -> Result<GaiseInstructResponse, BoxErr> {
        Err("unused".into())
    }
    async fn embeddings(
        &self,
        _: &GaiseEmbeddingsRequest,
    ) -> Result<GaiseEmbeddingsResponse, BoxErr> {
        Err("unused".into())
    }
    async fn transcribe(
        &self,
        request: &GaiseTranscriptionRequest,
    ) -> Result<GaiseTranscriptionResponse, BoxErr> {
        assert_eq!(
            request.model, "scribe",
            "the router strips the provider prefix"
        );
        assert!(request.connection.is_none());
        Ok(GaiseTranscriptionResponse {
            text: "hello there".into(),
            language: Some("en".into()),
            words: vec![GaiseTranscriptWord {
                text: "hello".into(),
                speaker: Some("A".into()),
                ..Default::default()
            }],
            ..Default::default()
        })
    }
    async fn generate_images(
        &self,
        request: &GaiseImageRequest,
    ) -> Result<GaiseImageResponse, BoxErr> {
        Ok(GaiseImageResponse {
            images: vec![GaiseGeneratedMedia {
                data: Some(vec![1, 2, 3]),
                mime_type: Some("image/png".into()),
                ..Default::default()
            }],
            text: Some(request.prompt.clone()),
            ..Default::default()
        })
    }
    async fn start_video(&self, request: &GaiseVideoRequest) -> Result<GaiseVideoJob, BoxErr> {
        assert_eq!(request.model, "clip");
        Ok(GaiseVideoJob {
            id: "projects/p/operations/op-1".into(),
            model: request.model.clone(),
            ..Default::default()
        })
    }
    async fn get_video(&self, request: &GaiseVideoJobRequest) -> Result<GaiseVideoJob, BoxErr> {
        assert_eq!(request.model, "clip");
        Ok(GaiseVideoJob {
            id: request.id.clone(),
            model: request.model.clone(),
            status: GaiseJobStatus::Completed,
            progress: Some(100),
            videos: vec![GaiseGeneratedMedia {
                url: Some("gs://bucket/out.mp4".into()),
                ..Default::default()
            }],
            ..Default::default()
        })
    }
}

#[derive(Debug, Default)]
struct Logger(Mutex<Vec<Value>>);
impl IGaiseLogger for Logger {
    fn log_request(&self, _: Option<&str>, _: &str, _: &str, value: Value) {
        self.0.lock().unwrap().push(value);
    }
    fn log_response(&self, _: Option<&str>, _: &str, _: &str, _: Value, _: Option<Value>) {}
    fn log_stream_chunk(&self, _: Option<&str>, _: &str, _: &str, _: Value) {}
}

async fn app_with_fake() -> (axum::Router, Arc<Logger>) {
    let logger = Arc::new(Logger::default());
    let service = GaiseClientService::new(GaiseClientConfig {
        logger: Some(logger.clone()),
        ..Default::default()
    });
    service.add_client("fake", Arc::new(FakeMedia)).await;
    (
        create_app(Arc::new(AppState {
            client_service: service,
        })),
        logger,
    )
}

async fn send(app: &axum::Router, request: Request<Body>) -> (StatusCode, Value) {
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let value = serde_json::from_slice(&bytes)
        .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into()));
    (status, value)
}

fn post(path: &str, body: Value) -> Request<Body> {
    Request::post(path)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

#[tokio::test]
async fn transcription_route_and_summarised_logging() {
    let (app, logger) = app_with_fake().await;
    let (status, body) = send(
        &app,
        post(
            "/v1/transcriptions",
            json!({"model": "fake::scribe", "audio": {"type": "bytes", "data": [1, 2, 3, 4], "mime_type": "audio/wav"}, "diarize": true}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["text"], "hello there");
    assert_eq!(body["words"][0]["speaker"], "A");
    // The request log carries the audio size, never the bytes.
    let logged = logger.0.lock().unwrap()[0].clone();
    assert_eq!(logged["audio"]["bytes"], 4);
    assert!(logged.get("data").is_none());

    let (status, _) = send(
        &app,
        post("/v1/transcriptions", json!({"model": "fake::scribe"})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "missing audio");
}

#[tokio::test]
async fn image_route() {
    let (app, _) = app_with_fake().await;
    let (status, body) = send(
        &app,
        post(
            "/v1/images",
            json!({"model": "fake::img", "prompt": "a lighthouse", "aspect_ratio": "16:9"}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["images"][0]["data"], json!([1, 2, 3]));
    assert_eq!(body["text"], "a lighthouse");
    let (status, _) = send(
        &app,
        post("/v1/images", json!({"model": "fake::img", "prompt": " "})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn video_start_and_poll_with_encoded_job_id() {
    let (app, _) = app_with_fake().await;
    let (status, job) = send(
        &app,
        post(
            "/v1/videos",
            json!({"model": "fake::clip", "prompt": "waves"}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(job["status"], "queued");
    assert_eq!(job["model"], "fake::clip", "jobs report the routable model");
    let id = job["id"].as_str().unwrap().replace('/', "%2F");
    let (status, polled) = send(
        &app,
        Request::get(format!("/v1/videos/{id}?model=fake::clip"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(polled["id"], "projects/p/operations/op-1");
    assert_eq!(polled["status"], "completed");
    assert_eq!(polled["videos"][0]["url"], "gs://bucket/out.mp4");
}

#[tokio::test]
async fn providers_without_media_support_return_errors() {
    let service = GaiseClientService::new(GaiseClientConfig {
        ollama_url: Some("http://127.0.0.1:9".into()),
        ..Default::default()
    });
    let app = create_app(Arc::new(AppState {
        client_service: service,
    }));
    let (status, body) = send(
        &app,
        post(
            "/v1/images",
            json!({"model": "ollama::flux", "prompt": "x"}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert!(body.as_str().unwrap().contains("not supported"), "{body}");
}
