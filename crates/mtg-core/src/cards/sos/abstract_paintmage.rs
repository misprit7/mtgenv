//! Abstract Paintmage — `{U}{U/R}{R}` Creature — Djinn Sorcerer 2/2 (first printed SOS).
//!
//! Oracle: "At the beginning of your first main phase, add {U}{R}. Spend this mana only to cast
//! instant and sorcery spells."
//!
//! **Fully implemented** — a two-colour hybrid cost (`{U/R}`) plus a `BeginningOfStep(PrecombatMain)`
//! trigger (your first main phase) gated on `YourTurn` that adds restricted `{U}{R}` (S13:
//! `ManaSpec.restriction = InstantSorceryOnly` → the pool's `restricted` bucket). Exercises **both**
//! the begin-of-step-trigger cap (the trigger actually fires through the turn engine) and the
//! restricted-mana cap (the floated mana pays an instant/sorcery cast only).

use crate::basics::Color;
use crate::cards::{creature, mana_cost_hybrid, CardDb};
use crate::effects::ability::{Ability, EventPattern};
use crate::effects::condition::Condition;
use crate::effects::target::{ManaSpec, SpendRestriction};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const ABSTRACT_PAINTMAGE: u32 = 323;

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        ABSTRACT_PAINTMAGE,
        "Abstract Paintmage",
        &[CreatureType::Djinn, CreatureType::Sorcerer],
        Color::Blue,
        mana_cost_hybrid(0, &[(Color::Blue, 1), (Color::Red, 1)], &[(Color::Blue, Color::Red)]),
        2,
        2,
        vec![Ability::Triggered {
            // "At the beginning of your first main phase" = precombat main, on your turn.
            event: EventPattern::BeginningOfStep(crate::basics::Phase::PrecombatMain),
            condition: Some(Condition::YourTurn),
            intervening_if: false,
            effect: Effect::AddMana {
                who: PlayerRef::Controller,
                mana: ManaSpec {
                    produces: vec![
                        (Color::Blue, ValueExpr::Fixed(1)),
                        (Color::Red, ValueExpr::Fixed(1)),
                    ],
                    any_color: None,
                    restriction: Some(SpendRestriction::InstantSorceryOnly),
                },
            },
        }],
    );
    def.chars.colors = vec![Color::Blue, Color::Red];
    def.text = "At the beginning of your first main phase, add {U}{R}. Spend this mana only to cast instant and sorcery spells.".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn abstract_paintmage_shape() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(ABSTRACT_PAINTMAGE).unwrap();
        assert_eq!(def.chars.colors, vec![Color::Blue, Color::Red]);
        assert_eq!(def.chars.mana_cost.as_ref().unwrap().hybrid, vec![(Color::Blue, Color::Red)]);
        assert_eq!(def.chars.mana_value(), 3);
        assert!(def.fully_implemented);
    }

    #[test]
    fn abstract_paintmage_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(ABSTRACT_PAINTMAGE).unwrap();
        expect![[r#"
            [
                Triggered {
                    event: BeginningOfStep(
                        PrecombatMain,
                    ),
                    condition: Some(
                        YourTurn,
                    ),
                    intervening_if: false,
                    effect: AddMana {
                        who: Controller,
                        mana: ManaSpec {
                            produces: [
                                (
                                    Blue,
                                    Fixed(
                                        1,
                                    ),
                                ),
                                (
                                    Red,
                                    Fixed(
                                        1,
                                    ),
                                ),
                            ],
                            any_color: None,
                            restriction: Some(
                                InstantSorceryOnly,
                            ),
                        },
                    },
                },
            ]"#]].assert_eq(&format!("{:#?}", def.abilities));
    }

    /// Integration (real turn engine + S13): at your precombat main the trigger fires and floats
    /// restricted {U}{R}; that mana pays an instant/sorcery cast but not a creature spell. Also gated
    /// off on the opponent's turn (YourTurn condition).
    #[test]
    fn abstract_paintmage_floats_restricted_mana_at_your_first_main() {
        use crate::agent::{Agent, DecisionRequest, DecisionResponse, GameEvent, PlayerView};
        use crate::basics::{Phase, Zone};
        use crate::cards::{build_game, mana_cost};
        use crate::ids::PlayerId;
        use crate::mana;
        use crate::priority::Engine;

        #[derive(Clone)]
        struct PassAgent;
        impl Agent for PassAgent {
            fn decide(&mut self, _v: &PlayerView, _r: &DecisionRequest) -> DecisionResponse {
                DecisionResponse::Pass
            }
        }

        // Returns (restricted mana floated, can-pay-an-I/S {U}, can-pay-a-creature {U}).
        let run = |active: PlayerId| -> (u32, bool, bool) {
            let mut state = build_game(1, &[&[], &[]]);
            let mage = {
                let c = state.card_db().get(ABSTRACT_PAINTMAGE).unwrap().chars.clone();
                state.add_card(PlayerId(0), c, Zone::Battlefield)
            };
            state.objects.get_mut(&mage).unwrap().summoning_sick = false;
            state.active_player = active;
            state.phase = Phase::PrecombatMain;
            let mut e = Engine::new(state, vec![Box::new(PassAgent), Box::new(PassAgent)]);
            e.broadcast(GameEvent::PhaseBegan {
                turn: 1,
                phase: Phase::PrecombatMain,
                active,
            });
            e.run_agenda();
            if !e.state.stack.is_empty() {
                e.resolve_top();
            }
            let restricted: u32 =
                e.state.player(PlayerId(0)).mana_pool.restricted.values().copied().sum();
            let u = mana_cost(0, &[(Color::Blue, 1)]);
            let pays_is = mana::can_pay_ex(&e.state, PlayerId(0), &u, true);
            let pays_creature = mana::can_pay(&e.state, PlayerId(0), &u);
            (restricted, pays_is, pays_creature)
        };

        // On YOUR precombat main: {U}{R} restricted floated; pays an I/S {U}, not a creature {U}.
        assert_eq!(run(PlayerId(0)), (2, true, false));
        // On the opponent's turn: the YourTurn condition fails → no mana floated.
        assert_eq!(run(PlayerId(1)), (0, false, false));
    }
}
