//! The priority loop + the agenda pipeline, run to a fixpoint between priority passes:
//! recompute continuous effects → state-based actions (loop) → put triggers on the
//! stack (APNAP) → grant priority. CR 117.5, 603.3, 704.3.
//!
//! Stub — milestone 2. See `docs/design/WHITEBOARD_MODEL.md` §2.2.
