//! xAI text-to-speech contract (`POST /v1/tts`).
//!
//! The endpoint has no model field: one TTS engine serves every voice.
//! GAISe routes it as `xai::tts` and ignores the model name.

use gaise_core::contracts::{GaiseSpeechAlignment, GaiseSpeechRequest};
use serde::{Deserialize, Serialize};

/// Voice used when the request names none (xAI's own default).
pub const DEFAULT_VOICE: &str = "eve";

/// PCM / WAV / MP3 sample rates the TTS endpoint accepts.
pub const TTS_SAMPLE_RATES: &[u32] = &[8000, 16000, 22050, 24000, 44100, 48000];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct XaiTtsRequest {
    pub text: String,
    /// BCP-47 code or `auto`; required upstream.
    pub language: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voice_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_format: Option<XaiTtsOutputFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speed: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub with_timestamps: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct XaiTtsOutputFormat {
    /// `mp3`, `wav`, `pcm`, `mulaw`, or `alaw`.
    pub codec: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sample_rate: Option<u32>,
}

/// JSON envelope returned when `with_timestamps` is set.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XaiTtsJsonResponse {
    /// Base64-encoded audio in the requested codec.
    pub audio: String,
    #[serde(default)]
    pub content_type: Option<String>,
    #[serde(default)]
    pub duration: Option<f64>,
    #[serde(default)]
    pub audio_timestamps: Option<XaiTtsTimestamps>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XaiTtsTimestamps {
    #[serde(default)]
    pub graph_chars: Vec<String>,
    #[serde(default)]
    pub graph_times: Vec<XaiTtsTime>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XaiTtsTime {
    pub start: f64,
    pub end: f64,
}

impl From<&XaiTtsTimestamps> for GaiseSpeechAlignment {
    fn from(t: &XaiTtsTimestamps) -> Self {
        GaiseSpeechAlignment {
            characters: t.graph_chars.clone(),
            start_seconds: t.graph_times.iter().map(|t| t.start).collect(),
            end_seconds: t.graph_times.iter().map(|t| t.end).collect(),
        }
    }
}

/// The resolved codec, the MIME type returned to callers, and the PCM rate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XaiTtsFormat {
    pub codec: &'static str,
    pub mime: &'static str,
    pub sample_rate: u32,
}

/// Map the provider-neutral `format` + `sample_rate` onto an xAI codec.
/// Defaults to 24 kHz MP3, the endpoint's own default.
pub fn resolve_tts_format(
    format: Option<&str>,
    sample_rate: Option<u32>,
) -> Result<XaiTtsFormat, String> {
    let (codec, mime) = match format
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "" | "mp3" | "mpeg" | "audio/mp3" | "audio/mpeg" => ("mp3", "audio/mpeg"),
        "wav" | "wave" | "audio/wav" | "audio/wave" | "audio/x-wav" => ("wav", "audio/wav"),
        "pcm" | "audio/pcm" | "audio/l16" | "pcm16" => ("pcm", "audio/pcm"),
        "mulaw" | "ulaw" | "pcmu" | "audio/basic" | "audio/pcmu" | "audio/mulaw" => {
            ("mulaw", "audio/basic")
        }
        "alaw" | "pcma" | "audio/pcma" | "audio/alaw" => ("alaw", "audio/alaw"),
        other => {
            return Err(format!(
                "xAI text-to-speech supports mp3, wav, pcm, mulaw, and alaw output, not '{other}'"
            ));
        }
    };
    let sample_rate = match codec {
        "mulaw" | "alaw" => 8000,
        _ => sample_rate.unwrap_or(24_000),
    };
    if !TTS_SAMPLE_RATES.contains(&sample_rate) {
        return Err(format!(
            "xAI text-to-speech does not support a {sample_rate} Hz sample rate (supported: {TTS_SAMPLE_RATES:?})"
        ));
    }
    Ok(XaiTtsFormat {
        codec,
        mime,
        sample_rate,
    })
}

/// Build the `POST /v1/tts` body. `with_timestamps` is only sent when
/// alignment is requested, because it switches the response to JSON.
pub fn build_tts_request(
    request: &GaiseSpeechRequest,
) -> Result<(XaiTtsRequest, XaiTtsFormat), String> {
    if request.input.trim().is_empty() {
        return Err("xAI text-to-speech needs non-empty input text".to_string());
    }
    if request.input.chars().count() > 60_000 {
        return Err("xAI text-to-speech accepts at most 60,000 characters".to_string());
    }
    let format = resolve_tts_format(request.format.as_deref(), request.sample_rate)?;
    let speed = request
        .voice_settings
        .as_ref()
        .and_then(|v| v.speed)
        .map(|s| s.clamp(0.7, 1.5));
    Ok((
        XaiTtsRequest {
            text: request.input.clone(),
            language: request
                .language
                .clone()
                .filter(|l| !l.trim().is_empty())
                .unwrap_or_else(|| "auto".to_string()),
            voice_id: request.voice.clone().filter(|v| !v.trim().is_empty()),
            output_format: Some(XaiTtsOutputFormat {
                codec: format.codec.to_string(),
                sample_rate: Some(format.sample_rate),
            }),
            speed,
            with_timestamps: request.include_alignment.then_some(true),
        },
        format,
    ))
}
