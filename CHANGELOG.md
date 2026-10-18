# Changelog

All notable changes to the GAISe crates are listed here. Every crate in the
workspace shares one version. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/): a new or changed model, a lifecycle
change, or an additive adapter feature is a minor release; a change to a public
contract (removed or renamed items, new required fields, changed serialization)
is a major release.

## [5.5.1] - 2026-10-17

### Fixed
- Parallel function calls in speech-to-speech sessions (xAI `grok-voice-*`
  and OpenAI Realtime). Before, each `ToolResponse` was followed by its own
  `response.create`, so with two parallel calls the model could resume
  speaking after the first result and before the second arrived. Now
  `response.create` is sent once, after the response that issued the calls
  has finished and every call has an output. That is what xAI requires
  ("Do not send `response.create` until all function call outputs have been
  submitted"). A single call behaves as before.

### Added
- `gaise_core::contracts::LiveToolCallGate`, the state machine both live
  clients use for this. It also has `observe_tool_calls` helpers in the xAI
  and OpenAI live modules.

## [5.5.0] - 2026-10-17

Media operations: speech-to-text (batch and streaming), image generation and
editing, and asynchronous video generation. The contracts were designed by
comparing OpenAI, Google, Bedrock, ElevenLabs, and xAI. Every change is
additive: new trait methods have defaults, and existing types, enums, and
model ids are unchanged.

### Added
- `gaise-core` media contracts:
  - Shared: `GaiseMediaSource` and `GaiseGeneratedMedia`.
  - Transcription: `GaiseTranscriptionRequest` / `Response` with words,
    segments, channels, and speakers.
  - Images: `GaiseImageRequest` / `Response`.
  - Video: `GaiseVideoRequest`, `GaiseVideoJob`, `GaiseVideoJobRequest`, and
    `GaiseJobStatus`.
  - Streaming transcription: `GaiseTranscriptionSessionConfig` /
    `Input` / `Event` / `Session`, with partial and final transcripts and
    end-of-utterance marks.
  - `GaiseAudioFormat`, plus the non-exhaustive `GaiseMediaOperation` enum.
- Default methods `GaiseClient::transcribe`, `generate_images`,
  `start_video`, and `get_video`, plus `GaiseLiveClient::transcribe_connect`.
- Router: routes all five methods by `provider::model` and logs size
  summaries, never media bytes. New helper
  `GaiseClientService::generate_video_and_wait`.
- HTTP:
  - `POST /v1/transcriptions` and `POST /v1/images`.
  - `POST /v1/videos` (answers `202` with the job) and
    `GET /v1/videos/{id}?model=`.
  - `GET /v1/transcriptions/live` (WebSocket, `live` feature).
- Providers:
  - OpenAI: transcription (`gpt-transcribe`, `whisper-1`,
    `gpt-4o-transcribe-diarize`) and images (gpt-image-2 and 2.5;
    generations and JSON edits).
  - Gemini API: transcription (`gemini-3.5-transcribe`) and Nano Banana
    images.
  - Vertex AI: transcription, Nano Banana images, and Veo 3.1 video
    (`predictLongRunning` / `fetchPredictOperation`).
  - Bedrock: Stability AI images (SD3.5 Large, Stable Image Ultra and
    Core) and Luma Ray 2 video (`StartAsyncInvoke`; needs an `s3://`
    `output_uri`).
  - ElevenLabs: Scribe v2 transcription and streaming transcription
    (`scribe_v2_realtime`).
  - xAI: transcription and streaming transcription (`/v1/stt`), Grok
    Imagine images, and Grok Imagine video (generate, edit, extend).
- Registry:
  - Capability terms `transcription`, `live_transcription`,
    `image_generation`, and `video_generation`, with
    `RegistryModel::media_operations`.
  - New rows:
    - Bedrock: `stability.sd3-5-large-v1:0`,
      `stability.stable-image-ultra-v1:1`,
      `stability.stable-image-core-v1:1`, and `luma.ray-v2:0`.
    - Vertex: `veo-3.1-generate-001`, `-fast-`, and `-lite-`, and
      `gemini-3.5-transcribe`.
    - ElevenLabs: `scribe_v2_realtime`.
    - Retired: Imagen (Gemini API and Vertex) and Veo 3.0.
    - Gemini API Veo 3.1 previews, deprecated with shutdown on
      2026-10-22.
  - The generated tables show media operations as `T`, `W`, `P`, and `M`.
- `wiki/media.md`, and media sections on every affected vendor page.

### Changed
- `elevenlabs::scribe_v2_realtime` now resolves to its own registry row,
  which carries `live_transcription`. Before, it was an alias of
  `scribe_v2`.

## [5.0.0] - 2026-10-17

xAI (Grok) is a new provider covering chat, text-to-speech, and
speech-to-speech. This is a major release because `GaiseClientConfig` gains
public fields. Nothing changes for existing model ids.

### Added
- New crate `gaise-provider-xai`:
  - `GaiseClientXai` implements `GaiseClient` (`instruct`, `instruct_stream`,
    `list_models`) on xAI Chat Completions, with function tools, image input,
    `reasoning_content` mapped to reasoning parts, and usage including
    `cost_in_usd_ticks`.
  - `GaiseClientXai` also implements `GaiseSpeechClient` on `POST /v1/tts`,
    routed as `xai::tts` because the endpoint has no model id. It supports
    mp3, wav, pcm, mulaw, and alaw output, and character timestamps.
  - `GaiseClientXaiLive` (feature `live`) runs speech-to-speech over
    `wss://api.x.ai/v1/realtime` (`grok-voice-3.0`, `grok-voice-latest`):
    24 kHz PCM16, server VAD or manual turns, function tools, and input
    transcription.
  - `xai_chat_rules` sets the `reasoning_effort` values each family accepts:
    - grok-4.7 and 4.6: low to xhigh; reasoning cannot be disabled.
    - grok-4.5: low to high.
    - grok-4.3: none to xhigh.
    - Never sent for grok-4.20, grok-build-0.1, or the retired slugs.
    - The multi-agent, Imagine, and voice models fail fast before any HTTP
      call.
- `gaise-client` feature `xai` (on by default). The router resolves `xai::`
  for instruct, speech, and live. `configured_providers` lists `xai` when a
  key is set.
- `gaise-api` reads `XAI_API_KEY` and `XAI_API_URL` and has a new `xai`
  feature (on by default). The speech routes are served when either
  `elevenlabs` or `xai` is enabled.
- Registry: an `xai` provider entry and 25 models.
  - Current text models: grok-4.7, grok-4.6, grok-4.5, grok-4.3, grok-4.20
    (reasoning and non-reasoning), and grok-build-0.1.
  - The grok-4.20 multi-agent model is registered as Responses-only.
  - Slugs retired on 2026-05-15 are marked retired; they redirect upstream
    to grok-4.3.
  - Grok Voice 3.0, and Grok Voice 2.0 (deprecated).
  - `tts`, the speech-to-text models, and the Grok Imagine image and video
    models.
- Registry capability term `video_output`.
- `wiki/vendor-xai.md`.

### Changed
- **Breaking:** `GaiseClientConfig` has new `xai_api_url` and `xai_api_key`
  fields (behind the `xai` feature). Code that builds the struct with a
  literal and no `..Default::default()` must add them.
- `GaiseClientService`'s speech methods and its `GaiseSpeechClient` impl are
  compiled when either the `elevenlabs` or the `xai` feature is enabled.

## [4.5.0] - 2026-10-16

Model audit of every provider. TypeSafe now takes images (Jev 1.13 and 1.14);
Vertex AI 12-month dates for Gemini 4 Argon and 3.8 Flash/Live; new Ollama
decision and Nemotron families. Plain `decision` requests are unchanged for
every existing model id.

### Added
- TypeSafe image input: `GaiseClientTypeSafe` implements
  `GaiseClient::decision_with_images` (so `POST /v1/decision` with `images` now
  works for `typesafe::*`). Images go in TypeSafe's top-level `assets` map as
  base64 PNG/JPEG under fresh references, with the
  `TypeSafe-Danger-Zone-Images: true` header; the references are listed under
  `images` in the state (an object or `null` state gains the field; any other
  state is wrapped as `{"state": ..., "images": [...]}`). TypeSafe labels images
  Dreamer-ready.
- `GaiseClientTypeSafe::decision_with_assets` for caller-placed image
  references, `GaiseClientTypeSafe::with_long_context` (sends
  `TypeSafe-Danger-Zone-Long-Context: true`, 262k tokens), the `IMAGES_HEADER`
  and `LONG_CONTEXT_HEADER` constants, and `contracts::ImageAsset`.
- Registry rows: `typesafe::jev-1.14.0` (preview, released 2026-10-14), Ollama
  decision families `d1:*` and `pplx-decider:*` (both take images), and Ollama
  chat families `nemotron-3.5-super:*`, `nemotron-3.5-bolt:*`, and
  `muse-spark-open:*` (cloud tag only).
- Hermetic TypeSafe image tests and lookup pins for every new or changed row.

### Changed
- `typesafe::jev` gains the `image_input` capability. The `jev-preview` alias
  now resolves to the `jev-1.14.0` row, following TypeSafe, which moved it to
  Jev 1.14 on 2026-10-14; `jev`, `jev-latest`, and `jev-1.13.0` are unchanged.
- Vertex AI: `gemini-4-argon` (not before 2027-10-13), `gemini-3.8-live`
  (2027-09-25), and `gemini-3.8-flash` (2027-09-02) now carry the published
  12-month retirement floors; `gemini-3.8-flash` moves from `short_term_active`
  to `active`.
- Ollama: Clef runs in Ollama's cloud since v0.40.5 (`clef:cloud` through a
  signed-in local server); the local 64 KiB limit now excludes images.
