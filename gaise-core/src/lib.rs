use async_trait::async_trait;

use crate::contracts::{
    GaiseEmbeddingsRequest, GaiseEmbeddingsResponse, GaiseInstructRequest, GaiseInstructResponse,
    GaiseInstructStreamResponse, GaiseListModelsRequest, GaiseListModelsResponse, GaiseLiveConfig,
    GaiseLiveSession, GaiseSpeechRequest, GaiseSpeechResponse, GaiseSpeechStreamResponse,
};
pub mod contracts;
pub mod logging;
pub mod registry;

#[async_trait]
pub trait GaiseClient: Send + Sync {
    /// Evaluate typed questions against shared state. Unsupported by default
    /// so existing providers and custom clients retain their behavior.
    async fn decision(
        &self,
        request: &contracts::GaiseDecisionRequest,
    ) -> Result<contracts::GaiseDecisionResponse, Box<dyn std::error::Error + Send + Sync>> {
        let _ = request;
        Err("Decision is not supported by this client".into())
    }

    /// [`GaiseClient::decision`] with images shared by every question
    /// ([`contracts::GaiseContent::Image`] items). Without images this is
    /// `decision`; clients whose decision models cannot see images reject them.
    async fn decision_with_images(
        &self,
        request: &contracts::GaiseDecisionRequest,
        images: &[contracts::GaiseContent],
    ) -> Result<contracts::GaiseDecisionResponse, Box<dyn std::error::Error + Send + Sync>> {
        if images.is_empty() {
            return self.decision(request).await;
        }
        Err("Image input for decisions is not supported by this client".into())
    }

    /// Pre-4.0 name of [`GaiseClient::decision`]; forwards to it.
    #[deprecated(since = "4.0.0", note = "renamed to `decision`")]
    async fn system_one(
        &self,
        request: &contracts::GaiseDecisionRequest,
    ) -> Result<contracts::GaiseDecisionResponse, Box<dyn std::error::Error + Send + Sync>> {
        self.decision(request).await
    }

    async fn instruct_stream(
        &self,
        request: &GaiseInstructRequest,
    ) -> Result<
        std::pin::Pin<
            Box<
                dyn futures_util::Stream<
                        Item = Result<
                            GaiseInstructStreamResponse,
                            Box<dyn std::error::Error + Send + Sync>,
                        >,
                    > + Send,
            >,
        >,
        Box<dyn std::error::Error + Send + Sync>,
    >;

    async fn instruct(
        &self,
        request: &GaiseInstructRequest,
    ) -> Result<GaiseInstructResponse, Box<dyn std::error::Error + Send + Sync>>;
    async fn embeddings(
        &self,
        request: &GaiseEmbeddingsRequest,
    ) -> Result<GaiseEmbeddingsResponse, Box<dyn std::error::Error + Send + Sync>>;

    /// Embed text, images, audio, video, or documents: one vector per input
    /// item. Text-only requests go through [`GaiseClient::embeddings`] on
    /// every client; other content needs a multimodal embedding adapter.
    async fn embed_contents(
        &self,
        request: &contracts::GaiseContentEmbeddingsRequest,
    ) -> Result<GaiseEmbeddingsResponse, Box<dyn std::error::Error + Send + Sync>> {
        request.validate()?;
        match request.text_request() {
            Some(text) => self.embeddings(&text).await,
            None => Err("Multimodal embeddings are not supported by this client".into()),
        }
    }

    /// List the models this client can reach.
    ///
    /// Adapters return bare provider model identifiers and only the
    /// capabilities the provider API actually reports; see
    /// [`contracts::GaiseModel`] for the unknown-vs-unsupported rules and
    /// [`registry`] for the advisory overlay applied by the router. The
    /// default implementation reports that listing is unsupported so custom
    /// clients keep compiling.
    async fn list_models(
        &self,
        request: &GaiseListModelsRequest,
    ) -> Result<GaiseListModelsResponse, Box<dyn std::error::Error + Send + Sync>> {
        let _ = request;
        Err("model listing is not supported by this client".into())
    }
}

#[async_trait]
pub trait GaiseLiveClient: Send + Sync {
    async fn live_connect(
        &self,
        config: &GaiseLiveConfig,
    ) -> Result<GaiseLiveSession, Box<dyn std::error::Error + Send + Sync>>;
}

/// Text-to-speech. Realtime text-in/audio-out streaming uses
/// [`GaiseLiveClient`] with `GaiseLiveInput::Text` and `GaiseLiveEvent::Audio`.
#[async_trait]
pub trait GaiseSpeechClient: Send + Sync {
    async fn speech(
        &self,
        request: &GaiseSpeechRequest,
    ) -> Result<GaiseSpeechResponse, Box<dyn std::error::Error + Send + Sync>>;

    async fn speech_stream(
        &self,
        request: &GaiseSpeechRequest,
    ) -> Result<
        std::pin::Pin<
            Box<
                dyn futures_util::Stream<
                        Item = Result<
                            GaiseSpeechStreamResponse,
                            Box<dyn std::error::Error + Send + Sync>,
                        >,
                    > + Send,
            >,
        >,
        Box<dyn std::error::Error + Send + Sync>,
    >;
}
