//! Media operations: speech-to-text transcription, image generation and
//! editing, and video generation.
//!
//! The shapes are the union of what OpenAI, Google (Gemini API and Vertex
//! AI), Amazon Bedrock, ElevenLabs, and xAI expose, normalised so that one
//! request works across providers. Adapters map what their API supports,
//! reject what it cannot honour, and accept vendor-only knobs through
//! `provider_options` (merged into the wire body as-is).

use serde::{Deserialize, Serialize};

use super::{GaiseConnection, GaiseUsage};

// ── Shared ────────────────────────────────────────────────────────────

/// Media operations a model can be driven through. Kept apart from
/// [`super::GaiseOperation`] so adding them was not a breaking change; see
/// [`crate::registry::RegistryModel::media_operations`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum GaiseMediaOperation {
    /// [`crate::GaiseClient::transcribe`].
    Transcription,
    /// [`crate::GaiseLiveClient::transcribe_connect`].
    LiveTranscription,
    /// [`crate::GaiseClient::generate_images`].
    ImageGeneration,
    /// [`crate::GaiseClient::start_video`] / [`crate::GaiseClient::get_video`].
    VideoGeneration,
}

impl GaiseMediaOperation {
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "transcription" | "transcribe" | "stt" | "speech_to_text" => Some(Self::Transcription),
            "live_transcription" | "realtime_transcription" | "streaming_transcription" => {
                Some(Self::LiveTranscription)
            }
            "image_generation" | "images" | "image" => Some(Self::ImageGeneration),
            "video_generation" | "videos" | "video" => Some(Self::VideoGeneration),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Transcription => "transcription",
            Self::LiveTranscription => "live_transcription",
            Self::ImageGeneration => "image_generation",
            Self::VideoGeneration => "video_generation",
        }
    }
}

/// Media passed into a request: inline bytes or a URL the provider fetches
/// (`https://`, `gs://`, `s3://`, a data URL, or a provider file id URI).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum GaiseMediaSource {
    Bytes {
        #[serde(with = "serde_bytes")]
        data: Vec<u8>,
        /// MIME type (`audio/wav`, `image/png`, `video/mp4`, …). Adapters
        /// sniff common formats when absent.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mime_type: Option<String>,
    },
    Url {
        url: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mime_type: Option<String>,
    },
}

impl GaiseMediaSource {
    pub fn bytes(data: Vec<u8>, mime_type: impl Into<String>) -> Self {
        Self::Bytes {
            data,
            mime_type: Some(mime_type.into()),
        }
    }

    pub fn url(url: impl Into<String>) -> Self {
        Self::Url {
            url: url.into(),
            mime_type: None,
        }
    }

    pub fn mime_type(&self) -> Option<&str> {
        match self {
            Self::Bytes { mime_type, .. } | Self::Url { mime_type, .. } => mime_type.as_deref(),
        }
    }

    /// `data:` URL for inline bytes (`fallback_mime` when the type is unset);
    /// URLs are returned unchanged.
    pub fn to_url(&self, fallback_mime: &str) -> String {
        match self {
            Self::Bytes { data, mime_type } => {
                use base64::Engine;
                format!(
                    "data:{};base64,{}",
                    mime_type.as_deref().unwrap_or(fallback_mime),
                    base64::prelude::BASE64_STANDARD.encode(data)
                )
            }
            Self::Url { url, .. } => url.clone(),
        }
    }
}

/// One generated image or video. At least one of `data` and `url` is set.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GaiseGeneratedMedia {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "optional_bytes"
    )]
    pub data: Option<Vec<u8>>,
    /// Download location (`https://`, `gs://`, `s3://`). Provider URLs may
    /// expire or need the API key (Gemini video URIs do).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    /// The prompt the provider actually used, when it rewrites prompts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revised_prompt: Option<String>,
    /// Video length in seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_seconds: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
}

mod optional_bytes {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(value: &Option<Vec<u8>>, s: S) -> Result<S::Ok, S::Error> {
        match value {
            Some(bytes) => serde_bytes::serialize(bytes, s),
            None => s.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Vec<u8>>, D::Error> {
        Option::<serde_bytes::ByteBuf>::deserialize(d).map(|b| b.map(|b| b.into_vec()))
    }
}

// ── Transcription ─────────────────────────────────────────────────────

/// Timing detail requested from a transcription.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GaiseTimestampGranularity {
    None,
    Segment,
    Word,
    Character,
}

/// `verbatim` keeps fillers and disfluencies; `smart` cleans them up and
/// formats numbers (Gemini SMART, xAI `format`, ElevenLabs `no_verbatim`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GaiseTranscriptionMode {
    Verbatim,
    Smart,
}

