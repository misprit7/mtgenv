//! Moseo, Vein's New Dean — `{2}{B}` Legendary Creature — Bird Skeleton Warlock 2/1.
//!
//! Oracle: "Flying
//! When Moseo enters, create a 1/1 black and green Pest creature token with 'Whenever this token
//! attacks, you gain 1 life.'
//! Infusion — At the beginning of your end step, if you gained life this turn, return up to one target
//! creature card with mana value X or less from your graveyard to the battlefield, where X is the amount
//! of life you gained this turn."
//!
//! **Fully implemented** — Flying + an ETB Pest token + the Infusion end-step reanimate. The reanimate
//! target is "creature card in your graveyard with mana value ≤ (life you gained this turn)", the
//! **dynamic** `CardFilter::ManaValueExpr{ max: LifeGainedThisTurn }`. This is the first card to prove
//! the target-path dynamic-filter fix: `target_matches_filter` now resolves such a bound against a
//! source-derived ctx, so the dynamic-MV target actually enumerates (previously it hit the fail-closed
//! arm and NEVER matched).

use crate::basics::{CardType, Color, Phase};
use crate::cards::helpers::pest_token;
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern, Keyword};
use crate::effects::condition::Condition;
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};
use crate::basics::{Zone, ZoneDest, ZonePos};
use crate::subtypes::{CreatureType, Supertype};

/// grp id (per-set ids live near their cards).
pub const MOSEO_VEINS_NEW_DEAN: u32 = 440;

pub fn register(db: &mut CardDb) {
    // "When Moseo enters, create a 1/1 B/G Pest token" (its attack-lifegain ability rides on the def).
    let etb = Ability::Triggered {
        event: EventPattern::SelfEnters,
        condition: None,
        intervening_if: false,
        effect: Effect::CreateToken {
            spec: pest_token(),
            count: ValueExpr::Fixed(1),
            controller: PlayerRef::Controller,
            dynamic_counters: vec![],
        },
    };
    // "Infusion — At the beginning of your end step, if you gained life this turn, return up to one
    // target creature card with mana value X or less from your graveyard … where X = life gained."
    let infusion = Ability::Triggered {
        event: EventPattern::BeginningOfStep(Phase::End),
        condition: Some(Condition::All(vec![
            Condition::YourTurn,
            Condition::ValueAtLeast(
                ValueExpr::LifeGainedThisTurn { who: PlayerRef::Controller },
                ValueExpr::Fixed(1),
            ),
        ])),
        intervening_if: true,
        effect: Effect::MoveZone {
            what: EffectTarget::Target(TargetSpec {
                kind: TargetKind::CardInZone {
                    zone: Zone::Graveyard,
                    filter: CardFilter::All(vec![
                        CardFilter::ControlledBy(PlayerRef::Controller),
                        CardFilter::HasCardType(CardType::Creature),
                        CardFilter::ManaValueExpr {
                            min: None,
                            max: Some(Box::new(ValueExpr::LifeGainedThisTurn { who: PlayerRef::Controller })),
                        },
                    ]),
                },
                min: 0,
                max: 1,
                distinct: true,
            }),
            to: ZoneDest { zone: Zone::Battlefield, pos: ZonePos::Any },
            tapped: false,
        },
    };
    let mut def = creature(
        MOSEO_VEINS_NEW_DEAN,
        "Moseo, Vein's New Dean",
        &[CreatureType::Bird, CreatureType::Skeleton, CreatureType::Warlock],
        Color::Black,
        mana_cost(2, &[(Color::Black, 1)]),
        2,
        1,
        vec![etb, infusion],
    );
    def.chars.supertypes = vec![Supertype::Legendary];
    def.chars.keywords = vec![Keyword::Flying];
    def.text = "Flying\nWhen Moseo enters, create a 1/1 black and green Pest creature token with \"Whenever this token attacks, you gain 1 life.\"\nInfusion — At the beginning of your end step, if you gained life this turn, return up to one target creature card with mana value X or less from your graveyard to the battlefield, where X is the amount of life you gained this turn.".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, DecisionRequest, DecisionResponse, GameEvent, PlayerView};
    use crate::basics::Target;
    use crate::cards::{grp, starter_db};
    use crate::ids::{ObjId, PlayerId};
    use crate::priority::Engine;
    use crate::state::GameState;
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::sync::Arc;

    fn db_with_card() -> CardDb {
        let mut db = starter_db();
        register(&mut db);
        db
    }

    #[test]
    fn moseo_shape() {
        let db = db_with_card();
        let def = db.get(MOSEO_VEINS_NEW_DEAN).unwrap();
        assert_eq!(def.chars.keywords, vec![Keyword::Flying]);
        assert_eq!(def.chars.supertypes, vec![Supertype::Legendary]);
        assert_eq!((def.chars.power, def.chars.toughness), (Some(2), Some(1)));
        assert!(def.fully_implemented);
    }

    /// Captures the legal targets it's offered, and reanimates the named creature.
    #[derive(Clone)]
    struct CaptureAgent {
        pick: ObjId,
        offered: Rc<RefCell<Vec<Target>>>,
    }
    impl Agent for CaptureAgent {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::ChooseTargets { slots, .. } => {
                    *self.offered.borrow_mut() = slots[0].legal.clone();
                    match slots[0].legal.iter().position(|t| *t == Target::Object(self.pick)) {
                        Some(i) => DecisionResponse::Pairs(vec![(0, i as u32)]),
                        None => DecisionResponse::Pairs(vec![]),
                    }
                }
                _ => DecisionResponse::Pass,
            }
        }
    }

    /// P0 gained 3 life this turn; graveyard holds a Grizzly Bears (MV 2 ≤ 3) and a Hill Giant (MV 4 > 3).
    /// Firing Moseo's end-step Infusion offers ONLY the Bears (dynamic-MV bound enumerates correctly) and
    /// reanimates it; the Hill Giant stays in the graveyard.
    #[test]
    fn infusion_reanimates_only_within_the_life_gained_mv_bound() {
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(db_with_card()));
        {
            let c = state.card_db().get(MOSEO_VEINS_NEW_DEAN).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield);
        }
        let bears = {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Graveyard)
        };
        let giant = {
            let c = state.card_db().get(grp::HILL_GIANT).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Graveyard)
        };
        // P0 gained 3 life this turn.
        state.player_mut(PlayerId(0)).life_gained_this_turn = 3;
        state.active_player = PlayerId(0);
        state.phase = Phase::End;
        state.mark_chars_dirty();
        let offered = Rc::new(RefCell::new(Vec::new()));
        let mut e = Engine::new(
            state,
            vec![
                Box::new(CaptureAgent { pick: bears, offered: offered.clone() }),
                Box::new(CaptureAgent { pick: bears, offered: offered.clone() }),
            ],
        );
        // Fire the begin-end-step trigger (mirrors primary_research's end-step test).
        e.broadcast(GameEvent::PhaseBegan { turn: 1, phase: Phase::End, active: PlayerId(0) });
        loop {
            e.run_agenda();
            if e.state.stack.items.is_empty() {
                break;
            }
            e.resolve_top();
        }
        let legal = offered.borrow();
        assert!(legal.contains(&Target::Object(bears)), "the MV-2 Bears was an offered target");
        assert!(!legal.contains(&Target::Object(giant)), "the MV-4 Hill Giant was NOT (4 > life gained 3)");
        assert_eq!(e.state.object(bears).zone, Zone::Battlefield, "the Bears was reanimated");
        assert_eq!(e.state.object(giant).zone, Zone::Graveyard, "the Hill Giant stayed in the graveyard");
    }
}
