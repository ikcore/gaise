use super::OneOrMany;
use serde::{Deserialize, Serialize};

/// What the vectors are for. Providers that distinguish document and query
/// embeddings (Gemini/Vertex `taskType`, Cohere `input_type`, Nova
/// `embeddingPurpose`) produce measurably better retrieval when told; providers
/// without the concept ignore it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GaiseEmbeddingTask {
    /// Text being indexed for later retrieval (the default on every provider).
    Document,
    /// A search query to match against indexed documents.
    Query,
    Classification,
    Clustering,
    /// Symmetric similarity between two texts.
    Similarity,
    /// A natural-language query for code retrieval.
    CodeQuery,
    FactVerification,
    QuestionAnswering,
}

impl GaiseEmbeddingTask {
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().replace('-', "_").as_str() {
            "document" | "retrieval_document" | "search_document" | "index" | "passage" => {
                Some(Self::Document)
            }
            "query" | "retrieval_query" | "search_query" | "search" => Some(Self::Query),
            "classification" | "classify" => Some(Self::Classification),
            "clustering" | "cluster" => Some(Self::Clustering),
            "similarity" | "semantic_similarity" | "sts" => Some(Self::Similarity),
            "code_query" | "code_retrieval_query" | "code" => Some(Self::CodeQuery),
            "fact_verification" | "fact" => Some(Self::FactVerification),
            "question_answering" | "qa" => Some(Self::QuestionAnswering),
            _ => None,
        }
    }
}

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
pub struct GaiseEmbeddingsRequest {
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,
    /// Per-request provider endpoint/credential overrides (take precedence
    /// over the router configuration for this call only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection: Option<super::GaiseConnection>,
    pub input: OneOrMany<String>,
    /// Intended use of the vectors. Mapped to the provider's task/input type
    /// where one exists; embed queries with `Query` and corpus text with
    /// `Document` when the model distinguishes them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task: Option<GaiseEmbeddingTask>,
    /// Requested vector length for models with selectable (Matryoshka)
    /// dimensions. Snapped to the nearest size the model offers; ignored by
    /// fixed-size models. Truncated vectors are re-normalized only when the
    /// provider does so or `normalize` is set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dimensions: Option<u32>,
    /// Ask for unit-length (L2-normalized) vectors. Sent natively where the
    /// provider has a flag (Titan V2); otherwise applied locally by the
    /// adapter so cosine similarity and dot product agree.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub normalize: Option<bool>,
}

impl GaiseEmbeddingTask {
    /// Google's documented prompt convention for models that take the task as
    /// an instruction instead of a `taskType` field (`gemini-embedding-2` on
    /// the Gemini API and Vertex AI). Documents carry `title: none | text: …`,
    /// queries `task: search result | query: …`, and so on.
    pub fn gemini_instruction(self, text: &str) -> String {
        match self {
            Self::Document => format!("title: none | text: {text}"),
            Self::Query => format!("task: search result | query: {text}"),
            Self::QuestionAnswering => format!("task: question answering | query: {text}"),
            Self::FactVerification => format!("task: fact checking | query: {text}"),
            Self::CodeQuery => format!("task: code retrieval | query: {text}"),
            Self::Classification => format!("task: classification | text: {text}"),
            Self::Clustering => format!("task: clustering | text: {text}"),
            Self::Similarity => format!("task: sentence similarity | text: {text}"),
        }
    }
}

/// L2-normalize a vector in place (no-op for the zero vector).
pub fn normalize_l2(vector: &mut [f32]) {
    let norm = vector.iter().map(|v| (*v as f64) * (*v as f64)).sum::<f64>().sqrt();
    if norm > 0.0 {
        for v in vector.iter_mut() {
            *v = (*v as f64 / norm) as f32;
        }
    }
}

/// Snap a requested dimension count onto a model's supported set (nearest,
/// ties resolve upward). Returns `None` when the model has no choice.
pub fn snap_dimensions(requested: u32, supported: &[u32]) -> Option<u32> {
    if supported.is_empty() {
        return None;
    }
    supported
        .iter()
        .copied()
        .min_by_key(|d| (d.abs_diff(requested), u32::from(*d < requested)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_task_aliases() {
        assert_eq!(GaiseEmbeddingTask::parse("search_query"), Some(GaiseEmbeddingTask::Query));
        assert_eq!(GaiseEmbeddingTask::parse("RETRIEVAL_DOCUMENT"), Some(GaiseEmbeddingTask::Document));
        assert_eq!(GaiseEmbeddingTask::parse("semantic-similarity"), Some(GaiseEmbeddingTask::Similarity));
        assert_eq!(GaiseEmbeddingTask::parse("nope"), None);
        assert_eq!(serde_json::to_string(&GaiseEmbeddingTask::CodeQuery).unwrap(), "\"code_query\"");
        assert_eq!(GaiseEmbeddingTask::Query.gemini_instruction("x"), "task: search result | query: x");
        assert_eq!(GaiseEmbeddingTask::Document.gemini_instruction("x"), "title: none | text: x");
    }

    #[test]
    fn normalizes_and_snaps() {
        let mut v = vec![3.0, 4.0];
        normalize_l2(&mut v);
        assert!((v[0] - 0.6).abs() < 1e-6 && (v[1] - 0.8).abs() < 1e-6);
        let mut zero = vec![0.0, 0.0];
        normalize_l2(&mut zero);
        assert_eq!(zero, vec![0.0, 0.0]);

        assert_eq!(snap_dimensions(300, &[256, 512, 1024]), Some(256));
        assert_eq!(snap_dimensions(384, &[256, 512, 1024]), Some(512), "tie resolves upward");
        assert_eq!(snap_dimensions(4096, &[256, 512, 1024]), Some(1024));
        assert_eq!(snap_dimensions(768, &[]), None);
    }
}
