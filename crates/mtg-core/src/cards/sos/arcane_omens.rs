//! Arcane Omens — `{4}{B}` Sorcery (first printed SOS).
//!
//! Oracle: "Converge — Target player discards X cards, where X is the number of colors of mana spent
//! to cast this spell."
//!
//! **Fully implemented** — a `TargetPlayer` declaration whose `Discard` count is `ValueExpr::ColorsSpent`
//! (the Converge value recorded at cast). Exercises player-as-target + Converge.

use crate::basics::{CardType, Color};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::target::PlayerFilter;
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;

/// grp id (per-set ids live near their cards).
pub const ARCANE_OMENS: u32 = 292;

pub fn register(db: &mut CardDb) {
    let effect = Effect::Sequence(vec![
        Effect::TargetPlayer(PlayerFilter::Any),
        Effect::Discard { who: PlayerRef::ChosenTarget(0), count: ValueExpr::ColorsSpent },
    ]);
    db.insert(
        spell(ARCANE_OMENS, "Arcane Omens", CardType::Sorcery, Color::Black, mana_cost(4, &[(Color::Black, 1)]), effect)
            .with_text("Converge — Target player discards X cards, where X is the number of colors of mana spent to cast this spell."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn arcane_omens_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        assert!(db.get(ARCANE_OMENS).unwrap().fully_implemented);
        expect![[r#"
            Sequence(
                [
                    TargetPlayer(
                        Any,
                    ),
                    Discard {
                        who: ChosenTarget(
                            0,
                        ),
                        count: ColorsSpent,
                    },
                ],
            )"#]].assert_eq(&format!("{:#?}", db.get(ARCANE_OMENS).unwrap().spell_effect().unwrap()));
    }

    /// Behaviour: the targeted player discards a number of cards equal to the source's recorded
    /// `colors_spent` (here 2 → discards two).
    #[test]
    fn arcane_omens_discards_by_colors_spent() {
        use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView};
        use crate::basics::{Target, Zone};
        use crate::cards::{build_game, grp};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        #[derive(Clone)] struct DiscardLow;
        impl Agent for DiscardLow {
            fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
                match req {
                    DecisionRequest::SelectCards { min, .. } => DecisionResponse::Indices((0..*min).collect()),
                    _ => DecisionResponse::Pass,
                }
            }
        }
        let mut state = build_game(1, &[&[], &[]]);
        // The spell object on the stack, with 2 colours of mana recorded as spent.
        let omens = state.add_card(PlayerId(0), state.card_db().get(ARCANE_OMENS).unwrap().chars.clone(), Zone::Stack);
        state.objects.get_mut(&omens).unwrap().colors_spent = 2;
        // Target player (P1) holds three cards.
        for _ in 0..3 {
            let c = state.card_db().get(grp::FOREST).unwrap().chars.clone();
            state.add_card(PlayerId(1), c, Zone::Hand);
        }
        let effect = state.card_db().get(ARCANE_OMENS).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(DiscardLow), Box::new(DiscardLow)]);
        let hand0 = e.state.players[1].hand.len();
        e.resolve_effect(
            &effect,
            &ResolutionCtx { controller: Some(PlayerId(0)), source: Some(omens), chosen_targets: vec![Target::Player(PlayerId(1))], ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        assert_eq!(e.state.players[1].hand.len(), hand0 - 2, "targeted player discarded X=2 (colors spent)");
    }
}
