//! Environmental Scientist — `{1}{G}` Creature — Human Druid 2/2 (first printed SOS).
//!
//! Oracle: "When this creature enters, you may search your library for a basic land card, reveal
//! it, put it into your hand, then shuffle."
//!
//! **Fully implemented** — an ETB triggered `Search` for a basic land to hand. The "you may" is the
//! search's `min: 0` (the controller may find 0 = decline, or 1 basic); the engine shuffles after.

use crate::basics::{Color, Zone, ZoneDest, ZonePos};
use crate::cards::helpers::basic_land_filter;
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern};
use crate::effects::value::PlayerRef;
use crate::effects::Effect;
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const ENVIRONMENTAL_SCIENTIST: u32 = 212;

pub fn register(db: &mut CardDb) {
    db.insert(
        creature(
            ENVIRONMENTAL_SCIENTIST,
            "Environmental Scientist",
            &[CreatureType::Human, CreatureType::Druid],
            Color::Green,
            mana_cost(1, &[(Color::Green, 1)]),
            2,
            2,
            vec![Ability::Triggered {
                event: EventPattern::SelfEnters,
                condition: None,
                intervening_if: false,
                effect: Effect::Search {
                    who: PlayerRef::Controller,
                    zone: Zone::Library,
                    filter: basic_land_filter(),
                    min: 0,
                    max: 1,
                    to: ZoneDest { zone: Zone::Hand, pos: ZonePos::Any },
                    tapped: false,
                },
            }],
        )
        .with_text("When this creature enters, you may search your library for a basic land card, reveal it, put it into your hand, then shuffle."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn environmental_scientist_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(ENVIRONMENTAL_SCIENTIST).unwrap();
        assert_eq!(def.chars.power, Some(2));
        assert!(def.fully_implemented);
        expect![[r#"
            [
                Triggered {
                    event: SelfEnters,
                    condition: None,
                    intervening_if: false,
                    effect: Search {
                        who: Controller,
                        zone: Library,
                        filter: All(
                            [
                                HasCardType(
                                    Land,
                                ),
                                Supertype(
                                    Basic,
                                ),
                            ],
                        ),
                        min: 0,
                        max: 1,
                        to: ZoneDest {
                            zone: Hand,
                            pos: Any,
                        },
                        tapped: false,
                    },
                },
            ]"#]].assert_eq(&format!("{:#?}", def.abilities));
    }

    /// Behaviour: resolving the ETB search (with the controller opting in) fetches a basic land from
    /// the library into hand.
    #[test]
    fn environmental_scientist_fetches_basic_to_hand() {
        use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView};
        use crate::basics::Zone;
        use crate::cards::{build_game, grp};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;

        // Opts into the "may" search and takes the offered basic.
        #[derive(Clone)]
        struct TakeItAgent;
        impl Agent for TakeItAgent {
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

        // P0's library holds a Forest to fetch.
        let mut state = build_game(1, &[&[grp::FOREST], &[]]);
        let chars = state.card_db().get(ENVIRONMENTAL_SCIENTIST).unwrap().chars.clone();
        let src = state.add_card(PlayerId(0), chars, Zone::Battlefield);
        let etb = match &state.card_db().get(ENVIRONMENTAL_SCIENTIST).unwrap().abilities[0] {
            Ability::Triggered { effect, .. } => effect.clone(),
            o => panic!("expected ETB Triggered, got {o:?}"),
        };
        let mut e = Engine::new(state, vec![Box::new(TakeItAgent), Box::new(TakeItAgent)]);
        e.resolve_effect(
            &etb,
            &ResolutionCtx { controller: Some(PlayerId(0)), source: Some(src), ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        let forest_in_hand = e.state.players[0]
            .hand
            .iter()
            .any(|&o| e.state.object(o).chars.grp_id == grp::FOREST);
        assert!(forest_in_hand, "a basic land was fetched into hand");
        assert!(e.state.players[0].library.is_empty(), "the library's only Forest was taken");
    }
}
