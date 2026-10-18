# xAI (`xai`)

> Part of the [GAISe wiki](README.md) · [Models](models.md#xai) · [Capabilities](capabilities.md) · [HTTP API](api.md) · [Rust SDK](sdk.md) · [Flows](flows.md) · [Examples](examples.md)

The `gaise-provider-xai` crate drives xAI's Grok models through these surfaces:

- **Chat:** `instruct` and `instruct_stream` on Chat Completions, with function tools, image input, and reasoning traces.
- **Text to speech:** `speech` and `speech_stream` on `POST /v1/tts`.
- **Speech to text:** `transcribe` on `POST /v1/stt`, and `transcribe_connect` on the streaming `wss://api.x.ai/v1/stt` endpoint (crate feature `live`).
- **Image generation:** `generate_images` on `/v1/images/generations` and `/v1/images/edits`.
- **Video generation:** `start_video` and `get_video` on the asynchronous Videos API.
- **Speech to speech:** `live_connect` over the realtime WebSocket (crate feature `live`).
- **Model discovery:** `list_models` on `GET /v1/models`.

It is enabled by the `xai` feature of [`gaise-client`](../gaise-client/Cargo.toml), which is on by default. xAI's documentation now uses the name SpaceXSI, but the API host is still `api.x.ai`.

## At a glance

| Crate | Feature flag | Client types | Instruct surface | Speech surface | Media surface | Live surface | Model listing surface |
|---|---|---|---|---|---|---|---|
| [`gaise-provider-xai`](../gaise-provider-xai/Cargo.toml) | `xai` in [`gaise-client`](../gaise-client/Cargo.toml); crate-level `live` for WebSockets | [`GaiseClientXai`](../gaise-provider-xai/src/xai_client.rs) (`GaiseClient` + `GaiseSpeechClient`), [`GaiseClientXaiLive`](../gaise-provider-xai/src/xai_live_client.rs) | `POST /v1/chat/completions` (SSE streaming) | `POST /v1/tts` | `POST /v1/stt`, `/v1/images/{generations,edits}`, `/v1/videos/{generations,edits,extensions}` + `GET /v1/videos/{id}` | `wss://api.x.ai/v1/realtime?model=…`, `wss://api.x.ai/v1/stt?…` | `GET /v1/models` (+ `GET /v1/tts/voices` helper) |

## Configuration

| Variable | Read by | Default | Purpose |
|---|---|---|---|
| `XAI_API_KEY` | [`gaise-api/src/main.rs`](../gaise-api/src/main.rs) → `GaiseClientConfig.xai_api_key` | none; `get_client("xai")` errors with "xAI API Key not configured" | Sent as `Authorization: Bearer …` on every request and WebSocket handshake |
| `XAI_API_URL` | `GaiseClientConfig.xai_api_url` | `https://api.x.ai/v1` ([`DEFAULT_API_URL`](../gaise-provider-xai/src/xai_client.rs)) | API root. Set it to `https://us.api.x.ai/v1` for the US regional endpoint, which serves only grok-4.7 and grok-4.6 and no voice, image, or video APIs. For live sessions, `https://` is rewritten to `wss://`. |

`configured_providers()` lists `xai` only when a key is set. A per-request [`GaiseConnection`](sdk.md) can override the URL and key.

## Request mapping

### Chat Completions

[`xai_chat_request`](../gaise-provider-xai/src/xai_client.rs) builds the body:

| GAISe | xAI | Notes |
|---|---|---|
| `generation_config.max_tokens` | `max_completion_tokens` | `max_tokens` is deprecated upstream. The limit counts visible output only; the upstream default is 128,000. |
| `temperature`, `top_p` | same | Passed through unchanged. |
| `thinking_effort` | `reasoning_effort` | Uses the family rules below. |
| `cache_key` | `prompt_cache_key` | xAI maps it to the `x-grok-conv-id` cache route. |
| `input_image_detail` | `image_url.detail` | `original` becomes `high`. |
| `tools` | `tools[{type: function, function}]` | The root of the parameter schema is always `object`. |
| `tool_config.mode` | `tool_choice` | `any`/`required` → `required`; `none`; anything else → `auto`. |
| Image content | `image_url` data URL | xAI accepts JPEG and PNG, up to 20 MiB each. |
| Text file content | `<attached_document>` text part | Binary files are rejected. |
| Audio content | — | Rejected: Chat Completions has no audio input. Use a voice model through `live_connect`. |
| `Reasoning` / `RedactedReasoning` content | — | Dropped, because Chat Completions has no field for replaying reasoning. |

GAISe never sends `presence_penalty`, `frequency_penalty`, or `stop`. xAI returns an error for those on reasoning models.

### Model-family rules

The rules come from [`xai_chat_rules`](../gaise-provider-xai/src/xai_client.rs) and are pinned in [`parameter_matrix_tests.rs`](../gaise-provider-xai/tests/parameter_matrix_tests.rs).

| Family | `reasoning_effort` sent | Default upstream |
|---|---|---|
| `grok-4.7`, `grok-4.6`, and later `grok-4.N` | `low`, `medium`, `high`, `xhigh`. Reasoning cannot be disabled, so `none` and `minimal` become `low`, and `max`/`ultra` become `xhigh`. | `high` |
| `grok-4.5` (also `grok-build-latest`) | `low`, `medium`, `high`. xAI treats `xhigh` as `high` on 4.5, so GAISe clamps it the same way. | `high` |
| `grok-4.3` | `none`, `low`, `medium`, `high`, `xhigh` | `low` |
| `grok-4.20-*-reasoning`, `grok-build-0.1` / `grok-code-fast*` | Never sent; no values are documented. | — |
| `grok-4.20-*-non-reasoning`, retired slugs (`grok-4-*-fast-*`, `grok-4-0709`, `grok-3*`) | Never sent. Retired slugs redirect upstream to grok-4.3 at a fixed effort. | — |
| `*multi-agent*`, `grok-imagine-*`, `grok-voice*` | The adapter fails fast before any HTTP call: multi-agent needs the Responses API, and the others have their own endpoints. | — |
| Other `grok-*` ids | The canonical value is passed through. | — |

## Response mapping

- `message.reasoning_content` becomes a leading [`GaiseContent::Reasoning`](sdk.md) part with no signature. `content` becomes text.
- `refusal` is returned as text when there is no other text.
- Tool calls map one to one.

## Streaming

`stream: true` and `stream_options.include_usage: true` are sent. The SSE lines are buffered until each one is complete, so tool calls and other events can span network reads.

Each chunk emits events in this order:
1. A `Content(Reasoning)` event for `reasoning_content`.
2. One `ToolCall` event per call. xAI sends each call whole in a single chunk.
3. `Text`.
4. `Usage`, when the chunk carries it.

## Usage counters

| Side | Keys |
|---|---|
| Input | `prompt_tokens`, plus `text_tokens`, `image_tokens`, `audio_tokens`, `cached_tokens` when reported |
| Output | `completion_tokens`, plus `reasoning_tokens`, `audio_tokens`, `accepted_prediction_tokens`, `rejected_prediction_tokens` when reported |
| Total | `total_tokens` and `cost_in_usd_ticks` (1 USD = 10^10 ticks) |

## Text to speech

The model is routed as `xai::tts`. `POST /v1/tts` has no model field, so the name is ignored. [`build_tts_request`](../gaise-provider-xai/src/contracts/speech.rs) maps the request:

| GAISe | xAI |
|---|---|
| `input` | `text` (at most 60,000 characters) |
| `voice` | `voice_id` (xAI defaults to `eve`). List voices with `GaiseClientXai::list_voices` (`GET /v1/tts/voices`). |
| `language` | `language`, defaulting to `auto` (the field is required upstream) |
| `format` + `sample_rate` | `output_format.codec` + `sample_rate`. Codecs: `mp3` (default, `audio/mpeg`), `wav`, `pcm`, and `mulaw`/`alaw` (fixed at 8 kHz). Rates: 8000, 16000, 22050, 24000 (default), 44100, 48000. Other formats and rates are rejected client-side. |
| `voice_settings.speed` | `speed`, clamped to 0.7–1.5 |
| `include_alignment` | `with_timestamps: true`. The response is then a JSON envelope whose `audio_timestamps` become `GaiseSpeechAlignment`. |

`speech_stream` forwards the response body chunk by chunk. When alignment is requested it emits the whole clip and then the alignment, because xAI returns a single JSON envelope in that case.

## Transcription (speech to text)

`transcribe` sends `POST /v1/stt` as `multipart/form-data`. The fields are built by [`build_xai_stt_parts`](../gaise-provider-xai/src/xai_media.rs), and the audio `file` is always the last part, because xAI ignores fields that come after it. The default model is `grok-voice-transcribe-2.0`. A multipart form cannot be cloned, so it is rebuilt for each retry of a 429 or 5xx.

| `GaiseTranscriptionRequest` | xAI form field | Notes |
|---|---|---|
| `model` | `model` | Sent as given. |
| `audio` (bytes) | `file` | The file name and MIME type come from `mime_type` or are sniffed (`wav`, `mp3`, `ogg`, `flac`, `mp4`, `webm`). Raw audio is named `audio.pcm`, `audio.mulaw`, or `audio.alaw`. |
| `audio` (URL) | `url` | xAI downloads the file itself. |
| `languages[0]` | `language` | Only the first language is used. |
| `mode: smart` | `format=true` | Inverse text normalization. It needs a language, so it is skipped when none is given. |
| `mode: verbatim` | `filler_words=true` | xAI removes fillers by default. |
| `diarize` | `diarize=true` | |
| `keyterms` | `keyterm`, repeated | At most 100 terms upstream. |
| `multichannel` | `multichannel=true` | |
| `format` with codec `pcm`/`mulaw`/`alaw` | `audio_format` + `sample_rate` (+ `channels` when > 1) | `sample_rate` is required for raw audio. Container codecs are auto-detected and not sent. |
| `prompt`, `temperature`, `max_speakers` | — | Rejected with an error. |
| `provider_options` | extra form fields | Strings as-is, arrays repeated, other values as JSON text. They replace mapped fields with the same name (for example `vad_threshold`). |

The response maps `text`, `language`, `duration` (as `duration_seconds`), `words` (with `confidence`, and `speaker` turned from an integer into a string label), and `channels` (with each word tagged with its channel). Usage is `input.audio_milliseconds`, taken from `duration`.

## Image generation

`generate_images` builds its body with [`build_xai_image_request`](../gaise-provider-xai/src/xai_media.rs). With no input images it posts to `/v1/images/generations`. With one input image it posts to `/v1/images/edits` with `image: {url}`, and with several it sends `images: [{url}, …]` (refer to them as `<IMAGE_0>`, `<IMAGE_1>`, … in the prompt). Inline bytes become data URLs.

| `GaiseImageRequest` | xAI | Notes |
|---|---|---|
| `model`, `prompt`, `n` | same | `n` is 1–10 upstream. |
| `aspect_ratio` | `aspect_ratio` | Passed through (`1:1`, `16:9`, …, `auto`). |
| `resolution` | `resolution` | `1K`/`1.5K`/`2K` (any case, or `1024`/`1536`/`2048`) become `1k`/`1.5k`/`2k`. Other values are rejected. |
| `quality` | `quality` | `low`, `medium`, or `auto`; anything else is rejected. Only `grok-imagine-image-2.0` accepts it. |
| — | `response_format: b64_json` | Always sent, so images come back as bytes. |
| `mask`, `negative_prompt`, `seed`, `size`, `output_format` | — | Rejected with an error that names the field. |
| `provider_options` | merged into the body | For example `user`, `storage_options`, or `output.upload_urls`. |

`data[].b64_json` is decoded into `GaiseGeneratedMedia.data`, and `url` and `mime_type` are kept when present. Items with `respect_moderation: false` are counted in `filtered`. Usage keeps `input_tokens`, `output_tokens` and their details, and puts `total_tokens` and `cost_in_usd_ticks` under `total`.

## Video generation

Video is asynchronous: `start_video` returns a `GaiseVideoJob` with the xAI `request_id` as its `id` and status `queued`, and `get_video` polls `GET /v1/videos/{id}`. [`build_xai_video_request`](../gaise-provider-xai/src/xai_media.rs) picks the endpoint:

| Request | Endpoint | Body |
|---|---|---|
| No `video` | `/v1/videos/generations` | `model`, `prompt`, `image{url}` (first frame), `last_frame{url}`, `reference_images[{url}]`, `duration` (1–15 s), `aspect_ratio`, `resolution` (`480p`/`720p`/`1080p`; others are rejected), `generate_audio` |
| `video`, no `duration_seconds` | `/v1/videos/edits` | `model`, `prompt`, `video{url}` (the output keeps the input's length, aspect ratio, and resolution) |
| `video` with `duration_seconds` | `/v1/videos/extensions` | `model`, `prompt`, `video{url}`, `duration` (2–10 s of continuation) |

- Edits and extensions need a prompt. They reject `image`, `last_frame`, `reference_images`, `aspect_ratio`, `resolution`, and `generate_audio`.
- `n` other than 1, `negative_prompt`, and `seed` are rejected.
- An http(s) `output_uri` becomes `output.upload_url`, a signed URL that xAI uploads the result to with HTTP PUT. Other schemes are rejected.
- `provider_options` are merged into the body (for example `keyframes`, `reference_audios`, or `storage_options`).
- Only `grok-imagine-video` supports edits and extensions. `last_frame` needs `grok-imagine-video-1.5`.

[`map_xai_video_job`](../gaise-provider-xai/src/xai_media.rs) maps the poll response:

| xAI `status` | `GaiseJobStatus` | Fields |
|---|---|---|
| `pending` | `running` | `progress` (0–99) |
| `done` | `completed` | `videos[0]` with `url`, `duration_seconds`, and `video/mp4`; `progress` 100. If `video.respect_moderation` is `false`, `filtered` is 1 and there is no video. |
| `failed`, `expired` | `failed` | `error` from `error.message` |

Usage is mapped the same way as for images, including `cost_in_usd_ticks`. Video URLs are temporary, so download them promptly.

## Streaming transcription

[`GaiseClientXaiLive::transcribe_connect`](../gaise-provider-xai/src/xai_live_client.rs) (feature `live`) dials the URL from [`build_xai_stt_url`](../gaise-provider-xai/src/xai_live_client.rs) with a Bearer key. It waits for `transcript.created` before it returns the session, so no audio is sent before the ASR backend is ready.

| `GaiseTranscriptionSessionConfig` | Query parameter |
|---|---|
| `model` | `model` (omitted when empty; xAI defaults to `grok-voice-transcribe-2.0`) |
| `audio_format.codec` | `encoding`: `pcm` (default), `mulaw`, `alaw`, or `opus`. Other codecs are rejected. |
| `audio_format.sample_rate` | `sample_rate` (default 16000; not sent for Opus) |
| `audio_format.channels` ≥ 2 | `multichannel=true&channels=N` (interleaved; not with Opus) |
| `interim_results` | `interim_results` |
| `languages[0]` | `language` |
| `diarize` | `diarize=true` |
| `keyterms` | `keyterm`, repeated and URL-encoded |
| `silence_duration_ms` | `endpointing` (capped at 5000) |
| `commit_strategy: turn_prediction` | `smart_turn=0.5` |
| `commit_strategy: manual` | Rejected, because xAI always endpoints on silence |
| `prompt` | Rejected |
| `provider_options` | Extra query parameters that replace mapped ones (for example `format=true`, `filler_words`, `smart_turn=0.7`, `smart_turn_timeout`, `vad_threshold`) |

| Input | Frame |
|---|---|
| `Audio { data }` | A binary frame of raw bytes (no base64). With Opus, send one packet per `Audio` input. |
| `Commit` | `{"type": "finalize"}` |
| `Close` | `{"type": "audio.done"}`. The server flushes, sends `transcript.done`, and closes. |

| Server event | `GaiseTranscriptionEvent` |
|---|---|
| `transcript.created` | `Started` (sent once the handshake completes) |
| `transcript.partial` | `Transcript`: `is_final`, `speech_final` → `utterance_end`, `start`, `end` = `start + duration`, `channel_index` → `channel`, `words` (and a segment `speaker` when every word has the same one) |
| `transcript.done` | `Usage` with `input.audio_milliseconds` from `duration` |
| `error` | `Error` (most xAI errors then close the socket) |
| socket close | `Ended`, with `reason` `completed` after a `transcript.done` |


## Live / realtime (speech to speech)

[`GaiseClientXaiLive`](../gaise-provider-xai/src/xai_live_client.rs) dials `wss://api.x.ai/v1/realtime?model=<model>` with a Bearer key. It waits for `session.created`, sends [`build_xai_session_update`](../gaise-provider-xai/src/xai_live_client.rs), and then waits for `session.updated`.

| `GaiseLiveConfig` | `session.update` |
|---|---|
| `system_instruction` | `instructions` |
| `voice` | `voice` (lowercased) |
| `vad_config.enabled = false` | `turn_detection: null` (manual turns: `ActivityEnd` sends `input_audio_buffer.commit` + `response.create`) |
| otherwise | `turn_detection: {type: server_vad}` |
| — | `audio.input.format` / `audio.output.format` = `audio/pcm` at 24 kHz |
| `transcription.input` | `audio.input.transcription.model: grok-transcribe` |
| `generation_config.thinking_effort` | `reasoning.effort`: `none` stays `none`, `auto` is omitted, and anything else becomes `high` |
| `tools` | flat `{type: function, name, description, parameters}` |

The server events map as follows:

| Server event | `GaiseLiveEvent` |
|---|---|
| `response.output_audio.delta` / `response.audio.delta` | `Audio` (24 kHz) |
| `response.output_text.delta` / `response.text.delta` | `Text` |
| `response.output_audio_transcript.delta` | `Transcript` (assistant) |
| `conversation.item.input_audio_transcription.completed` | `Transcript` (user). The cumulative `.updated` event is skipped. |
| `response.function_call_arguments.done` | `ToolCall` |
| `response.done` | `Usage` (when reported) and then `TurnComplete` |
| `input_audio_buffer.speech_started` | `Interrupted` |
| `error` | `Error`. Most xAI errors are recoverable, so the session stays open. |

Other inputs:
- Audio input must be 24 kHz PCM16.
- Image input is answered with an `Error` event.
- `ToolResponse` sends `function_call_output`. `response.create` is sent only once the response that issued the calls has finished and every call has an output, as xAI requires for parallel calls (`LiveToolCallGate`, since 5.5.1).

Ephemeral client secrets (`POST /v1/realtime/client_secrets`), Opus/G.711 codecs, binary transport, session resumption, server-side tools (`web_search`, `x_search`, `file_search`, `mcp`), and SIP are not mapped.

## Model discovery

`GET /v1/models` is mapped by [`map_xai_model`](../gaise-provider-xai/src/contracts/catalog.rs):
- It records `created`, `aliases` (as a note), `context_length`, and `capabilities.reasoning_effort` (as `reasoning_values`).
- Modalities and operations come from an id heuristic: `grok-imagine-image*` image, `grok-imagine-video*` video, `grok-voice*` live, `grok-voice-transcribe*` speech to text, `*multi-agent*` Responses-only, and other `grok-*` instruct.
- The registry overlay fills in the rest.

## Models

| Model | Aliases | Status | Dates | Input | Output | Operations | Reasoning values | GAISe support | Notes |
|---|---|---|---|---|---|---|---|---|---|
| `grok-4.7` | — | `active` | — | text, image | text | instruct, instruct_stream | `low`, `medium`, `high`, `xhigh` | native via Chat Completions (instruct, streaming, function tools, images); reasoning_content mapped to reasoning parts | Released September 2026; flagship. Reasoning cannot be disabled (effort default high; none is clamped to low). Knowledge cutoff May 2026. No… |
| `grok-4.6` | — | `active` | — | text, image | text | instruct, instruct_stream | `low`, `medium`, `high`, `xhigh` | native via Chat Completions | Released August 2026. Same effort set as grok-4.7 (default high, reasoning cannot be disabled). Served by the global and US regional endpoin… |
| `grok-4.5` | `grok-4.5-latest`, `grok-build-latest` | `active` | — | text, image | text | instruct, instruct_stream | `low`, `medium`, `high` | native via Chat Completions | Released July 2026. The model page lists xhigh, but the reasoning guide says xhigh is available on grok-4.6 and later and is treated as high… |
| `grok-4.3` | `grok-4.3-latest` | `active` | — | text, image | text | instruct, instruct_stream | `none`, `low`, `medium`, `high`, `xhigh` | native via Chat Completions | Default effort low; none disables reasoning. Redirect target of the slugs retired on 2026-05-15. Batch API supported (20% off). Release date… |
| `grok-4.20-0309-reasoning` | `grok-4.20`, `grok-4.20-reasoning`, `grok-4.20-reasoning-latest`, `grok-4.20-0309`, `grok-4.20-reasoning-gv2` | `active` | — | text, image | text | instruct, instruct_stream | supported | native via Chat Completions; reasoning_effort is not sent (no values documented) | Released March 2026. The model page lists no reasoning-effort values. logprobs are silently ignored on grok-4.20 and newer. Beta aliases (gr… |
| `grok-4.20-0309-non-reasoning` | `grok-4.20-non-reasoning`, `grok-4.20-non-reasoning-latest`, `grok-4.20-beta-non-reasoning`, `grok-4.20-non-reasoning-gv2` | `active` | — | text, image | text | instruct, instruct_stream | — | native via Chat Completions | Released March 2026; non-reasoning variant of Grok 4.20. |
| `grok-4.20-multi-agent-0309` | `grok-4.20-multi-agent`, `grok-4.20-multi-agent-latest`, `grok-4.20-multi-agent-beta-0309` | `preview` | — | text, image | text | — | `low`, `medium`, `high`, `xhigh` | not supported: Responses API only (the model does not work with Chat Completions); the adapter fails fast | Beta, March 2026. The effort level controls the agent count (4 or 16 agents). |
| `grok-build-0.1` | `grok-code-fast-1`, `grok-code-fast`, `grok-code-fast-1-0825` | `preview` | — | text, image | text | instruct, instruct_stream | supported | native via Chat Completions; reasoning_effort is not sent (no values documented) | Early access since May 2026; agentic coding model. grok-code-fast-1 was retired on 2026-05-15 and its slug redirects here. Batch API not sup… |
| `grok-4-1-fast-reasoning` | — | `retired` | shutdown 2026-05-15 | text, image | text | instruct, instruct_stream | supported | redirected upstream; reasoning_effort is not sent | Retired 2026-05-15 12:00 PT. The slug still resolves and is served by grok-4.3 with low reasoning effort, billed at grok-4.3 prices. Replacement `grok-4.3`. |
| `grok-4-1-fast-non-reasoning` | — | `retired` | shutdown 2026-05-15 | text, image | text | instruct, instruct_stream | — | redirected upstream | Retired 2026-05-15 12:00 PT. The slug still resolves and is served by grok-4.3 with reasoning effort none, billed at grok-4.3 prices. Replacement `grok-4.3`. |
| `grok-4-fast-reasoning` | — | `retired` | shutdown 2026-05-15 | text, image | text | instruct, instruct_stream | supported | redirected upstream; reasoning_effort is not sent | Retired 2026-05-15 12:00 PT. The slug still resolves and is served by grok-4.3 with low reasoning effort. Replacement `grok-4.3`. |
| `grok-4-fast-non-reasoning` | — | `retired` | shutdown 2026-05-15 | text, image | text | instruct, instruct_stream | — | redirected upstream | Retired 2026-05-15 12:00 PT. The slug still resolves and is served by grok-4.3 with reasoning effort none. Replacement `grok-4.3`. |
| `grok-4-0709` | — | `retired` | shutdown 2026-05-15 | text, image | text | instruct, instruct_stream | supported | redirected upstream; reasoning_effort is not sent | Retired 2026-05-15 12:00 PT. The slug still resolves and is served by grok-4.3 with low reasoning effort. Replacement `grok-4.3`. |
| `grok-3` | — | `retired` | shutdown 2026-05-15 | text | text | instruct, instruct_stream | — | redirected upstream | Retired 2026-05-15 12:00 PT. The slug still resolves and is served by grok-4.3 with reasoning effort none. grok-3-mini and the Grok 2 models… |
| `grok-voice-3.0` | `grok-voice-latest` | `active` | — | text, audio | text, audio | live | `none`, `high` | realtime transport (wss://api.x.ai/v1/realtime, feature live): 24 kHz PCM16, server VAD or manual turns, function tools, input transcription | Speech-to-speech, released October 2026; grok-voice-latest points here. OpenAI Realtime-compatible protocol with xAI session fields. reasoni… |
| `grok-voice-2.0` | — | `deprecated` | — | text, audio | text, audio | live | `none`, `high` | realtime transport (feature live) | Deprecated October 2026 when Grok Voice 3.0 shipped with the same endpoint, session parameters, events, and price. No shutdown date publishe… |
| `tts` | — | `active` | — | text | audio | speech | — | speech and speech_stream via POST /v1/tts (mp3, wav, pcm, mulaw, alaw; 8-48 kHz); alignment from with_timestamps | xAI text-to-speech has no model id: POST /v1/tts takes text, language (auto by default), voice_id (default eve), output_format, and speed (0… |
| `grok-voice-transcribe-2.0` | — | `active` | — | text, audio | text | transcription, live_transcription | — | transcribe: POST /v1/stt (diarization, word timestamps, keyterms, multichannel); transcribe_connect: wss://api.x.ai/v1/stt (feature live) | Default speech-to-text model since September 2026 (POST /v1/stt and wss://api.x.ai/v1/stt); diarization, keyterms, multichannel. $0.10 per h… |
| `grok-voice-transcribe-1.0` | — | `retired` | shutdown 2026-10-02 | text, audio | text | transcription, live_transcription | — | redirected upstream to grok-voice-transcribe-2.0 | End of life 2026-10-02; requests to the slug are routed to grok-voice-transcribe-2.0. Replacement `grok-voice-transcribe-2.0`. |
| `grok-imagine-image-2.0` | — | `active` | — | image | image | image_generation | — | generate_images: /v1/images/generations or /edits (multi-image references); quality low, medium, auto; resolution 1k, 1.5k, 2k | Recommended image model; quality low, medium, or auto; resolutions 1k, 1.5k, 2k; up to 5 source images for edits. $0.04 per image. |
| `grok-imagine-image` | — | `active` | — | image | image | image_generation | — | generate_images: /v1/images/generations or /edits; quality is not accepted | Grok Imagine Image 1.0 (snapshot grok-imagine-image-2026-03-02). $0.02 per image. |
| `grok-imagine-image-quality` | `grok-imagine-image-quality-20260403`, `grok-imagine-image-quality-latest`, `grok-imagine-image-pro` | `deprecated` | shutdown 2026-11-02 | image | image | image_generation | — | generate_images: /v1/images/generations or /edits; served by grok-imagine-image-2.0 at low quality after 2026-11-02 | Retires 2026-11-02; requests to the slug will then be served by grok-imagine-image-2.0 with quality low. grok-imagine-image-pro was retired… |
| `grok-imagine-video-1.5` | `grok-imagine-video-1.5-preview` | `active` | — | image | video | video_generation | — | start_video / get_video: /v1/videos/generations (first and last frame, references, 1-15 s, 480p-1080p) | Text or image to video (snapshot grok-imagine-video-1.5-2026-05-30); 1-15 s, 480p/720p/1080p. $0.080 per second. |
| `grok-imagine-video-1.5-lite` | — | `active` | — | image | video | video_generation | — | start_video / get_video: /v1/videos/generations | Lower-cost Grok Imagine Video 1.5. $0.020 per second. |
| `grok-imagine-video` | — | `active` | — | image, video | video | video_generation | — | start_video / get_video: generations, edits (source video), and extensions (source video plus duration) | Grok Imagine Video 1.0; also edits and extends videos. $0.050 per second. |

## Limitations and explicit fallbacks

- The Responses API is not used for xAI. That means no encrypted reasoning round-trip, no server-side tools (web search, X search, code execution, collections, MCP), no stateful `previous_response_id`, and no multi-agent model.
- Embeddings return an error, because xAI lists no public embedding model.
- Images: masks, negative prompts, seeds, exact `size`, and `output_format` are rejected (xAI has no such fields). Deferred image requests (`deferred: true`), Files API inputs (`file_id`), and `storage_options` are not mapped; pass them through `provider_options` if needed (a deferred request returns no images to `generate_images`).
- Video: `n` > 1, negative prompts, and seeds are rejected. `keyframes`, `reference_audios`, Files API inputs, and `storage_options` are reachable only through `provider_options`.
- Speech to text: `prompt`, `temperature`, and `max_speakers` are rejected. Streaming sessions reject `GaiseCommitStrategy::Manual`, because xAI always endpoints on silence (`Commit` still sends `finalize`).
- Deferred completions, `service_tier`, `search_parameters`, and the Batch API are not sent.

## Tests

- [`tests/parameter_matrix_tests.rs`](../gaise-provider-xai/tests/parameter_matrix_tests.rs): the per-family effort rules.
- [`tests/mapping_tests.rs`](../gaise-provider-xai/tests/mapping_tests.rs): request, response, streaming, TTS, speech-to-text (multipart field order with the file last), image (generation/edit path selection, base64 decoding, rejected fields), and video (start, poll, moderation) mapping, against a local HTTP server.
- [`tests/live_mapping_tests.rs`](../gaise-provider-xai/tests/live_mapping_tests.rs): `session.update`, the realtime event mapping, the streaming STT URL builder and event mapping, and a `transcribe_connect` round trip against a local WebSocket server (feature `live`).
- [`gaise-client/tests/xai_routing_tests.rs`](../gaise-client/tests/xai_routing_tests.rs): routing through the client.

None of these has run against the live xAI API.

## Sources

- Models: <https://docs.x.ai/developers/models>
- Chat Completions: <https://docs.x.ai/developers/rest-api-reference/inference/chat-completions>
- Reasoning: <https://docs.x.ai/developers/model-capabilities/text/reasoning>
- Voice (realtime, TTS, STT): <https://docs.x.ai/developers/rest-api-reference/inference/voice>, <https://docs.x.ai/developers/model-capabilities/audio/speech-to-speech>, and <https://docs.x.ai/developers/model-capabilities/audio/speech-to-text>
- Images: <https://docs.x.ai/developers/rest-api-reference/inference/images> and <https://docs.x.ai/developers/model-capabilities/images/generation>
- Videos: <https://docs.x.ai/developers/rest-api-reference/inference/videos> and <https://docs.x.ai/developers/model-capabilities/video/generation>
- Release notes: <https://docs.x.ai/developers/release-notes>
- May 15 retirement: <https://docs.x.ai/developers/migration/may-15-retirement>
