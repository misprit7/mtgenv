//! Paradox Surveyor — `{G}{G/U}{U}` Creature — Elf Druid 3/3 (first printed SOS).
//!
//! Oracle: "Reach / When this creature enters, look at the top five cards of your library. You may
//! reveal a land card or a card with {X} in its mana cost from among them and put it into your hand.
//! Put the rest on the bottom of your library in a random order."
//!
//! **Fully implemented** — printed Reach + a hybrid cost (`{G/U}`) + an ETB filtered `LookAndPick`
//! (look 5, may take one **land or {X}-cost** card to hand, rest to the bottom). Multicolored (G/U).

use crate::basics::{CardType, Color, Zone};
use crate::cards::{creature, mana_cost_hybrid, CardDb};
use crate::effects::ability::{Ability, EventPattern, Keyword};
use crate::effects::target::CardFilter;
use crate::effects::value::ValueExpr;
use crate::effects::Effect;
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const PARADOX_SURVEYOR: u32 = 305;

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        PARADOX_SURVEYOR,
        "Paradox Surveyor",
        &[CreatureType::Elf, CreatureType::Druid],
        Color::Green,
        mana_cost_hybrid(0, &[(Color::Green, 1), (Color::Blue, 1)], &[(Color::Green, Color::Blue)]),
        3,
        3,
        vec![Ability::Triggered {
            event: EventPattern::SelfEnters,
            condition: None,
            intervening_if: false,
            effect: Effect::LookAndPick {
                count: ValueExpr::Fixed(5),
                take: ValueExpr::Fixed(1),
                take_to: Zone::Hand,
                rest_to: Zone::Library,
                take_filter: CardFilter::AnyOf(vec![
                    CardFilter::HasCardType(CardType::Land),
                    CardFilter::HasXInCost,
                ]),
            },
        }],
    );
    def.chars.colors = vec![Color::Green, Color::Blue];
    def.chars.keywords = vec![Keyword::Reach];
    def.text = "Reach\nWhen this creature enters, look at the top five cards of your library. You may reveal a land card or a card with {X} in its mana cost from among them and put it into your hand. Put the rest on the bottom of your library in a random order.".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paradox_surveyor_shape() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(PARADOX_SURVEYOR).unwrap();
        assert_eq!(def.chars.keywords, vec![Keyword::Reach]);
        assert_eq!(def.chars.mana_cost.as_ref().unwrap().hybrid, vec![(Color::Green, Color::Blue)]);
        assert!(def.fully_implemented);
    }

    /// The filtered take only offers a land (or {X}-cost) card: with a Forest among the top and the
    /// rest nonland non-X, the agent can take the Forest to hand.
    #[test]
    fn paradox_surveyor_takes_only_a_land_or_x_card() {
        use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView, SelectReason};
        use crate::cards::{build_game, grp};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        #[derive(Clone)] struct TakeFirst;
        impl Agent for TakeFirst {
            fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
                match req {
                    DecisionRequest::SelectCards { reason: SelectReason::ScryStage, from, .. } if !from.is_empty() => DecisionResponse::Indices(vec![0]),
                    _ => DecisionResponse::Pass,
                }
            }
        }
        // Top five (bottom→top): Bears, Bears, Bears, Bears, Forest → the Forest is a land (takeable),
        // the Grizzlies are nonland non-X (not offered).
        let lib = vec![grp::GRIZZLY_BEARS, grp::GRIZZLY_BEARS, grp::GRIZZLY_BEARS, grp::GRIZZLY_BEARS, grp::FOREST];
        let mut state = build_game(1, &[&lib, &[]]);
        let eff = match &state.card_db().get(PARADOX_SURVEYOR).unwrap().abilities[0] {
            Ability::Triggered { effect, .. } => effect.clone(), o => panic!("{o:?}") };
        let mut e = Engine::new(state, vec![Box::new(TakeFirst), Box::new(TakeFirst)]);
        let hand0 = e.state.players[0].hand.len();
        e.resolve_effect(&eff, &ResolutionCtx { controller: Some(PlayerId(0)), ..Default::default() }, WbReason::Resolve(StackId(0)));
        assert_eq!(e.state.players[0].hand.len(), hand0 + 1, "took the land");
        assert!(e.state.players[0].hand.iter().any(|&o| e.state.object(o).chars.name == "Forest"), "the taken card is the Forest (a land)");
    }
}
