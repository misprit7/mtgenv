#!/usr/bin/env bash
#
# One-time setup for the mtgenv repo. Idempotent / safe to re-run.
#
# Steps:
#   1. Download Scryfall "default_cards" bulk data into the gitignored data/ dir.
#      (default_cards = one object per printing, so every Arena printing's `arena_id`
#      is present — unlike oracle_cards, which keeps only one representative printing
#      per oracle id and drops most Arena ids.)
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

# 1. Scryfall default_cards bulk data -------------------------------------------------------
fetch_scryfall_default_cards() {
  need curl; need jq
  mkdir -p "$DATA_DIR"
  local out="$DATA_DIR/default-cards.json"
  local stamp="$DATA_DIR/.default-cards.updated_at"

  echo "==> Resolving Scryfall default_cards bulk download…"
  local meta uri updated
  meta="$(curl -fsSL -H "User-Agent: $UA" -H "Accept: application/json" \
            https://api.scryfall.com/bulk-data)"
  uri="$(printf '%s' "$meta" | jq -r '.data[] | select(.type=="default_cards") | .download_uri')"
  updated="$(printf '%s' "$meta" | jq -r '.data[] | select(.type=="default_cards") | .updated_at')"

  if [[ -z "$uri" || "$uri" == "null" ]]; then
    echo "error: could not find a 'default_cards' entry in the Scryfall bulk-data index." >&2
    exit 1
  fi
  if [[ -f "$out" && -f "$stamp" && "$(cat "$stamp")" == "$updated" ]]; then
    echo "    default-cards.json already current ($updated) — skipping download."
    return 0
  fi

  echo "==> Downloading default_cards ($updated)… (~450MB, every printing)"
  curl -fSL --progress-bar -H "User-Agent: $UA" "$uri" -o "$out.tmp"
  mv "$out.tmp" "$out"
  printf '%s' "$updated" > "$stamp"
  # Remove the legacy oracle_cards dump if present (we use default_cards now).
  rm -f "$DATA_DIR/oracle-cards.json" "$DATA_DIR/.oracle-cards.updated_at"
  local stats
  stats="$(jq -c '{n: length, arena: ([.[] | select(.arena_id)] | length)}' "$out")"
  echo "    saved $out ($(du -h "$out" | cut -f1); $stats)."
}

# 1b. Build a queryable SQLite index from the bulk JSON ---------------------------------------
# The 550MB JSON is too slow to scan per card lookup (~2 min/pass). We derive a single
# indexed SQLite file (one row per printing, gameplay columns only) that card-authoring
# queries by name in milliseconds. Rebuilt whenever the underlying JSON changes.
build_card_index() {
  need python3
  local src="$DATA_DIR/default-cards.json"
  local db="$DATA_DIR/cards.sqlite"
  local stamp="$DATA_DIR/.default-cards.updated_at"
  local dbstamp="$DATA_DIR/.cards-sqlite.updated_at"

  if [[ ! -f "$src" ]]; then
    echo "    note: $src missing — skipping card index." ; return 0
  fi
  if [[ -f "$db" && -f "$stamp" && -f "$dbstamp" && "$(cat "$stamp")" == "$(cat "$dbstamp")" ]]; then
    echo "    cards.sqlite already current — skipping rebuild."
    return 0
  fi
  echo "==> Building SQLite card index (data/scryfall/cards.sqlite)…"
  python3 "$REPO_ROOT/scripts/build_card_index.py"
  [[ -f "$stamp" ]] && cp "$stamp" "$dbstamp"
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
  fetch_scryfall_default_cards
  build_card_index
  install_web_deps
  echo "==> Setup complete."
}

main "$@"
