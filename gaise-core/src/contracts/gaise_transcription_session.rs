//! Streaming speech-to-text sessions: audio in, partial and final
//! transcripts out.
//!
//! The shape covers ElevenLabs Scribe realtime, xAI `/v1/stt`, Gemini
//! transcribe-live, and OpenAI transcription sessions. Partial results
//! *replace* the previous partial for the same utterance; a final result
//! locks it.

use std::pin::Pin;

use futures_util::Stream;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use super::{
    GaiseAudioFormat, GaiseConnection, GaiseTimestampGranularity, GaiseTranscriptWord, GaiseUsage,
};

/// How the provider decides an utterance is finished.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum GaiseCommitStrategy {
    /// Silence-based voice activity detection.
    Vad,
    /// Model-predicted end of turn (ElevenLabs `turn_prediction`, xAI
    /// `smart_turn`); falls back to VAD where unsupported.
    TurnPrediction,
    /// Only [`GaiseTranscriptionInput::Commit`] finalizes text.
    Manual,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GaiseTranscriptionSessionConfig {
    /// `provider::model` through the router, bare model id on a direct client.
    pub model: String,
    /// Input audio layout. Defaults to 16 kHz mono PCM16 where unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio_format: Option<GaiseAudioFormat>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub languages: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keyterms: Vec<String>,
    /// Text spoken before this session, as context (ElevenLabs).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(default)]
    pub diarize: bool,
    /// Emit non-final (partial) transcripts.
    #[serde(default)]
    pub interim_results: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamps: Option<GaiseTimestampGranularity>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit_strategy: Option<GaiseCommitStrategy>,
    /// Trailing silence that ends an utterance under VAD.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub silence_duration_ms: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_options: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection: Option<GaiseConnection>,
}

impl GaiseTranscriptionSessionConfig {
    /// The configured format, or 16 kHz PCM16.
    pub fn audio_format_or_default(&self) -> GaiseAudioFormat {
        self.audio_format
            .clone()
            .unwrap_or_else(|| GaiseAudioFormat::pcm(16_000))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum GaiseTranscriptionInput {
    /// A chunk of audio in the session's `audio_format`.
    Audio {
        #[serde(with = "serde_bytes")]
        data: Vec<u8>,
    },
    /// Finalize the current utterance now (push-to-talk, manual commit).
    Commit,
    /// Flush remaining audio, wait for the last transcript, and end.
    Close,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GaiseLiveTranscript {
    pub text: String,
    /// `false`: may still change and replaces the previous partial.
    pub is_final: bool,
    /// The speaker finished their utterance (end of turn).
    #[serde(default)]
    pub utterance_end: bool,
    /// Seconds from the session start.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speaker: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel: Option<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub words: Vec<GaiseTranscriptWord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum GaiseTranscriptionEvent {
    Started {
        session_id: String,
        model: String,
    },
    Transcript(GaiseLiveTranscript),
    SpeechStarted,
    SpeechStopped,
    Usage(GaiseUsage),
    /// Recoverable problem; fatal errors are followed by `Ended`.
    Error {
        message: String,
    },
    Ended {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
}

pub type GaiseTranscriptionEventStream = Pin<
    Box<
        dyn Stream<Item = Result<GaiseTranscriptionEvent, Box<dyn std::error::Error + Send + Sync>>>
            + Send,
    >,
>;

pub struct GaiseTranscriptionSession {
    pub tx: mpsc::Sender<GaiseTranscriptionInput>,
    pub rx: GaiseTranscriptionEventStream,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_serialize_with_type_tags() {
        let event = GaiseTranscriptionEvent::Transcript(GaiseLiveTranscript {
            text: "hi".into(),
            is_final: true,
            ..Default::default()
        });
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "transcript");
        assert_eq!(json["is_final"], true);
        let input: GaiseTranscriptionInput =
            serde_json::from_value(serde_json::json!({"type": "commit"})).unwrap();
        assert!(matches!(input, GaiseTranscriptionInput::Commit));
        let ended: GaiseTranscriptionEvent =
            serde_json::from_value(serde_json::json!({"type": "ended"})).unwrap();
        assert!(matches!(ended, GaiseTranscriptionEvent::Ended { reason: None }));
    }
}