- Anthropic: fast mode for Opus 5.5 left research preview (GAISe never sends
  `speed`). OpenAI: GPT-Live's native Decisions delegation noted. Bedrock:
  Claude Sonnet 4 is past its EOL date but still labelled Legacy; Claude 3 Haiku
  regional EOL on 2026-10-17. Gemini API and ElevenLabs: audit notes only.

## [4.4.0] - 2026-10-14

Model audit of every provider: Gemini 4 Argon on both Google surfaces, three
new Ollama decision families, and the GPT-4o / GPT-4.1 deprecations. Registry
and documentation only; no request shape changes.

### Added
- Registry rows: `gemini::gemini-4-argon` (GA 2026-10-13), `vertexai::gemini-4-argon`
  and `vertexai::gemini-4-argon-cyber-permissive` (allowlisted), and the Ollama
  decision families `nimble-v3:*`, `kev:*`, and `strands-decider:*`.
- Hermetic tests pinning Gemini 4 Argon's token-budget-to-`thinkingLevel`
  mapping in the Gemini and Vertex AI crates, and lookup pins for every new row.

### Changed
- Gemini 4 Argon is now documented rather than a forward guard: it already
  received `thinkingLevel` (LOW/MEDIUM/HIGH, MINIMAL clamped to LOW) and no
  sampling. Non-streaming requests above 32,768 output tokens return 400
  upstream; GAISe does not implement long decode.
