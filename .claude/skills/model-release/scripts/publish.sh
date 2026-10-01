#!/usr/bin/env bash
# Publish every GAISe crate at the current workspace version, in dependency
# order (wiki/releasing.md). Run from a clean checkout of main after the
# release PR is merged. Safe to re-run: a crate whose version is already on
# crates.io is skipped, so a failed run resumes where it stopped.
set -uo pipefail
cd "$(git rev-parse --show-toplevel)"

if [ -n "$(git status --porcelain)" ]; then
  echo "working tree is not clean; refusing to publish" >&2
  exit 1
fi

CRATES=(
  gaise
  gaise-provider-anthropic gaise-provider-bedrock gaise-provider-gemini
  gaise-provider-ollama gaise-provider-openai gaise-provider-vertexai
  gaise-provider-elevenlabs gaise-provider-typesafe
  gaise-client
  gaise-api
)

for crate in "${CRATES[@]}"; do
  out=$(cargo publish -p "$crate" 2>&1)
  if echo "$out" | grep -q "Published"; then
    echo "published $crate"
  elif echo "$out" | grep -qi "already exists"; then
    echo "skipped $crate (version already on crates.io)"
  else
    echo "$out" | tail -20 >&2
    echo "FAILED at $crate; fix the cause and re-run" >&2
    exit 1
  fi
done
