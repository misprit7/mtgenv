//! Fractal Anomaly — `{U}` Instant (first printed SOS).
//!
//! Oracle: "Create a 0/0 green and blue Fractal creature token and put X +1/+1 counters on it, where
//! X is the number of cards you've drawn this turn."
//!
//! **Fully implemented** — the shared 0/0 Fractal token (`fractal_token(0)`) entering with **X**
//! +1/+1 counters via `Effect::CreateToken.dynamic_counters`, where the counter count is the new
//! `ValueExpr::CardsDrawnThisTurn` (S19 — reads `Player.cards_drawn_this_turn`, reset each turn and
//! incremented in `draw`). Same token+dynamic-counters shape as Wild Hypothesis, but the count is
//! "cards drawn this turn" instead of `{X}`.

use crate::basics::{CardType, Color, CounterKind};
use crate::cards::helpers::fractal_token;
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;

/// grp id (per-set ids live near their cards).
pub const FRACTAL_ANOMALY: u32 = 344;

pub fn register(db: &mut CardDb) {
    let effect = Effect::CreateToken {
        spec: fractal_token(0),
        count: ValueExpr::Fixed(1),
        controller: PlayerRef::Controller,
        dynamic_counters: vec![(CounterKind::PlusOnePlusOne, ValueExpr::CardsDrawnThisTurn)],
    };
    db.insert(
        spell(
            FRACTAL_ANOMALY,
            "Fractal Anomaly",
            CardType::Instant,
            Color::Blue,
            mana_cost(0, &[(Color::Blue, 1)]),
            effect,
        )
        .with_text(
            "Create a 0/0 green and blue Fractal creature token and put X +1/+1 counters on it, where X is the number of cards you've drawn this turn.",
        ),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::RandomAgent;
    use crate::basics::Zone;
    use crate::cards::{build_game, grp};
    use crate::effects::action::{ResolutionCtx, WbReason};
    use crate::ids::{PlayerId, StackId};
    use crate::priority::Engine;
    use crate::subtypes::{CreatureType, Subtype};
    use expect_test::expect;

    #[test]
    fn fractal_anomaly_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(FRACTAL_ANOMALY).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Instant]);
        assert!(def.fully_implemented);
        expect![[r#"
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
                        CardsDrawnThisTurn,
                    ),
                ],
            }"#]]
        .assert_eq(&format!("{:#?}", def.spell_effect().unwrap()));
    }

    /// After the caster has drawn 3 cards this turn, the Fractal enters with three +1/+1 counters
    /// (a 3/3). Drives the REAL draw path (`Engine::draw`) so `cards_drawn_this_turn` is set for real.
    #[test]
    fn fractal_enters_with_counters_equal_to_cards_drawn() {
        let mut state = build_game(1, &[&[], &[]]);
        // Stock P0's library so it can actually draw.
        let bears = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
        for _ in 0..3 {
            state.add_card(PlayerId(0), bears.clone(), Zone::Library);
        }
        let effect = state.card_db().get(FRACTAL_ANOMALY).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
        e.draw(PlayerId(0), 3);
        assert_eq!(e.state.player(PlayerId(0)).cards_drawn_this_turn, 3);
        let before: Vec<_> = e.state.player(PlayerId(0)).battlefield.clone();
        e.resolve_effect(
            &effect,
            &ResolutionCtx { controller: Some(PlayerId(0)), ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        let token = *e
            .state
            .player(PlayerId(0))
            .battlefield
            .iter()
            .find(|id| !before.contains(id))
            .expect("one Fractal token created");
        assert!(e
            .state
            .object(token)
            .chars
            .subtypes
            .contains(&Subtype::Creature(CreatureType::Fractal)));
        assert_eq!(
            e.state.object(token).counters.get(&CounterKind::PlusOnePlusOne),
            3,
            "entered with 3 +1/+1 counters (3 cards drawn this turn)"
        );
    }

    /// With no cards drawn this turn the Fractal enters as a 0/0 (no counters).
    #[test]
    fn no_cards_drawn_means_zero_counters() {
        let mut state = build_game(1, &[&[], &[]]);
        let effect = state.card_db().get(FRACTAL_ANOMALY).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
        let before: Vec<_> = e.state.player(PlayerId(0)).battlefield.clone();
        e.resolve_effect(
            &effect,
            &ResolutionCtx { controller: Some(PlayerId(0)), ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        let token = e.state.player(PlayerId(0)).battlefield.iter().find(|id| !before.contains(id)).copied();
        if let Some(token) = token {
            assert_eq!(e.state.object(token).counters.get(&CounterKind::PlusOnePlusOne), 0, "0/0, no counters");
        }
    }
}
