#![cfg(feature = "live")]

use gaise_core::contracts::*;
use gaise_provider_xai::xai_live_client::*;
use serde_json::json;

#[test]
fn session_update_uses_xai_fields() {
    let config: GaiseLiveConfig = serde_json::from_value(json!({
        "model": "grok-voice-3.0",
        "system_instruction": "You are helpful.",
        "voice": "Eve",
        "tools": [{"name": "get_weather", "description": "Weather", "parameters": {
            "type": "object", "properties": {"city": {"type": "text"}}, "required": ["city"]
        }}],
        "generation_config": {"thinking_effort": "none"},
        "transcription": {"input": true}
    }))
    .unwrap();
    let update = build_xai_session_update(&config);
    let session = &update["session"];
    assert_eq!(update["type"], "session.update");
    assert_eq!(session["instructions"], "You are helpful.");
    assert_eq!(session["voice"], "eve");
    assert_eq!(session["turn_detection"]["type"], "server_vad");
    assert_eq!(session["audio"]["input"]["format"]["rate"], 24_000);
    assert_eq!(session["audio"]["output"]["format"]["type"], "audio/pcm");
    assert_eq!(
        session["audio"]["input"]["transcription"]["model"],
        "grok-transcribe"
    );
    assert_eq!(session["reasoning"]["effort"], "none");
    assert_eq!(session["tools"][0]["type"], "function");
    assert_eq!(session["tools"][0]["name"], "get_weather");
    assert_eq!(
        session["tools"][0]["parameters"]["properties"]["city"]["type"],
        "string"
    );
    // OpenAI-only fields are never sent.
    assert!(session.get("type").is_none());
    assert!(session.get("output_modalities").is_none());
}

#[test]
fn manual_turns_send_null_turn_detection_and_effort_maps_to_high() {
    let config: GaiseLiveConfig = serde_json::from_value(json!({
        "model": "grok-voice-latest",
        "vad_config": {"enabled": false},
        "generation_config": {"thinking_effort": "medium"}
    }))
    .unwrap();
    let update = build_xai_session_update(&config);
    assert!(update["session"]["turn_detection"].is_null());
    assert!(
        update["session"]
            .as_object()
            .unwrap()
            .contains_key("turn_detection")
    );
    assert_eq!(update["session"]["reasoning"]["effort"], "high");
    assert_eq!(xai_live_reasoning_effort("auto"), None);
}

#[test]
fn realtime_url_handles_v1_and_bare_hosts() {
    assert_eq!(
        realtime_url("https://api.x.ai/v1", "grok-voice-3.0"),
        "wss://api.x.ai/v1/realtime?model=grok-voice-3.0"
    );
    assert_eq!(
        realtime_url("https://api.x.ai/", "grok-voice-latest"),
        "wss://api.x.ai/v1/realtime?model=grok-voice-latest"
    );
}

#[test]
fn server_events_map_to_live_events() {
    let audio =
        map_xai_live_event(&json!({"type": "response.output_audio.delta", "delta": "AQID"}));
    assert!(
        matches!(&audio[0], GaiseLiveEvent::Audio { data, sample_rate: 24_000 } if data == &vec![1, 2, 3])
    );
    let legacy = map_xai_live_event(&json!({"type": "response.audio.delta", "delta": "AQID"}));
    assert_eq!(legacy.len(), 1);
    let text = map_xai_live_event(&json!({"type": "response.text.delta", "delta": "hi"}));
    assert!(matches!(&text[0], GaiseLiveEvent::Text { text } if text == "hi"));
    let transcript = map_xai_live_event(
        &json!({"type": "response.output_audio_transcript.delta", "delta": "yo"}),
    );
    assert!(
        matches!(&transcript[0], GaiseLiveEvent::Transcript { role, .. } if role == "assistant")
    );
    assert!(
        map_xai_live_event(&json!({"type": "conversation.item.input_audio_transcription.updated", "transcript": "partial"}))
            .is_empty()
    );
    let user = map_xai_live_event(
        &json!({"type": "conversation.item.input_audio_transcription.completed", "transcript": "hello"}),
    );
    assert!(
        matches!(&user[0], GaiseLiveEvent::Transcript { role, text } if role == "user" && text == "hello")
    );
    let call = map_xai_live_event(
        &json!({"type": "response.function_call_arguments.done", "call_id": "c1", "name": "f", "arguments": "{}"}),
    );
    assert!(
        matches!(&call[0], GaiseLiveEvent::ToolCall { id, function } if id == "c1" && function.name == "f")
    );
    let done = map_xai_live_event(
        &json!({"type": "response.done", "response": {"usage": {"input_tokens": 5, "output_tokens": 7, "total_tokens": 12}}}),
    );
    assert!(
        matches!(&done[0], GaiseLiveEvent::Usage(u) if u.total.as_ref().unwrap()["total_tokens"] == 12)
    );
    assert!(matches!(done[1], GaiseLiveEvent::TurnComplete));
    let done_no_usage = map_xai_live_event(&json!({"type": "response.done", "response": {}}));
    assert_eq!(done_no_usage.len(), 1);
    assert!(matches!(
        map_xai_live_event(&json!({"type": "input_audio_buffer.speech_started"}))[0],
        GaiseLiveEvent::Interrupted
    ));
    let err = map_xai_live_event(&json!({"type": "error", "error": {"message": "bad"}}));
    assert!(matches!(&err[0], GaiseLiveEvent::Error { message } if message == "bad"));
}
