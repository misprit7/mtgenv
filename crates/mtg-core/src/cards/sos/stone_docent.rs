//! Stone Docent — `{1}{W}` Creature — Spirit Chimera 3/1 (first printed SOS).
//!
//! Oracle: "{W}, Exile this card from your graveyard: You gain 2 life. Surveil 1. Activate only as a
//! sorcery."
//!
//! **Fully implemented** — a vanilla 3/1 with a **sorcery-speed graveyard-activated** ability (`{W}` +
//! exile this from the graveyard → gain 2, surveil 1).

use crate::basics::Color;
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, Cost, CostComponent, Timing};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const STONE_DOCENT: u32 = 298;

pub fn register(db: &mut CardDb) {
    db.insert(
        creature(
            STONE_DOCENT,
            "Stone Docent",
            &[CreatureType::Spirit, CreatureType::Chimera],
            Color::White,
            mana_cost(1, &[(Color::White, 1)]),
            3,
            1,
            vec![Ability::Activated {
                cost: Cost {
                    mana: Some(mana_cost(0, &[(Color::White, 1)])),
                    components: vec![CostComponent::ExileSelfFromGraveyard],
                },
                effect: Effect::Sequence(vec![
                    Effect::GainLife { who: PlayerRef::Controller, amount: ValueExpr::Fixed(2) },
                    Effect::Surveil { count: ValueExpr::Fixed(1) },
                ]),
                timing: Timing::Sorcery,
                restriction: None,
                is_mana: false,
            }],
        )
        .with_text("{W}, Exile this card from your graveyard: You gain 2 life. Surveil 1. Activate only as a sorcery."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stone_docent_is_sorcery_speed_graveyard_activated() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(STONE_DOCENT).unwrap();
        assert!(def.fully_implemented);
        match &def.abilities[0] {
            Ability::Activated { cost, timing, .. } => {
                assert!(cost.components.iter().any(|c| matches!(c, CostComponent::ExileSelfFromGraveyard)));
                assert!(matches!(timing, Timing::Sorcery), "activate only as a sorcery");
            }
            o => panic!("expected Activated, got {o:?}"),
        }
    }

    /// S18 end-to-end: with the card in your graveyard and a Plains for the `{W}`, the ability is
    /// offered from the graveyard; activating it exiles the card, and after it resolves you've
    /// gained 2 life.
    #[test]
    fn stone_docent_activates_from_graveyard() {
        use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayableAction, PlayerView, SelectReason};
        use crate::basics::{Phase, Zone};
        use crate::cards::{build_game, grp};
        use crate::ids::PlayerId;
        use crate::priority::Engine;
        #[derive(Clone)]
        struct GyAgent;
        impl Agent for GyAgent {
            fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
                match req {
                    // Surveil 1: keep on top (bin nothing).
                    DecisionRequest::SelectCards { reason: SelectReason::ScryStage, .. } => DecisionResponse::Indices(vec![]),
                    _ => DecisionResponse::Pass,
                }
            }
        }
        let lib: Vec<u32> = std::iter::repeat(grp::FOREST).take(2).collect();
        let mut state = build_game(1, &[&lib, &[]]);
        state.active_player = PlayerId(0);
        let card = state.add_card(PlayerId(0), state.card_db().get(STONE_DOCENT).unwrap().chars.clone(), Zone::Graveyard);
        state.add_card(PlayerId(0), state.card_db().get(grp::PLAINS).unwrap().chars.clone(), Zone::Battlefield);
        let mut e = Engine::new(state, vec![Box::new(GyAgent), Box::new(GyAgent)]);
        e.state.phase = Phase::PrecombatMain;
        // Offered from the graveyard.
        let act = e.legal_actions(PlayerId(0)).into_iter().find(|a| {
            matches!(a, PlayableAction::Activate { source, .. } if *source == card)
        });
        assert!(act.is_some(), "graveyard-activated ability offered");
        // Activate it, then resolve the ability off the stack.
        let life0 = e.state.player(PlayerId(0)).life;
        if let Some(PlayableAction::Activate { source, ability }) = act {
            e.activate_ability(PlayerId(0), source, ability);
        }
        assert!(e.state.players[0].exile.contains(&card), "the card was exiled as the cost");
        e.resolve_top();
        assert_eq!(e.state.player(PlayerId(0)).life, life0 + 2, "gained 2 life on resolution");
    }
}
