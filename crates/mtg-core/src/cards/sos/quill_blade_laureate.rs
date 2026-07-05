//! Quill-Blade Laureate // Twofold Intent — `{1}{W}` Creature — Human Cleric 1/1 // `{1}{W}` Sorcery
//! (first printed SOS). A **Prepare** DFC (enters prepared).
//!
//! Front: "This creature enters prepared."  Back (Twofold Intent): "Target creature gets +1/+0 and
//! gains double strike until end of turn."
//!
//! **Fully implemented** — enters-prepared via [`helpers::enters_prepared`]; the back pumps +1/+0 and
//! grants double strike until end of turn.

use crate::basics::{CardType, Color};
use crate::cards::{creature, helpers, mana_cost, spell, CardDb};
use crate::effects::ability::Keyword;
use crate::effects::condition::Duration;
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::ValueExpr;
use crate::effects::{Effect, EffectTarget};
use crate::subtypes::CreatureType;

pub const QUILL_BLADE_LAUREATE: u32 = 381;
pub const TWOFOLD_INTENT: u32 = 9708;

pub fn register(db: &mut CardDb) {
    let twofold_intent = Effect::Sequence(vec![
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
            keyword: Keyword::DoubleStrike,
            duration: Duration::UntilEndOfTurn,
        },
    ]);
    db.insert(
        spell(TWOFOLD_INTENT, "Twofold Intent", CardType::Sorcery, Color::White, mana_cost(1, &[(Color::White, 1)]), twofold_intent)
            .with_text("Target creature gets +1/+0 and gains double strike until end of turn."),
    );
    let mut front = creature(
        QUILL_BLADE_LAUREATE,
        "Quill-Blade Laureate",
        &[CreatureType::Human, CreatureType::Cleric],
        Color::White,
        mana_cost(1, &[(Color::White, 1)]),
        1,
        1,
        helpers::enters_prepared(TWOFOLD_INTENT),
    );
    front.text = "This creature enters prepared. (While it's prepared, you may cast a copy of its spell. Doing so unprepares it.)\n// Twofold Intent {1}{W} Sorcery — Target creature gets +1/+0 and gains double strike until end of turn.".to_string();
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
    fn twofold_intent_pumps_and_grants_double_strike() {
        let mut state = build_game(1, &[&[], &[]]);
        let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
        let bears = state.add_card(PlayerId(0), c, Zone::Battlefield);
        let effect = state.card_db().get(TWOFOLD_INTENT).unwrap().spell_effect().unwrap().clone();
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
        assert_eq!(cc.power, Some(3), "+1 power");
        assert!(cc.has_keyword(Keyword::DoubleStrike), "gained double strike");
    }
}