- Bedrock: Claude Sonnet 4.5 and Llama 4 Maverick Legacy dates, Pegasus 1.5
  launch date, GPT-5.6 limits (1,050,000 / 128,000), GPT-6 Sol/Luna InvokeModel
  support, and the removed Opus 5 access gate.
- Ollama decision limits updated for v0.40.4 (2-10 score levels, empty state
  rejected server-side) and the `think` string rules.
- Anthropic Haiku 5.5 and Mythos 5 / 5.1 notes; ElevenLabs and TypeSafe audit notes.

### Deprecated
- OpenAI `gpt-4o`, `gpt-4o-mini`, `gpt-4.1`, and `gpt-4.1-mini` (removal
  2027-01-14, replacement `gpt-6-luna`).
- Gemini API `gemini-3.7-flash` and `gemini-3.5-flash` (no shutdown date;
  requests are routed to `gemini-3.8-flash` and `gemini-3.6-flash`). Vertex AI
  lifecycles are unchanged.

## [4.3.0] - 2026-10-09

Model audit of every provider, plus OpenAI's Decisions API (public beta since
2026-10-06), image input for decisions, and multimodal embeddings for
`gemini-embedding-2`. Every change is additive.

### Added
- OpenAI Decisions API: `GaiseClient::decision` on `openai::gpt-6-luna` sends
  `POST /v1/decisions`. Questions map to named `predicate` / `choice` / `score`
  questions, answers are matched back by name, and a refused question fails the
  call. New `gaise_provider_openai::decisions` module (`decision_request`,
  `decision_request_with_images`, `map_decision_answers`, `map_decision_usage`).
- `GaiseClient::decision_with_images` (default: unsupported unless the image list
  is empty), implemented for OpenAI (base64 data URLs, up to 128 images) and
  Ollama (`images` on `/v1/systemone` for Clef and Clef Flash), and forwarded by
  the router. `POST /v1/decision` accepts an optional `images` array.
