# Work Log

Short, dated entries for future-agent consumption. Newest first. One line or a few bullets
per unit of meaningful progress. Keep it terse — detail lives in `docs/` and git history.

## 2026-06-13

- **design:** implemented task #4 — the agent boundary + Effect IR are now real code in
  `mtg-core` (commit 360d3a6). New: `agent.rs` (the `Agent` trait, `DecisionRequest` 21-variant
  enum, `DecisionResponse`, `PlayerView` + view types, all supporting request types, `GameEvent`,
  and a `RandomAgent` reference backend that can only pick legal options); `effects/` split into
  `mod.rs` (the `Effect` IR), `action.rs` (`Action`/`Whiteboard`), `ability.rs` (the 5 ability
  kinds + costs/keywords/qualifications), `value.rs`/`target.rs`/`condition.rs`/`native.rs`
  (the `Native` escape hatch). Plus shared `basics.rs` (Color/Zone/Phase/Status/ManaCost/
  ManaPool/CounterKind/CounterBag/DamageKind/Target/ZoneDest — one canonical home; **engine
  imports these, doesn't redefine**) and `error.rs` (EngineError). `cargo build`+`cargo test`+
  `cargo clippy` all green; 6 unit tests (RandomAgent legality, ChooseNumber constraint
  honoring, determinism-by-seed, serde round-trip). Boundary types derive serde (the §1.1
  GRE-server contract). One open item flagged: batched `CastingTimeOptions` needs a multi-part
  response (decompose vs. structured) — ratify with engine/gym/client at integration.
- **design:** reconciled `AGENT_INTERFACE.md` against the recovered+log-validated GRE schema
  (decompile's `../mtga-re/`) — §9 now RESOLVED, not open. Confirmed strict-superset holds
  (variant set unchanged); enriched `ChooseNumber` to match `NumericInputReq` exactly
  (`step`/`disallow_even`/`disallow_odd`; `forbidden`↔`disallowedValues`). Key validation:
  GRE `CastingTimeOptionReq` embeds `numericInputReq`/`modalReq`/`selectNReq` as inner
  messages — i.e. GRE's own wire literally decomposes a cast's options into our
  ChooseNumber/ChooseModes/SelectCards sub-steps. `TargetSelection` ≅ our `TargetSlot`.
  Also added §8.1: decision *elision* (auto-pass / forced-single-option) is an engine/Arena-
  profile concern, uniform across all backends (load-bearing for differential-testing/replay).
- **client:** wrote `docs/plans/CLIENT_PLAN.md` (task #5) — web play UI + a **GRE-protocol
  server** (`mtg-gre-server` crate, axum + WebSocket, depends only on `mtg-core`) fronting the
  engine. A human at the web UI is just another `Agent` backend (`GreSessionAgent`) — same
  single boundary as RL/Gym and scripted AI. The seam is the GRE protocol itself, so the
  **real MTGA client can be dropped in** (two strategies: protocol-compatible server +
  endpoint redirect, vs. patch/runtime-hook the Mono client). Milestones: CLI text client →
  minimal web board (JSON) → protocol-compatible server (recovered protobuf) → real-client
  drop-in. Reconciled the DecisionRequest⇄GRE mapping to `AGENT_INTERFACE.md` §6.1; the docs
  now cross-reference (design added §1.1 GRE-server serialization contract).
- **client (follow-up):** decompile #2 landed → folded the **recovered + log-validated GRE
  transport + schema** into CLIENT_PLAN §4/§5/§8 (no longer assumptions): wire = TLS 1.2 over
  TCP, custom **6-byte frame** `[ver=4][type|format][int32 LE len]` inside the TLS stream +
  ping/pong keepalive; envelope = `IMessageEnvelope{Protobuf|Json, Compressed, TransId}` w/
  protobuf payload as `Any`; **endpoint is dynamic** (match push `MatchInfoV3.MatchEndpointHost/
  Port`+`MatchId`); GRE `ConnectReq` is **tokenless** (auth binds upstream). Net: real-client
  drop-in **de-risked** — no GRE token to forge, TLS solvable via controlling the pushed
  hostname + local dev-CA (no pinning bypass). Mapping table updated to exact recovered resp
  names; sent transport facts to decompile for their #6.
- **design:** wrote `docs/design/AGENT_INTERFACE.md` — the single `Agent` trait +
  `DecisionRequest`/`DecisionResponse` enums + `PlayerView` (info-filtered, hidden zones
  masked) + the Effect IR / whiteboard `Action` / `Native` hatch (Rust sketches). The
  `DecisionRequest` set is a proven **superset** of Forge's 107-method `PlayerController`
  AND the recovered MTGA GRE `*Req` catalog (coverage matrices in §6). Masking is the
  engine's job. Asked `decompile` for field-level GRE Req/Resp shapes (§9 open questions);
  variant set not expected to change. Task #4 (implement agent.rs + effects/) blocked on
  the workspace scaffold (#1).
- **Project bootstrapped from skeleton into a planned project.** Established docs, the
  architecture, and three implementation plans. No engine code written yet (planning phase).
- Downloaded the MTG Comprehensive Rules (eff. 2026-02-27) → `docs/rules/`
  (`MagicCompRules_20260227.pdf` + extracted `comprules.txt`).
- Wrote `docs/rules/RULES_SUMMARY.md` — engine-implementer's map of the CR (layers, SBAs,
  priority/stack, combat, replacement/triggers, keyword index), with rule numbers.
- **Architecture decided: the MTGA "whiteboard" model** (per WotC dev diaries) →
  `docs/design/WHITEBOARD_MODEL.md`. Card-agnostic core + declarative effect rules that
  rewrite a pending-actions whiteboard; agenda pipeline; qualifications; layers; LKI.
- Wrote `docs/plans/ENGINE_PLAN.md` (Rust workspace, milestones, agent boundary, testing
  incl. differential-vs-Forge), `docs/plans/GYM_PLAN.md` (PyO3+maturin, action masking,
  self-play), `docs/plans/DECOMPILE_PLAN.md` (MTGA protocol recovery).
- Recon: **MTGA is a Mono build** (not IL2CPP), Steam install, **protobuf** GRE protocol
  (`Wizards.MDN.GreProtobuf.dll`). Decompile is the easy path; work to live in `../mtga-re`.
- Wrote `CLAUDE.md` (orientation + conventions) and these trackers. Initialized git history.
