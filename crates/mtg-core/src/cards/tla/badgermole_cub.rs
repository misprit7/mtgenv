//! Badgermole Cub — `{1}{G}` Creature — Badger Mole 2/2 (first printed TLA, Avatar: The Last
//! Airbender).
//!
//! Oracle:
//!   When this creature enters, earthbend 1. (Target land you control becomes a 0/0 creature with
//!   haste that's still a land. Put a +1/+1 counter on it. When it dies or is exiled, return it to
//!   the battlefield tapped.)
//!   Whenever you tap a creature for mana, add an additional {G}.
//!
//! **Fully implemented** — both abilities faithful:
//! - "When this creature enters, **earthbend 1**" — a `Triggered{SelfEnters}` over
//!   `Effect::Earthbend{target: target land you control, n: 1}` (C12, fully landed incl. the
//!   "dies/exiled → return tapped" delayed trigger). The targeted land becomes a 0/0 haste
//!   land-creature with one +1/+1 counter.
//! - "Whenever you tap a creature for mana, add an additional {G}." — a `Triggered{TapCreatureForMana}`
//!   (cap 23242f2; CR 605.1b, fires per creature tapped for mana) over `Effect::AddMana{Controller, {G}}`,
//!   a no-stack triggered mana ability. So tapping any creature for mana yields an extra green.

