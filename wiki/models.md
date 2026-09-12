# Models

> Part of the [GAISe wiki](README.md) · [Capabilities](capabilities.md) · [HTTP API](api.md#get-v1models) · [Rust SDK](sdk.md#model-discovery) · [Flows](flows.md#model-discovery) · Vendors: [OpenAI](vendor-openai.md) · [Anthropic](vendor-anthropic.md) · [Google Gemini API](vendor-gemini.md) · [Google Vertex AI](vendor-vertexai.md) · [Amazon Bedrock](vendor-bedrock.md) · [Ollama](vendor-ollama.md) · [ElevenLabs](vendor-elevenlabs.md)

This page is the human-readable view of [`gaise-core/model-registry.toml`](../gaise-core/model-registry.toml) (schema 2, audited **2026-09-12**), which is compiled into the `gaise` crate and applied as an overlay by [`list_models`](api.md#get-v1models). It is advisory: GAISe accepts arbitrary model IDs so new releases work before this file is updated, and the provider's own model API (see [Model discovery](capabilities.md#model-discovery)) is always the first source of truth.

This page is generated from the registry by [`cargo run -p gaise --example models_page`](../gaise-core/examples/models_page.rs); edit the registry (or that example's introductions), not this file. Columns:

- **Input / Output** — modalities classified from the entry's `capabilities` list by [`classify_capabilities`](../gaise-core/src/registry.rs).
- **Ops** — GAISe operations the entry maps to: `I` instruct, `S` instruct_stream, `E` embeddings, `V` speech (voice), `L` live. Empty means no GAISe surface drives the model (image generation, TTS, bidirectional audio).
- **Tools / Reasoning** — ✓ supported, ✗ not listed, and the `reasoning_values` the provider documents.
- **Dates** — `shutdown` is a published retirement date; `not before` is an availability guarantee. Gemini API and Vertex AI dates are **never** interchangeable.
- **Limits** — context windows, output ceilings, per-input token limits, and character budgets are not repeated here; see [limits.md](limits.md) for the generated model × limits matrix and `GET /v1/models/limits`.

## Contents


- [OpenAI](#openai) — 58 entries
- [Anthropic](#anthropic) — 17 entries
- [Google Gemini API](#gemini) — 29 entries
- [Google Vertex AI](#vertexai) — 25 entries
- [Amazon Bedrock](#bedrock) — 59 entries
- [Ollama](#ollama) — 31 entries
- [ElevenLabs](#elevenlabs) — 14 entries
- [Maintaining the registry](#maintaining-the-registry)
- [Lifecycle calendar](#lifecycle-calendar)

## openai

### OpenAI

Instruct uses **Chat Completions**; Responses-only models (GPT-5.5 Pro, gpt-5.6-cyber, the Daybreak models, image generation) are listed but cannot be driven, and GPT-6 function tools are refused because OpenAI serves them through Responses only. `GET /v1/models` reports identity only, so everything in the Input/Output/Ops columns is registry- or heuristic-sourced at runtime ([`catalog.rs`](../gaise-provider-openai/src/contracts/catalog.rs)).

- Vendor page: [vendor-openai.md](vendor-openai.md) · GAISe surface: Chat Completions, Embeddings, and Realtime
- Discovery: GET /v1/models (id, created, owned_by, shutdown_date only)
- Official catalog: <https://developers.openai.com/api/docs/models> · lifecycle: <https://developers.openai.com/api/docs/deprecations>
- The current instruct client uses Chat Completions. Responses-only features, the Images API, and the Live API (/v1/live/sessions, GPT-Live 1, 2026-09-10) are outside that client. Chat Completions contract as of 2026-09-12: reasoning_effort accepts none, minimal, low, medium, high, xhigh, max (per-model subsets); service_tier accepts auto, default, flex, scale, priority, and fast (Priority processing was renamed Fast mode on 2026-07-30; the response reports priority for fast requests); prompt_cache_options is {mode: implicit|explicit, ttl: 30m} on gpt-5.6 and later and prompt_cache_retention is deprecated; usage.prompt_tokens_details reports image_tokens and text_tokens; the Assistants API shut down 2026-08-26.

#### Current and preview

| Model | Aliases | Status | Dates | Input | Output | Ops | Tools | Reasoning | GAISe support / notes |
|---|---|---|---|---|---|---|---|---|---|
| `gpt-6-astra` | — | `active` | — | text, image | text | IS | ✗ | ✓ (low, medium, high, xhigh, max) | chat-compatible features without function tools — Released 2026-09-03 as a limited preview and generally avai… |
| `gpt-5.6` | `gpt-5.6-sol` | `active` | — | text, image | text | IS | ✓ | ✓ (none, low, medium, high, xhigh, max) | chat-compatible features — OpenAI documents gpt-5.6-sol as the snapshot ID and gpt-5.6 as the alias that rout… |
| `gpt-5.6-terra` | — | `active` | — | text, image | text | IS | ✓ | ✓ (none, low, medium, high, xhigh, max) | chat-compatible features — On Chat Completions, function tools require reasoning_effort='none'; the adapter a… |
| `gpt-5.6-luna` | — | `active` | — | text, image | text | IS | ✓ | ✓ (none, low, medium, high, xhigh, max) | chat-compatible features — On Chat Completions, function tools require reasoning_effort='none'; the adapter a… |
| `gpt-5.5` | — | `active` | — | text, image | text | IS | ✓ | ✓ (none, low, medium, high, xhigh) | chat-compatible features — Defaults to medium reasoning effort. |
| `gpt-5.5-pro` | — | `active` | — | text, image | text | — | ✓ | ✓ (medium, high, xhigh) | not reachable through the Chat Completions instruct client — OpenAI lists Chat Completions as not supported;… |
| `gpt-5.4` | — | `active` | — | text, image | text | IS | ✓ | ✓ (none, low, medium, high, xhigh) | chat-compatible features |
| `gpt-5.4-mini` | — | `active` | — | text, image | text | IS | ✓ | ✓ (none, low, medium, high, xhigh) | chat-compatible features |
| `gpt-5.4-nano` | — | `active` | — | text, image | text | IS | ✓ | ✓ (none, low, medium, high, xhigh) | chat-compatible features |
| `gpt-5.2` | `gpt-5.2-2025-12-11` | `active` | — | text, image | text | IS | ✓ | ✓ (none, low, medium, high, xhigh) | chat-compatible features — Previous flagship; OpenAI recommends GPT-6 Astra or GPT-5.6. Not on the deprecatio… |
| `gpt-5.1` | `gpt-5.1-2025-11-13` | `active` | — | text, image | text | IS | ✓ | ✓ (none, low, medium, high) | chat-compatible features — Not on the deprecations page as of 2026-09-04 (only gpt-5.1-chat-latest and the 5.… |
| `gpt-4.1` | `gpt-4.1-2025-04-14` | `active` | — | text, image | text | IS | ✓ | ✗ | chat-compatible features — Non-reasoning; sampling accepted; image detail limited to low/high/auto. gpt-4.1-n… |
| `gpt-4.1-mini` | `gpt-4.1-mini-2025-04-14` | `active` | — | text, image | text | IS | ✓ | ✗ | chat-compatible features |
| `gpt-4o` | `gpt-4o-2024-11-20`, `gpt-4o-2024-08-06` | `active` | — | text, image | text | IS | ✓ | ✗ | chat-compatible features — gpt-4o resolves to gpt-4o-2024-08-06. The gpt-4o-2024-05-13 snapshot alone retires… |
| `gpt-4o-mini` | `gpt-4o-mini-2024-07-18` | `active` | — | text, image | text | IS | ✓ | ✗ | chat-compatible features |
| `text-embedding-3-large` | — | `active` | — | text | embedding | E | ✗ | ✗ | native |
| `text-embedding-3-small` | — | `active` | — | text | embedding | E | ✗ | ✗ | native |
| `text-embedding-ada-002` | — | `active` | — | text | embedding | E | ✗ | ✗ | native — Previous generation; fixed 1536 dimensions, no retirement date published. |
| `gpt-realtime-2.1` | — | `active` | — | text, image, audio | text, audio | L | ✓ | ✓ (minimal, low, medium, high, xhigh) | realtime transport — The Realtime session reference enumerates reasoning.effort as minimal, low, medium, high… |
| `gpt-realtime-2.1-mini` | — | `active` | — | text, image, audio | text, audio | L | ✓ | ✓ (minimal, low, medium, high, xhigh) | realtime transport |
| `gpt-realtime-2` | — | `active` | — | text, image, audio | text, audio | L | ✓ | ✓ (minimal, low, medium, high, xhigh) | realtime transport |
| `gpt-realtime-1.5` | — | `active` | — | text, image, audio | text, audio | L | ✓ | ✗ | realtime transport — No reasoning controls. |
| `gpt-realtime-translate` | — | `active` | — | audio | audio | — | ✗ | ✗ | not supported: the /v1/realtime/translations endpoint uses its own session and event vocabulary — Streaming s… |
| `gpt-audio-1.5` | — | `active` | — | text, audio | text, audio | IS | ✓ | ✗ | Chat audio input is native; audio output is not mapped by the current instruct client — Chat Completions supp… |
| `gpt-image-2` | — | `active` | — | image | image | — | ✗ | ✗ | not yet native — Requires OpenAI Images or Responses image-generation tooling; the GAISe OpenAI instruct clie… |
| `gpt-image-2.5-sunburst` | — | `active` | — | image | image | — | ✗ | ✗ | not yet native — Released 2026-09-08 (default snapshot gpt-image-2.5-sunburst-2026-09-08); the editing-precis… |
| `gpt-image-2.5-flare` | — | `active` | — | image | image | — | ✗ | ✗ | not yet native — Released 2026-09-08 (default snapshot gpt-image-2.5-flare-2026-09-08); the fast everyday GPT… |
| `gpt-live-1` | — | `active` | — | text, audio | text, audio | — | ✓ | ✗ | not supported: the Live API (wss://api.openai.com/v1/live/sessions) has its own session.start / session.input… |
| `gpt-5.6-cyber` | — | `limited_availability` | — | text, image | text | — | ✓ | ✓ | not reachable: Responses API only, Daybreak program approval required — Daybreak Red model (2026-08-12). 400K… |
| `gpt-daybreak-*` | — | `limited_availability` | — | text | text | — | ✓ | ✓ | not reachable: Responses API only, Daybreak program approval required — gpt-daybreak-red-latest and gpt-daybr… |
| `gpt-rosalind-research` | — | `limited_availability` | — | text | text | — | ✗ | ✓ | not verified: no model page is published and trusted-access approval is required; the instruct client forward… |
| `gpt-transcribe` | — | `active` | — | text, audio | text | — | ✗ | ✗ | not supported: speech-to-text has no GAISe surface — Released 2026-07-28; /v1/audio/transcriptions and realti… |
| `gpt-live-transcribe` | — | `active` | — | text, audio | text | — | ✗ | ✗ | not supported: realtime transcription sessions have no GAISe surface — Released 2026-07-28; /v1/realtime/tran… |
| `gpt-realtime-whisper` | — | `active` | — | text, audio | text | — | ✗ | ✗ | not supported: realtime transcription sessions have no GAISe surface — turn_detection must be null for this m… |
| `gpt-4o-mini-tts` | `gpt-4o-mini-tts-2025-12-15`, `gpt-4o-mini-tts-2025-03-20` | `active` | — | text | text, audio | — | ✗ | ✗ | not supported: OpenAI text-to-speech has no GAISe surface (use elevenlabs::) — /v1/audio/speech only; 2,000 i… |

#### Deprecated and legacy

| Model | Aliases | Status | Dates | Input | Output | Ops | Tools | Reasoning | GAISe support / notes |
|---|---|---|---|---|---|---|---|---|---|
| `gpt-5-2025-08-07` | `gpt-5` | `deprecated` | shutdown 2026-12-11 | — | — | — | ? | ? | The deprecations page lists the dated snapshot; the gpt-5 alias resolves to it and has no other snapshot. |
| `gpt-5-mini-2025-08-07` | `gpt-5-mini` | `deprecated` | shutdown 2026-12-11 | — | — | — | ? | ? | — |
| `gpt-5-nano-2025-08-07` | `gpt-5-nano` | `deprecated` | shutdown 2026-12-11 | — | — | — | ? | ? | — |
| `gpt-5-pro-2025-10-06` | `gpt-5-pro` | `deprecated` | shutdown 2026-12-11 | — | — | — | ? | ? | Responses and Batch only; reasoning.effort high only. |
| `o3-2025-04-16` | `o3` | `deprecated` | shutdown 2026-12-11 | — | — | — | ? | ? | — |
| `o3-pro-2025-06-10` | `o3-pro` | `deprecated` | shutdown 2026-12-11 | — | — | — | ? | ? | Responses and Batch only. |
| `o4-mini` | `o4-mini-2025-04-16` | `deprecated` | shutdown 2026-10-23 | — | — | — | ? | ? | — |
| `o1` | `o1-2024-12-17`, `o1-pro`, `o1-pro-2025-03-19` | `deprecated` | shutdown 2026-10-23 | — | — | — | ? | ? | Announced 2026-04-22. o1-pro is Responses-only and replaced by gpt-5.6-sol with reasoning.mode pro. |
| `o3-mini` | `o3-mini-2025-01-31` | `deprecated` | shutdown 2026-10-23 | — | — | — | ? | ? | — |
| `gpt-4o-2024-05-13` | — | `deprecated` | shutdown 2026-10-23 | — | — | — | ? | ? | Only this gpt-4o snapshot retires; gpt-4o and the 2024-08-06 / 2024-11-20 snapshots carry no deprecation. |
| `gpt-4-turbo` | `gpt-4-turbo-2024-04-09` | `deprecated` | shutdown 2026-10-23 | — | — | — | ? | ? | — |
| `gpt-4` | `gpt-4-0613`, `gpt-4-1106-preview` | `deprecated` | shutdown 2026-10-23 | — | — | — | ? | ? | gpt-4-1106-preview is 128K context. |
| `gpt-3.5-turbo` | `gpt-3.5-turbo-0125` | `deprecated` | shutdown 2026-10-23 | — | — | — | ? | ? | gpt-3.5-turbo-1106, gpt-3.5-turbo-instruct, babbage-002, and davinci-002 shut down earlier, on 2026-09-28. |
| `gpt-4.1-nano` | `gpt-4.1-nano-2025-04-14` | `deprecated` | shutdown 2026-10-23 | — | — | — | ? | ? | — |
| `gpt-image-1` | — | `deprecated` | shutdown 2026-10-23 | — | — | — | ? | ? | — |
| `gpt-image-1.5` | `gpt-image-1-mini`, `chatgpt-image-latest` | `deprecated` | shutdown 2026-12-01 | — | — | — | ? | ? | — |
| `gpt-realtime` | `gpt-4o-realtime`, `gpt-realtime-mini`, `gpt-4o-mini-realtime` | `deprecated` | shutdown 2027-01-20 | — | — | — | ? | ? | Announced 2026-07-20. Context 32K for gpt-realtime, gpt-realtime-mini, and gpt-4o-realtime. The gpt-4o-*-real… |
| `gpt-audio` | `gpt-4o-audio`, `gpt-audio-mini`, `gpt-4o-mini-audio` | `deprecated` | shutdown 2027-01-20 | — | — | — | ? | ? | — |
| `whisper-1` | `gpt-4o-transcribe`, `gpt-4o-mini-transcribe`, `gpt-4o-transcribe-diarize` | `deprecated` | shutdown 2027-02-26 | text, audio | text | — | ✗ | ✗ | not supported: speech-to-text has no GAISe surface — Deprecation announced 2026-08-26 for the whole legacy tr… |

#### Retired

| Model | Aliases | Status | Dates | Input | Output | Ops | Tools | Reasoning | GAISe support / notes |
|---|---|---|---|---|---|---|---|---|---|
| `gpt-5-chat-latest` | — | `retired` | shutdown 2026-07-23 | — | — | — | ? | ? | — |
| `gpt-5.1-chat-latest` | — | `retired` | shutdown 2026-07-23 | — | — | — | ? | ? | — |
| `gpt-5.2-chat-latest` | — | `retired` | shutdown 2026-08-10 | — | — | — | ? | ? | — |
| `gpt-5.3-chat-latest` | — | `retired` | shutdown 2026-08-10 | — | — | — | ? | ? | — |

## anthropic

### Anthropic

The Messages API. Anthropic's `GET /v1/models` reports image/PDF input, thinking types, effort levels, structured outputs, and token limits, so at runtime the registry contributes only lifecycle dates and notes ([`catalog.rs`](../gaise-provider-anthropic/src/contracts/catalog.rs)). Thinking and sampling rules by family are enforced in [`anthropic_client.rs`](../gaise-provider-anthropic/src/anthropic_client.rs). `legacy` mirrors Anthropic's own "Legacy" label (still served, no retirement date announced); the Models API keeps reporting those ids as active.

- Vendor page: [vendor-anthropic.md](vendor-anthropic.md) · GAISe surface: Messages
- Discovery: GET /v1/models (capabilities: image_input, pdf_input, thinking types, effort levels, structured_outputs, token limits)
- Official catalog: <https://platform.claude.com/docs/en/about-claude/models/overview> · lifecycle: <https://platform.claude.com/docs/en/about-claude/model-deprecations>
- Direct Claude API lifecycle. Bedrock-hosted Claude has a separate AWS lifecycle. Anthropic labels every 4.x model except Haiku 4.5, plus Fable 5, as Legacy (still served, no retirement date) since the 2026-09-01 Fable 5.1 release; the registry mirrors that as status legacy. Messages contract as of 2026-09-12: temperature/top_p/top_k are deprecated and rejected at non-default values on Opus 4.7 and later; output_format is deprecated in favour of output_config.format; stop_reason may be refusal (with stop_details) on Fable/Opus 5 models and model_context_window_exceeded; the Files API and Skills API are out of beta (no header); thinking.display accepts updates behind the thinking-display-updates-2026-08-18 beta; stop_details.category is one of cyber, bio, frontier_llm, reasoning_extraction, general_harms, or null; server-side fallback (fallbacks: default or a list of up to three models, beta server-side-fallback-2026-07-01) and per-message effort (beta mid-conversation-output-config-2026-07-01, also on Google Cloud since 2026-09-03) are documented but not sent. No model, limit, or lifecycle change between 2026-09-04 and 2026-09-12.

#### Current and preview

| Model | Aliases | Status | Dates | Input | Output | Ops | Tools | Reasoning | GAISe support / notes |
|---|---|---|---|---|---|---|---|---|---|
| `claude-fable-5-1` | — | `active` | not before 2027-09-01 | text, image, file | text | IS | ✓ | ✓ adaptive (low, medium, high, xhigh, max) | native — Released 2026-09-01; Anthropic's most capable widely released model (same tier, limits, tokenizer, a… |
| `claude-mythos-5-1` | — | `limited_availability` | not before 2027-09-01 | text, image, file | text | IS | ✓ | ✓ adaptive (low, medium, high, xhigh, max) | native when account access exists — Released 2026-09-01, invite only (Project Glasswing); shares Fable 5.1's… |
| `claude-mythos-5` | — | `limited_availability` | not before 2027-06-09 | text, image, file | text | IS | ✓ | ✓ adaptive (low, medium, high, xhigh, max) | native when account access exists — Adaptive thinking is always on and cannot be disabled. Superseded by clau… |
| `claude-opus-5` | — | `active` | not before 2027-07-24 | text, image, file | text | IS | ✓ | ✓ adaptive (low, medium, high, xhigh, max) | native — Released 2026-07-24; Anthropic's recommended default model. Adaptive-only thinking, on by default (d… |
| `claude-sonnet-5` | — | `active` | not before 2027-06-30 | text, image, file | text | IS | ✓ | ✓ adaptive (low, medium, high, xhigh, max) | native — Adaptive thinking on by default (omitting thinking runs adaptive); thinking.type disabled accepted;… |
| `claude-haiku-4-5-20251001` | `claude-haiku-4-5` | `active` | not before 2026-10-15 | text, image, file | text | IS | ✓ | ✓ manual | native — Manual thinking budget; no adaptive thinking or effort parameter. |

#### Deprecated and legacy

| Model | Aliases | Status | Dates | Input | Output | Ops | Tools | Reasoning | GAISe support / notes |
|---|---|---|---|---|---|---|---|---|---|
| `claude-fable-5` | — | `legacy` | not before 2027-06-09 | text, image, file | text | IS | ✓ | ✓ adaptive (low, medium, high, xhigh, max) | native — Released 2026-06-09; labelled Legacy since 2026-09-01 (still served; migrate to claude-fable-5-1, wh… |
| `claude-opus-4-8` | — | `legacy` | not before 2027-05-28 | text, image, file | text | IS | ✓ | ✓ adaptive (low, medium, high, xhigh, max) | native — Labelled Legacy since 2026-09-01 (migrate to claude-opus-5). Adaptive-only thinking, off unless requ… |
| `claude-opus-4-7` | — | `legacy` | not before 2027-04-16 | text, image, file | text | IS | ✓ | ✓ adaptive (low, medium, high, xhigh, max) | native — Labelled Legacy since 2026-09-01. Adaptive-only thinking; budget_tokens and non-default temperature/… |
| `claude-opus-4-6` | — | `legacy` | not before 2027-02-05 | text, image, file | text | IS | ✓ | ✓ adaptive (low, medium, high, max) | native — Labelled Legacy since 2026-09-01. Adaptive thinking recommended; thinking.type enabled with budget_t… |
| `claude-opus-4-5-20251101` | `claude-opus-4-5` | `legacy` | not before 2026-11-24 | text, image, file | text | IS | ✓ | ✓ manual (low, medium, high) | native — Labelled Legacy since 2026-09-01. Manual thinking budget plus output_config.effort (low/medium/high)… |
| `claude-sonnet-4-6` | — | `legacy` | not before 2027-02-17 | text, image, file | text | IS | ✓ | ✓ adaptive (low, medium, high, max) | native — Labelled Legacy since 2026-09-01 (migrate to claude-sonnet-5). Adaptive thinking recommended; enable… |
| `claude-sonnet-4-5-20250929` | `claude-sonnet-4-5` | `legacy` | not before 2026-09-29 | text, image, file | text | IS | ✓ | ✓ manual | native — Labelled Legacy since 2026-09-01. Manual thinking budget only; no effort parameter. The retirement f… |
| `claude-mythos-preview` | — | `deprecated` | — | text, image, file | text | IS | ✓ | ✓ adaptive (low, medium, high, max) | Deprecated; migrate to claude-mythos-5-1 (Project Glasswing) or claude-fable-5-1. No retirement date is publi… |

#### Retired

| Model | Aliases | Status | Dates | Input | Output | Ops | Tools | Reasoning | GAISe support / notes |
|---|---|---|---|---|---|---|---|---|---|
| `claude-opus-4-1-20250805` | — | `retired` | shutdown 2026-08-05 | — | — | — | ? | ? | — |
| `claude-opus-4-20250514` | — | `retired` | shutdown 2026-06-15 | — | — | — | ? | ? | — |
| `claude-sonnet-4-20250514` | — | `retired` | shutdown 2026-06-15 | — | — | — | ? | ? | — |

## gemini

### Google Gemini API

Google AI Gemini API lifecycle only — see [Vertex AI](#vertexai) for Google Cloud. `models.list` reports `supportedGenerationMethods` (→ Ops), `thinking`, and token limits but no modalities ([`catalog.rs`](../gaise-provider-gemini/src/contracts/catalog.rs)). Gemini 2.5 uses `thinkingBudget`, 3.x uses `thinkingLevel`.

- Vendor page: [vendor-gemini.md](vendor-gemini.md) · GAISe surface: Gemini generateContent, streamGenerateContent, Embeddings, and Live
- Discovery: GET /v1beta/models (supportedGenerationMethods, token limits, thinking flag; no modalities)
- Official catalog: <https://ai.google.dev/gemini-api/docs/models> · lifecycle: <https://ai.google.dev/gemini-api/docs/deprecations>
- Google AI Gemini API lifecycle; do not reuse these dates for Vertex AI. Shutdown dates on the deprecations page are the earliest possible retirement dates. generateContent contract as of 2026-09-12: temperature/top_p/top_k are deprecated on every Gemini 3.x (the adapter omits them) and candidateCount is unsupported there; generationConfig gained responseFormat, enableAffectiveDialog, translationConfig, and audioTranscriptionConfig while responseSchema is deprecated in favour of responseJsonSchema; Part.mediaProcessing (STATIC | AGENTIC) selects agentic video understanding on 3.5 Flash-Lite, 3.6, 3.7, and 3.8; embedContent's top-level taskType/title/outputDimensionality are deprecated in favour of embedContentConfig (the adapter still sends the accepted top-level form). The thinking guide is now written against the Interactions API and lists the 2.5 family under thinking_level, but the REST reference and Vertex state that thinkingLevel errors on pre-3 models, so the adapter keeps thinkingBudget for 2.5. Requests also accept a top-level serviceTier (standard, flex, priority) and store, which the adapter does not send; batchEmbedContents returns usageMetadata; the Live API accepts only HIGH or LOW for start and end sensitivity (default HIGH), so the adapter omits any other value. No model or lifecycle change between 2026-09-04 and 2026-09-12.

#### Current and preview

| Model | Aliases | Status | Dates | Input | Output | Ops | Tools | Reasoning | GAISe support / notes |
|---|---|---|---|---|---|---|---|---|---|
| `gemini-3.8-flash` | — | `stable` | — | text, image, audio, video, file | text | IS | ✓ | ✓ (low, medium, high) | native — GA 2026-09-02; Google's most capable Flash model, aimed at long-horizon software engineering and age… |
| `gemini-3.7-flash` | — | `stable` | — | text, image, audio, video, file | text | IS | ✓ | ✓ (low, medium, high) | native — GA 2026-08-13. thinkingLevel minimal is not supported (default medium). Live API not supported; agen… |
| `gemini-3.6-flash` | — | `stable` | — | text, image, audio, video, file | text | IS | ✓ | ✓ (minimal, low, medium, high) | native — Released 2026-07-21. Fixed sampling: temperature, top_p, and top_k are deprecated (changelog 2026-07… |
| `gemini-3.5-flash` | — | `stable` | — | text, image, audio, video, file | text | IS | ✓ | ✓ (minimal, low, medium, high) | native |
| `gemini-3.5-flash-lite` | — | `stable` | — | text, image, audio, video, file | text | IS | ✓ | ✓ (minimal, low, medium, high) | native — thinkingLevel minimal (default), low, medium, high. Agentic video understanding supported since 2026… |
| `gemini-3.1-flash-lite` | — | `stable` | shutdown 2027-05-07 | text, image, audio, video, file | text | IS | ✓ | ✓ (minimal, low, medium, high) | native — thinkingLevel minimal (default), low, medium, high per the Gemini 3.5 guide's comparison table; the… |
| `gemini-3.1-pro-preview` | — | `preview` | — | text, image, audio, video, file | text | IS | ✓ | ✓ (low, medium, high) | native — No shutdown date announced. A gemini-3.1-pro-preview-customtools variant endpoint exists. |
| `gemini-3-flash-preview` | — | `preview` | — | text, image, audio, video, file | text | IS | ✓ | ✓ (minimal, low, medium, high) | native — No shutdown date announced. |
| `gemini-3.1-flash-live-preview` | — | `preview` | — | text, image, audio, video | text, audio | L | ✓ | ✓ (minimal, low, medium, high) | native Live API transport — Released 2026-03-11; no shutdown date announced. |
| `gemini-2.5-flash-native-audio-preview-12-2025` | — | `preview_legacy` | — | text, audio, video | text, audio | L | ✓ | ✓ | Live transport; migration recommended — Live guide still documents thinkingBudget (0 disables) for this model. |
| `gemini-3.1-flash-image` | — | `stable` | — | text, image, video, file | text, image | IS | ✗ | ✓ (minimal, high) | native image output through generateContent — Released 2026-05-28. Function calling and structured outputs ar… |
| `gemini-3.1-flash-lite-image` | — | `stable` | — | text, image | text, image | IS | ✗ | ✓ (minimal, high) | native image output through generateContent — GA 2026-06-30 (Nano Banana 2 Lite); 1K output only. The model p… |
| `gemini-3-pro-image` | — | `stable` | — | text, image | text, image | IS | ✗ | ✓ | native image output through generateContent — Released 2026-05-28. Function calling is not supported. |
| `gemini-embedding-2` | `gemini-embedding-2-preview` | `stable` | — | text, image, audio, video | embedding | E | ✗ | ✗ | native — Released 2026-04-22. Text, image, video, audio, and PDF input. The catalog table still shows the -pr… |
| `gemini-3.1-flash-tts-preview` | — | `preview` | — | text | text, audio | — | ✗ | ✗ | text-to-speech is outside the instruct surface — Released 2026-04-13; replacement for the 2.5 TTS previews (g… |
| `gemini-3.5-transcribe` | `gemini-3.5-transcribe-live` | `stable` | — | text, audio | text | — | ✗ | ✗ | not supported: speech-to-text has no GAISe surface — GA 2026-08-26. gemini-3.5-transcribe is unary generateCo… |
| `gemini-3.5-live-translate-preview` | — | `preview` | — | audio | audio | — | ✗ | ✗ | not supported: speech-to-speech translation uses translationConfig on the Live API, which the live transport… |
| `gemini-omni-1.1-flash` | — | `stable` | — | text, image, video | text | — | ✗ | ✗ | not supported: video generation is served by the Interactions API only — GA 2026-08-27; conversational video… |
| `gemini-2.5-pro` | — | `stable` | — | text, image, audio, video, file | text | IS | ✓ | ✓ | native (thinkingBudget mapper path) — No shutdown date announced on the Gemini API deprecations page; the 202… |
| `gemini-2.5-flash` | — | `stable` | — | text, image, audio, video, file | text | IS | ✓ | ✓ | native (thinkingBudget mapper path) — No shutdown date announced on the Gemini API deprecations page. |
| `gemini-2.5-flash-lite` | — | `stable` | — | text, image, audio, video, file | text | IS | ✓ | ✓ | native (thinkingBudget mapper path) — Released 2025-07-22; thinking is off by default. No shutdown date annou… |

#### Deprecated and legacy

| Model | Aliases | Status | Dates | Input | Output | Ops | Tools | Reasoning | GAISe support / notes |
|---|---|---|---|---|---|---|---|---|---|
| `gemini-omni-flash-preview` | — | `deprecated` | shutdown 2026-09-30 | — | — | — | ? | ? | Released 2026-06-30; deprecated 2026-08-27. |
| `gemini-2.5-flash-image` | — | `deprecated` | shutdown 2026-10-02 | — | — | — | ? | ? | The deprecations page names gemini-3.1-flash-image-preview (itself shut down 2026-06-25); the GA replacements… |
| `gemini-embedding-001` | — | `deprecated` | shutdown 2028-05-14 | text | embedding | E | ✗ | ✗ | native — Still listed as callable; the earlier registry date (2026-07-14) was the release date. |

#### Retired

| Model | Aliases | Status | Dates | Input | Output | Ops | Tools | Reasoning | GAISe support / notes |
|---|---|---|---|---|---|---|---|---|---|
| `embedding-2-preview` | — | `retired` | shutdown 2026-08-10 | — | — | — | ? | ? | — |
| `gemini-2.0-flash` | — | `retired` | shutdown 2026-06-01 | — | — | — | ? | ? | — |
| `gemini-2.0-flash-lite` | — | `retired` | shutdown 2026-06-01 | — | — | — | ? | ? | — |
| `gemini-2.0-flash-live-001` | — | `retired` | shutdown 2025-12-09 | — | — | — | ? | ? | — |
| `gemini-live-2.5-flash-preview` | — | `retired` | shutdown 2025-12-09 | — | — | — | ? | ? | — |

## vertexai

### Google Vertex AI

Google Cloud lifecycle only. Model Garden listing returns names, versions, and launch stages — no modalities or limits ([`catalog.rs`](../gaise-provider-vertexai/src/contracts/catalog.rs)). Short-term-availability models retire 45 days after a designated replacement ships.

- Vendor page: [vendor-vertexai.md](vendor-vertexai.md) · GAISe surface: Vertex AI generateContent, streamGenerateContent, and Embeddings
- Discovery: GET {region}-aiplatform.googleapis.com/v1beta1/publishers/{publisher}/models (names, versionId, launchStage; no modalities or limits)
- Official catalog: <https://docs.cloud.google.com/vertex-ai/generative-ai/docs/learn/models> · lifecycle: <https://docs.cloud.google.com/gemini-enterprise-agent-platform/models/model-versions>
- Model availability and retirement dates are specific to Google Cloud. Short-term availability models retire 45 days after a replacement is released. Release notes moved to https://docs.cloud.google.com/gemini-enterprise-agent-platform/release-notes (the Vertex AI release-notes page is frozen at 2026-05-26). On 3.6+ Flash, 3.5 Flash-Lite, 3.7, and 3.8, custom temperature/topP/topK are ignored, frequency/presence penalties and candidateCount raise errors, and a trailing model turn is rejected; thinking_level on a pre-Gemini-3 model errors, and mixing thinking_level with thinking_budget on Gemini 3 errors. Claude on Vertex uses suffix-less ids (claude-fable-5-1, claude-opus-5, claude-sonnet-5, ...) through the Anthropic rawPredict Messages shape, which the Gemini-shaped Vertex adapter does not produce. Priority PayGo is available on the global and the us/eu multi-region endpoints since 2026-09-09 (X-Vertex-AI-LLM-Shared-Request-Type: priority, with X-Vertex-AI-LLM-Request-Type: shared to bypass Provisioned Throughput); a deferred service tier (preview) exists for the Interactions API only.

#### Current and preview

| Model | Aliases | Status | Dates | Input | Output | Ops | Tools | Reasoning | GAISe support / notes |
|---|---|---|---|---|---|---|---|---|---|
| `gemini-3.8-flash` | — | `short_term_active` | — | text, image, audio, video, file | text | IS | ✓ | ✓ (low, medium, high) | native — GA 2026-09-02; short-term availability table, no retirement date announced. thinking_level LOW, MEDI… |
| `gemini-3.7-flash` | — | `short_term_active` | — | text, image, audio, video, file | text | IS | ✓ | ✓ (low, medium, high) | native — GA 2026-08-13; short-term availability table, no retirement date announced and no replacement named… |
| `gemini-3.6-flash` | — | `short_term_active` | — | text, image, audio, video, file | text | IS | ✓ | ✓ (minimal, low, medium, high) | native — Released 2026-07-21; short-term availability (retires 45 days after a designated replacement). Neith… |
| `gemini-3.5-flash` | — | `active` | not before 2027-05-19 | text, image, audio, video, file | text | IS | ✓ | ✓ (minimal, low, medium, high) | native — thinking_level MINIMAL..HIGH (default MEDIUM); sampling parameters still accepted on this model. |
| `gemini-3.5-flash-lite` | — | `active` | not before 2027-07-21 | text, image, audio, video, file | text | IS | ✓ | ✓ (minimal, low, medium, high) | native — thinking_level MINIMAL (default)..HIGH; custom sampling values are ignored. Agentic video understand… |
| `gemini-3.1-flash-lite` | — | `active` | not before 2027-05-07 | text, image, audio, video, file | text | IS | ✓ | ✓ (minimal, low, medium, high) | native — GA 2026-05-07. thinking_level MINIMAL (default)..HIGH. |
| `gemini-3.1-pro-preview` | `gemini-3.1-pro-preview-customtools` | `preview` | — | text, image, audio, video, file | text | IS | ✓ | ✓ (low, medium, high) | native — Public preview since 2026-02-19 (customtools variant 2026-02-23); global endpoint only; not in the l… |
| `gemini-3-flash-preview` | — | `preview` | — | text, image, audio, video, file | text | IS | ✓ | ✓ (minimal, low, medium, high) | native — Public preview since 2025-12-17; not in the Vertex lifecycle table. The Vertex model page shows laun… |
| `gemini-3.1-flash-image` | — | `active` | not before 2027-05-28 | text, image, video, file | text, image | IS | ✗ | ✓ (minimal, high) | native image output through generateContent — Video input and 4K output are GA since 2026-08-31; us and eu mu… |
| `gemini-3.1-flash-lite-image` | — | `active` | — | text, image, video, file | text, image | IS | ✗ | ✓ (minimal, high) | native image output through generateContent — GA 2026-06-23 on Vertex (the Gemini API date is 2026-06-30). Li… |
| `gemini-3-pro-image` | — | `active` | not before 2027-05-28 | text, image, file | text, image | IS | ✗ | ✓ (high) | native image output through generateContent — Function calling is not supported; video input is not supported… |
| `gemini-embedding-2` | `gemini-embedding-2-preview` | `active` | — | text, image, audio, video | embedding | — | ✗ | ✗ | not yet: Vertex serves it via :embedContent on the aiplatform.{location}.rep.googleapis.com host, which the a… |
| `gemini-embedding-001` | — | `active` | not before 2028-05-20 | text | embedding | E | ✗ | ✗ | native — Previous-generation text embedding model; gemini-embedding-2 is current. Accepts one input text per… |
| `text-embedding-005` | `text-embedding-004`, `text-multilingual-embedding-002` | `active` | not before 2027-04-01 | text | embedding | E | ✗ | ✗ | native — Legacy text embedding family; all retire 2027-04-01. 768 dimensions, 2,048 tokens, 250 texts per cal… |
| `multimodalembedding@001` | — | `active` | not before 2027-04-01 | text, image, audio, video | embedding | — | ✗ | ✗ | not yet: uses the image/video embedding request schema — 1408 dimensions (128/256/512 for text+image); 32 tex… |
| `gemini-live-2.5-flash-native-audio` | — | `active` | shutdown 2026-12-13 | text, image, audio, video | text, audio | — | ✓ | ✗ | not supported: the Vertex AI adapter has no Live transport — GA 2025-12-12. Documented as a 128K context wind… |
| `gemini-3.5-transcribe-preview` | `gemini-3.5-transcribe-live-preview` | `preview` | — | text, audio | text | — | ✗ | ✗ | not supported: speech-to-text has no GAISe surface — Preview since August 2026, global region only; ids diffe… |
| `gemini-omni-1.1-flash-preview` | — | `preview` | — | text, image, video | text | — | ✗ | ✗ | not supported: video generation has no GAISe surface — Preview since 2026-08-27 (the Gemini API serves gemini… |
| `gemini-omni-flash-preview` | — | `preview` | shutdown 2027-06-30 | text, image, video | text | — | ✗ | ✗ | not supported: video generation has no GAISe surface — Preview on Vertex since 2026-06-30 with a Vertex retir… |
| `gemini-3.5-live-translate-preview` | — | `preview` | — | audio | audio | — | ✗ | ✗ | not supported: speech-to-speech translation uses translationConfig on the Live API, and the Vertex AI adapter… |

#### Deprecated and legacy

| Model | Aliases | Status | Dates | Input | Output | Ops | Tools | Reasoning | GAISe support / notes |
|---|---|---|---|---|---|---|---|---|---|
| `gemini-2.5-pro` | — | `deprecated` | shutdown 2026-10-20 | text, image, audio, video, file | text | IS | ✓ | ✓ | native (thinkingBudget mapper path) |
| `gemini-2.5-flash` | — | `deprecated` | shutdown 2026-10-20 | text, image, audio, video, file | text | IS | ✓ | ✓ | native (thinkingBudget mapper path) |
| `gemini-2.5-flash-lite` | — | `deprecated` | shutdown 2026-10-20 | text, image, audio, video, file | text | IS | ✓ | ✓ | native (thinkingBudget mapper path) |
| `gemini-2.5-flash-image` | — | `deprecated` | shutdown 2026-10-02 | text, image | text, image | IS | ✗ | ✗ | Vertex documents a 32,768-token context window for this model; the Gemini API documents 65,536. |

#### Retired

| Model | Aliases | Status | Dates | Input | Output | Ops | Tools | Reasoning | GAISe support / notes |
|---|---|---|---|---|---|---|---|---|---|
| `gemini-2.0-flash` | `gemini-2.0-flash-lite` | `retired` | shutdown 2026-06-01 | — | — | — | ? | ? | — |

## bedrock

### Amazon Bedrock

Model IDs, inference profiles, and lifecycle are **region-specific**; entries are representative and `ListFoundationModels` / `ListInferenceProfiles` are authoritative ([`catalog.rs`](../gaise-provider-bedrock/src/catalog.rs)). The registry matcher strips `us.`/`eu.`/`apac.`/`ap.`/`jp.`/`au.`/`ca.`/`il.`/`in.`/`global.`/`us-gov.` profile prefixes and `-vN:M` suffixes, and `*` entries are family globs.

- Vendor page: [vendor-bedrock.md](vendor-bedrock.md) · GAISe surface: Converse, ConverseStream, and InvokeModel
- Discovery: ListFoundationModels, GetFoundationModel, and ListInferenceProfiles
- Official catalog: <https://docs.aws.amazon.com/bedrock/latest/userguide/model-cards.html> · lifecycle: <https://docs.aws.amazon.com/bedrock/latest/userguide/model-lifecycle.html>
- Model IDs, inference profiles, lifecycle, and availability vary by AWS region. AWS replaced the Converse feature matrix with per-model cards (model-card-*.html) and models-api-compatibility.html in mid-2026; models-supported.html and inference-profiles-support.html now redirect to them. Since 2026-08-15 AWS recommends the bedrock-runtime endpoint for new applications (bedrock-mantle keeps server-side tools, async inference, and some gated models). Cross-region profile prefixes seen on cards: us., eu., apac., jp., au., in. (India, 2026-08-18), global., us-gov.; the registry matcher strips all of them. Converse contract as of 2026-09-12: serviceTier priority/default/flex/reserved; outputConfig.textFormat for structured outputs; cachePoint.ttl 5m/1h; audio and searchResult content blocks; stopReason adds malformed_model_output, malformed_tool_use, model_context_window_exceeded. Since 2026-09-07 AWS splits the lifecycle policy: models launched on or after that date follow model-lifecycle.html (the card shows an EOL-no-sooner-than date plus a 6-month or 45-day Legacy period); earlier launches and every current Legacy/EOL table are on model-lifecycle-legacy.html.

#### Current and preview

| Model | Aliases | Status | Dates | Input | Output | Ops | Tools | Reasoning | GAISe support / notes |
|---|---|---|---|---|---|---|---|---|---|
| `anthropic.claude-fable-5-1` | — | `active` | — | text, image, file | text | IS | ✓ | ✓ adaptive (low, medium, high, xhigh, max) | native via Converse — Launched 2026-09-01. Profiles us. and global. only (no in-region id on bedrock-runtime)… |
| `anthropic.claude-mythos-5-1` | — | `limited_availability` | — | text, image, file | text | IS | ✓ | ✓ adaptive (low, medium, high, xhigh, max) | native via Converse when access exists — Launched 2026-09-01 as a gated preview (Anthropic trusted-access pro… |
| `anthropic.claude-opus-5` | — | `active` | — | text, image, file | text | IS | ✓ | ✓ adaptive (low, medium, high, xhigh, max) | native via Converse — Launched 2026-07-24. Profiles: us., eu., au., global. Access is gated ('See Access' on… |
| `anthropic.claude-fable-5` | — | `active` | — | text, image, file | text | IS | ✓ | ✓ adaptive (low, medium, high, xhigh, max) | native via Converse — Launched 2026-06-09. Profiles us. and global. on bedrock-runtime (the us-east-1 in-regi… |
| `anthropic.claude-opus-4-8` | — | `active` | — | text, image, file | text | IS | ✓ | ✓ adaptive (low, medium, high, xhigh, max) | native via Converse — Launched 2026-05-28. Profiles: us., eu., jp., au., global. Prompt-cache minimum 1,024 t… |
| `anthropic.claude-sonnet-5` | — | `active` | — | text, image, file | text | IS | ✓ | ✓ adaptive (low, medium, high, xhigh, max) | native via Converse — Launched 2026-06-30. Profiles: us., eu., au., global. Adaptive thinking is on by defaul… |
| `anthropic.claude-opus-4-7` | — | `active` | — | text, image, file | text | IS | ✓ | ✓ adaptive (low, medium, high, xhigh, max) | native via Converse — Launched 2026-04-16. Profiles: us., eu., jp., au., global. thinking.type adaptive only;… |
| `anthropic.claude-sonnet-4-6` | — | `active` | — | text, image, file | text | IS | ✓ | ✓ adaptive (low, medium, high, max) | native via Converse — Launched 2026-02-17. Profiles: us., eu., au., jp., global.; in-region eu-west-2 added.… |
| `anthropic.claude-opus-4-6-v1` | — | `active` | — | text, image, file | text | IS | ✓ | ✓ adaptive (low, medium, high, max) | native via Converse — Launched 2026-02-05. Profiles: us., eu., au., global.; in-region eu-west-2 added. Last… |
| `anthropic.claude-opus-4-5-20251101-v1:0` | — | `active` | — | text, image, file | text | IS | ✓ | ✓ manual (low, medium, high) | native via Converse — Profiles: us., eu., global. EOL floor (2026-03-25) has passed without a Legacy announce… |
| `anthropic.claude-sonnet-4-5-20250929-v1:0` | — | `active` | not before 2026-09-29 | text, image, file | text | IS | ✓ | ✓ manual | native via Converse — Profiles: us., eu., au., jp., global. |
| `anthropic.claude-haiku-4-5-20251001-v1:0` | `anthropic.claude-haiku-4-5` | `active` | not before 2026-10-01 | text, image, file | text | IS | ✓ | ✓ manual | native via Converse — The suffix-less anthropic.claude-haiku-4-5 id serves the Messages API on bedrock-runtim… |
| `anthropic.claude-mythos-5` | — | `limited_availability` | — | text, image, file | text | — | ✓ | ✓ adaptive | not reachable: Messages API on bedrock-mantle only; Converse and InvokeModel are not supported — us-east-1 pr… |
| `anthropic.claude-mythos-preview` | — | `limited_availability` | — | text, image, file | text | — | ✓ | ✓ adaptive (low, medium, high, max) | not reachable: Messages API on bedrock-mantle only — Invitation only; AWS lifecycle 'Preview' (us-east-1, ap-… |
| `amazon.nova-2-lite-v1:0` | — | `active` | — | text, image, video, file | text | IS | ✓ | ✓ (low, medium, high) | native via Converse — The only Converse-capable Nova 2 model. Profiles: us., eu., jp., global. Extended think… |
| `amazon.nova-2-sonic-v1:0` | — | `active` | — | text, audio | text, audio | — | ✓ | ✗ | not supported: InvokeModelWithBidirectionalStream only |
| `amazon.nova-2-multimodal-embeddings-v1:0` | — | `active` | — | text, image, audio, video | embedding | E | ✗ | ✗ | text embeddings via InvokeModel (SINGLE_EMBEDDING schema); image/audio/video input not mapped — us-east-1 and… |
| `amazon.nova-pro-v1:0` | — | `active` | — | text, image, video, file | text | IS | ✓ | ✗ | native via Converse — Bedrock model card: 300K context, 5K max output (the Nova user guide's spec table says… |
| `amazon.nova-lite-v1:0` | — | `active` | — | text, image, video, file | text | IS | ✓ | ✗ | native via Converse — Bedrock model card: 300K context, 5K max output (the Nova user guide's spec table says… |
| `amazon.nova-micro-v1:0` | — | `active` | — | text | text | IS | ✓ | ✗ | native via Converse — Text only. Bedrock model card: 128K context, 5K max output (the Nova user guide's spec… |
| `amazon.nova-*` | — | `dynamic_active_family` | — | text, image, video, file | text | IS | ✓ | ✗ | Converse where the regional catalog lists the model — Nova Pro/Lite/Micro (v1) remain Active (EOL floor 2025-… |
| `amazon.titan-embed-text-v2:0` | — | `active` | — | text | embedding | E | ✗ | ✗ | native via InvokeModel — In-region only. 8,192 tokens / 50,000 characters per text; one text per call; `norma… |
| `amazon.titan-embed-text-v1` | — | `active` | — | text | embedding | E | ✗ | ✗ | native via InvokeModel — First-generation Titan text embeddings; fixed 1536 dimensions, one text per call. |
| `amazon.titan-embed-image-v1` | — | `active` | — | text, image, audio, video | embedding | E | ✗ | ✗ | text input via InvokeModel; no image path — Text + image embeddings in one space; 256 text tokens, 25 MB imag… |
| `amazon.titan-embed-*` | — | `dynamic_active_family` | — | text | embedding | E | ✗ | ✗ | native via InvokeModel — Catch-all for Titan embedding ids not listed above; verify with ListFoundationModels. |
| `cohere.embed-v4:0` | — | `active` | — | text, image, audio, video | embedding | E | ✗ | ✗ | native via InvokeModel — Launched 2025-04-15; text and image input; profiles us., eu., global. `input_type` i… |
| `cohere.embed-english-v3` | `cohere.embed-multilingual-v3` | `active` | — | text | embedding | E | ✗ | ✗ | native via InvokeModel — Fixed 1024 dimensions, 512 tokens per text, 96 texts per call; `input_type` is requi… |
| `cohere.embed-*` | — | `dynamic_active_family` | — | text | embedding | E | ✗ | ✗ | native via InvokeModel — Catch-all for other Cohere embed ids; `input_type` is always required. |
| `openai.gpt-5.6-sol` | — | `active` | — | text, image | text | IS | ✓ | ✓ | Converse text, images, and tools; reasoning effort is not mapped on Converse — Model launch 2026-07-13; Conve… |
| `openai.gpt-5.6-terra` | — | `active` | — | text, image | text | IS | ✓ | ✓ | Converse text, images, and tools; reasoning effort is not mapped on Converse — Converse on bedrock-runtime si… |
| `openai.gpt-5.6-luna` | — | `active` | — | text, image | text | IS | ✓ | ✓ | Converse text, images, and tools; reasoning effort is not mapped on Converse — Converse on bedrock-runtime si… |
| `openai.gpt-5.6-cyber` | — | `limited_availability` | — | text, image | text | — | ✓ | ✓ | not reachable: Responses API on bedrock-mantle only (us-east-2), Trusted Access for Cyber — OpenAI Daybreak R… |
| `openai.gpt-6-astra` | — | `active` | — | text, image | text | IS | ✓ | ✓ | Converse text, images, and tools; reasoning effort is not mapped on Converse — Launched on Bedrock 2026-09-08… |
| `openai.gpt-daybreak-blue-5.6-sol` | — | `limited_availability` | — | text, image | text | — | ✓ | ✓ | not reachable: Responses and Chat Completions on bedrock-mantle only (us-east-2), Trusted Access for Cyber re… |
| `openai.gpt-oss-120b-1:0` | — | `active` | — | text | text | IS | ✓ | ✓ | native via Converse; reasoning effort is not mapped on Converse — Open-weight; Converse, InvokeModel, Chat Co… |
| `openai.gpt-oss-20b-1:0` | — | `active` | — | text | text | IS | ✓ | ✓ | native via Converse; reasoning effort is not mapped on Converse |
| `xai.grok-4.6` | — | `active` | — | text, image | text | IS | ✓ | ✓ (low, medium, high, xhigh) | Converse text, images, and tools; reasoning content and effort are Responses-API only — Launched 2026-08-18 (… |
| `xai.grok-4.3` | — | `active` | — | text | text | — | ✓ | ✓ (none, low, medium, high) | not reachable: Responses and Chat Completions on bedrock-mantle only (Converse and InvokeModel are not suppor… |
| `deepseek.v3.2` | — | `active` | — | text | text | IS | ✓ | ✓ | native via Converse — Launched 2025-12-01; Converse, InvokeModel, Chat Completions; structured outputs suppor… |
| `mistral.mistral-large-3-675b-instruct` | — | `active` | — | text, image | text | IS | ✓ | ✗ | native via Converse — Converse, InvokeModel, Chat Completions; structured outputs supported. |
| `meta.llama4-maverick-17b-instruct-v1:0` | — | `active` | — | text, image | text | IS | ✓ | ✗ | native via Converse — us. profile; structured outputs not supported. |
| `qwen.qwen3-coder-next` | — | `active` | — | text | text | IS | ✓ | ✓ | native via Converse — Launched 2026-02-04. |
| `zai.glm-5` | — | `active` | — | text | text | IS | ✓ | ✓ | native via Converse — Launched 2026-02-11. |
| `moonshotai.kimi-k2.5` | — | `active` | — | text, image | text | IS | ✓ | ✓ | native via Converse — Launched 2026-01-27; images up to 3 MB. |
| `nvidia.nemotron-super-3-120b` | — | `active` | — | text | text | IS | ✓ | ✓ | native via Converse — Launched 2026-03-11; us-gov. profile in GovCloud. |
| `minimax.minimax-m2.5` | — | `active` | — | text | text | IS | ✓ | ✓ | native via Converse — Launched 2026-02-12. |
| `google.gemma-4-*` | — | `active` | — | text, image, video | text | — | ✓ | ✗ | not reachable: Gemma 4 is served on bedrock-mantle only (no Converse) — google.gemma-4-31b, google.gemma-4-26… |
| `twelvelabs.marengo-embed-3-0-v1:0` | — | `active` | — | text, image, audio, video | embedding | — | ✗ | ✗ | not supported: the InvokeModel embedding builder covers Titan, Cohere, and Nova schemas only — Launched 2025-… |

#### Deprecated and legacy

| Model | Aliases | Status | Dates | Input | Output | Ops | Tools | Reasoning | GAISe support / notes |
|---|---|---|---|---|---|---|---|---|---|
| `anthropic.claude-opus-4-1-20250805-v1:0` | — | `legacy` | shutdown 2027-01-08 | — | — | — | ? | ? | Legacy since 2026-07-08; public extended access (higher pricing) from 2026-10-08. us. profile only. |
| `anthropic.claude-sonnet-4-20250514-v1:0` | — | `legacy` | shutdown 2026-10-14 | — | — | — | ? | ? | Legacy since 2026-04-14; extended-access pricing applies since 2026-07-14. |
| `ai21.jamba-1-5-*` | — | `legacy` | shutdown 2026-11-26 | — | — | — | ? | ? | ai21.jamba-1-5-large-v1:0 and ai21.jamba-1-5-mini-v1:0. Legacy since 2026-05-26; extended access from 2026-08… |
| `twelvelabs.marengo-embed-2-7-v1:0` | — | `legacy` | shutdown 2026-11-30 | text, image, audio, video | embedding | — | ✗ | ✗ | not supported: no InvokeModel embedding builder for this schema — Legacy since 2026-05-29; extended access fr… |
| `amazon.nova-premier-v1:0` | — | `legacy` | shutdown 2026-09-14 | — | — | — | ? | ? | — |
| `amazon.nova-sonic-v1:0` | — | `legacy` | shutdown 2026-09-14 | — | — | — | ? | ? | — |
| `amazon.nova-reel-v1:*` | — | `legacy` | shutdown 2026-09-30 | — | — | — | ? | ? | — |
| `amazon.nova-canvas-v1:0` | — | `legacy` | shutdown 2026-09-30 | — | — | — | ? | ? | — |

#### Retired

| Model | Aliases | Status | Dates | Input | Output | Ops | Tools | Reasoning | GAISe support / notes |
|---|---|---|---|---|---|---|---|---|---|
| `anthropic.claude-3-haiku-20240307-v1:0` | — | `retired` | shutdown 2026-09-10 | — | — | — | ? | ? | Legacy since 2026-03-10; extended access from 2026-06-10; the AWS EOL date 2026-09-10 (commercial and GovClou… |
| `anthropic.claude-3-5-haiku-20241022-v1:0` | — | `retired` | shutdown 2026-06-19 | — | — | — | ? | ? | AWS EOL date 2026-06-19 has passed; the card still reads Legacy and the id still appears in the API-compatibi… |
| `cohere.command-r-*` | — | `retired` | shutdown 2026-08-19 | — | — | — | ? | ? | cohere.command-r-v1:0 and cohere.command-r-plus-v1:0. |

## ollama

### Ollama

The installed catalog is dynamic (`GET /api/tags`); entries are family globs describing typical capabilities, and `-cloud` tags match the same globs. `POST /api/show` (opt-in `include_details`) reports the real capabilities of each installed tag ([`catalog.rs`](../gaise-provider-ollama/src/contracts/catalog.rs)).

- Vendor page: [vendor-ollama.md](vendor-ollama.md) · GAISe surface: Chat, streaming chat, and Embeddings
- Discovery: GET /api/tags
- Official catalog: <https://ollama.com/search> · lifecycle: <dynamic local catalog>
- Tags are installed locally and can move; Ollama has no centralized retirement calendar. Cloud-hosted tags (`family:size-cloud`, run through ollama.com with an API key) match the same family globs. API as of Ollama v0.34.0 (2026-09-05; the release adds OpenAI-compatible tool search and response compaction on /v1 only): `think` accepts true/false or the strings low, medium, high, max (any other string is rejected; GPT-OSS takes levels only); chat and generate responses report prompt_eval_cached_count; requests accept logprobs/top_logprobs; /api/show capabilities may include image and audio; the default repeat_penalty became 1.0 in v0.32.10.

#### Current and preview

| Model | Aliases | Status | Dates | Input | Output | Ops | Tools | Reasoning | GAISe support / notes |
|---|---|---|---|---|---|---|---|---|---|
| `qwen3:*` | — | `dynamic_local` | — | text | text | IS | ✓ | ✓ (true, false) | native — Context 40,960 on the original dense tags (0.6b-32b); the 2507 builds, 4b, 30b, and 235b are 262,144… |
| `gpt-oss:*` | — | `dynamic_local` | — | text | text | IS | ✓ | ✓ (low, medium, high) | native — Context 131,072 on 20b and 120b. |
| `deepseek-r1:*` | `deepseek-v3.1:*` | `dynamic_local` | — | text | text | IS | ✓ | ✓ (true, false) | native — Context 131,072 on the distilled tags; deepseek-r1:671b and deepseek-v3.1 are 163,840. The library l… |
| `gemma4:*` | — | `dynamic_local` | — | text, image, audio | text | IS | ✓ | ✓ (true, false) | native when the installed tag advertises the capability (/api/show) — Context 131,072 on e2b/e4b; 12b, 26b, a… |
| `qwen3.8:*` | `qwen3.8-flash-next:*` | `dynamic_local` | — | text, image | text | IS | ✓ | ✓ (true, false) | native — Qwen 3.8 27B (Ollama v0.32.12, 2026-08-14) and the MLX-only Qwen 3.8 Flash Next 125B-A6B (v0.33.1, 2… |
| `qwen3.6:*` | — | `dynamic_local` | — | text, image | text | IS | ✓ | ✓ (true, false) | native — 27b and 35b tags; 256K context; retains reasoning context across turns. |
| `qwen3.5:*` | — | `dynamic_local` | — | text, image | text | IS | ✓ | ✓ (true, false) | native — 0.8b-122b tags, all 256K context; multimodal; cloud tags available. |
| `muse-glimmer:*` | — | `dynamic_local` | — | text, image | text | IS | ✓ | ✓ (true, false) | native (boolean thinking toggle) — Meta's 30B open model for local agents (2026-08-10, Apache 2.0); documents… |
| `nemotron-3.5-lightning:*` | — | `dynamic_local` | — | text | text | IS | ✓ | ✓ (true, false) | native — NVIDIA 30B-A3B MoE for always-on agents (2026-08-11); 1M context on the GGUF tag, 256K on MLX. |
| `laguna-s-2.1:*` | — | `dynamic_local` | — | text | text | IS | ✓ | ✓ (true, false) | native — Ollama's own 118B-A8B model for long-horizon work (OpenMDW-1.1 licence); tool calling and interleave… |
| `laguna-xs-2.1:*` | — | `dynamic_local` | — | text | text | IS | ✓ | ✓ (true, false) | native — Ollama's 33B-A3B MoE for local agentic coding (OpenMDW-1.1 licence, 2026-09); 256K on every tag (q4_… |
| `glm-5.3:*` | — | `dynamic_local` | — | text | text | IS | ✓ | ✓ (low, high, max) | native (cloud tag only; served through ollama.com with an API key) — Z.ai GLM-5.3, cloud-only glm-5.3:cloud (… |
| `glm-5.3-flash:*` | — | `dynamic_local` | — | text, image | text | IS | ✓ | ✓ (low, high, max) | native (cloud tag only; served through ollama.com with an API key) — GLM-5.3 Flash, 320B-A18B natively multim… |
| `deepseek-v4.1-flash:*` | — | `dynamic_local` | — | text, image | text | IS | ✓ | ✓ (true, false) | native (cloud tag only; served through ollama.com with an API key) — DeepSeek V4.1 Flash (2026-09-10), 763B M… |
| `kimi-k3:*` | — | `dynamic_local` | — | text, image | text | IS | ✓ | ✓ (true, false) | native (cloud tag only; served through ollama.com with an API key) — Moonshot Kimi K3, 2.81T-parameter open-w… |
| `granite4.2:*` | — | `dynamic_local` | — | text | text | IS | ✓ | ✓ (low, high) | native — IBM Granite 4.2 3b/8b/30b (Apache 2.0, 2026-08); 128K context; tool use, structured JSON output, and… |
| `ornith-1.5:*` | — | `dynamic_local` | — | text, image | text | IS | ✗ | ✗ | native — Ornith 1.5 9b/35b/397b (2026-08); 256K context; vision badge only (no tools or thinking badge, unlik… |
| `mistral-medium-3.5:*` | — | `dynamic_local` | — | text, image | text | IS | ✓ | ✓ (true, false) | native — 128B single-weight model with a configurable reasoning mode; 256K context. |
| `llama4:*` | — | `dynamic_local` | — | text, image | text | IS | ✓ | ✗ | native — Scout 16x17b (10M context) and Maverick 128x17b (1M); vision and tools, no thinking. Context recorde… |
| `embeddinggemma:*` | — | `dynamic_local` | — | text | embedding | E | ✗ | ✗ | native — EmbeddingGemma 300m; Matryoshka 768/512/256/128; 2,048-token context; Google prompt-instruction conv… |
| `nomic-embed-text:*` | — | `dynamic_local` | — | text | embedding | E | ✗ | ✗ | native — nomic-embed-text v1.5; Matryoshka 64-768; 8,192-token context (raise num_ctx); prefixes are required… |
| `nomic-embed-text-v2-moe:*` | — | `dynamic_local` | — | text | embedding | E | ✗ | ✗ | native — Multilingual MoE; Matryoshka 256-768; 512-token context. |
| `qwen3-embedding:*` | — | `dynamic_local` | — | text | embedding | E | ✗ | ✗ | native — 0.6b/4b/8b = 1024/2560/4096 dimensions (Matryoshka 32-4096); 32k context per the model card (the Oll… |
| `mxbai-embed-large:*` | — | `dynamic_local` | — | text | embedding | E | ✗ | ✗ | native — mixedbread mxbai-embed-large-v1; fixed 1024; 512-token context; queries take an instruction. |
| `bge-m3:*` | — | `dynamic_local` | — | text | embedding | E | ✗ | ✗ | native — BAAI bge-m3; fixed 1024; 8,192-token context; multilingual; no prefix. |
| `bge-large:*` | — | `dynamic_local` | — | text | embedding | E | ✗ | ✗ | native — BAAI bge-large-en-v1.5; fixed 1024; 512-token context; optional query instruction. |
| `all-minilm:*` | — | `dynamic_local` | — | text | embedding | E | ✗ | ✗ | native — all-MiniLM-L6/L12; fixed 384; 256-token context; English. |
| `snowflake-arctic-embed:*` | — | `dynamic_local` | — | text | embedding | E | ✗ | ✗ | native — Arctic-embed v1 22m-335m; 384-1024 dimensions by tag; 512-token context except the 137m (m-long) tag… |
| `snowflake-arctic-embed2:*` | — | `dynamic_local` | — | text | embedding | E | ✗ | ✗ | native — Arctic-embed 2.0; 1024 dimensions (Matryoshka to 256); 8,192-token context; multilingual; queries ta… |
| `granite-embedding:*` | — | `dynamic_local` | — | text | embedding | E | ✗ | ✗ | native — IBM Granite 30m (384, English) / 278m (768, 12 languages); 512-token context; no prefix. |
| `paraphrase-multilingual:*` | — | `dynamic_local` | — | text | embedding | E | ✗ | ✗ | native — paraphrase-multilingual-MiniLM-L12-v2; fixed 768; 128-token context; 50+ languages. |

## elevenlabs

### ElevenLabs

Text-to-speech and realtime voice. `GET /v1/models` reports model ids, languages, `can_do_text_to_speech`, style/speaker-boost support, and per-request character limits ([`models.rs`](../gaise-provider-elevenlabs/src/contracts/models.rs)). Voices are account-specific (`GET /v2/voices`) and ElevenLabs default voices expire 2026-12-31, so no voice is hard-coded. `eleven_v3*` realtime goes through the text-to-dialogue WebSocket; other models use `stream-input`.

- Vendor page: [vendor-elevenlabs.md](vendor-elevenlabs.md) · GAISe surface: Text-to-speech, streaming speech, realtime voice WebSocket, and model listing
- Discovery: GET /v1/models (model_id, languages, can_do_text_to_speech, can_use_style, character limits); GET /v2/voices for voices
- Official catalog: <https://elevenlabs.io/docs/overview/models> · lifecycle: <https://elevenlabs.io/docs/overview/models>
- Voices are account-specific and default voices expire 2026-12-31; never hard-code a voice id. Character limits per request are model-specific; the character-cost header reports billing.

#### Current and preview

| Model | Aliases | Status | Dates | Input | Output | Ops | Tools | Reasoning | GAISe support / notes |
|---|---|---|---|---|---|---|---|---|---|
| `eleven_v3` | — | `active` | — | text | audio | VL | ✗ | ✗ | speech via /v1/text-to-speech; realtime via the text-to-dialogue WebSocket — Flagship, 70+ languages, 5,000 c… |
| `eleven_v3_conversational` | — | `active` | — | text | text, audio | L | ✗ | ✗ | realtime only via the text-to-dialogue WebSocket (one voice) — ~280 ms latency variant of v3 for realtime use. |
| `eleven_multilingual_v2` | — | `active` | — | text | audio | VL | ✗ | ✗ | native — Default model; 29 languages; 10,000 characters per request. language_code is documented as not suppo… |
| `eleven_flash_v2_5` | — | `active` | — | text | audio | VL | ✗ | ✗ | native — ~75 ms latency, 32 languages, 40,000 characters per request, accepts language_code. Numbers are not… |
| `eleven_flash_v2` | — | `active` | — | text | audio | VL | ✗ | ✗ | native — English only; 30,000 characters per request. |
| `eleven_multilingual_sts_v2` | `eleven_english_sts_v2` | `active` | — | audio | audio | — | ✗ | ✗ | not supported: speech-to-speech has no GAISe surface |
| `scribe_v2` | `scribe_v2_realtime` | `active` | — | text, audio | text | — | ✗ | ✗ | not supported: speech-to-text has no GAISe surface yet — scribe_v2_realtime is listed as its own model (~150… |
| `eleven_ttv_v3` | `eleven_multilingual_ttv_v2` | `active` | — | text | text, audio | — | ✗ | ✗ | not supported: voice design (/v1/text-to-voice/design) has no GAISe surface — Text-to-voice design models; th… |
| `music_v2` | — | `active` | — | text | text, audio | — | ✗ | ✗ | not supported: music generation (/v1/music) has no GAISe surface — Studio-grade music with composition plans,… |
| `music_v2_5` | — | `active` | — | text | text, audio | — | ✗ | ✗ | not supported: music generation (/v1/music) has no GAISe surface — Most advanced music model (listed by 2026-… |
| `eleven_text_to_sound_v2` | — | `active` | — | text | text, audio | — | ✗ | ✗ | not supported: sound effects (/v1/sound-generation) have no GAISe surface — 0.5-30 s effects with optional lo… |

#### Deprecated and legacy

| Model | Aliases | Status | Dates | Input | Output | Ops | Tools | Reasoning | GAISe support / notes |
|---|---|---|---|---|---|---|---|---|---|
| `eleven_turbo_v2_5` | — | `deprecated` | — | text | audio | VL | ✗ | ✗ | native while available — Functionally equivalent to eleven_flash_v2_5; no shutdown date published. Character… |
| `eleven_turbo_v2` | — | `deprecated` | — | text | audio | VL | ✗ | ✗ | native while available — Character limit as for the functionally equivalent eleven_flash_v2. |
| `music_v1` | — | `deprecated` | — | text | text, audio | — | ✗ | ✗ | not supported: music generation (/v1/music) has no GAISe surface — Listed under Deprecated models by 2026-09-… |

## Lifecycle calendar

Published shutdown dates for entries that are not yet retired, soonest first. Treat them as the **earliest** possible date; providers may extend but not advance them.

| Date | Provider | Model | Replacement |
|---|---|---|---|
| 2026-09-14 | [bedrock](#bedrock) | `amazon.nova-premier-v1:0` | amazon.nova-2-lite-v1:0 |
| 2026-09-14 | [bedrock](#bedrock) | `amazon.nova-sonic-v1:0` | amazon.nova-2-sonic-v1:0 |
| 2026-09-30 | [gemini](#gemini) | `gemini-omni-flash-preview` | gemini-omni-1.1-flash |
| 2026-09-30 | [bedrock](#bedrock) | `amazon.nova-canvas-v1:0` | — |
| 2026-09-30 | [bedrock](#bedrock) | `amazon.nova-reel-v1:*` | — |
| 2026-10-02 | [gemini](#gemini) | `gemini-2.5-flash-image` | gemini-3.1-flash-image |
| 2026-10-02 | [vertexai](#vertexai) | `gemini-2.5-flash-image` | gemini-3.1-flash-lite-image |
| 2026-10-14 | [bedrock](#bedrock) | `anthropic.claude-sonnet-4-20250514-v1:0` | anthropic.claude-sonnet-5 |
| 2026-10-20 | [vertexai](#vertexai) | `gemini-2.5-flash` | gemini-3.5-flash-lite or gemini-3.1-flash-lite |
| 2026-10-20 | [vertexai](#vertexai) | `gemini-2.5-flash-lite` | gemini-3.1-flash-lite or Gemma 4 |
| 2026-10-20 | [vertexai](#vertexai) | `gemini-2.5-pro` | gemini-3.5-flash |
| 2026-10-23 | [openai](#openai) | `gpt-3.5-turbo` | gpt-5.6-terra |
| 2026-10-23 | [openai](#openai) | `gpt-4` | gpt-5.6-sol |
| 2026-10-23 | [openai](#openai) | `gpt-4-turbo` | gpt-5.6-sol |
| 2026-10-23 | [openai](#openai) | `gpt-4.1-nano` | gpt-5.6-luna |
| 2026-10-23 | [openai](#openai) | `gpt-4o-2024-05-13` | gpt-5.6-sol |
| 2026-10-23 | [openai](#openai) | `gpt-image-1` | gpt-image-2 |
| 2026-10-23 | [openai](#openai) | `o1` | gpt-5.6-sol |
| 2026-10-23 | [openai](#openai) | `o3-mini` | gpt-5.6-sol |
| 2026-10-23 | [openai](#openai) | `o4-mini` | gpt-5.6-terra |
| 2026-11-26 | [bedrock](#bedrock) | `ai21.jamba-1-5-*` | — |
| 2026-11-30 | [bedrock](#bedrock) | `twelvelabs.marengo-embed-2-7-v1:0` | twelvelabs.marengo-embed-3-0-v1:0 |
| 2026-12-01 | [openai](#openai) | `gpt-image-1.5` | gpt-image-2 |
| 2026-12-11 | [openai](#openai) | `gpt-5-2025-08-07` | gpt-5.6-sol |
| 2026-12-11 | [openai](#openai) | `gpt-5-mini-2025-08-07` | gpt-5.6-terra |
| 2026-12-11 | [openai](#openai) | `gpt-5-nano-2025-08-07` | gpt-5.6-luna |
| 2026-12-11 | [openai](#openai) | `gpt-5-pro-2025-10-06` | gpt-5.6-sol (reasoning.mode: pro) |
| 2026-12-11 | [openai](#openai) | `o3-2025-04-16` | gpt-5.6-sol |
| 2026-12-11 | [openai](#openai) | `o3-pro-2025-06-10` | gpt-5.6-sol (reasoning.mode: pro) |
| 2026-12-13 | [vertexai](#vertexai) | `gemini-live-2.5-flash-native-audio` | — |
| 2027-01-08 | [bedrock](#bedrock) | `anthropic.claude-opus-4-1-20250805-v1:0` | anthropic.claude-opus-4-8 |
| 2027-01-20 | [openai](#openai) | `gpt-audio` | gpt-audio-1.5 |
| 2027-01-20 | [openai](#openai) | `gpt-realtime` | gpt-realtime-2.1 or gpt-realtime-2.1-mini |
| 2027-02-26 | [openai](#openai) | `whisper-1` | gpt-transcribe or gpt-live-transcribe |
| 2027-05-07 | [gemini](#gemini) | `gemini-3.1-flash-lite` | gemini-3.5-flash-lite |
| 2027-06-30 | [vertexai](#vertexai) | `gemini-omni-flash-preview` | gemini-omni-1.1-flash-preview |
| 2028-05-14 | [gemini](#gemini) | `gemini-embedding-001` | gemini-embedding-2 |

## Maintaining the registry

1. Verify against the official catalog and lifecycle pages linked above — never aggregators, and never the *other* Google surface.
2. Edit [`gaise-core/model-registry.toml`](../gaise-core/model-registry.toml) using only the closed vocabulary in its header. `cargo test -p gaise --lib registry` fails on unknown terms, unmapped statuses, or broken lookups ([`registry.rs` tests](../gaise-core/src/registry.rs)).
3. Separate lifecycle (`status`, `shutdown_date`, `retirement_not_before`, `replacement`) from adapter support (`gaise_support`).
4. When a family needs a new control mapping (thinking type, fixed sampling, tools rule), add a hermetic request-shape test in the provider crate and update [capabilities.md](capabilities.md).
5. Bump `audited_on`, regenerate this page (`cargo run -p gaise --example models_page > wiki/models.md`), the [limits](limits.md), [reasoning](reasoning.md), and [embeddings](embeddings.md) matrices, refresh the [vendor pages](README.md#vendors), and append to the audit report.
6. Never validate lifecycle by making a billable inference request.

The `.claude/agents/model-audit.md` agent definition automates steps 1–2 for coding agents.
