//! Escape Tunnel — Land (first printed MKM, Murders at Karlov Manor).
//!
//! Oracle:
//!   {T}, Sacrifice this land: Search your library for a basic land card, put it onto the
//!   battlefield tapped, then shuffle.
//!   {T}, Sacrifice this land: Target creature with power 2 or less can't be blocked this turn.
//!
//! **Fully implemented** — two `{T}, Sacrifice this:` activated abilities:
//! - the fetch (`{T}, Sacrifice this:` → a basic onto the battlefield tapped, C5).
//! - "Target creature with power 2 or less can't be blocked this turn." — `GrantQualification{ what:
//!   Target(Creature(PowerAtMost(2))), qualification: CantBeBlocked, UntilEndOfTurn }` (cap 7dd18a9):
//!   a resolution-granted `CantBeBlocked` evasion qualification (combat `can_block` reads it on the
//!   attacker, CR 509.1b), wearing off at cleanup.

use crate::basics::CardType;
use crate::cards::helpers::{fetch_basic_tapped, sacrifice_self};
use crate::cards::{CardDb, CardDef};
use crate::effects::ability::{Ability, Cost, CostComponent, Qualification, Timing};
use crate::effects::condition::Duration;
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::{Effect, EffectTarget};
use crate::state::Characteristics;

/// grp id (per-set ids live near their cards).
pub const ESCAPE_TUNNEL: u32 = 107;

