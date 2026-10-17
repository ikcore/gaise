#![cfg(feature = "xai")]
//! `xai` routing through `GaiseClientService`; no xAI traffic is generated.

use gaise_client::{GaiseClientConfig, GaiseClientService};

#[tokio::test]
async fn xai_resolves_for_instruct_and_speech_when_keyed() {
    let service = GaiseClientService::new(GaiseClientConfig {
        xai_api_key: Some("xai-test".to_string()),
        ..Default::default()
    });
    assert!(service.get_client("xai").await.is_ok());
    assert!(service.get_speech_client("xai").await.is_ok());
    assert!(
        service
            .configured_providers()
            .await
            .contains(&"xai".to_string())
    );
}

#[tokio::test]
async fn xai_without_a_key_is_an_error() {
    let service = GaiseClientService::new(GaiseClientConfig::default());
    let err = service.get_client("xai").await.err().unwrap().to_string();
    assert!(err.contains("xAI API Key not configured"), "{err}");
    assert!(
        !service
            .configured_providers()
            .await
            .contains(&"xai".to_string())
    );
}

#[cfg(feature = "live")]
#[tokio::test]
async fn xai_resolves_a_live_client() {
    let service = GaiseClientService::new(GaiseClientConfig {
        xai_api_key: Some("xai-test".to_string()),
        ..Default::default()
    });
    assert!(service.get_live_client("xai").await.is_ok());
}
