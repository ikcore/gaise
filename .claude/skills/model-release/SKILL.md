---
name: model-release
description: Audit every GAISe provider for new, changed, and retiring models, update the model registry and adapters, then release. Bumps the minor version for model or lifecycle changes and the major version when a public contract must change, opens a PR to main with a CHANGELOG.md entry, merges it, and publishes every crate to crates.io. Use when the user asks to check for new models, run a model audit or model release, or update the providers' models and publish. Do not use for a single targeted change the user describes (edit that directly) or for a release with no model audit.
---

# Model audit and release

Asking for this is the go-ahead for the whole pipeline, including the merge
and the crates.io publish. Do not stop to ask for confirmation between steps,
but stop and report instead of publishing if anything below says to stop.

Publishing is irreversible: a crates.io version can be yanked but never
replaced. Everything before step 8 must be green first.

## 1. Preflight

```bash
git checkout main && git pull --ff-only && git status --porcelain   # must be empty
cargo test --workspace --all-features                                # must pass before any edit
gh auth status
```

Stop if the tree is dirty or the baseline tests fail. Report it; don't fix
unrelated breakage inside a release.

Read these first:
- `gaise-core/model-registry.toml`: the header, the `[[providers]]` notes, and `audited_on`.
- `.claude/agents/model-audit.md`: current documentation URLs per provider, and what to check.
- `wiki/models.md#maintaining-the-registry`.

## 2. Research (parallel agents)

Launch one research-only agent per group, in a single message:
1. OpenAI
2. Anthropic
3. Gemini API and Vertex AI
4. Bedrock
5. ElevenLabs and Ollama
6. TypeSafe

Give each agent:
- the provider's current registry rows (it should grep them itself);
- the source URLs from `model-audit.md`;
- the instruction to quote the URL and exact wording for every fact, separate official docs from third-party reports, and say plainly what it could not verify.

Each agent reports:
- **New models:** id, aliases, release date, status, context and output limits, modalities, reasoning values, sampling and tool rules.
- **Lifecycle changes:** legacy, deprecated, shutdown date, replacement.
- **Request or response contract changes** that affect a GAISe adapter.

Never record anything that only third-party sources support.

## 3. Decide the release type

- **No change** (nothing new, no status or date change, no contract change): stop here. Report "no changes" with the date checked. No branch, no PR, no publish.
- **Minor** (`bump_version.py minor`), any of:
  - new models;
  - status, date, limit, or note changes;
  - adapter rule changes for a vendor-side change;
  - new public functions;
  - new optional behaviour.
- **Major** (`bump_version.py major`): only when GAISe's own public contract must break:
  - removing or renaming a public item;
  - adding a field to a public struct callers construct;
  - changing a serialized value or HTTP path;
  - changing the request shape GAISe sends for an existing model id in a way callers can observe.

  Prefer an additive design that keeps the release minor (see the rules below). Explain in the PR why a break was unavoidable.

## 4. Apply the changes

Registry rules (`gaise-core/model-registry.toml`):
- **Never remove an id.** A shut-down model becomes `status = "retired"`. Aliases are only ever added.
- **Take statuses from the vendor, not from age.**
  - A model becomes `legacy` / `deprecated` / `retired` only when the vendor labels it so; record dates (`shutdown_date`, `retirement_not_before`) and `replacement` as published.
  - "Older" models the vendor still lists as current stay `active`.
  - Anthropic's "Legacy models" list maps to `legacy`.
- **Lifecycles are per surface.** Gemini API and Vertex AI dates are separate, and Bedrock lifecycle is AWS's, not the model vendor's. Never copy dates across.
- **Update the audit metadata.** Bump `audited_on` and the header comment, and add a dated sentence to each re-audited provider's `notes`.
- **Keep the vocabulary closed.** `capabilities` must stay within the vocabulary in the file header; `cargo test -p gaise` enforces it.

