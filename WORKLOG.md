# Work Log

Short, dated entries for future-agent consumption. Newest first. One line or a few bullets
per unit of meaningful progress. Keep it terse — detail lives in `docs/` and git history.

## 2026-06-13

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
