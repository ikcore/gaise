## Model Audit Report — 2026-04-03

### New Models Found

**OpenAI:**
- `openai::o3-mini` — Smaller/cheaper o3 variant. reasoning_effort: low/medium/high. Action: Add to registry.
- `openai::gpt-5.4-mini` — Released March 17, 2026. reasoning_effort: none/low/medium/high. Action: Add to registry.
- `openai::gpt-5.4-nano` — Released March 17, 2026. reasoning_effort: none/low/medium/high. Action: Add to registry.
- `openai::gpt-5.4-pro` — Responses API only. reasoning_effort: medium/high/xhigh. Action: Add to registry with note about API limitation.
- `openai::gpt-5.2-pro` — Responses API only. reasoning_effort: medium/high/xhigh. Action: Add to registry with note.
- `openai::gpt-5-pro` — Responses API only. reasoning_effort: high only. Action: Add to registry with note.
- `openai::o3-pro` — Responses API only. reasoning_effort: low/medium/high. Action: Add to registry with note.

**Anthropic:**
- `anthropic::claude-opus-4-5-20251101` — Legacy Opus 4.5. thinking: enabled (budget required). Retirement: Nov 24, 2026. Action: Add to registry.
- `anthropic::claude-opus-4-1-20250805` — Legacy Opus 4.1. thinking: enabled (budget required). Retirement: Aug 5, 2026. Action: Add to registry.
- `anthropic::claude-opus-4-20250514` — Legacy Opus 4.0. thinking: enabled (budget required). Retirement: May 14, 2026. Action: Add to registry.

**Gemini:**
- No new text generation models beyond what's already tracked. Live API / TTS / image models are out of scope for gaise-provider-gemini (text-only provider).

### Deprecated / Shutdown Models

- `gemini::gemini-2.0-flash` — Shuts down June 1, 2026 (59 days). Replace with gemini-2.5-flash. **Already marked deprecated in registry.**
- `gemini::gemini-2.5-pro` — Shuts down June 17, 2026 (75 days). Replace with gemini-3.1-pro-preview.
- `gemini::gemini-2.5-flash` — Shuts down June 17, 2026 (75 days). Replace with gemini-3-flash-preview.
- `gemini::gemini-2.5-flash-lite` — Shuts down July 22, 2026. Replace with gemini-3.1-flash-lite-preview.
- `gemini::gemini-embedding-001` — Shuts down July 14, 2026. Replace with gemini-embedding-2-preview.
- `openai::gpt-4o` / `gpt-4o-mini` — Retired from ChatGPT Feb 13, 2026. API still active but on deprecation path.
- `openai::gpt-4.1` / `gpt-4.1-mini` / `gpt-4.1-nano` — Retired from ChatGPT Feb 13, 2026. API still active.
- `anthropic::claude-opus-4-20250514` — Earliest retirement May 14, 2026 (41 days).

### Capability Changes

- `anthropic::claude-opus-4-6` — Now supports `"max"` effort level (Opus 4.6 only). Registry had ["low", "medium", "high"], should be ["low", "medium", "high", "max"]. Action: Update registry.
- `anthropic::claude-haiku-4-5-20251001` — Does NOT support adaptive thinking or the effort parameter. Only supports `thinking.type=enabled` with `budget_tokens` required. Registry incorrectly shows reasoning_values. Action: Fix registry entry.
- `anthropic` — Effort parameter moved to `output_config.effort` (new field). The `thinking.type=enabled` + `budget_tokens` approach still works but is deprecated on 4.6 models. Current GAISe Anthropic mapping uses `thinking.type=enabled` which remains functional.

### Breaking API Changes

- **Anthropic** — New `output_config.effort` field for controlling reasoning on 4.6 models. Current code at `gaise-provider-anthropic/src/anthropic_client.rs:163-173` maps `thinking_effort` to `thinking.type=enabled`. This still works but is deprecated for 4.6 models. No immediate breakage, but should migrate to `output_config.effort` eventually.
- **Anthropic** — New `thinking.display` field (`"summarized"` / `"omitted"`). Not currently exposed in GAISe contracts. No breakage — just a missing feature.
- **OpenAI** — Models with `reasoning_effort` support vary in accepted values. Sending unsupported values (e.g., `"xhigh"` to `o3`) returns 400. Current code passes through whatever the caller sets — no validation.

### Backward Compatibility Suggestions

