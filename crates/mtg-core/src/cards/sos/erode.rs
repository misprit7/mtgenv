//! Erode — `{W}` Instant (first printed SOS).
//!
//! Oracle: "Destroy target creature or planeswalker. Its controller may search their library for a
//! basic land card, put it onto the battlefield tapped, then shuffle."
//!
//! Fully implemented (no deferrals):
//! - Destroy a targeted creature/planeswalker (`Effect::Destroy` over a `Target` permanent — the
//!   single declared target, so `collect_target_specs` prompts for it at cast).
//! - The rider, searched by the *destroyed permanent's controller* (`ControllerOfTarget(0)` — its
//!   controller snapshotted at resolution start, before the Destroy graveyards it). "may search"
//!   is the `min: 0` of the fetch: that player picks 0 (decline) or 1 basic. The engine asks the
//!   `who` player to `SelectCards`, so the opponent — not the caster — makes the choice.

use crate::basics::{CardType, Color};
use crate::cards::helpers::fetch_basic_tapped_by;
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::PlayerRef;
use crate::effects::{Effect, EffectTarget};

/// grp id (per-set ids live near their cards).
pub const ERODE: u32 = 108;

pub fn register(db: &mut CardDb) {
    let effect = Effect::Sequence(vec![
        Effect::Destroy {
            what: EffectTarget::Target(TargetSpec {
                kind: TargetKind::Permanent(CardFilter::AnyOf(vec![
                    CardFilter::HasCardType(CardType::Creature),
                    CardFilter::HasCardType(CardType::Planeswalker),
                ])),
                min: 1,
                max: 1,
                distinct: true,
            }),
        },
        // "Its controller may search …" — the destroyed permanent's controller (target 0).
        fetch_basic_tapped_by(PlayerRef::ControllerOfTarget(0)),
    ]);
    db.insert(
        spell(
            ERODE,
            "Erode",
            CardType::Instant,
            Color::White,
            mana_cost(0, &[(Color::White, 1)]),
            effect,
        )
        .with_text("Destroy target creature or planeswalker. Its controller may search their library for a basic land card, put it onto the battlefield tapped, then shuffle."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn erode_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(ERODE).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Instant]);
        assert!(def.fully_implemented); // no deferred clauses
        expect![[r#"
            Sequence(
                [
                    Destroy {
                        what: Target(
                            TargetSpec {
                                kind: Permanent(
                                    AnyOf(
                                        [
                                            HasCardType(
                                                Creature,
                                            ),
                                            HasCardType(
                                                Planeswalker,
                                            ),
                                        ],
                                    ),
                                ),
                                min: 1,
                                max: 1,
                                distinct: true,
                            },
                        ),
                    },
                    Search {
                        who: ControllerOfTarget(
                            0,
                        ),
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
                ],
            )"#]].assert_eq(&format!("{:#?}", def.spell_effect().unwrap()));
    }

    /// Behaviour: resolving Erode destroys the targeted creature (moves it to its owner's graveyard).
    /// The trailing "its controller may fetch a basic" is a no-op here (empty library, min 0).
    #[test]
    fn erode_destroys_the_target() {
        use crate::agent::RandomAgent;
        use crate::basics::{Target, Zone};
        use crate::cards::{build_game, grp};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        let mut state = build_game(1, &[&[], &[]]);
        let bears_chars = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
        let victim = state.add_card(PlayerId(1), bears_chars, Zone::Battlefield); // opponent's creature
        let erode = state.card_db().get(ERODE).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(
            state,
            vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))],
        );
        e.resolve_effect(
            &erode,
            &ResolutionCtx {
                controller: Some(PlayerId(0)),
                chosen_targets: vec![Target::Object(victim)],
                ..Default::default()
            },
            WbReason::Resolve(StackId(0)),
        );
        assert!(!e.state.players[1].battlefield.contains(&victim), "destroyed (off the battlefield)");
        assert!(e.state.players[1].graveyard.contains(&victim), "in its owner's graveyard");
    }

    /// Behaviour: the "its controller may search …" rider benefits the **destroyed permanent's
    /// controller** (`ControllerOfTarget(0)` = the opponent P1), not the caster (P0). With a basic in
    /// P1's library and P1 choosing to take it, the basic enters **P1's** battlefield **tapped**, the
    /// victim is in P1's graveyard, and **P0 gains nothing**. Pins that the rider's `who` is the
    /// opponent — the subtle clause the destroy-only test couldn't see (empty library, min 0).
    #[test]
    fn erode_controller_fetches_a_basic_tapped_for_the_opponent() {
        use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView};
        use crate::basics::{Target, Zone};
        use crate::cards::{build_game, grp};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;

        // Takes any offered "may" fetch (the opponent here opts in).
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

        // P1 (the opponent) has a Forest to fetch; P0 (the caster) has nothing.
        let mut state = build_game(1, &[&[], &[grp::FOREST]]);
        let bears_chars = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
        let victim = state.add_card(PlayerId(1), bears_chars, Zone::Battlefield); // opponent's creature
        let erode = state.card_db().get(ERODE).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(TakeItAgent), Box::new(TakeItAgent)]);
        e.resolve_effect(
            &erode,
            &ResolutionCtx {
                controller: Some(PlayerId(0)), // P0 casts Erode
                chosen_targets: vec![Target::Object(victim)],
                // The real cast path snapshots each target's controller at resolution start (so
                // `ControllerOfTarget(0)` survives the Destroy). A direct `resolve_effect` must
                // supply it; without it `ControllerOfTarget` would wrongly fall back to the caster.
                target_controllers: vec![Some(PlayerId(1))],
                ..Default::default()
            },
            WbReason::Resolve(StackId(0)),
        );
        // The opponent's library is empty (the Forest was fetched) and now sits on P1's battlefield…
        assert!(e.state.players[1].graveyard.contains(&victim), "victim destroyed → P1 graveyard");
        let fetched: Vec<_> = e.state.players[1]
            .battlefield
            .iter()
            .filter(|&&o| e.state.objects.get(&o).map(|x| x.chars.grp_id) == Some(grp::FOREST))
            .copied()
            .collect();
        assert_eq!(fetched.len(), 1, "exactly one basic fetched onto P1's battlefield");
        assert!(
            e.state.objects.get(&fetched[0]).unwrap().status.tapped,
            "the fetched basic enters tapped"
        );
        assert!(e.state.players[1].library.is_empty(), "P1's library emptied by the fetch");
        // The caster (P0) gains nothing from the opponent's rider.
        assert!(e.state.players[0].battlefield.is_empty(), "P0 (caster) gets no land");
    }
}
