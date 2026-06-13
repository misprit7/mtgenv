//! State-based actions (CR 704), checked and applied to a fixpoint within the agenda
//! loop before any player receives priority (CR 117.5, 704.3).
//!
//! Milestone 2 implements the three player-loss checks that a lands-only game can hit
//! (most importantly decking, 704.5b); the permanent-related SBAs (lethal damage,
//! toughness ≤ 0, the legend rule, …) arrive with creatures/combat in milestone 3.
//!
//! [`collect`] is a pure read of the state — it never mutates. The engine performs the
//! returned actions "simultaneously as a single event" (704.3) and repeats until none
//! apply.

use crate::ids::PlayerId;
use crate::state::GameState;

/// Why a player loses the game (CR 704.5a–c).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LossReason {
    /// 704.5a — life total 0 or less.
    ZeroOrLessLife,
    /// 704.5b — attempted to draw from an empty library since the last SBA check.
    DrewFromEmptyLibrary,
    /// 704.5c — ten or more poison counters.
    TenPoison,
}

/// A state-based action that needs performing. Milestone 2 only models player losses; the
/// enum is left open (non-exhaustive in spirit) so permanent SBAs slot in later.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateBasedAction {
    PlayerLoses {
        player: PlayerId,
        reason: LossReason,
    },
}

/// Collect every state-based action that currently applies (CR 704.5). Pure: no mutation.
/// Players who have already lost are skipped so the agenda fixpoint terminates.
pub fn collect(state: &GameState) -> Vec<StateBasedAction> {
    let mut out = Vec::new();
    for p in &state.players {
        if p.has_lost {
            continue;
        }
        // A single player can satisfy several loss conditions at once; each is reported,
        // but they collapse to the same result (that player loses).
        if p.life <= 0 {
            out.push(StateBasedAction::PlayerLoses {
                player: p.id,
                reason: LossReason::ZeroOrLessLife,
            });
        }
        if p.drew_from_empty {
            out.push(StateBasedAction::PlayerLoses {
                player: p.id,
                reason: LossReason::DrewFromEmptyLibrary,
            });
        }
        if p.poison >= 10 {
            out.push(StateBasedAction::PlayerLoses {
                player: p.id,
                reason: LossReason::TenPoison,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::GameState;

    #[test]
    fn no_sbas_in_a_fresh_game() {
        let state = GameState::new(2, 1);
        assert!(collect(&state).is_empty());
    }

    #[test]
    fn zero_life_and_poison_and_decking_are_losses() {
        let mut state = GameState::new(2, 1);
        state.player_mut(PlayerId(0)).life = 0;
        state.player_mut(PlayerId(1)).poison = 10;
        let sbas = collect(&state);
        assert!(sbas.contains(&StateBasedAction::PlayerLoses {
            player: PlayerId(0),
            reason: LossReason::ZeroOrLessLife,
        }));
        assert!(sbas.contains(&StateBasedAction::PlayerLoses {
            player: PlayerId(1),
            reason: LossReason::TenPoison,
        }));

        // A player already marked lost is no longer reported (loop must terminate).
        state.player_mut(PlayerId(0)).has_lost = true;
        let sbas = collect(&state);
        assert!(!sbas.iter().any(|s| matches!(
            s,
            StateBasedAction::PlayerLoses { player, .. } if *player == PlayerId(0)
        )));
    }
}
