# Media: transcription, images, and video

> Part of the [GAISe wiki](README.md) · [HTTP API](api.md#media) · [Rust SDK](sdk.md) · [Capabilities](capabilities.md) · [Models](models.md)

GAISe 5.5 adds four provider-neutral operations. Each one is a default method that returns "not supported" until a provider overrides it, so existing custom clients keep compiling:

| Operation | Rust | HTTP | Providers |
|---|---|---|---|
| Speech-to-text | `GaiseClient::transcribe` | `POST /v1/transcriptions` | OpenAI, Gemini API, Vertex AI, ElevenLabs, xAI |
| Streaming speech-to-text | `GaiseLiveClient::transcribe_connect` (feature `live`) | `GET /v1/transcriptions/live` (WebSocket) | ElevenLabs, xAI |
| Image generation and editing | `GaiseClient::generate_images` | `POST /v1/images` | OpenAI, Gemini API, Vertex AI, Bedrock, xAI |
| Video generation (async job) | `GaiseClient::start_video` / `get_video`, router helper `generate_video_and_wait` | `POST /v1/videos`, `GET /v1/videos/{id}?model=` | Vertex AI (Veo 3.1), Bedrock (Luma Ray 2), xAI |

The contracts live in [`gaise_media.rs`](../gaise-core/src/contracts/gaise_media.rs) and [`gaise_transcription_session.rs`](../gaise-core/src/contracts/gaise_transcription_session.rs). Anthropic, Ollama, and TypeSafe have no media APIs.

Several things are unavailable upstream:
- OpenAI shut down the Videos API and Sora on 2026-09-24.
- Imagen is shut down on both Google surfaces.
- The Gemini API's Veo previews end on 2026-10-22. Their replacement, Gemini Omni, uses the Interactions API, which GAISe does not call.
- Amazon Nova Canvas and Nova Reel reached end of life on 2026-09-30.

The registry marks which models support each operation with the capability terms `transcription`, `live_transcription`, `image_generation`, and `video_generation`. Read them with `RegistryModel::media_operations()`. The generated model tables show them as `T`, `W`, `P`, and `M`. They are kept out of `GaiseOperation` so that adding them was not a breaking change.

## Shared types

| Type | Shape |
|---|---|
| `GaiseMediaSource` | `{"type": "bytes", "data": [..], "mime_type": "audio/wav"}` or `{"type": "url", "url": "https://… / gs://… / s3://… / data:…"}` |
| `GaiseGeneratedMedia` | `data` (bytes) and/or `url`, plus `mime_type`, `revised_prompt`, `duration_seconds`, and `seed` |
| `provider_options` | A JSON object on every request. Its keys are merged into the provider's wire body for vendor-only settings such as OpenAI `background`, Bedrock Stability `strength`, Vertex `instance.cameraControl`, or Luma `loop`. |

Adapters return an error that names any field the provider cannot honour. They never drop it silently.

## Transcription

**`GaiseTranscriptionRequest` fields:**
- `model` and `audio`.
- `languages`. Leave it empty to auto-detect.
- `prompt` and `keyterms`.
- `diarize` and `max_speakers`.
- `timestamps`: `none`, `segment`, `word`, or `character`.
- `mode`: `verbatim` or `smart`.
- `format`, for headerless audio: `{codec: pcm|mulaw|alaw|opus, sample_rate, channels}`.
- `multichannel` and `temperature`.

**`GaiseTranscriptionResponse` fields:**
- `text`, `language` and `duration_seconds`.
- `segments`, each with text, start, end, speaker and channel.
- `words`, each with text, start, end, `confidence` (0–1), speaker, `kind` (word, spacing or audio_event) and channel.
- `channels`.
- `usage`: tokens, or `input.audio_milliseconds` for models billed by duration.

| Provider | Endpoint | Notable rules |
|---|---|---|
| OpenAI | `POST /v1/audio/transcriptions` | **gpt-transcribe** takes `languages[]` and `keywords[]`. **gpt-4o-transcribe-diarize** is needed for diarization. **whisper-1** is needed for word and segment timestamps. Audio must be inline bytes, 25 MB at most. |
| Gemini API / Vertex | `generateContent` with `audioTranscriptionConfig` (`gemini-3.5-transcribe`) | Custom vocabulary cannot be combined with timestamps. On the Gemini API it also cannot be combined with diarization. Smart mode cannot be combined with diarization or timestamps. |
| ElevenLabs | `POST /v1/speech-to-text` (`scribe_v2`, `scribe_v2_medical`) | Up to 32 speakers, word or character timestamps, audio events, multichannel. |
| xAI | `POST /v1/stt` (`grok-voice-transcribe-2.0`) | Diarization, word timestamps, keyterms, multichannel, raw pcm/mulaw/alaw. |

## Streaming transcription

`transcribe_connect(&GaiseTranscriptionSessionConfig)` returns a `GaiseTranscriptionSession { tx, rx }`.

**Config:**
- `model` and `audio_format` (16 kHz PCM16 by default).
- `languages`, `keyterms` and `prompt`.
- `diarize`, `interim_results` and `timestamps`.
- `commit_strategy` (`vad`, `turn_prediction` or `manual`) and `silence_duration_ms`.

**Inputs:**
- `Audio{data}`.
- `Commit`, which finalizes the current utterance.
- `Close`, which flushes the remaining audio and ends the session.

**Events:**
- `Started`.
- `Transcript(GaiseLiveTranscript)`.
  - `is_final: false` is a partial, which replaces the previous partial.
  - `utterance_end` marks the end of a turn.
  - It also carries timings, speaker, language, channel and words.
- `SpeechStarted` / `SpeechStopped`, `Usage`, and `Error`.
- `Ended{reason}`.

**Providers:**
- **ElevenLabs:** `wss://…/v1/speech-to-text/realtime` with `scribe_v2_realtime`. There is no diarization.
- **xAI:** `wss://api.x.ai/v1/stt`. The `manual` commit strategy is rejected, because xAI always ends utterances on silence.

OpenAI transcription sessions and Gemini `gemini-3.5-transcribe-live` are not mapped yet. Their WebSocket details could not be confirmed.

Over HTTP, `GET /v1/transcriptions/live` works like this:
1. Send the config as the first text frame.
2. Send audio as binary frames, and `{"type":"commit"}` or `{"type":"close"}` as text frames.
3. Every event comes back as a JSON text frame.

## Image generation

**`GaiseImageRequest` fields:**
- `model` and `prompt`.
- `images`: input images. Supplying any makes the request an edit, or a reference-guided generation.
- `mask` and `n`.
- `size`: exact `WxH`, for OpenAI.
- `aspect_ratio` and `resolution` (`512`, `1K`, `1.5K`, `2K`, `4K`).
- `quality`, `output_format` (a MIME type), `negative_prompt` and `seed`.

**`GaiseImageResponse` fields:**
- `images`.
- `text`: what the model wrote alongside the images (Nano Banana).
- `filtered`: the number of outputs withheld for safety.
- `usage`.

| Provider | Endpoint | Notes |
|---|---|---|
| OpenAI | `/v1/images/generations`, or the JSON `/v1/images/edits` body | gpt-image-2 and 2.5. Up to 16 input images plus a mask. Aspect ratios 1:1, 3:2 and 2:3 map to the standard sizes. No seed or negative prompt. |
| Gemini API / Vertex | `generateContent`, `responseModalities: [TEXT, IMAGE]`, `imageConfig` | Nano Banana 2.1, Pro, Lite, 3.1 Flash Image and 2.5 Flash Image. Up to 14 input images. `n > 1` is rejected. |
| Bedrock | `InvokeModel` (Stability AI) | SD3.5 Large and Stable Image Ultra support image-to-image; Stable Image Core is text-only. `n` loops sequential calls. Filtered outputs are counted. |
| xAI | `/v1/images/generations` or `/edits` | Grok Imagine. Resolution `1k`, `1.5k` or `2k`. Quality `low`, `medium` or `auto` (2.0 only). |

## Video generation

`start_video(&GaiseVideoRequest)` returns a `GaiseVideoJob` with status `queued`. Poll it with `get_video(&GaiseVideoJobRequest { model, id })` until `status` is `completed` or `failed`. The router helper `generate_video_and_wait(request, poll_interval, timeout)` does the polling for you.

**`GaiseVideoRequest` fields:**
- `prompt`.
- `image`: the first frame.
- `last_frame` and `reference_images`.
- `video`: a source video to extend or edit.
- `duration_seconds`, `aspect_ratio`, `resolution`, `generate_audio`, `n`, `negative_prompt` and `seed`.
- `output_uri`: where the provider writes the result.

**`GaiseVideoJob` fields:**
- `id`: an opaque handle. Pass it back unchanged; over HTTP, URL-encode it.
- `status` and `progress`.
- `videos`, `error`, `filtered` and `usage`.

| Provider | Start / poll | Notes |
|---|---|---|
| Vertex AI | `:predictLongRunning` / `:fetchPredictOperation` | Veo 3.1 and Fast (GA), Lite (preview). Up to 3 asset references, extension, `sampleCount` 1–4, optional `gs://` `storageUri`. The job id is the operation name. |
| Bedrock | `StartAsyncInvoke` / `GetAsyncInvoke` | Luma Ray 2. Requires an `s3://` `output_uri`. 5 s or 9 s, at 540p or 720p. The job id is the invocation ARN. |
| xAI | `/v1/videos/generations`, `/edits`, `/extensions` / `GET /v1/videos/{id}` | Grok Imagine Video. A source video goes to edits, or to extensions when `duration_seconds` is also set. Video URLs are returned. |

## Not verified

None of the media paths has been run against a live provider API; every test is hermetic. Each vendor page lists its own open questions. The main ones are:
- the Luma output object path;
- whether xAI's REST speech-to-text takes a `model` field;
- whether ElevenLabs accepts repeated `keyterms` fields;
- whether Gemini accepts `audio/pcm;rate=N` for inline audio.