/// Headerless audio layout; container formats (wav, mp3, …) are sniffed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GaiseAudioFormat {
    /// `pcm` (16-bit little-endian), `mulaw`, `alaw`, `opus`, or a container
    /// name (`wav`, `mp3`, `ogg`, `flac`, `webm`, `m4a`).
    pub codec: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sample_rate: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channels: Option<u32>,
}

impl GaiseAudioFormat {
    pub fn pcm(sample_rate: u32) -> Self {
        Self {
            codec: "pcm".into(),
            sample_rate: Some(sample_rate),
            channels: None,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GaiseTranscriptionRequest {
    /// `provider::model` through the router, bare model id on a direct client.
    pub model: String,
    pub audio: Option<GaiseMediaSource>,
    /// Expected languages (BCP-47 / ISO-639). Empty = auto-detect.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub languages: Vec<String>,
    /// Free-text context or style hint where supported (OpenAI `prompt`,
    /// ElevenLabs `previous_text`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    /// Vocabulary to bias towards (keyterms / keywords / custom vocabulary).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keyterms: Vec<String>,
    /// Label speakers.
    #[serde(default)]
    pub diarize: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_speakers: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamps: Option<GaiseTimestampGranularity>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<GaiseTranscriptionMode>,
    /// Layout of headerless audio (raw PCM, μ-law, A-law).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<GaiseAudioFormat>,
    /// Transcribe each channel separately.
    #[serde(default)]
    pub multichannel: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// Vendor-only fields merged into the wire request unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_options: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection: Option<GaiseConnection>,
}

impl GaiseTranscriptionRequest {
    pub fn validate(&self) -> Result<(), String> {
        if self.model.trim().is_empty() {
            return Err("transcription request needs a model".into());
        }
        match &self.audio {
            None => Err("transcription request needs audio".into()),
            Some(GaiseMediaSource::Bytes { data, .. }) if data.is_empty() => {
                Err("transcription audio is empty".into())
            }
            _ => Ok(()),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GaiseTranscriptTokenKind {
    #[default]
    Word,
    Spacing,
    /// Non-speech sound such as `(laughter)` (ElevenLabs).
    AudioEvent,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GaiseTranscriptWord {
    pub text: String,
    /// Seconds from the start of the audio.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<f64>,
    /// 0.0–1.0 where reported (log-probabilities are converted).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
    /// Provider speaker label (`A`, `spk_1`, `speaker_0`, `0`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speaker: Option<String>,
    #[serde(default)]
    pub kind: GaiseTranscriptTokenKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GaiseTranscriptSegment {
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speaker: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GaiseTranscriptionChannel {
    pub index: u32,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub words: Vec<GaiseTranscriptWord>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GaiseTranscriptionResponse {
    pub text: String,
    /// Detected language as the provider reports it (`en`, `eng`, `es-mx`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_seconds: Option<f64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub segments: Vec<GaiseTranscriptSegment>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub words: Vec<GaiseTranscriptWord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub channels: Vec<GaiseTranscriptionChannel>,
    /// Token counters, or `audio_milliseconds` for duration-billed models.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<GaiseUsage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_id: Option<String>,
}

// ── Images ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GaiseImageRequest {
    pub model: String,
    pub prompt: String,
    /// Input images. Non-empty turns the request into an edit or a
    /// reference-guided generation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<GaiseMediaSource>,
    /// Inpainting mask (applied to the first image) where supported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mask: Option<GaiseMediaSource>,
    /// Number of images; providers cap it and may return fewer when outputs
    /// are filtered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub n: Option<u32>,
    /// Exact `WIDTHxHEIGHT` (OpenAI). Takes precedence over the two below
    /// where a provider accepts it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<String>,
    /// `16:9`, `1:1`, … (Gemini, xAI, Bedrock Stability).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aspect_ratio: Option<String>,
    /// Resolution tier: `512`, `1K`, `1.5K`, `2K`, `4K` (case-insensitive).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolution: Option<String>,
    /// `low`, `medium`, `high`, `xhigh`, `max`, `auto`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality: Option<String>,
    /// Output MIME type: `image/png`, `image/jpeg`, `image/webp`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_format: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub negative_prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_options: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection: Option<GaiseConnection>,
}

impl GaiseImageRequest {
    pub fn validate(&self) -> Result<(), String> {
        if self.model.trim().is_empty() {
            return Err("image request needs a model".into());
        }
        if self.prompt.trim().is_empty() {
            return Err("image request needs a prompt".into());
        }
        if self.n == Some(0) {
            return Err("image request n must be at least 1".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GaiseImageResponse {
    pub images: Vec<GaiseGeneratedMedia>,
    /// Text the model returned alongside the images (Gemini Nano Banana).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Outputs the provider withheld for safety reasons, when it says so.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filtered: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<GaiseUsage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_id: Option<String>,
}

// ── Video ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GaiseVideoRequest {
    pub model: String,
    /// Optional for image-to-video on some models.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    /// First frame (image-to-video).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<GaiseMediaSource>,
    /// Last frame (interpolation) where supported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_frame: Option<GaiseMediaSource>,
    /// Style or subject references.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reference_images: Vec<GaiseMediaSource>,
    /// Source video to extend or edit where supported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub video: Option<GaiseMediaSource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_seconds: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aspect_ratio: Option<String>,
    /// `480p`, `540p`, `720p`, `1080p`, `4k`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolution: Option<String>,
    /// Generate a soundtrack where the model makes it optional.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generate_audio: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub n: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub negative_prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
    /// Where the provider writes the result (`gs://…` for Vertex, `s3://…`,
    /// required by Bedrock).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_uri: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_options: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection: Option<GaiseConnection>,
}

impl GaiseVideoRequest {
    pub fn validate(&self) -> Result<(), String> {
        if self.model.trim().is_empty() {
            return Err("video request needs a model".into());
        }
        let has_prompt = self.prompt.as_deref().is_some_and(|p| !p.trim().is_empty());
        if !has_prompt && self.image.is_none() && self.video.is_none() {
            return Err("video request needs a prompt, an image, or a video".into());
        }
        Ok(())
    }
}

/// Poll an existing video job.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GaiseVideoJobRequest {
    /// The model the job was started with (`provider::model` through the
    /// router), so the router can find the provider.
    pub model: String,
    /// [`GaiseVideoJob::id`] as returned by `start_video`.
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection: Option<GaiseConnection>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GaiseJobStatus {
    #[default]
    Queued,
    Running,
    Completed,
    Failed,
}

impl GaiseJobStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Failed)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GaiseVideoJob {
    /// Opaque job handle (xAI request id, Google operation name, Bedrock
    /// invocation ARN). Pass it back unchanged to `get_video`.
    pub id: String,
    pub model: String,
    pub status: GaiseJobStatus,
    /// 0–100 where reported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<u8>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub videos: Vec<GaiseGeneratedMedia>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filtered: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<GaiseUsage>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_source_round_trips_and_builds_data_urls() {
        let src = GaiseMediaSource::bytes(vec![1, 2, 3], "image/png");
        let json = serde_json::to_value(&src).unwrap();
        assert_eq!(json["type"], "bytes");
        assert_eq!(src.to_url("image/jpeg"), "data:image/png;base64,AQID");
        let url: GaiseMediaSource =
            serde_json::from_value(serde_json::json!({"type": "url", "url": "gs://b/a.mp4"}))
                .unwrap();
        assert_eq!(url.to_url("video/mp4"), "gs://b/a.mp4");
    }

    #[test]
    fn generated_media_serializes_compactly() {
        let media = GaiseGeneratedMedia {
            data: Some(vec![9]),
            mime_type: Some("image/png".into()),
            ..Default::default()
        };
        let json = serde_json::to_value(&media).unwrap();
        assert_eq!(json["data"], serde_json::json!([9]));
        assert!(json.get("url").is_none());
        let back: GaiseGeneratedMedia = serde_json::from_value(json).unwrap();
        assert_eq!(back, media);
        let empty: GaiseGeneratedMedia =
            serde_json::from_value(serde_json::json!({"url": "https://x"})).unwrap();
        assert!(empty.data.is_none());
    }

    #[test]
    fn validation() {
        assert!(GaiseTranscriptionRequest::default().validate().is_err());
        let ok = GaiseTranscriptionRequest {
            model: "m".into(),
            audio: Some(GaiseMediaSource::url("https://a/b.wav")),
            ..Default::default()
        };
        assert!(ok.validate().is_ok());
        assert!(
            GaiseImageRequest {
                model: "m".into(),
                ..Default::default()
            }
            .validate()
            .is_err()
        );
        assert!(
            GaiseVideoRequest {
                model: "m".into(),
                ..Default::default()
            }
            .validate()
            .is_err()
        );
        assert!(GaiseJobStatus::Failed.is_terminal());
    }
}
