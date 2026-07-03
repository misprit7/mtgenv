//! Stirring Honormancer — `{2}{W}{W/B}{B}` Creature — Rhino Bard 4/5 (first printed SOS).
//!
//! Oracle: "When this creature enters, look at the top X cards of your library, where X is the number
//! of creatures you control. Put one of those cards into your hand and the rest into your graveyard."
//!
//! **Fully implemented** — a hybrid cost (`{W/B}` pip) + an ETB `LookAndPick` that looks at the top
//! `Count(creatures you control)`, keeps one to hand and bins the rest to the graveyard.

use crate::basics::{CardType, Color, Zone};
use crate::cards::{creature, mana_cost_hybrid, CardDb};
use crate::effects::ability::{Ability, EventPattern};
use crate::effects::target::CardFilter;
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const STIRRING_HONORMANCER: u32 = 295;

pub fn register(db: &mut CardDb) {
    let creatures_you_control = ValueExpr::Count {
        zone: Zone::Battlefield,
        filter: CardFilter::HasCardType(CardType::Creature),
        controller: Some(PlayerRef::Controller),
    };
    let mut def = creature(
        STIRRING_HONORMANCER,
        "Stirring Honormancer",
        &[CreatureType::Rhino, CreatureType::Bard],
        Color::White,
        mana_cost_hybrid(2, &[(Color::White, 1), (Color::Black, 1)], &[(Color::White, Color::Black)]),
        4,
        5,
        vec![Ability::Triggered {
            event: EventPattern::SelfEnters,
            condition: None,
            intervening_if: false,
            effect: Effect::LookAndPick {
                count: creatures_you_control,
                take: ValueExpr::Fixed(1),
                take_to: Zone::Hand,
                rest_to: Zone::Graveyard,
                take_filter: CardFilter::Any,
            },
        }],
    )
    .with_text("When this creature enters, look at the top X cards of your library, where X is the number of creatures you control. Put one of those cards into your hand and the rest into your graveyard.");
    def.chars.colors = vec![Color::White, Color::Black];
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stirring_honormancer_shape() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(STIRRING_HONORMANCER).unwrap();
        assert_eq!(def.chars.colors, vec![Color::White, Color::Black]);
        assert_eq!(def.chars.mana_cost.as_ref().unwrap().hybrid, vec![(Color::White, Color::Black)]);
        assert_eq!(def.chars.mana_value(), 5, "{{2}}{{W}}{{W/B}}{{B}} = MV 5");
        assert!(def.fully_implemented);
    }

    #[test]
    fn stirring_honormancer_etb_looks_and_keeps_one() {
        use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView};
        use crate::cards::{build_game, grp};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        #[derive(Clone)] struct KeepFirst;
        impl Agent for KeepFirst {
            fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
                match req { DecisionRequest::SelectCards { .. } => DecisionResponse::Indices(vec![0]), _ => DecisionResponse::Pass }
            }
        }
        // Library of 3; two creatures you control → look at top 2, keep 1, bin 1.
        let lib: Vec<u32> = std::iter::repeat(grp::FOREST).take(3).collect();
        let mut state = build_game(1, &[&lib, &[]]);
        state.add_card(PlayerId(0), state.card_db().get(STIRRING_HONORMANCER).unwrap().chars.clone(), Zone::Battlefield);
        state.add_card(PlayerId(0), state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Battlefield);
        let eff = match &state.card_db().get(STIRRING_HONORMANCER).unwrap().abilities[0] {
            Ability::Triggered { effect, .. } => effect.clone(), o => panic!("{o:?}") };
        let mut e = Engine::new(state, vec![Box::new(KeepFirst), Box::new(KeepFirst)]);
        let (hand0, gy0) = (e.state.players[0].hand.len(), e.state.players[0].graveyard.len());
        e.resolve_effect(&eff, &ResolutionCtx { controller: Some(PlayerId(0)), ..Default::default() }, WbReason::Resolve(StackId(0)));
        assert_eq!(e.state.players[0].hand.len(), hand0 + 1, "kept one to hand");
        assert_eq!(e.state.players[0].graveyard.len(), gy0 + 1, "binned one (X=2 creatures → looked at 2)");
    }
}
