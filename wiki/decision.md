# Decision: TypeSafe Jev, OpenAI, and Ollama

## Decision and TypeSafe Jev

The Decision operation evaluates questions about shared context and returns typed
decisions.
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

### HTTP: `POST /v1/decision`

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
curl http://localhost:3000/v1/decision \
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
zero. GAISe does not invent a `total` counter; only OpenAI reports one (see
[OpenAI Decisions](#openai-decisions-gpt-6-luna)). Scores, probabilities, confidence,
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
GET /v1/models?provider=typesafe&operation=decision
GET /v1/models/typesafe::jev
GET /v1/models/limits?provider=typesafe&operation=decision
```

The first two routes query the provider catalog. The limits route reads GAISe's
bundled registry and needs no provider credentials. `GaiseOperation::Decision`
is serialized as `decision`; `decision` is also the Rust method name, and the
HTTP path is `/v1/decision`.

`POST /v1/systemone` is a supported alias of `POST /v1/decision`: it is the path
TypeSafe and Ollama use for this protocol, and both GAISe routes share one
handler. The request still names a routable `provider::model`.

Before 4.0 the operation itself was called System One. `operation=system_one`,
`GaiseClient::system_one`, `GaiseSystemOneRequest`, `GaiseSystemOneResponse`, and
`GaiseOperation::SystemOne` remain as deprecated aliases. Model listings report
the operation as `decision`.

Jev uses this typed decision operation. Text generation (`instruct`), SSE
streaming, embeddings, tool calling, and live audio are not implemented for this
provider. Give it text or structured context instead of image/audio attachments.

### Ollama decision models

Ollama 0.35 (2026-09-29) serves the same protocol locally at `POST /v1/systemone`.
GAISe routes `ollama::<tag>` through it with the request and response shapes shown
above; no API key is involved.

```bash
ollama pull nimble
curl http://localhost:3000/v1/decision \
  -H "Content-Type: application/json" \
  -d '{"model": "ollama::nimble", "state": "I was charged twice.", "questions": {"refund": {"type": "noul", "instructions": "Is a refund requested?"}}}'
```

| Tag | Model |
| --- | --- |
| `nimble` | Bespoke Labs, 9B |
| `tev1`, `tev1:0.8b` | Together AI, 4B and 0.8B (experimental) |
| `clef`, `clef-flash` | Cloudflare, 27B and 9B; accept images (Ollama 0.35.1+) |
| `laya` | Convai Innovations, 421M ModernBERT-large; MLX only (Ollama 0.40+) |

Ollama's limits differ from TypeSafe's: at most 64 questions and a 64 KiB body per
request, 2 to 26 choice options or score levels, `instructions` required on every
question, and each rendered prompt must fit the loaded context. It accepts local
models only and has no streaming or tool support on this endpoint. Only Clef and
Clef Flash accept images: GAISe sends them as raw base64 strings in `images` (no
data-URL prefix), and a request with images may be up to 32 MiB. GAISe
applies its own validation and leaves these limits to Ollama, whose error text is
returned with the status code. The model tag is sent unchanged (no `jev` mapping)
and `usage.output_tokens` can be non-zero. `GET /v1/models?provider=ollama&operation=decision`
lists installed decision tags.

### OpenAI Decisions (gpt-6-luna)

OpenAI's Decisions API (`POST /v1/decisions`) has been in public beta since
2026-10-06, and `gpt-6-luna` is the only model it accepts. GAISe routes
`openai::gpt-6-luna` decisions there with the `OPENAI_API_KEY` configured for
chat. It bills input tokens only ($0.10 per 1M at launch). OpenAI's shape differs
from System One, so the adapter
([`decisions.rs`](../gaise-provider-openai/src/decisions.rs)) translates it:

| GAISe | OpenAI | Notes |
| --- | --- | --- |
| `state` | `input` text | Strings pass through; objects and arrays are sent as compact JSON text; `null` is rejected |
| question id | `name` | Answers are matched back by name, not position |
| `noul` | `predicate` | `true` / `false` criteria are appended to the instructions as `True when: …` / `False when: …` lines |
| `choice` criteria | `choices` (`value`, `description`) | 2 to 255 options; a `null` description is omitted |
| `score` criteria | ordered `levels` (`label`, `description`) | A string becomes the label; a `{label, description}` object passes through; `null` becomes the index; anything else becomes JSON text |
| `instructions` | `instructions` | Required on every question; non-string values are sent as JSON text |

On the way back, `predicate.probability` becomes `noul`. Choice values become
strings (OpenAI's boolean values turn into `"true"` / `"false"`). Score
probabilities are keyed by level index (`"0"`, `"1"`, …), and the score `legend`
is rebuilt from the request criteria, so it matches TypeSafe's. If any question
is refused (`type: refusal`), the whole call fails with an error naming the
refused questions, because `GaiseAnswer` has no refusal variant. Usage keeps the
decision vocabulary and adds OpenAI's details:
`usage.input.{input_tokens, cached_tokens, cache_write_tokens}`,
`usage.output.{output_tokens, reasoning_tokens}`, and `usage.total.total_tokens`.

`safety_identifier` is not sent. Correlation and connection metadata never
leave GAISe.

### Images

`GaiseClient::decision_with_images(&request, &images)` adds images
(`GaiseContent::Image`) that every question can see. Over HTTP, add an `images`
array to the `POST /v1/decision` body:

```json
{
  "model": "openai::gpt-6-luna",
  "state": "Inspect the product in this photo.",
  "images": [{ "type": "image", "data": [137, 80, 78, 71], "format": "png" }],
  "questions": {
    "visible_damage": { "type": "noul", "instructions": "Does the product have visible damage?" }
  }
}
```

| Provider | How images are sent | Limit |
| --- | --- | --- |
| OpenAI (`gpt-6-luna`) | One user message: the state as `input_text`, then each image as an `input_image` base64 data URL | 128 images |
| Ollama (`clef`, `clef-flash`) | `images`: raw base64 strings | 32 MiB request body |
| TypeSafe and every other client | Rejected: "Image input for decisions is not supported by this client" | — |

An empty `images` array behaves exactly like `decision`. Only image content is
accepted. OpenAI's guide says images must be inline base64 data URLs, while its
API reference also allows public HTTPS URLs. `GaiseContent::Image` carries bytes,
so GAISe always sends data URLs.

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
are returned without retrying. There is no Decision streaming endpoint.

### Rust example

```toml
[dependencies]
gaise-core = { package = "gaise", version = "4.3.0" }
gaise-client = { version = "4.3.0", default-features = false, features = ["typesafe"] }
serde_json = "1"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

```rust
use gaise_client::{GaiseClientConfig, GaiseClientService};
use gaise_core::{
    GaiseClient,
    contracts::{GaiseAnswer, GaiseDecisionRequest},
};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let client = GaiseClientService::new(GaiseClientConfig {
        typesafe_api_key: Some(std::env::var("TYPESAFE_API_KEY")?),
        ..Default::default()
    });
    let request: GaiseDecisionRequest = serde_json::from_value(json!({
        "model": "typesafe::jev",
        "state": {"ticket": "I was charged twice. Please fix this today."},
        "questions": {
            "urgent": {"type": "noul", "instructions": "Is this urgent?"}
        }
    }))?;
    let response = client.decision(&request).await?;
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
