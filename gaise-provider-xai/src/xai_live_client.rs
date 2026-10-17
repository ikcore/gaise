//! Grok speech-to-speech over `wss://api.x.ai/v1/realtime`.
//!
//! The protocol is OpenAI Realtime-compatible (`session.update`,
//! `input_audio_buffer.*`, `conversation.item.create`, `response.*`), with
//! xAI's own session fields: top-level `turn_detection`, `reasoning.effort`
//! limited to `high` or `none`, flat function tools, and the `grok-transcribe`
//! input-transcription model.

use async_trait::async_trait;
use base64::Engine;
use futures_util::{SinkExt, StreamExt};
use gaise_core::GaiseLiveClient;
use gaise_core::contracts::{
    GaiseFunctionCall, GaiseLiveConfig, GaiseLiveEvent, GaiseLiveEventStream, GaiseLiveInput,
    GaiseLiveSession, GaiseReasoningEffort, GaiseTool, GaiseToolParameter, GaiseUsage,
};
use serde_json::{Value, json};
use std::collections::HashMap;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// PCM16 sample rate GAISe negotiates for both directions.
pub const LIVE_SAMPLE_RATE: u32 = 24_000;

/// Voice models use `reasoning.effort` `high` (default) or `none`: `none`
/// stays `none`, `auto` omits the field, anything else asks for `high`.
pub fn xai_live_reasoning_effort(effort: &str) -> Option<&'static str> {
    match GaiseReasoningEffort::parse(effort) {
        GaiseReasoningEffort::Auto => None,
        GaiseReasoningEffort::None => Some("none"),
        _ => Some("high"),
    }
}

fn map_tool_parameter(param: &GaiseToolParameter) -> Value {
    let mut obj = serde_json::Map::new();
    if let Some(t) = &param.r#type {
        let mapped = if t == "text" { "string" } else { t.as_str() };
        obj.insert("type".into(), json!(mapped));
    }
    if let Some(desc) = &param.description {
        obj.insert("description".into(), json!(desc));
    }
    if let Some(props) = &param.properties {
        let properties: serde_json::Map<String, Value> = props
            .iter()
            .map(|(k, v)| (k.clone(), map_tool_parameter(v)))
            .collect();
        obj.insert("properties".into(), Value::Object(properties));
    }
    if let Some(items) = &param.items {
        obj.insert("items".into(), map_tool_parameter(items));
    }
    if let Some(req) = &param.required {
        obj.insert("required".into(), json!(req));
    }
    Value::Object(obj)
}

fn build_tools(tools: &[GaiseTool]) -> Vec<Value> {
    tools
        .iter()
        .map(|t| {
            let mut parameters = t
                .parameters
                .as_ref()
                .map(map_tool_parameter)
                .unwrap_or_else(|| json!({}));
            // The root of a function schema must be an object.
            parameters["type"] = json!("object");
            if parameters.get("properties").is_none() {
                parameters["properties"] = json!({});
            }
            let mut tool = json!({"type": "function", "name": t.name, "parameters": parameters});
            if let Some(desc) = &t.description {
                tool["description"] = json!(desc);
            }
            tool
        })
        .collect()
}

/// Build the `session.update` frame for a live config (pure; used by tests).
pub fn build_xai_session_update(config: &GaiseLiveConfig) -> Value {
    let mut session = serde_json::Map::new();
    if let Some(instructions) = &config.system_instruction {
        session.insert("instructions".into(), json!(instructions));
    }
    if let Some(voice) = config.voice.as_ref().filter(|v| !v.trim().is_empty()) {
        session.insert("voice".into(), json!(voice.to_ascii_lowercase()));
    }
    // `null` selects manual turns (commit + response.create).
    let turn_detection = match &config.vad_config {
        Some(vad) if !vad.enabled => Value::Null,
        _ => json!({"type": "server_vad"}),
    };
    session.insert("turn_detection".into(), turn_detection);

    let pcm = json!({"type": "audio/pcm", "rate": LIVE_SAMPLE_RATE});
    let mut input = json!({"format": pcm});
    if config.transcription.as_ref().is_some_and(|t| t.input) {
        input["transcription"] = json!({"model": "grok-transcribe"});
    }
    session.insert(
        "audio".into(),
        json!({"input": input, "output": {"format": pcm}}),
    );

    if let Some(effort) = config
        .generation_config
        .as_ref()
        .and_then(|gc| gc.thinking_effort.as_deref())
        .and_then(xai_live_reasoning_effort)
    {
        session.insert("reasoning".into(), json!({"effort": effort}));
    }
    if let Some(tools) = config.tools.as_ref().filter(|t| !t.is_empty()) {
        session.insert("tools".into(), json!(build_tools(tools)));
    }
    json!({"type": "session.update", "session": Value::Object(session)})
}

