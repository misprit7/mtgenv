//! The turn machine: phases and steps, turn-based actions (CR 500s).
//!
//! This module owns the *static shape* of a turn — the ordered list of steps and the
//! per-step predicates (which steps grant priority, which are main phases). The engine
//! (`priority.rs`) walks this sequence, performing each step's turn-based actions and
//! then (where applicable) running a priority round.
//!
//! See `docs/rules/RULES_SUMMARY.md` §3 for the full table with CR rule numbers.

use crate::basics::Phase;

/// The twelve steps of a turn in order (CR 500.1, flattened over phases). Every turn has
/// all five phases even if empty (CR 500.1); the combat phase's steps always occur (a
/// lands-only game simply declares no attackers).
pub const TURN_STEPS: [Phase; 12] = [
    // Beginning phase (CR 501)
    Phase::Untap,   // 502
    Phase::Upkeep,  // 503
    Phase::Draw,    // 504
    // Precombat main phase (CR 505)
    Phase::PrecombatMain,
    // Combat phase (CR 506)
    Phase::BeginCombat,      // 507
    Phase::DeclareAttackers, // 508
    Phase::DeclareBlockers,  // 509
    Phase::CombatDamage,     // 510
    Phase::EndCombat,        // 511
    // Postcombat main phase (CR 505)
    Phase::PostcombatMain,
    // Ending phase (CR 512)
    Phase::End,     // 513
    Phase::Cleanup, // 514
];

/// Whether the active player receives priority during this step.
///
/// Untap gives no priority (CR 502.4); cleanup *normally* gives none (CR 514.3) — the
/// engine handles the 514.3a exception (pending SBAs/triggers ⇒ a priority round, then
/// repeat) separately. Every other step grants priority (CR 117.3a).
pub fn step_grants_priority(step: Phase) -> bool {
    !matches!(step, Phase::Untap | Phase::Cleanup)
}

/// The two main phases (CR 505): the only steps where sorcery-speed actions are legal and
/// where a land may be played (CR 117.1a, 505.6).
pub fn is_main_phase(step: Phase) -> bool {
    matches!(step, Phase::PrecombatMain | Phase::PostcombatMain)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turn_has_twelve_ordered_steps() {
        assert_eq!(TURN_STEPS.len(), 12);
        assert_eq!(TURN_STEPS[0], Phase::Untap);
        assert_eq!(TURN_STEPS[11], Phase::Cleanup);
    }

    #[test]
    fn untap_and_cleanup_skip_priority() {
        assert!(!step_grants_priority(Phase::Untap));
        assert!(!step_grants_priority(Phase::Cleanup));
        assert!(step_grants_priority(Phase::Upkeep));
        assert!(step_grants_priority(Phase::PrecombatMain));
    }

    #[test]
    fn only_main_phases_are_main() {
        assert!(is_main_phase(Phase::PrecombatMain));
        assert!(is_main_phase(Phase::PostcombatMain));
        assert!(!is_main_phase(Phase::Upkeep));
        assert!(!is_main_phase(Phase::CombatDamage));
    }
}
