# Models and lifecycle

This page records the conclusions of the 2026-07-22 documentation audit. It is guidance, not a runtime allowlist: GAISe accepts arbitrary model IDs so new releases can work before the registry is updated.

Always confirm account access, region, endpoint, and provider documentation before deployment. [`model-registry.toml`](../../model-registry.toml) is the machine-readable source in this repository.

## OpenAI

Current families relevant to the implemented adapters include:

- GPT-5.6 (`gpt-5.6` / `gpt-5.6-sol`, Terra, and Luna) for current text/image-input/reasoning work.
- GPT-5.5 and GPT-5.4 families as model-specific alternatives. GPT-5.5 Pro is non-streaming.
- `gpt-realtime-2.1` and `gpt-realtime-2.1-mini` for text/audio/image-input realtime sessions with tools and configurable reasoning.
- `gpt-audio-1.5` for audio-capable API workflows. The current Chat adapter maps audio input but not native audio output.
- `text-embedding-3-large` and `text-embedding-3-small` for the implemented embeddings path.
- `gpt-image-2` is current, but requires an Images/Responses adapter that this repository does not yet implement.

Notable announced migrations include:

| Retiring family | Date | Replacement guidance |
|---|---|---|
| `gpt-5-chat-latest`, GPT-5.1 chat/Codex aliases | 2026-07-23 | GPT-5.5/current GPT-5 family |
| `gpt-5.2-chat-latest`, `gpt-5.3-chat-latest` | 2026-08-10 | GPT-5.5 |
| Selected GPT-4/o-series and `o4-mini` snapshots | 2026-10-23 | Current GPT-5 equivalents |
| Original GPT-5 and o3 snapshots | 2026-12-11 | Current GPT-5 family |
| `gpt-image-1.5`, mini, and ChatGPT image alias | 2026-12-01 | `gpt-image-2` |
| Legacy Realtime/audio families | 2027-01-20 | Realtime 2.1 / audio 1.5 |

Official sources: [model catalog](https://developers.openai.com/api/docs/models), [latest-model guidance](https://developers.openai.com/api/docs/guides/latest-model), [deprecations](https://developers.openai.com/api/docs/deprecations), [GPT-5.5 Pro](https://developers.openai.com/api/docs/models/gpt-5.5-pro), and [GPT-Realtime-2.1](https://developers.openai.com/api/docs/models/gpt-realtime-2.1).

## Anthropic

The audited first-party lineup includes Claude Fable 5, Opus 4.8, Sonnet 5, and Haiku 4.5, with Mythos 5 limited availability. Model families differ in thinking controls:

- Current Fable/Mythos/Opus/Sonnet families can require or prefer adaptive thinking.
- Some 4.6 models accept adaptive or manual-budget thinking.
- Older models use manual `budget_tokens` or no thinking control.

The adapter applies effort/display controls and removes incompatible sampling settings by family.

Lifecycle highlights:

- Claude Opus 4.1 is deprecated and retires 2026-08-05 on the direct Claude API.
- Claude Opus 4 and Sonnet 4 retired 2026-06-15.
- Mythos Preview retired 2026-07-21 in favor of Mythos 5.
- AWS-hosted Claude lifecycle is separate and must be read from Bedrock.

Official sources: [model overview](https://platform.claude.com/docs/en/about-claude/models/overview), [model deprecations](https://platform.claude.com/docs/en/about-claude/model-deprecations), [extended thinking](https://platform.claude.com/docs/en/build-with-claude/extended-thinking), and [effort](https://platform.claude.com/docs/en/build-with-claude/effort).

## Gemini API

Audited stable/current families include Gemini 3.6 Flash, 3.5 Flash, 3.5 Flash-Lite, 3.1 Flash-Lite, Gemini 3.1 Flash Image, Gemini 3 Pro Image, and Gemini Embedding 2. Gemini 3.1 Pro remains preview.

For Live, use `gemini-3.1-flash-live-preview`. The former `gemini-2.0-flash-live-001` and `gemini-live-2.5-flash-preview` shut down on 2025-12-09. The 2.5 native-audio preview remains listed without an announced shutdown date but has 3.1 Flash Live as its recommended replacement.

Important scheduled removals:

| Model/family | Date | Replacement |
|---|---|---|
| Gemini 2.5 Pro / Flash / Flash-Lite | 2026-10-16 | Gemini 3.x family |
| Gemini 2.5 Flash Image | 2026-10-02 | Gemini 3.1 Flash Image |
| `embedding-2-preview` | 2026-08-10 | `gemini-embedding-2` |
| `gemini-embedding-001` | Retired 2026-07-14 | `gemini-embedding-2` |

Official sources: [model catalog](https://ai.google.dev/gemini-api/docs/models), [deprecation schedule](https://ai.google.dev/gemini-api/docs/deprecations), [changelog](https://ai.google.dev/gemini-api/docs/changelog), and [Live API reference](https://ai.google.dev/api/live).

## Vertex AI

Do not copy Gemini API dates into Vertex AI. Google Cloud has separate availability and retirement guarantees. At the audit date:

- Vertex `gemini-embedding-001` is available until no sooner than 2028-05-20 even though the Gemini API ID has retired.
- Vertex Gemini 2.5 text-family retirement is listed as 2026-10-16.
- Vertex Gemini 2.5 Flash Image retirement is listed as 2026-10-02.
- `gemini-3.6-flash` is short-term availability with no fixed retirement date announced; its policy is tied to replacement availability.

Official sources: [Vertex model lifecycle](https://docs.cloud.google.com/gemini-enterprise-agent-platform/models/model-versions), [Vertex model catalog](https://docs.cloud.google.com/vertex-ai/generative-ai/docs/learn/models), and [GenerationConfig](https://cloud.google.com/vertex-ai/generative-ai/docs/reference/rest/v1beta1/GenerationConfig).

## Amazon Bedrock

Bedrock model availability is region-, inference-profile-, and platform-specific. Direct vendor dates do not automatically apply. Use `ListFoundationModels` and `GetFoundationModel`, then confirm the exact model or cross-region inference profile in the deployment region.

The registry records representative current Claude, Nova, Titan embedding, and Cohere embedding families plus lifecycle-sensitive examples. The standard Bedrock Runtime Converse/Invoke surfaces are the implemented paths; optional alternative endpoints do not change model availability automatically.

Official sources: [supported models](https://docs.aws.amazon.com/bedrock/latest/userguide/models.html), [model lifecycle](https://docs.aws.amazon.com/bedrock/latest/userguide/model-lifecycle.html), [Converse](https://docs.aws.amazon.com/bedrock/latest/userguide/conversation-inference.html), and [TokenUsage](https://docs.aws.amazon.com/bedrock/latest/APIReference/API_runtime_TokenUsage.html).

## Ollama

Ollama tags are dynamic and local. Discover them from `GET /api/tags` or the local CLI and inspect the model template/capabilities. The registry describes useful families such as Qwen 3 and GPT-OSS but intentionally does not assign centralized retirement dates.

## Updating the registry

1. Record an `audited_on` date.
2. Use official provider documentation, not aggregators or a different host's lifecycle page.
3. Separate status (`active`, `preview`, `deprecated`, `retired`) from GAISe adapter support.
4. Record an exact shutdown date only when the provider publishes one.
5. Keep Gemini API and Vertex AI entries separate.
6. For Bedrock, include region/profile caveats; for Ollama, keep discovery dynamic.
7. Add a hermetic request-shape test when a model requires a new control mapping.
8. Never validate lifecycle by making a billable inference request.

