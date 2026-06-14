//! Fabled Passage — Land (first printed ELD, Throne of Eldraine).
//!
//! Oracle: "{T}, Sacrifice this land: Search your library for a basic land card, put it onto the
//! battlefield tapped, then shuffle. Then if you control four or more lands, untap that land."
//!
//! **Fully implemented:** the activated `{T}, Sacrifice this:` (TapSelf + Sacrifice `ItSelf`) →
//! `Sequence[ search a basic onto the battlefield tapped (C5), Conditional{ if you control 4+ lands,
//! untap that land } ]`. The "untap that land" references the just-fetched permanent via
//! `EffectTarget::Searched(0)` (the 0th permanent this Search fetched), gated on
//! `Condition::CountAtLeast(lands you control ≥ 4)` and applied with `Effect::Tap{ tap: false }`
//! (cap `bcff1cd`). The count is taken after the fetch, so the new land itself counts toward the 4.

use crate::basics::{CardType, Zone};
use crate::cards::helpers::{fetch_basic_tapped, sacrifice_self};
use crate::cards::{CardDb, CardDef};
use crate::effects::ability::{Ability, Cost, CostComponent, Timing};
use crate::effects::condition::Condition;
use crate::effects::target::CardFilter;
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};
use crate::state::Characteristics;

/// grp id (per-set ids live near their cards).
pub const FABLED_PASSAGE: u32 = 106;

pub fn register(db: &mut CardDb) {
    let chars = Characteristics {
        name: "Fabled Passage".to_string(),
        card_types: vec![CardType::Land],
        grp_id: FABLED_PASSAGE,
        ..Default::default()
    };
    // Fetch a basic (tapped), then "if you control 4+ lands, untap that land" — the untap references
    // the just-fetched permanent (`Searched(0)`), gated on the land count taken after the fetch.
    let effect = Effect::Sequence(vec![
        fetch_basic_tapped(),
        Effect::Conditional {
            cond: Condition::CountAtLeast {
                zone: Zone::Battlefield,
                filter: CardFilter::HasCardType(CardType::Land),
                controller: Some(PlayerRef::Controller),
                n: ValueExpr::Fixed(4),
            },
            then: Box::new(Effect::Tap { what: EffectTarget::Searched(0), tap: false }),
            otherwise: None,
        },
    ]);
    db.insert(CardDef {
        chars,
        abilities: vec![Ability::Activated {
            cost: Cost {
                mana: None,
                components: vec![
                    CostComponent::TapSelf,
                    CostComponent::Sacrifice(sacrifice_self()),
                ],
            },
            effect,
            timing: Timing::Instant,
            restriction: None,
            is_mana: false,
        }],
        text: "{T}, Sacrifice this land: Search your library for a basic land card, put it onto the battlefield tapped, then shuffle. Then if you control four or more lands, untap that land.".to_string(),
        fully_implemented: true,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn fabled_passage_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(FABLED_PASSAGE).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Land]);
        assert!(!def.is_mana_source()); // a fetch, not a mana source
        assert!(def.fully_implemented); // fetch + conditional untap-that-land both implemented
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
                    effect: Sequence(
                        [
                            Search {
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
                            Conditional {
                                cond: CountAtLeast {
                                    zone: Battlefield,
                                    filter: HasCardType(
                                        Land,
                                    ),
                                    controller: Some(
                                        Controller,
                                    ),
                                    n: Fixed(
                                        4,
                                    ),
                                },
                                then: Tap {
                                    what: Searched(
                                        0,
                                    ),
                                    tap: false,
                                },
                                otherwise: None,
                            },
                        ],
                    ),
                    timing: Instant,
                    restriction: None,
                    is_mana: false,
                },
            ]"#]].assert_eq(&format!("{:#?}", def.abilities));
    }

    /// Behaviour: resolving the activated ability tutors a basic land from your library onto the
    /// battlefield **tapped** (and it stays tapped — you control < 4 lands). Snapshot the resolved zones.
    #[test]
    fn fabled_passage_fetches_a_basic_tapped() {
        use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView};
        use crate::cards::{build_game, grp};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        use expect_test::expect;

        // Takes any "may" choice and picks the offered card(s) (1+ up to max).
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

        let mut state = build_game(1, &[&[grp::FOREST], &[]]); // P0 library = one Forest (a basic)
        let fetch = match &state.card_db().get(FABLED_PASSAGE).unwrap().abilities[0] {
            Ability::Activated { effect, .. } => effect.clone(),
            o => panic!("expected Activated, got {o:?}"),
        };
        let mut e = Engine::new(state, vec![Box::new(TakeItAgent), Box::new(TakeItAgent)]);
        e.resolve_effect(
            &fetch,
            &ResolutionCtx { controller: Some(PlayerId(0)), ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        let land = e.state.players[0].battlefield.first().copied();
        let status = land.map(|l| format!("{:?}", e.state.objects.get(&l).unwrap().status));
        let render = format!(
            "library={} battlefield={} fetched_status={:?}",
            e.state.players[0].library.len(),
            e.state.players[0].battlefield.len(),
            status,
        );
        expect![[r#"library=0 battlefield=1 fetched_status=Some("Status { tapped: true, flipped: false, face_down: false, phased_out: false }")"#]].assert_eq(&render);
    }
}
