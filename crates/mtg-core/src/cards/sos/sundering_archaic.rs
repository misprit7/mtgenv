//! Sundering Archaic — `{6}` Creature — Avatar 3/3.
//!
//! Oracle: "Converge — When this creature enters, exile target nonland permanent an opponent controls
//! with mana value less than or equal to the number of colors of mana spent to cast this creature.
//! {2}: Put target card from a graveyard on the bottom of its owner's library."
//!
//! **Fully implemented** — a Converge ETB exile + a graveyard-hate activated ability:
//! - **ETB (Converge):** exile target nonland permanent an opponent controls whose mana value ≤
//!   `ColorsSpent` (the number of colors of mana spent to cast Sundering, read from its recorded
//!   `colors_spent`). The bound is the **dynamic** `CardFilter::ManaValueExpr{ max: ColorsSpent }` — it
//!   enumerates via the target-path dynamic-filter fix (`target_matches_filter` resolves it against a
//!   source-derived ctx).
//! - **`{2}`:** return target card from a graveyard to the bottom of its owner's library.

use crate::basics::{CardType, Color, Zone, ZoneDest, ZonePos};
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, Cost, EventPattern, Timing};
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const SUNDERING_ARCHAIC: u32 = 441;

pub fn register(db: &mut CardDb) {
    // Converge ETB: exile target nonland permanent an opponent controls with MV ≤ colors spent.
    let etb = Ability::Triggered {
        event: EventPattern::SelfEnters,
        condition: None,
        intervening_if: false,
        effect: Effect::Exile {
            what: EffectTarget::Target(TargetSpec {
                kind: TargetKind::Permanent(CardFilter::All(vec![
                    CardFilter::ControlledBy(PlayerRef::Opponent),
                    CardFilter::Not(Box::new(CardFilter::HasCardType(CardType::Land))),
                    CardFilter::ManaValueExpr {
                        min: None,
                        max: Some(Box::new(ValueExpr::ColorsSpent)),
                    },
                ])),
                min: 1,
                max: 1,
                distinct: true,
            }),
        },
    };
    // "{2}: Put target card from a graveyard on the bottom of its owner's library."
    let bottom = Ability::Activated {
        cost: Cost { mana: Some(mana_cost(2, &[])), components: vec![] },
        effect: Effect::MoveZone {
            what: EffectTarget::Target(TargetSpec {
                kind: TargetKind::CardInZone { zone: Zone::Graveyard, filter: CardFilter::Any },
                min: 1,
                max: 1,
                distinct: true,
            }),
            to: ZoneDest { zone: Zone::Library, pos: ZonePos::Bottom },
            tapped: false,
        },
        timing: Timing::Instant,
        restriction: None,
        is_mana: false,
    };
    let mut def = creature(
        SUNDERING_ARCHAIC,
        "Sundering Archaic",
        &[CreatureType::Avatar],
        Color::Colorless,
        mana_cost(6, &[]),
        3,
        3,
        vec![etb, bottom],
    );
    def.chars.colors = vec![]; // colorless (CR 105.2c)
    def.text = "Converge — When this creature enters, exile target nonland permanent an opponent controls with mana value less than or equal to the number of colors of mana spent to cast this creature.\n{2}: Put target card from a graveyard on the bottom of its owner's library.".to_string();
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
    fn sundering_shape() {
        let db = db_with_card();
        let def = db.get(SUNDERING_ARCHAIC).unwrap();
        assert_eq!((def.chars.power, def.chars.toughness), (Some(3), Some(3)));
        assert_eq!(def.chars.mana_value(), 6);
        assert!(def.fully_implemented);
        assert!(matches!(def.abilities[1], Ability::Activated { .. }));
    }

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

    /// Sundering entered having spent 2 colors of mana. Its Converge ETB may exile an opponent's nonland
    /// permanent with MV ≤ 2: the MV-2 Bears is offered (and exiled), the MV-4 Hill Giant is not.
    #[test]
    fn converge_etb_exiles_only_within_the_colors_spent_mv_bound() {
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(db_with_card()));
        let sundering = {
            let c = state.card_db().get(SUNDERING_ARCHAIC).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        // It was cast spending two colors of mana.
        state.objects.get_mut(&sundering).unwrap().colors_spent = 2;
        // Opponent (P1) controls a MV-2 Bears and a MV-4 Hill Giant.
        let bears = {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            state.add_card(PlayerId(1), c, Zone::Battlefield)
        };
        let giant = {
            let c = state.card_db().get(grp::HILL_GIANT).unwrap().chars.clone();
            state.add_card(PlayerId(1), c, Zone::Battlefield)
        };
        state.active_player = PlayerId(0);
        state.mark_chars_dirty();
        let offered = Rc::new(RefCell::new(Vec::new()));
        let mut e = Engine::new(
            state,
            vec![
                Box::new(CaptureAgent { pick: bears, offered: offered.clone() }),
                Box::new(CaptureAgent { pick: bears, offered: offered.clone() }),
            ],
        );
        // Fire Sundering's ETB (CR 603.6a — an enter is an ObjectMoved-to-battlefield event).
        e.broadcast(GameEvent::ObjectMoved { obj: sundering, to: Zone::Battlefield });
        loop {
            e.run_agenda();
            if e.state.stack.items.is_empty() {
                break;
            }
            e.resolve_top();
        }
        let legal = offered.borrow();
        assert!(legal.contains(&Target::Object(bears)), "the MV-2 Bears was an offered target (2 ≤ 2)");
        assert!(!legal.contains(&Target::Object(giant)), "the MV-4 Hill Giant was NOT (4 > 2)");
        assert_eq!(e.state.object(bears).zone, Zone::Exile, "the Bears was exiled");
        assert_eq!(e.state.object(giant).zone, Zone::Battlefield, "the Hill Giant stayed");
    }
}
