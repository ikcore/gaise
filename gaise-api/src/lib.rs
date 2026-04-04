use std::sync::Arc;
use axum::{
    extract::State,
    http::StatusCode,
    response::{sse::{Event, Sse}, IntoResponse},
    routing::post,
    Json, Router,
};
use futures_util::StreamExt;
use gaise_core::{
    contracts::{GaiseEmbeddingsRequest, GaiseInstructRequest},
    GaiseClient,
};
use gaise_client::GaiseClientService;
use tracing::error;

#[cfg(feature = "live")]
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
#[cfg(feature = "live")]
use gaise_core::{
    GaiseLiveClient,
    contracts::{GaiseLiveConfig, GaiseLiveEvent, GaiseLiveInput},
};

pub struct AppState {
    pub client_service: GaiseClientService,
}

pub fn create_app(state: Arc<AppState>) -> Router {
    let router = Router::new()
        .route("/v1/instruct", post(handle_instruct))
        .route("/v1/instruct/stream", post(handle_instruct_stream))
        .route("/v1/embeddings", post(handle_embeddings));

    #[cfg(feature = "live")]
    let router = router.route("/v1/live", axum::routing::get(handle_live_ws));

    router.with_state(state)
}

async fn handle_instruct(
    State(state): State<Arc<AppState>>,
    Json(request): Json<GaiseInstructRequest>,
) -> impl IntoResponse {
    match state.client_service.instruct(&request).await {
        Ok(response) => Json(response).into_response(),
        Err(e) => {
            error!("Instruct error: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}

async fn handle_instruct_stream(
    State(state): State<Arc<AppState>>,
    Json(request): Json<GaiseInstructRequest>,
) -> impl IntoResponse {
    match state.client_service.instruct_stream(&request).await {
        Ok(stream) => {
            let sse_stream = stream.map(|item| {
                match item {
                    Ok(chunk) => {
                        Event::default().json_data(chunk)
                    }
                    Err(e) => {
                        Ok(Event::default().event("error").data(e.to_string()))
                    }
                }
            });
            Sse::new(sse_stream).into_response()
        }
        Err(e) => {
            error!("Instruct stream error: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}

async fn handle_embeddings(
    State(state): State<Arc<AppState>>,
    Json(request): Json<GaiseEmbeddingsRequest>,
) -> impl IntoResponse {
    match state.client_service.embeddings(&request).await {
        Ok(response) => Json(response).into_response(),
        Err(e) => {
            error!("Embeddings error: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}

// ── Live WebSocket endpoint ─────────────────────────────────────────

#[cfg(feature = "live")]
async fn handle_live_ws(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_live_session(socket, state))
}

#[cfg(feature = "live")]
async fn handle_live_session(socket: WebSocket, state: Arc<AppState>) {
    use futures_util::SinkExt;

    let (mut ws_sink, mut ws_source) = socket.split();

    // First message must be a config message
    let config: GaiseLiveConfig = loop {
        match ws_source.next().await {
            Some(Ok(Message::Text(text))) => {
                match serde_json::from_str::<GaiseLiveConfig>(&text) {
                    Ok(cfg) => break cfg,
                    Err(e) => {
                        let err = serde_json::json!({"type": "error", "message": format!("Invalid config: {}", e)});
                        let _ = ws_sink.send(Message::Text(err.to_string().into())).await;
                        return;
                    }
                }
            }
            Some(Ok(Message::Close(_))) | None => return,
            _ => continue,
        }
    };

    // Connect to provider
    let session = match state.client_service.live_connect(&config).await {
        Ok(s) => s,
        Err(e) => {
            let err = serde_json::json!({"type": "error", "message": format!("Connect error: {}", e)});
            let _ = ws_sink.send(Message::Text(err.to_string().into())).await;
            return;
        }
    };

    let input_tx = session.tx;
    let mut event_rx = session.rx;

    // Forward provider events → client WebSocket
    let send_handle = tokio::spawn(async move {
        while let Some(event) = event_rx.next().await {
            let msg = match event {
                Ok(GaiseLiveEvent::Audio { data, sample_rate }) => {
                    // Send audio as binary frames
                    // Prepend 4-byte sample rate header for client to know the rate
                    let mut frame = Vec::with_capacity(4 + data.len());
                    frame.extend_from_slice(&sample_rate.to_le_bytes());
                    frame.extend_from_slice(&data);
                    Message::Binary(frame.into())
                }
                Ok(event) => {
                    match serde_json::to_string(&event) {
                        Ok(json) => Message::Text(json.into()),
                        Err(_) => continue,
                    }
                }
                Err(e) => {
                    let err = serde_json::json!({"type": "error", "message": e.to_string()});
                    Message::Text(err.to_string().into())
                }
            };
            if ws_sink.send(msg).await.is_err() {
                break;
            }
        }
    });

    // Forward client WebSocket → provider input channel
    while let Some(msg) = ws_source.next().await {
        let msg = match msg {
            Ok(m) => m,
            Err(_) => break,
        };

        let input = match msg {
            Message::Text(text) => {
                match serde_json::from_str::<GaiseLiveInput>(&text) {
                    Ok(input) => input,
                    Err(_) => continue,
                }
            }
            Message::Binary(data) => {
                // Binary frames are raw PCM audio (16kHz PCM16 by default)
                GaiseLiveInput::Audio {
                    data: data.to_vec(),
                    sample_rate: 16000,
                }
            }
            Message::Close(_) => {
                let _ = input_tx.send(GaiseLiveInput::Close).await;
                break;
            }
            _ => continue,
        };

        if input_tx.send(input).await.is_err() {
            break;
        }
    }

    // Cleanup
    let _ = input_tx.send(GaiseLiveInput::Close).await;
    send_handle.abort();
}