/// `response.done` usage, when the server reports it (OpenAI Realtime shape).
pub fn map_xai_live_usage(usage: &Value) -> Option<GaiseUsage> {
    let count = |v: &Value| v.as_u64().map(|n| n as usize);
    let mut input = HashMap::new();
    let mut output = HashMap::new();
    if let Some(n) = count(&usage["input_tokens"]) {
        input.insert("input_tokens".to_string(), n);
    }
    if let Some(n) = count(&usage["output_tokens"]) {
        output.insert("output_tokens".to_string(), n);
    }
    for key in [
        "text_tokens",
        "audio_tokens",
        "cached_tokens",
        "image_tokens",
    ] {
        if let Some(n) = count(&usage["input_token_details"][key]) {
            input.insert(key.to_string(), n);
        }
        if let Some(n) = count(&usage["output_token_details"][key]) {
            output.insert(key.to_string(), n);
        }
    }
    let total = count(&usage["total_tokens"]);
    if input.is_empty() && output.is_empty() && total.is_none() {
        return None;
    }
    Some(GaiseUsage {
        input: (!input.is_empty()).then_some(input),
        output: (!output.is_empty()).then_some(output),
        total: total.map(|n| HashMap::from([("total_tokens".to_string(), n)])),
    })
}

/// Map one server event to GAISe live events (pure; used by tests).
pub fn map_xai_live_event(event: &Value) -> Vec<GaiseLiveEvent> {
    let text = |key: &str| event[key].as_str().map(str::to_string);
    match event["type"].as_str().unwrap_or_default() {
        "response.output_audio.delta" | "response.audio.delta" => text("delta")
            .and_then(|d| base64::prelude::BASE64_STANDARD.decode(d).ok())
            .map(|data| GaiseLiveEvent::Audio {
                data,
                sample_rate: LIVE_SAMPLE_RATE,
            })
            .into_iter()
            .collect(),
        "response.output_text.delta" | "response.text.delta" => text("delta")
            .map(|text| GaiseLiveEvent::Text { text })
            .into_iter()
            .collect(),
        "response.output_audio_transcript.delta" | "response.audio_transcript.delta" => {
            text("delta")
                .map(|text| GaiseLiveEvent::Transcript {
                    role: "assistant".to_string(),
                    text,
                })
                .into_iter()
                .collect()
        }
        // `.updated` carries a cumulative, correctable transcript; only the
        // final `.completed` text is forwarded.
        "conversation.item.input_audio_transcription.completed" => text("transcript")
            .map(|text| GaiseLiveEvent::Transcript {
                role: "user".to_string(),
                text,
            })
            .into_iter()
            .collect(),
        "response.function_call_arguments.done" => match (text("call_id"), text("name")) {
            (Some(id), Some(name)) => vec![GaiseLiveEvent::ToolCall {
                id,
                function: GaiseFunctionCall {
                    name,
                    arguments: text("arguments"),
                },
            }],
            _ => Vec::new(),
        },
        "response.done" => {
            let mut events: Vec<GaiseLiveEvent> = map_xai_live_usage(&event["response"]["usage"])
                .map(GaiseLiveEvent::Usage)
                .into_iter()
                .collect();
            events.push(GaiseLiveEvent::TurnComplete);
            events
        }
        "input_audio_buffer.speech_started" => vec![GaiseLiveEvent::Interrupted],
        "error" => vec![GaiseLiveEvent::Error {
            message: event["error"]["message"]
                .as_str()
                .or(event["message"].as_str())
                .unwrap_or("Unknown xAI realtime error")
                .to_string(),
        }],
        _ => Vec::new(),
    }
}

