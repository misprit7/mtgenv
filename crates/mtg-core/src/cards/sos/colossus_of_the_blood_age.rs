//! Colossus of the Blood Age — `{4}{R}{W}` Artifact Creature — Construct 6/6 (first printed SOS).
//!
//! Oracle: "When this creature enters, it deals 3 damage to each opponent and you gain 3 life. / When
//! this creature dies, discard any number of cards, then draw that many cards plus one."
//!
//! **Fully implemented**: the ETB — 3 damage to each opponent + gain 3 life — plus the dies clause
//! "discard any number of cards, then draw that many cards plus one" over the new
//! [`Effect::DiscardChosen`] (player picks how many + which) and [`ValueExpr::DiscardedThisResolution`]
//! (the count captured by the discard scratch this resolution). The draw is `Sum(discarded, 1)`, so
//! even discarding zero draws one.

use crate::basics::{CardType, Color, DamageKind};
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const COLOSSUS_OF_THE_BLOOD_AGE: u32 = 314;

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        COLOSSUS_OF_THE_BLOOD_AGE,
        "Colossus of the Blood Age",
        &[CreatureType::Construct],
        Color::Red,
        mana_cost(4, &[(Color::Red, 1), (Color::White, 1)]),
        6,
        6,
        vec![
            Ability::Triggered {
                event: EventPattern::SelfEnters,
                condition: None,
                intervening_if: false,
                effect: Effect::Sequence(vec![
                    Effect::DealDamage {
                        amount: ValueExpr::Fixed(3),
                        to: EffectTarget::Player(PlayerRef::EachOpponent),
                        kind: DamageKind::Noncombat,
                    },
                    Effect::GainLife { who: PlayerRef::Controller, amount: ValueExpr::Fixed(3) },
                ]),
            },
            // "When this creature dies, discard any number of cards, then draw that many cards plus one."
            Ability::Triggered {
                event: EventPattern::SelfDies,
                condition: None,
                intervening_if: false,
                effect: Effect::Sequence(vec![
                    Effect::DiscardChosen { who: PlayerRef::Controller },
                    Effect::Draw {
                        who: PlayerRef::Controller,
                        count: ValueExpr::Sum(
                            Box::new(ValueExpr::DiscardedThisResolution),
                            Box::new(ValueExpr::Fixed(1)),
                        ),
                    },
                ]),
            },
        ],
    );
    def.chars.card_types = vec![CardType::Artifact, CardType::Creature];
    def.chars.colors = vec![Color::Red, Color::White];
    def.text = "When this creature enters, it deals 3 damage to each opponent and you gain 3 life.\nWhen this creature dies, discard any number of cards, then draw that many cards plus one.".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colossus_shape() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(COLOSSUS_OF_THE_BLOOD_AGE).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Artifact, CardType::Creature]);
        assert_eq!(def.chars.colors, vec![Color::Red, Color::White]);
        assert!(def.fully_implemented, "ETB + dies clause both implemented");
        assert!(matches!(def.abilities[1], Ability::Triggered { event: EventPattern::SelfDies, .. }));
    }

    /// Behaviour: the ETB deals 3 to the opponent and gains its controller 3 life.
    #[test]
    fn colossus_etb_drains_opponent_and_gains_life() {
        use crate::agent::RandomAgent;
        use crate::basics::Zone;
        use crate::cards::build_game;
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        let mut state = build_game(1, &[&[], &[]]);
        let src = state.add_card(PlayerId(0), state.card_db().get(COLOSSUS_OF_THE_BLOOD_AGE).unwrap().chars.clone(), Zone::Battlefield);
        let etb = match &state.card_db().get(COLOSSUS_OF_THE_BLOOD_AGE).unwrap().abilities[0] {
            Ability::Triggered { effect, .. } => effect.clone(),
            o => panic!("expected ETB, got {o:?}"),
        };
        let (my_life, opp_life) = (state.player(PlayerId(0)).life, state.player(PlayerId(1)).life);
        let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
        e.resolve_effect(
            &etb,
            &ResolutionCtx { controller: Some(PlayerId(0)), source: Some(src), ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        assert_eq!(e.state.player(PlayerId(1)).life, opp_life - 3, "3 damage to the opponent");
        assert_eq!(e.state.player(PlayerId(0)).life, my_life + 3, "gained 3 life");
    }

    /// Discards a fixed number of cards from the front of the hand (`n`), else defaults.
    #[derive(Clone)]
    struct DiscardNAgent {
        n: usize,
    }
    impl crate::agent::Agent for DiscardNAgent {
        fn decide(
            &mut self,
            _v: &crate::agent::PlayerView,
            req: &crate::agent::DecisionRequest,
        ) -> crate::agent::DecisionResponse {
            match req {
                crate::agent::DecisionRequest::SelectCards { from, .. } => {
                    let idxs = (0..self.n.min(from.len()) as u32).collect();
                    crate::agent::DecisionResponse::Indices(idxs)
                }
                _ => crate::agent::DecisionResponse::Pass,
            }
        }
    }

    /// Dies clause: discard 2 of the 4 cards in hand, then draw 2 + 1 = 3. Net hand = 4 − 2 + 3 = 5.
    #[test]
    fn colossus_dies_discards_then_draws_that_many_plus_one() {
        use crate::basics::Zone;
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        use crate::state::GameState;
        use crate::cards::grp;
        use std::sync::Arc;

        let mut db = CardDb::default();
        register(&mut db);
        // Reuse the starter db for Bears (hand/library filler).
        let full = crate::cards::starter_db();
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(full));
        let src = state.add_card(
            PlayerId(0),
            state.card_db().get(COLOSSUS_OF_THE_BLOOD_AGE).unwrap().chars.clone(),
            Zone::Battlefield,
        );
        for _ in 0..4 {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Hand);
        }
        for _ in 0..10 {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Library);
        }
        let dies = match &state.card_db().get(COLOSSUS_OF_THE_BLOOD_AGE).unwrap().abilities[1] {
            Ability::Triggered { effect, .. } => effect.clone(),
            o => panic!("expected dies clause, got {o:?}"),
        };
        let mut e = Engine::new(
            state,
            vec![Box::new(DiscardNAgent { n: 2 }), Box::new(DiscardNAgent { n: 2 })],
        );
        e.resolve_effect(
            &dies,
            &ResolutionCtx { controller: Some(PlayerId(0)), source: Some(src), ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        assert_eq!(e.state.player(PlayerId(0)).hand.len(), 5, "4 − 2 discarded + 3 drawn = 5");
        assert_eq!(e.state.player(PlayerId(0)).graveyard.len(), 2, "the 2 discarded cards");
    }
}
