# ElevenLabs (`elevenlabs`)

> Part of the [GAISe wiki](README.md) · [Models](models.md#elevenlabs) · [Capabilities](capabilities.md) · [HTTP API](api.md) · [Rust SDK](sdk.md) · [Flows](flows.md) · [Examples](examples.md)

The `gaise-provider-elevenlabs` crate drives ElevenLabs **text-to-speech**: complete clips (`speech`), chunked audio as it is rendered (`speech_stream`), realtime voice over WebSocket (`live_connect`, feature `live`), and model discovery (`list_models`). Since 5.5.0 it also wraps **Scribe speech-to-text**: recorded audio through `transcribe` (`POST /v1/speech-to-text`) and streaming audio through `transcribe_connect` (`wss://…/v1/speech-to-text/realtime`, feature `live`). It deliberately has no chat or embeddings surface — `instruct` and `embeddings` return explicit "not supported" errors — and it does not wrap speech-to-speech, music, or Agents. Enabled by the `elevenlabs` feature of [`gaise-client`](../gaise-client/Cargo.toml) (on by default).

## At a glance

| Crate | Feature flag | Client type | Speech surface | Streaming surface | Live surface | Model listing surface |
|---|---|---|---|---|---|---|
| [`gaise-provider-elevenlabs`](../gaise-provider-elevenlabs/Cargo.toml) | `elevenlabs` in [`gaise-client`](../gaise-client/Cargo.toml); crate-level `live` for WebSockets | [`GaiseClientElevenLabs`](../gaise-provider-elevenlabs/src/elevenlabs_client.rs) | `POST /v1/text-to-speech/{voice_id}` and `/with-timestamps` | `POST …/stream` (chunked bytes) and `…/stream/with-timestamps` (NDJSON) | `wss://…/v1/text-to-speech/{voice_id}/stream-input`; `wss://…/v1/text-to-dialogue/stream-input` for `eleven_v3*` | `GET /v1/models` (+ `GET /v2/voices` helper) |

Speech-to-text: `transcribe` → `POST /v1/speech-to-text` (multipart); `transcribe_connect` (feature `live`) → `wss://…/v1/speech-to-text/realtime`.

## Configuration

| Variable | Read by | Default | Purpose |
|---|---|---|---|
| `ELEVENLABS_API_KEY` | [`gaise-api/src/main.rs`](../gaise-api/src/main.rs) → `GaiseClientConfig.elevenlabs_api_key` | none; `get_speech_client("elevenlabs")` errors with "ElevenLabs API Key not configured" | Sent as the `xi-api-key` header on every HTTP request and WebSocket handshake |
| `ELEVENLABS_API_URL` | `GaiseClientConfig.elevenlabs_api_url` | `https://api.elevenlabs.io` ([`DEFAULT_API_URL`](../gaise-provider-elevenlabs/src/elevenlabs_client.rs)) | Base URL; regional residency hosts (`api.eu.residency.elevenlabs.io`, `api.in.…`, `api.sg.…`, `api.us.elevenlabs.io`) work unchanged, and `https://` is rewritten to `wss://` for live sessions |

`configured_providers()` lists `elevenlabs` only when the key is set, so aggregate model listings never wait on it otherwise.

```rust
use gaise_provider_elevenlabs::elevenlabs_client::GaiseClientElevenLabs;

let client = GaiseClientElevenLabs::new(
    "https://api.elevenlabs.io".to_string(),
    std::env::var("ELEVENLABS_API_KEY")?,
);
let voices = client.list_voices().await?; // GET /v2/voices, paginated
```

The crate reads no other environment variables.

## Request mapping

### Voice selection

Every speech request needs a voice id. It is taken from [`GaiseSpeechRequest.voice`](sdk.md#speech) (or `GaiseLiveConfig.voice` for live sessions) and travels in the URL path. There is **no default voice**: ElevenLabs' premade voices are account-gated and retire on 2026-12-31, so [`voice_for`](../gaise-provider-elevenlabs/src/elevenlabs_client.rs) returns an explicit error instead of guessing. Discover ids with [`list_voices`](../gaise-provider-elevenlabs/src/elevenlabs_client.rs).

### Output format

[`resolve_output_format`](../gaise-provider-elevenlabs/src/contracts/models.rs) maps the provider-neutral `format` + `sample_rate` onto an ElevenLabs `output_format` query parameter and records the MIME type returned to callers:

| `format` | `sample_rate` | `output_format` | Returned MIME |
|---|---|---|---|
| _none_ / `audio/mpeg` / `mp3` | _none_ or other | `mp3_44100_128` | `audio/mpeg` |
| `audio/mpeg` | `22050` / `24000` | `mp3_22050_32` / `mp3_24000_48` | `audio/mpeg` |
| `audio/pcm` / `pcm` | 8000–48000 (default 24000) | `pcm_{rate}` | `audio/pcm` |
| `audio/wav` / `wav` | as PCM | `wav_{rate}` | `audio/wav` |
| `audio/opus` / `opus` | — | `opus_48000_64` | `audio/opus` |
| `audio/basic` / `ulaw` | — | `ulaw_8000` | `audio/basic` |
| `alaw` | — | `alaw_8000` | `audio/alaw` |
| provider-native (`pcm_16000`, `mp3_44100_192`, …) | — | passed through | derived from the codec |

Unsupported MIME types (`audio/flac`) and PCM rates outside the documented set are rejected client-side. Plan gating (192 kbps MP3 and 44.1 kHz PCM/WAV need higher tiers) is left to the API.

### Body

[`build_tts_request`](../gaise-provider-elevenlabs/src/contracts/models.rs) serializes [`ElevenLabsTtsRequest`](../gaise-provider-elevenlabs/src/contracts/models.rs):

| `GaiseSpeechRequest` | Wire field | Notes |
|---|---|---|
| `input` | `text` | Per-model character limits apply (v3 5,000; multilingual_v2 10,000; flash_v2_5 40,000) |
| `model` | `model_id` | Bare model id after routing |
| `language` | `language_code` | **Omitted for `eleven_multilingual_v2`**, which rejects it ([`model_accepts_language_code`](../gaise-provider-elevenlabs/src/contracts/models.rs)) |
| `voice_settings.stability` / `similarity` / `style` / `speaker_boost` / `speed` | `voice_settings.stability` / `similarity_boost` / `style` / `use_speaker_boost` / `speed` | `speed` clamped to 0.7–1.2; `style` and speaker boost are v2+ features — check `can_use_style` from `list_models` |
| `seed` | `seed` | Clamped to `u32::MAX`; best-effort determinism |
| `include_alignment` | selects `/with-timestamps` | Returns character timing alongside audio |
| `instructions` | — | Not mapped; ElevenLabs v3 takes delivery hints as audio tags inside `text` |

Request stitching (`previous_text`, `previous_request_ids`), pronunciation dictionaries, and text normalization flags are not mapped.

### Model-family rules

| Rule | Source |
|---|---|
| `eleven_v3*` and `eleven_v4*` realtime use the **text-to-dialogue** WebSocket ("`model_id` must start with `eleven_v3` or `eleven_v4`"; `eleven_v4_turbo` takes exactly one voice); all other models use `stream-input` ([`model_uses_dialogue_websocket`](../gaise-provider-elevenlabs/src/contracts/models.rs)) | ElevenLabs API reference: "this endpoint does not support the eleven_v3 model" |
| `eleven_multilingual_v2` rejects `language_code` | TTS reference |
| `eleven_turbo_v2_5` / `eleven_turbo_v2` are deprecated in favour of the Flash equivalents | Models overview |
| `eleven_v3` has no request stitching | Request-stitching guide |

## Response mapping

- `speech` without alignment: the response body is the encoded audio; `format`/`sample_rate` echo the resolved output format; `external_id` is the `request-id` header.
- `speech` with alignment: `/with-timestamps` JSON → base64 audio decoded, `alignment` (or `normalized_alignment` when the former is absent) mapped to [`GaiseSpeechAlignment`](sdk.md#speech) in seconds.
- Usage: `input.characters` = character count of the text; `total.character_cost` = the `character-cost` response header when present ([`usage_from`](../gaise-provider-elevenlabs/src/elevenlabs_client.rs)).
- Errors: the `detail` envelope is parsed whether it is an object (`status`/`code` + `message`) or a FastAPI validation array ([`format_error`](../gaise-provider-elevenlabs/src/contracts/models.rs)).

## Streaming

| Mode | Endpoint | Framing | Chunks emitted |
|---|---|---|---|
| `include_alignment: false` | `POST …/stream` | Raw chunked bytes of the chosen format | `Audio` per network chunk, then one `Usage` |
| `include_alignment: true` | `POST …/stream/with-timestamps` | Newline-delimited JSON, optional `data:` prefix tolerated | `Audio` + `Alignment` per frame, then `Usage` |

[`LineBuffer`](../gaise-provider-elevenlabs/src/elevenlabs_client.rs) reassembles frames split across TCP chunks and flushes an unterminated tail at end of stream. Over HTTP, [`POST /v1/speech/audio`](api.md#post-v1speechaudio) forwards only the `Audio` chunks as a raw body.

## Usage counters

| Map | Key | Source |
|---|---|---|
| input | `characters` | length of `input` |
| total | `character_cost` | `character-cost` header |

## Embeddings

Not supported — `embeddings` returns "ElevenLabs does not offer text embeddings".

## Live / realtime

`live_connect` opens a text-in / audio-out session. Output is always `pcm_24000` (events carry `sample_rate: 24000`, matching OpenAI Realtime) with `sync_alignment=true` so every audio frame can also yield a transcript.

| `GaiseLiveInput` | `stream-input` frame | text-to-dialogue frame |
|---|---|---|
| `Text { text }` | `{"text": "<text> "}` (trailing space enforced) | `{"inputs":[{"text","voice_id"}]}` |
| `AudioStreamEnd` / `ActivityEnd` | `{"text":" ","flush":true}` | `{"flush":true}` |
| `Close` | `{"text":""}` (end of stream) | `{"close_socket":true}` |
| `ActivityStart` | ignored | ignored |
| `Audio`, `Image`, `ToolResponse`, `ClearAudio`, `CancelResponse` | `error` event — not applicable to TTS | same |

| Server frame | `GaiseLiveEvent` |
|---|---|
| `audio` (base64) | `Audio { data, sample_rate: 24000 }` |
| `alignment` / `normalizedAlignment` (`chars`, `charStartTimesMs`, `charDurationsMs`) | `Transcript { role: "assistant", text }` |
| `isFinal: true` / `is_final_audio_for_turn: true` | `TurnComplete` |
| `message` / `error` / abnormal close | `Error { message }` |
| socket closed | `SessionEnded` |

Mapping functions: [`stream_input_events`](../gaise-provider-elevenlabs/src/elevenlabs_client.rs), [`dialogue_events`](../gaise-provider-elevenlabs/src/elevenlabs_client.rs), URL construction in [`live_url`](../gaise-provider-elevenlabs/src/elevenlabs_client.rs). The first frame carries `chunk_length_schedule: [120, 160, 250, 290]` (stream-input) or `voices: [voice]` (dialogue). Conversational agents (audio in) are not wrapped.

## Transcription

`transcribe` ([`GaiseClient::transcribe`](sdk.md)) posts a `multipart/form-data` body to `POST /v1/speech-to-text` with the `xi-api-key` header. The form is built by the pure [`build_transcription_form`](../gaise-provider-elevenlabs/src/contracts/transcription.rs) and encoded without extra dependencies by [`ElevenLabsTranscriptionForm::encode`](../gaise-provider-elevenlabs/src/contracts/transcription.rs). The call is always synchronous; `webhook=true` (HTTP 202, result delivered later) is never sent and is rejected when it appears in `provider_options`.

| `GaiseTranscriptionRequest` | Form field | Notes |
|---|---|---|
| `model` | `model_id` | `scribe_v2`, `scribe_v2_medical`; `scribe_v1` is deprecated |
| `audio` = `Bytes` | `file` | Filename and `Content-Type` from `mime_type`, else sniffed (wav, mp3, ogg, flac, webm, m4a) |
| `audio` = `Url` (`https://`) | `source_url` | `cloud_storage_url` is deprecated and never sent; `data:` URLs are decoded and uploaded as `file`; `gs://` / `s3://` are rejected |
| `languages[0]` | `language_code` | The API takes one language; the rest are ignored. Empty = auto-detect |
| `diarize` | `diarize=true` | Words carry `speaker_id` (`speaker_0`, …) |
| `max_speakers` | `num_speakers` | 1–32, validated client-side |
| `timestamps` | `timestamps_granularity` | `word` / `segment` → `word`, `character` → `character`; `none` or unset → omitted |
| `format` (raw PCM) | `file_format=pcm_s16le_16` | Only 16-bit 16 kHz mono PCM; other raw rates, μ-law and A-law are rejected with a clear error. Container codecs send nothing |
| `temperature` | `temperature` | 0.0–2.0 |
| `multichannel` | `use_multi_channel=true` | Up to 5 channels, each billed at full duration |
| `keyterms` | `keyterms` (repeated once per term) | +20% surcharge on ElevenLabs' side |
| `mode` = `smart` | `no_verbatim=true` | `scribe_v2` only; rejected for `scribe_v2_medical` |
| `prompt` | — | Rejected: the batch API has no context field (`previous_text` is realtime-only) |
| `provider_options` | one field per top-level key | Strings as-is, arrays repeat the field, other values JSON-stringified; a key replaces the mapped field of the same name (e.g. `tag_audio_events`, `seed`, `entity_detection`, `diarization_threshold`, `multichannel_output_style`) |

Response mapping ([`map_transcription_response`](../gaise-provider-elevenlabs/src/contracts/transcription.rs)):

| ElevenLabs | `GaiseTranscriptionResponse` |
|---|---|
| `text` | `text` |
| `language_code` | `language` (as reported, e.g. `eng`) |
| `words[].type` `word` / `spacing` / `audio_event` | `words[].kind` `Word` / `Spacing` / `AudioEvent` |
| `words[].logprob` | `words[].confidence` = `exp(logprob)` clamped to 0–1 |
| `words[].speaker_id`, `channel_index` | `words[].speaker`, `words[].channel` |
| consecutive words of one speaker | `segments` (only when diarized) |
| `transcription_id` | `external_id` |
| `audio_duration_secs` | `duration_seconds` and `usage.input.audio_milliseconds` |
| multichannel `transcripts[]` | `channels[]` (index, text, language, words); top-level `text` joins channel texts with newlines and `words` interleaves all channels by start time |

Per-character timings (`characters`) are not surfaced. Errors use the same `detail` envelope parser as text-to-speech.

## Streaming transcription

`transcribe_connect` (feature `live`) opens `wss://{host}/v1/speech-to-text/realtime` with the `xi-api-key` header; the host comes from `ELEVENLABS_API_URL` with `https://` → `wss://`. The URL is built by the pure [`build_realtime_transcription_setup`](../gaise-provider-elevenlabs/src/contracts/transcription.rs).

| `GaiseTranscriptionSessionConfig` | Query parameter | Notes |
|---|---|---|
| `model` | `model_id` | Defaults to `scribe_v2_realtime` |
| `audio_format` | `audio_format` | `pcm` at 8000 / 16000 / 22050 / 24000 / 44100 / 48000 Hz → `pcm_{rate}`; `mulaw` 8000 → `ulaw_8000`; anything else (and stereo) is rejected. Default `pcm_16000` |
| `languages[0]` | `language_code` | |
| `commit_strategy` | `commit_strategy` | `manual`, `vad`, `turn_prediction` |
| `silence_duration_ms` | `vad_silence_threshold_secs` | Converted to seconds |
| `timestamps` = word / character / segment | `include_timestamps=true` | Final transcripts then carry words |
| `keyterms` | `keyterms` (repeated) | |
| `prompt` | `previous_text` on the **first** audio chunk only | The API errors if it appears later |
| `diarize` | — | Rejected: the realtime API has no diarization |
| `interim_results` | — | Client-side: partial transcripts are forwarded only when set |
| `provider_options` | one query parameter per top-level key | e.g. `no_verbatim`, `vad_threshold`, `include_language_detection`; `token` is refused (the client authenticates with the header) |

| `GaiseTranscriptionInput` | Client frame |
|---|---|
| `Audio { data }` | `{"message_type":"input_audio_chunk","audio_base_64":…,"commit":false,"sample_rate":N}` |
| `Commit` | Empty chunk with `"commit":true` |
| `Close` | Commit any uncommitted audio, wait up to 5 s for the committed transcript, then close the socket |

| Server `message_type` | `GaiseTranscriptionEvent` |
|---|---|
| `session_started` | `Started { session_id, model }` |
| `partial_transcript` | `Transcript { is_final: false }` (when `interim_results`) |
| `committed_transcript` | `Transcript { is_final: true, utterance_end: true }` |
| `committed_transcript_with_timestamps` | Same, with `words`, `start`/`end`, `language`, `speaker`; when `include_timestamps` is on it replaces the plain commit so each segment is emitted once ([`ElevenLabsRealtimeMapper`](../gaise-provider-elevenlabs/src/contracts/transcription.rs)) |
| `warning` and recoverable errors (`commit_throttled`, `input_error`, `chunk_size_exceeded`, `insufficient_audio_activity`, `transcriber_error`, `error`) | `Error { message }` |
| `auth_error`, `quota_exceeded`, `unaccepted_terms`, `rate_limited`, `queue_overflow`, `resource_exhausted`, `session_time_limit_exceeded`, `invalid_request` | `Error`, then `Ended { reason: <type> }` |
| `committed_transcript_entities`, `edited_transcript` | not mapped |
| socket closed | `Ended` (reason = close frame for abnormal closes) |

There are no speech-started / speech-stopped or usage frames on this API.

## Model discovery

`GET /v1/models` → [`map_elevenlabs_model`](../gaise-provider-elevenlabs/src/contracts/models.rs). Provider-sourced: id, name, description, `can_do_text_to_speech` (→ input `text`, output `audio`, operations `speech` + `live`), `can_do_voice_conversion` (→ input `audio`), `requires_alpha_access` (→ `preview`), `maximum_text_length_per_request` (→ notes). Tools, reasoning, and structured output are `unsupported`. The registry overlay adds lifecycle notes and the `eleven_v3_conversational` live-only override.

**Limits.** `maximum_text_length_per_request` is provider-sourced and becomes `limits.max_input_characters` (also echoed in `notes`); speech models have no token window. The documented per-model character budgets are in [limits.md](limits.md).

## Models

| Model | Aliases | Status | Dates | Input | Output | Operations | Reasoning values | GAISe support | Notes |
|---|---|---|---|---|---|---|---|---|---|
| `eleven_v4` | — | `active` | — | text | text, audio | live | — | realtime via the text-to-dialogue WebSocket (up to 10 voices); /v1/text-to-speech is not documented for v4 | Released 2026-09-28 for content creation and long-form audio, 90+ languages. The Text to Dialogue API takes up to 10,000 characters (~10 min… |
| `eleven_v4_turbo` | — | `active` | — | text | text, audio | live | — | realtime only via the text-to-dialogue WebSocket (exactly one voice) | Released 2026-09-28; ElevenLabs' most expressive realtime model (~100 ms) for agents and interactive use. Register exactly one voice per ses… |
| `eleven_v3` | — | `active` | — | text | audio | speech, live | — | speech via /v1/text-to-speech; realtime via the text-to-dialogue WebSocket | Flagship, 70+ languages, 5,000 characters per request on /v1/text-to-speech; the text-to-dialogue endpoints (GAISe's realtime path for v3) d… |
| `eleven_v3_conversational` | — | `active` | — | text | text, audio | live | — | realtime only via the text-to-dialogue WebSocket (one voice) | ~280 ms latency variant of v3 for realtime use. |
| `eleven_multilingual_v2` | — | `active` | — | text | audio | speech, live | — | native | Default model; 29 languages; 10,000 characters per request. language_code is documented as not supported (ignored) for multilingual_v2, so t… |
| `eleven_flash_v2_5` | — | `active` | — | text | audio | speech, live | — | native | ~75 ms latency, 32 languages, 40,000 characters per request, accepts language_code. Numbers are not normalized by default. |
| `eleven_flash_v2` | — | `active` | — | text | audio | speech, live | — | native | English only; 30,000 characters per request. |
| `eleven_turbo_v2_5` | — | `deprecated` | — | text | audio | speech, live | — | native while available | Functionally equivalent to eleven_flash_v2_5; no shutdown date published. Character limit as for the functionally equivalent eleven_flash_v2… |
| `eleven_turbo_v2` | — | `deprecated` | — | text | audio | speech, live | — | native while available | Character limit as for the functionally equivalent eleven_flash_v2. Replacement `eleven_flash_v2`. |
| `eleven_multilingual_sts_v2` | `eleven_english_sts_v2` | `active` | — | audio | audio | — | — | not supported: speech-to-speech has no GAISe surface | — |
| `scribe_v2` | — | `active` | — | text, audio | text | transcription | — | transcribe: POST /v1/speech-to-text (diarization up to 32 speakers, word or character timestamps, audio events, keyterms, multichannel, no_verbatim for smart mode) | scribe_v2_realtime is listed as its own model (~150 ms, 90+ languages); scribe_v1 is deprecated with no shutdown date. scribe_v2_realtime mo… |
| `scribe_v2_medical` | — | `active` | — | text, audio | text | transcription | — | transcribe: POST /v1/speech-to-text, same request shape as scribe_v2 (smart mode is rejected) | Speech recognition fine-tuned for clinical audio, 90+ languages; same Speech to Text API and price as Scribe v2. Listed on the models page b… |
| `scribe_v2_realtime` | — | `active` | — | text, audio | text | live_transcription | — | transcribe_connect: wss://api.elevenlabs.io/v1/speech-to-text/realtime (feature live); partial and committed transcripts, manual, VAD, or turn-prediction commits, word timestamps; no diarization | Realtime Scribe (~150 ms partials), WebSocket only; PCM 8-48 kHz or u-law 8 kHz input. Was an alias of scribe_v2 until 2026-10-17. |
| `eleven_ttv_v3` | `eleven_multilingual_ttv_v2` | `active` | — | text | text, audio | — | — | not supported: voice design (/v1/text-to-voice/design) has no GAISe surface | Text-to-voice design models; the endpoint defaults to eleven_multilingual_ttv_v2 and takes 100-1,000 characters of description. |
| `music_v2` | — | `active` | — | text | text, audio | — | — | not supported: music generation (/v1/music) has no GAISe surface | Studio-grade music with composition plans, audio reference, and inpainting; superseded as the flagship by music_v2_5 (2026-09). Section dura… |
| `music_v2_5` | — | `active` | — | text | text, audio | — | — | not supported: music generation (/v1/music) has no GAISe surface | Most advanced music model (listed by 2026-09-12; /v1/music/compose model_id enum music_v1, music_v2, music_v2_5). Section durations are alwa… |
| `music_v1` | — | `deprecated` | — | text | text, audio | — | — | not supported: music generation (/v1/music) has no GAISe surface | Listed under Deprecated models by 2026-09-12 (outclassed by music_v2 and music_v2_5); still the /v1/music default; no shutdown date publishe… |
| `eleven_text_to_sound_v2` | — | `active` | — | text | text, audio | — | — | not supported: sound effects (/v1/sound-generation) have no GAISe surface | 0.5-30 s effects with optional looping. |

Full generated table: [models.md#elevenlabs](models.md#elevenlabs).

## Limitations and explicit fallbacks

- No default voice; a missing `voice` is an error, never a silent pick.
- `instructions` is not mapped (use v3 audio tags in the text).
- Live sessions accept text only; audio/image/tool inputs and clear/cancel produce `error` events.
- Speech-to-speech, music, voice design, and Agents are out of scope.
- Speech-to-text: webhook (asynchronous) mode, `transcript_edit`, `additional_formats`, entity output, and per-character timings are not surfaced; batch `prompt` and realtime `diarize` are rejected.
- Alignment from `/stream/with-timestamps` is parsed defensively (`data:` prefix or bare JSON) because the framing is documented only through the official SDKs.
- Plan-gated formats and concurrency limits surface as API errors (`429 rate_limit_error`).

## Flow

```mermaid
sequenceDiagram
    participant App
    participant Router as GaiseClientService
    participant Adapter as GaiseClientElevenLabs
    participant API as api.elevenlabs.io
    App->>Router: speech(model=elevenlabs::eleven_flash_v2_5, voice, input, format)
    Router->>Adapter: speech(bare model)
    Adapter->>Adapter: resolve_output_format, build_tts_request
    Adapter->>API: POST /v1/text-to-speech/{voice}?output_format=pcm_24000 (xi-api-key)
    API-->>Adapter: 200 audio bytes + request-id + character-cost
    Adapter-->>Router: GaiseSpeechResponse { audio, format, sample_rate, usage }
    Router-->>App: response
```

```mermaid
sequenceDiagram
    participant App
    participant Session as GaiseLiveSession
    participant WS as wss stream-input / text-to-dialogue
    App->>Session: live_connect(model, voice)
    Session->>WS: upgrade (xi-api-key) + init frame
    Session-->>App: SessionStarted
    loop text fragments
        App->>Session: tx.send(Text)
        Session->>WS: text frame
        WS-->>Session: audio + alignment
        Session-->>App: Audio, Transcript
    end
    App->>Session: tx.send(AudioStreamEnd)
    Session->>WS: flush
    WS-->>Session: isFinal
    Session-->>App: TurnComplete
    App->>Session: tx.send(Close)
    Session->>WS: end-of-stream / close_socket
    Session-->>App: SessionEnded
```

## Tests

- [`contracts/models.rs`](../gaise-provider-elevenlabs/src/contracts/models.rs) unit tests: output-format resolution, request body rules, timestamp-frame parsing, error envelopes, model mapping, WebSocket alignment conversion.
- [`elevenlabs_client.rs`](../gaise-provider-elevenlabs/src/elevenlabs_client.rs) unit tests: NDJSON line buffering across chunk boundaries, frame → chunk mapping, voice requirement, base-URL normalization.
- [`contracts/transcription.rs`](../gaise-provider-elevenlabs/src/contracts/transcription.rs) unit tests: multipart form fields and encoding, rejections (webhook, PCM rates, smart mode on medical), single- and multichannel response mapping with audio events, realtime URL/query building, audio chunk frames, and server-event mapping (duplicate commits, fatal errors).
- [`tests/transcription_tests.rs`](../gaise-provider-elevenlabs/tests/transcription_tests.rs): `transcribe` against a local mock HTTP server (multipart body, `source_url`, multichannel, error envelopes).
- [`tests/realtime_transcription_tests.rs`](../gaise-provider-elevenlabs/tests/realtime_transcription_tests.rs) (feature `live`): `transcribe_connect` against a local WebSocket server (URL, header, `previous_text` on the first chunk only, commit on close, one final transcript per segment).
- Router and HTTP: [`gaise-client/tests/speech_tests.rs`](../gaise-client/tests/speech_tests.rs), [`gaise-api/tests/speech_route_tests.rs`](../gaise-api/tests/speech_route_tests.rs).
- No live tests; nothing in the suite contacts ElevenLabs.

## Sources

- API reference: <https://elevenlabs.io/docs/api-reference/text-to-speech/convert>, <https://elevenlabs.io/docs/api-reference/text-to-speech/stream>, <https://elevenlabs.io/docs/api-reference/text-to-speech/v-1-text-to-speech-voice-id-stream-input>, <https://elevenlabs.io/docs/api-reference/text-to-dialogue/ttd-websocket>, <https://elevenlabs.io/docs/api-reference/models/list>, <https://elevenlabs.io/docs/api-reference/voices/search>
- Speech-to-text: <https://elevenlabs.io/docs/api-reference/speech-to-text/convert>, <https://elevenlabs.io/docs/api-reference/speech-to-text/v-1-speech-to-text-realtime> (audited 2026-10-17)
- Models and limits: <https://elevenlabs.io/docs/overview/models>; errors: <https://elevenlabs.io/docs/eleven-api/resources/errors>
- Source: [`elevenlabs_client.rs`](../gaise-provider-elevenlabs/src/elevenlabs_client.rs), [`contracts/models.rs`](../gaise-provider-elevenlabs/src/contracts/models.rs), [`contracts/transcription.rs`](../gaise-provider-elevenlabs/src/contracts/transcription.rs), [`gaise_speech.rs`](../gaise-core/src/contracts/gaise_speech.rs), [`gaise-client/src/lib.rs`](../gaise-client/src/lib.rs), [`gaise-api/src/lib.rs`](../gaise-api/src/lib.rs)
