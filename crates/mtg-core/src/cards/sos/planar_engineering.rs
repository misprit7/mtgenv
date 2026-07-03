//! Planar Engineering — `{3}{G}` Sorcery (first printed SOS).
//!
//! Oracle: "Sacrifice two lands. Search your library for four basic land cards, put them onto the
//! battlefield tapped, then shuffle."
//!
//! **Fully implemented** — `Effect::Sacrifice` (the caster sacrifices two of their lands), then a
//! `Search` fetching up to four basic lands onto the battlefield tapped. Exercises the `Sacrifice`
//! effect leaf.

use crate::basics::{CardType, Color, Zone, ZoneDest, ZonePos};
use crate::cards::helpers::basic_land_filter;
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::target::{CardFilter, SelectSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;

/// grp id (per-set ids live near their cards).
pub const PLANAR_ENGINEERING: u32 = 230;

pub fn register(db: &mut CardDb) {
    let effect = Effect::Sequence(vec![
        Effect::Sacrifice {
            who: PlayerRef::Controller,
            what: SelectSpec {
                zone: Zone::Battlefield,
                filter: CardFilter::HasCardType(CardType::Land),
                chooser: PlayerRef::Controller,
                min: ValueExpr::Fixed(2),
                max: ValueExpr::Fixed(2),
            },
        },
        Effect::Search {
            who: PlayerRef::Controller,
            zone: Zone::Library,
            filter: basic_land_filter(),
            min: 0,
            max: 4,
            to: ZoneDest { zone: Zone::Battlefield, pos: ZonePos::Any },
            tapped: true,
        },
    ]);
    db.insert(
        spell(
            PLANAR_ENGINEERING,
            "Planar Engineering",
            CardType::Sorcery,
            Color::Green,
            mana_cost(3, &[(Color::Green, 1)]),
            effect,
        )
        .with_text("Sacrifice two lands. Search your library for four basic land cards, put them onto the battlefield tapped, then shuffle."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn planar_engineering_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(PLANAR_ENGINEERING).unwrap();
        assert!(def.fully_implemented);
        expect![[r#"
            Sequence(
                [
                    Sacrifice {
                        who: Controller,
                        what: SelectSpec {
                            zone: Battlefield,
                            filter: HasCardType(
                                Land,
                            ),
                            chooser: Controller,
                            min: Fixed(
                                2,
                            ),
                            max: Fixed(
                                2,
                            ),
                        },
                    },
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
                        max: 4,
                        to: ZoneDest {
                            zone: Battlefield,
                            pos: Any,
                        },
                        tapped: true,
                    },
                ],
            )"#]].assert_eq(&format!("{:#?}", def.spell_effect().unwrap()));
    }

    /// Behaviour: the caster sacrifices two of their three lands (→ graveyard) and fetches basics tapped.
    #[test]
    fn planar_engineering_sacrifices_two_and_fetches() {
        use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView};
        use crate::basics::Zone;
        use crate::cards::{build_game, grp};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;

        #[derive(Clone)]
        struct PickAgent;
        impl Agent for PickAgent {
            fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
                match req {
                    // Take as many as allowed (max) — sacrifices the two required lands and fetches
                    // all available basics.
                    DecisionRequest::SelectCards { max, from, .. } => {
                        let n = (*max).min(from.len() as u32);
                        DecisionResponse::Indices((0..n).collect())
                    }
                    _ => DecisionResponse::Pass,
                }
            }
        }

        // Three Mountains on the battlefield to sacrifice from; two Forests in library to fetch.
        let mut state = build_game(1, &[&[grp::FOREST, grp::FOREST], &[]]);
        let lands: Vec<_> = (0..3)
            .map(|_| {
                let c = state.card_db().get(grp::MOUNTAIN).unwrap().chars.clone();
                state.add_card(PlayerId(0), c, Zone::Battlefield)
            })
            .collect();
        let effect = state.card_db().get(PLANAR_ENGINEERING).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(PickAgent), Box::new(PickAgent)]);
        e.resolve_effect(
            &effect,
            &ResolutionCtx { controller: Some(PlayerId(0)), ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        let sacked = lands.iter().filter(|&&l| e.state.players[0].graveyard.contains(&l)).count();
        assert_eq!(sacked, 2, "exactly two lands were sacrificed to the graveyard");
        let fetched = e.state.players[0]
            .battlefield
            .iter()
            .filter(|&&o| e.state.object(o).chars.grp_id == grp::FOREST && e.state.object(o).status.tapped)
            .count();
        assert_eq!(fetched, 2, "the two library basics entered tapped");
    }
}
