//! Snarl Song — `{5}{G}` Sorcery (first printed SOS).
//!
//! Oracle: "Converge — Create two 0/0 green and blue Fractal creature tokens. Put X +1/+1 counters
//! on each of them and you gain X life, where X is the number of colors of mana spent to cast this
//! spell."
//!
//! **Fully implemented** with no new cap — Converge (`ValueExpr::ColorsSpent`, S7) drives both the
//! per-token counter count (`CreateToken.dynamic_counters`, so each of the two 0/0 Fractals enters as
//! an X/X) and the life gain. Two tokens = `count: Fixed(2)`; the dynamic counters apply to *each*
//! created token (one `Action::CreateToken` per token, both baked with X counters).

use crate::basics::{CardType, Color, CounterKind};
use crate::cards::helpers::fractal_token;
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;

/// grp id (per-set ids live near their cards).
pub const SNARL_SONG: u32 = 331;

pub fn register(db: &mut CardDb) {
    let effect = Effect::Sequence(vec![
        Effect::CreateToken {
            spec: fractal_token(0),
            count: ValueExpr::Fixed(2),
            controller: PlayerRef::Controller,
            dynamic_counters: vec![(CounterKind::PlusOnePlusOne, ValueExpr::ColorsSpent)],
        },
        Effect::GainLife { who: PlayerRef::Controller, amount: ValueExpr::ColorsSpent },
    ]);
    db.insert(
        spell(
            SNARL_SONG,
            "Snarl Song",
            CardType::Sorcery,
            Color::Green,
            mana_cost(5, &[(Color::Green, 1)]),
            effect,
        )
        .with_text("Converge — Create two 0/0 green and blue Fractal creature tokens. Put X +1/+1 counters on each of them and you gain X life, where X is the number of colors of mana spent to cast this spell."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn snarl_song_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(SNARL_SONG).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Sorcery]);
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
                            2,
                        ),
                        controller: Controller,
                        dynamic_counters: [
                            (
                                PlusOnePlusOne,
                                ColorsSpent,
                            ),
                        ],
                    },
                    GainLife {
                        who: Controller,
                        amount: ColorsSpent,
                    },
                ],
            )"#]]
        .assert_eq(&format!("{:#?}", def.spell_effect().unwrap()));
    }

    /// Behaviour with colors_spent = 3 (Converge): TWO 0/0 Fractals each enter with three +1/+1
    /// counters (3/3s), and the caster gains 3 life.
    #[test]
    fn snarl_song_scales_with_colors_spent() {
        use crate::agent::RandomAgent;
        use crate::basics::{CounterKind as CK, Zone};
        use crate::cards::build_game;
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        let mut state = build_game(1, &[&[], &[]]);
        let src = state.add_card(
            PlayerId(0),
            state.card_db().get(SNARL_SONG).unwrap().chars.clone(),
            Zone::Stack,
        );
        state.objects.get_mut(&src).unwrap().colors_spent = 3;
        let effect = state.card_db().get(SNARL_SONG).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(
            state,
            vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))],
        );
        let (bf_before, life_before) =
            (e.state.player(PlayerId(0)).battlefield.len(), e.state.player(PlayerId(0)).life);
        e.resolve_effect(
            &effect,
            &ResolutionCtx { controller: Some(PlayerId(0)), source: Some(src), ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        let bf = e.state.player(PlayerId(0)).battlefield.clone();
        assert_eq!(bf.len(), bf_before + 2, "two Fractal tokens created");
        for tok in bf.iter().rev().take(2) {
            assert_eq!(
                e.state.object(*tok).counters.get(&CK::PlusOnePlusOne),
                3,
                "each token entered with X=3 counters (a 3/3)"
            );
        }
        assert_eq!(e.state.player(PlayerId(0)).life, life_before + 3, "gained X=3 life");
    }
}
