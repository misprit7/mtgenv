//! Dina's Guidance — `{1}{B}{G}` Instant (first printed SOS).
//!
//! Oracle: "Search your library for a creature card, reveal it, put it into your hand or graveyard,
//! then shuffle."
//!
//! **Fully implemented** — a modal tutor: choose one — search a creature card to your hand, or to
//! your graveyard (each mode is a `Search` with a different destination; the search shuffles).

use crate::basics::{CardType, Color, Zone, ZoneDest, ZonePos};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::target::CardFilter;
use crate::effects::value::PlayerRef;
use crate::effects::{Effect, Mode};

/// grp id (per-set ids live near their cards).
pub const DINAS_GUIDANCE: u32 = 275;

fn search_creature_to(zone: Zone) -> Effect {
    Effect::Search {
        who: PlayerRef::Controller,
        zone: Zone::Library,
        filter: CardFilter::HasCardType(CardType::Creature),
        min: 0,
        max: 1,
        to: ZoneDest { zone, pos: ZonePos::Any },
        tapped: false,
    }
}

pub fn register(db: &mut CardDb) {
    let effect = Effect::Modal {
        modes: vec![
            Mode { label: "Put it into your hand".to_string(), effect: search_creature_to(Zone::Hand) },
            Mode { label: "Put it into your graveyard".to_string(), effect: search_creature_to(Zone::Graveyard) },
        ],
        min: 1,
        max: 1,
        allow_repeat: false,
    };
    let mut def = spell(
        DINAS_GUIDANCE,
        "Dina's Guidance",
        CardType::Instant,
        Color::Black,
        mana_cost(1, &[(Color::Black, 1), (Color::Green, 1)]),
        effect,
    )
    .with_text("Search your library for a creature card, reveal it, put it into your hand or graveyard, then shuffle.");
    def.chars.colors = vec![Color::Black, Color::Green];
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn dinas_guidance_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(DINAS_GUIDANCE).unwrap();
        assert_eq!(def.chars.colors, vec![Color::Black, Color::Green]);
        assert!(def.fully_implemented);
        expect![[r#"
            Modal {
                modes: [
                    Mode {
                        label: "Put it into your hand",
                        effect: Search {
                            who: Controller,
                            zone: Library,
                            filter: HasCardType(
                                Creature,
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
                    Mode {
                        label: "Put it into your graveyard",
                        effect: Search {
                            who: Controller,
                            zone: Library,
                            filter: HasCardType(
                                Creature,
                            ),
                            min: 0,
                            max: 1,
                            to: ZoneDest {
                                zone: Graveyard,
                                pos: Any,
                            },
                            tapped: false,
                        },
                    },
                ],
                min: 1,
                max: 1,
                allow_repeat: false,
            }"#]].assert_eq(&format!("{:#?}", def.spell_effect().unwrap()));
    }

    /// Behaviour (hand mode): a creature is tutored from library into hand.
    #[test]
    fn dinas_guidance_tutors_to_hand() {
        use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView};
        use crate::basics::Zone;
        use crate::cards::{build_game, grp};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        // Agent: pick mode 0 (hand), then pick the creature in the search.
        #[derive(Clone)] struct PickFirst;
        impl Agent for PickFirst {
            fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
                match req {
                    DecisionRequest::ChooseModes { .. } => DecisionResponse::Indices(vec![0]),
                    DecisionRequest::SelectCards { .. } => DecisionResponse::Indices(vec![0]),
                    _ => DecisionResponse::Pass,
                }
            }
        }
        let mut state = build_game(1, &[&[grp::GRIZZLY_BEARS, grp::FOREST], &[]]);
        let effect = state.card_db().get(DINAS_GUIDANCE).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(PickFirst), Box::new(PickFirst)]);
        e.resolve_effect(&effect, &ResolutionCtx { controller: Some(PlayerId(0)), ..Default::default() }, WbReason::Resolve(StackId(0)));
        assert!(e.state.players[0].hand.iter().any(|&o| e.state.object(o).chars.name == "Grizzly Bears"), "tutored the creature to hand");
    }
}