use crate::basics::Color;
use crate::cards::helpers::earthbend;
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern};
use crate::effects::target::ManaSpec;
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const BADGERMOLE_CUB: u32 = 113;

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        BADGERMOLE_CUB,
        "Badgermole Cub",
        &[CreatureType::Badger, CreatureType::Mole],
        Color::Green,
        mana_cost(1, &[(Color::Green, 1)]),
        2,
        2,
        vec![
            // "When this creature enters, earthbend 1."
            Ability::Triggered {
                event: EventPattern::SelfEnters,
                condition: None,
                intervening_if: false,
                effect: earthbend(1),
            },
            // "Whenever you tap a creature for mana, add an additional {G}." (no-stack mana trigger).
            Ability::Triggered {
                event: EventPattern::TapCreatureForMana,
                condition: None,
                intervening_if: false,
                effect: Effect::AddMana {
                    who: PlayerRef::Controller,
                    mana: ManaSpec {
                        produces: vec![(Color::Green, ValueExpr::Fixed(1))],
                        any_color: None,
                        restriction: None,
                    },
                },
            },
        ],
    );
    def.text = "When this creature enters, earthbend 1. (Target land you control becomes a 0/0 creature with haste that's still a land. Put a +1/+1 counter on it. When it dies or is exiled, return it to the battlefield tapped.)\nWhenever you tap a creature for mana, add an additional {G}.".to_string();
    // Fully implemented: ETB earthbend 1 (C12) + the reflexive "tap a creature for mana → add {G}"
    // trigger (cap 23242f2). See module docs.
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::basics::CardType;
    use crate::subtypes::Subtype;
    use expect_test::expect;

    #[test]
    fn badgermole_cub_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(BADGERMOLE_CUB).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Creature]);
        assert_eq!(
            def.chars.subtypes,
            vec![Subtype::Creature(CreatureType::Badger), Subtype::Creature(CreatureType::Mole)]
        );
        assert_eq!((def.chars.power, def.chars.toughness), (Some(2), Some(2)));
        // Fully implemented: ETB earthbend 1 + the reflexive "tap a creature for mana → add {G}" trigger.
        assert!(def.fully_implemented);
        // ETB earthbend trigger (targets "a land you control") + the TapCreatureForMana → add {G} trigger.
        expect![[r#"
            [
                Triggered {
                    event: SelfEnters,
                    condition: None,
                    intervening_if: false,
                    effect: Earthbend {
                        target: Target(
                            TargetSpec {
                                kind: Permanent(
                                    All(
                                        [
                                            HasCardType(
                                                Land,
                                            ),
                                            ControlledBy(
                                                Controller,
                                            ),
                                        ],
                                    ),
                                ),
                                min: 1,
                                max: 1,
                                distinct: true,
                            },
                        ),
                        n: Fixed(
                            1,
                        ),
                    },
                },
                Triggered {
                    event: TapCreatureForMana,
                    condition: None,
                    intervening_if: false,
                    effect: AddMana {
                        who: Controller,
                        mana: ManaSpec {
                            produces: [
                                (
                                    Green,
                                    Fixed(
                                        1,
                                    ),
                                ),
                            ],
                            any_color: None,
                            restriction: None,
                        },
                    },
                },
            ]"#]]
        .assert_eq(&format!("{:#?}", def.abilities));
    }

    /// Behaviour: the reflexive "whenever you tap a creature for mana" ability adds an extra {G} to
    /// your mana pool when it resolves.
    #[test]
    fn badgermole_reflexive_mana_adds_green() {
        use crate::agent::RandomAgent;
        use crate::basics::{Color, Zone};
        use crate::cards::build_game;
        use crate::effects::ability::Ability;
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        let mut state = build_game(1, &[&[], &[]]);
        let chars = state.card_db().get(BADGERMOLE_CUB).unwrap().chars.clone();
        let badger = state.add_card(PlayerId(0), chars, Zone::Battlefield);
        let add_mana = match &state.card_db().get(BADGERMOLE_CUB).unwrap().abilities[1] {
            Ability::Triggered { effect, .. } => effect.clone(),
            o => panic!("expected TapCreatureForMana Triggered, got {o:?}"),
        };
        let mut e = Engine::new(
            state,
            vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))],
        );
        assert_eq!(e.state.players[0].mana_pool.amounts.get(&Color::Green), None);
        e.resolve_effect(
            &add_mana,
            &ResolutionCtx { controller: Some(PlayerId(0)), source: Some(badger), ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        assert_eq!(e.state.players[0].mana_pool.amounts.get(&Color::Green), Some(&1)); // +{G}
    }

    /// #60 end-to-end (REAL cast → ETB trigger): cast Badgermole Cub `{1}{G}`; on entering, its
    /// "When this creature enters, earthbend 1" trigger goes on the stack (prompting `ChooseTargets`
    /// for a land you control) and resolves to animate that land into a 1/1 haste land-creature
    /// (0/0 + one +1/+1 counter). Drives `cast_spell` (real mana) → `resolve_top` (enters) →
    /// `run_agenda` (stacks the ETB trigger) → `resolve_top` (earthbend resolves).
    #[test]
    fn badgermole_etb_earthbend_via_real_cast() {
        use crate::agent::{Agent, CastVariant, DecisionRequest, DecisionResponse, PlayerView};
        use crate::basics::{CardType, Target, Zone};
        use crate::cards::{grp, starter_db};
        use crate::effects::ability::Keyword;
        use crate::ids::{ObjId, PlayerId};
        use crate::priority::Engine;
        use crate::state::GameState;
        use std::sync::Arc;

        // Earthbend the specific (untapped) land we set aside, not one of the mana lands.
        #[derive(Clone)]
        struct TargetAgent {
            want: ObjId,
        }
        impl Agent for TargetAgent {
            fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
                match req {
                    DecisionRequest::ChooseTargets { slots, .. } => {
                        let i = slots[0]
                            .legal
                            .iter()
                            .position(|t| matches!(t, Target::Object(o) if *o == self.want))
                            .unwrap_or(0);
                        DecisionResponse::Pairs(vec![(0, i as u32)])
                    }
                    _ => DecisionResponse::Pass,
                }
            }
        }

        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(starter_db()));
        let badger = {
            let c = state.card_db().get(BADGERMOLE_CUB).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Hand)
        };
        for _ in 0..2 {
            let c = state.card_db().get(grp::FOREST).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield); // pays {1}{G}
        }
        let target_land = {
            let c = state.card_db().get(grp::FOREST).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield) // the land to earthbend
        };
        let mut e = Engine::new(
            state,
            vec![Box::new(TargetAgent { want: target_land }), Box::new(TargetAgent { want: target_land })],
        );

        e.cast_spell(PlayerId(0), badger, CastVariant::Normal);
        e.resolve_top(); // Badgermole enters
        e.run_agenda(); // ETB earthbend trigger goes on the stack (targets chosen here)
        while !e.state.stack.items.is_empty() {
            e.resolve_top();
            e.run_agenda();
        }

        let cc = e.state.computed(target_land);
        assert!(cc.is_creature(), "the earthbent land is now a creature");
        assert!(cc.card_types.contains(&CardType::Land), "and is still a land");
        assert!(cc.has_keyword(Keyword::Haste), "with haste");
        assert_eq!((cc.power, cc.toughness), (Some(1), Some(1)), "earthbend 1 → 0/0 + one counter = 1/1");
    }
}
