use async_trait::async_trait;
use base64::Engine;
use futures_util::{SinkExt, StreamExt};
use gaise_core::GaiseLiveClient;
use gaise_core::contracts::{
    GaiseFunctionCall, GaiseLiveConfig, GaiseLiveEvent, GaiseLiveEventStream, GaiseLiveInput,
    GaiseLiveModality, GaiseLiveSession, GaiseTool, GaiseToolParameter, GaiseUsage,
};
use std::collections::HashMap;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

use crate::contracts::realtime_models::*;

pub struct GaiseClientOpenAILive {
    api_url: String,
    api_key: String,
}

impl GaiseClientOpenAILive {
    pub fn new(api_url: String, api_key: String) -> Self {
        Self { api_url, api_key }
    }
}

fn map_tool_parameter(param: &GaiseToolParameter) -> serde_json::Value {
    let mut obj = serde_json::Map::new();
    if let Some(t) = &param.r#type {
        let mapped = if t == "text" { "string" } else { t.as_str() };
        obj.insert("type".into(), serde_json::Value::String(mapped.to_string()));
    }
    if let Some(desc) = &param.description {
        obj.insert(
            "description".into(),
            serde_json::Value::String(desc.clone()),
        );
    }
    if let Some(props) = &param.properties {
        let mut properties = serde_json::Map::new();
        for (k, v) in props {
            properties.insert(k.clone(), map_tool_parameter(v));
        }
        obj.insert("properties".into(), serde_json::Value::Object(properties));
    }
    if let Some(items) = &param.items {
        obj.insert("items".into(), map_tool_parameter(items));
    }
    if let Some(req) = &param.required {
        obj.insert(
            "required".into(),
            serde_json::Value::Array(
                req.iter()
                    .map(|r| serde_json::Value::String(r.clone()))
                    .collect(),
            ),
        );
    }
    serde_json::Value::Object(obj)
}

fn build_realtime_tools(tools: &[GaiseTool]) -> Vec<OpenAIRealtimeTool> {
    tools
        .iter()
        .map(|t| OpenAIRealtimeTool {
            r#type: "function".to_string(),
            name: t.name.clone(),
            description: t.description.clone(),
            parameters: t
                .parameters
                .as_ref()
                .map(map_tool_parameter)
                .unwrap_or(serde_json::json!({"type": "object", "properties": {}})),
        })
        .collect()
}

fn build_session_update(config: &GaiseLiveConfig) -> OpenAIRealtimeSessionUpdate {
    let modalities: Vec<String> = if config.modalities.is_empty() {
        vec!["audio".to_string(), "text".to_string()]
    } else {
        config
            .modalities
            .iter()
            .map(|m| match m {
                GaiseLiveModality::Text => "text".to_string(),
                GaiseLiveModality::Audio => "audio".to_string(),
            })
            .collect()
    };

    let turn_detection = config.vad_config.as_ref().map(|vad| {
        OpenAIRealtimeTurnDetection {
            r#type: "server_vad".to_string(),
            threshold: None,
            prefix_padding_ms: vad.prefix_padding_ms,
            silence_duration_ms: vad.silence_duration_ms,
        }
    });

    let tools = config.tools.as_ref().map(|ts| build_realtime_tools(ts));

    let temperature = config
        .generation_config
        .as_ref()
        .and_then(|gc| gc.temperature);
    let max_response_output_tokens = config
        .generation_config
        .as_ref()
        .and_then(|gc| gc.max_tokens)
        .map(serde_json::Value::from);

    let input_audio_transcription = config
        .transcription
        .as_ref()
        .filter(|t| t.input)
        .map(|_| OpenAIRealtimeTranscriptionConfig {
            model: "whisper-1".to_string(),
        });

    OpenAIRealtimeSessionUpdate {
        r#type: "session.update".to_string(),
        session: OpenAIRealtimeSessionConfig {
            modalities: Some(modalities),
            instructions: config.system_instruction.clone(),
            voice: config.voice.clone(),
            temperature,
            max_response_output_tokens,
            tools,
            tool_choice: None,
            turn_detection,
            input_audio_transcription,
        },
    }
}

async fn send_two_messages<S: serde::Serialize, T: serde::Serialize>(
    sink: &mut futures_util::stream::SplitSink<
        tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
        Message,
    >,
    msg1: &S,
    msg2: &T,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let json1 = serde_json::to_string(msg1)?;
    sink.send(Message::Text(json1.into())).await?;
    let json2 = serde_json::to_string(msg2)?;
    sink.send(Message::Text(json2.into())).await?;
    Ok(())
}

