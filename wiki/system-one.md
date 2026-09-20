# System One: TypeSafe Jev

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
