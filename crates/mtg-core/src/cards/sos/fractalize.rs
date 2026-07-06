//! Fractalize — `{X}{U}` Instant (first printed SOS).
//!
//! Oracle: "Until end of turn, target creature becomes a green and blue Fractal with base power and
//! toughness each equal to X plus 1. (It loses all other colors and creature types.)"
//!
//! **Fully implemented** — the first consumer of the **layer-4 subtype-set** capability. A single
//! [`Effect::Becomes`] grants one continuous effect (CR 611.2 / 613) over the target for the turn:
//! - [`StaticContribution::SetCreatureSubtypes`] (layer 4) → exactly `[Fractal]`, dropping all other
//!   creature types (keeps non-creature subtypes, e.g. an animated land's land types).
//! - [`StaticContribution::SetColor`] (layer 5) → `[Green, Blue]`, losing all other colors.
//! - base P/T (layer 7b) = `X + 1` each, resolved to a concrete value at resolution (X is known then;
//!   a later static recompute has no cast-X, so a dynamic `SetBasePTValue` wouldn't work).

use crate::basics::{CardType, Color};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::ability::StaticContribution;
use crate::effects::condition::Duration;
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::ValueExpr;
use crate::effects::{Effect, EffectTarget};
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const FRACTALIZE: u32 = 500;

pub fn register(db: &mut CardDb) {
    // base P/T = X + 1 (CR — the target becomes an (X+1)/(X+1)).
    let x_plus_1 = || ValueExpr::Sum(Box::new(ValueExpr::X), Box::new(ValueExpr::Fixed(1)));
    let effect = Effect::Becomes {
        what: EffectTarget::Target(TargetSpec {
            kind: TargetKind::Creature(CardFilter::Any),
            min: 1,
            max: 1,
            distinct: true,
        }),
        contributions: vec![
            StaticContribution::SetCreatureSubtypes(vec![CreatureType::Fractal.into()]),
            StaticContribution::SetColor(vec![Color::Green, Color::Blue]),
        ],
        base_pt: Some((x_plus_1(), x_plus_1())),
        duration: Duration::UntilEndOfTurn,
    };
    let mut mc = mana_cost(0, &[(Color::Blue, 1)]);
    mc.x = 1; // `{X}{U}` — one `{X}` pip.
    db.insert(
        spell(FRACTALIZE, "Fractalize", CardType::Instant, Color::Blue, mc, effect).with_text(
            "Until end of turn, target creature becomes a green and blue Fractal with base power and toughness each equal to X plus 1. (It loses all other colors and creature types.)",
        ),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, CastVariant, DecisionRequest, DecisionResponse, PlayerView};
    use crate::basics::Zone;
    use crate::cards::{grp, starter_db};
    use crate::ids::PlayerId;
    use crate::priority::Engine;
    use crate::state::GameState;
    use crate::subtypes::Subtype;
    use expect_test::expect;
    use std::sync::Arc;

    #[test]
    fn fractalize_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(FRACTALIZE).unwrap();
        assert!(def.fully_implemented);
        assert_eq!(def.chars.card_types, vec![CardType::Instant]);
        assert_eq!(def.chars.mana_cost.as_ref().unwrap().x, 1);
        expect![[r#"
            Becomes {
                what: Target(
                    TargetSpec {
                        kind: Creature(
                            Any,
                        ),
                        min: 1,
                        max: 1,
                        distinct: true,
                    },
                ),
                contributions: [
                    SetCreatureSubtypes(
                        [
                            Creature(
                                Fractal,
                            ),
                        ],
                    ),
                    SetColor(
                        [
                            Green,
                            Blue,
                        ],
                    ),
                ],
                base_pt: Some(
                    (
                        Sum(
                            X,
                            Fixed(
                                1,
                            ),
                        ),
                        Sum(
                            X,
                            Fixed(
                                1,
                            ),
                        ),
                    ),
                ),
                duration: UntilEndOfTurn,
            }"#]]
        .assert_eq(&format!("{:#?}", def.spell_effect().unwrap()));
    }

    /// An agent that announces `X = x`, targets the first legal candidate of each slot, else passes.
    #[derive(Clone)]
    struct FractalizeAgent {
        x: i64,
    }
    impl Agent for FractalizeAgent {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::ChooseNumber { .. } => DecisionResponse::Number(self.x),
                DecisionRequest::ChooseTargets { slots, .. } => DecisionResponse::Pairs(
                    slots
                        .iter()
                        .enumerate()
                        .filter(|(_, s)| !s.legal.is_empty())
                        .map(|(i, _)| (i as u32, 0))
                        .collect(),
                ),
                _ => DecisionResponse::Pass,
            }
        }
    }

    /// Behaviour: cast for `{X=2}{U}` on a Grizzly Bears (2/2, green) → until end of turn it becomes a
    /// green-and-blue Fractal that is a 3/3 (X+1), losing its Bear type and its other colors.
    #[test]
    fn fractalize_turns_a_creature_into_a_fractal() {
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(starter_db_with_fractalize()));
        let bears = {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        let frac = {
            let c = state.card_db().get(FRACTALIZE).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Hand)
        };
        // Two Islands (for {U}) + one more for a generic; X=2 needs {U} + 2 generic = 3 mana.
        for _ in 0..3 {
            let i = state.card_db().get(grp::ISLAND).unwrap().chars.clone();
            state.add_card(PlayerId(0), i, Zone::Battlefield);
        }
        state.active_player = PlayerId(0);
        state.phase = crate::basics::Phase::PrecombatMain;

        // Sanity: the Bears starts as a green 2/2 Bear.
        {
            let cc = state.computed(bears);
            assert_eq!(cc.power, Some(2));
            assert!(cc.colors.contains(&Color::Green) && !cc.colors.contains(&Color::Blue));
            assert!(cc.subtypes.contains(&Subtype::Creature(CreatureType::Bear)));
        }

        let mut e = Engine::new(
            state,
            vec![Box::new(FractalizeAgent { x: 2 }), Box::new(FractalizeAgent { x: 2 })],
        );
        // Cast with X = 2 (the agent answers ChooseNumber(2)).
        e.cast_spell(PlayerId(0), frac, CastVariant::Normal);
        e.run_agenda();
        while !e.state.stack.items.is_empty() {
            e.resolve_top();
            e.run_agenda();
        }

        let cc = e.state.computed(bears);
        assert_eq!(cc.power, Some(3), "base P/T = X+1 = 3");
        assert_eq!(cc.toughness, Some(3));
        assert_eq!(cc.colors, vec![Color::Green, Color::Blue], "becomes green and blue only");
        assert!(
            cc.subtypes.contains(&Subtype::Creature(CreatureType::Fractal)),
            "is now a Fractal"
        );
        assert!(
            !cc.subtypes.contains(&Subtype::Creature(CreatureType::Bear)),
            "loses its Bear creature type"
        );
        assert!(cc.card_types.contains(&CardType::Creature), "still a creature");
    }

    fn starter_db_with_fractalize() -> CardDb {
        let mut db = starter_db();
        register(&mut db);
        db
    }
}
