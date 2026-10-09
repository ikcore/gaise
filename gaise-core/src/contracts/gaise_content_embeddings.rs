//! Multimodal embeddings: one vector per [`GaiseContent`] item, for models
//! that embed images, audio, video, or documents (`gemini-embedding-2`).
//! Served by [`GaiseClient::embed_contents`](crate::GaiseClient::embed_contents);
//! text-only requests fall back to [`GaiseEmbeddingsRequest`] on every client.

use super::{GaiseConnection, GaiseContent, GaiseEmbeddingTask, GaiseEmbeddingsRequest, OneOrMany};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
pub struct GaiseContentEmbeddingsRequest {
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection: Option<GaiseConnection>,
    /// One embedding per item. A `parts` item yields one aggregated embedding
    /// of all its parts (text, image, audio, file).
    pub input: Vec<GaiseContent>,
    /// Same meaning as [`GaiseEmbeddingsRequest::task`]; applied to text-only
    /// items. Providers advise against task prefixes on multimodal items.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task: Option<GaiseEmbeddingTask>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dimensions: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub normalize: Option<bool>,
}

impl GaiseContentEmbeddingsRequest {
    /// The equivalent text request when every item is plain text.
    pub fn text_request(&self) -> Option<GaiseEmbeddingsRequest> {
        let texts = self
            .input
            .iter()
            .map(|item| match item {
                GaiseContent::Text { text } => Some(text.clone()),
                _ => None,
            })
            .collect::<Option<Vec<_>>>()?;
        Some(GaiseEmbeddingsRequest {
            model: self.model.clone(),
            correlation_id: self.correlation_id.clone(),
            connection: self.connection.clone(),
            input: OneOrMany::Many(texts),
            task: self.task.clone(),
            dimensions: self.dimensions,
            normalize: self.normalize,
        })
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.model.trim().is_empty() {
            return Err("Embeddings model must not be empty".into());
        }
        if self.input.is_empty() {
            return Err("Embeddings require at least one input".into());
        }
        Ok(())
    }
}
