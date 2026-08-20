//! `think` mapping per model family through the canonical reasoning vocabulary.

use gaise_core::contracts::GaiseGenerationConfig;
use gaise_provider_ollama::contracts::models::OllamaThink;
use gaise_provider_ollama::ollama_client::ollama_think;

fn think(model: &str, effort: Option<&str>, tokens: Option<usize>) -> Option<serde_json::Value> {
    let config = GaiseGenerationConfig {
        thinking_effort: effort.map(str::to_string),
        thinking_tokens: tokens,
        ..Default::default()
    };
    ollama_think(model, &config).map(|t: OllamaThink| serde_json::to_value(t).unwrap())
}

#[test]
fn gpt_oss_takes_levels_and_other_models_take_booleans() {
    assert_eq!(think("gpt-oss:20b", Some("low"), None), Some("low".into()));
    assert_eq!(
        think("gpt-oss:20b", Some("minimal"), None),
        Some("low".into())
    );
    assert_eq!(
        think("gpt-oss:20b", Some("xhigh"), None),
        Some("high".into())
    );
    assert_eq!(
        think("gpt-oss:20b", Some("ultra"), None),
        Some("high".into())
    );
    assert_eq!(
        think("gpt-oss:20b", Some("maximum"), None),
        Some("high".into())
    );
    assert_eq!(think("gpt-oss:20b", Some("none"), None), Some(false.into()));
    assert_eq!(think("gpt-oss:20b", Some("auto"), None), Some(true.into()));
    assert_eq!(
        think("gpt-oss:20b", Some("deep"), None),
        Some("deep".into()),
        "custom forwarded"
    );
    assert_eq!(
        think("gpt-oss:20b", None, Some(2048)),
        Some("medium".into())
    );
    assert_eq!(think("gpt-oss:20b", None, Some(0)), Some(false.into()));

    assert_eq!(think("qwen3:8b", Some("high"), None), Some(true.into()));
    assert_eq!(think("qwen3:8b", Some("off"), None), Some(false.into()));
    assert_eq!(think("qwen3:8b", Some("auto"), None), Some(true.into()));
    assert_eq!(think("qwen3:8b", None, Some(512)), Some(true.into()));
    assert_eq!(think("qwen3:8b", None, None), None);
}

#[test]
fn embedding_requests_forward_dimensions_and_truncate() {
    use gaise_provider_ollama::contracts::models::OllamaEmbedRequest;
    let body = OllamaEmbedRequest {
        model: "embeddinggemma:latest".into(),
        input: vec!["a".into()],
        options: None,
        dimensions: Some(256),
        truncate: Some(true),
    };
    let json = serde_json::to_value(body).unwrap();
    assert_eq!(json["dimensions"], 256);
    assert_eq!(json["truncate"], true);
}