#[async_trait]
impl GaiseLiveClient for GaiseClientOpenAILive {
    async fn live_connect(
        &self,
        config: &GaiseLiveConfig,
    ) -> Result<GaiseLiveSession, Box<dyn std::error::Error + Send + Sync>> {
        // Build WebSocket URL: wss://api.openai.com/v1/realtime?model=MODEL
        let base = self
            .api_url
            .trim_end_matches('/')
            .replace("https://", "wss://")
            .replace("http://", "ws://");
        let ws_url = format!("{}/v1/realtime?model={}", base, config.model);

        // Build request with auth header
        let request = http::Request::builder()
            .uri(&ws_url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("OpenAI-Beta", "realtime=v1")
            .header("Sec-WebSocket-Version", "13")
            .header("Sec-WebSocket-Key", tungstenite::handshake::client::generate_key())
            .header("Connection", "Upgrade")
            .header("Upgrade", "websocket")
            .header("Host", http::Uri::try_from(&ws_url)?.host().unwrap_or("api.openai.com"))
            .body(())?;

        let (ws_stream, _response) = tokio_tungstenite::connect_async(request).await?;
        let (mut ws_sink, mut ws_source) = ws_stream.split();

        // Wait for session.created
        while let Some(msg) = ws_source.next().await {
            let msg = msg?;
            if let Message::Text(text) = msg {
                let event: OpenAIRealtimeServerEvent = serde_json::from_str(&text)?;
                if event.r#type == "session.created" {
                    break;
                }
            }
        }

        // Send session.update with config
        let session_update = build_session_update(config);
        let update_json = serde_json::to_string(&session_update)?;
        ws_sink.send(Message::Text(update_json.into())).await?;

        // Wait for session.updated
        while let Some(msg) = ws_source.next().await {
            let msg = msg?;
            if let Message::Text(text) = msg {
                let event: OpenAIRealtimeServerEvent = serde_json::from_str(&text)?;
                if event.r#type == "session.updated" {
                    break;
                }
            }
        }

        // Create channels
        let (input_tx, mut input_rx) = mpsc::channel::<GaiseLiveInput>(256);
        let (event_tx, event_rx) =
            mpsc::channel::<Result<GaiseLiveEvent, Box<dyn std::error::Error + Send + Sync>>>(256);

        let session_id = uuid_simple();

        let _ = event_tx
            .send(Ok(GaiseLiveEvent::SessionStarted {
                session_id: session_id.clone(),
                model: config.model.clone(),
            }))
            .await;

        // Spawn send loop
        let event_tx_send = event_tx.clone();
        tokio::spawn(async move {
            while let Some(input) = input_rx.recv().await {
                let result: Result<(), Box<dyn std::error::Error + Send + Sync>> = match input {
                    GaiseLiveInput::Audio { data, .. } => {
                        let b64 = base64::prelude::BASE64_STANDARD.encode(&data);
                        let msg = OpenAIRealtimeAudioAppend {
                            r#type: "input_audio_buffer.append".to_string(),
                            audio: b64,
                        };
                        match serde_json::to_string(&msg) {
                            Ok(json) => ws_sink
                                .send(Message::Text(json.into()))
                                .await
                                .map_err(|e| e.into()),
                            Err(e) => Err(e.into()),
                        }
                    }
                    GaiseLiveInput::Text { text } => {
                        let item_msg = OpenAIRealtimeItemCreate {
                            r#type: "conversation.item.create".to_string(),
                            item: OpenAIRealtimeItem {
                                r#type: "message".to_string(),
                                role: Some("user".to_string()),
                                content: Some(vec![OpenAIRealtimeItemContent {
                                    r#type: "input_text".to_string(),
                                    text,
                                }]),
                                call_id: None,
                                output: None,
                            },
                        };
                        let response_msg = OpenAIRealtimeResponseCreate {
                            r#type: "response.create".to_string(),
                        };
                        send_two_messages(&mut ws_sink, &item_msg, &response_msg).await
                    }
                    GaiseLiveInput::ToolResponse {
                        call_id,
                        name: _,
                        result,
                    } => {
                        let item_msg = OpenAIRealtimeItemCreate {
                            r#type: "conversation.item.create".to_string(),
                            item: OpenAIRealtimeItem {
                                r#type: "function_call_output".to_string(),
                                role: None,
                                content: None,
                                call_id: Some(call_id),
                                output: Some(result.to_string()),
                            },
                        };
                        let response_msg = OpenAIRealtimeResponseCreate {
                            r#type: "response.create".to_string(),
                        };
                        send_two_messages(&mut ws_sink, &item_msg, &response_msg).await
                    }
                    GaiseLiveInput::Close => {
                        let _ = ws_sink.close().await;
                        break;
                    }
                };

                if let Err(e) = result {
                    let _ = event_tx_send
                        .send(Ok(GaiseLiveEvent::Error {
                            message: format!("Send error: {}", e),
                        }))
                        .await;
                    break;
                }
            }
        });

