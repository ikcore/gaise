# gaise

[![crates.io](https://img.shields.io/crates/v/gaise.svg)](https://crates.io/crates/gaise)
[![docs.rs](https://docs.rs/gaise/badge.svg)](https://docs.rs/gaise)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/License-MIT%20OR%20Apache--2.0-blue.svg)](https://github.com/ikcore/gaise#license)

Core trait and contracts for **GAISe** (Generative AI Service) — a unified Rust abstraction across GenAI providers.

GAISe provides common contracts for OpenAI, Anthropic, Gemini, Vertex AI, Bedrock, Ollama, ElevenLabs, and TypeSafe AI. Route supported operations using `provider::model`; use `typesafe::jev` for typed System One decisions.

## The `GaiseClient` Trait

```rust
#[async_trait]
pub trait GaiseClient: Send + Sync {
    async fn system_one(&self, request: &GaiseSystemOneRequest)
        -> Result<GaiseSystemOneResponse, Box<dyn Error + Send + Sync>>;

    async fn instruct(&self, request: &GaiseInstructRequest)
        -> Result<GaiseInstructResponse, Box<dyn Error + Send + Sync>>;

    async fn instruct_stream(&self, request: &GaiseInstructRequest)
        -> Result<Pin<Box<dyn Stream<Item = Result<GaiseInstructStreamResponse, ...>> + Send>>, ...>;

    async fn embeddings(&self, request: &GaiseEmbeddingsRequest)
        -> Result<GaiseEmbeddingsResponse, Box<dyn Error + Send + Sync>>;

    // Default implementation reports "not supported" so custom clients keep compiling.
    async fn list_models(&self, request: &GaiseListModelsRequest)
        -> Result<GaiseListModelsResponse, Box<dyn Error + Send + Sync>>;
}
```

Every provider crate implements this trait. Your application depends on `gaise` for the contracts and picks whichever provider crates it needs.

### Model discovery and the bundled registry

`list_models` returns `GaiseModel` records: routable id, lifecycle status and dates, token limits, and a `GaiseModelCapabilities` block with input/output modalities, the GAISe operations that can drive the model (`instruct`, `instruct_stream`, `embeddings`, `speech`, `live`, `system_one`), tri-state `tools` / `reasoning` / `structured_output` flags, and a `sources` list saying whether each claim came from the provider API, the registry, or a name heuristic. Provider model APIs differ widely in what they report, so `unknown` is a first-class answer.

`gaise_core::registry` compiles `model-registry.toml` into the crate and exposes `ModelRegistry::bundled()`, `find(provider, id)` (exact, alias, `*` glob, dated-snapshot, and Bedrock profile-prefix matching), and `enrich(&mut GaiseModel)`, which fills in what a provider left unknown without overriding provider facts.

### `GaiseLiveClient` Trait (real-time sessions)

```rust
#[async_trait]
pub trait GaiseLiveClient: Send + Sync {
    async fn live_connect(&self, config: &GaiseLiveConfig)
        -> Result<GaiseLiveSession, Box<dyn Error + Send + Sync>>;
}
```

Returns a `GaiseLiveSession` with a `tx` channel (send audio/text/tool responses) and an `rx` stream (receive audio/text/transcripts/tool calls). Implemented by `gaise-provider-openai` and `gaise-provider-gemini` when their `live` feature is enabled.

## Key Types

| Type | Purpose |
|------|---------|
| `GaiseInstructRequest` | Input: model, messages, tools, generation config |
| `GaiseInstructResponse` | Output: messages, usage |
| `GaiseInstructStreamResponse` | Streaming chunk: text delta, tool call delta, or usage |
| `GaiseEmbeddingsRequest/Response` | Embedding vectors |
| `GaiseSystemOneRequest/Response` | Shared state, named questions, typed answers, and usage |
| `GaiseQuestion` / `GaiseAnswer` | `Noul`, `Choice`, and `Score` variants |
| `GaiseNoulCriteria` | Optional descriptions for yes and no outcomes |
| `GaiseContent` | Enum: `Text`, `Image`, `Audio`, `File`, `Parts` |
| `GaiseMessage` | Role + content + optional tool calls |
| `GaiseGenerationConfig` | Temperature, max_tokens, thinking_effort, thinking_tokens |
| `GaiseTool` | Function calling definition with JSON Schema params |
| `OneOrMany<T>` | Flexible single-or-array wrapper |
| `GaiseStreamAccumulator` | Collects stream chunks into a complete message |
| `GaiseLiveConfig` | Live session config: model, voice, modalities, VAD, transcription, tools |
| `GaiseLiveSession` | Active session: `tx` (send inputs) + `rx` (receive events) |
| `GaiseLiveEvent` | Server event: `Audio`, `Text`, `Transcript`, `ToolCall`, `TurnComplete`, etc. |
| `GaiseLiveInput` | Client input: `Audio`, `Text`, `ToolResponse`, `Close` |

## Quick Start

```rust
use gaise_core::GaiseClient;
use gaise_core::contracts::*;

let request = GaiseInstructRequest {
    model: "my-model".to_string(),
    input: OneOrMany::One(GaiseMessage {
        role: "user".to_string(),
        content: Some(OneOrMany::One(GaiseContent::Text {
            text: "Hello!".to_string(),
        })),
        ..Default::default()
    }),
    ..Default::default()
};

// Pass `request` to any GaiseClient implementation
let response = client.instruct(&request).await?;
```

## Provider Crates

| Crate | Provider |
|-------|----------|
| [`gaise-provider-openai`](https://crates.io/crates/gaise-provider-openai) | OpenAI (GPT-6 Astra, GPT-5.6, GPT-5.5, Realtime 2.1, text-embedding-3) |
| [`gaise-provider-anthropic`](https://crates.io/crates/gaise-provider-anthropic) | Anthropic (Claude Fable 5.1, Opus 5, Sonnet 5, Haiku 4.5; adaptive and extended thinking) |
| [`gaise-provider-gemini`](https://crates.io/crates/gaise-provider-gemini) | Google Gemini (Gemini 3.8 Flash and the 3.x line, Live, gemini-embedding-2) |
| [`gaise-provider-vertexai`](https://crates.io/crates/gaise-provider-vertexai) | Google Vertex AI |
| [`gaise-provider-bedrock`](https://crates.io/crates/gaise-provider-bedrock) | AWS Bedrock (Claude, Nova, GPT-6 Astra and GPT-5.6, third-party Converse families) |
| [`gaise-provider-ollama`](https://crates.io/crates/gaise-provider-ollama) | Ollama (local and cloud tags: Qwen 3.8, GPT-OSS, Gemma 4, GLM 5.3, Granite 4.2) |
| [`gaise-provider-elevenlabs`](https://crates.io/crates/gaise-provider-elevenlabs) | ElevenLabs text-to-speech and realtime voice |
| [`gaise-provider-typesafe`](https://crates.io/crates/gaise-provider-typesafe) | TypeSafe Jev typed decisions (`typesafe::jev`, System One) |
| [`gaise-client`](https://crates.io/crates/gaise-client) | Router — `"provider::model"` string routing |
| [`gaise-api`](https://crates.io/crates/gaise-api) | Axum HTTP server with SSE streaming |

## System One and TypeSafe Jev

System One evaluates questions about shared context and returns typed decisions.
Use it for tasks such as ticket routing, urgency detection, eligibility checks,
and rubric scoring. GAISe currently provides this operation through TypeSafe AI's
Jev model, selected as **`typesafe::jev`**.

The router removes `typesafe::`; the adapter maps `jev` to TypeSafe's `jev-latest`
and sends `POST https://api.typesafe.ai/v1/systemone`. The response keeps the
provider-reported model ID so you can record which version answered. Model listing
maps the upstream `jev-latest` alias back to `typesafe::jev`.

Each request contains a shared `state` and a map of named `questions`. Answers use
those same names. Questions are evaluated independently against the state; use
application code to combine their results or to decide when a threshold is met.

| Question type | Criteria | Answer |
| --- | --- | --- |
| `noul` | Optional `true` / `false` descriptions | `noul`: probability from 0 to 1; GAISe does not convert it to a Boolean |
| `choice` | Object mapping option names to descriptions (or `null`) | Selected `choice`, probability per option, and `confidence` |
| `score` | Ordered array with at least two level descriptions | Fractional expected `score`, probability per level, `legend`, and `confidence` |

Scores use zero-based rubric positions. For example, a three-level rubric has
positions 0, 1, and 2; a returned score of 1.2 falls between the middle and highest
levels. Confidence is a provider-reported measure derived from the distribution;
GAISe preserves it without recalculating it or treating it as a correctness guarantee.

### HTTP: `POST /v1/systemone`

Configure the GAISe server with `TYPESAFE_API_KEY`, then run `cargo run -p gaise-api`
from the repository.
Save this request as `request.json`:

```json
{
  "model": "typesafe::jev",
  "state": {
    "ticket": "I was charged twice. Please fix this today."
  },
  "questions": {
    "urgent": {
      "type": "noul",
      "instructions": "Does this ticket need urgent attention?"
    },
    "team": {
      "type": "choice",
      "instructions": "Which team should handle this ticket?",
      "criteria": {
        "billing": "Payments, invoices, or refunds",
        "technical": "Bugs or integration problems"
      }
    },
    "severity": {
      "type": "score",
      "instructions": "How severe is the customer impact?",
      "criteria": [
        "Low: minor inconvenience",
        "Medium: a problem needing attention",
        "High: service cannot be used"
      ]
    }
  }
}
```

```bash
curl http://localhost:3000/v1/systemone \
  -H "Content-Type: application/json" \
  --data-binary @request.json
```

GAISe applies the configured TypeSafe key to the upstream Bearer authorization
header. The local request uses the GAISe model name and request shape.

Illustrative response (values are examples, not a live prediction):

```json
{
  "model": "jev-1.13.0",
  "answers": {
    "urgent": {
      "type": "noul",
      "noul": 0.92
    },
    "team": {
      "type": "choice",
      "choice": "billing",
      "probabilities": {
        "billing": 0.95,
        "technical": 0.05
      },
      "confidence": 0.71
    },
    "severity": {
      "type": "score",
      "score": 1.2,
      "legend": {
        "0": "Low: minor inconvenience",
        "1": "Medium: a problem needing attention",
        "2": "High: service cannot be used"
      },
      "probabilities": {
        "0": 0.1,
        "1": 0.6,
        "2": 0.3
      },
      "confidence": 0.18
    }
  },
  "usage": {
    "input": {
      "input_tokens": 123
    },
    "output": {
      "output_tokens": 0
    }
  }
}
```

Upstream `usage.input_tokens` maps to `usage.input.input_tokens`, and
`usage.output_tokens` maps to `usage.output.output_tokens`. Zero counts remain
zero. GAISe does not invent a `total` counter. Scores, probabilities, confidence,
and score legends remain typed JSON values.

### Configuration

| Setting | Purpose |
| --- | --- |
| `GaiseClientConfig.typesafe_api_key` | TypeSafe API key for library callers |
| `GaiseClientConfig.typesafe_api_url` | Optional API root; default `https://api.typesafe.ai`, without `/v1` |
| `TYPESAFE_API_KEY` | API key read by the `gaise-api` executable |
| `TYPESAFE_API_URL` | API root read by the executable; takes precedence over `TYPESAFE_BASE_URL` |
| `connection.api_key` / `connection.api_url` | Optional per-request overrides; each supplied field wins over service configuration |
| `correlation_id` | Optional identifier used by GAISe request/response logging |

Library callers supply configuration explicitly; environment variables are loaded
by the server executable. The `typesafe` feature is enabled by default in
`gaise-client`; use `default-features = false, features = ["typesafe"]` to select
just this provider. Connection and correlation metadata are not sent in the
upstream JSON body. GAISe redacts connection credentials before request logging.

### Discovery and supported operations

```text
GET /v1/models?provider=typesafe&operation=system_one
GET /v1/models/typesafe::jev
GET /v1/models/limits?provider=typesafe&operation=system_one
```

The first two routes query the provider catalog. The limits route reads GAISe's
bundled registry and needs no provider credentials. `GaiseOperation::SystemOne`
is serialized as `system_one`; `system_one` is also the Rust method name, while
the HTTP path uses `/v1/systemone`.

Jev uses this typed decision operation. Text generation (`instruct`), SSE
streaming, embeddings, tool calling, and live audio are not implemented for this
provider. Give it text or structured context instead of image/audio attachments.

### Validation and errors

GAISe validates a nonempty model and question map, nonempty Choice criteria, and
at least two Score levels. It accepts strings, objects, arrays, or `null` for
state and instruction/criterion descriptions, following the official SDK; useful
context and explicit instructions are recommended. The provider can apply
additional validation. Answer IDs and types must match the submitted questions.

The HTTP endpoint returns `400` for GAISe request validation failures. Axum rejects
malformed JSON or incompatible field types before the handler. Provider,
configuration, and transport failures currently return `500` with an error
message; upstream status codes are included in provider error messages, not
forwarded as the GAISe HTTP status.

Each upstream attempt has a 30-second timeout. HTTP 429 and 5xx responses,
including 529, are retried twice using 500/1000ms backoff, or a numeric
`Retry-After` delay capped at 60 seconds. Transport errors and other HTTP statuses
are returned without retrying. There is no System One streaming endpoint.

### Rust example

```toml
[dependencies]
gaise-core = { package = "gaise", version = "3.0.1" }
gaise-client = { version = "3.0.1", default-features = false, features = ["typesafe"] }
serde_json = "1"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

```rust
use gaise_client::{GaiseClientConfig, GaiseClientService};
use gaise_core::{
    GaiseClient,
    contracts::{GaiseAnswer, GaiseSystemOneRequest},
};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let client = GaiseClientService::new(GaiseClientConfig {
        typesafe_api_key: Some(std::env::var("TYPESAFE_API_KEY")?),
        ..Default::default()
    });
    let request: GaiseSystemOneRequest = serde_json::from_value(json!({
        "model": "typesafe::jev",
        "state": {"ticket": "I was charged twice. Please fix this today."},
        "questions": {
            "urgent": {"type": "noul", "instructions": "Is this urgent?"}
        }
    }))?;
    let response = client.system_one(&request).await?;
    if let Some(GaiseAnswer::Noul { noul }) = response.answers.get("urgent") {
        println!("Urgency probability: {noul}");
    }
    Ok(())
}
```

The same response map can contain `GaiseAnswer::Choice` and `GaiseAnswer::Score`.
The [TypeSafe provider crate](https://crates.io/crates/gaise-provider-typesafe)
also exposes `GaiseClientTypeSafe::new(api_url, api_key)` for direct use with the
bare model name `jev`.

References: [TypeSafe API](https://docs.typesafe.ai/api),
[question types](https://docs.typesafe.ai/introduction), and the
[official SDK contracts](https://github.com/typesafe-ai/typesafe-sdk-js/blob/main/src/types.ts).

## Reasoning / Thinking

GAISe standardises reasoning across providers via `GaiseGenerationConfig`:

```rust
generation_config: Some(GaiseGenerationConfig {
    thinking_effort: Some("high".to_string()),  // low, medium, high
    thinking_tokens: Some(10000),                // explicit budget (Anthropic, Gemini 2.5)
    max_tokens: Some(32000),
    ..Default::default()
}),
```

| GAISe field | OpenAI | Anthropic | Gemini |
|---|---|---|---|
| `thinking_effort` | `reasoning_effort` | `thinking.type` | `thinkingConfig.thinkingLevel` |
| `thinking_tokens` | N/A | `thinking.budget_tokens` | `thinkingConfig.thinkingBudget` |
| `max_tokens` | `max_completion_tokens` | `max_tokens` | `maxOutputTokens` |

## License

MIT OR Apache-2.0, at your option (see `LICENSE-APACHE` and `LICENSE-MIT`, bundled in the crate and in the repository). Versions 0.2.2 and earlier remain AGPL-3.0-only on crates.io.