pub fn register(db: &mut CardDb) {
    let chars = Characteristics {
        name: "Escape Tunnel".to_string(),
        card_types: vec![CardType::Land],
        grp_id: ESCAPE_TUNNEL,
        ..Default::default()
    };
    db.insert(CardDef {
        chars,
        abilities: vec![
            // "{T}, Sacrifice this land: Search your library for a basic land card …"
            Ability::Activated {
                cost: Cost {
                    mana: None,
                    components: vec![
                        CostComponent::TapSelf,
                        CostComponent::Sacrifice(sacrifice_self()),
                    ],
                },
                effect: fetch_basic_tapped(),
                timing: Timing::Instant,
                restriction: None,
                is_mana: false,
            },
            // "{T}, Sacrifice this land: Target creature with power 2 or less can't be blocked this turn."
            Ability::Activated {
                cost: Cost {
                    mana: None,
                    components: vec![
                        CostComponent::TapSelf,
                        CostComponent::Sacrifice(sacrifice_self()),
                    ],
                },
                effect: Effect::GrantQualification {
                    what: EffectTarget::Target(TargetSpec {
                        kind: TargetKind::Creature(CardFilter::PowerAtMost(2)),
                        min: 1,
                        max: 1,
                        distinct: true,
                    }),
                    qualification: Qualification::CantBeBlocked,
                    duration: Duration::UntilEndOfTurn,
                },
                timing: Timing::Instant,
                restriction: None,
                is_mana: false,
            },
        ],
        text: "{T}, Sacrifice this land: Search your library for a basic land card, put it onto the battlefield tapped, then shuffle.\n{T}, Sacrifice this land: Target creature with power 2 or less can't be blocked this turn.".to_string(),
        fully_implemented: true,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn escape_tunnel_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(ESCAPE_TUNNEL).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Land]);
        assert!(!def.is_mana_source());
        assert!(def.fully_implemented); // both abilities implemented (fetch + can't-be-blocked)
        // Two activated abilities: the fetch + the "target power≤2 creature can't be blocked" grant.
        expect![[r#"
            [
                Activated {
                    cost: Cost {
                        mana: None,
                        components: [
                            TapSelf,
                            Sacrifice(
                                SelectSpec {
                                    zone: Battlefield,
                                    filter: ItSelf,
                                    chooser: Controller,
                                    min: Fixed(
                                        1,
                                    ),
                                    max: Fixed(
                                        1,
                                    ),
                                },
                            ),
                        ],
                    },
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
                            zone: Battlefield,
                            pos: Any,
                        },
                        tapped: true,
                    },
                    timing: Instant,
                    restriction: None,
                    is_mana: false,
                },
                Activated {
                    cost: Cost {
                        mana: None,
                        components: [
                            TapSelf,
                            Sacrifice(
                                SelectSpec {
                                    zone: Battlefield,
                                    filter: ItSelf,
                                    chooser: Controller,
                                    min: Fixed(
                                        1,
                                    ),
                                    max: Fixed(
                                        1,
                                    ),
                                },
                            ),
                        ],
                    },
                    effect: GrantQualification {
                        what: Target(
                            TargetSpec {
                                kind: Creature(
                                    PowerAtMost(
                                        2,
                                    ),
                                ),
                                min: 1,
                                max: 1,
                                distinct: true,
                            },
                        ),
                        qualification: CantBeBlocked,
                        duration: UntilEndOfTurn,
                    },
                    timing: Instant,
                    restriction: None,
                    is_mana: false,
                },
            ]"#]].assert_eq(&format!("{:#?}", def.abilities));
    }

    /// Behaviour: resolving the fetch ability tutors a basic land from your library onto the
    /// battlefield tapped.
    #[test]
    fn escape_tunnel_fetches_a_basic_tapped() {
        use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView};
        use crate::cards::{build_game, grp};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        use expect_test::expect;

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

        let state = build_game(1, &[&[grp::FOREST], &[]]); // P0 library = one Forest
        let fetch = match &state.card_db().get(ESCAPE_TUNNEL).unwrap().abilities[0] {
            Ability::Activated { effect, .. } => effect.clone(),
            o => panic!("expected fetch Activated, got {o:?}"),
        };
        let mut e = Engine::new(state, vec![Box::new(TakeItAgent), Box::new(TakeItAgent)]);
        e.resolve_effect(
            &fetch,
            &ResolutionCtx { controller: Some(PlayerId(0)), ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        let land = e.state.players[0].battlefield.first().copied();
        let tapped = land.map(|l| e.state.objects.get(&l).unwrap().status.tapped);
        let render = format!(
            "library={} battlefield={} fetched_tapped={:?}",
            e.state.players[0].library.len(),
            e.state.players[0].battlefield.len(),
            tapped,
        );
        expect![["library=0 battlefield=1 fetched_tapped=Some(true)"]].assert_eq(&render);
    }

    /// Behaviour: resolving the *actual* second ability (`abilities[1]`, not a hand-built effect)
    /// against a power-2 creature grants `CantBeBlocked` until end of turn, and it wears off at
    /// cleanup (CR 509.1b / 514.2). Pins the card's real `GrantQualification{ Target(PowerAtMost(2)) }`
    /// IR — the engine's `combat::cant_be_blocked_until_eot_escape_tunnel` only exercises a synthetic
    /// `ChosenIndex` effect, so this is the card-side guard that the ability is wired correctly.
    #[test]
    fn escape_tunnel_makes_a_small_creature_unblockable_until_eot() {
        use crate::agent::RandomAgent;
        use crate::basics::{Target, Zone};
        use crate::cards::{build_game, grp};
        use crate::effects::ability::Qualification;
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;

        let mut state = build_game(1, &[&[], &[]]);
        // Grizzly Bears is a vanilla 2/2 — power 2 ≤ 2, a legal target for the ability.
        let bears_chars = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
        let bears = state.add_card(PlayerId(0), bears_chars, Zone::Battlefield);
        // The card's REAL second ability effect (the can't-be-blocked grant).
        let grant = match &state.card_db().get(ESCAPE_TUNNEL).unwrap().abilities[1] {
            Ability::Activated { effect, .. } => effect.clone(),
            o => panic!("expected the grant Activated, got {o:?}"),
        };
        let mut e = Engine::new(
            state,
            vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))],
        );
        assert!(
            !e.state.computed(bears).has_qualification(Qualification::CantBeBlocked),
            "no qualification before the ability resolves"
        );
        e.resolve_effect(
            &grant,
            &ResolutionCtx {
                controller: Some(PlayerId(0)),
                chosen_targets: vec![Target::Object(bears)],
                ..Default::default()
            },
            WbReason::Resolve(StackId(0)),
        );
        assert!(
            e.state.computed(bears).has_qualification(Qualification::CantBeBlocked),
            "after resolving abilities[1], the power-2 creature can't be blocked this turn"
        );
        e.state.end_of_turn_continuous_cleanup();
        assert!(
            !e.state.computed(bears).has_qualification(Qualification::CantBeBlocked),
            "the can't-be-blocked grant wears off at cleanup"
        );
    }

    /// #60 end-to-end (the REAL activate path, not `resolve_effect`): activate the second ability via
    /// `activate_ability` so the engine pays the **full cost** — `{T}` *and* **Sacrifice this land** —
    /// chooses the target through `ChooseTargets`, puts it on the stack, and `resolve_top` resolves it.
    /// Asserts both halves the resolve-level test couldn't see: (1) the land is **sacrificed** (gone to
    /// graveyard — the cost actually happened), and (2) the power-2 creature gains `CantBeBlocked`.
    #[test]
    fn escape_tunnel_unblockable_via_full_activation() {
        use crate::agent::{Agent, AbilityRef, DecisionRequest, DecisionResponse, PlayerView};
        use crate::basics::Zone;
        use crate::cards::{grp, starter_db};
        use crate::effects::ability::Qualification;
        use crate::ids::PlayerId;
        use crate::priority::Engine;
        use crate::state::GameState;
        use std::sync::Arc;

        // Picks the offered target (slot 0, candidate 0) and accepts any confirm.
        #[derive(Clone)]
        struct PlayAgent;
        impl Agent for PlayAgent {
            fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
                match req {
                    DecisionRequest::ChooseTargets { .. } => DecisionResponse::Pairs(vec![(0, 0)]),
                    DecisionRequest::Confirm { .. } => DecisionResponse::Bool(true),
                    _ => DecisionResponse::Pass,
                }
            }
        }

        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(starter_db()));
        let tunnel = {
            let c = state.card_db().get(ESCAPE_TUNNEL).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        let bears = {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(); // 2/2, power 2 ≤ 2
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        let mut e = Engine::new(state, vec![Box::new(PlayAgent), Box::new(PlayAgent)]);
        e.activate_ability(PlayerId(0), tunnel, AbilityRef(1)); // pays {T} + Sacrifice this land
        e.resolve_top(); // the can't-be-blocked grant resolves

        // The cost actually happened: Escape Tunnel was sacrificed off the battlefield…
        assert!(
            !e.state.players[0].battlefield.contains(&tunnel),
            "Escape Tunnel was sacrificed as a cost (no longer on the battlefield)"
        );
        assert_eq!(e.state.object(tunnel).zone, Zone::Graveyard);
        // …and the effect landed: the power-2 creature can't be blocked this turn.
        assert!(
            e.state.computed(bears).has_qualification(Qualification::CantBeBlocked),
            "the targeted power-2 creature can't be blocked this turn"
        );
    }
}