        // Spawn receive loop
        tokio::spawn(async move {
            while let Some(msg) = ws_source.next().await {
                let msg = match msg {
                    Ok(m) => m,
                    Err(e) => {
                        let _ = event_tx
                            .send(Ok(GaiseLiveEvent::Error {
                                message: format!("WebSocket error: {}", e),
                            }))
                            .await;
                        break;
                    }
                };

                let text = match msg {
                    Message::Text(t) => t,
                    Message::Close(_) => {
                        let _ = event_tx.send(Ok(GaiseLiveEvent::SessionEnded)).await;
                        break;
                    }
                    _ => continue,
                };

                let event: OpenAIRealtimeServerEvent = match serde_json::from_str(&text) {
                    Ok(e) => e,
                    Err(e) => {
                        let _ = event_tx
                            .send(Ok(GaiseLiveEvent::Error {
                                message: format!("Parse error: {}", e),
                            }))
                            .await;
                        continue;
                    }
                };

                match event.r#type.as_str() {
                    // Audio output
                    "response.audio.delta" => {
                        if let Some(delta) = &event.delta {
                            if let Ok(audio_bytes) =
                                base64::prelude::BASE64_STANDARD.decode(delta)
                            {
                                let _ = event_tx
                                    .send(Ok(GaiseLiveEvent::Audio {
                                        data: audio_bytes,
                                        sample_rate: 24000,
                                    }))
                                    .await;
                            }
                        }
                    }

                    // Text output
                    "response.text.delta" => {
                        if let Some(delta) = &event.delta {
                            let _ = event_tx
                                .send(Ok(GaiseLiveEvent::Text {
                                    text: delta.clone(),
                                }))
                                .await;
                        }
                    }

                    // Audio transcript (model speech as text)
                    "response.audio_transcript.delta" => {
                        if let Some(delta) = &event.delta {
                            let _ = event_tx
                                .send(Ok(GaiseLiveEvent::Transcript {
                                    role: "assistant".to_string(),
                                    text: delta.clone(),
                                }))
                                .await;
                        }
                    }

                    // Input audio transcription
                    "conversation.item.input_audio_transcription.completed" => {
                        if let Some(transcript) = &event.transcript {
                            let _ = event_tx
                                .send(Ok(GaiseLiveEvent::Transcript {
                                    role: "user".to_string(),
                                    text: transcript.clone(),
                                }))
                                .await;
                        }
                    }

                    // Tool call completed
                    "response.function_call_arguments.done" => {
                        if let (Some(call_id), Some(name)) = (&event.call_id, &event.name) {
                            let _ = event_tx
                                .send(Ok(GaiseLiveEvent::ToolCall {
                                    id: call_id.clone(),
                                    function: GaiseFunctionCall {
                                        name: name.clone(),
                                        arguments: event.arguments.clone(),
                                    },
                                }))
                                .await;
                        }
                    }

                    // Response done (turn complete)
                    "response.done" => {
                        // Extract usage if available
                        if let Some(resp) = &event.response {
                            if let Some(usage) = &resp.usage {
                                let mut input_map = HashMap::new();
                                let mut output_map = HashMap::new();
                                if let Some(input_tokens) = usage.input_tokens {
                                    input_map
                                        .insert("input_tokens".to_string(), input_tokens);
                                }
                                if let Some(output_tokens) = usage.output_tokens {
                                    output_map
                                        .insert("output_tokens".to_string(), output_tokens);
                                }
                                let _ = event_tx
                                    .send(Ok(GaiseLiveEvent::Usage(GaiseUsage {
                                        input: if input_map.is_empty() {
                                            None
                                        } else {
                                            Some(input_map)
                                        },
                                        output: if output_map.is_empty() {
                                            None
                                        } else {
                                            Some(output_map)
                                        },
                                    })))
                                    .await;
                            }
                        }
                        let _ = event_tx.send(Ok(GaiseLiveEvent::TurnComplete)).await;
                    }

                    // Speech stopped (barge-in)
                    "input_audio_buffer.speech_stopped" => {
                        let _ = event_tx.send(Ok(GaiseLiveEvent::Interrupted)).await;
                    }

                    // Error
                    "error" => {
                        let message = event
                            .error
                            .as_ref()
                            .and_then(|e| e.message.clone())
                            .unwrap_or_else(|| "Unknown error".to_string());
                        let _ = event_tx
                            .send(Ok(GaiseLiveEvent::Error { message }))
                            .await;
                    }

                    // Ignore other event types (session.created, session.updated, etc.)
                    _ => {}
                }
            }
        });

        let event_stream = tokio_stream::wrappers::ReceiverStream::new(event_rx);
        let pinned: GaiseLiveEventStream = Box::pin(event_stream);

        Ok(GaiseLiveSession {
            tx: input_tx,
            rx: pinned,
        })
    }
}

fn uuid_simple() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let d = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!("{:x}{:x}", d.as_secs(), d.subsec_nanos())
}
