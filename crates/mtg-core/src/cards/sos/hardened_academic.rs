//! Hardened Academic — `{R}{W}` Creature — Bird Cleric 2/1 (first printed SOS).
//!
//! Oracle: "Flying, haste / Discard a card: This creature gains lifelink until end of turn. /
//! Whenever one or more cards leave your graveyard, put a +1/+1 counter on target creature you
//! control."
//!
//! **Fully implemented** — printed Flying + Haste; a `{Discard a card}` activated ability granting
//! itself Lifelink until end of turn (lifelink is live in `apply_damage`, CR 702.15); and the shared
//! S9 `CardsLeaveYourGraveyard` trigger putting a +1/+1 counter on a target creature you control.

use crate::basics::{Color, CounterKind};
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, Cost, CostComponent, EventPattern, Keyword, Timing};
use crate::effects::condition::Duration;
use crate::effects::target::{CardFilter, SelectSpec, TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const HARDENED_ACADEMIC: u32 = 341;

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        HARDENED_ACADEMIC,
        "Hardened Academic",
        &[CreatureType::Bird, CreatureType::Cleric],
        Color::White,
        mana_cost(0, &[(Color::Red, 1), (Color::White, 1)]),
        2,
        1,
        vec![
            // "Discard a card: This creature gains lifelink until end of turn."
            Ability::Activated {
                cost: Cost {
                    mana: None,
                    components: vec![CostComponent::Discard(SelectSpec {
                        zone: crate::basics::Zone::Hand,
                        filter: CardFilter::Any,
                        chooser: PlayerRef::Controller,
                        min: ValueExpr::Fixed(1),
                        max: ValueExpr::Fixed(1),
                    })],
                },
                effect: Effect::GrantKeyword {
                    what: EffectTarget::SourceSelf,
                    keyword: Keyword::Lifelink,
                    duration: Duration::UntilEndOfTurn,
                },
                timing: Timing::Instant,
                restriction: None,
                is_mana: false,
            },
            // "Whenever one or more cards leave your graveyard, put a +1/+1 counter on target
            // creature you control." (S9 CardsLeaveYourGraveyard.)
            Ability::Triggered {
                event: EventPattern::CardsLeaveYourGraveyard,
                condition: None,
                intervening_if: false,
                effect: Effect::PutCounters {
                    what: EffectTarget::Target(TargetSpec {
                        kind: TargetKind::Creature(CardFilter::ControlledBy(PlayerRef::Controller)),
                        min: 1,
                        max: 1,
                        distinct: true,
                    }),
                    kind: CounterKind::PlusOnePlusOne,
                    n: ValueExpr::Fixed(1),
                },
            },
        ],
    );
    def.chars.colors = vec![Color::Red, Color::White];
    def.chars.keywords = vec![Keyword::Flying, Keyword::Haste];
    def.text = "Flying, haste\nDiscard a card: This creature gains lifelink until end of turn.\nWhenever one or more cards leave your graveyard, put a +1/+1 counter on target creature you control.".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, AbilityRef, DecisionRequest, DecisionResponse, PlayerView, SelectReason};
    use crate::basics::{Target, Zone};
    use crate::cards::{grp, starter_db};
    use crate::ids::PlayerId;
    use crate::priority::Engine;
    use crate::state::GameState;
    use std::sync::Arc;

    /// Discards the first offered card; picks the first target; passes otherwise.
    #[derive(Clone)]
    struct Doer;
    impl Agent for Doer {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::SelectCards { reason: SelectReason::Discard, from, min, .. } => {
                    DecisionResponse::Indices((0..(*min).min(from.len() as u32)).collect())
                }
                DecisionRequest::ChooseTargets { slots, .. } => {
                    DecisionResponse::Pairs(vec![(0, if slots[0].legal.is_empty() { 0 } else { 0 })])
                }
                _ => DecisionResponse::Pass,
            }
        }
    }

    #[test]
    fn hardened_academic_shape() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(HARDENED_ACADEMIC).unwrap();
        assert_eq!((def.chars.power, def.chars.toughness), (Some(2), Some(1)));
        assert_eq!(def.chars.keywords, vec![Keyword::Flying, Keyword::Haste]);
        assert!(def.fully_implemented);
    }

    /// "Discard a card: gains lifelink until end of turn." — activating it discards a card and grants
    /// Lifelink (read from the computed keyword set, so it's live in `apply_damage`).
    #[test]
    fn discard_grants_lifelink() {
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(starter_db()));
        let bird = {
            let c = state.card_db().get(HARDENED_ACADEMIC).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        {
            let c = state.card_db().get(grp::FOREST).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Hand); // a card to discard
        }
        assert!(!state.computed(bird).has_keyword(Keyword::Lifelink), "no lifelink before");
        let mut e = Engine::new(state, vec![Box::new(Doer), Box::new(Doer)]);
        e.activate_ability(PlayerId(0), bird, AbilityRef(0));
        e.run_agenda();
        while !e.state.stack.items.is_empty() {
            e.resolve_top();
            e.run_agenda();
        }
        assert_eq!(e.state.player(PlayerId(0)).hand.len(), 0, "the card was discarded as the cost");
        assert!(
            e.state.computed(bird).has_keyword(Keyword::Lifelink),
            "the ability granted lifelink until end of turn"
        );
    }

    /// S9 through the real engine: when a card leaves your graveyard, put a +1/+1 counter on a target
    /// creature you control. Drive it by exiling a graveyard card (a MoveZone off the graveyard),
    /// which fires the LeftGraveyard event.
    #[test]
    fn graveyard_leave_puts_counter() {
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::StackId;
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(starter_db()));
        let bird = {
            let c = state.card_db().get(HARDENED_ACADEMIC).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        // A card in P0's graveyard to move out (firing "cards leave your graveyard").
        let gy = {
            let c = state.card_db().get(grp::FOREST).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Graveyard)
        };
        let mut e = Engine::new(state, vec![Box::new(Doer), Box::new(Doer)]);
        // Move the graveyard card to exile → LeftGraveyard fires → the trigger targets the Bird.
        e.resolve_effect(
            &Effect::MoveZone {
                what: EffectTarget::Target(TargetSpec {
                    kind: TargetKind::CardInZone {
                        zone: Zone::Graveyard,
                        filter: CardFilter::ControlledBy(PlayerRef::Controller),
                    },
                    min: 1,
                    max: 1,
                    distinct: true,
                }),
                to: crate::basics::ZoneDest { zone: Zone::Exile, pos: crate::basics::ZonePos::Any },
                tapped: false,
            },
            &ResolutionCtx {
                controller: Some(PlayerId(0)),
                chosen_targets: vec![Target::Object(gy)],
                ..Default::default()
            },
            WbReason::Resolve(StackId(0)),
        );
        e.run_agenda();
        while !e.state.stack.items.is_empty() {
            e.resolve_top();
            e.run_agenda();
        }
        assert_eq!(
            e.state.object(bird).counters.get(&CounterKind::PlusOnePlusOne),
            1,
            "a card left the graveyard → +1/+1 counter on the target creature"
        );
    }
}