1. **Anthropic thinking.type migration**: Keep `thinking.type=enabled` as the default mapping. Add `output_config.effort` as an alternative path for 4.6 models. Default: if `thinking_effort` is set and model contains "4-6", use `output_config.effort`; otherwise fall back to `thinking.type=enabled` + `budget_tokens`.

2. **OpenAI reasoning_effort validation**: The safest approach is to **not validate** in the provider — let the API return the error. This avoids hardcoding model lists in code. Document in CLAUDE.md which models accept which values.

3. **Anthropic "max" effort**: Map GAISe `thinking_effort: "max"` through to Anthropic. For OpenAI/Gemini which don't support "max", map it to their highest value ("high" for OpenAI, "HIGH" for Gemini). Default: pass through as-is.

### Registry Updates Needed

**Add (OpenAI):**
```toml
[[models]]
provider = "openai"
model = "o3-mini"
reasoning = true
reasoning_values = ["low", "medium", "high"]
reasoning_default = "medium"
max_output_field = "max_completion_tokens"
embeddings = false
streaming = true
tools = true
status = "ga"

[[models]]
provider = "openai"
model = "gpt-5.4-mini"
reasoning = true
reasoning_values = ["none", "low", "medium", "high"]
reasoning_default = "none"
max_output_field = "max_completion_tokens"
embeddings = false
streaming = true
tools = true
status = "ga"

[[models]]
provider = "openai"
model = "gpt-5.4-nano"
reasoning = true
reasoning_values = ["none", "low", "medium", "high"]
reasoning_default = "none"
max_output_field = "max_completion_tokens"
embeddings = false
streaming = true
tools = true
status = "ga"

[[models]]
provider = "openai"
model = "gpt-5.4-pro"
reasoning = true
reasoning_values = ["medium", "high", "xhigh"]
reasoning_default = "medium"
max_output_field = "max_completion_tokens"
embeddings = false
streaming = true
tools = true
status = "ga"
notes = "Responses API only. Not compatible with Chat Completions endpoint."

[[models]]
provider = "openai"
model = "gpt-5.2-pro"
reasoning = true
reasoning_values = ["medium", "high", "xhigh"]
reasoning_default = "medium"
max_output_field = "max_completion_tokens"
embeddings = false
streaming = true
tools = true
status = "ga"
notes = "Responses API only."

[[models]]
provider = "openai"
model = "gpt-5-pro"
reasoning = true
reasoning_values = ["high"]
reasoning_default = "high"
max_output_field = "max_completion_tokens"
embeddings = false
streaming = true
tools = true
status = "ga"
notes = "Responses API only. Only supports high effort."

[[models]]
provider = "openai"
model = "o3-pro"
reasoning = true
reasoning_values = ["low", "medium", "high"]
reasoning_default = "medium"
max_output_field = "max_completion_tokens"
embeddings = false
streaming = true
tools = true
status = "ga"
notes = "Responses API only."
```

**Add (Anthropic):**
```toml
[[models]]
provider = "anthropic"
model = "claude-opus-4-5-20251101"
reasoning = true
reasoning_values = ["low", "medium", "high"]
max_output_field = "max_tokens"
embeddings = false
streaming = true
tools = true
status = "ga"
shutdown_date = "2026-11-24"
notes = "Uses thinking.type=enabled with budget_tokens required."

[[models]]
provider = "anthropic"
model = "claude-opus-4-1-20250805"
reasoning = true
reasoning_values = ["low", "medium", "high"]
max_output_field = "max_tokens"
embeddings = false
streaming = true
tools = true
status = "ga"
shutdown_date = "2026-08-05"
notes = "Uses thinking.type=enabled with budget_tokens required."

[[models]]
provider = "anthropic"
model = "claude-opus-4-20250514"
reasoning = true
reasoning_values = ["low", "medium", "high"]
max_output_field = "max_tokens"
embeddings = false
streaming = true
tools = true
status = "ga"
shutdown_date = "2026-05-14"
notes = "Uses thinking.type=enabled with budget_tokens required. Retiring in 41 days."
```

**Modify (Anthropic):**
- `claude-opus-4-6`: Change `reasoning_values` to `["low", "medium", "high", "max"]`
- `claude-haiku-4-5-20251001`: Change `reasoning_values` to remove effort values, add note "No adaptive thinking. Uses thinking.type=enabled with budget_tokens required only."

**Modify (OpenAI):**
- `gpt-4o`: Add note about ChatGPT retirement
- `gpt-4.1` family: Add note about ChatGPT retirement
