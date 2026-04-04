use async_trait::async_trait;
use base64::Engine;
use futures_util::{SinkExt, StreamExt};
use gaise_core::GaiseLiveClient;
use gaise_core::contracts::{
    GaiseLiveConfig, GaiseLiveEvent, GaiseLiveEventStream, GaiseLiveInput,
    GaiseLiveModality, GaiseLiveSession, GaiseTool, GaiseToolParameter, GaiseUsage,
};
use std::collections::HashMap;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

use crate::contracts::live_models::*;

pub struct GaiseClientGeminiLive {
    api_url: String,
    api_key: String,
}

impl GaiseClientGeminiLive {
    pub fn new(api_url: String, api_key: String) -> Self {
        Self { api_url, api_key }
    }
}

/// Gemini doesn't allow hyphens in function names.
fn sanitize_tool_name(name: &str) -> String {
    name.replace('-', "_")
}

fn unsanitize_tool_name(name: &str) -> String {
    name.replace('_', "-")
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

fn build_tool_declarations(tools: &[GaiseTool]) -> Vec<GeminiLiveFunctionDeclaration> {
    tools
        .iter()
        .map(|t| GeminiLiveFunctionDeclaration {
            name: sanitize_tool_name(&t.name),
            description: t.description.clone(),
            parameters: t.parameters.as_ref().map(map_tool_parameter),
        })
        .collect()
}

fn build_setup_message(config: &GaiseLiveConfig, api_model_path: &str) -> GeminiLiveSetup {
    let modalities: Vec<String> = if config.modalities.is_empty() {
        vec!["AUDIO".to_string()]
    } else {
        config
            .modalities
            .iter()
            .map(|m| match m {
                GaiseLiveModality::Text => "TEXT".to_string(),
                GaiseLiveModality::Audio => "AUDIO".to_string(),
            })
            .collect()
    };

    let speech_config = config.voice.as_ref().map(|voice| GeminiLiveSpeechConfig {
        voice_config: GeminiLiveVoiceConfig {
            prebuilt_voice_config: GeminiLivePrebuiltVoice {
                voice_name: voice.clone(),
            },
        },
    });

    let transcription_config = config.transcription.as_ref();
    let input_audio_transcription = transcription_config
        .filter(|t| t.input)
        .map(|_| serde_json::json!({}));
    let output_audio_transcription = transcription_config
        .filter(|t| t.output)
        .map(|_| serde_json::json!({}));

    let realtime_input_config = config.vad_config.as_ref().map(|vad| {
        let sensitivity_map = |s: &str| match s {
            "high" => "START_SENSITIVITY_HIGH",
            "low" => "START_SENSITIVITY_LOW",
            _ => "START_SENSITIVITY_MEDIUM",
        };
        let end_sensitivity_map = |s: &str| match s {
            "high" => "END_SENSITIVITY_HIGH",
            "low" => "END_SENSITIVITY_LOW",
            _ => "END_SENSITIVITY_MEDIUM",
        };

        GeminiLiveRealtimeInputConfig {
            automatic_activity_detection: Some(GeminiLiveVadConfig {
                disabled: Some(!vad.enabled),
                start_of_speech_sensitivity: vad
                    .start_sensitivity
                    .as_deref()
                    .map(|s| sensitivity_map(s).to_string()),
                end_of_speech_sensitivity: vad
                    .end_sensitivity
                    .as_deref()
                    .map(|s| end_sensitivity_map(s).to_string()),
                prefix_padding_ms: vad.prefix_padding_ms,
                silence_duration_ms: vad.silence_duration_ms,
            }),
        }
    });

    let temperature = config
        .generation_config
        .as_ref()
        .and_then(|gc| gc.temperature);
    let max_output_tokens = config.generation_config.as_ref().and_then(|gc| gc.max_tokens);

    let generation_config = GeminiLiveGenerationConfig {
        response_modalities: Some(modalities),
        speech_config,
        temperature,
        max_output_tokens,
        input_audio_transcription,
        output_audio_transcription,
        realtime_input_config,
    };

    let system_instruction =
        config
            .system_instruction
            .as_ref()
            .map(|text| GeminiLiveSystemInstruction {
                parts: vec![GeminiLiveTextPart { text: text.clone() }],
            });

    let tools = config.tools.as_ref().map(|ts| {
        vec![GeminiLiveToolSet {
            function_declarations: build_tool_declarations(ts),
        }]
    });

    GeminiLiveSetup {
        setup: GeminiLiveSetupConfig {
            model: api_model_path.to_string(),
            generation_config: Some(generation_config),
            system_instruction,
            tools,
        },
    }
}

#[async_trait]
impl GaiseLiveClient for GaiseClientGeminiLive {
    async fn live_connect(
        &self,
        config: &GaiseLiveConfig,
    ) -> Result<GaiseLiveSession, Box<dyn std::error::Error + Send + Sync>> {
        // Build WebSocket URL
        // api_url is like "https://generativelanguage.googleapis.com/v1beta"
        // We need: wss://generativelanguage.googleapis.com/ws/google.ai.generativelanguage.v1beta.GenerativeService.BidiGenerateContent
        let url_parsed: url::Url = self.api_url.parse()?;
        let host = url_parsed
            .host_str()
            .ok_or("Invalid API URL: no host")?
            .to_string();
        let scheme = if self.api_url.starts_with("https") {
            "wss"
        } else {
            "ws"
        };

        let ws_url = format!(
            "{}://{}/ws/google.ai.generativelanguage.v1beta.GenerativeService.BidiGenerateContent?key={}",
            scheme, host, self.api_key
        );

        let api_model_path = format!("models/{}", config.model);

        // Connect WebSocket
        let (ws_stream, _response) = tokio_tungstenite::connect_async(&ws_url).await?;
        let (mut ws_sink, mut ws_source) = ws_stream.split();

        // Send setup message
        let setup_msg = build_setup_message(config, &api_model_path);
        let setup_json = serde_json::to_string(&setup_msg)?;
        ws_sink.send(Message::Text(setup_json.into())).await?;

        // Wait for setupComplete
        while let Some(msg) = ws_source.next().await {
            let msg = msg?;
            if let Message::Text(text) = msg {
                let server_msg: GeminiLiveServerMessage = serde_json::from_str(&text)?;
                if server_msg.setup_complete.is_some() {
                    break;
                }
            }
        }

        // Create channels
        let (input_tx, mut input_rx) = mpsc::channel::<GaiseLiveInput>(256);
        let (event_tx, event_rx) = mpsc::channel::<Result<GaiseLiveEvent, Box<dyn std::error::Error + Send + Sync>>>(256);

        let session_id = uuid_simple();

        // Send session started event
        let _ = event_tx
            .send(Ok(GaiseLiveEvent::SessionStarted {
                session_id: session_id.clone(),
                model: config.model.clone(),
            }))
            .await;

        // Spawn send loop: reads from input_rx, writes to ws_sink
        let event_tx_send = event_tx.clone();
        tokio::spawn(async move {
            while let Some(input) = input_rx.recv().await {
                let msg_result = match input {
                    GaiseLiveInput::Audio { data, sample_rate } => {
                        let b64 = base64::prelude::BASE64_STANDARD.encode(&data);
                        let mime = format!("audio/pcm;rate={}", sample_rate);
                        let msg = GeminiLiveRealtimeInput {
                            realtime_input: GeminiLiveRealtimeInputData {
                                media_chunks: Some(vec![GeminiLiveMediaChunk {
                                    mime_type: mime,
                                    data: b64,
                                }]),
                                text: None,
                                audio_stream_end: None,
                            },
                        };
                        serde_json::to_string(&msg)
                    }
                    GaiseLiveInput::Text { text } => {
                        let msg = GeminiLiveClientContent {
                            client_content: GeminiLiveClientContentData {
                                turns: vec![GeminiLiveTurn {
                                    role: "user".to_string(),
                                    parts: vec![GeminiLiveTextPart { text }],
                                }],
                                turn_complete: true,
                            },
                        };
                        serde_json::to_string(&msg)
                    }
                    GaiseLiveInput::ToolResponse {
                        call_id,
                        name,
                        result,
                    } => {
                        let msg = GeminiLiveToolResponse {
                            tool_response: GeminiLiveToolResponseData {
                                function_responses: vec![GeminiLiveFunctionResponse {
                                    id: call_id,
                                    name: sanitize_tool_name(&name),
                                    response: result,
                                }],
                            },
                        };
                        serde_json::to_string(&msg)
                    }
                    GaiseLiveInput::Close => {
                        let msg = GeminiLiveRealtimeInput {
                            realtime_input: GeminiLiveRealtimeInputData {
                                media_chunks: None,
                                text: None,
                                audio_stream_end: Some(true),
                            },
                        };
                        let _ = match serde_json::to_string(&msg) {
                            Ok(json) => ws_sink.send(Message::Text(json.into())).await,
                            Err(_) => Ok(()),
                        };
                        let _ = ws_sink.close().await;
                        break;
                    }
                };

                match msg_result {
                    Ok(json) => {
                        if let Err(e) = ws_sink.send(Message::Text(json.into())).await {
                            let _ = event_tx_send
                                .send(Ok(GaiseLiveEvent::Error {
                                    message: format!("WebSocket send error: {}", e),
                                }))
                                .await;
                            break;
                        }
                    }
                    Err(e) => {
                        let _ = event_tx_send
                            .send(Ok(GaiseLiveEvent::Error {
                                message: format!("Serialization error: {}", e),
                            }))
                            .await;
                    }
                }
            }
        });

        // Spawn receive loop: reads from ws_source, writes to event_tx
        tokio::spawn(async move {
            while let Some(msg) = ws_source.next().await {
                let msg = match msg {
                    Ok(m) => m,
                    Err(e) => {
                        let _ = event_tx
                            .send(Ok(GaiseLiveEvent::Error {
                                message: format!("WebSocket receive error: {}", e),
                            }))
                            .await;
                        break;
                    }
                };

                match msg {
                    Message::Text(text) => {
                        let server_msg: GeminiLiveServerMessage = match serde_json::from_str(&text)
                        {
                            Ok(m) => m,
                            Err(e) => {
                                let _ = event_tx
                                    .send(Ok(GaiseLiveEvent::Error {
                                        message: format!("Parse error: {}", e),
                                    }))
                                    .await;
                                continue;
                            }
                        };

                        // Process server content
                        if let Some(content) = &server_msg.server_content {
                            // Audio output from model
                            if let Some(model_turn) = &content.model_turn {
                                for part in &model_turn.parts {
                                    if let Some(text) = &part.text {
                                        let _ = event_tx
                                            .send(Ok(GaiseLiveEvent::Text {
                                                text: text.clone(),
                                            }))
                                            .await;
                                    }
                                    if let Some(inline_data) = &part.inline_data {
                                        if let Ok(audio_bytes) =
                                            base64::prelude::BASE64_STANDARD
                                                .decode(&inline_data.data)
                                        {
                                            // Parse sample rate from mime_type (e.g. "audio/pcm;rate=24000")
                                            let sample_rate = inline_data
                                                .mime_type
                                                .split("rate=")
                                                .nth(1)
                                                .and_then(|s| s.parse::<u32>().ok())
                                                .unwrap_or(24000);

                                            let _ = event_tx
                                                .send(Ok(GaiseLiveEvent::Audio {
                                                    data: audio_bytes,
                                                    sample_rate,
                                                }))
                                                .await;
                                        }
                                    }
                                }
                            }

                            // Transcriptions
                            if let Some(tx_data) = &content.input_transcription {
                                if let Some(text) = &tx_data.text {
                                    let _ = event_tx
                                        .send(Ok(GaiseLiveEvent::Transcript {
                                            role: "user".to_string(),
                                            text: text.clone(),
                                        }))
                                        .await;
                                }
                            }
                            if let Some(tx_data) = &content.output_transcription {
                                if let Some(text) = &tx_data.text {
                                    let _ = event_tx
                                        .send(Ok(GaiseLiveEvent::Transcript {
                                            role: "assistant".to_string(),
                                            text: text.clone(),
                                        }))
                                        .await;
                                }
                            }

                            // Turn complete
                            if content.turn_complete == Some(true) {
                                let _ = event_tx.send(Ok(GaiseLiveEvent::TurnComplete)).await;
                            }

                            // Interrupted (barge-in)
                            if content.interrupted == Some(true) {
                                let _ = event_tx.send(Ok(GaiseLiveEvent::Interrupted)).await;
                            }
                        }

                        // Tool calls
                        if let Some(tool_call) = &server_msg.tool_call {
                            for fc in &tool_call.function_calls {
                                let _ = event_tx
                                    .send(Ok(GaiseLiveEvent::ToolCall {
                                        id: fc.id.clone(),
                                        function: gaise_core::contracts::GaiseFunctionCall {
                                            name: unsanitize_tool_name(&fc.name),
                                            arguments: fc.args.as_ref().map(|a| a.to_string()),
                                        },
                                    }))
                                    .await;
                            }
                        }

                        // Tool call cancellation
                        if let Some(cancel) = &server_msg.tool_call_cancellation {
                            let _ = event_tx
                                .send(Ok(GaiseLiveEvent::ToolCallCancelled {
                                    ids: cancel.ids.clone(),
                                }))
                                .await;
                        }

                        // Usage metadata
                        if let Some(usage) = &server_msg.usage_metadata {
                            if let Some(total) = usage.total_token_count {
                                let mut output_map = HashMap::new();
                                output_map.insert("total_tokens".to_string(), total);
                                let _ = event_tx
                                    .send(Ok(GaiseLiveEvent::Usage(GaiseUsage {
                                        input: None,
                                        output: Some(output_map),
                                    })))
                                    .await;
                            }
                        }

                        // GoAway
                        if server_msg.go_away.is_some() {
                            let _ = event_tx
                                .send(Ok(GaiseLiveEvent::Error {
                                    message: "Server requested disconnect (goAway)".to_string(),
                                }))
                                .await;
                            let _ = event_tx.send(Ok(GaiseLiveEvent::SessionEnded)).await;
                            break;
                        }
                    }
                    Message::Binary(_) => {
                        // Gemini Live uses JSON text frames, not binary
                    }
                    Message::Close(_) => {
                        let _ = event_tx.send(Ok(GaiseLiveEvent::SessionEnded)).await;
                        break;
                    }
                    _ => {}
                }
            }
        });

        // Convert event_rx into a Stream
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
