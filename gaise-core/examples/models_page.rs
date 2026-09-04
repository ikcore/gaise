//! Generate `wiki/models.md` from the bundled model registry.
//!
//! Every table, the contents list, and the lifecycle calendar are derived from
//! `model-registry.toml`; the per-vendor introductions and the maintenance
//! procedure are the constants below. Nothing contacts a provider.
//!
//! ```powershell
//! cargo run -p gaise --example models_page > wiki/models.md
//! ```

use gaise_core::contracts::GaiseModelStatus;
use gaise_core::registry::{ModelRegistry, RegistryModel, RegistryProvider};

/// Notes longer than this many characters are cut with an ellipsis so the
/// tables stay readable; the registry keeps the full text.
const NOTE_LIMIT: usize = 109;

const PROVIDERS: [(&str, &str, &str); 7] = [
    ("openai", "OpenAI", OPENAI_INTRO),
    ("anthropic", "Anthropic", ANTHROPIC_INTRO),
    ("gemini", "Google Gemini API", GEMINI_INTRO),
    ("vertexai", "Google Vertex AI", VERTEXAI_INTRO),
    ("bedrock", "Amazon Bedrock", BEDROCK_INTRO),
    ("ollama", "Ollama", OLLAMA_INTRO),
    ("elevenlabs", "ElevenLabs", ELEVENLABS_INTRO),
];

const HEADER: &str = "# Models

