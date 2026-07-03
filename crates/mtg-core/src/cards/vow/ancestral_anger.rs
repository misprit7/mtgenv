//! Ancestral Anger — `{R}` Sorcery (first printed VOW; reprinted in SOS).
//!
//! Oracle: "Target creature gains trample and gets +X/+0 until end of turn, where X is 1 plus the
//! number of cards named Ancestral Anger in your graveyard. / Draw a card."
//!
//! **Fully implemented** — one target creature gains trample + `+X/+0` (X = 1 + `Count` of cards
//! `Named("Ancestral Anger")` in your graveyard, via `ChosenIndex(0)` to reuse the target), then draw.

use crate::basics::{CardType, Color, Zone};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::ability::Keyword;
use crate::effects::condition::Duration;
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};

/// grp id (per-set ids live near their cards).
pub const ANCESTRAL_ANGER: u32 = 278;

pub fn register(db: &mut CardDb) {
    let x = ValueExpr::Sum(
        Box::new(ValueExpr::Fixed(1)),
        Box::new(ValueExpr::Count {
            zone: Zone::Graveyard,
            filter: CardFilter::Named("Ancestral Anger".to_string()),
            controller: Some(PlayerRef::Controller),
        }),
    );
    let effect = Effect::Sequence(vec![
        Effect::GrantKeyword {
            what: EffectTarget::Target(TargetSpec {
                kind: TargetKind::Creature(CardFilter::Any),
                min: 1,
                max: 1,
                distinct: true,
            }),
            keyword: Keyword::Trample,
            duration: Duration::UntilEndOfTurn,
        },
        Effect::PumpPT {
            what: EffectTarget::ChosenIndex(0),
            power: x,
            toughness: ValueExpr::Fixed(0),
            duration: Duration::UntilEndOfTurn,
        },
        Effect::Draw { who: PlayerRef::Controller, count: ValueExpr::Fixed(1) },
    ]);
    db.insert(
        spell(
            ANCESTRAL_ANGER,
            "Ancestral Anger",
            CardType::Sorcery,
            Color::Red,
            mana_cost(0, &[(Color::Red, 1)]),
            effect,
        )
        .with_text("Target creature gains trample and gets +X/+0 until end of turn, where X is 1 plus the number of cards named Ancestral Anger in your graveyard.\nDraw a card."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::RandomAgent;
    use crate::basics::Target;
    use crate::cards::{build_game, grp};
    use crate::effects::action::{ResolutionCtx, WbReason};
    use crate::ids::{PlayerId, StackId};
    use crate::priority::Engine;

    fn cast_on(copies_in_gy: usize) -> (i32, bool, usize) {
        let lib: Vec<u32> = std::iter::repeat(grp::FOREST).take(2).collect();
        let mut state = build_game(1, &[&lib, &[]]);
        let bear = state.add_card(PlayerId(0), state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Battlefield);
        for _ in 0..copies_in_gy {
            let c = state.card_db().get(ANCESTRAL_ANGER).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Graveyard);
        }
        let effect = state.card_db().get(ANCESTRAL_ANGER).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
        let hand_before = e.state.players[0].hand.len();
        e.resolve_effect(&effect, &ResolutionCtx { controller: Some(PlayerId(0)), chosen_targets: vec![Target::Object(bear)], ..Default::default() }, WbReason::Resolve(StackId(0)));
        let cc = e.state.computed(bear);
        (cc.power.unwrap(), cc.has_keyword(Keyword::Trample), e.state.players[0].hand.len() - hand_before)
    }

    #[test]
    fn ancestral_anger_x_scales_with_named_copies_and_draws() {
        // No copies in gy → X = 1 → 2/2 bear becomes 3/2, gains trample, and we draw a card.
        assert_eq!(cast_on(0), (3, true, 1));
        // One copy named "Ancestral Anger" in gy → X = 2 → 4/2.
        assert_eq!(cast_on(1).0, 4, "X counts cards named Ancestral Anger in graveyard");
    }
}
