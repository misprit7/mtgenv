//! Pest Mascot — `{1}{B}{G}` Creature — Pest Ape 2/3 (first printed SOS).
//!
//! Oracle: "Trample / Whenever you gain life, put a +1/+1 counter on this creature."
//!
//! **Fully implemented** — printed Trample plus a `GainLife` triggered ability (fires once per
//! life-gain event) that puts a +1/+1 counter on itself. Multicolored (B/G).

use crate::basics::{Color, CounterKind};
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern, Keyword};
use crate::effects::value::ValueExpr;
use crate::effects::{Effect, EffectTarget};
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const PEST_MASCOT: u32 = 241;

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        PEST_MASCOT,
        "Pest Mascot",
        &[CreatureType::Pest, CreatureType::Ape],
        Color::Black,
        mana_cost(1, &[(Color::Black, 1), (Color::Green, 1)]),
        2,
        3,
        vec![Ability::Triggered {
            event: EventPattern::GainLife,
            condition: None,
            intervening_if: false,
            effect: Effect::PutCounters {
                what: EffectTarget::SourceSelf,
                kind: CounterKind::PlusOnePlusOne,
                n: ValueExpr::Fixed(1),
            },
        }],
    );
    def.chars.colors = vec![Color::Black, Color::Green];
    def.chars.keywords = vec![Keyword::Trample];
    def.text = "Trample\nWhenever you gain life, put a +1/+1 counter on this creature.".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn pest_mascot_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(PEST_MASCOT).unwrap();
        assert_eq!(def.chars.keywords, vec![Keyword::Trample]);
        assert!(def.fully_implemented);
        expect![[r#"
            [
                Triggered {
                    event: GainLife,
                    condition: None,
                    intervening_if: false,
                    effect: PutCounters {
                        what: SourceSelf,
                        kind: PlusOnePlusOne,
                        n: Fixed(
                            1,
                        ),
                    },
                },
            ]"#]].assert_eq(&format!("{:#?}", def.abilities));
    }

    /// Behaviour (end-to-end): gaining life fires the trigger through the real agenda → a +1/+1
    /// counter lands on Pest Mascot (2/3 → 3/4).
    #[test]
    fn pest_mascot_grows_when_you_gain_life() {
        use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView};
        use crate::basics::Zone;
        use crate::cards::build_game;
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::effects::value::PlayerRef;
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;

        #[derive(Clone)]
        struct PassiveAgent;
        impl Agent for PassiveAgent {
            fn decide(&mut self, _v: &PlayerView, _req: &DecisionRequest) -> DecisionResponse {
                DecisionResponse::Pass
            }
        }

        let mut state = build_game(1, &[&[], &[]]);
        let chars = state.card_db().get(PEST_MASCOT).unwrap().chars.clone();
        let src = state.add_card(PlayerId(0), chars, Zone::Battlefield);
        let mut e = Engine::new(state, vec![Box::new(PassiveAgent), Box::new(PassiveAgent)]);
        assert_eq!(e.state.computed(src).power, Some(2));
        // Resolve a GainLife — this broadcasts LifeChanged, which queues the GainLife trigger.
        e.resolve_effect(
            &Effect::GainLife { who: PlayerRef::Controller, amount: ValueExpr::Fixed(2) },
            &ResolutionCtx { controller: Some(PlayerId(0)), ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        // Drain the agenda: put the trigger on the stack and resolve it.
        e.run_agenda();
        while !e.state.stack.items.is_empty() {
            e.resolve_top();
            e.run_agenda();
        }
        assert_eq!(e.state.computed(src).power, Some(3), "gain life → +1/+1 counter → 3/4");
    }
}