- `GaiseContentEmbeddingsRequest` and `GaiseClient::embed_contents`: one embedding
  per `GaiseContent` item (a `parts` item is one aggregated embedding). Gemini
  sends images, audio, video (`File` named `*.mp4` / `*.mov`), and PDFs to
  `gemini-embedding-2`; text-only requests fall back to `embeddings` on every
  client. New route `POST /v1/embeddings/contents`.
- Anthropic: `claude-haiku-5-5`. Bedrock: `anthropic.claude-haiku-5-5`,
  `zai.glm-5.3`, and `twelvelabs.pegasus-1-5-v1:0` (listed only).
- Gemini API and Vertex AI: `gemini-nano-banana-2.1`.
- Ollama: `clef`, `clef-flash`, and `laya` decision models, `mistral-large-4`
  (cloud), and `embeddinggemma-2`. ElevenLabs: `scribe_v2_medical` (listed only).

### Changed
- `claude-haiku-5*` ids (Anthropic and Bedrock) use the Opus 5 / Sonnet 5 rules:
  adaptive thinking only, five effort levels, thinking can be turned off, and no
  sampling parameters. Before this release they fell through to manual budgets.
- `gemini-nano-banana-*` ids (Gemini and Vertex) use `thinkingLevel` with MINIMAL,
  MEDIUM, and HIGH and never receive sampling parameters, instead of the
  Gemini 2.5 `thinkingBudget` path.
- `gpt-6-luna` gains the `decision` capability and operation.
- Lifecycle: `gpt-5.1` and `gpt-5.4-nano` deprecated (shutdown 2027-04-01);
  `gpt-image-1` and `gpt-image-1.5` now point to GPT Image 2.5; `claude-haiku-4-5`
  is Legacy; Gemini API `gemini-3.1-flash-image` deprecated, and new shutdown
  dates for the 3.1 Flash Live, 2.5 native audio, 3.1 Flash TTS, Omni Flash
  previews, and 2.5 Flash Image; Vertex `gemini-3.6-flash` (2026-11-19) and
  `gemini-3.7-flash` (2027-01-28) retirement dates; Bedrock Claude Sonnet 4.5
  and Llama 4 Maverick are Legacy, and the Haiku 4.5, Opus 4.5, and Sonnet 5.5
  notes are corrected.

## [4.2.0] - 2026-10-01

Model audit of every provider. Gemini 4 Argon was announced on 2026-09-30 but
has no API model id yet, so it has no registry row; Gemini 4 ids get the Gemini 3
request rules in advance.

### Added
- Gemini API: `gemini-3.8-live`, `gemini-3.8-live-extended-thinking`,
  `gemini-3.8-flash-tts`, and `gemini-3.8-flash-lite-tts`.
- Vertex AI: `gemini-3.8-flash-cyber` (allowlist) and `gemini-3.8-live` (listed
  only; the Vertex crate has no Live transport).
- Bedrock: `openai.gpt-6.1-sol`, `openai.gpt-6-sol`, `openai.gpt-6-luna`,
  `moonshotai.kimi-k3`, and `xai.grok-4.7`.
- ElevenLabs: `eleven_v4` and `eleven_v4_turbo`, served through the
  text-to-dialogue WebSocket.
- `gemini_major_version` (Gemini and Vertex crates) and
  `live_model_rejects_thinking_config` (Gemini crate).
- Ollama listings map the `/api/show` `decision` capability (Ollama 0.35.1) to the
  `decision` operation.

### Changed
- Gemini 4 and later ids (`gemini-<major>` with major 4 or more) use
  `thinkingLevel` without MINIMAL and never receive sampling parameters, instead of
  the Gemini 2.5 `thinkingBudget` path. This is a forward guard until Google
  publishes a Gemini 4 contract.
- The Gemini Live transport no longer sends `thinkingConfig` to `gemini-3.8-live`,
  and clamps MINIMAL to LOW on `gemini-3.8-live-extended-thinking`.
- Replacement models and dates updated for Gemini 3.1 Flash Live, the 2.5 native
  audio and TTS previews, Vertex Gemini 3.6/3.7 Flash and 2.5, and the Vertex
  2.5 Flash Image retirement (now 2027-03-15).
- The Bedrock gpt-oss note reflects AWS's correction: Responses is served on
  bedrock-mantle only.