/// `wss://…/v1/realtime?model=…` from an `https://…` API URL with or without `/v1`.
pub fn realtime_url(api_url: &str, model: &str) -> String {
    let base = api_url
        .trim_end_matches('/')
        .replacen("https://", "wss://", 1)
        .replacen("http://", "ws://", 1);
    let root = if base.ends_with("/v1") {
        base
    } else {
        format!("{base}/v1")
    };
    format!("{root}/realtime?model={model}")
}

pub struct GaiseClientXaiLive {
    api_url: String,
    api_key: String,
}

impl GaiseClientXaiLive {
    pub fn new(api_url: String, api_key: String) -> Self {
        Self { api_url, api_key }
    }
}

type WsSink = futures_util::stream::SplitSink<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    Message,
>;

async fn send_json(sink: &mut WsSink, frames: &[Value]) -> Result<(), BoxError> {
    for frame in frames {
        sink.send(Message::Text(frame.to_string().into())).await?;
    }
    Ok(())
}

/// Read server frames until `wanted` arrives; an `error` frame fails.
async fn wait_for<S>(source: &mut S, wanted: &str) -> Result<(), BoxError>
where
    S: futures_util::Stream<Item = Result<Message, tungstenite::Error>> + Unpin,
{
    while let Some(msg) = source.next().await {
        if let Message::Text(text) = msg? {
            let event: Value = serde_json::from_str(&text)?;
            match event["type"].as_str() {
                Some(t) if t == wanted => return Ok(()),
                Some("error") => {
                    let message = event["error"]["message"]
                        .as_str()
                        .unwrap_or("xAI realtime session setup failed");
                    return Err(std::io::Error::other(message.to_string()).into());
                }
                _ => {}
            }
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::UnexpectedEof,
        format!("xAI realtime closed before {wanted}"),
    )
    .into())
}

fn user_text_item(text: String) -> Value {
    json!({
        "type": "conversation.item.create",
        "item": {"type": "message", "role": "user", "content": [{"type": "input_text", "text": text}]}
    })
}

