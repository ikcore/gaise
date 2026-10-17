//! Hermetic checks of the per-family Chat Completions rules: which effort
//! values reach the wire for each Grok family.

use gaise_core::contracts::GaiseInstructRequest;
use gaise_provider_xai::xai_client::{normalize_xai_effort, xai_chat_request, xai_chat_rules};
use serde_json::json;

fn effort_sent(model: &str, effort: &str) -> Option<String> {
    let request: GaiseInstructRequest = serde_json::from_value(json!({
        "model": model,
        "input": {"role": "user", "content": {"type": "text", "text": "hi"}},
        "generation_config": {"thinking_effort": effort}
    }))
    .unwrap();
    xai_chat_request(&request).unwrap().reasoning_effort
}

#[test]
fn grok_47_and_46_accept_low_to_xhigh_and_cannot_disable_reasoning() {
    for model in ["grok-4.7", "grok-4.6", "grok-4.7-2026-09-28"] {
        assert_eq!(
            effort_sent(model, "xhigh").as_deref(),
            Some("xhigh"),
            "{model}"
        );
        assert_eq!(
            effort_sent(model, "none").as_deref(),
            Some("low"),
            "{model}"
        );
        assert_eq!(
            effort_sent(model, "minimal").as_deref(),
            Some("low"),
            "{model}"
        );
        assert_eq!(
            effort_sent(model, "max").as_deref(),
            Some("xhigh"),
            "{model}"
        );
        assert_eq!(
            effort_sent(model, "ultra").as_deref(),
            Some("xhigh"),
            "{model}"
        );
        assert_eq!(effort_sent(model, "auto"), None, "{model}");
    }
    let rules = xai_chat_rules("grok-4.7");
    assert_eq!(rules.default_effort, Some("high"));
}

#[test]
fn grok_45_treats_xhigh_as_high() {
    for model in ["grok-4.5", "grok-4.5-latest", "grok-build-latest"] {
        assert_eq!(
            effort_sent(model, "xhigh").as_deref(),
            Some("high"),
            "{model}"
        );
        assert_eq!(
            effort_sent(model, "medium").as_deref(),
            Some("medium"),
            "{model}"
        );
    }
}

#[test]
fn grok_43_can_disable_reasoning() {
    assert_eq!(effort_sent("grok-4.3", "none").as_deref(), Some("none"));
    assert_eq!(
        effort_sent("grok-4.3-latest", "xhigh").as_deref(),
        Some("xhigh")
    );
    assert_eq!(xai_chat_rules("grok-4.3").default_effort, Some("low"));
}

#[test]
fn families_without_documented_effort_never_send_it() {
    for model in [
        "grok-4.20-0309-reasoning",
        "grok-4.20",
        "grok-4.20-0309-non-reasoning",
        "grok-build-0.1",
        "grok-code-fast-1",
        "grok-4-1-fast-reasoning",
        "grok-4-fast-non-reasoning",
        "grok-4-0709",
        "grok-3",
        "grok-3-mini",
    ] {
        assert_eq!(effort_sent(model, "high"), None, "{model}");
    }
}

#[test]
fn unknown_grok_models_pass_effort_through() {
    assert_eq!(effort_sent("grok-5", "high").as_deref(), Some("high"));
    assert_eq!(
        effort_sent("grok-5", "custom-level").as_deref(),
        Some("custom-level")
    );
    // A later 4.x release inherits the grok-4.7 profile.
    assert_eq!(effort_sent("grok-4.8", "none").as_deref(), Some("low"));
}

#[test]
fn non_chat_models_are_flagged() {
    for model in [
        "grok-4.20-multi-agent-0309",
        "grok-4.20-multi-agent",
        "grok-imagine-image-2.0",
        "grok-imagine-video-1.5",
        "grok-voice-3.0",
        "grok-voice-transcribe-2.0",
    ] {
        assert!(!xai_chat_rules(model).chat_supported, "{model}");
    }
    assert!(xai_chat_rules("grok-4.7").chat_supported);
}

#[test]
fn normalize_ignores_non_reasoning_rules() {
    let rules = xai_chat_rules("grok-4.20-non-reasoning");
    assert!(!rules.reasoning);
    assert_eq!(normalize_xai_effort(&rules, "high"), None);
}
