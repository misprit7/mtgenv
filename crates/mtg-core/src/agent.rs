//! The single decision boundary: the `Agent` trait + `DecisionRequest` /
//! `DecisionResponse` + `PlayerView` (info-filtered). Every player choice — scripted AI,
//! Python RL, web/GRE client — flows through here; the engine pre-enumerates the *legal*
//! options (masking is the engine's job).
//!
//! **Owned by the `design` workstream (task #4).** Spec: `docs/design/AGENT_INTERFACE.md`.
//! Intentionally an empty stub — `design` implements it against the recovered GRE schema
//! (`../mtga-re/`), keeping `DecisionRequest` a strict superset of the GRE `*Req` catalog.
