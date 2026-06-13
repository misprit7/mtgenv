#!/usr/bin/env bash
#
# One-time setup for the mtgenv repo. Idempotent / safe to re-run.
#
# Steps:
#   1. Download Scryfall "oracle_cards" bulk data into the gitignored data/ dir.
#   2. Install the web client's npm deps (only if the web front end + npm are present).
#
# Add further one-time setup below as the project grows.
#
# Scryfall bulk data: https://scryfall.com/docs/api/bulk-data — the download URI is
# timestamped, so we always resolve the current one from the bulk-data index first.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DATA_DIR="$REPO_ROOT/data/scryfall"
# Be a good API citizen: identify ourselves (Scryfall asks for a descriptive UA).
UA="mtgenv/0.1 (https://github.com/misprit7/mtgenv; local dev)"

need() { command -v "$1" >/dev/null 2>&1 || { echo "error: '$1' is required but not installed." >&2; exit 1; }; }

# 1. Scryfall oracle_cards bulk data --------------------------------------------------------
fetch_scryfall_oracle_cards() {
  need curl; need jq
  mkdir -p "$DATA_DIR"
  local out="$DATA_DIR/oracle-cards.json"
  local stamp="$DATA_DIR/.oracle-cards.updated_at"

  echo "==> Resolving Scryfall oracle_cards bulk download…"
  local meta uri updated
  meta="$(curl -fsSL -H "User-Agent: $UA" -H "Accept: application/json" \
            https://api.scryfall.com/bulk-data)"
  uri="$(printf '%s' "$meta" | jq -r '.data[] | select(.type=="oracle_cards") | .download_uri')"
  updated="$(printf '%s' "$meta" | jq -r '.data[] | select(.type=="oracle_cards") | .updated_at')"

  if [[ -z "$uri" || "$uri" == "null" ]]; then
    echo "error: could not find an 'oracle_cards' entry in the Scryfall bulk-data index." >&2
    exit 1
  fi
  if [[ -f "$out" && -f "$stamp" && "$(cat "$stamp")" == "$updated" ]]; then
    echo "    oracle-cards.json already current ($updated) — skipping download."
    return 0
  fi

  echo "==> Downloading oracle_cards ($updated)…"
  curl -fSL --progress-bar -H "User-Agent: $UA" "$uri" -o "$out.tmp"
  mv "$out.tmp" "$out"
  printf '%s' "$updated" > "$stamp"
  echo "    saved $out ($(du -h "$out" | cut -f1), $(jq 'length' "$out") cards)."
}

# 2. Web client deps (optional) -------------------------------------------------------------
install_web_deps() {
  local web="$REPO_ROOT/crates/mtg-gre-server/web"
  if [[ -d "$web" && -f "$web/package.json" ]]; then
    if command -v npm >/dev/null 2>&1; then
      echo "==> Installing web client npm deps…"
      (cd "$web" && npm install --no-audit --no-fund)
    else
      echo "    note: web front end present but 'npm' not found — skipping (install Node to build the web UI)."
    fi
  fi
}

main() {
  fetch_scryfall_oracle_cards
  install_web_deps
  echo "==> Setup complete."
}

main "$@"
