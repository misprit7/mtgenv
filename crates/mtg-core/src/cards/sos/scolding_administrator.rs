//! Scolding Administrator — `{W}{B}` Creature — Dwarf Cleric 2/2.
//!
//! Oracle: "Menace (This creature can't be blocked except by two or more creatures.)
//! Repartee — Whenever you cast an instant or sorcery spell that targets a creature, put a +1/+1
//! counter on this creature.
//! When this creature dies, if it had counters on it, put those counters on up to one target creature."
//!
//! **Fully implemented** — Menace keyword + a Repartee cast-trigger putting a +1/+1 counter on itself,
//! plus a `SelfDies` trigger that moves "those counters" onto up to one target creature. The dies clause
//! reads `ValueExpr::CountersOnSelf` — backed by the last-known counter bag (CR 603.10a) captured when
//! the creature left the battlefield — both as the intervening-if gate ("if it had counters") and as the
//! number of +1/+1 counters to put on the target. Since Scolding only ever accrues +1/+1 counters, "those
//! counters" is exactly that many +1/+1 counters.

use crate::basics::{CardType, Color, CounterKind};
use crate::cards::helpers::instant_or_sorcery;
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern, Keyword};
use crate::effects::condition::Condition;
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::ValueExpr;
use crate::effects::{Effect, EffectTarget};
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const SCOLDING_ADMINISTRATOR: u32 = 437;

pub fn register(db: &mut CardDb) {
    // "Repartee — Whenever you cast an I/S spell that targets a creature, put a +1/+1 counter on this."
    let repartee = Ability::Triggered {
        event: EventPattern::SpellCastTargetingCreature(instant_or_sorcery()),
        condition: None,
        intervening_if: false,
        effect: Effect::PutCounters {
            what: EffectTarget::SourceSelf,
            kind: CounterKind::PlusOnePlusOne,
            n: ValueExpr::Fixed(1),
        },
    };
    // "When this dies, if it had counters on it, put those counters on up to one target creature."
    let dies = Ability::Triggered {
        event: EventPattern::SelfDies,
        condition: Some(Condition::ValueAtLeast(
            ValueExpr::CountersOnSelf(CounterKind::PlusOnePlusOne),
            ValueExpr::Fixed(1),
        )),
        intervening_if: true,
        effect: Effect::PutCounters {
            what: EffectTarget::Target(TargetSpec {
                kind: TargetKind::Creature(CardFilter::Any),
                min: 0,
                max: 1,
                distinct: true,
            }),
            kind: CounterKind::PlusOnePlusOne,
            n: ValueExpr::CountersOnSelf(CounterKind::PlusOnePlusOne),
        },
    };
    let mut def = creature(
        SCOLDING_ADMINISTRATOR,
        "Scolding Administrator",
        &[CreatureType::Dwarf, CreatureType::Cleric],
        Color::White,
        mana_cost(0, &[(Color::White, 1), (Color::Black, 1)]),
        2,
        2,
        vec![repartee, dies],
    );
    def.chars.colors = vec![Color::White, Color::Black];
    def.chars.keywords = vec![Keyword::Menace];
    def.text = "Menace\nRepartee — Whenever you cast an instant or sorcery spell that targets a creature, put a +1/+1 counter on this creature.\nWhen this creature dies, if it had counters on it, put those counters on up to one target creature.".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, CastVariant, DecisionRequest, DecisionResponse, PlayerView};
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
    fn scolding_shape() {
        let db = db_with_card();
        let def = db.get(SCOLDING_ADMINISTRATOR).unwrap();
        assert_eq!(def.chars.keywords, vec![Keyword::Menace]);
        assert_eq!(def.chars.colors, vec![Color::White, Color::Black]);
        assert_eq!((def.chars.power, def.chars.toughness), (Some(2), Some(2)));
        assert!(def.fully_implemented);
        assert!(matches!(def.abilities[1], Ability::Triggered { event: EventPattern::SelfDies, .. }));
    }

    /// Points a dies-trigger's optional creature target at the named creature.
    #[derive(Clone)]
    struct TargetAgent {
        recipient: ObjId,
    }
    impl Agent for TargetAgent {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::ChooseTargets { slots, .. } => {
                    let idx = slots[0]
                        .legal
                        .iter()
                        .position(|t| *t == Target::Object(self.recipient));
                    match idx {
                        Some(i) => DecisionResponse::Pairs(vec![(0, i as u32)]),
                        None => DecisionResponse::Pairs(vec![]),
                    }
                }
                _ => DecisionResponse::Pass,
            }
        }
    }

    fn kill_and_settle(e: &mut Engine, obj: ObjId) {
        let t = e.state.computed(obj).toughness.unwrap_or(1).max(1) as u32;
        e.state.objects.get_mut(&obj).unwrap().damage_marked = t;
        e.state.mark_chars_dirty();
        e.run_agenda();
        while !e.state.stack.is_empty() {
            e.resolve_top();
            e.run_agenda();
        }
    }

    /// Scolding with three +1/+1 counters dies; the three counters land on a target Bears (2/2 → 5/5).
    #[test]
    fn dies_moves_its_counters_to_a_target_creature() {
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(db_with_card()));
        let scold = {
            let c = state.card_db().get(SCOLDING_ADMINISTRATOR).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        if let Some(o) = state.objects.get_mut(&scold) {
            o.counters.counts.insert(CounterKind::PlusOnePlusOne, 3);
        }
        let recipient = {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        state.mark_chars_dirty();
        let mut e = Engine::new(
            state,
            vec![Box::new(TargetAgent { recipient }), Box::new(TargetAgent { recipient })],
        );
        kill_and_settle(&mut e, scold);
        assert_eq!(
            e.state.object(recipient).counters.get(&CounterKind::PlusOnePlusOne),
            3,
            "the three counters moved onto the Bears"
        );
        assert_eq!(e.state.computed(recipient).power, Some(5), "2/2 + three +1/+1 = 5/5");
    }
}
