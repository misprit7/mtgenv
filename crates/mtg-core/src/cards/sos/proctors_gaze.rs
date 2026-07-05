//! Proctor's Gaze — `{2}{G}{U}` Instant (first printed SOS).
//!
//! Oracle: "Return up to one target nonland permanent to its owner's hand. Search your library for
//! a basic land card, put it onto the battlefield tapped, then shuffle."
//!
//! **Fully implemented** — a single-target `MoveZone` bounce of "up to one" (`min: 0`) nonland
//! permanent to its owner's hand, then a `Search` fetching a basic land onto the battlefield tapped.
//! Multicolored (G/U).

use crate::basics::{CardType, Color, Zone, ZoneDest, ZonePos};
use crate::cards::helpers::basic_land_filter;
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::PlayerRef;
use crate::effects::{Effect, EffectTarget};

/// grp id (per-set ids live near their cards).
pub const PROCTORS_GAZE: u32 = 227;

pub fn register(db: &mut CardDb) {
    let effect = Effect::Sequence(vec![
        Effect::MoveZone {
            what: EffectTarget::Target(TargetSpec {
                kind: TargetKind::Permanent(CardFilter::Not(Box::new(CardFilter::HasCardType(
                    CardType::Land,
                )))),
                min: 0,
                max: 1,
                distinct: true,
            }),
            to: ZoneDest { zone: Zone::Hand, pos: ZonePos::Any },
            tapped: false,
        },
        Effect::Search {
            who: PlayerRef::Controller,
            zone: Zone::Library,
            filter: basic_land_filter(),
            min: 0,
            max: 1,
            to: ZoneDest { zone: Zone::Battlefield, pos: ZonePos::Any },
            tapped: true,
        },
    ]);
    let mut def = spell(
        PROCTORS_GAZE,
        "Proctor's Gaze",
        CardType::Instant,
        Color::Green,
        mana_cost(2, &[(Color::Green, 1), (Color::Blue, 1)]),
        effect,
    )
    .with_text("Return up to one target nonland permanent to its owner's hand. Search your library for a basic land card, put it onto the battlefield tapped, then shuffle.");
    def.chars.colors = vec![Color::Green, Color::Blue];
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn proctors_gaze_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(PROCTORS_GAZE).unwrap();
        assert_eq!(def.chars.colors, vec![Color::Green, Color::Blue]);
        assert!(def.fully_implemented);
        expect![[r#"
            Sequence(
                [
                    MoveZone {
                        what: Target(
                            TargetSpec {
                                kind: Permanent(
                                    Not(
                                        HasCardType(
                                            Land,
                                        ),
                                    ),
                                ),
                                min: 0,
                                max: 1,
                                distinct: true,
                            },
                        ),
                        to: ZoneDest {
                            zone: Hand,
                            pos: Any,
                        },
                        tapped: false,
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

    /// Behaviour: bounces the opponent's creature to hand and fetches a basic onto our battlefield.
    #[test]
    fn proctors_gaze_bounces_and_fetches() {
        use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView};
        use crate::basics::{Target, Zone};
        use crate::cards::{build_game, grp};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;

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

        let mut state = build_game(1, &[&[grp::FOREST], &[]]); // our library: one Forest to fetch
        let bears = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
        let victim = state.add_card(PlayerId(1), bears, Zone::Battlefield);
        let effect = state.card_db().get(PROCTORS_GAZE).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(TakeItAgent), Box::new(TakeItAgent)]);
        e.resolve_effect(
            &effect,
            &ResolutionCtx {
                controller: Some(PlayerId(0)),
                chosen_targets: vec![Target::Object(victim)],
                ..Default::default()
            },
            WbReason::Resolve(StackId(0)),
        );
        assert!(e.state.players[1].hand.contains(&victim), "the creature was bounced to its owner's hand");
        let fetched = e.state.players[0]
            .battlefield
            .iter()
            .any(|&o| e.state.object(o).chars.grp_id == grp::FOREST && e.state.object(o).status.tapped);
        assert!(fetched, "a basic land entered our battlefield tapped");
    }
}
