# xAI (`xai`)

> Part of the [GAISe wiki](README.md) · [Models](models.md#xai) · [Capabilities](capabilities.md) · [HTTP API](api.md) · [Rust SDK](sdk.md) · [Flows](flows.md) · [Examples](examples.md)

The `gaise-provider-xai` crate drives xAI's Grok models through four surfaces:

- **Chat:** `instruct` and `instruct_stream` on Chat Completions, with function tools, image input, and reasoning traces.
- **Text to speech:** `speech` and `speech_stream` on `POST /v1/tts`.
- **Speech to speech:** `live_connect` over the realtime WebSocket (crate feature `live`).
- **Model discovery:** `list_models` on `GET /v1/models`.

It is enabled by the `xai` feature of [`gaise-client`](../gaise-client/Cargo.toml), which is on by default. xAI's documentation now uses the name SpaceXSI, but the API host is still `api.x.ai`.

## At a glance

| Crate | Feature flag | Client types | Instruct surface | Speech surface | Live surface | Model listing surface |
|---|---|---|---|---|---|---|
| [`gaise-provider-xai`](../gaise-provider-xai/Cargo.toml) | `xai` in [`gaise-client`](../gaise-client/Cargo.toml); crate-level `live` for WebSockets | [`GaiseClientXai`](../gaise-provider-xai/src/xai_client.rs) (`GaiseClient` + `GaiseSpeechClient`), [`GaiseClientXaiLive`](../gaise-provider-xai/src/xai_live_client.rs) | `POST /v1/chat/completions` (SSE streaming) | `POST /v1/tts` | `wss://api.x.ai/v1/realtime?model=…` | `GET /v1/models` (+ `GET /v1/tts/voices` helper) |

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
- `ToolResponse` sends `function_call_output` and then `response.create`.

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
| `grok-voice-transcribe-2.0` | — | `active` | — | text, audio | text | — | — | not supported: speech-to-text has no GAISe surface | Default speech-to-text model since September 2026 (POST /v1/stt and wss://api.x.ai/v1/stt); diarization, keyterms, multichannel. $0.10 per h… |
| `grok-voice-transcribe-1.0` | — | `retired` | shutdown 2026-10-02 | text, audio | text | — | — | not supported: speech-to-text has no GAISe surface | End of life 2026-10-02; requests to the slug are routed to grok-voice-transcribe-2.0. Replacement `grok-voice-transcribe-2.0`. |
| `grok-imagine-image-2.0` | — | `active` | — | image | image | — | — | not yet native: Images API (/v1/images/generations and /edits) | Recommended image model; quality low, medium, or auto; resolutions 1k, 1.5k, 2k; up to 5 source images for edits. $0.04 per image. |
| `grok-imagine-image` | — | `active` | — | image | image | — | — | not yet native: Images API | Grok Imagine Image 1.0 (snapshot grok-imagine-image-2026-03-02). $0.02 per image. |
| `grok-imagine-image-quality` | `grok-imagine-image-quality-20260403`, `grok-imagine-image-quality-latest`, `grok-imagine-image-pro` | `deprecated` | shutdown 2026-11-02 | image | image | — | — | not yet native: Images API | Retires 2026-11-02; requests to the slug will then be served by grok-imagine-image-2.0 with quality low. grok-imagine-image-pro was retired… |
| `grok-imagine-video-1.5` | `grok-imagine-video-1.5-preview` | `active` | — | image | video | — | — | not yet native: asynchronous Videos API (/v1/videos/generations) | Text or image to video (snapshot grok-imagine-video-1.5-2026-05-30); 1-15 s, 480p/720p/1080p. $0.080 per second. |
| `grok-imagine-video-1.5-lite` | — | `active` | — | image | video | — | — | not yet native: asynchronous Videos API | Lower-cost Grok Imagine Video 1.5. $0.020 per second. |
| `grok-imagine-video` | — | `active` | — | image, video | video | — | — | not yet native: asynchronous Videos API (generations, edits, extensions) | Grok Imagine Video 1.0; also edits and extends videos. $0.050 per second. |

## Limitations and explicit fallbacks

- The Responses API is not used for xAI. That means no encrypted reasoning round-trip, no server-side tools (web search, X search, code execution, collections, MCP), no stateful `previous_response_id`, and no multi-agent model.
- Embeddings return an error, because xAI lists no public embedding model.
- Image generation (`/v1/images/*`), video generation (`/v1/videos/*`), and speech to text (`/v1/stt`) have no GAISe surface. Their models are registered with no operations.
- Deferred completions, `service_tier`, `search_parameters`, and the Batch API are not sent.

## Tests

- [`tests/parameter_matrix_tests.rs`](../gaise-provider-xai/tests/parameter_matrix_tests.rs): the per-family effort rules.
- [`tests/mapping_tests.rs`](../gaise-provider-xai/tests/mapping_tests.rs): request, response, streaming, and TTS mapping, against a local HTTP server.
- [`tests/live_mapping_tests.rs`](../gaise-provider-xai/tests/live_mapping_tests.rs): `session.update` and the event mapping (feature `live`).
- [`gaise-client/tests/xai_routing_tests.rs`](../gaise-client/tests/xai_routing_tests.rs): routing through the client.

None of these has run against the live xAI API.

## Sources

- Models: <https://docs.x.ai/developers/models>
- Chat Completions: <https://docs.x.ai/developers/rest-api-reference/inference/chat-completions>
- Reasoning: <https://docs.x.ai/developers/model-capabilities/text/reasoning>
- Voice (realtime, TTS, STT): <https://docs.x.ai/developers/rest-api-reference/inference/voice> and <https://docs.x.ai/developers/model-capabilities/audio/speech-to-speech>
- Release notes: <https://docs.x.ai/developers/release-notes>
- May 15 retirement: <https://docs.x.ai/developers/migration/may-15-retirement>
