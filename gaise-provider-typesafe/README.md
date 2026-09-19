# GAISe TypeSafe AI provider

TypeSafe's System One API, exposed as `GaiseClient::system_one`. Questions and
answers use GAISe core contracts. The schema follows TypeSafe's `state` / `questions` convention.

```toml
[dependencies]
gaise-core = { package = "gaise", version = "3.0.0" }
gaise-client = { version = "3.0.0", default-features = false, features = ["typesafe"] }
serde_json = "1"
```

```rust,no_run
use gaise_client::{GaiseClientConfig, GaiseClientService};
use gaise_core::{GaiseClient, contracts::GaiseSystemOneRequest};
use serde_json::json;

# async fn example() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
let service = GaiseClientService::new(GaiseClientConfig {
    typesafe_api_key: Some(std::env::var("TYPESAFE_API_KEY")?),
    ..Default::default()
});
let request: GaiseSystemOneRequest = serde_json::from_value(json!({
    "model": "typesafe::jev",
    "state": {"ticket": "I was charged twice. Please fix this today."},
    "questions": {
        "urgent": {"type": "noul", "instructions": "Is this urgent?"},
        "team": {"type": "choice", "instructions": "Which team should help?",
                 "criteria": {"billing": null, "technical": null}},
        "severity": {"type": "score", "instructions": "How severe is this?",
                     "criteria": ["Low", "Medium", "High"]}
    }
}))?;
let response = service.system_one(&request).await?;
println!("{:?}", response.answers);
# Ok(()) }
```

## Configuration

The router follows the existing resolution order: per-request `connection.api_url`
and `connection.api_key`, then `GaiseClientConfig.typesafe_api_url` and
`typesafe_api_key`, then the default root URL `https://api.typesafe.ai`. The key
is required. URLs are API roots, **without `/v1`**; trailing slashes are removed.

`gaise-api` reads `TYPESAFE_API_KEY` and `TYPESAFE_API_URL` (also accepting the
official SDK's `TYPESAFE_BASE_URL` as a fallback). As with other providers, library
callers supply config explicitly. Each request may carry `correlation_id` and
`connection`; these remain inside GAISe and credentials are redacted from logs.

The direct adapter is `GaiseClientTypeSafe::new(api_url, api_key)` and accepts bare
model names such as `jev` (mapped to TypeSafe's `jev-latest`). The router uses `typesafe::jev` and strips
the prefix; the adapter translates `jev` to `jev-latest`. Returned `model` is the provider-reported identifier.

## Mapping and limits

| TypeSafe | GAISe |
| --- | --- |
| `POST /v1/systemone` | `GaiseClient::system_one`, HTTP `POST /v1/systemone` |
| `state`, named `questions` | `GaiseSystemOneRequest`, `GaiseQuestion` |
| `choice`, `score`, `noul` answers | `GaiseAnswer` variants; IDs, probabilities, confidence and score legend preserved |
| `usage.input_tokens` | `usage.input.input_tokens` |
| `usage.output_tokens` | `usage.output.output_tokens` (including zero) |
| `GET /v1/models` | Gaise model listing, routable IDs, `system_one` operation filter |

Scores remain fractional. Noul remains a probability, with no threshold or
invented confidence. No total usage is synthesized. The adapter rejects
response IDs/types that do not match the questions. Chat, streaming, embeddings,
and live generation are unsupported; use the typed decision operation.

Requests have a 30-second timeout per attempt. HTTP 429 and 5xx (including 529)
are retried twice with 500/1000ms backoff or numeric `Retry-After` capped at 60s.
Other HTTP failures return status and provider detail; credentials are redacted.

This integration supports TypeSafe's hosted Jev model. Model listing maps the
upstream `jev-latest` alias to the routable `typesafe::jev` ID.

Sources checked 2026-09-19: [API reference](https://docs.typesafe.ai/api),
[official SDK types](https://github.com/typesafe-ai/typesafe-sdk-js/blob/main/src/types.ts),
[model listing](https://github.com/typesafe-ai/typesafe-sdk-js/blob/main/src/resources/models.ts).
