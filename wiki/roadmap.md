# Roadmap

> Part of the [GAISe wiki](README.md) · [Media](media.md) · [Changelog](../CHANGELOG.md)

This page lists what is planned after 5.5.0 (released 2026-10-17). Items are grouped by the release they need: anything that would break callers waits for 6.0.

## 1. Verify against live APIs (before anything else)

Every path added in 5.0 and 5.5 has only been tested against local mocks. The first job is a smoke test with real keys. Each item below should become an `#[ignore]` live test, like OpenAI's `live_tool_round_trip`.

- **xAI** (`XAI_API_KEY`):
  - Chat, streaming, and tools on `grok-4.7`.
  - `xai::tts`.
  - `grok-voice-latest` speech-to-speech.
  - `/v1/stt`, both batch and WebSocket. Confirm that the REST endpoint accepts the `model` form field (the guide shows it; the reference does not).
  - Imagine image and video, including which source-video requests go to edits and which to extensions.
- **OpenAI:**
  - `gpt-transcribe`: which `response_format` values it accepts, and whether it takes `temperature`.
  - The JSON image edit body.
  - Whether the 2.5 image models accept `input_fidelity`.
- **Gemini / Vertex:**
  - Whether `gemini-3.5-transcribe` works through `generateContent` with `audioTranscriptionConfig` (the guide only shows the Interactions API).
  - Whether inline audio accepts `audio/pcm;rate=N`.
  - Veo `resolution: "4k"`.
  - Inline Veo bytes when `storageUri` is omitted.
  - Whether the Vertex client's 120 s timeout is long enough for 4K images and long audio.
- **Bedrock:**
  - The Luma Ray 2 output object path (`{s3Uri}/output.mp4` is inferred from the Nova Reel docs).
  - The Stable Image Ultra and Core request shapes and `output_format` casing.
  - The `stability.stable-image-core-v1:1` model id.
- **ElevenLabs:**
  - Whether repeated `keyterms` multipart fields are the right encoding.
  - Realtime: whether `committed_transcript_with_timestamps` arrives alongside or instead of `committed_transcript`.
  - Realtime: whether an empty commit chunk is accepted.
  - Which realtime errors are fatal.

## 2. Additive work (5.x minor releases)

- **Streaming transcription for more providers:**
  - OpenAI transcription sessions (`gpt-live-transcribe`, `gpt-realtime-whisper`). The connect URL was unconfirmed on 2026-10-17.
  - Gemini `gemini-3.5-transcribe-live` through the Live API.
- **OpenAI GPT-Live 1** (`wss://api.openai.com/v1/live/sessions`). It is a full-duplex protocol: no turn or interrupt events, and tools run through `delegation` (Responses or Decisions) plus transcript triggers. It can probably be added as a separate client on the existing `GaiseLiveClient` trait. Its model rows are catalogued but not driven.
- **xAI Responses API:**
  - Encrypted reasoning round-trip.
  - Server-side tools (`web_search`, `x_search`, code execution, collections, MCP).
  - The `grok-4.20-multi-agent` model, which fails fast today.
- **Google Interactions API:** Gemini Omni video (`gemini-omni-1.1-flash`), which replaces the Veo previews that shut down 2026-10-22.
- **Media extras:**
  - OpenAI image streaming (`partial_images`) and transcription streaming (`transcript.text.delta`).
  - xAI deferred images and Files API inputs.
  - ElevenLabs `secondary_languages`, entity detection and redaction.
  - Webhook-based jobs.
- **Media in model listings:** surface `media_operations` on `GET /v1/models` (for example through a query flag) without adding a field to `GaiseModelCapabilities`.
- **Ephemeral client tokens** for browser sessions: xAI `/v1/realtime/client_secrets`, ElevenLabs single-use tokens, Gemini constrained tokens.
- **Text-to-speech WebSockets:** xAI `wss://api.x.ai/v1/tts`. OpenAI TTS has no GAISe surface yet.

## 3. Breaking changes (6.0)

These were left out of 5.5 to keep it additive:

- **Live contract:**
  - `GaiseLiveConfig` gains a session kind, input and output audio formats, richer transcription options (languages, keyterms, diarization, interim results), a VAD mode (server, semantic, turn-prediction, manual), an idle timeout, and a resume handle.
  - `GaiseLiveEvent` gains `SpeechStarted`, `SpeechStopped`, `UtteranceEnd`, `GoAway{time_left_ms}`, `ResumptionHandle{handle, resumable}`, and `IdleTimeout`. `Transcript` gains `is_final`, speaker, timings, and words, and `SessionEnded` gains a `reason`.
  - `GaiseLiveInput` gains `Commit` and `Truncate{played_ms}`.
  - Make `GaiseLiveEvent` and `GaiseLiveInput` `#[non_exhaustive]`.
  - Stop mapping OpenAI `input_audio_buffer.speech_started` to `Interrupted`; it should become `SpeechStarted`.
- **Make the operation enum extensible:** make `GaiseOperation` `#[non_exhaustive]` and fold the media operations (`Transcription`, `ImageGeneration`, `VideoGeneration`) into it, retiring the separate `GaiseMediaOperation` list.
- **Long-standing next-major candidates:**
  - TypeSafe Box questions and a `GaiseAnswer::Refusal` variant (see the decision notes).
  - Drop the deprecated `system_one` names.
- **Configuration:** fix `gaise-api --no-default-features`, which fails because `main.rs` gates provider config fields on the API's own features while `gaise-client` keeps its defaults on.

## 4. Housekeeping

- Add xAI as its own research group in the `model-release` skill and `.claude/agents/model-audit.md` (docs: `https://docs.x.ai/llms.txt`, model pages under `docs.x.ai/developers/models/`, release notes, migration guides).
- Mirror 5.0 and 5.5 into the .NET port (`../gaise.net`), which is at 4.2.0.