#[async_trait]
impl GaiseLiveClient for GaiseClientXaiLive {
    async fn live_connect(&self, config: &GaiseLiveConfig) -> Result<GaiseLiveSession, BoxError> {
        let ws_url = realtime_url(&self.api_url, &config.model);
        let host = http::Uri::try_from(&ws_url)?
            .host()
            .unwrap_or("api.x.ai")
            .to_string();
        let request = http::Request::builder()
            .uri(&ws_url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Sec-WebSocket-Version", "13")
            .header(
                "Sec-WebSocket-Key",
                tungstenite::handshake::client::generate_key(),
            )
            .header("Connection", "Upgrade")
            .header("Upgrade", "websocket")
            .header("Host", host)
            .body(())?;

        let (ws_stream, _response) = tokio_tungstenite::connect_async(request).await?;
        let (mut ws_sink, mut ws_source) = ws_stream.split();

        // `conversation.created` and `session.created` arrive first.
        wait_for(&mut ws_source, "session.created").await?;
        send_json(&mut ws_sink, &[build_xai_session_update(config)]).await?;
        wait_for(&mut ws_source, "session.updated").await?;

        let (input_tx, mut input_rx) = mpsc::channel::<GaiseLiveInput>(256);
        let (event_tx, event_rx) = mpsc::channel::<Result<GaiseLiveEvent, BoxError>>(256);
        let _ = event_tx
            .send(Ok(GaiseLiveEvent::SessionStarted {
                session_id: session_id(),
                model: config.model.clone(),
            }))
            .await;

        let manual_turns = config.vad_config.as_ref().is_some_and(|vad| !vad.enabled);
        let send_events = event_tx.clone();
        tokio::spawn(async move {
            let response_create = json!({"type": "response.create"});
            while let Some(input) = input_rx.recv().await {
                let frames: Vec<Value> = match input {
                    GaiseLiveInput::Audio { data, sample_rate } => {
                        if sample_rate != LIVE_SAMPLE_RATE {
                            let _ = send_events
                                .send(Ok(GaiseLiveEvent::Error {
                                    message: format!(
                                        "xAI realtime sessions are configured for {LIVE_SAMPLE_RATE} Hz PCM16 input; received {sample_rate} Hz"
                                    ),
                                }))
                                .await;
                            continue;
                        }
                        vec![json!({
                            "type": "input_audio_buffer.append",
                            "audio": base64::prelude::BASE64_STANDARD.encode(&data),
                        })]
                    }
                    GaiseLiveInput::Image { .. } => {
                        let _ = send_events
                            .send(Ok(GaiseLiveEvent::Error {
                                message: "xAI speech-to-speech does not accept image input"
                                    .to_string(),
                            }))
                            .await;
                        continue;
                    }
                    GaiseLiveInput::Text { text } => {
                        vec![user_text_item(text), response_create.clone()]
                    }
                    GaiseLiveInput::ToolResponse {
                        call_id, result, ..
                    } => vec![
                        json!({
                            "type": "conversation.item.create",
                            "item": {"type": "function_call_output", "call_id": call_id, "output": result.to_string()}
                        }),
                        response_create.clone(),
                    ],
                    GaiseLiveInput::ActivityStart => continue,
                    GaiseLiveInput::ActivityEnd | GaiseLiveInput::AudioStreamEnd => {
                        if !manual_turns {
                            continue;
                        }
                        vec![
                            json!({"type": "input_audio_buffer.commit"}),
                            response_create.clone(),
                        ]
                    }
                    GaiseLiveInput::ClearAudio => vec![json!({"type": "input_audio_buffer.clear"})],
                    GaiseLiveInput::CancelResponse => vec![json!({"type": "response.cancel"})],
                    GaiseLiveInput::Close => {
                        let _ = ws_sink.close().await;
                        break;
                    }
                };
                if let Err(e) = send_json(&mut ws_sink, &frames).await {
                    let _ = send_events
                        .send(Ok(GaiseLiveEvent::Error {
                            message: format!("Send error: {e}"),
                        }))
                        .await;
                    break;
                }
            }
        });

        tokio::spawn(async move {
            while let Some(msg) = ws_source.next().await {
                let text = match msg {
                    Ok(Message::Text(t)) => t,
                    Ok(Message::Close(_)) => {
                        let _ = event_tx.send(Ok(GaiseLiveEvent::SessionEnded)).await;
                        break;
                    }
                    Ok(_) => continue,
                    Err(e) => {
                        let _ = event_tx
                            .send(Ok(GaiseLiveEvent::Error {
                                message: format!("WebSocket error: {e}"),
                            }))
                            .await;
                        break;
                    }
                };
                let event: Value = match serde_json::from_str(&text) {
                    Ok(v) => v,
                    Err(e) => {
                        let _ = event_tx
                            .send(Ok(GaiseLiveEvent::Error {
                                message: format!("Parse error: {e}"),
                            }))
                            .await;
                        continue;
                    }
                };
                for mapped in map_xai_live_event(&event) {
                    if event_tx.send(Ok(mapped)).await.is_err() {
                        return;
                    }
                }
            }
        });

        let stream: GaiseLiveEventStream =
            Box::pin(tokio_stream::wrappers::ReceiverStream::new(event_rx));
        Ok(GaiseLiveSession {
            tx: input_tx,
            rx: stream,
        })
    }
}

fn session_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let d = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!("{:x}{:x}", d.as_secs(), d.subsec_nanos())
}
