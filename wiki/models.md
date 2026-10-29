# Models

> Part of the [GAISe wiki](README.md) · [Capabilities](capabilities.md) · [HTTP API](api.md#get-v1models) · [Rust SDK](sdk.md#model-discovery) · [Flows](flows.md#model-discovery) · Vendors: [OpenAI](vendor-openai.md) · [Anthropic](vendor-anthropic.md) · [Google Gemini API](vendor-gemini.md) · [Google Vertex AI](vendor-vertexai.md) · [Amazon Bedrock](vendor-bedrock.md) · [Ollama](vendor-ollama.md) · [ElevenLabs](vendor-elevenlabs.md) · [xAI](vendor-xai.md)

This page is the human-readable view of [`gaise-core/model-registry.toml`](../gaise-core/model-registry.toml) (schema 2, audited **2026-10-29**), which is compiled into the `gaise` crate and applied as an overlay by [`list_models`](api.md#get-v1models). It is advisory: GAISe accepts arbitrary model IDs so new releases work before this file is updated, and the provider's own model API (see [Model discovery](capabilities.md#model-discovery)) is always the first source of truth.

This page is generated from the registry by [`cargo run -p gaise --example models_page`](../gaise-core/examples/models_page.rs); edit the registry (or that example's introductions), not this file. Columns:

- **Input / Output** — modalities classified from the entry's `capabilities` list by [`classify_capabilities`](../gaise-core/src/registry.rs).
- **Ops** — GAISe operations the entry maps to: `I` instruct, `S` instruct_stream, `E` embeddings, `V` speech (voice), `L` live, `D` decision, `T` transcription, `W` streaming transcription, `P` image generation (picture), `M` video generation (movie). Empty means no GAISe surface drives the model.
- **Tools / Reasoning** — ✓ supported, ✗ not listed, and the `reasoning_values` the provider documents.
- **Dates** — `shutdown` is a published retirement date; `not before` is an availability guarantee. Gemini API and Vertex AI dates are **never** interchangeable.
- **Limits** — context windows, output ceilings, per-input token limits, and character budgets are not repeated here; see [limits.md](limits.md) for the generated model × limits matrix and `GET /v1/models/limits`.

## Contents


- [OpenAI](#openai) — 62 entries
- [Anthropic](#anthropic) — 20 entries
- [Google Gemini API](#gemini) — 38 entries
- [Google Vertex AI](#vertexai) — 38 entries
- [Amazon Bedrock](#bedrock) — 75 entries
- [Ollama](#ollama) — 54 entries
- [ElevenLabs](#elevenlabs) — 18 entries
- [xAI](#xai) — 25 entries
- [TypeSafe AI](#typesafe) — 2 entries
- [Maintaining the registry](#maintaining-the-registry)
- [Lifecycle calendar](#lifecycle-calendar)

## openai

### OpenAI

Instruct uses **Chat Completions**; Responses-only models (GPT-5.5 Pro, gpt-5.6-cyber, the Daybreak models, image generation) are listed but cannot be driven, and GPT-6 Astra / GPT-6.1 Sol tool conversations are sent to the Responses API because Chat Completions cannot call functions for them. `GET /v1/models` reports identity only, so everything in the Input/Output/Ops columns is registry- or heuristic-sourced at runtime ([`catalog.rs`](../gaise-provider-openai/src/contracts/catalog.rs)).

- Vendor page: [vendor-openai.md](vendor-openai.md) · GAISe surface: Chat Completions, Responses (GPT-6 Astra / GPT-6.1 Sol / GPT-6.1 Terra tool calls), Decisions, Embeddings, and Realtime
- Discovery: GET /v1/models (id, created, owned_by, shutdown_date only)
- Official catalog: <https://developers.openai.com/api/docs/models> · lifecycle: <https://developers.openai.com/api/docs/deprecations>
- The current instruct client uses Chat Completions, and the Responses API (stateless, store false) only for GPT-6 Astra / GPT-6.1 Sol / GPT-6.1 Terra tool conversations. Other Responses-only features, the Images API, and the Live API (/v1/live/sessions, GPT-Live 1, 2026-09-10) are outside that client. Chat Completions contract as of 2026-09-12: reasoning_effort accepts none, minimal, low, medium, high, xhigh, max (per-model subsets); service_tier accepts auto, default, flex, scale, priority, and fast (Priority processing was renamed Fast mode on 2026-07-30; the response reports priority for fast requests); prompt_cache_options is {mode: implicit|explicit, ttl: 30m} on gpt-5.6 and later and prompt_cache_retention is deprecated; usage.prompt_tokens_details reports image_tokens and text_tokens; the Assistants API shut down 2026-08-26. Audit 2026-09-30: GPT-6 Sol and GPT-6 Luna (2026-09-22) and GPT-6.1 Sol (2026-09-29) added; service_tier ultrafast exists for GPT-6 Astra on the Responses API only; the Decisions API (limited preview announced 2026-09-29) has no published reference, SDK, or model id and is not mapped. Audit 2026-10-09: the Decisions API is in public beta since 2026-10-06 (POST /v1/decisions, gpt-6-luna only, predicate / choice / score questions, text and up to 128 images, input tokens only at $0.10 per 1M); GAISe maps it to decision. gpt-5.1, gpt-5.4-nano, and gpt-5.3-codex were deprecated on 2026-10-01 (removal 2027-04-01), as were tts-1, tts-1-hd, and the dated gpt-4o-mini-tts snapshots (removal 2027-01-06, replacement gpt-realtime-2.1-mini). GPT-6.1 Sol gained service_tier ultrafast on the Responses API (2026-10-08); chat-latest was re-pointed (2026-10-07). No new chat, embedding, or realtime model. Audit 2026-10-14: no new model and no contract change; on 2026-10-14 OpenAI deprecated gpt-4o (2024-08-06 and 2024-11-20 snapshots), gpt-4o-mini, gpt-4.1, and gpt-4.1-mini, removal 2027-01-14, replacement gpt-6-luna (the deprecations page and model pages; the changelog does not list it yet). The Decisions API is still in public beta (gpt-6-luna only). Audit 2026-10-16: no model, lifecycle, or contract change for the GAISe surfaces; on 2026-10-15 GPT-Live gained native Decisions delegation and transcript triggers (gpt-live-1 is still not driven by GAISe). Audit 2026-10-29: GPT-6.1 Terra (gpt-6.1-terra) released 2026-10-26 on Chat Completions, Responses, and Batch (tool calling requires Responses; effort none..max, default medium). The legacy snapshots announced 2026-04-22 (o1, o1-pro, o3-mini, o4-mini, gpt-4o-2024-05-13, gpt-4-turbo, gpt-4, gpt-3.5-turbo, gpt-4.1-nano, gpt-image-1) were shut down on 2026-10-23 and are now retired. The Decisions API is generally available since 2026-10-22 (up to 400 questions, hosted image URLs, image parts in question instructions, input_json, prompt caching); the changes are additive, so the mapping is unchanged. The Responses API accepts prompt_cache_options.ttl 12h (2026-10-20); the caching guide still documents prompt_cache_retention for models before GPT-5.6. Projects can restrict service tiers (2026-10-21).

#### Current and preview

| Model | Aliases | Status | Dates | Input | Output | Ops | Tools | Reasoning | GAISe support / notes |
|---|---|---|---|---|---|---|---|---|---|
| `gpt-6-astra` | — | `active` | — | text, image | text | IS | ✓ | ✓ (low, medium, high, xhigh, max) | chat-compatible features; function tools through the Responses API — Released 2026-09-03 as a limited preview… |
| `gpt-6.1-sol` | — | `active` | — | text, image | text | IS | ✓ | ✓ (low, medium, high, xhigh, max) | chat-compatible features; function tools through the Responses API — Released 2026-09-29 for complex coding a… |
| `gpt-6.1-terra` | — | `active` | — | text, image | text | IS | ✓ | ✓ (none, low, medium, high, xhigh, max) | chat-compatible features; function tools through the Responses API — Released 2026-10-26 (changelog) as a fas… |
| `gpt-6-sol` | — | `active` | — | text, image | text | IS | ✓ | ✓ (none, low, medium, high, xhigh, max) | chat-compatible features — Released 2026-09-22; superseded by gpt-6.1-sol on 2026-09-29 but not deprecated. F… |
| `gpt-6-luna` | — | `active` | — | text, image | text | DIS | ✓ | ✓ (none, low, medium, high, xhigh, max) | chat-compatible features; typed decisions through the Decisions API — Released 2026-09-22. Same Chat Completi… |
| `gpt-5.6` | `gpt-5.6-sol` | `active` | — | text, image | text | IS | ✓ | ✓ (none, low, medium, high, xhigh, max) | chat-compatible features — OpenAI documents gpt-5.6-sol as the snapshot ID and gpt-5.6 as the alias that rout… |
| `gpt-5.6-terra` | — | `active` | — | text, image | text | IS | ✓ | ✓ (none, low, medium, high, xhigh, max) | chat-compatible features — On Chat Completions, function tools require reasoning_effort='none'; the adapter a… |
| `gpt-5.6-luna` | — | `active` | — | text, image | text | IS | ✓ | ✓ (none, low, medium, high, xhigh, max) | chat-compatible features — On Chat Completions, function tools require reasoning_effort='none'; the adapter a… |
| `gpt-5.5` | — | `active` | — | text, image | text | IS | ✓ | ✓ (none, low, medium, high, xhigh) | chat-compatible features — Defaults to medium reasoning effort. |
| `gpt-5.5-pro` | — | `active` | — | text, image | text | — | ✓ | ✓ (medium, high, xhigh) | not reachable through the Chat Completions instruct client — OpenAI lists Chat Completions as not supported;… |
| `gpt-5.4` | — | `active` | — | text, image | text | IS | ✓ | ✓ (none, low, medium, high, xhigh) | chat-compatible features |
| `gpt-5.4-mini` | — | `active` | — | text, image | text | IS | ✓ | ✓ (none, low, medium, high, xhigh) | chat-compatible features |
| `gpt-5.2` | `gpt-5.2-2025-12-11` | `active` | — | text, image | text | IS | ✓ | ✓ (none, low, medium, high, xhigh) | chat-compatible features — Previous flagship; OpenAI recommends GPT-6 Astra or GPT-5.6. Not on the deprecatio… |
| `text-embedding-3-large` | — | `active` | — | text | embedding | E | ✗ | ✗ | native |
| `text-embedding-3-small` | — | `active` | — | text | embedding | E | ✗ | ✗ | native |
| `text-embedding-ada-002` | — | `active` | — | text | embedding | E | ✗ | ✗ | native — Previous generation; fixed 1536 dimensions, no retirement date published. |
| `gpt-realtime-2.1` | — | `active` | — | text, image, audio | text, audio | L | ✓ | ✓ (minimal, low, medium, high, xhigh) | realtime transport — The Realtime session reference enumerates reasoning.effort as minimal, low, medium, high… |
| `gpt-realtime-2.1-mini` | — | `active` | — | text, image, audio | text, audio | L | ✓ | ✓ (minimal, low, medium, high, xhigh) | realtime transport |
| `gpt-realtime-2` | — | `active` | — | text, image, audio | text, audio | L | ✓ | ✓ (minimal, low, medium, high, xhigh) | realtime transport |
| `gpt-realtime-1.5` | — | `active` | — | text, image, audio | text, audio | L | ✓ | ✗ | realtime transport — No reasoning controls. |
| `gpt-realtime-translate` | — | `active` | — | audio | audio | — | ✗ | ✗ | not supported: the /v1/realtime/translations endpoint uses its own session and event vocabulary — Streaming s… |
| `gpt-audio-1.5` | — | `active` | — | text, audio | text, audio | IS | ✓ | ✗ | Chat audio input is native; audio output is not mapped by the current instruct client — Chat Completions supp… |
| `gpt-image-2` | — | `active` | — | image | image | P | ✗ | ✗ | generate_images: POST /v1/images/generations, or the JSON /v1/images/edits body when input images are given (… |
| `gpt-image-2.5-sunburst` | — | `active` | — | image | image | P | ✗ | ✗ | generate_images: POST /v1/images/generations, or the JSON /v1/images/edits body when input images are given (… |
| `gpt-image-2.5-flare` | — | `active` | — | image | image | P | ✗ | ✗ | generate_images: POST /v1/images/generations, or the JSON /v1/images/edits body when input images are given (… |
| `gpt-live-1` | — | `active` | — | text, audio | text, audio | — | ✓ | ✗ | not supported: the Live API (wss://api.openai.com/v1/live/sessions) has its own session.start / session.input… |
| `gpt-5.6-cyber` | — | `limited_availability` | — | text, image | text | — | ✓ | ✓ | not reachable: Responses API only, Daybreak program approval required — Daybreak Red model (2026-08-12). 400K… |
| `gpt-daybreak-*` | — | `limited_availability` | — | text | text | — | ✓ | ✓ | not reachable: Responses API only, Daybreak program approval required — gpt-daybreak-red-latest and gpt-daybr… |
| `gpt-rosalind-research` | — | `limited_availability` | — | text | text | — | ✗ | ✓ | not verified: no model page is published and trusted-access approval is required; the instruct client forward… |
| `gpt-transcribe` | — | `active` | — | text, audio | text | T | ✗ | ✗ | transcribe: POST /v1/audio/transcriptions (languages[], keywords[], prompt; json); realtime transcription ses… |
| `gpt-live-transcribe` | — | `active` | — | text, audio | text | — | ✗ | ✗ | not supported: realtime transcription sessions have no GAISe surface — Released 2026-07-28; /v1/realtime/tran… |
| `gpt-realtime-whisper` | — | `active` | — | text, audio | text | — | ✗ | ✗ | not supported: realtime transcription sessions have no GAISe surface — turn_detection must be null for this m… |
| `gpt-4o-mini-tts` | `gpt-4o-mini-tts-2025-12-15`, `gpt-4o-mini-tts-2025-03-20` | `active` | — | text | text, audio | — | ✗ | ✗ | not supported: OpenAI text-to-speech has no GAISe surface (use elevenlabs::) — /v1/audio/speech only; 2,000 i… |

#### Deprecated and legacy

| Model | Aliases | Status | Dates | Input | Output | Ops | Tools | Reasoning | GAISe support / notes |
|---|---|---|---|---|---|---|---|---|---|
| `gpt-5.4-nano` | `gpt-5.4-nano-2026-03-17` | `deprecated` | shutdown 2027-04-01 | text, image | text | IS | ✓ | ✓ (none, low, medium, high, xhigh) | chat-compatible features — Deprecated 2026-10-01; removal 2027-04-01. |
| `gpt-5.1` | `gpt-5.1-2025-11-13` | `deprecated` | shutdown 2027-04-01 | text, image | text | IS | ✓ | ✓ (none, low, medium, high) | chat-compatible features — Deprecated 2026-10-01 together with gpt-5.3-codex and gpt-5.4-nano; removal 2027-0… |
| `gpt-4.1` | `gpt-4.1-2025-04-14` | `deprecated` | shutdown 2027-01-14 | text, image | text | IS | ✓ | ✗ | chat-compatible features — Non-reasoning; sampling accepted; image detail limited to low/high/auto. Deprecate… |
| `gpt-4.1-mini` | `gpt-4.1-mini-2025-04-14` | `deprecated` | shutdown 2027-01-14 | text, image | text | IS | ✓ | ✗ | chat-compatible features |
| `gpt-4o` | `gpt-4o-2024-11-20`, `gpt-4o-2024-08-06` | `deprecated` | shutdown 2027-01-14 | text, image | text | IS | ✓ | ✗ | chat-compatible features — gpt-4o resolves to gpt-4o-2024-08-06. Deprecated 2026-10-14 (both listed snapshots… |
| `gpt-4o-mini` | `gpt-4o-mini-2024-07-18` | `deprecated` | shutdown 2027-01-14 | text, image | text | IS | ✓ | ✗ | chat-compatible features |
| `gpt-5-2025-08-07` | `gpt-5` | `deprecated` | shutdown 2026-12-11 | — | — | — | ? | ? | The deprecations page lists the dated snapshot; the gpt-5 alias resolves to it and has no other snapshot. |
| `gpt-5-mini-2025-08-07` | `gpt-5-mini` | `deprecated` | shutdown 2026-12-11 | — | — | — | ? | ? | — |
| `gpt-5-nano-2025-08-07` | `gpt-5-nano` | `deprecated` | shutdown 2026-12-11 | — | — | — | ? | ? | — |
| `gpt-5-pro-2025-10-06` | `gpt-5-pro` | `deprecated` | shutdown 2026-12-11 | — | — | — | ? | ? | Responses and Batch only; reasoning.effort high only. |
| `o3-2025-04-16` | `o3` | `deprecated` | shutdown 2026-12-11 | — | — | — | ? | ? | — |
| `o3-pro-2025-06-10` | `o3-pro` | `deprecated` | shutdown 2026-12-11 | — | — | — | ? | ? | Responses and Batch only. |
| `gpt-image-1.5` | `gpt-image-1-mini`, `chatgpt-image-latest` | `deprecated` | shutdown 2026-12-01 | image | image | P | ✗ | ✗ | generate_images: POST /v1/images/generations, or the JSON /v1/images/edits body when input images are given (… |
| `gpt-realtime` | `gpt-4o-realtime`, `gpt-realtime-mini`, `gpt-4o-mini-realtime` | `deprecated` | shutdown 2027-01-20 | — | — | — | ? | ? | Announced 2026-07-20. Context 32K for gpt-realtime, gpt-realtime-mini, and gpt-4o-realtime. The gpt-4o-*-real… |
| `gpt-audio` | `gpt-4o-audio`, `gpt-audio-mini`, `gpt-4o-mini-audio` | `deprecated` | shutdown 2027-01-20 | — | — | — | ? | ? | — |
| `whisper-1` | `gpt-4o-transcribe`, `gpt-4o-mini-transcribe`, `gpt-4o-transcribe-diarize` | `deprecated` | shutdown 2027-02-26 | text, audio | text | T | ✗ | ✗ | transcribe: POST /v1/audio/transcriptions (whisper-1 verbose_json with word/segment timestamps; gpt-4o-transc… |

#### Retired

| Model | Aliases | Status | Dates | Input | Output | Ops | Tools | Reasoning | GAISe support / notes |
|---|---|---|---|---|---|---|---|---|---|
| `gpt-5-chat-latest` | — | `retired` | shutdown 2026-07-23 | — | — | — | ? | ? | — |
| `gpt-5.1-chat-latest` | — | `retired` | shutdown 2026-07-23 | — | — | — | ? | ? | — |
| `gpt-5.2-chat-latest` | — | `retired` | shutdown 2026-08-10 | — | — | — | ? | ? | — |
| `gpt-5.3-chat-latest` | — | `retired` | shutdown 2026-08-10 | — | — | — | ? | ? | — |
| `o4-mini` | `o4-mini-2025-04-16` | `retired` | shutdown 2026-10-23 | — | — | — | ? | ? | — |
| `o1` | `o1-2024-12-17`, `o1-pro`, `o1-pro-2025-03-19` | `retired` | shutdown 2026-10-23 | — | — | — | ? | ? | Announced 2026-04-22. o1-pro is Responses-only and replaced by gpt-5.6-sol with reasoning.mode pro. |
| `o3-mini` | `o3-mini-2025-01-31` | `retired` | shutdown 2026-10-23 | — | — | — | ? | ? | — |
| `gpt-4o-2024-05-13` | — | `retired` | shutdown 2026-10-23 | — | — | — | ? | ? | Shut down 2026-10-23. The other gpt-4o snapshots (2024-08-06, 2024-11-20) were deprecated on 2026-10-14 with… |
| `gpt-4-turbo` | `gpt-4-turbo-2024-04-09` | `retired` | shutdown 2026-10-23 | — | — | — | ? | ? | — |
| `gpt-4` | `gpt-4-0613`, `gpt-4-1106-preview` | `retired` | shutdown 2026-10-23 | — | — | — | ? | ? | gpt-4-1106-preview is 128K context. |
| `gpt-3.5-turbo` | `gpt-3.5-turbo-0125` | `retired` | shutdown 2026-10-23 | — | — | — | ? | ? | gpt-3.5-turbo-1106, gpt-3.5-turbo-instruct, babbage-002, and davinci-002 shut down earlier, on 2026-09-28. |
| `gpt-4.1-nano` | `gpt-4.1-nano-2025-04-14` | `retired` | shutdown 2026-10-23 | — | — | — | ? | ? | — |
| `gpt-image-1` | — | `retired` | shutdown 2026-10-23 | image | image | P | ✗ | ✗ | generate_images: POST /v1/images/generations, or the JSON /v1/images/edits body when input images are given (… |

## anthropic

### Anthropic

The Messages API. Anthropic's `GET /v1/models` reports image/PDF input, thinking types, effort levels, structured outputs, and token limits, so at runtime the registry contributes only lifecycle dates and notes ([`catalog.rs`](../gaise-provider-anthropic/src/contracts/catalog.rs)). Thinking and sampling rules by family are enforced in [`anthropic_client.rs`](../gaise-provider-anthropic/src/anthropic_client.rs). `legacy` mirrors Anthropic's own "Legacy" label (still served, no retirement date announced); the Models API keeps reporting those ids as active.

- Vendor page: [vendor-anthropic.md](vendor-anthropic.md) · GAISe surface: Messages
- Discovery: GET /v1/models (capabilities: image_input, pdf_input, thinking types, effort levels, structured_outputs, token limits)
- Official catalog: <https://platform.claude.com/docs/en/about-claude/models/overview> · lifecycle: <https://platform.claude.com/docs/en/about-claude/model-deprecations>
- Direct Claude API lifecycle. Bedrock-hosted Claude has a separate AWS lifecycle. Anthropic labels every 4.x model except Haiku 4.5, plus Fable 5, as Legacy (still served, no retirement date) since the 2026-09-01 Fable 5.1 release; the registry mirrors that as status legacy. Messages contract as of 2026-09-12: temperature/top_p/top_k are deprecated and rejected at non-default values on Opus 4.7 and later; output_format is deprecated in favour of output_config.format; stop_reason may be refusal (with stop_details) on Fable/Opus 5 models and model_context_window_exceeded; the Files API and Skills API are out of beta (no header); thinking.display accepts updates behind the thinking-display-updates-2026-08-18 beta; stop_details.category is one of cyber, bio, frontier_llm, reasoning_extraction, general_harms, or null; server-side fallback (fallbacks: default or a list of up to three models, beta server-side-fallback-2026-07-01) and per-message effort (beta mid-conversation-output-config-2026-07-01, also on Google Cloud since 2026-09-03) are documented but not sent. No model, limit, or lifecycle change between 2026-09-04 and 2026-09-12. Audit 2026-10-01: claude-sonnet-4-5-20250929 deprecated on 2026-09-30 (retires 2026-11-30); no new models or Messages API changes; the deprecations table still lists the overview's Legacy models as Active. Audit 2026-09-30: Opus 5.5 (2026-09-22) and Sonnet 5.5 (2026-09-28) added, which moves Opus 5 and Sonnet 5 to Legacy on the models overview; both 5.5 models reject thinking.type disabled and forced tool_choice; thinking.type between_tools (Sonnet 5.5), on-demand compaction (beta compact-2026-09-04), and the GA diagnostics object are documented but not sent. Audit 2026-10-09: Claude Haiku 5.5 released 2026-10-07 (adaptive thinking only, five effort levels, fixed sampling, forced tool_choice allowed, no prefill), which moves Haiku 4.5 to Legacy on its model page; the Models API gained line (2026-10-01), capabilities.thinking.types.disabled (2026-10-05), and capabilities.server_tools (2026-10-06); no Messages contract change. Audit 2026-10-14: no new model, lifecycle change, or Messages contract change (OpenTelemetry export beta, 2026-10-14, is configured in the Console). The Mythos 5 and 5.1 pages now read Verification required; Haiku 4.5 is still Active (legacy) with retirement not sooner than 2026-10-15 and no deprecation notice. Audit 2026-10-16: no model, lifecycle, or Messages contract change; fast mode for Opus 5.5 left research preview on 2026-10-15 (GAISe never sends speed). Haiku 4.5's not-sooner-than date (2026-10-15) passed with no deprecation notice. Audit 2026-10-29: no new model and no lifecycle change. API version 2026-10-21 (released 2026-10-21; the new SDKs send it by default) returns 400 for temperature, top_p, or top_k even at default values, for a document block in an assistant message, and for citations on a user text block. GAISe still sends 2023-06-01 by default; when a caller opts in with with_version, the client omits sampling (strip_for_api_version). On Bedrock, 2026-10-21 is available only for Claude 4.7 and later. The Models API reports capabilities.thinking.types.between_tools (2026-10-15). Claude Models on Google Cloud (Anthropic managed) launched 2026-10-22 and shares the Claude API lifecycle; Opus 5.5 fast mode is available there. Opus 4.5 (not sooner than 2026-11-24) and Haiku 4.5 still have no deprecation notice.

#### Current and preview

| Model | Aliases | Status | Dates | Input | Output | Ops | Tools | Reasoning | GAISe support / notes |
|---|---|---|---|---|---|---|---|---|---|
| `claude-fable-5-1` | — | `active` | not before 2027-09-01 | text, image, file | text | IS | ✓ | ✓ adaptive (low, medium, high, xhigh, max) | native — Released 2026-09-01; Anthropic's most capable widely released model (same tier, limits, tokenizer, a… |
| `claude-mythos-5-1` | — | `limited_availability` | not before 2027-09-01 | text, image, file | text | IS | ✓ | ✓ adaptive (low, medium, high, xhigh, max) | native when account access exists — Released 2026-09-01; access requires verification (model page status: Act… |
| `claude-mythos-5` | — | `limited_availability` | not before 2027-06-09 | text, image, file | text | IS | ✓ | ✓ adaptive (low, medium, high, xhigh, max) | native when account access exists — Access requires verification (model page status: Active (verification req… |
| `claude-opus-5-5` | — | `active` | not before 2027-09-22 | text, image, file | text | IS | ✓ | ✓ adaptive (low, medium, high, xhigh, max) | native — Released 2026-09-22. Adaptive thinking is always on: thinking.type disabled and budget_tokens return… |
| `claude-sonnet-5-5` | — | `active` | not before 2027-09-28 | text, image, file | text | IS | ✓ | ✓ adaptive (low, medium, high, xhigh, max) | native — Released 2026-09-28. Adaptive thinking on by default, default effort high; thinking.type disabled an… |
| `claude-haiku-5-5` | — | `active` | not before 2027-10-07 | text, image | text | IS | ✓ | ✓ adaptive (low, medium, high, xhigh, max) | native — Released 2026-10-07; fixed id with no dated snapshot or alias. Adaptive thinking (default effort med… |

#### Deprecated and legacy

| Model | Aliases | Status | Dates | Input | Output | Ops | Tools | Reasoning | GAISe support / notes |
|---|---|---|---|---|---|---|---|---|---|
| `claude-fable-5` | — | `legacy` | not before 2027-06-09 | text, image, file | text | IS | ✓ | ✓ adaptive (low, medium, high, xhigh, max) | native — Released 2026-06-09; labelled Legacy since 2026-09-01 (still served; migrate to claude-fable-5-1, wh… |
| `claude-opus-5` | — | `legacy` | not before 2027-07-24 | text, image, file | text | IS | ✓ | ✓ adaptive (low, medium, high, xhigh, max) | native — Released 2026-07-24; listed under Legacy models since the 2026-09-22 Opus 5.5 release (still served)… |
| `claude-opus-4-8` | — | `legacy` | not before 2027-05-28 | text, image, file | text | IS | ✓ | ✓ adaptive (low, medium, high, xhigh, max) | native — Labelled Legacy since 2026-09-01 (migrate to claude-opus-5). Adaptive-only thinking, off unless requ… |
| `claude-opus-4-7` | — | `legacy` | not before 2027-04-16 | text, image, file | text | IS | ✓ | ✓ adaptive (low, medium, high, xhigh, max) | native — Labelled Legacy since 2026-09-01. Adaptive-only thinking; budget_tokens and non-default temperature/… |
| `claude-opus-4-6` | — | `legacy` | not before 2027-02-05 | text, image, file | text | IS | ✓ | ✓ adaptive (low, medium, high, max) | native — Labelled Legacy since 2026-09-01. Adaptive thinking recommended; thinking.type enabled with budget_t… |
| `claude-opus-4-5-20251101` | `claude-opus-4-5` | `legacy` | not before 2026-11-24 | text, image, file | text | IS | ✓ | ✓ manual (low, medium, high) | native — Labelled Legacy since 2026-09-01. Manual thinking budget plus output_config.effort (low/medium/high)… |
| `claude-sonnet-5` | — | `legacy` | not before 2027-06-30 | text, image, file | text | IS | ✓ | ✓ adaptive (low, medium, high, xhigh, max) | native — Listed under Legacy models since the 2026-09-28 Sonnet 5.5 release (still served). Adaptive thinking… |
| `claude-sonnet-4-6` | — | `legacy` | not before 2027-02-17 | text, image, file | text | IS | ✓ | ✓ adaptive (low, medium, high, max) | native — Labelled Legacy since 2026-09-01 (migrate to claude-sonnet-5). Adaptive thinking recommended; enable… |
| `claude-sonnet-4-5-20250929` | `claude-sonnet-4-5` | `deprecated` | shutdown 2026-11-30 · not before 2026-09-29 | text, image, file | text | IS | ✓ | ✓ manual | native — Labelled Legacy since 2026-09-01; deprecated 2026-09-30 with retirement on the Claude API scheduled… |
| `claude-haiku-4-5-20251001` | `claude-haiku-4-5` | `legacy` | not before 2026-10-15 | text, image, file | text | IS | ✓ | ✓ manual | native — Manual thinking budget; no adaptive thinking or effort parameter. Legacy since 2026-10-07 (model pag… |
| `claude-mythos-preview` | — | `deprecated` | — | text, image, file | text | IS | ✓ | ✓ adaptive (low, medium, high, max) | Deprecated 2026-06-09; migrate to claude-mythos-5-1 or claude-fable-5-1. No retirement date is published and… |

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
- Google AI Gemini API lifecycle; do not reuse these dates for Vertex AI. Shutdown dates on the deprecations page are the earliest possible retirement dates. generateContent contract as of 2026-09-12: temperature/top_p/top_k are deprecated on every Gemini 3.x (the adapter omits them) and candidateCount is unsupported there; generationConfig gained responseFormat, enableAffectiveDialog, translationConfig, and audioTranscriptionConfig while responseSchema is deprecated in favour of responseJsonSchema; Part.mediaProcessing (STATIC | AGENTIC) selects agentic video understanding on 3.5 Flash-Lite, 3.6, 3.7, and 3.8; embedContent's top-level taskType/title/outputDimensionality are deprecated in favour of embedContentConfig (the adapter still sends the accepted top-level form). The thinking guide is now written against the Interactions API and lists the 2.5 family under thinking_level, but the REST reference and Vertex state that thinkingLevel errors on pre-3 models, so the adapter keeps thinkingBudget for 2.5. Requests also accept a top-level serviceTier (standard, flex, priority) and store, which the adapter does not send; batchEmbedContents returns usageMetadata; the Live API accepts only HIGH or LOW for start and end sensitivity (default HIGH), so the adapter omits any other value. No model or lifecycle change between 2026-09-04 and 2026-09-12. Audit 2026-10-01: Gemini 4 Argon was announced 2026-09-30 (restricted early access through the Fairwind Program, 1M output tokens) with no API model id or model page; the adapters treat any gemini-<major> id with major >= 3 like Gemini 3 (thinkingLevel without MINIMAL, no sampling) as a forward guard. Added gemini-3.8-live, gemini-3.8-live-extended-thinking, and the 3.8 Flash / Flash-Lite TTS models; the Live transport omits thinking_config for gemini-3.8-live. Audit 2026-10-09: Nano Banana 2.1 (gemini-nano-banana-2.1) GA 2026-10-06, which deprecates gemini-3.1-flash-image (no shutdown date); the adapters give it thinkingLevel (minimal, medium, high) and no sampling. The deprecations table now shows 2026-11-17 shutdowns for gemini-3.1-flash-live-preview, gemini-2.5-flash-native-audio-preview-12-2025, gemini-3.1-flash-tts-preview, and the 2.5 TTS previews; gemini-2.5-flash-image moved to 2027-03-15 (replacement gemini-3.1-flash-lite-image); gemini-omni-flash-preview shows 2026-10-22. gemini-embedding-2 multimodal input (images, audio, video, PDF) is mapped through embed_contents. Gemini 4 Argon had no API model id at that audit. Audit 2026-10-14: Gemini 4 Argon (gemini-4-argon) is GA since 2026-10-13 (thinkingLevel low/medium/high, sampling ignored, 32K output cap on non-streaming REST without long decode, implicit caching only). gemini-3.7-flash and gemini-3.5-flash were deprecated on 2026-10-08 with no shutdown date; requests are routed to gemini-3.8-flash and gemini-3.6-flash. The deprecations page now names gemini-3.8-live as the replacement for the retired 2.0 and 2.5 Live previews. Audit 2026-10-16: no change (newest changelog entry 2026-10-13; deprecations page last updated 2026-10-12). Audit 2026-10-29: no new chat model. veo-3.1-generate-preview, veo-3.1-fast-generate-preview, veo-3.1-lite-generate-preview, and gemini-omni-flash-preview were shut down on 2026-10-22 (greyed on the deprecations page) and are now retired. Since 2026-10-26 new paid-tier projects store generateContent requests by default (store=true; existing projects keep their setting); the adapter does not send store. The deprecations page says access to the 2.5 models is limited to users who have used them before; they are not deprecated. gemini-robotics-er-2-preview is not registered.

#### Current and preview

| Model | Aliases | Status | Dates | Input | Output | Ops | Tools | Reasoning | GAISe support / notes |
|---|---|---|---|---|---|---|---|---|---|
| `gemini-4-argon` | — | `stable` | — | text, image, audio, video, file | text | IS | ✓ | ✓ (low, medium, high) | native (long decode not implemented) — GA 2026-10-13. Unified 1,048,576-token input and output window; 65,536… |
| `gemini-3.8-flash` | — | `stable` | — | text, image, audio, video, file | text | IS | ✓ | ✓ (low, medium, high) | native — GA 2026-09-02; Google's most capable Flash model, aimed at long-horizon software engineering and age… |
| `gemini-3.6-flash` | — | `stable` | — | text, image, audio, video, file | text | IS | ✓ | ✓ (minimal, low, medium, high) | native — Released 2026-07-21. Fixed sampling: temperature, top_p, and top_k are deprecated (changelog 2026-07… |
| `gemini-3.5-flash-lite` | — | `stable` | — | text, image, audio, video, file | text | IS | ✓ | ✓ (minimal, low, medium, high) | native — thinkingLevel minimal (default), low, medium, high. Agentic video understanding supported since 2026… |
| `gemini-3.1-flash-lite` | — | `stable` | shutdown 2027-05-07 | text, image, audio, video, file | text | IS | ✓ | ✓ (minimal, low, medium, high) | native — thinkingLevel minimal (default), low, medium, high per the Gemini 3.5 guide's comparison table; the… |
| `gemini-3.1-pro-preview` | — | `preview` | — | text, image, audio, video, file | text | IS | ✓ | ✓ (low, medium, high) | native — No shutdown date announced. A gemini-3.1-pro-preview-customtools variant endpoint exists. |
| `gemini-3-flash-preview` | — | `preview` | — | text, image, audio, video, file | text | IS | ✓ | ✓ (minimal, low, medium, high) | native — No shutdown date announced. |
| `gemini-3.8-live` | — | `stable` | — | text, image, audio, video | text, audio | L | ✓ | ✗ | native Live API transport; thinking configuration is never sent — GA 2026-09-15; replaces gemini-3.1-flash-li… |
| `gemini-3.8-live-extended-thinking` | — | `stable` | — | text, image, audio, video | text, audio | L | ✓ | ✓ (low, medium, high) | native Live API transport (thinking_level low, medium, or high) — GA 2026-09-15. Background reasoning during… |
| `gemini-nano-banana-2.1` | — | `stable` | — | text, image, video, file | text, image | ISP | ✗ | ✓ (minimal, medium, high) | generate_images: generateContent with responseModalities TEXT+IMAGE and imageConfig (aspectRatio, imageSize);… |
| `gemini-3.1-flash-lite-image` | — | `stable` | — | text, image | text, image | ISP | ✗ | ✓ (minimal, high) | generate_images: generateContent with responseModalities TEXT+IMAGE and imageConfig (aspectRatio, imageSize);… |
| `gemini-3-pro-image` | — | `stable` | — | text, image | text, image | ISP | ✗ | ✓ | generate_images: generateContent with responseModalities TEXT+IMAGE and imageConfig (aspectRatio, imageSize);… |
| `gemini-embedding-2` | `gemini-embedding-2-preview` | `stable` | — | text, image, audio, video | embedding | E | ✗ | ✗ | native; images, audio, video, and PDF through embed_contents (POST /v1/embeddings/contents) — Released 2026-0… |
| `gemini-3.8-flash-tts` | — | `stable` | — | text | text, audio | — | ✗ | ✗ | text-to-speech is outside the instruct surface — GA 2026-09-22; 130+ languages with automatic language detect… |
| `gemini-3.8-flash-lite-tts` | — | `stable` | — | text | text, audio | — | ✗ | ✗ | text-to-speech is outside the instruct surface — GA 2026-09-22; 100+ languages with automatic language detect… |
| `gemini-3.5-transcribe` | `gemini-3.5-transcribe-live` | `stable` | — | text, audio | text | T | ✗ | ✗ | transcribe: generateContent with audioTranscriptionConfig (languageCodes, customVocabulary, wordTimestamp, di… |
| `gemini-3.5-live-translate-preview` | — | `preview` | — | audio | audio | — | ✗ | ✗ | not supported: speech-to-speech translation uses translationConfig on the Live API, which the live transport… |
| `gemini-omni-1.1-flash` | — | `stable` | — | text, image, video | text | — | ✗ | ✗ | not supported: video generation is served by the Interactions API only — GA 2026-08-27; conversational video… |
| `gemini-2.5-pro` | — | `stable` | — | text, image, audio, video, file | text | IS | ✓ | ✓ | native (thinkingBudget mapper path) — No shutdown date announced on the Gemini API deprecations page; the 202… |
| `gemini-2.5-flash` | — | `stable` | — | text, image, audio, video, file | text | IS | ✓ | ✓ | native (thinkingBudget mapper path) — No shutdown date announced on the Gemini API deprecations page. Since 2… |
| `gemini-2.5-flash-lite` | — | `stable` | — | text, image, audio, video, file | text | IS | ✓ | ✓ | native (thinkingBudget mapper path) — Released 2025-07-22; thinking is off by default. No shutdown date annou… |

#### Deprecated and legacy

| Model | Aliases | Status | Dates | Input | Output | Ops | Tools | Reasoning | GAISe support / notes |
|---|---|---|---|---|---|---|---|---|---|
| `gemini-3.7-flash` | — | `deprecated` | — | text, image, audio, video, file | text | IS | ✓ | ✓ (low, medium, high) | native — GA 2026-08-13; deprecated 2026-10-08 with no shutdown date: requests to gemini-3.7-flash are routed… |
| `gemini-3.5-flash` | — | `deprecated` | — | text, image, audio, video, file | text | IS | ✓ | ✓ (minimal, low, medium, high) | native — Deprecated 2026-10-08 with no shutdown date: requests to gemini-3.5-flash are routed automatically t… |
| `gemini-3.1-flash-live-preview` | — | `deprecated` | shutdown 2026-11-17 | text, image, audio, video | text, audio | L | ✓ | ✓ (minimal, low, medium, high) | native Live API transport — Released 2026-03-11. The deprecations page names gemini-3.8-live as its replaceme… |
| `gemini-2.5-flash-native-audio-preview-12-2025` | — | `deprecated` | shutdown 2026-11-17 | text, audio, video | text, audio | L | ✓ | ✓ | Live transport; migration recommended — Live guide still documents thinkingBudget (0 disables) for this model… |
| `gemini-3.1-flash-image` | — | `deprecated` | — | text, image, video, file | text, image | ISP | ✗ | ✓ (minimal, high) | generate_images: generateContent with responseModalities TEXT+IMAGE and imageConfig (aspectRatio, imageSize);… |
| `gemini-3.1-flash-tts-preview` | — | `deprecated` | shutdown 2026-11-17 | text | text, audio | — | ✗ | ✗ | text-to-speech is outside the instruct surface — Released 2026-04-13; replacement for the 2.5 TTS previews (g… |
| `gemini-2.5-flash-image` | — | `deprecated` | shutdown 2027-03-15 | — | — | P | ✗ | ✗ | generate_images: generateContent with responseModalities TEXT+IMAGE and imageConfig (aspectRatio, imageSize);… |
| `gemini-embedding-001` | — | `deprecated` | shutdown 2028-05-14 | text | embedding | E | ✗ | ✗ | native — Still listed as callable; the earlier registry date (2026-07-14) was the release date. |

#### Retired

| Model | Aliases | Status | Dates | Input | Output | Ops | Tools | Reasoning | GAISe support / notes |
|---|---|---|---|---|---|---|---|---|---|
| `veo-3.1-generate-preview` | `veo-3.1-fast-generate-preview`, `veo-3.1-lite-generate-preview` | `retired` | shutdown 2026-10-22 | image | video | — | ✗ | ✗ | not supported: the Veo previews shut down on 2026-10-22 and the replacement, gemini-omni-1.1-flash, is served… |
| `veo-3.0-*` | `veo-2.0-generate-001` | `retired` | shutdown 2026-06-30 | — | video | — | ✗ | ✗ | Veo 3.0 and Veo 2 were shut down on the Gemini API on 2026-06-30. |
| `imagen-*` | — | `retired` | shutdown 2026-08-17 | — | image | — | ✗ | ✗ | imagen-4.0-generate-001, -ultra-generate-001, and -fast-generate-001 shut down on 2026-08-17; earlier Imagen… |
| `gemini-omni-flash-preview` | — | `retired` | shutdown 2026-10-22 | — | — | — | ? | ? | Released 2026-06-30; deprecated 2026-08-27; shut down 2026-10-22 (the deprecations page greys the row). |
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
- Model availability and retirement dates are specific to Google Cloud. Short-term availability models retire 45 days after a replacement is released. Release notes moved to https://docs.cloud.google.com/gemini-enterprise-agent-platform/release-notes (the Vertex AI release-notes page is frozen at 2026-05-26). On 3.6+ Flash, 3.5 Flash-Lite, 3.7, and 3.8, custom temperature/topP/topK are ignored, frequency/presence penalties and candidateCount raise errors, and a trailing model turn is rejected; thinking_level on a pre-Gemini-3 model errors, and mixing thinking_level with thinking_budget on Gemini 3 errors. Claude on Vertex uses suffix-less ids (claude-fable-5-1, claude-opus-5, claude-sonnet-5, ...) through the Anthropic rawPredict Messages shape, which the Gemini-shaped Vertex adapter does not produce. Priority PayGo is available on the global and the us/eu multi-region endpoints since 2026-09-09 (X-Vertex-AI-LLM-Shared-Request-Type: priority, with X-Vertex-AI-LLM-Request-Type: shared to bypass Provisioned Throughput); a deferred service tier (preview) exists for the Interactions API only. Audit 2026-10-01: added gemini-3.8-flash-cyber (allowlist) and gemini-3.8-live; gemini-2.5-flash-image retirement extended to 2027-03-15; Gemini 3 models are available on the Interactions API (preview, 2026-09-28); Gemini 4 Argon is not listed. Audit 2026-10-09: added gemini-nano-banana-2.1 (GA 2026-10-06, global only, short-term table with no retirement date; seed, temperature, topP, topK, and logprobs return errors). The model-versions table now retires gemini-3.6-flash on 2026-11-19 and gemini-3.7-flash on 2027-01-28 (replacement gemini-3.8-flash); gemini-3.1-flash-image is not deprecated on Vertex. Claude Haiku 5.5 is in Model Garden (2026-10-07). Gemini 4 Argon is not listed. Audit 2026-10-14: Gemini 4 Argon (gemini-4-argon, 12-month table) and gemini-4-argon-cyber-permissive (Fairwind Program allowlist, short-term table) are GA since 2026-10-13, both with no retirement date; Argon serves only the global, us, and eu endpoints, and long decode needs v1beta1. No other lifecycle change; the Gemini API deprecation of 3.5 and 3.7 Flash does not apply on Vertex. Audit 2026-10-16: the model-versions page (updated 2026-10-15) lists gemini-4-argon, gemini-3.8-live, and gemini-3.8-flash in the 12-month table with retirement not before 2027-10-13, 2027-09-25, and 2027-09-02; Priority PayGo is GA for gemini-nano-banana-2.1 on the global endpoint (2026-10-14). No new Gemini model and no contract change. Audit 2026-10-29: gemini-3.8-flash-tts and gemini-3.8-flash-lite-tts are GA on Vertex since 2026-10-26 (global, us, eu; no retirement date published), outside the adapter. gemini-robotics-er-2-preview (Preview, 2026-10-27) is not registered: its accepted thinking levels are not fully documented and the adapter has no rule for it. gemini-2.5-pro, 2.5-flash, and 2.5-flash-lite are past their 2026-10-20 retirement date but still listed in the 12-month table, so they stay deprecated here. Direct Cloud Storage output (outputConfig.outputUriPrefix, image delivery URI) is GA for the image models since 2026-10-26; the adapter does not request it. Supervised fine-tuning for Gemini 3 is GA (2026-10-19).

#### Current and preview

| Model | Aliases | Status | Dates | Input | Output | Ops | Tools | Reasoning | GAISe support / notes |
|---|---|---|---|---|---|---|---|---|---|
| `gemini-4-argon` | — | `active` | not before 2027-10-13 | text, image, audio, video, file | text | IS | ✓ | ✓ (low, medium, high) | native on the global, us, or eu endpoints (long decode not implemented) — GA 2026-10-13; 12-month availabilit… |
| `gemini-4-argon-cyber-permissive` | — | `limited_availability` | — | text, image, audio, video, file | text | IS | ✓ | ✓ (low, medium, high) | native for projects on the Fairwind Program allowlist — GA behind the Fairwind Program allowlist since 2026-1… |
| `gemini-3.8-flash-cyber` | — | `limited_availability` | — | text, image, file | text | IS | ✗ | ✓ | native generateContent for allowlisted projects; function calling not supported — GA behind an allowlist sinc… |
| `gemini-3.8-live` | — | `active` | not before 2027-09-25 | text, image, audio, video | text, audio | — | ✓ | ✗ | not reachable: the Vertex AI crate has no Live transport — GA on Vertex 2026-09-24 (the Gemini API date is 20… |
| `gemini-3.8-flash-tts` | — | `active` | — | text | text, audio | — | ✗ | ✗ | text-to-speech is outside the instruct surface — GA on Vertex 2026-10-26 (model page release date 2026-09-28;… |
| `gemini-3.8-flash-lite-tts` | — | `active` | — | text | text, audio | — | ✗ | ✗ | text-to-speech is outside the instruct surface — GA on Vertex 2026-10-26 (model page release date 2026-09-28;… |
| `gemini-3.8-flash` | — | `active` | not before 2027-09-02 | text, image, audio, video, file | text | IS | ✓ | ✓ (low, medium, high) | native — GA 2026-09-02; in the 12-month availability table (retirement September 2, 2027 or later) as of 2026… |
| `gemini-3.7-flash` | — | `short_term_active` | shutdown 2027-01-28 | text, image, audio, video, file | text | IS | ✓ | ✓ (low, medium, high) | native — GA 2026-08-13; short-term availability table, no retirement date announced and no replacement named… |
| `gemini-3.6-flash` | — | `short_term_active` | shutdown 2026-11-19 | text, image, audio, video, file | text | IS | ✓ | ✓ (minimal, low, medium, high) | native — Released 2026-07-21; short-term availability (retires 45 days after a designated replacement). Neith… |
| `gemini-3.5-flash` | — | `active` | not before 2027-05-19 | text, image, audio, video, file | text | IS | ✓ | ✓ (minimal, low, medium, high) | native — thinking_level MINIMAL..HIGH (default MEDIUM); sampling parameters still accepted on this model. |
| `gemini-3.5-flash-lite` | — | `active` | not before 2027-07-21 | text, image, audio, video, file | text | IS | ✓ | ✓ (minimal, low, medium, high) | native — thinking_level MINIMAL (default)..HIGH; custom sampling values are ignored. Agentic video understand… |
| `gemini-3.1-flash-lite` | — | `active` | not before 2027-05-07 | text, image, audio, video, file | text | IS | ✓ | ✓ (minimal, low, medium, high) | native — GA 2026-05-07. thinking_level MINIMAL (default)..HIGH. |
| `gemini-3.1-pro-preview` | `gemini-3.1-pro-preview-customtools` | `preview` | — | text, image, audio, video, file | text | IS | ✓ | ✓ (low, medium, high) | native — Public preview since 2026-02-19 (customtools variant 2026-02-23); global endpoint only; not in the l… |
| `gemini-3-flash-preview` | — | `preview` | — | text, image, audio, video, file | text | IS | ✓ | ✓ (minimal, low, medium, high) | native — Public preview since 2025-12-17; not in the Vertex lifecycle table. The Vertex model page shows laun… |
| `gemini-nano-banana-2.1` | — | `short_term_active` | — | text, image, video | text, image | ISP | ✗ | ✓ | generate_images: generateContent with responseModalities TEXT+IMAGE and imageConfig (aspectRatio, imageSize);… |
| `gemini-3.1-flash-image` | — | `active` | not before 2027-05-28 | text, image, video, file | text, image | ISP | ✗ | ✓ (minimal, high) | generate_images: generateContent with responseModalities TEXT+IMAGE and imageConfig (aspectRatio, imageSize);… |
| `gemini-3.1-flash-lite-image` | — | `active` | not before 2027-06-28 | text, image, video, file | text, image | ISP | ✗ | ✓ (minimal, high) | generate_images: generateContent with responseModalities TEXT+IMAGE and imageConfig (aspectRatio, imageSize);… |
| `gemini-3-pro-image` | — | `active` | not before 2027-05-28 | text, image, file | text, image | ISP | ✗ | ✓ (high) | generate_images: generateContent with responseModalities TEXT+IMAGE and imageConfig (aspectRatio, imageSize);… |
| `gemini-embedding-2` | `gemini-embedding-2-preview` | `active` | — | text, image, audio, video | embedding | — | ✗ | ✗ | not yet: Vertex serves it via :embedContent on the aiplatform.{location}.rep.googleapis.com host, which the a… |
| `gemini-embedding-001` | — | `active` | not before 2028-05-20 | text | embedding | E | ✗ | ✗ | native — Previous-generation text embedding model; gemini-embedding-2 is current. Accepts one input text per… |
| `text-embedding-005` | `text-embedding-004`, `text-multilingual-embedding-002` | `active` | not before 2027-04-01 | text | embedding | E | ✗ | ✗ | native — Legacy text embedding family; all retire 2027-04-01. 768 dimensions, 2,048 tokens, 250 texts per cal… |
| `multimodalembedding@001` | — | `active` | not before 2027-04-01 | text, image, audio, video | embedding | — | ✗ | ✗ | not yet: uses the image/video embedding request schema — 1408 dimensions (128/256/512 for text+image); 32 tex… |
| `gemini-live-2.5-flash-native-audio` | — | `active` | shutdown 2026-12-13 | text, image, audio, video | text, audio | — | ✓ | ✗ | not supported: the Vertex AI adapter has no Live transport — GA 2025-12-12. Documented as a 128K context wind… |
| `gemini-3.5-transcribe-preview` | `gemini-3.5-transcribe-live-preview` | `preview` | — | text, audio | text | T | ✗ | ✗ | transcribe: generateContent with audioTranscriptionConfig — Preview since August 2026, global region only; id… |
| `gemini-3.5-transcribe` | `gemini-3.5-transcribe-live` | `active` | — | text, audio | text | T | ✗ | ✗ | transcribe: generateContent with audioTranscriptionConfig (languageCodes, customVocabulary, wordTimestamp, di… |
| `veo-3.1-generate-001` | — | `active` | not before 2026-11-17 | image, video | video | M | ✗ | ✗ | start_video / get_video: predictLongRunning and fetchPredictOperation (first and last frame, up to 3 asset re… |
| `veo-3.1-fast-generate-001` | — | `active` | not before 2026-11-17 | image, video | video | M | ✗ | ✗ | start_video / get_video: predictLongRunning and fetchPredictOperation — Veo 3.1 Fast, GA 2025-11-17 in us-cen… |
| `veo-3.1-lite-generate-001` | — | `preview` | — | image | video | M | ✗ | ✗ | start_video / get_video: predictLongRunning and fetchPredictOperation — Veo 3.1 Lite, preview since 2026-04-0… |
| `gemini-omni-1.1-flash-preview` | — | `preview` | — | text, image, video | text | — | ✗ | ✗ | not supported: video generation has no GAISe surface — Preview since 2026-08-27 (the Gemini API serves gemini… |
| `gemini-omni-flash-preview` | — | `preview` | shutdown 2027-06-30 | text, image, video | text | — | ✗ | ✗ | not supported: video generation has no GAISe surface — Preview on Vertex since 2026-06-30 with a Vertex retir… |
| `gemini-3.5-live-translate-preview` | — | `preview` | — | audio | audio | — | ✗ | ✗ | not supported: speech-to-speech translation uses translationConfig on the Live API, and the Vertex AI adapter… |

#### Deprecated and legacy

| Model | Aliases | Status | Dates | Input | Output | Ops | Tools | Reasoning | GAISe support / notes |
|---|---|---|---|---|---|---|---|---|---|
| `gemini-2.5-pro` | — | `deprecated` | shutdown 2026-10-20 | text, image, audio, video, file | text | IS | ✓ | ✓ | native (thinkingBudget mapper path) |
| `gemini-2.5-flash` | — | `deprecated` | shutdown 2026-10-20 | text, image, audio, video, file | text | IS | ✓ | ✓ | native (thinkingBudget mapper path) |
| `gemini-2.5-flash-lite` | — | `deprecated` | shutdown 2026-10-20 | text, image, audio, video, file | text | IS | ✓ | ✓ | native (thinkingBudget mapper path) |
| `gemini-2.5-flash-image` | — | `deprecated` | shutdown 2027-03-15 | text, image | text, image | ISP | ✗ | ✗ | generate_images: generateContent with responseModalities TEXT+IMAGE and imageConfig (aspectRatio, imageSize);… |

#### Retired

| Model | Aliases | Status | Dates | Input | Output | Ops | Tools | Reasoning | GAISe support / notes |
|---|---|---|---|---|---|---|---|---|---|
| `veo-3.0-*` | — | `retired` | shutdown 2026-06-30 | — | video | — | ✗ | ✗ | Veo 3.0 endpoints were retired on 2026-06-30. |
| `imagen-*` | `imagegeneration@*` | `retired` | shutdown 2026-06-30 | — | image | — | ✗ | ✗ | Every imagegeneration@00x, imagen-3.0-*, and imagen-4.0-* endpoint is listed as discontinued (migrate before… |
| `gemini-2.0-flash` | `gemini-2.0-flash-lite` | `retired` | shutdown 2026-06-01 | — | — | — | ? | ? | — |

## bedrock

### Amazon Bedrock

Model IDs, inference profiles, and lifecycle are **region-specific**; entries are representative and `ListFoundationModels` / `ListInferenceProfiles` are authoritative ([`catalog.rs`](../gaise-provider-bedrock/src/catalog.rs)). The registry matcher strips `us.`/`eu.`/`apac.`/`ap.`/`jp.`/`au.`/`ca.`/`il.`/`in.`/`global.`/`us-gov.` profile prefixes and `-vN:M` suffixes, and `*` entries are family globs.

- Vendor page: [vendor-bedrock.md](vendor-bedrock.md) · GAISe surface: Converse, ConverseStream, and InvokeModel
- Discovery: ListFoundationModels, GetFoundationModel, and ListInferenceProfiles
- Official catalog: <https://docs.aws.amazon.com/bedrock/latest/userguide/model-cards.html> · lifecycle: <https://docs.aws.amazon.com/bedrock/latest/userguide/model-lifecycle.html>
- Model IDs, inference profiles, lifecycle, and availability vary by AWS region. AWS replaced the Converse feature matrix with per-model cards (model-card-*.html) and models-api-compatibility.html in mid-2026; models-supported.html and inference-profiles-support.html now redirect to them. Since 2026-08-15 AWS recommends the bedrock-runtime endpoint for new applications (bedrock-mantle keeps server-side tools, async inference, and some gated models). Cross-region profile prefixes seen on cards: us., eu., apac., jp., au., in. (India, 2026-08-18), global., us-gov.; the registry matcher strips all of them. Converse contract as of 2026-09-12: serviceTier priority/default/flex/reserved; outputConfig.textFormat for structured outputs; cachePoint.ttl 5m/1h; audio and searchResult content blocks; stopReason adds malformed_model_output, malformed_tool_use, model_context_window_exceeded. Since 2026-09-07 AWS splits the lifecycle policy: models launched on or after that date follow model-lifecycle.html (the card shows an EOL-no-sooner-than date plus a 6-month or 45-day Legacy period); earlier launches and every current Legacy/EOL table are on model-lifecycle-legacy.html. Audit 2026-10-01: added GPT-6.1 Sol, GPT-6 Sol, GPT-6 Luna, Kimi K3, and Grok 4.7; Nova Premier, Nova Sonic v1, Nova Reel v1, and Nova Canvas passed their EOL dates; bedrock-runtime is the recommended path for Responses and Chat Completions (2026-09-15); per-model daily token quotas became a cross-model per-account quota (2026-09-21). Gemini models are not on Bedrock. Audit 2026-10-09: added Claude Haiku 5.5 (2026-10-07; Geo and global profiles only, Messages/Converse/Invoke), Z.AI GLM 5.3 (2026-10-05; us. and global. profiles, gated), and TwelveLabs Pegasus 1.5 (2026-10-05; InvokeModel only); Claude Sonnet 4.5 (EOL 2027-04-08) and Llama 4 Maverick (EOL 2027-03-30) are Legacy on their cards; Claude Opus 4.1 public extended access began 2026-10-08; the Opus 5.5 and Sonnet 5.5 cards no longer carry an access gate; Kimi K3 gained an in. profile (2026-10-06). No Converse contract change. Audit 2026-10-14: no new model card (Amazon Nova 2.5 Sonic was announced GA on 2026-10-05 but has no card, model id, or API listing yet). Claude Sonnet 4 reached its AWS EOL date on 2026-10-14; the lifecycle table gives Claude Sonnet 4.5 a Legacy date of 2026-10-08 (extended access 2027-01-08, EOL 2027-04-08) and Llama 4 Maverick 2026-09-30; Pegasus 1.5 launched 2026-10-08. GPT-6 Sol and Luna now list InvokeModel; GPT-5.6 Sol, Terra, and Luna cards give 1,050,000 context and 128,000 output. Nova Premier and Claude 3.5 Haiku are no longer listed on the legacy page. No Converse contract change. Audit 2026-10-16: no new model card and no Converse change; Nova 2.5 Sonic still has no card or model id; Claude Sonnet 4 is past its 2026-10-14 EOL date but still labelled Legacy; the legacy table added Claude 3.x Asia Pacific and eu-west-2 rows (EOL 2026-10-17) and a 2026-09-30 batch of third-party Legacy rows that are not registered. Audit 2026-10-29: new cards for OpenAI GPT-6.1 Terra (launched 2026-10-26; us. and global. profiles; the card links the pre-2026-09-07 lifecycle policy although it gives an EOL-no-sooner-than date) and Amazon Nova 2.5 Sonic (launched 2026-10-05; InvokeModelWithBidirectionalStream only). Claude Sonnet 4 moved to the EOL table (EOL 2026-10-14) and is now retired. The Claude 3 Haiku, 3 Sonnet, 3.5 Sonnet, 3.5 Sonnet v2, and 3.7 Sonnet regional Legacy rows now end on 2026-11-13. The Grok 4.7 card gives a 524,288-token window shared by input and output and lists client-side tool calling. Structured outputs for Claude are documented on both endpoints (2026-10-18; InvokeModel takes output_config.format as an object); the Converse contract is unchanged.

#### Current and preview

| Model | Aliases | Status | Dates | Input | Output | Ops | Tools | Reasoning | GAISe support / notes |
|---|---|---|---|---|---|---|---|---|---|
| `anthropic.claude-fable-5-1` | — | `active` | — | text, image, file | text | IS | ✓ | ✓ adaptive (low, medium, high, xhigh, max) | native via Converse — Launched 2026-09-01. Profiles us. and global. only (no in-region id on bedrock-runtime)… |
| `anthropic.claude-mythos-5-1` | — | `limited_availability` | — | text, image, file | text | IS | ✓ | ✓ adaptive (low, medium, high, xhigh, max) | native via Converse when access exists — Launched 2026-09-01 as a gated preview (Anthropic trusted-access pro… |
| `anthropic.claude-opus-5-5` | — | `active` | — | text, image, file | text | IS | ✓ | ✓ adaptive (low, medium, high, xhigh, max) | native via Converse — Launched 2026-09-22 (EOL no sooner than 2027-09-22, Legacy period 6 months). Profiles:… |
| `anthropic.claude-opus-5` | — | `active` | — | text, image, file | text | IS | ✓ | ✓ adaptive (low, medium, high, xhigh, max) | native via Converse — Launched 2026-07-24. Profiles: us., eu., au., global., and in. (India). The card no lon… |
| `anthropic.claude-fable-5` | — | `active` | — | text, image, file | text | IS | ✓ | ✓ adaptive (low, medium, high, xhigh, max) | native via Converse — Launched 2026-06-09. Profiles us. and global. on bedrock-runtime (the us-east-1 in-regi… |
| `anthropic.claude-opus-4-8` | — | `active` | — | text, image, file | text | IS | ✓ | ✓ adaptive (low, medium, high, xhigh, max) | native via Converse — Launched 2026-05-28. Profiles: us., eu., jp., au., global. Prompt-cache minimum 1,024 t… |
| `anthropic.claude-sonnet-5-5` | — | `active` | — | text, image, file | text | IS | ✓ | ✓ adaptive (low, medium, high, xhigh, max) | native via Converse — Launched 2026-09-28 (EOL no sooner than 2027-09-28, Legacy period 6 months). Profiles u… |
| `anthropic.claude-sonnet-5` | — | `active` | — | text, image, file | text | IS | ✓ | ✓ adaptive (low, medium, high, xhigh, max) | native via Converse — Launched 2026-06-30. Profiles: us., eu., au., global. Adaptive thinking is on by defaul… |
| `anthropic.claude-opus-4-7` | — | `active` | — | text, image, file | text | IS | ✓ | ✓ adaptive (low, medium, high, xhigh, max) | native via Converse — Launched 2026-04-16. Profiles: us., eu., jp., au., global. thinking.type adaptive only;… |
| `anthropic.claude-sonnet-4-6` | — | `active` | — | text, image, file | text | IS | ✓ | ✓ adaptive (low, medium, high, max) | native via Converse — Launched 2026-02-17. Profiles: us., eu., au., jp., global.; in-region eu-west-2 added.… |
| `anthropic.claude-opus-4-6-v1` | — | `active` | — | text, image, file | text | IS | ✓ | ✓ adaptive (low, medium, high, max) | native via Converse — Launched 2026-02-05. Profiles: us., eu., au., global.; in-region eu-west-2 added. Last… |
| `anthropic.claude-haiku-5-5` | — | `active` | not before 2027-10-07 | text, image | text | IS | ✓ | ✓ adaptive (low, medium, high, xhigh, max) | native via Converse — Launched 2026-10-07 (EOL no sooner than 2027-10-07, Legacy period 6 months). No in-Regi… |
| `anthropic.claude-opus-4-5-20251101-v1:0` | — | `active` | — | text, image, file | text | IS | ✓ | ✓ manual (low, medium, high) | native via Converse — Profiles: us., eu., global. The card shows Active with an EOL floor of 2026-11-24 (2026… |
| `anthropic.claude-haiku-4-5-20251001-v1:0` | `anthropic.claude-haiku-4-5` | `active` | not before 2026-10-16 | text, image, file | text | IS | ✓ | ✓ manual | native via Converse — The suffix-less anthropic.claude-haiku-4-5 id serves the Messages API on bedrock-runtim… |
| `anthropic.claude-mythos-5` | — | `limited_availability` | — | text, image, file | text | — | ✓ | ✓ adaptive | not reachable: Messages API on bedrock-mantle only; Converse and InvokeModel are not supported — us-east-1 pr… |
| `anthropic.claude-mythos-preview` | — | `limited_availability` | — | text, image, file | text | — | ✓ | ✓ adaptive (low, medium, high, max) | not reachable: Messages API on bedrock-mantle only — Invitation only; AWS lifecycle 'Preview' (us-east-1, ap-… |
| `amazon.nova-2-lite-v1:0` | — | `active` | — | text, image, video, file | text | IS | ✓ | ✓ (low, medium, high) | native via Converse — The only Converse-capable Nova 2 model. Profiles: us., eu., jp., global. Extended think… |
| `amazon.nova-2-sonic-v1:0` | — | `active` | — | text, audio | text, audio | — | ✓ | ✗ | not supported: InvokeModelWithBidirectionalStream only |
| `amazon.nova-2-5-sonic` | — | `active` | — | text, audio | text, audio | — | ✓ | ✗ | not supported: InvokeModelWithBidirectionalStream only — Launched 2026-10-05 (EOL no sooner than 2027-10-05,… |
| `amazon.nova-2-multimodal-embeddings-v1:0` | — | `active` | — | text, image, audio, video | embedding | E | ✗ | ✗ | text embeddings via InvokeModel (SINGLE_EMBEDDING schema); image/audio/video input not mapped — us-east-1 and… |
| `amazon.nova-pro-v1:0` | — | `active` | — | text, image, video, file | text | IS | ✓ | ✗ | native via Converse — 300K context. The Bedrock model card says 10K max output (2026-10-14), as does the Nova… |
| `amazon.nova-lite-v1:0` | — | `active` | — | text, image, video, file | text | IS | ✓ | ✗ | native via Converse — 300K context. The Bedrock model card says 10K max output (2026-10-14), as does the Nova… |
| `amazon.nova-micro-v1:0` | — | `active` | — | text | text | IS | ✓ | ✗ | native via Converse — Text only. 128K context. The Bedrock model card says 10K max output (2026-10-14), as do… |
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
| `openai.gpt-6.1-sol` | — | `active` | — | text, image | text | IS | ✓ | ✓ | Converse text, images, and tools; reasoning effort is not mapped on Converse — Launched 2026-09-29; follows O… |
| `openai.gpt-6.1-terra` | — | `active` | — | text, image | text | IS | ✓ | ✓ | Converse text, images, and tools; reasoning effort is not mapped on Converse — Launched 2026-10-26 (EOL no so… |
| `openai.gpt-6-sol` | — | `active` | — | text, image | text | IS | ✓ | ✓ (none, low, medium, high, xhigh, max) | Converse text, images, and tools; reasoning effort is not mapped on Converse — Launched 2026-09-22 (EOL no so… |
| `openai.gpt-6-luna` | — | `active` | — | text, image | text | IS | ✓ | ✓ (none, low, medium, high, xhigh, max) | Converse text, images, and tools; reasoning effort is not mapped on Converse — Launched 2026-09-22 (EOL no so… |
| `moonshotai.kimi-k3` | — | `active` | — | text, image | text | IS | ✗ | ✓ | Converse text and images; do not replay earlier reasoning content — Launched 2026-09-18 (EOL notice at least… |
| `xai.grok-4.7` | — | `active` | — | text, image | text | IS | ✓ | ✓ (low, medium, high, xhigh) | Converse text, images, and tools; reasoning effort is not mapped on Converse — Launched 2026-09-28 (EOL no so… |
| `openai.gpt-6-astra` | — | `active` | — | text, image | text | IS | ✓ | ✓ | Converse text, images, and tools; reasoning effort is not mapped on Converse — Launched on Bedrock 2026-09-08… |
| `openai.gpt-daybreak-blue-5.6-sol` | — | `limited_availability` | — | text, image | text | — | ✓ | ✓ | not reachable: Responses and Chat Completions on bedrock-mantle only (us-east-2), Trusted Access for Cyber re… |
| `openai.gpt-oss-120b-1:0` | — | `active` | — | text | text | IS | ✓ | ✓ | native via Converse; reasoning effort is not mapped on Converse — Open-weight; Converse, InvokeModel, and Cha… |
| `openai.gpt-oss-20b-1:0` | — | `active` | — | text | text | IS | ✓ | ✓ | native via Converse; reasoning effort is not mapped on Converse |
| `xai.grok-4.6` | — | `active` | — | text, image | text | IS | ✓ | ✓ (low, medium, high, xhigh) | Converse text, images, and tools; reasoning content and effort are Responses-API only — Launched 2026-08-18 (… |
| `xai.grok-4.3` | — | `active` | — | text | text | — | ✓ | ✓ (none, low, medium, high) | not reachable: Responses and Chat Completions on bedrock-mantle only (Converse and InvokeModel are not suppor… |
| `deepseek.v3.2` | — | `active` | — | text | text | IS | ✓ | ✓ | native via Converse — Launched 2025-12-01; Converse, InvokeModel, Chat Completions; structured outputs suppor… |
| `mistral.mistral-large-3-675b-instruct` | — | `active` | — | text, image | text | IS | ✓ | ✗ | native via Converse — Converse, InvokeModel, Chat Completions; structured outputs supported. |
| `qwen.qwen3-coder-next` | — | `active` | — | text | text | IS | ✓ | ✓ | native via Converse — Launched 2026-02-04. |
| `zai.glm-5` | — | `active` | — | text | text | IS | ✓ | ✓ | native via Converse — Launched 2026-02-11. |
| `zai.glm-5.3` | — | `active` | — | text | text | IS | ✓ | ✓ | native via Converse — Launched 2026-10-05 (at least 45 days' EOL notice). Cross-Region only (us.zai.glm-5.3,… |
| `moonshotai.kimi-k2.5` | — | `active` | — | text, image | text | IS | ✓ | ✓ | native via Converse — Launched 2026-01-27; images up to 3 MB. |
| `nvidia.nemotron-super-3-120b` | — | `active` | — | text | text | IS | ✓ | ✓ | native via Converse — Launched 2026-03-11; us-gov. profile in GovCloud. |
| `minimax.minimax-m2.5` | — | `active` | — | text | text | IS | ✓ | ✓ | native via Converse — Launched 2026-02-12. |
| `google.gemma-4-*` | — | `active` | — | text, image, video | text | — | ✓ | ✗ | not reachable: Gemma 4 is served on bedrock-mantle only (no Converse) — google.gemma-4-31b, google.gemma-4-26… |
| `twelvelabs.pegasus-1-5-v1:0` | — | `active` | — | text, video | text | — | ✗ | ✗ | not supported: InvokeModel only (no Converse) — Launched 2026-10-08 per the model card (45-day Legacy period)… |
| `twelvelabs.marengo-embed-3-0-v1:0` | — | `active` | — | text, image, audio, video | embedding | — | ✗ | ✗ | not supported: the InvokeModel embedding builder covers Titan, Cohere, and Nova schemas only — Launched 2025-… |
| `stability.sd3-5-large-v1:0` | — | `active` | — | image | image | P | ✗ | ✗ | generate_images: InvokeModel (text-to-image, or image-to-image with one input image and strength); n loops se… |
| `stability.stable-image-ultra-v1:1` | — | `active` | — | image | image | P | ✗ | ✗ | generate_images: InvokeModel (text-to-image, or image-to-image with strength) — Stability AI Stable Image Ult… |
| `stability.stable-image-core-v1:1` | — | `active` | — | — | image | P | ✗ | ✗ | generate_images: InvokeModel (text-to-image only) — Stability AI Stable Image Core 1.0, us-west-2; png or jpe… |
| `luma.ray-v2:0` | — | `active` | — | image | video | M | ✗ | ✗ | start_video / get_video: StartAsyncInvoke and GetAsyncInvoke; output_uri (s3://) is required; first and last… |

#### Deprecated and legacy

| Model | Aliases | Status | Dates | Input | Output | Ops | Tools | Reasoning | GAISe support / notes |
|---|---|---|---|---|---|---|---|---|---|
| `anthropic.claude-sonnet-4-5-20250929-v1:0` | — | `legacy` | shutdown 2027-04-08 · not before 2026-09-30 | text, image, file | text | IS | ✓ | ✓ manual | native via Converse — Profiles: us., eu., au., jp., global. The lifecycle table shows Legacy since 2026-10-08… |
| `meta.llama4-maverick-17b-instruct-v1:0` | — | `legacy` | shutdown 2027-03-30 | text, image | text | IS | ✓ | ✗ | native via Converse — us. profile; structured outputs not supported. Legacy since 2026-09-30 per the lifecycl… |
| `anthropic.claude-opus-4-1-20250805-v1:0` | — | `legacy` | shutdown 2027-01-08 | — | — | — | ? | ? | Legacy since 2026-07-08; public extended access (higher pricing) from 2026-10-08. us. profile only. |
| `ai21.jamba-1-5-*` | — | `legacy` | shutdown 2026-11-26 | — | — | — | ? | ? | ai21.jamba-1-5-large-v1:0 and ai21.jamba-1-5-mini-v1:0. Legacy since 2026-05-26; extended access from 2026-08… |
| `twelvelabs.marengo-embed-2-7-v1:0` | — | `legacy` | shutdown 2026-11-30 | text, image, audio, video | embedding | — | ✗ | ✗ | not supported: no InvokeModel embedding builder for this schema — Legacy since 2026-05-29; extended access fr… |

#### Retired

| Model | Aliases | Status | Dates | Input | Output | Ops | Tools | Reasoning | GAISe support / notes |
|---|---|---|---|---|---|---|---|---|---|
| `anthropic.claude-sonnet-4-20250514-v1:0` | — | `retired` | shutdown 2026-10-14 | — | — | — | ? | ? | Legacy since 2026-04-14; extended-access pricing applied from 2026-07-14. AWS EOL date 2026-10-14; by 2026-10… |
| `anthropic.claude-3-haiku-20240307-v1:0` | — | `retired` | shutdown 2026-09-10 | — | — | — | ? | ? | Legacy since 2026-03-10; extended access from 2026-06-10; the AWS EOL date 2026-09-10 (commercial and GovClou… |
| `anthropic.claude-3-5-haiku-20241022-v1:0` | — | `retired` | shutdown 2026-06-19 | — | — | — | ? | ? | AWS EOL date 2026-06-19 has passed; the card still reads Legacy and the id still appears in the API-compatibi… |
| `amazon.nova-premier-v1:0` | — | `retired` | shutdown 2026-09-14 | — | — | — | ? | ? | EOL date passed; AWS's legacy table still listed it on 2026-10-01. |
| `amazon.nova-sonic-v1:0` | — | `retired` | shutdown 2026-09-14 | — | — | — | ? | ? | EOL date passed; AWS's legacy table still listed it on 2026-10-01. |
| `amazon.nova-reel-v1:*` | — | `retired` | shutdown 2026-09-30 | — | — | — | ? | ? | EOL date passed; AWS's legacy table still listed it on 2026-10-01. |
| `amazon.nova-canvas-v1:0` | — | `retired` | shutdown 2026-09-30 | — | — | — | ? | ? | EOL date passed; AWS's legacy table still listed it on 2026-10-01. |
| `cohere.command-r-*` | — | `retired` | shutdown 2026-08-19 | — | — | — | ? | ? | cohere.command-r-v1:0 and cohere.command-r-plus-v1:0. |

## ollama

### Ollama

Decision models (`nimble`, `tev1`, `clef`, `clef-flash`, `laya`, `d1`, `pplx-decider`, and others) are driven through `decision` on Ollama's `POST /v1/systemone`; Clef, Clef Flash, d1, and pplx-decider also take images through `decision_with_images`. The installed catalog is dynamic (`GET /api/tags`); entries are family globs describing typical capabilities, and `-cloud` tags match the same globs. `POST /api/show` (opt-in `include_details`) reports the real capabilities of each installed tag ([`catalog.rs`](../gaise-provider-ollama/src/contracts/catalog.rs)).

- Vendor page: [vendor-ollama.md](vendor-ollama.md) · GAISe surface: Chat, streaming chat, Embeddings, and typed decisions (System One)
- Discovery: GET /api/tags
- Official catalog: <https://ollama.com/search> · lifecycle: <dynamic local catalog>
- Ollama 0.35 (released 2026-09-28, announced 2026-09-29) added POST /v1/systemone, the TypeSafe System One wire protocol for local decision models (nimble, tev1); the adapter maps it to decision. Tags are installed locally and can move; Ollama has no centralized retirement calendar. Cloud-hosted tags (`family:size-cloud`, run through ollama.com with an API key) match the same family globs. API as of Ollama v0.34.0 (2026-09-05; the release adds OpenAI-compatible tool search and response compaction on /v1 only): `think` accepts true/false or the strings low, medium, high, max (any other string is rejected; GPT-OSS takes levels only); chat and generate responses report prompt_eval_cached_count; requests accept logprobs/top_logprobs; /api/show capabilities may include image and audio; the default repeat_penalty became 1.0 in v0.32.10. Audit 2026-10-01: think also accepts null (model default) and, since v0.34.3, /api/show advertises each model's thinking values and default; named levels a model does not support fall back to its default instead of failing. typical_p is deprecated (v0.34.1; warns rather than fails since v0.35.0). v0.35.1 adds a decision capability to /api/show, which the adapter maps to the decision operation. No new decision models; no Gemini 4 or Gemma 5 in the library. Audit 2026-10-09: Ollama 0.35.1 (2026-09-29) added Clef and Clef Flash (Cloudflare, multimodal decision models) and an images field on /v1/systemone (base64, no URLs; 32 MiB body with images), which GAISe maps through decision_with_images; v0.40.0 runs models on MLX by default and adds Laya (decision, MLX only) and embeddinggemma-2; v0.40.1 proxies cloud usage and balance APIs; v0.40.2 upgrades older llama.cpp models in the background. The chat docs now say think strings are model-defined and must match /api/show exactly (numbers are not supported). Mistral Large 4 is cloud only. No OpenAI-Decisions-compatible endpoint. Audit 2026-10-14: v0.40.3 (2026-10-11) paused the background llama.cpp upgrade; v0.40.4 adds Kev (MLX), ollama run --ask for decision models, multimodal embeddings through llama-server, and server-side /v1/systemone validation (score 2-10 levels, choice 2-26 options for most models with a schema maximum of 255, empty state rejected, no noul defaults). New decision families kev, nimble-v3, and strands-decider. The thinking docs now say unsupported think names fall back to the model default. Audit 2026-10-16: v0.40.5 (2026-10-14) serves Clef from ollama.com cloud (clef:cloud through a signed-in local server, or https://ollama.com/v1/systemone with a Bearer key, which GAISe does not send) and adds Nemotron 3.5 Super and Bolt; the library added the d1 and pplx-decider decision families (images supported) and the cloud-only muse-spark-open. The local System One limit is now 64 KiB of text and schema excluding images. Audit 2026-10-29: v0.40.6 through v0.40.13 (2026-10-16 to 2026-10-28) add Flint (v0.40.8), Nemotron 3.5 Bolt e4b (v0.40.10), the d3 decision family and a speech endpoint (/v1/audio/speech, Gradium Phonon; v0.40.11), and LFM3 with d1:2b (v0.40.12); the library also lists lfm3-vl, bonsai-2, d1-omni, and the cloud-only beam and minimax-m3.1-flash. /v1/systemone images accept WAV clips on models with audio input. /api/show adds renderer, parser, and template_source (v0.40.7). The think contract is unchanged. Thinking values for the new chat families are not published, so no reasoning values are recorded and the adapter sends booleans.

#### Current and preview

| Model | Aliases | Status | Dates | Input | Output | Ops | Tools | Reasoning | GAISe support / notes |
|---|---|---|---|---|---|---|---|---|---|
| `nimble:*` | — | `dynamic_local` | — | text | — | D | ✗ | ✗ | GaiseClient::decision and POST /v1/decision through Ollama's POST /v1/systemone (Ollama 0.35+) — Bespoke Labs… |
| `tev1:*` | — | `dynamic_local` | — | text | — | D | ✗ | ✗ | GaiseClient::decision and POST /v1/decision through Ollama's POST /v1/systemone (Ollama 0.35+) — Experimental… |
| `clef:*` | — | `dynamic_local` | — | text, image | — | D | ✗ | ✗ | GaiseClient::decision and decision_with_images through Ollama's POST /v1/systemone (Ollama 0.35.1+) — Cloudfl… |
| `clef-flash:*` | — | `dynamic_local` | — | text, image | — | D | ✗ | ✗ | GaiseClient::decision and decision_with_images through Ollama's POST /v1/systemone (Ollama 0.35.1+) — Cloudfl… |
| `laya:*` | — | `dynamic_local` | — | text | — | D | ✗ | ✗ | GaiseClient::decision through Ollama's POST /v1/systemone (Ollama 0.40+) — Convai Innovations 421M ModernBERT… |
| `nimble-v3:*` | — | `dynamic_local` | — | text | — | D | ✗ | ✗ | GaiseClient::decision through Ollama's POST /v1/systemone (Ollama 0.40.4+) — Bespoke Labs 9B decision model f… |
| `kev:*` | — | `dynamic_local` | — | text | — | D | ✗ | ✗ | GaiseClient::decision through Ollama's POST /v1/systemone (Ollama 0.40.4+, MLX runner on Apple silicon) — Jar… |
| `strands-decider:*` | — | `dynamic_local` | — | text | — | D | ✗ | ✗ | GaiseClient::decision through Ollama's POST /v1/systemone (Ollama 0.40+ on Apple silicon) — Strands Agents 2B… |
| `d1:*` | — | `dynamic_local` | — | text, image | — | D | ✗ | ✗ | GaiseClient::decision and decision_with_images through Ollama's POST /v1/systemone — Liquid AI decision model… |
| `pplx-decider:*` | — | `dynamic_local` | — | text, image | — | D | ✗ | ✗ | GaiseClient::decision and decision_with_images through Ollama's POST /v1/systemone — Perplexity 27B decision… |
| `d3:*` | — | `dynamic_local` | — | text, image | — | D | ✗ | ✗ | GaiseClient::decision and decision_with_images through Ollama's POST /v1/systemone (Ollama 0.40.9+) — vLLM Se… |
| `d1-omni:*` | — | `dynamic_local` | — | text, image, audio | — | D | ✗ | ✗ | GaiseClient::decision and decision_with_images through Ollama's POST /v1/systemone; a WAV clip goes in the im… |
| `qwen3:*` | — | `dynamic_local` | — | text | text | IS | ✓ | ✓ (true, false) | native — Context 40,960 on the original dense tags (0.6b-32b); the 2507 builds, 4b, 30b, and 235b are 262,144… |
| `gpt-oss:*` | — | `dynamic_local` | — | text | text | IS | ✓ | ✓ (low, medium, high) | native — Context 131,072 on 20b and 120b. |
| `deepseek-r1:*` | `deepseek-v3.1:*` | `dynamic_local` | — | text | text | IS | ✓ | ✓ (true, false) | native — Context 131,072 on the distilled tags; deepseek-r1:671b and deepseek-v3.1 are 163,840. The library l… |
| `gemma4:*` | — | `dynamic_local` | — | text, image, audio | text | IS | ✓ | ✓ (true, false) | native when the installed tag advertises the capability (/api/show) — Context 131,072 on e2b/e4b; 12b, 26b, a… |
| `qwen3.8:*` | `qwen3.8-flash-next:*` | `dynamic_local` | — | text, image | text | IS | ✓ | ✓ (true, false) | native — Qwen 3.8 27B (Ollama v0.32.12, 2026-08-14) and the MLX-only Qwen 3.8 Flash Next 125B-A6B (v0.33.1, 2… |
| `qwen3.6:*` | — | `dynamic_local` | — | text, image | text | IS | ✓ | ✓ (true, false) | native — 27b and 35b tags; 256K context; retains reasoning context across turns. |
| `qwen3.5:*` | — | `dynamic_local` | — | text, image | text | IS | ✓ | ✓ (true, false) | native — 0.8b-122b tags, all 256K context; multimodal; cloud tags available. |
| `muse-glimmer:*` | — | `dynamic_local` | — | text, image | text | IS | ✓ | ✓ (true, false) | native (boolean thinking toggle) — Meta's 30B open model for local agents (2026-08-10, Apache 2.0); documents… |
| `nemotron-3.5-lightning:*` | — | `dynamic_local` | — | text | text | IS | ✓ | ✓ (true, false) | native — NVIDIA 30B-A3B MoE for always-on agents (2026-08-11); 1M context on the GGUF tag, 256K on MLX. |
| `nemotron-3.5-super:*` | — | `dynamic_local` | — | text, image | text | IS | ✓ | ✓ (true, false) | native — NVIDIA 120B-A12B vision-language model for coding, reasoning, tools, and documents (Ollama v0.40.5,… |
| `nemotron-3.5-bolt:*` | — | `dynamic_local` | — | text, image | text | IS | ✓ | ✓ (true, false) | native — NVIDIA vision-language models (OpenMDW 1.1): 74B-A9B since Ollama v0.40.5 (2026-10-14; 256K context,… |
| `muse-spark-open:*` | — | `dynamic_local` | — | text, image | text | IS | ✓ | ✓ | native through a signed-in local server (boolean thinking toggle) — Meta 816B-A42B natively multimodal model… |
| `laguna-s-2.1:*` | — | `dynamic_local` | — | text | text | IS | ✓ | ✓ (true, false) | native — Ollama's own 118B-A8B model for long-horizon work (OpenMDW-1.1 licence); tool calling and interleave… |
| `laguna-xs-2.1:*` | — | `dynamic_local` | — | text | text | IS | ✓ | ✓ (true, false) | native — Ollama's 33B-A3B MoE for local agentic coding (OpenMDW-1.1 licence, 2026-09); 256K on every tag (q4_… |
| `glm-5.3:*` | — | `dynamic_local` | — | text | text | IS | ✓ | ✓ (low, high, max) | native (cloud tag only; served through ollama.com with an API key) — Z.ai GLM-5.3, cloud-only glm-5.3:cloud (… |
| `glm-5.3-flash:*` | — | `dynamic_local` | — | text, image | text | IS | ✓ | ✓ (low, high, max) | native (cloud tag only; served through ollama.com with an API key) — GLM-5.3 Flash, 320B-A18B natively multim… |
| `deepseek-v4.1-flash:*` | — | `dynamic_local` | — | text, image | text | IS | ✓ | ✓ (true, false) | native (cloud tag only; served through ollama.com with an API key) — DeepSeek V4.1 Flash (2026-09-10), 763B M… |
| `kimi-k3:*` | — | `dynamic_local` | — | text, image | text | IS | ✓ | ✓ (true, false) | native (cloud tag only; served through ollama.com with an API key) — Moonshot Kimi K3, 2.81T-parameter open-w… |
| `granite4.2:*` | — | `dynamic_local` | — | text | text | IS | ✓ | ✓ (low, high) | native — IBM Granite 4.2 3b/8b/30b (Apache 2.0, 2026-08); 128K context; tool use, structured JSON output, and… |
| `ornith-1.5:*` | — | `dynamic_local` | — | text, image | text | IS | ✗ | ✗ | native — Ornith 1.5 9b/35b/397b (2026-08); 256K context; vision badge only (no tools or thinking badge, unlik… |
| `mistral-medium-3.5:*` | — | `dynamic_local` | — | text, image | text | IS | ✓ | ✓ (true, false) | native — 128B single-weight model with a configurable reasoning mode; 256K context. |
| `mistral-large-4:*` | — | `dynamic_local` | — | text, image | text | IS | ✓ | ✓ (true, false) | native — Cloud only (library, about 2026-10-07): vision, tools, thinking, 1M context. /api/show reports no th… |
| `llama4:*` | — | `dynamic_local` | — | text, image | text | IS | ✓ | ✗ | native — Scout 16x17b (10M context) and Maverick 128x17b (1M); vision and tools, no thinking. Context recorde… |
| `flint:*` | — | `dynamic_local` | — | text | text | IS | ✓ | ✓ | native (boolean thinking toggle) — Recursive's coding model, 40B MoE with about 2.5B active (Apache 2.0), Oll… |
| `lfm3:*` | — | `dynamic_local` | — | text | text | IS | ✓ | ✓ | native (boolean thinking toggle) — Liquid AI LFM3 1.7B text model (LFM Open License v1.0), Ollama v0.40.12 (2… |
| `lfm3-vl:*` | — | `dynamic_local` | — | text, image | text | IS | ✓ | ✓ | native (boolean thinking toggle) — Liquid AI LFM3 vision-language model (library, about 2026-10-27); 128K con… |
| `bonsai-2:*` | — | `dynamic_local` | — | text, image | text | IS | ✓ | ✓ | native (boolean thinking toggle) — Prism ML ternary-weight models (2b, 4b, 9b, 27b; Apache 2.0), in the libra… |
| `beam:*` | — | `dynamic_local` | — | text | text | IS | ✓ | ✓ | native through a signed-in local server (boolean thinking toggle) — 501B text model, cloud tag only (beam:clo… |
| `minimax-m3.1-flash:*` | — | `dynamic_local` | — | text, image | text | IS | ✓ | ✓ | native through a signed-in local server (boolean thinking toggle) — MiniMax M3.1 Flash, 428B, cloud tag only… |
| `embeddinggemma:*` | — | `dynamic_local` | — | text | embedding | E | ✗ | ✗ | native — EmbeddingGemma 300m; Matryoshka 768/512/256/128; 2,048-token context; Google prompt-instruction conv… |
| `embeddinggemma-2:*` | — | `dynamic_local` | — | text, image, audio, video | embedding | E | ✗ | ✗ | native (text only through GAISe) — EmbeddingGemma 2 (270m, 440m, 570m, 740m tags; Ollama 0.40+): 768 dimensio… |
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
- Voices are account-specific and default voices expire 2026-12-31; never hard-code a voice id. Character limits per request are model-specific; the character-cost header reports billing. Audit 2026-10-01: eleven_v4 and eleven_v4_turbo (2026-09-28) use the text-to-dialogue WebSocket, which accepts model ids starting with eleven_v3 or eleven_v4; GET /v1/models deprecated max_characters_request_free_user and max_characters_request_subscribed_user in favour of maximum_text_length_per_request; is_final_audio_for_turn now follows every buffered byte (2026-09-21). Audit 2026-10-09: no TTS model or lifecycle change; the only changelog entry since 2026-09-28 (2026-10-05) makes eleven_v4_turbo the ElevenAgents default voice model and removes the crawl-job max_depth field. scribe_v2_medical (clinical speech-to-text, same Speech to Text API as Scribe v2) is listed on the models page with no date. Audit 2026-10-14: no model or lifecycle change. The 2026-10-12 changelog adds an optional audio_effects object to the text-to-speech and text-to-dialogue HTTP endpoints (stereo effects return 400 with pcm, ulaw, and alaw output); GAISe does not send it. Audit 2026-10-16: no change (no changelog after 2026-10-12; the models page matches the registry). Audit 2026-10-29: no speech or transcription model and no lifecycle change. The 2026-10-19 changelog adds Eleven Realtime (eleven_realtime_v1_mini, eleven_realtime_v1_max), complete voice-agent configurations for ElevenAgents and an OpenAI-Realtime-compatible WebSocket, which GAISe does not drive; use_pvc_as_ivc became nullable and speech-to-text gained edited_text. The 2026-10-26 changelog adds a clone_strength voice setting (and deprecates use_pvc_as_ivc on text-to-dialogue), honours realtime vad_silence_threshold_secs below 0.5 s (minimum about 0.19 s; the adapter forwards silence_duration_ms unclamped, so short values now take effect), and deprecates the /v1/speech-to-speech paths in favour of /v1/voice-changer. GAISe sends neither voice field.

#### Current and preview

| Model | Aliases | Status | Dates | Input | Output | Ops | Tools | Reasoning | GAISe support / notes |
|---|---|---|---|---|---|---|---|---|---|
| `eleven_v4` | — | `active` | — | text | text, audio | L | ✗ | ✗ | realtime via the text-to-dialogue WebSocket (up to 10 voices); /v1/text-to-speech is not documented for v4 —… |
| `eleven_v4_turbo` | — | `active` | — | text | text, audio | L | ✗ | ✗ | realtime only via the text-to-dialogue WebSocket (exactly one voice) — Released 2026-09-28; ElevenLabs' most… |
| `eleven_v3` | — | `active` | — | text | audio | VL | ✗ | ✗ | speech via /v1/text-to-speech; realtime via the text-to-dialogue WebSocket — Flagship, 70+ languages, 5,000 c… |
| `eleven_v3_conversational` | — | `active` | — | text | text, audio | L | ✗ | ✗ | realtime only via the text-to-dialogue WebSocket (one voice) — ~280 ms latency variant of v3 for realtime use. |
| `eleven_multilingual_v2` | — | `active` | — | text | audio | VL | ✗ | ✗ | native — Default model; 29 languages; 10,000 characters per request. language_code is documented as not suppo… |
| `eleven_flash_v2_5` | — | `active` | — | text | audio | VL | ✗ | ✗ | native — ~75 ms latency, 32 languages, 40,000 characters per request, accepts language_code. Numbers are not… |
| `eleven_flash_v2` | — | `active` | — | text | audio | VL | ✗ | ✗ | native — English only; 30,000 characters per request. |
| `eleven_multilingual_sts_v2` | `eleven_english_sts_v2` | `active` | — | audio | audio | — | ✗ | ✗ | not supported: speech-to-speech has no GAISe surface |
| `scribe_v2` | — | `active` | — | text, audio | text | T | ✗ | ✗ | transcribe: POST /v1/speech-to-text (diarization up to 32 speakers, word or character timestamps, audio event… |
| `scribe_v2_medical` | — | `active` | — | text, audio | text | T | ✗ | ✗ | transcribe: POST /v1/speech-to-text, same request shape as scribe_v2 (smart mode is rejected) — Speech recogn… |
| `scribe_v2_realtime` | — | `active` | — | text, audio | text | W | ✗ | ✗ | transcribe_connect: wss://api.elevenlabs.io/v1/speech-to-text/realtime (feature live); partial and committed… |
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

## xai

### xAI

Grok text models through **Chat Completions** (instruct, streaming, function tools, images; `reasoning_content` becomes reasoning parts), text-to-speech as `xai::tts` (`POST /v1/tts` has no model id), and speech-to-speech over `wss://api.x.ai/v1/realtime` (feature `live`). `GET /v1/models` reports aliases, context length, and accepted reasoning efforts ([`catalog.rs`](../gaise-provider-xai/src/contracts/catalog.rs)). Retired slugs still resolve and redirect upstream to grok-4.3; the multi-agent model is Responses-only, and Imagine image/video and speech-to-text have no GAISe surface.

- Vendor page: [vendor-xai.md](vendor-xai.md) · GAISe surface: Chat Completions (instruct, streaming, function tools), text-to-speech, realtime speech-to-speech, and model listing
- Discovery: GET /v1/models (id, created, owned_by, aliases, context_length, capabilities.reasoning_effort and default, prices); GET /v1/tts/voices for voices
- Official catalog: <https://docs.x.ai/developers/models> · lifecycle: <https://docs.x.ai/developers/release-notes>
- Added 2026-10-17 from docs.x.ai (now branded SpaceXSI; the API host is still api.x.ai). The instruct client uses Chat Completions, which xAI labels a legacy endpoint but still serves for every text model except the multi-agent model; the Responses API (recommended upstream) is needed for encrypted reasoning, server-side tools (web_search, x_search, code execution, collections, MCP), and the multi-agent model, none of which GAISe maps. Chat Completions contract: max_completion_tokens (max_tokens is deprecated; default 128,000, excluding reasoning); reasoning_effort per model (grok-4.7/4.6 low-xhigh, grok-4.5 low-high, grok-4.3 none-xhigh); presence_penalty, frequency_penalty, and stop return an error on reasoning models (GAISe never sends them); logprobs are ignored on grok-4.20 and newer; responses may carry reasoning_content; tool calls stream whole in one chunk; usage reports cost_in_usd_ticks (1 USD = 10^10 ticks). There is no deprecations page: retirements are announced in the release notes and migration guides (the 2026-05-15 retirement redirects old slugs to grok-4.3). No public embedding model is listed. The US regional endpoint (https://us.api.x.ai/v1) serves only grok-4.7 and grok-4.6 and no voice, image, or video API. Text-to-speech (POST /v1/tts) has no model id; GAISe routes it as xai::tts. Audit 2026-10-29: no new model and no lifecycle change; grok-imagine-image-quality still retires on 2026-11-02. The October release notes add an optional guidelines array (custom moderation for nsfw, violence, and profanity) to Chat Completions and Responses requests, which GAISe does not send; safety_identifier (September) is also optional and not sent.

#### Current and preview

| Model | Aliases | Status | Dates | Input | Output | Ops | Tools | Reasoning | GAISe support / notes |
|---|---|---|---|---|---|---|---|---|---|
| `grok-4.7` | — | `active` | — | text, image | text | IS | ✓ | ✓ (low, medium, high, xhigh) | native via Chat Completions (instruct, streaming, function tools, images); reasoning_content mapped to reason… |
| `grok-4.6` | — | `active` | — | text, image | text | IS | ✓ | ✓ (low, medium, high, xhigh) | native via Chat Completions — Released August 2026. Same effort set as grok-4.7 (default high, reasoning cann… |
| `grok-4.5` | `grok-4.5-latest`, `grok-build-latest` | `active` | — | text, image | text | IS | ✓ | ✓ (low, medium, high) | native via Chat Completions — Released July 2026. The model page lists xhigh, but the reasoning guide says xh… |
| `grok-4.3` | `grok-4.3-latest` | `active` | — | text, image | text | IS | ✓ | ✓ (none, low, medium, high, xhigh) | native via Chat Completions — Default effort low; none disables reasoning. Redirect target of the slugs retir… |
| `grok-4.20-0309-reasoning` | `grok-4.20`, `grok-4.20-reasoning`, `grok-4.20-reasoning-latest`, `grok-4.20-0309`, `grok-4.20-reasoning-gv2` | `active` | — | text, image | text | IS | ✓ | ✓ | native via Chat Completions; reasoning_effort is not sent (no values documented) — Released March 2026. The m… |
| `grok-4.20-0309-non-reasoning` | `grok-4.20-non-reasoning`, `grok-4.20-non-reasoning-latest`, `grok-4.20-beta-non-reasoning`, `grok-4.20-non-reasoning-gv2` | `active` | — | text, image | text | IS | ✓ | ✗ | native via Chat Completions — Released March 2026; non-reasoning variant of Grok 4.20. |
| `grok-4.20-multi-agent-0309` | `grok-4.20-multi-agent`, `grok-4.20-multi-agent-latest`, `grok-4.20-multi-agent-beta-0309` | `preview` | — | text, image | text | — | ✓ | ✓ (low, medium, high, xhigh) | not supported: Responses API only (the model does not work with Chat Completions); the adapter fails fast — B… |
| `grok-build-0.1` | `grok-code-fast-1`, `grok-code-fast`, `grok-code-fast-1-0825` | `preview` | — | text, image | text | IS | ✓ | ✓ | native via Chat Completions; reasoning_effort is not sent (no values documented) — Early access since May 202… |
| `grok-voice-3.0` | `grok-voice-latest` | `active` | — | text, audio | text, audio | L | ✓ | ✓ (none, high) | realtime transport (wss://api.x.ai/v1/realtime, feature live): 24 kHz PCM16, server VAD or manual turns, func… |
| `tts` | — | `active` | — | text | audio | V | ✗ | ✗ | speech and speech_stream via POST /v1/tts (mp3, wav, pcm, mulaw, alaw; 8-48 kHz); alignment from with_timesta… |
| `grok-voice-transcribe-2.0` | — | `active` | — | text, audio | text | TW | ✗ | ✗ | transcribe: POST /v1/stt (diarization, word timestamps, keyterms, multichannel); transcribe_connect: wss://ap… |
| `grok-imagine-image-2.0` | — | `active` | — | image | image | P | ✗ | ✗ | generate_images: /v1/images/generations or /edits (multi-image references); quality low, medium, auto; resolu… |
| `grok-imagine-image` | — | `active` | — | image | image | P | ✗ | ✗ | generate_images: /v1/images/generations or /edits; quality is not accepted — Grok Imagine Image 1.0 (snapshot… |
| `grok-imagine-video-1.5` | `grok-imagine-video-1.5-preview` | `active` | — | image | video | M | ✗ | ✗ | start_video / get_video: /v1/videos/generations (first and last frame, references, 1-15 s, 480p-1080p) — Text… |
| `grok-imagine-video-1.5-lite` | — | `active` | — | image | video | M | ✗ | ✗ | start_video / get_video: /v1/videos/generations — Lower-cost Grok Imagine Video 1.5. $0.020 per second. |
| `grok-imagine-video` | — | `active` | — | image, video | video | M | ✗ | ✗ | start_video / get_video: generations, edits (source video), and extensions (source video plus duration) — Gro… |

#### Deprecated and legacy

| Model | Aliases | Status | Dates | Input | Output | Ops | Tools | Reasoning | GAISe support / notes |
|---|---|---|---|---|---|---|---|---|---|
| `grok-voice-2.0` | — | `deprecated` | — | text, audio | text, audio | L | ✓ | ✓ (none, high) | realtime transport (feature live) — Deprecated October 2026 when Grok Voice 3.0 shipped with the same endpoin… |
| `grok-imagine-image-quality` | `grok-imagine-image-quality-20260403`, `grok-imagine-image-quality-latest`, `grok-imagine-image-pro` | `deprecated` | shutdown 2026-11-02 | image | image | P | ✗ | ✗ | generate_images: /v1/images/generations or /edits; served by grok-imagine-image-2.0 at low quality after 2026… |

#### Retired

| Model | Aliases | Status | Dates | Input | Output | Ops | Tools | Reasoning | GAISe support / notes |
|---|---|---|---|---|---|---|---|---|---|
| `grok-4-1-fast-reasoning` | — | `retired` | shutdown 2026-05-15 | text, image | text | IS | ✓ | ✓ | redirected upstream; reasoning_effort is not sent — Retired 2026-05-15 12:00 PT. The slug still resolves and… |
| `grok-4-1-fast-non-reasoning` | — | `retired` | shutdown 2026-05-15 | text, image | text | IS | ✓ | ✗ | redirected upstream — Retired 2026-05-15 12:00 PT. The slug still resolves and is served by grok-4.3 with rea… |
| `grok-4-fast-reasoning` | — | `retired` | shutdown 2026-05-15 | text, image | text | IS | ✓ | ✓ | redirected upstream; reasoning_effort is not sent — Retired 2026-05-15 12:00 PT. The slug still resolves and… |
| `grok-4-fast-non-reasoning` | — | `retired` | shutdown 2026-05-15 | text, image | text | IS | ✓ | ✗ | redirected upstream — Retired 2026-05-15 12:00 PT. The slug still resolves and is served by grok-4.3 with rea… |
| `grok-4-0709` | — | `retired` | shutdown 2026-05-15 | text, image | text | IS | ✓ | ✓ | redirected upstream; reasoning_effort is not sent — Retired 2026-05-15 12:00 PT. The slug still resolves and… |
| `grok-3` | — | `retired` | shutdown 2026-05-15 | text | text | IS | ✓ | ✗ | redirected upstream — Retired 2026-05-15 12:00 PT. The slug still resolves and is served by grok-4.3 with rea… |
| `grok-voice-transcribe-1.0` | — | `retired` | shutdown 2026-10-02 | text, audio | text | TW | ✗ | ✗ | redirected upstream to grok-voice-transcribe-2.0 — End of life 2026-10-02; requests to the slug are routed to… |

## typesafe

### TypeSafe AI

Typed decisions through `GaiseClient::decision` and `POST /v1/decision`.
Use `typesafe::jev`, mapped to TypeSafe's `jev-latest` (Jev 1.14 since the
2026-10-29 audit), `typesafe::jev-1.14.0` to pin Jev 1.14, or
`typesafe::jev-1.13.0` to pin Jev 1.13.
Supports Choice, Score, and Noul questions, and PNG/JPEG images through
`decision_with_images` (Dreamer-ready since 2026-10-14). Box questions are not
mapped yet. The API reports resolved model IDs, typed answers, and usage.

- Discovery: `GET /v1/models`; supports `operation=decision` in GAISe.
- Documentation: [TypeSafe models](https://docs.typesafe.ai/models).
- [Configuration and mappings](../gaise-provider-typesafe/README.md).

## Lifecycle calendar

Published shutdown dates for entries that are not yet retired, soonest first. Treat them as the **earliest** possible date; providers may extend but not advance them.

| Date | Provider | Model | Replacement |
|---|---|---|---|
| 2026-10-20 | [vertexai](#vertexai) | `gemini-2.5-flash` | gemini-3.8-flash or gemini-3.5-flash-lite or gemini-3.1-flash-lite |
| 2026-10-20 | [vertexai](#vertexai) | `gemini-2.5-flash-lite` | gemini-3.8-flash or gemini-3.1-flash-lite or Gemma 4 |
| 2026-10-20 | [vertexai](#vertexai) | `gemini-2.5-pro` | gemini-3.8-flash or gemini-3.5-flash |
| 2026-11-02 | [xai](#xai) | `grok-imagine-image-quality` | grok-imagine-image-2.0 |
| 2026-11-17 | [gemini](#gemini) | `gemini-2.5-flash-native-audio-preview-12-2025` | gemini-3.8-live |
| 2026-11-17 | [gemini](#gemini) | `gemini-3.1-flash-live-preview` | gemini-3.8-live |
| 2026-11-17 | [gemini](#gemini) | `gemini-3.1-flash-tts-preview` | gemini-3.8-flash-tts or gemini-3.8-flash-lite-tts |
| 2026-11-19 | [vertexai](#vertexai) | `gemini-3.6-flash` | gemini-3.8-flash |
| 2026-11-26 | [bedrock](#bedrock) | `ai21.jamba-1-5-*` | — |
| 2026-11-30 | [anthropic](#anthropic) | `claude-sonnet-4-5-20250929` | claude-sonnet-5-5 |
| 2026-11-30 | [bedrock](#bedrock) | `twelvelabs.marengo-embed-2-7-v1:0` | twelvelabs.marengo-embed-3-0-v1:0 |
| 2026-12-01 | [openai](#openai) | `gpt-image-1.5` | gpt-image-2.5-sunburst or gpt-image-2.5-flare |
| 2026-12-11 | [openai](#openai) | `gpt-5-2025-08-07` | gpt-5.6-sol |
| 2026-12-11 | [openai](#openai) | `gpt-5-mini-2025-08-07` | gpt-5.6-terra |
| 2026-12-11 | [openai](#openai) | `gpt-5-nano-2025-08-07` | gpt-5.6-luna |
| 2026-12-11 | [openai](#openai) | `gpt-5-pro-2025-10-06` | gpt-5.6-sol (reasoning.mode: pro) |
| 2026-12-11 | [openai](#openai) | `o3-2025-04-16` | gpt-5.6-sol |
| 2026-12-11 | [openai](#openai) | `o3-pro-2025-06-10` | gpt-5.6-sol (reasoning.mode: pro) |
| 2026-12-13 | [vertexai](#vertexai) | `gemini-live-2.5-flash-native-audio` | — |
| 2027-01-08 | [bedrock](#bedrock) | `anthropic.claude-opus-4-1-20250805-v1:0` | anthropic.claude-opus-4-8 |
| 2027-01-14 | [openai](#openai) | `gpt-4.1` | gpt-6-luna |
| 2027-01-14 | [openai](#openai) | `gpt-4.1-mini` | gpt-6-luna |
| 2027-01-14 | [openai](#openai) | `gpt-4o` | gpt-6-luna |
| 2027-01-14 | [openai](#openai) | `gpt-4o-mini` | gpt-6-luna |
| 2027-01-20 | [openai](#openai) | `gpt-audio` | gpt-audio-1.5 |
| 2027-01-20 | [openai](#openai) | `gpt-realtime` | gpt-realtime-2.1 or gpt-realtime-2.1-mini |
| 2027-01-28 | [vertexai](#vertexai) | `gemini-3.7-flash` | gemini-3.8-flash |
| 2027-02-26 | [openai](#openai) | `whisper-1` | gpt-transcribe or gpt-live-transcribe |
| 2027-03-15 | [gemini](#gemini) | `gemini-2.5-flash-image` | gemini-3.1-flash-lite-image |
| 2027-03-15 | [vertexai](#vertexai) | `gemini-2.5-flash-image` | gemini-3.1-flash-lite-image |
| 2027-03-30 | [bedrock](#bedrock) | `meta.llama4-maverick-17b-instruct-v1:0` | — |
| 2027-04-01 | [openai](#openai) | `gpt-5.1` | gpt-6-sol |
| 2027-04-01 | [openai](#openai) | `gpt-5.4-nano` | gpt-6-luna |
| 2027-04-08 | [bedrock](#bedrock) | `anthropic.claude-sonnet-4-5-20250929-v1:0` | — |
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
