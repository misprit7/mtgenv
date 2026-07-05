//! Sanar, Unfinished Genius // Wild Idea — `{U}{R}` Legendary Creature — Goblin Sorcerer 0/4 //
//! `{3}{U}{R}` Sorcery (first printed SOS). A **Prepare** DFC (enters prepared).
//!
//! Front: "Sanar enters prepared."
//! Back (Wild Idea): "Search your library for an instant or sorcery card, reveal it, put it into your
//! hand, then shuffle."
//!
//! **Fully implemented** — enters-prepared via [`helpers::enters_prepared`]; the back searches the
//! library for an instant or sorcery card to hand (the reveal is cosmetic).

use crate::basics::{CardType, Color, Zone, ZoneDest, ZonePos};
use crate::cards::{creature, helpers, mana_cost, spell, CardDb};
use crate::effects::value::PlayerRef;
use crate::effects::Effect;
use crate::subtypes::{CreatureType, Supertype};

pub const SANAR_UNFINISHED_GENIUS: u32 = 391;
pub const WILD_IDEA: u32 = 9718;

pub fn register(db: &mut CardDb) {
    let wild_idea = Effect::Search {
        who: PlayerRef::Controller,
        zone: Zone::Library,
        filter: helpers::instant_or_sorcery(),
        min: 0,
        max: 1,
        to: ZoneDest { zone: Zone::Hand, pos: ZonePos::Any },
        tapped: false,
    };
    let mut back = spell(
        WILD_IDEA,
        "Wild Idea",
        CardType::Sorcery,
        Color::Blue,
        mana_cost(3, &[(Color::Blue, 1), (Color::Red, 1)]),
        wild_idea,
    )
    .with_text("Search your library for an instant or sorcery card, reveal it, put it into your hand, then shuffle.");
    back.chars.colors = vec![Color::Blue, Color::Red];
    db.insert(back);

    let mut front = creature(
        SANAR_UNFINISHED_GENIUS,
        "Sanar, Unfinished Genius",
        &[CreatureType::Goblin, CreatureType::Sorcerer],
        Color::Blue,
        mana_cost(0, &[(Color::Blue, 1), (Color::Red, 1)]),
        0,
        4,
        helpers::enters_prepared(WILD_IDEA),
    );
    front.chars.colors = vec![Color::Blue, Color::Red];
    front.chars.supertypes = vec![Supertype::Legendary];
    front.text = "Sanar enters prepared. (While it's prepared, you may cast a copy of its spell. Doing so unprepares it.)\n// Wild Idea {3}{U}{R} Sorcery — Search your library for an instant or sorcery card, reveal it, put it into your hand, then shuffle.".to_string();
    db.insert(front);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView};
    use crate::cards::{build_game, grp};
    use crate::effects::ability::{Ability, EventPattern};
    use crate::effects::action::{ResolutionCtx, WbReason};
    use crate::ids::{PlayerId, StackId};
    use crate::priority::Engine;

    struct TakeIt;
    impl Agent for TakeIt {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::Confirm { .. } => DecisionResponse::Bool(true),
                DecisionRequest::SelectCards { from, min, max, .. } => {
                    let n = (*min).max(1).min(*max).min(from.len() as u32);
                    DecisionResponse::Indices((0..n).collect())
                }
                _ => DecisionResponse::Pass,
            }
        }
    }

    #[test]
    fn sanar_enters_prepared_and_wild_idea_fetches_a_spell() {
        let mut db = CardDb::default();
        register(&mut db);
        let f = db.get(SANAR_UNFINISHED_GENIUS).unwrap();
        assert!(matches!(f.abilities[0], Ability::Prepare { spell: WILD_IDEA }));
        assert!(matches!(f.abilities[1], Ability::Triggered { event: EventPattern::SelfEnters, .. }));
        // Behaviour: library holds a Lightning Bolt (an instant); Wild Idea fetches it to hand.
        let state = build_game(1, &[&[grp::LIGHTNING_BOLT], &[]]);
        let effect = state.card_db().get(WILD_IDEA).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(TakeIt), Box::new(TakeIt)]);
        e.resolve_effect(
            &effect,
            &ResolutionCtx { controller: Some(PlayerId(0)), ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        assert!(
            e.state.player(PlayerId(0)).hand.iter().any(|&o| e.state.object(o).chars.grp_id == grp::LIGHTNING_BOLT),
            "fetched the instant to hand"
        );
    }
}