> Part of the [GAISe wiki](README.md) · [Capabilities](capabilities.md) · [HTTP API](api.md#get-v1models) · [Rust SDK](sdk.md#model-discovery) · [Flows](flows.md#model-discovery) · Vendors: [OpenAI](vendor-openai.md) · [Anthropic](vendor-anthropic.md) · [Google Gemini API](vendor-gemini.md) · [Google Vertex AI](vendor-vertexai.md) · [Amazon Bedrock](vendor-bedrock.md) · [Ollama](vendor-ollama.md) · [ElevenLabs](vendor-elevenlabs.md)

This page is the human-readable view of [`gaise-core/model-registry.toml`](../gaise-core/model-registry.toml) (schema {schema}, audited **{audited}**), which is compiled into the `gaise` crate and applied as an overlay by [`list_models`](api.md#get-v1models). It is advisory: GAISe accepts arbitrary model IDs so new releases work before this file is updated, and the provider's own model API (see [Model discovery](capabilities.md#model-discovery)) is always the first source of truth.

This page is generated from the registry by [`cargo run -p gaise --example models_page`](../gaise-core/examples/models_page.rs); edit the registry (or that example's introductions), not this file. Columns:

- **Input / Output** — modalities classified from the entry's `capabilities` list by [`classify_capabilities`](../gaise-core/src/registry.rs).
- **Ops** — GAISe operations the entry maps to: `I` instruct, `S` instruct_stream, `E` embeddings, `V` speech (voice), `L` live. Empty means no GAISe surface drives the model (image generation, TTS, bidirectional audio).
- **Tools / Reasoning** — ✓ supported, ✗ not listed, and the `reasoning_values` the provider documents.
- **Dates** — `shutdown` is a published retirement date; `not before` is an availability guarantee. Gemini API and Vertex AI dates are **never** interchangeable.
- **Limits** — context windows, output ceilings, per-input token limits, and character budgets are not repeated here; see [limits.md](limits.md) for the generated model × limits matrix and `GET /v1/models/limits`.
";

const OPENAI_INTRO: &str = "Instruct uses **Chat Completions**; Responses-only models (GPT-5.5 Pro, gpt-5.6-cyber, the Daybreak models, image generation) are listed but cannot be driven, and GPT-6 function tools are refused because OpenAI serves them through Responses only. `GET /v1/models` reports identity only, so everything in the Input/Output/Ops columns is registry- or heuristic-sourced at runtime ([`catalog.rs`](../gaise-provider-openai/src/contracts/catalog.rs)).";

const ANTHROPIC_INTRO: &str = "The Messages API. Anthropic's `GET /v1/models` reports image/PDF input, thinking types, effort levels, structured outputs, and token limits, so at runtime the registry contributes only lifecycle dates and notes ([`catalog.rs`](../gaise-provider-anthropic/src/contracts/catalog.rs)). Thinking and sampling rules by family are enforced in [`anthropic_client.rs`](../gaise-provider-anthropic/src/anthropic_client.rs). `legacy` mirrors Anthropic's own \"Legacy\" label (still served, no retirement date announced); the Models API keeps reporting those ids as active.";

const GEMINI_INTRO: &str = "Google AI Gemini API lifecycle only — see [Vertex AI](#vertexai) for Google Cloud. `models.list` reports `supportedGenerationMethods` (→ Ops), `thinking`, and token limits but no modalities ([`catalog.rs`](../gaise-provider-gemini/src/contracts/catalog.rs)). Gemini 2.5 uses `thinkingBudget`, 3.x uses `thinkingLevel`.";

const VERTEXAI_INTRO: &str = "Google Cloud lifecycle only. Model Garden listing returns names, versions, and launch stages — no modalities or limits ([`catalog.rs`](../gaise-provider-vertexai/src/contracts/catalog.rs)). Short-term-availability models retire 45 days after a designated replacement ships.";

const BEDROCK_INTRO: &str = "Model IDs, inference profiles, and lifecycle are **region-specific**; entries are representative and `ListFoundationModels` / `ListInferenceProfiles` are authoritative ([`catalog.rs`](../gaise-provider-bedrock/src/catalog.rs)). The registry matcher strips `us.`/`eu.`/`apac.`/`ap.`/`jp.`/`au.`/`ca.`/`il.`/`in.`/`global.`/`us-gov.` profile prefixes and `-vN:M` suffixes, and `*` entries are family globs.";

const OLLAMA_INTRO: &str = "The installed catalog is dynamic (`GET /api/tags`); entries are family globs describing typical capabilities, and `-cloud` tags match the same globs. `POST /api/show` (opt-in `include_details`) reports the real capabilities of each installed tag ([`catalog.rs`](../gaise-provider-ollama/src/contracts/catalog.rs)).";

const ELEVENLABS_INTRO: &str = "Text-to-speech and realtime voice. `GET /v1/models` reports model ids, languages, `can_do_text_to_speech`, style/speaker-boost support, and per-request character limits ([`models.rs`](../gaise-provider-elevenlabs/src/contracts/models.rs)). Voices are account-specific (`GET /v2/voices`) and ElevenLabs default voices expire 2026-12-31, so no voice is hard-coded. `eleven_v3*` realtime goes through the text-to-dialogue WebSocket; other models use `stream-input`.";

const CALENDAR_INTRO: &str = "Published shutdown dates for entries that are not yet retired, soonest first. Treat them as the **earliest** possible date; providers may extend but not advance them.";

const MAINTAINING: &str = "## Maintaining the registry

1. Verify against the official catalog and lifecycle pages linked above — never aggregators, and never the *other* Google surface.
2. Edit [`gaise-core/model-registry.toml`](../gaise-core/model-registry.toml) using only the closed vocabulary in its header. `cargo test -p gaise --lib registry` fails on unknown terms, unmapped statuses, or broken lookups ([`registry.rs` tests](../gaise-core/src/registry.rs)).
3. Separate lifecycle (`status`, `shutdown_date`, `retirement_not_before`, `replacement`) from adapter support (`gaise_support`).
4. When a family needs a new control mapping (thinking type, fixed sampling, tools rule), add a hermetic request-shape test in the provider crate and update [capabilities.md](capabilities.md).
5. Bump `audited_on`, regenerate this page (`cargo run -p gaise --example models_page > wiki/models.md`), the [limits](limits.md), [reasoning](reasoning.md), and [embeddings](embeddings.md) matrices, refresh the [vendor pages](README.md#vendors), and append to the audit report.
6. Never validate lifecycle by making a billable inference request.

The `.claude/agents/model-audit.md` agent definition automates steps 1–2 for coding agents.
";

const TABLE_HEAD: &str = "| Model | Aliases | Status | Dates | Input | Output | Ops | Tools | Reasoning | GAISe support / notes |
|---|---|---|---|---|---|---|---|---|---|";

/// Lower-case wire name of a serializable enum value (`text`, `supported`, ...).
fn wire_name<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

fn support_mark(value: &str) -> &'static str {
    match value {
        "supported" => "✓",
        "unsupported" => "✗",
        _ => "?",
    }
}

fn join_or_dash(items: Vec<String>, sep: &str) -> String {
    if items.is_empty() {
        "—".into()
    } else {
        items.join(sep)
    }
}

fn dates(m: &RegistryModel) -> String {
    let mut parts = Vec::new();
    if let Some(d) = &m.shutdown_date {
        parts.push(format!("shutdown {d}"));
    }
    if let Some(d) = &m.retirement_not_before {
        parts.push(format!("not before {d}"));
    }
    join_or_dash(parts, " · ")
}

fn ops_letters(ops: &[gaise_core::contracts::GaiseOperation]) -> String {
    let letters: String = ops
        .iter()
        .map(|op| match wire_name(op).as_str() {
            "instruct" => "I",
            "instruct_stream" => "S",
            "embeddings" => "E",
            "speech" => "V",
            "live" => "L",
            _ => "?",
        })
        .collect();
    if letters.is_empty() {
        "—".into()
    } else {
        letters
    }
}

fn reasoning(m: &RegistryModel, reasoning: &str, features: &[String]) -> String {
    let mark = support_mark(reasoning);
    if mark != "✓" {
        return mark.to_string();
    }
    let kind = if features.iter().any(|f| f == "adaptive_reasoning") {
        " adaptive"
    } else if features.iter().any(|f| f == "manual_reasoning") {
        " manual"
    } else {
        ""
    };
    match &m.reasoning_values {
        Some(values) if !values.is_empty() => format!("✓{kind} ({})", values.join(", ")),
        _ => format!("✓{kind}"),
    }
}

fn notes(m: &RegistryModel) -> String {
    let text = match (&m.gaise_support, &m.notes) {
        (Some(s), Some(n)) => format!("{s} — {n}"),
        (Some(s), None) => s.clone(),
        (None, Some(n)) => n.clone(),
        (None, None) => return "—".into(),
    };
    let text = text.replace('|', "\\|");
    if text.chars().count() > NOTE_LIMIT + 1 {
        let cut: String = text.chars().take(NOTE_LIMIT).collect();
        format!("{}…", cut.trim_end())
    } else {
        text
    }
}

fn row(m: &RegistryModel) -> String {
    let classified = m.classified().unwrap_or_default();
    let aliases = join_or_dash(m.aliases.iter().map(|a| format!("`{a}`")).collect(), ", ");
    let modalities = |items: &[gaise_core::contracts::GaiseModality]| {
        join_or_dash(items.iter().map(wire_name).collect(), ", ")
    };
    format!(
        "| `{}` | {} | `{}` | {} | {} | {} | {} | {} | {} | {} |",
        m.model,
        aliases,
        m.status,
        dates(m),
        modalities(&classified.input),
        modalities(&classified.output),
        ops_letters(&classified.operations),
        support_mark(&wire_name(&classified.tools)),
        reasoning(m, &wire_name(&classified.reasoning), &classified.features),
        notes(m),
    )
}

fn provider_section(
    out: &mut String,
    key: &str,
    name: &str,
    intro: &str,
    provider: Option<&RegistryProvider>,
    models: &[&RegistryModel],
) {
    out.push_str(&format!("## {key}\n\n### {name}\n\n{intro}\n\n"));
    if let Some(p) = provider {
        out.push_str(&format!(
            "- Vendor page: [vendor-{key}.md](vendor-{key}.md) · GAISe surface: {}\n",
            p.gaise_surface.as_deref().unwrap_or("—")
        ));
        out.push_str(&format!(
            "- Discovery: {}\n",
            p.discovery.as_deref().unwrap_or("—")
        ));
        out.push_str(&format!(
            "- Official catalog: <{}> · lifecycle: <{}>\n",
            p.catalog.as_deref().unwrap_or("—"),
            p.lifecycle.as_deref().unwrap_or("—")
        ));
        if let Some(notes) = &p.notes {
            out.push_str(&format!("- {notes}\n"));
        }
    }
    out.push('\n');

    let (mut current, mut deprecated, mut retired) = (Vec::new(), Vec::new(), Vec::new());
    for m in models {
        match m.status() {
            GaiseModelStatus::Retired => retired.push(*m),
            GaiseModelStatus::Deprecated | GaiseModelStatus::Legacy => deprecated.push(*m),
            _ => current.push(*m),
        }
    }
    for (title, rows) in [
        ("Current and preview", current),
        ("Deprecated and legacy", deprecated),
        ("Retired", retired),
    ] {
        if rows.is_empty() {
            continue;
        }
        out.push_str(&format!("#### {title}\n\n{TABLE_HEAD}\n"));
        for m in rows {
            out.push_str(&row(m));
            out.push('\n');
        }
        out.push('\n');
    }
}

fn main() {
    let registry = ModelRegistry::bundled();
    let mut out = HEADER
        .replace("{schema}", &registry.schema_version.to_string())
        .replace("{audited}", &registry.audited_on);

    out.push_str("\n## Contents\n\n\n");
    for (key, name, _) in PROVIDERS {
        let count = registry.models_for(key).count();
        out.push_str(&format!("- [{name}](#{key}) — {count} entries\n"));
    }
    out.push_str("- [Maintaining the registry](#maintaining-the-registry)\n");
    out.push_str("- [Lifecycle calendar](#lifecycle-calendar)\n\n");

    for (key, name, intro) in PROVIDERS {
        let models: Vec<&RegistryModel> = registry.models_for(key).collect();
        provider_section(&mut out, key, name, intro, registry.provider(key), &models);
    }

    out.push_str(&format!("## Lifecycle calendar\n\n{CALENDAR_INTRO}\n\n"));
    out.push_str("| Date | Provider | Model | Replacement |\n|---|---|---|---|\n");
    let provider_rank = |key: &str| PROVIDERS.iter().position(|(k, _, _)| *k == key);
    let mut calendar: Vec<&RegistryModel> = registry
        .models
        .iter()
        .filter(|m| m.shutdown_date.is_some() && m.status() != GaiseModelStatus::Retired)
        .collect();
    calendar.sort_by_key(|m| {
        (
            m.shutdown_date.clone(),
            provider_rank(&m.provider),
            m.model.clone(),
        )
    });
    for m in calendar {
        out.push_str(&format!(
            "| {} | [{}](#{}) | `{}` | {} |\n",
            m.shutdown_date.as_deref().unwrap_or("—"),
            m.provider,
            m.provider,
            m.model,
            m.replacement.as_deref().unwrap_or("—")
        ));
    }
    out.push('\n');
    out.push_str(MAINTAINING);
    print!("{out}");
}
