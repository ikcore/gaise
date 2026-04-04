#![cfg(feature = "live")]

use gaise_core::contracts::*;

mod tests {
    use super::*;
    use gaise_provider_openai::contracts::realtime_models::*;

    #[test]
    fn test_session_update_basic() {
        let config = GaiseLiveConfig {
            model: "gpt-4o-realtime-preview".to_string(),
            system_instruction: Some("You are a helpful assistant.".to_string()),
            voice: Some("alloy".to_string()),
            modalities: vec![GaiseLiveModality::Audio, GaiseLiveModality::Text],
            ..Default::default()
        };

        let update = build_test_session_update(&config);
        let json = serde_json::to_value(&update).unwrap();

        assert_eq!(json["type"], "session.update");
        assert_eq!(json["session"]["modalities"][0], "audio");
        assert_eq!(json["session"]["modalities"][1], "text");
        assert_eq!(json["session"]["instructions"], "You are a helpful assistant.");
        assert_eq!(json["session"]["voice"], "alloy");
    }

    #[test]
    fn test_session_update_with_tools() {
        let config = GaiseLiveConfig {
            model: "gpt-4o-realtime-preview".to_string(),
            tools: Some(vec![GaiseTool {
                name: "get-weather".to_string(),
                description: Some("Get weather".to_string()),
                parameters: Some(GaiseToolParameter {
                    r#type: Some("object".to_string()),
                    properties: Some(
                        [("city".to_string(), GaiseToolParameter {
                            r#type: Some("string".to_string()),
                            description: Some("City name".to_string()),
                            ..Default::default()
                        })]
                        .into_iter()
                        .collect(),
                    ),
                    required: Some(vec!["city".to_string()]),
                    ..Default::default()
                }),
            }]),
            ..Default::default()
        };

        let update = build_test_session_update(&config);
        let json = serde_json::to_value(&update).unwrap();

        let tools = &json["session"]["tools"];
        assert_eq!(tools[0]["type"], "function");
        // OpenAI preserves hyphens in tool names (unlike Gemini)
        assert_eq!(tools[0]["name"], "get-weather");
        assert_eq!(tools[0]["description"], "Get weather");
        assert_eq!(tools[0]["parameters"]["properties"]["city"]["type"], "string");
    }

    #[test]
    fn test_session_update_with_vad() {
        let config = GaiseLiveConfig {
            model: "gpt-4o-realtime-preview".to_string(),
            vad_config: Some(GaiseVadConfig {
                enabled: true,
                silence_duration_ms: Some(500),
                prefix_padding_ms: Some(300),
                ..Default::default()
            }),
            ..Default::default()
        };

        let update = build_test_session_update(&config);
        let json = serde_json::to_value(&update).unwrap();

        let td = &json["session"]["turn_detection"];
        assert_eq!(td["type"], "server_vad");
        assert_eq!(td["silence_duration_ms"], 500);
        assert_eq!(td["prefix_padding_ms"], 300);
    }

    #[test]
    fn test_session_update_with_transcription() {
        let config = GaiseLiveConfig {
            model: "gpt-4o-realtime-preview".to_string(),
            transcription: Some(GaiseTranscriptionConfig {
                input: true,
                output: false,
            }),
            ..Default::default()
        };

        let update = build_test_session_update(&config);
        let json = serde_json::to_value(&update).unwrap();

        assert_eq!(json["session"]["input_audio_transcription"]["model"], "whisper-1");
    }

