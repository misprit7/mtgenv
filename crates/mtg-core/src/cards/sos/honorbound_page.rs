//! Honorbound Page // Forum's Favor — `{3}{W}` Creature — Cat Cleric 3/3 // `{W}` Sorcery
//! (first printed SOS). A **Prepare** DFC (enters prepared).
//!
//! Front: "This creature enters prepared."  Back (Forum's Favor): "Target creature gets +1/+0 and
//! gains flying until end of turn."
//!
//! **Fully implemented** — enters-prepared via [`helpers::enters_prepared`]; the back face pumps +1/+0
//! and grants flying until end of turn (the grant references the same chosen target via `ChosenIndex(0)`).

use crate::basics::{CardType, Color};
use crate::cards::{creature, helpers, mana_cost, spell, CardDb};
use crate::effects::ability::Keyword;
use crate::effects::condition::Duration;
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::ValueExpr;
use crate::effects::{Effect, EffectTarget};
use crate::subtypes::CreatureType;

pub const HONORBOUND_PAGE: u32 = 380;
pub const FORUMS_FAVOR: u32 = 9707;

pub fn register(db: &mut CardDb) {
    let forums_favor = Effect::Sequence(vec![
        Effect::PumpPT {
            what: EffectTarget::Target(TargetSpec {
                kind: TargetKind::Creature(CardFilter::Any),
                min: 1,
                max: 1,
                distinct: true,
            }),
            power: ValueExpr::Fixed(1),
            toughness: ValueExpr::Fixed(0),
            duration: Duration::UntilEndOfTurn,
        },
        Effect::GrantKeyword {
            what: EffectTarget::ChosenIndex(0),
            keyword: Keyword::Flying,
            duration: Duration::UntilEndOfTurn,
        },
    ]);
    db.insert(
        spell(FORUMS_FAVOR, "Forum's Favor", CardType::Sorcery, Color::White, mana_cost(0, &[(Color::White, 1)]), forums_favor)
            .with_text("Target creature gets +1/+0 and gains flying until end of turn."),
    );
    let mut front = creature(
        HONORBOUND_PAGE,
        "Honorbound Page",
        &[CreatureType::Cat, CreatureType::Cleric],
        Color::White,
        mana_cost(3, &[(Color::White, 1)]),
        3,
        3,
        helpers::enters_prepared(FORUMS_FAVOR),
    );
    front.text = "This creature enters prepared. (While it's prepared, you may cast a copy of its spell. Doing so unprepares it.)\n// Forum's Favor {W} Sorcery — Target creature gets +1/+0 and gains flying until end of turn.".to_string();
    db.insert(front);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::RandomAgent;
    use crate::basics::{Target, Zone};
    use crate::cards::{build_game, grp};
    use crate::effects::action::{ResolutionCtx, WbReason};
    use crate::ids::{PlayerId, StackId};
    use crate::priority::Engine;

    #[test]
    fn forums_favor_pumps_and_grants_flying() {
        let mut state = build_game(1, &[&[], &[]]);
        let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
        let bears = state.add_card(PlayerId(0), c, Zone::Battlefield);
        let effect = state.card_db().get(FORUMS_FAVOR).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
        e.resolve_effect(
            &effect,
            &ResolutionCtx {
                controller: Some(PlayerId(0)),
                chosen_targets: vec![Target::Object(bears)],
                ..Default::default()
            },
            WbReason::Resolve(StackId(0)),
        );
        let cc = e.state.computed(bears);
        assert_eq!(cc.power, Some(3), "+1 power (2→3)");
        assert!(cc.has_keyword(Keyword::Flying), "gained flying");
    }
}
