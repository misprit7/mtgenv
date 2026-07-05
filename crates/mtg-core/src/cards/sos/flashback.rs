//! Flashback — `{R}` Instant.
//!
//! Oracle: "Target instant or sorcery card in your graveyard gains flashback until end of turn. The
//! flashback cost is equal to its mana cost. (You may cast that card from your graveyard for its flashback
//! cost. Then exile it.)"
//!
//! **Fully implemented** — a single [`Effect::GrantFlashbackUntilEndOfTurn`] over a "target I/S card in
//! your graveyard." The grant sets the card's `flashback_until_turn`, which `flashback_cost` reads to offer
//! a flashback cast equal to its mana cost; the existing flashback path exiles it as it leaves the stack.

use crate::basics::{CardType, Color};
use crate::cards::helpers::instant_or_sorcery;
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::PlayerRef;
use crate::effects::{Effect, EffectTarget};

/// grp id (per-set ids live near their cards).
pub const FLASHBACK: u32 = 459;

pub fn register(db: &mut CardDb) {
    let effect = Effect::GrantFlashbackUntilEndOfTurn {
        what: EffectTarget::Target(TargetSpec {
            kind: TargetKind::CardInZone {
                zone: crate::basics::Zone::Graveyard,
                filter: CardFilter::All(vec![
                    instant_or_sorcery(),
                    CardFilter::ControlledBy(PlayerRef::Controller),
                ]),
            },
            min: 1,
            max: 1,
            distinct: true,
        }),
    };
    let def = spell(FLASHBACK, "Flashback", CardType::Instant, Color::Red, mana_cost(0, &[(Color::Red, 1)]), effect)
        .with_text("Target instant or sorcery card in your graveyard gains flashback until end of turn. The flashback cost is equal to its mana cost. (You may cast that card from your graveyard for its flashback cost. Then exile it.)");
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, CastVariant, DecisionRequest, DecisionResponse, PlayableAction, PlayerView, RandomAgent};
    use crate::basics::{Phase, Target, Zone};
    use crate::cards::{grp, starter_db};
    use crate::ids::{ObjId, PlayerId};
    use crate::priority::Engine;
    use crate::state::GameState;
    use std::sync::Arc;

    fn db_with_card() -> CardDb {
        let mut db = starter_db();
        register(&mut db);
        db
    }

    #[test]
    fn flashback_shape() {
        let db = db_with_card();
        let def = db.get(FLASHBACK).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Instant]);
        assert_eq!(def.chars.colors, vec![Color::Red]);
        assert!(matches!(def.spell_effect(), Some(Effect::GrantFlashbackUntilEndOfTurn { .. })));
    }

    /// Targets the graveyard bolt for the grant; passes otherwise.
    struct GrantAgent {
        bolt: ObjId,
    }
    impl Agent for GrantAgent {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::ChooseTargets { slots, .. } => {
                    let idx = slots[0]
                        .legal
                        .iter()
                        .position(|t| *t == Target::Object(self.bolt))
                        .unwrap_or(0);
                    DecisionResponse::Pairs(vec![(0, idx as u32)])
                }
                _ => DecisionResponse::Pass,
            }
        }
    }

    /// Real cast: grant flashback to a Lightning Bolt in your graveyard → a flashback offer for it appears
    /// (cost = its mana cost {R}); no such offer existed before.
    #[test]
    fn grants_flashback_to_a_graveyard_instant() {
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(db_with_card()));
        for _ in 0..2 {
            let m = state.card_db().get(grp::MOUNTAIN).unwrap().chars.clone();
            state.add_card(PlayerId(0), m, Zone::Battlefield);
        }
        let bolt = {
            let c = state.card_db().get(grp::LIGHTNING_BOLT).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Graveyard)
        };
        let fb = {
            let c = state.card_db().get(FLASHBACK).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Hand)
        };
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let mut e = Engine::new(state, vec![Box::new(GrantAgent { bolt }), Box::new(RandomAgent::new(1))]);

        // No flashback offer for the bolt before the grant.
        assert!(
            !e.legal_actions(PlayerId(0)).iter().any(
                |a| matches!(a, PlayableAction::Cast { spell, variant: CastVariant::Flashback } if *spell == bolt)
            ),
            "no flashback before the grant"
        );

        e.cast_spell(PlayerId(0), fb, CastVariant::Normal);
        e.resolve_top();
        assert_eq!(e.state.object(bolt).flashback_until_turn, Some(e.state.turn_number), "grant recorded");
        // Now the bolt can be flashback-cast from the graveyard (cost = its {R} mana cost).
        assert!(
            e.legal_actions(PlayerId(0)).iter().any(
                |a| matches!(a, PlayableAction::Cast { spell, variant: CastVariant::Flashback } if *spell == bolt)
            ),
            "flashback offered after the grant"
        );
    }
}