### Deprecated
- `claude-sonnet-4-5-20250929`: retires on the Claude API on 2026-11-30;
  replacement `claude-sonnet-5-5`.

### Removed
- Marked retired (ids still resolve): Gemini API `gemini-omni-flash-preview`, and
  Bedrock Nova Premier, Nova Sonic v1, Nova Reel v1, and Nova Canvas.

## [4.1.0] - 2026-09-30

### Added
- Ollama decision models: `ollama::nimble` and `ollama::tev1` work through
  `GaiseClient::decision`, using Ollama's `POST /v1/systemone` (Ollama 0.35+).
- Registry rows `nimble:*` and `tev1:*`; Ollama model listings mark them with the
  `decision` operation.
- Shared System One wire helpers in `gaise`: `DecisionWireRequest`,
  `DecisionWireResponse`, `DecisionWireUsage`, `GaiseDecisionRequest::wire`, and
  `GaiseDecisionRequest::check_answers`.

### Changed
- `POST /v1/systemone` is documented as a supported alias of `POST /v1/decision`
  instead of a deprecated one.

## [4.0.1] - 2026-09-30

### Added
- GPT-6 Astra and GPT-6.1 Sol tool calling: tool conversations for these models
  are sent to the OpenAI Responses API (stateless, with reasoning replayed through
  `GaiseContent::Reasoning`). Tool-free requests stay on Chat Completions.

### Changed
- `gpt-6-astra` and `gpt-6.1-sol` are marked tool-capable in the registry.

## [4.0.0] - 2026-09-30

### Changed
- **Breaking:** the typed-decision operation is renamed from `system_one` to
  `decision`: `GaiseClient::decision`, `GaiseDecisionRequest` /
  `GaiseDecisionResponse`, `GaiseOperation::Decision` (serialized `decision`),
  `POST /v1/decision`, and the `decision` registry capability. The old Rust names
  remain as deprecated aliases.
- Opus 5.5 and Sonnet 5.5 use the always-on thinking profile on the Anthropic and
  Bedrock adapters; GPT-6 Sol and Luna follow the GPT-5.6 Chat Completions rules.

### Added
- Models `gpt-6.1-sol`, `gpt-6-sol`, `gpt-6-luna`, `claude-opus-5-5`,
  `claude-sonnet-5-5`, `anthropic.claude-opus-5-5`, and
  `anthropic.claude-sonnet-5-5`.

### Deprecated
- `claude-opus-5` and `claude-sonnet-5` moved to `legacy` (still served).

## [3.0.1]

### Changed
- Documentation for System One and the provider packages.

## [3.0.0]

### Added
- TypeSafe AI provider (`gaise-provider-typesafe`) with Jev typed decisions
  (System One).

## [0.2.3]

### Changed
- Relicensed to MIT OR Apache-2.0. Releases 0.2.2 and earlier remain AGPL.

## [0.2.2] - 2026-09-12

### Added
- Model audit 2026-09-12: GPT-Live 1, GPT-6 Astra general availability, Ollama
  GLM 5.3 and Granite 4.2. Fixed Gemini Live VAD sensitivity values.

## [0.2.1] - 2026-09-04

### Added
- Model audit 2026-09-04: GPT-6 Astra, Claude 5.1, Gemini 3.8, and the Bedrock
  catalog.

## [0.2.0]

### Added
- Model discovery, per-family parameter rules, a shared reasoning vocabulary,
  ElevenLabs speech, standardized embeddings, the model limits matrix
  (`/v1/models/limits`), and the wiki.

[5.5.1]: https://github.com/ikcore/gaise/pull/23
[5.5.0]: https://github.com/ikcore/gaise/pull/22
[5.0.0]: https://github.com/ikcore/gaise/pull/21
[4.5.0]: https://github.com/ikcore/gaise/pull/20
[4.4.0]: https://github.com/ikcore/gaise/pull/19
[4.3.0]: https://github.com/ikcore/gaise/pull/18
[4.2.0]: https://github.com/ikcore/gaise/pull/17
[4.1.0]: https://github.com/ikcore/gaise/commit/81116ed
[4.0.1]: https://github.com/ikcore/gaise/pull/15
[4.0.0]: https://github.com/ikcore/gaise/pull/14
[3.0.1]: https://github.com/ikcore/gaise/pull/13
[3.0.0]: https://github.com/ikcore/gaise/pull/12
