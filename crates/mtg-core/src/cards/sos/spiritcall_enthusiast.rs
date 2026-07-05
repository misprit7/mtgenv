//! Spiritcall Enthusiast // Scrollboost — `{2}{W}` Creature — Cat Cleric 3/3 // `{1}{W}` Sorcery
//! (first printed SOS). A **Prepare** DFC — the **tokens-enter** variant.
//!
//! Front: "Whenever one or more tokens you control enter, this creature becomes prepared."
//! Back (Scrollboost): "One or two target creatures each get +2/+2 until end of turn."
//!
//! **Fully implemented** — the prepare trigger is a `PermanentEnters(token you control)` ability whose
//! effect is [`Effect::BecomePrepared`] (idempotent, so firing per-token matches "one or more"). The
//! back uses `ForEachTarget` over a one-or-two-creature slot, pumping each +2/+2 until end of turn.

use crate::basics::{CardType, Color};
use crate::cards::{creature, helpers, mana_cost, spell, CardDb};
use crate::effects::ability::EventPattern;
use crate::effects::condition::Duration;
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};
use crate::subtypes::{CreatureType, Supertype};

pub const SPIRITCALL_ENTHUSIAST: u32 = 390;
pub const SCROLLBOOST: u32 = 9717;

pub fn register(db: &mut CardDb) {
    let scrollboost = Effect::ForEachTarget {
        slot: TargetSpec {
            kind: TargetKind::Creature(CardFilter::Any),
            min: 1,
            max: 2,
            distinct: true,
        },
        body: Box::new(Effect::PumpPT {
            what: EffectTarget::Each,
            power: ValueExpr::Fixed(2),
            toughness: ValueExpr::Fixed(2),
            duration: Duration::UntilEndOfTurn,
        }),
    };
    db.insert(
        spell(SCROLLBOOST, "Scrollboost", CardType::Sorcery, Color::White, mana_cost(1, &[(Color::White, 1)]), scrollboost)
            .with_text("One or two target creatures each get +2/+2 until end of turn."),
    );
    let token_you_control = CardFilter::All(vec![
        CardFilter::Supertype(Supertype::Token),
        CardFilter::ControlledBy(PlayerRef::Controller),
    ]);
    let mut front = creature(
        SPIRITCALL_ENTHUSIAST,
        "Spiritcall Enthusiast",
        &[CreatureType::Cat, CreatureType::Cleric],
        Color::White,
        mana_cost(2, &[(Color::White, 1)]),
        3,
        3,
        helpers::prepared_abilities(SCROLLBOOST, EventPattern::PermanentEnters(token_you_control), None, false),
    );
    front.text = "Whenever one or more tokens you control enter, this creature becomes prepared. (While it's prepared, you may cast a copy of its spell. Doing so unprepares it.)\n// Scrollboost {1}{W} Sorcery — One or two target creatures each get +2/+2 until end of turn.".to_string();
    db.insert(front);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::RandomAgent;
    use crate::basics::{Target, Zone};
    use crate::cards::{build_game, grp};
    use crate::effects::ability::Ability;
    use crate::effects::action::{ResolutionCtx, WbReason};
    use crate::ids::{PlayerId, StackId};
    use crate::priority::Engine;

    #[test]
    fn spiritcall_token_prepare_and_scrollboost() {
        let mut db = CardDb::default();
        register(&mut db);
        let f = db.get(SPIRITCALL_ENTHUSIAST).unwrap();
        assert!(matches!(f.abilities[0], Ability::Prepare { spell: SCROLLBOOST }));
        assert!(matches!(
            f.abilities[1],
            Ability::Triggered { event: EventPattern::PermanentEnters(_), .. }
        ));
        // Behaviour: pump two chosen creatures +2/+2.
        let mut state = build_game(1, &[&[], &[]]);
        let a = {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        let b = {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        let effect = state.card_db().get(SCROLLBOOST).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
        e.resolve_effect(
            &effect,
            &ResolutionCtx {
                controller: Some(PlayerId(0)),
                chosen_targets: vec![Target::Object(a), Target::Object(b)],
                ..Default::default()
            },
            WbReason::Resolve(StackId(0)),
        );
        assert_eq!(e.state.computed(a).power, Some(4), "first creature +2/+2");
        assert_eq!(e.state.computed(b).toughness, Some(4), "second creature +2/+2");
    }
}
