//! State-based actions (CR 704), checked and applied to a fixpoint within the agenda
//! loop before any player receives priority (CR 117.5, 704.3).
//!
//! Implemented: the three player-loss checks (704.5a–c, esp. decking) and, since milestone
//! 3, the two creature-death checks — toughness ≤ 0 (704.5f) and lethal marked damage
//! (704.5g). The legend rule, planeswalker loyalty, auras, counters, etc. arrive later.
//!
//! [`collect`] is a pure read of the state — it never mutates. The engine performs the
//! returned actions "simultaneously as a single event" (704.3) and repeats until none
//! apply.

use serde::{Deserialize, Serialize};

use crate::basics::Zone;
use crate::ids::{ObjId, PlayerId};
use crate::state::GameState;

/// Why a player loses the game (CR 704.5a–c). Serde-able because the engine records the
/// game's ending reason in `GameState` for the `Outcome` (a snapshot field).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LossReason {
    /// 704.5a — life total 0 or less.
    ZeroOrLessLife,
    /// 704.5b — attempted to draw from an empty library since the last SBA check.
    DrewFromEmptyLibrary,
    /// 704.5c — ten or more poison counters.
    TenPoison,
}

/// Why a creature is put into its owner's graveyard by an SBA (CR 704.5f/g/h).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeathReason {
    /// 704.5f — toughness 0 or less.
    ZeroToughness,
    /// 704.5g — marked damage greater than or equal to (positive) toughness.
    LethalDamage,
    /// 704.5h — dealt damage by a deathtouch source (any amount is lethal).
    Deathtouch,
}

/// A state-based action that needs performing. The enum grows as more SBAs are implemented.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateBasedAction {
    PlayerLoses {
        player: PlayerId,
        reason: LossReason,
    },
    /// A creature is put into its owner's graveyard (CR 704.5f/g). (Regeneration / "destroy"
    /// vs "put into graveyard" distinctions are deferred — milestone 3 has no replacements.)
    CreatureDies {
        creature: ObjId,
        reason: DeathReason,
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
    // Creature-death SBAs (CR 704.5f/g). Toughness/type come from the computed (layered)
    // characteristics (CR 613) — so anthems, set-base effects and counters all count.
    for o in state.objects.values() {
        if o.zone != Zone::Battlefield {
            continue;
        }
        let cc = state.computed(o.id);
        if !cc.is_creature() {
            continue;
        }
        let toughness = cc.toughness.unwrap_or(0);
        // Indestructible (CR 702.12) prevents destruction by lethal damage / deathtouch, but
        // NOT the toughness-0 SBA (704.5f).
        let indestructible = cc.has_keyword(crate::effects::ability::Keyword::Indestructible);
        if toughness <= 0 {
            out.push(StateBasedAction::CreatureDies {
                creature: o.id,
                reason: DeathReason::ZeroToughness,
            });
        } else if indestructible {
            // can't be destroyed
        } else if o.dealt_deathtouch {
            out.push(StateBasedAction::CreatureDies {
                creature: o.id,
                reason: DeathReason::Deathtouch,
            });
        } else if o.damage_marked >= toughness as u32 {
            out.push(StateBasedAction::CreatureDies {
                creature: o.id,
                reason: DeathReason::LethalDamage,
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
