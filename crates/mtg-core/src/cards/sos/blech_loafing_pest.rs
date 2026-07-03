//! Blech, Loafing Pest — `{1}{B}{G}` Legendary Creature — Pest 3/4 (first printed SOS).
//!
//! Oracle: "Whenever you gain life, put a +1/+1 counter on each Pest, Bat, Insect, Snake, and
//! Spider you control."
//!
//! **Fully implemented** — a `GainLife` triggered ability whose effect is a `ForEach` over the
//! matching creatures you control (Pest / Bat / Insect / Snake / Spider), putting a +1/+1 counter on
//! each. Blech itself is a Pest, so it grows too. Legendary; multicolored (B/G).

use crate::basics::{Color, CounterKind, Zone};
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern};
use crate::effects::target::{CardFilter, SelectSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};
use crate::subtypes::{CreatureType, Supertype};

/// grp id (per-set ids live near their cards).
pub const BLECH_LOAFING_PEST: u32 = 242;

fn tribal_creatures_you_control() -> SelectSpec {
    SelectSpec {
        zone: Zone::Battlefield,
        filter: CardFilter::All(vec![
            CardFilter::ControlledBy(PlayerRef::Controller),
            CardFilter::AnyOf(vec![
                CardFilter::HasSubtype(CreatureType::Pest.into()),
                CardFilter::HasSubtype(CreatureType::Bat.into()),
                CardFilter::HasSubtype(CreatureType::Insect.into()),
                CardFilter::HasSubtype(CreatureType::Snake.into()),
                CardFilter::HasSubtype(CreatureType::Spider.into()),
            ]),
        ]),
        chooser: PlayerRef::Controller,
        min: ValueExpr::Fixed(0),
        max: ValueExpr::Fixed(999),
    }
}

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        BLECH_LOAFING_PEST,
        "Blech, Loafing Pest",
        &[CreatureType::Pest],
        Color::Black,
        mana_cost(1, &[(Color::Black, 1), (Color::Green, 1)]),
        3,
        4,
        vec![Ability::Triggered {
            event: EventPattern::GainLife,
            condition: None,
            intervening_if: false,
            effect: Effect::ForEach {
                selector: tribal_creatures_you_control(),
                body: Box::new(Effect::PutCounters {
                    what: EffectTarget::Each,
                    kind: CounterKind::PlusOnePlusOne,
                    n: ValueExpr::Fixed(1),
                }),
            },
        }],
    );
    def.chars.colors = vec![Color::Black, Color::Green];
    def.chars.supertypes = vec![Supertype::Legendary];
    def.text = "Whenever you gain life, put a +1/+1 counter on each Pest, Bat, Insect, Snake, and Spider you control.".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn blech_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(BLECH_LOAFING_PEST).unwrap();
        assert_eq!(def.chars.supertypes, vec![Supertype::Legendary]);
        assert!(def.fully_implemented);
        expect![[r#"
            [
                Triggered {
                    event: GainLife,
                    condition: None,
                    intervening_if: false,
                    effect: ForEach {
                        selector: SelectSpec {
                            zone: Battlefield,
                            filter: All(
                                [
                                    ControlledBy(
                                        Controller,
                                    ),
                                    AnyOf(
                                        [
                                            HasSubtype(
                                                Creature(
                                                    Pest,
                                                ),
                                            ),
                                            HasSubtype(
                                                Creature(
                                                    Bat,
                                                ),
                                            ),
                                            HasSubtype(
                                                Creature(
                                                    Insect,
                                                ),
                                            ),
                                            HasSubtype(
                                                Creature(
                                                    Snake,
                                                ),
                                            ),
                                            HasSubtype(
                                                Creature(
                                                    Spider,
                                                ),
                                            ),
                                        ],
                                    ),
                                ],
                            ),
                            chooser: Controller,
                            min: Fixed(
                                0,
                            ),
                            max: Fixed(
                                999,
                            ),
                        },
                        body: PutCounters {
                            what: Each,
                            kind: PlusOnePlusOne,
                            n: Fixed(
                                1,
                            ),
                        },
                    },
                },
            ]"#]].assert_eq(&format!("{:#?}", def.abilities));
    }

    /// Behaviour (end-to-end): gaining life fires the trigger; Blech (a Pest you control) gets a
    /// +1/+1 counter (3/4 → 4/5).
    #[test]
    fn blech_pumps_tribe_on_gain_life() {
        use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView};
        use crate::cards::build_game;
        use crate::effects::action::{ResolutionCtx, WbReason};
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
        let chars = state.card_db().get(BLECH_LOAFING_PEST).unwrap().chars.clone();
        let src = state.add_card(PlayerId(0), chars, Zone::Battlefield);
        let mut e = Engine::new(state, vec![Box::new(PassiveAgent), Box::new(PassiveAgent)]);
        assert_eq!(e.state.computed(src).power, Some(3));
        e.resolve_effect(
            &Effect::GainLife { who: PlayerRef::Controller, amount: ValueExpr::Fixed(1) },
            &ResolutionCtx { controller: Some(PlayerId(0)), ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        e.run_agenda();
        while !e.state.stack.items.is_empty() {
            e.resolve_top();
            e.run_agenda();
        }
        assert_eq!(e.state.computed(src).power, Some(4), "Blech (a Pest) got a +1/+1 counter → 4/5");
    }
}
