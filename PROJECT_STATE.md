# Project State

Single source of truth for goals + where things stand. Update this (without being asked)
whenever meaningful progress changes the picture. Companion: `WORKLOG.md` (chronological).

_Last updated: 2026-06-13_

## Vision (one sentence)

A fast, correct, headless Rust implementation of the MTG rules engine — modeled on MTG
Arena's "whiteboard" architecture — that drives an efficient Gymnasium environment for
training an MTG AI in Python/PyTorch via self-play, with a pluggable decision boundary so
the same engine can be driven by a Python RL agent, a human via a web client, or the real
MTGA client.

## Long-term goals

1. **Full ruleset** implementation (card-agnostic core + a large card pool as Effect-IR data).
2. **Efficient Gymnasium env** (PyO3+maturin) for PyTorch RL self-play at high throughput.
3. **Pluggable decision boundary** — scripted AI, Python RL agent, web-client human, and the
   real MTGA client are interchangeable `Agent` implementations ("the easy switch").
4. **Web play interface + GRE-protocol server** — a from-scratch web client to play against
   mtg-core, built so closely to MTGA's client↔server interface that the real MTGA client
   can be dropped in against our backend (endpoint redirect, or decompile/recompile). The
   recovered GRE protocol is the client seam; see `docs/plans/CLIENT_PLAN.md`.
5. **Expressive enough for MTGA-grade complex cards** (Zurgo/Sylvan Library class) even
   though they're not implemented near-term — the architecture must not foreclose them.

## Short-term goals (next)

1. Stand up the **Cargo workspace** + headless `mtg-core` skeleton; move GUI out of core
   (ENGINE_PLAN milestone 1).
2. **Turn engine + priority + stack + agenda loop** correct on a lands-only game (milestone 2).
3. **Mana + casting + vanilla-creature combat** → first `RandomAgent` self-play games
   (milestone 3).
4. Spike the **MTGA decompile** in `../mtga-re` to recover the GRE protobuf schema
   (DECOMPILE_PLAN) — informs the `DecisionRequest` enum.

## Current state

- **Phase: parallel build kicked off** via a tmux agent team (`mtgenv`, lead + 4 teammates).
  Active workstreams (shared task board): **engine** scaffolding the Cargo workspace +
  headless `mtg-core` (#1); **decompile** recovering the GRE schema **and transport** in
  `../mtga-re` (#2); **design** authoring `AGENT_INTERFACE.md` then implementing
  agent/effects (#3,#4); **client** planning the web client + GRE server (#5).
  **design done with #3:** `docs/design/AGENT_INTERFACE.md` specifies the boundary (one
  `Agent` trait, superset `DecisionRequest`/`Response`, `PlayerView`, Effect IR + `Native`);
  #4 (implement it in `mtg-core`) waits on the scaffold (#1).
  **client done with #5:** `docs/plans/CLIENT_PLAN.md` plans the web UI + `mtg-gre-server`
  (axum+WS, depends only on `mtg-core`); a human is just a `GreSessionAgent` behind the one
  boundary; mapping reconciled to AGENT_INTERFACE §6.1/§1.1; real-client drop-in via
  endpoint-redirect or Mono patch. Transport/auth details blocked on decompile (#2).
- Existing `src/*.rs` is a ~500-line naming skeleton being **replaced** by the workspace
  (kept only as vocabulary reference); `egui`/`eframe` moving out of the core.
- Docs in place: architecture (`docs/design/WHITEBOARD_MODEL.md`), rules
  (`docs/rules/RULES_SUMMARY.md` + PDF/text), plans (`docs/plans/`), `CLAUDE.md`.
- Decompile recon done: MTGA = Mono + protobuf, Steam install; full decompile in progress.

## Key decisions

- **Whiteboard model** as the engine architecture (see WHITEBOARD_MODEL.md).
- **Core is card-agnostic**; card behavior is data (Effect IR) + `Native` escape hatch.
- **Single `Agent`/`DecisionRequest` boundary** with engine-provided legal-action masking;
  ALL front-ends (RL, scripted, web, MTGA client) are backends of this one boundary.
- **The GRE protocol is the client seam.** A GRE-protocol server wraps `mtg-core`; the web
  client and the real MTGA client are both clients of it → decompile must capture transport,
  not just message schemas.
- **PyO3 + maturin** for the RL hot path; a socket transport option for the GRE/web client.
- Target **paper CR** as truth + an **Arena profile** for MTGA-specific behavior.
- Validate rules correctness via CR-derived unit/expect tests + the captured MTGA Detailed-Logs.
  (No Forge — it's the abandoned prior attempt this project replaces, not a reference/oracle.)

## Risks / open questions

- Layer system (CR 613) + replacement-effect interaction are genuine fixpoint computations
  — the hardest correctness surface.
- Action-space design for RL (huge, variable) — factored + masked vocabulary, autoregressive
  later.
- State must stay cheaply cloneable/serializable for MCTS + vectorized envs.
- Card-pool acquisition strategy: MTGJSON/Scryfall oracle data → Effect-IR (oracle-text→IR compiler).
- Legal/ToS care around MTGA decompilation (personal research/interop; don't redistribute
  WotC code/assets) — see DECOMPILE_PLAN.
