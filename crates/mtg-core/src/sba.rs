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

use crate::basics::{CardType, CounterKind, Zone};
use crate::ids::{ObjId, PlayerId};
use crate::state::GameState;
use crate::subtypes::{ArtifactType, EnchantmentType, Subtype, Supertype};

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
    /// CR 704.5 (Auras): an Aura attached to an illegal object, or not attached at all, is put
    /// into its owner's graveyard.
    AuraFallsOff {
        aura: ObjId,
    },
    /// CR 704.5 (Equipment): an Equipment attached to an illegal permanent (one that isn't a
    /// creature) becomes unattached but stays on the battlefield. (A host *leaving* already
    /// unattaches it via `move_object`; this covers a host that stops being a creature.)
    EquipmentUnattaches {
        equipment: ObjId,
    },
    /// CR 704.5i: a planeswalker with 0 loyalty is put into its owner's graveyard.
    PlaneswalkerDies {
        pw: ObjId,
    },
    /// CR 111.7 / 704.5d: a **token** in a zone other than the battlefield ceases to exist. A token
    /// that dies / falls off / is sacrificed first moves to its owner's graveyard (so "dies"-triggers
    /// and last-known-information see it), then this SBA removes it. Detected by the `Supertype::Token`
    /// stamp; the stack is excluded (a resolving token spell-copy is handled by `is_copy` cease-to-exist).
    TokenCeasesToExist {
        token: ObjId,
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
        // "You can't lose the game this turn" (CR 720.6 — Angel's Grace): suppress this player's loss
        // SBAs (704.5a/b/c) entirely while the flag is set.
        if p.cant_lose_this_turn {
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
    // Aura attachment SBA (CR 704.5): an Aura must be attached to a legal object — first pass,
    // a creature on the battlefield (the starter set's auras all "enchant creature"). An Aura
    // not attached, or whose host is gone / no longer a creature, is put into its owner's
    // graveyard. (The host being destroyed unattaches the Aura first, via `move_object`, so the
    // creature's death and the Aura's fall-off resolve in successive SBA iterations.)
    for o in state.objects.values() {
        if o.zone != Zone::Battlefield {
            continue;
        }
        if !o.chars.subtypes.contains(&Subtype::Enchantment(EnchantmentType::Aura)) {
            continue;
        }
        let legal_host = o.attached_to.is_some_and(|h| {
            state
                .objects
                .get(&h)
                .is_some_and(|ho| ho.zone == Zone::Battlefield)
                && state.computed(h).is_creature()
        });
        if !legal_host {
            out.push(StateBasedAction::AuraFallsOff { aura: o.id });
        }
    }
    // Equipment attachment SBA (CR 704.5): an Equipment attached to a non-creature permanent
    // becomes unattached (but stays). `attached_to` being `Some` implies the host is still on
    // the battlefield (a host leaving clears the link in `move_object`).
    for o in state.objects.values() {
        if o.zone != Zone::Battlefield {
            continue;
        }
        if !o.chars.subtypes.contains(&Subtype::Artifact(ArtifactType::Equipment)) {
            continue;
        }
        if let Some(h) = o.attached_to {
            let host_ok = state
                .objects
                .get(&h)
                .is_some_and(|ho| ho.zone == Zone::Battlefield)
                && state.computed(h).is_creature();
            if !host_ok {
                out.push(StateBasedAction::EquipmentUnattaches { equipment: o.id });
            }
        }
    }
    // Planeswalker loyalty SBA (CR 704.5i): a planeswalker with 0 loyalty is put into its
    // owner's graveyard.
    for o in state.objects.values() {
        if o.zone != Zone::Battlefield {
            continue;
        }
        if !state.computed(o.id).card_types.contains(&CardType::Planeswalker) {
            continue;
        }
        if o.counters.get(&CounterKind::Loyalty) == 0 {
            out.push(StateBasedAction::PlaneswalkerDies { pw: o.id });
        }
    }
    // Token cease-to-exist SBA (CR 111.7 / 704.5d): a token that has left the battlefield (to a
    // graveyard, exile, hand, or library) ceases to exist. Excludes the stack — a resolving token
    // spell-copy is removed via its `is_copy` flag, not here. The `Supertype::Token` stamp
    // (`whiteboard::create_token`) is the detector.
    for o in state.objects.values() {
        if o.zone == Zone::Battlefield || o.zone == Zone::Stack {
            continue;
        }
        if o.chars.supertypes.contains(&Supertype::Token) {
            out.push(StateBasedAction::TokenCeasesToExist { token: o.id });
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
