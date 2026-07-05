//! Pull from the Grave — `{2}{B}` Sorcery (first printed SOS).
//!
//! Oracle: "Return up to two target creature cards from your graveyard to your hand. You gain 2
//! life."
//!
//! **Fully implemented** — the first **multi-target** `Effect::MoveZone`: a single "up to two
//! target" slot (`min: 0, max: 2`) flattens both picks into `chosen_targets`, and the `MoveZone`
//! materialize arm now emits one `Action::MoveZone { to: Hand, cause: Returned }` per chosen card
//! (up to `max`), followed by `GainLife 2` for the caster. Targets are scoped to the caster's
//! graveyard via `ControlledBy(Controller)` (graveyard cards read `o.controller == owner`).

use crate::basics::{CardType, Color, Zone, ZoneDest, ZonePos};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};

/// grp id (per-set ids live near their cards).
pub const PULL_FROM_THE_GRAVE: u32 = 327;

pub fn register(db: &mut CardDb) {
    let effect = Effect::Sequence(vec![
        Effect::MoveZone {
            what: EffectTarget::Target(TargetSpec {
                kind: TargetKind::CardInZone {
                    zone: Zone::Graveyard,
                    filter: CardFilter::All(vec![
                        CardFilter::HasCardType(CardType::Creature),
                        CardFilter::ControlledBy(PlayerRef::Controller),
                    ]),
                },
                min: 0,
                max: 2,
                distinct: true,
            }),
            to: ZoneDest { zone: Zone::Hand, pos: ZonePos::Any },
            tapped: false,
        },
        Effect::GainLife {
            who: PlayerRef::Controller,
            amount: ValueExpr::Fixed(2),
        },
    ]);
    db.insert(
        spell(
            PULL_FROM_THE_GRAVE,
            "Pull from the Grave",
            CardType::Sorcery,
            Color::Black,
            mana_cost(2, &[(Color::Black, 1)]),
            effect,
        )
        .with_text(
            "Return up to two target creature cards from your graveyard to your hand. You gain 2 life.",
        ),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn pull_from_the_grave_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(PULL_FROM_THE_GRAVE).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Sorcery]);
        assert_eq!(def.chars.colors, vec![Color::Black]);
        assert!(def.fully_implemented);
        expect![[r#"
            Sequence(
                [
                    MoveZone {
                        what: Target(
                            TargetSpec {
                                kind: CardInZone {
                                    zone: Graveyard,
                                    filter: All(
                                        [
                                            HasCardType(
                                                Creature,
                                            ),
                                            ControlledBy(
                                                Controller,
                                            ),
                                        ],
                                    ),
                                },
                                min: 0,
                                max: 2,
                                distinct: true,
                            },
                        ),
                        to: ZoneDest {
                            zone: Hand,
                            pos: Any,
                        },
                        tapped: false,
                    },
                    GainLife {
                        who: Controller,
                        amount: Fixed(
                            2,
                        ),
                    },
                ],
            )"#]]
        .assert_eq(&format!("{:#?}", def.spell_effect().unwrap()));
    }

    /// Behaviour: the multi-target `MoveZone` returns BOTH chosen creature cards from the caster's
    /// graveyard to hand (proving a `max > 1` slot's flattened picks are all consumed), and the
    /// caster gains 2 life.
    #[test]
    fn pull_returns_two_creatures_and_gains_life() {
        use crate::agent::RandomAgent;
        use crate::basics::{Target, Zone};
        use crate::cards::{build_game, grp};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        let mut state = build_game(1, &[&[], &[]]);
        // Two creature cards sitting in P0's graveyard.
        let bears = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
        let a = state.add_card(PlayerId(0), bears.clone(), Zone::Graveyard);
        let b = state.add_card(PlayerId(0), bears, Zone::Graveyard);
        let effect =
            state.card_db().get(PULL_FROM_THE_GRAVE).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(
            state,
            vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))],
        );
        let life_before = e.state.player(PlayerId(0)).life;
        e.resolve_effect(
            &effect,
            &ResolutionCtx {
                controller: Some(PlayerId(0)),
                chosen_targets: vec![Target::Object(a), Target::Object(b)],
                ..Default::default()
            },
            WbReason::Resolve(StackId(0)),
        );
        assert!(!e.state.players[0].graveyard.contains(&a), "first card left the graveyard");
        assert!(!e.state.players[0].graveyard.contains(&b), "second card left the graveyard");
        assert!(e.state.players[0].hand.contains(&a), "first card returned to hand");
        assert!(e.state.players[0].hand.contains(&b), "second card returned to hand");
        assert_eq!(e.state.player(PlayerId(0)).life, life_before + 2, "caster gains 2 life");
    }
}
