//! Icetill Explorer — `{2}{G}{G}` Creature — Insect Scout 2/4 (first printed EOE, Edge of Eternities).
//!
//! Oracle:
//!   You may play an additional land on each of your turns.
//!   You may play lands from your graveyard.
//!   Landfall — Whenever a land you control enters, mill a card.
//!
//! **Fully implemented** (C18 land-play permissions landed, cap `3ca7fef`):
//! - "You may play an additional land on each of your turns." — `Ability::Static{
//!   StaticContribution::ExtraLandPlays(1) }` (the land-play limit = 1 + Σ extra plays, CR 505.5b).
//! - "You may play lands from your graveyard." — `Ability::Static{ PlayLandsFrom(Zone::Graveyard) }`
//!   (the land-play legality offers graveyard lands while this is in play).
//! - "Landfall — Whenever a land you control enters, mill a card." — `Triggered{PermanentEnters(land
//!   you control)}` → `Mill` (C4 + C3).
//!
//! These two permissions are player-level statics: the engine reads them directly from the
//! controller's permanents in the land-play legality, not painted on objects — so `affects: itself()`.

use crate::basics::{Color, Zone};
use crate::cards::helpers::{itself, land_you_control};
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern, StaticContribution};
use crate::effects::condition::Duration;
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const ICETILL_EXPLORER: u32 = 104;

pub fn register(db: &mut CardDb) {
    let explorer = creature(
        ICETILL_EXPLORER,
        "Icetill Explorer",
        &[CreatureType::Insect, CreatureType::Scout],
        Color::Green,
        mana_cost(2, &[(Color::Green, 2)]),
        2,
        4,
        vec![
            // "You may play an additional land on each of your turns."
            Ability::Static {
                contribution: StaticContribution::ExtraLandPlays(1),
                affects: itself(),
                duration: Duration::WhileSourcePresent,
            },
            // "You may play lands from your graveyard."
            Ability::Static {
                contribution: StaticContribution::PlayLandsFrom(Zone::Graveyard),
                affects: itself(),
                duration: Duration::WhileSourcePresent,
            },
            // "Landfall — Whenever a land you control enters, mill a card."
            Ability::Triggered {
                event: EventPattern::PermanentEnters(land_you_control()),
                condition: None,
                intervening_if: false,
                effect: Effect::Mill {
                    who: PlayerRef::Controller,
                    count: ValueExpr::Fixed(1),
                },
            },
        ],
    );
    db.insert(explorer.with_text(
        "You may play an additional land on each of your turns.\nYou may play lands from your graveyard.\nLandfall — Whenever a land you control enters, mill a card.",
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn icetill_explorer_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(ICETILL_EXPLORER).unwrap();
        assert_eq!(def.chars.power, Some(2));
        assert_eq!(def.chars.toughness, Some(4));
        assert_eq!(def.chars.subtypes, vec![CreatureType::Insect.into(), CreatureType::Scout.into()]);
        assert!(!def.is_mana_source());
        assert!(def.fully_implemented); // two land-play statics (C18) + landfall mill
        // Two land-play permission statics (ExtraLandPlays + PlayLandsFrom Graveyard) + landfall mill.
        expect![[r#"
            [
                Static {
                    contribution: ExtraLandPlays(
                        1,
                    ),
                    affects: SelectSpec {
                        zone: Battlefield,
                        filter: ItSelf,
                        chooser: Controller,
                        min: Fixed(
                            0,
                        ),
                        max: Fixed(
                            0,
                        ),
                    },
                    duration: WhileSourcePresent,
                },
                Static {
                    contribution: PlayLandsFrom(
                        Graveyard,
                    ),
                    affects: SelectSpec {
                        zone: Battlefield,
                        filter: ItSelf,
                        chooser: Controller,
                        min: Fixed(
                            0,
                        ),
                        max: Fixed(
                            0,
                        ),
                    },
                    duration: WhileSourcePresent,
                },
                Triggered {
                    event: PermanentEnters(
                        All(
                            [
                                HasCardType(
                                    Land,
                                ),
                                ControlledBy(
                                    Controller,
                                ),
                            ],
                        ),
                    ),
                    condition: None,
                    intervening_if: false,
                    effect: Mill {
                        who: Controller,
                        count: Fixed(
                            1,
                        ),
                    },
                },
            ]"#]].assert_eq(&format!("{:#?}", def.abilities));
    }

    /// Behaviour: the landfall trigger mills one card (top of your library → your graveyard).
    #[test]
    fn icetill_landfall_mills_a_card() {
        use crate::agent::RandomAgent;
        use crate::basics::Zone;
        use crate::cards::{build_game, grp};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        let mut state = build_game(1, &[&[grp::FOREST, grp::FOREST], &[]]); // P0 library = 2 Forests
        let icetill_chars = state.card_db().get(ICETILL_EXPLORER).unwrap().chars.clone();
        let icetill = state.add_card(PlayerId(0), icetill_chars, Zone::Battlefield);
        let mill = match &state.card_db().get(ICETILL_EXPLORER).unwrap().abilities[2] {
            Ability::Triggered { effect, .. } => effect.clone(),
            o => panic!("expected landfall mill Triggered, got {o:?}"),
        };
        let lib_before = state.players[0].library.len();
        let mut e = Engine::new(
            state,
            vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))],
        );
        e.resolve_effect(
            &mill,
            &ResolutionCtx { controller: Some(PlayerId(0)), source: Some(icetill), ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        assert_eq!(e.state.players[0].library.len(), lib_before - 1); // one card milled
        assert_eq!(e.state.players[0].graveyard.len(), 1);
    }

    /// #60 end-to-end (REAL land drop + trigger loop): with Icetill in play, playing a land you
    /// control fires landfall — "mill a card." Driven via `play_land` → `run_agenda` → `resolve_top`:
    /// the library shrinks by one and that card lands in the graveyard. (The two static land-play
    /// *permissions* are verified by the engine's `extra_land_plays…_c18` legality test.)
    #[test]
    fn icetill_landfall_mills_via_real_land_drop() {
        use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView};
        use crate::basics::Zone;
        use crate::cards::{grp, starter_db};
        use crate::ids::PlayerId;
        use crate::priority::Engine;
        use crate::state::GameState;
        use std::sync::Arc;

        #[derive(Clone)]
        struct PassiveAgent;
        impl Agent for PassiveAgent {
            fn decide(&mut self, _v: &PlayerView, _req: &DecisionRequest) -> DecisionResponse {
                DecisionResponse::Pass
            }
        }

        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(starter_db()));
        let icetill = {
            let c = state.card_db().get(ICETILL_EXPLORER).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        let _ = icetill;
        for _ in 0..2 {
            let c = state.card_db().get(grp::FOREST).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Library); // 2 cards available to mill
        }
        let land = {
            let c = state.card_db().get(grp::FOREST).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Hand)
        };
        let mut e = Engine::new(state, vec![Box::new(PassiveAgent), Box::new(PassiveAgent)]);
        let lib_before = e.state.players[0].library.len();

        e.play_land(PlayerId(0), land);
        e.run_agenda();
        while !e.state.stack.items.is_empty() {
            e.resolve_top();
            e.run_agenda();
        }

        assert_eq!(e.state.players[0].library.len(), lib_before - 1, "landfall milled one card");
        assert_eq!(e.state.players[0].graveyard.len(), 1, "the milled card is in the graveyard");
    }
}
