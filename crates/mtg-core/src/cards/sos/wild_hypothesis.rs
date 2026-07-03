//! Wild Hypothesis — `{X}{G}` Sorcery (first printed SOS).
//!
//! Oracle: "Create a 0/0 green and blue Fractal creature token. Put X +1/+1 counters on it.
//! Surveil 2."
//!
//! **Fully implemented** — the shared 0/0 Fractal token (`fractal_token(0)`) entering with **X**
//! +1/+1 counters via the new `Effect::CreateToken.dynamic_counters` cap (counter counts computed at
//! resolution and baked onto the token, so it enters as an `X/X`), then Surveil 2 (S1). `{X}` in the
//! cost = `mana_cost.x = 1`; `ValueExpr::X` reads the chosen X from the resolution context.

use crate::basics::{CardType, Color, CounterKind};
use crate::cards::helpers::fractal_token;
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;

/// grp id (per-set ids live near their cards).
pub const WILD_HYPOTHESIS: u32 = 330;

pub fn register(db: &mut CardDb) {
    let effect = Effect::Sequence(vec![
        Effect::CreateToken {
            spec: fractal_token(0),
            count: ValueExpr::Fixed(1),
            controller: PlayerRef::Controller,
            dynamic_counters: vec![(CounterKind::PlusOnePlusOne, ValueExpr::X)],
        },
        Effect::Surveil { count: ValueExpr::Fixed(2) },
    ]);
    let mut def = spell(
        WILD_HYPOTHESIS,
        "Wild Hypothesis",
        CardType::Sorcery,
        Color::Green,
        mana_cost(0, &[(Color::Green, 1)]),
        effect,
    )
    .with_text(
        "Create a 0/0 green and blue Fractal creature token. Put X +1/+1 counters on it. Surveil 2.",
    );
    // `{X}{G}`: one `{X}` symbol in the printed cost (CR 107.3).
    def.chars.mana_cost.as_mut().unwrap().x = 1;
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn wild_hypothesis_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(WILD_HYPOTHESIS).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Sorcery]);
        assert_eq!(def.chars.mana_cost.as_ref().unwrap().x, 1, "one {{X}} in the cost");
        assert!(def.fully_implemented);
        expect![[r#"
            Sequence(
                [
                    CreateToken {
                        spec: TokenSpec {
                            name: "Fractal",
                            card_types: [
                                Creature,
                            ],
                            subtypes: [
                                Creature(
                                    Fractal,
                                ),
                            ],
                            colors: [
                                Green,
                                Blue,
                            ],
                            power: 0,
                            toughness: 0,
                            keywords: [],
                            counters: [],
                            grp_id: 0,
                        },
                        count: Fixed(
                            1,
                        ),
                        controller: Controller,
                        dynamic_counters: [
                            (
                                PlusOnePlusOne,
                                X,
                            ),
                        ],
                    },
                    Surveil {
                        count: Fixed(
                            2,
                        ),
                    },
                ],
            )"#]]
        .assert_eq(&format!("{:#?}", def.spell_effect().unwrap()));
    }

    /// Behaviour: resolving with X=3 creates one Fractal token entering with three +1/+1 counters
    /// (a 3/3) — proving `dynamic_counters` bakes the resolution-time X onto the token.
    #[test]
    fn creates_a_fractal_with_x_counters() {
        use crate::agent::RandomAgent;
        use crate::basics::CounterKind as CK;
        use crate::cards::build_game;
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        let state = build_game(1, &[&[], &[]]);
        let effect = state.card_db().get(WILD_HYPOTHESIS).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(
            state,
            vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))],
        );
        let before = e.state.player(PlayerId(0)).battlefield.len();
        e.resolve_effect(
            &effect,
            &ResolutionCtx {
                controller: Some(PlayerId(0)),
                x: Some(3),
                ..Default::default()
            },
            WbReason::Resolve(StackId(0)),
        );
        let bf = &e.state.player(PlayerId(0)).battlefield;
        assert_eq!(bf.len(), before + 1, "one Fractal token created");
        let token = *bf.last().unwrap();
        assert_eq!(
            e.state.object(token).counters.get(&CK::PlusOnePlusOne),
            3,
            "entered with X=3 +1/+1 counters (a 3/3 Fractal)"
        );
        let cc = e.state.computed(token);
        assert_eq!((cc.power, cc.toughness), (Some(3), Some(3)), "computed as a 3/3");
    }

    /// With X=0 the token enters as a 0/0 (and would die to SBAs in a real game) — the dynamic
    /// counter clause adds nothing.
    #[test]
    fn x_zero_makes_a_bare_zero_zero() {
        use crate::agent::RandomAgent;
        use crate::basics::CounterKind as CK;
        use crate::cards::build_game;
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        let state = build_game(1, &[&[], &[]]);
        let effect = state.card_db().get(WILD_HYPOTHESIS).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(
            state,
            vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))],
        );
        e.resolve_effect(
            &effect,
            &ResolutionCtx { controller: Some(PlayerId(0)), x: Some(0), ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        let token = *e.state.player(PlayerId(0)).battlefield.last().unwrap();
        assert_eq!(e.state.object(token).counters.get(&CK::PlusOnePlusOne), 0, "no counters at X=0");
    }
}
