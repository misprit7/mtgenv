//! Tam, Observant Sequencer // Deep Sight — `{2}{G}{U}` Legendary Creature — Gorgon Wizard 4/3 //
//! `{G}{U}` Sorcery (first printed SOS). A **Prepare** DFC — the **landfall** variant.
//!
//! Front: "Landfall — Whenever a land you control enters, Tam becomes prepared."
//! Back (Deep Sight): "You draw a card and gain 1 life."
//!
//! **Fully implemented** — the prepare trigger is a `PermanentEnters(land you control)` ability whose
//! effect is [`Effect::BecomePrepared`] (an ordinary landfall trigger). The back draws one and gains one.

use crate::basics::{CardType, Color};
use crate::cards::{creature, helpers, mana_cost, CardDb};
use crate::cards::spell;
use crate::effects::ability::EventPattern;
use crate::effects::target::CardFilter;
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;
use crate::subtypes::{CreatureType, Supertype};

pub const TAM_OBSERVANT_SEQUENCER: u32 = 387;
pub const DEEP_SIGHT: u32 = 9714;

pub fn register(db: &mut CardDb) {
    db.insert(
        spell(
            DEEP_SIGHT,
            "Deep Sight",
            CardType::Sorcery,
            Color::Blue,
            mana_cost(0, &[(Color::Green, 1), (Color::Blue, 1)]),
            Effect::Sequence(vec![
                Effect::Draw { who: PlayerRef::Controller, count: ValueExpr::Fixed(1) },
                Effect::GainLife { who: PlayerRef::Controller, amount: ValueExpr::Fixed(1) },
            ]),
        )
        .with_text("You draw a card and gain 1 life."),
    );
    let land_you_control = CardFilter::All(vec![
        CardFilter::HasCardType(CardType::Land),
        CardFilter::ControlledBy(PlayerRef::Controller),
    ]);
    let mut front = creature(
        TAM_OBSERVANT_SEQUENCER,
        "Tam, Observant Sequencer",
        &[CreatureType::Gorgon, CreatureType::Wizard],
        Color::Green,
        mana_cost(2, &[(Color::Green, 1), (Color::Blue, 1)]),
        4,
        3,
        helpers::prepared_abilities(DEEP_SIGHT, EventPattern::PermanentEnters(land_you_control), None, false),
    );
    front.chars.colors = vec![Color::Green, Color::Blue];
    front.chars.supertypes = vec![Supertype::Legendary];
    front.text = "Landfall — Whenever a land you control enters, Tam becomes prepared. (While it's prepared, you may cast a copy of its spell. Doing so unprepares it.)\n// Deep Sight {G}{U} Sorcery — You draw a card and gain 1 life.".to_string();
    db.insert(front);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::RandomAgent;
    use crate::cards::{build_game, grp};
    use crate::effects::ability::Ability;
    use crate::effects::action::{ResolutionCtx, WbReason};
    use crate::ids::{PlayerId, StackId};
    use crate::priority::Engine;

    #[test]
    fn tam_landfall_prepare_and_deep_sight() {
        let mut db = CardDb::default();
        register(&mut db);
        let f = db.get(TAM_OBSERVANT_SEQUENCER).unwrap();
        assert!(matches!(f.abilities[0], Ability::Prepare { spell: DEEP_SIGHT }));
        assert!(matches!(
            f.abilities[1],
            Ability::Triggered { event: EventPattern::PermanentEnters(_), .. }
        ));
        // Behaviour: the back draws one and gains one.
        let state = build_game(1, &[&[grp::FOREST], &[]]);
        let effect = state.card_db().get(DEEP_SIGHT).unwrap().spell_effect().unwrap().clone();
        let hand0 = state.player(PlayerId(0)).hand.len();
        let life0 = state.player(PlayerId(0)).life;
        let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
        e.resolve_effect(
            &effect,
            &ResolutionCtx { controller: Some(PlayerId(0)), ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        assert_eq!(e.state.player(PlayerId(0)).hand.len(), hand0 + 1, "drew one");
        assert_eq!(e.state.player(PlayerId(0)).life, life0 + 1, "gained one");
    }
}
