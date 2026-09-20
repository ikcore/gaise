# gaise-api

[![crates.io](https://img.shields.io/crates/v/gaise-api.svg)](https://crates.io/crates/gaise-api)
[![docs.rs](https://docs.rs/gaise-api/badge.svg)](https://docs.rs/gaise-api)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/License-MIT%20OR%20Apache--2.0-blue.svg)](https://github.com/ikcore/gaise#license)

Axum HTTP server for [GAISe](https://crates.io/crates/gaise) — exposes all GenAI providers behind a unified REST API with SSE streaming.

## Endpoints

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/v1/systemone` | Typed decisions with `typesafe::jev` |
| `POST` | `/v1/instruct` | Non-streaming instruct request |
| `POST` | `/v1/instruct/stream` | Server-Sent Events streaming |
| `POST` | `/v1/embeddings` | Generate embedding vectors |
| `GET` | `/v1/models` | List models across configured providers (`?provider=`, `?operation=`, `?include_details=`, `?include_raw=`) |
| `GET` | `/v1/models/{provider}::{id}` | One model record |
| `GET` | `/v1/live` | WebSocket for real-time audio/text sessions (feature = `live`) |

## Quick Start

```bash
# Set provider keys
export OPENAI_API_KEY="sk-..."
export ANTHROPIC_API_KEY="sk-ant-..."
export GEMINI_API_KEY="AIza..."

# Run the server
cargo run -p gaise-api
# Listening on 0.0.0.0:3000
```

```bash
# Call any provider via the same endpoint
curl -X POST http://localhost:3000/v1/instruct \
  -H "Content-Type: application/json" \
  -d '{
    "model": "openai::gpt-5.6-terra",
    "input": {
      "role": "user",
      "content": {"type": "text", "text": "Hello!"}
    }
  }'
```

Change `"model"` to `"anthropic::claude-sonnet-5"` or `"gemini::gemini-3.8-flash"` — same endpoint, same format.

### Live / Realtime (feature = "live")

Enable with `cargo run -p gaise-api --features live`. The `/v1/live` endpoint upgrades to a WebSocket for bidirectional audio + text streaming.

**Protocol:**
1. Client sends a JSON `GaiseLiveConfig` as the first message (model, voice, modalities, tools, etc.)
2. Server connects to the provider and begins forwarding:
   - **Client -> Server:** JSON text frames (`GaiseLiveInput`) or binary frames (raw PCM16 audio at 16kHz)
   - **Server -> Client:** JSON text frames (`GaiseLiveEvent`) or binary frames (PCM audio with 4-byte LE sample rate header)

## Environment Variables

| Variable | Description |
|----------|-------------|
| `GAISE_PORT` | Server port (default: `3000`) |
| `OLLAMA_URL` | Ollama API URL |
| `OPENAI_API_URL` / `OPENAI_API_KEY` | OpenAI credentials |
| `ANTHROPIC_API_URL` / `ANTHROPIC_API_KEY` | Anthropic credentials |
| `GEMINI_API_URL` / `GEMINI_API_KEY` | Gemini credentials |
| `VERTEXAI_API_URL` / `VERTEXAI_SA_PATH` | Vertex AI credentials |
| `BEDROCK_REGION` | AWS Bedrock region |
| `TYPESAFE_API_KEY` | TypeSafe API key |
| `TYPESAFE_API_URL` / `TYPESAFE_BASE_URL` | TypeSafe API root without `/v1`; defaults to `https://api.typesafe.ai` |

## As a Library

```rust
use std::sync::Arc;
use gaise_api::{create_app, AppState};
use gaise_client::{GaiseClientService, GaiseClientConfig};

let config = GaiseClientConfig { /* ... */ ..Default::default() };
let state = Arc::new(AppState {
    client_service: GaiseClientService::new(config),
});

let app = create_app(state);
// Mount into your own Axum server
```

## Part of [GAISe](https://github.com/ikcore/gaise)

License: MIT OR Apache-2.0, at your option (see `LICENSE-APACHE` and `LICENSE-MIT`). Versions 0.2.2 and earlier remain AGPL-3.0-only on crates.io.

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

References: [TypeSafe API](https://docs.typesafe.ai/api),
[question types](https://docs.typesafe.ai/introduction), and the
[official SDK contracts](https://github.com/typesafe-ai/typesafe-sdk-js/blob/main/src/types.ts).