Adapter rules:
- Family rule tables match by substring or prefix, so check every new id against them:
  - OpenAI: `openai_chat_rules`, `chat_tools_require_responses`, `gpt6_follows_gpt56_rules`.
  - Anthropic: `claude_family_rules`.
  - Bedrock: `claude_rules`.
  - Gemini and Vertex: `thinking_levels_for`.
  - Ollama: `ollama_think`.

  A new id that shares a prefix inherits rules that may be wrong (e.g. `claude-opus-5-5` contains `claude-opus-5`).
- Change behaviour only for ids that did not exist before, unless the vendor itself changed the old ones.
- Add a new public function rather than a field on a public struct.
- Every rule change gets a hermetic case in that crate's `tests/parameter_matrix_tests.rs` (or its unit tests).
- Every new registry row gets a lookup pin in `bundled_lookups_resolve_expected_entries` (`gaise-core/src/registry.rs`).

## 5. Regenerate and document

```bash
python .claude/skills/model-release/scripts/regen_wiki.py
```

The script rewrites the following and preserves the hand-written TypeSafe sections:
- the vendor `## Models` tables;
- the generated regions of `wiki/limits.md`, `wiki/reasoning.md`, and `wiki/embeddings.md`;
- `wiki/models.md`.

Then, by hand:
- bump the "(audited YYYY-MM-DD)" headings, and the anchors linking to them, only on pages for providers you re-audited;
- update hand-written vendor-page rows that describe changed rules;
- update the README model lineup if the headline models changed.

Add a dated section to `secret/audit-report.md` (gitignored), with these subsections:
- New models
- Status changes
- Adapter changes
- Could not verify
- Backward compatibility

## 6. Version and changelog

```bash
python .claude/skills/model-release/scripts/bump_version.py minor   # or major
```

Add an entry at the top of `CHANGELOG.md`: `## [X.Y.Z] - YYYY-MM-DD`, with `### Added` / `### Changed` / `### Deprecated` / `### Removed` / `### Fixed` as needed. Mark breaking items **Breaking:**. Add the link line at the bottom once the PR exists.

## 7. Verify

```bash
cargo fmt --all
# the working tree is CRLF under core.autocrlf; cargo fmt and generators write LF
for f in $(git ls-files --eol -m -o --exclude-standard | grep -v "w/crlf" | awk '{print $NF}'); do sed -i -b 's/\r$//; s/$/\r/' "$f"; done
cargo fmt --all -- --check
cargo clippy --workspace --all-features --all-targets   # no warnings
cargo test --workspace --all-features
python .claude/skills/model-release/scripts/regen_wiki.py --check   # "drift: none"
```

If something fails and the fix is not clearly right, stop: push the branch, open the PR without merging, and report why.

## 8. PR, merge, publish

```bash
git checkout -b model-release-YYYY-MM-DD
git add -A && git commit    # summary subject; body lists models and rule changes
git push -u origin HEAD
gh pr create --base main --title "Release X.Y.Z: model audit YYYY-MM-DD" --body "..."
gh pr checks --watch
gh pr merge --merge
git checkout main && git pull --ff-only
bash .claude/skills/model-release/scripts/publish.sh
```

- **Commit trailer:** no `Co-Authored-By` trailer; the maintainer forbids it on this repository.
- **PR body:**
  - summary per provider;
  - the release type and why;
  - a **Not verified** section (anything from summarised pages, anything untested live);
  - test results;
  - end with the 🤖 Generated with Claude Code line.
- **Merge:** only after checks pass.
- **`publish.sh`:** publishes in dependency order, skips versions already on crates.io, and stops at the first failure. Re-run it after fixing the cause.

## 9. Report

Tell the user:
- the version released and why that bump;
- new models per provider;
- status changes;
- adapter changes;
- what could not be verified;
- the PR link;
- whether all 11 crates published.

Lead with anything they need to act on.