    #[test]
    fn test_audio_append_serialization() {
        let msg = OpenAIRealtimeAudioAppend {
            r#type: "input_audio_buffer.append".to_string(),
            audio: "AQIDBA==".to_string(),
        };

        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["type"], "input_audio_buffer.append");
        assert_eq!(json["audio"], "AQIDBA==");
    }

    #[test]
    fn test_text_item_create_serialization() {
        let msg = OpenAIRealtimeItemCreate {
            r#type: "conversation.item.create".to_string(),
            item: OpenAIRealtimeItem {
                r#type: "message".to_string(),
                role: Some("user".to_string()),
                content: Some(vec![OpenAIRealtimeItemContent {
                    r#type: "input_text".to_string(),
                    text: "Hello".to_string(),
                }]),
                call_id: None,
                output: None,
            },
        };

        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["type"], "conversation.item.create");
        assert_eq!(json["item"]["type"], "message");
        assert_eq!(json["item"]["role"], "user");
        assert_eq!(json["item"]["content"][0]["type"], "input_text");
        assert_eq!(json["item"]["content"][0]["text"], "Hello");
    }

    #[test]
    fn test_tool_response_item_create_serialization() {
        let msg = OpenAIRealtimeItemCreate {
            r#type: "conversation.item.create".to_string(),
            item: OpenAIRealtimeItem {
                r#type: "function_call_output".to_string(),
                role: None,
                content: None,
                call_id: Some("call_123".to_string()),
                output: Some(r#"{"temperature": 22}"#.to_string()),
            },
        };

        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["item"]["type"], "function_call_output");
        assert_eq!(json["item"]["call_id"], "call_123");
        assert_eq!(json["item"]["output"], r#"{"temperature": 22}"#);
    }

    #[test]
    fn test_server_event_function_call_done() {
        let json = r#"{
            "type": "response.function_call_arguments.done",
            "call_id": "call_abc",
            "name": "get-weather",
            "arguments": "{\"city\": \"London\"}"
        }"#;

        let event: OpenAIRealtimeServerEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.r#type, "response.function_call_arguments.done");
        assert_eq!(event.call_id.unwrap(), "call_abc");
        assert_eq!(event.name.unwrap(), "get-weather");
        assert_eq!(event.arguments.unwrap(), r#"{"city": "London"}"#);
    }

    #[test]
    fn test_server_event_response_done_with_usage() {
        let json = r#"{
            "type": "response.done",
            "response": {
                "usage": {
                    "total_tokens": 150,
                    "input_tokens": 50,
                    "output_tokens": 100
                }
            }
        }"#;

        let event: OpenAIRealtimeServerEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.r#type, "response.done");
        let usage = event.response.unwrap().usage.unwrap();
        assert_eq!(usage.total_tokens, Some(150));
        assert_eq!(usage.input_tokens, Some(50));
        assert_eq!(usage.output_tokens, Some(100));
    }

    #[test]
    fn test_server_event_error() {
        let json = r#"{
            "type": "error",
            "error": {
                "message": "Rate limit exceeded",
                "code": "rate_limit"
            }
        }"#;

        let event: OpenAIRealtimeServerEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.r#type, "error");
        let err = event.error.unwrap();
        assert_eq!(err.message.unwrap(), "Rate limit exceeded");
        assert_eq!(err.code.unwrap(), "rate_limit");
    }

    #[test]
    fn test_server_event_audio_delta() {
        let json = r#"{
            "type": "response.audio.delta",
            "delta": "AQIDBA=="
        }"#;

        let event: OpenAIRealtimeServerEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.r#type, "response.audio.delta");
        assert_eq!(event.delta.unwrap(), "AQIDBA==");
    }

    // Helper to build session update (mirrors the private function in openai_live_client)
    fn build_test_session_update(config: &GaiseLiveConfig) -> OpenAIRealtimeSessionUpdate {
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

        let tools = config.tools.as_ref().map(|ts| {
            ts.iter()
                .map(|t| OpenAIRealtimeTool {
                    r#type: "function".to_string(),
                    name: t.name.clone(),
                    description: t.description.clone(),
                    parameters: t
                        .parameters
                        .as_ref()
                        .map(map_test_param)
                        .unwrap_or(serde_json::json!({"type": "object", "properties": {}})),
                })
                .collect()
        });

        let temperature = config.generation_config.as_ref().and_then(|gc| gc.temperature);
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

    fn map_test_param(param: &GaiseToolParameter) -> serde_json::Value {
        let mut obj = serde_json::Map::new();
        if let Some(t) = &param.r#type {
            obj.insert("type".into(), serde_json::Value::String(t.clone()));
        }
        if let Some(desc) = &param.description {
            obj.insert("description".into(), serde_json::Value::String(desc.clone()));
        }
        if let Some(props) = &param.properties {
            let mut properties = serde_json::Map::new();
            for (k, v) in props {
                properties.insert(k.clone(), map_test_param(v));
            }
            obj.insert("properties".into(), serde_json::Value::Object(properties));
        }
        if let Some(req) = &param.required {
            obj.insert(
                "required".into(),
                serde_json::Value::Array(req.iter().map(|r| serde_json::Value::String(r.clone())).collect()),
            );
        }
        serde_json::Value::Object(obj)
    }
}
