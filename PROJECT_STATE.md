# Project State

Single source of truth for goals + where things stand. Update this (without being asked)
whenever meaningful progress changes the picture. Companion: `WORKLOG.md` (chronological).

_Last updated: 2026-06-13_

## Vision (one sentence)

A fast, correct, headless Rust implementation of the MTG rules engine — modeled on MTG
Arena's "whiteboard" architecture — that drives an efficient Gymnasium environment for
training an MTG AI in Python/PyTorch via self-play, with a pluggable decision boundary so
the same engine can later be driven by the real MTGA client.

## Long-term goals

1. **Full ruleset** implementation (card-agnostic core + a large card pool as Effect-IR data).
2. **Efficient Gymnasium env** (PyO3+maturin) for PyTorch RL self-play at high throughput.
3. **Pluggable decision boundary** — scripted AI, Python RL agent, and the real MTGA client
   are interchangeable `Agent` implementations ("the easy switch").
4. **Expressive enough for MTGA-grade complex cards** (Zurgo/Sylvan Library class) even
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

- **Phase: planning complete, implementation not started.** (User asked for plans first;
  no engine code written yet.)
- Existing `src/*.rs` is a ~500-line naming skeleton — **to be replaced** by the workspace
  in ENGINE_PLAN (kept only as vocabulary reference). Core wrongly depends on `egui`/`eframe`.
- Docs in place: architecture (`docs/design/WHITEBOARD_MODEL.md`), rules
  (`docs/rules/RULES_SUMMARY.md` + PDF/text), three plans (`docs/plans/`), `CLAUDE.md`.
- Decompile recon done: MTGA = Mono + protobuf, Steam install; plan ready, repo not created.

## Key decisions

- **Whiteboard model** as the engine architecture (see WHITEBOARD_MODEL.md).
- **Core is card-agnostic**; card behavior is data (Effect IR) + `Native` escape hatch.
- **Single `Agent`/`DecisionRequest` boundary** with engine-provided legal-action masking.
- **PyO3 + maturin** for the RL hot path; keep a socket transport option for MTGA/Forge.
- Target **paper CR** as truth + an **Arena profile** for MTGA-specific behavior.
- Use **Forge** (`../forge-ai`) as a differential-testing oracle and possible interim Gym backend.

## Risks / open questions

- Layer system (CR 613) + replacement-effect interaction are genuine fixpoint computations
  — the hardest correctness surface.
- Action-space design for RL (huge, variable) — factored + masked vocabulary, autoregressive
  later.
- State must stay cheaply cloneable/serializable for MCTS + vectorized envs.
- Card-pool acquisition strategy (translate Forge `cardsfolder` → IR vs. oracle-text compiler).
- Legal/ToS care around MTGA decompilation (personal research/interop; don't redistribute
  WotC code/assets) — see DECOMPILE_PLAN.
