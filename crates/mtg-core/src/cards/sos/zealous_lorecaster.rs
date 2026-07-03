//! Zealous Lorecaster — `{5}{R}` Creature — Giant Sorcerer 4/4 (first printed SOS).
//!
//! Oracle: "When this creature enters, return target instant or sorcery card from your graveyard
//! to your hand."
//!
//! **Fully implemented** — an ETB triggered ability (CR 603.6a) whose effect is a single-target
//! `Effect::MoveZone` from the graveyard to hand. Exercises the `MoveZone` effect leaf (a
//! card-agnostic engine cap wired alongside this card): the chosen graveyard card is moved to its
//! owner's hand via `Action::MoveZone { to: Hand, cause: Returned }`.

use crate::basics::{CardType, Color, Zone, ZoneDest, ZonePos};
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern};
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::{Effect, EffectTarget};
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const ZEALOUS_LORECASTER: u32 = 209;

/// "target instant or sorcery card in your graveyard".
fn instant_or_sorcery_in_graveyard() -> TargetSpec {
    TargetSpec {
        kind: TargetKind::CardInZone {
            zone: Zone::Graveyard,
            filter: CardFilter::AnyOf(vec![
                CardFilter::HasCardType(CardType::Instant),
                CardFilter::HasCardType(CardType::Sorcery),
            ]),
        },
        min: 1,
        max: 1,
        distinct: true,
    }
}

pub fn register(db: &mut CardDb) {
    db.insert(
        creature(
            ZEALOUS_LORECASTER,
            "Zealous Lorecaster",
            &[CreatureType::Giant, CreatureType::Sorcerer],
            Color::Red,
            mana_cost(5, &[(Color::Red, 1)]),
            4,
            4,
            vec![Ability::Triggered {
                event: EventPattern::SelfEnters,
                condition: None,
                intervening_if: false,
                effect: Effect::MoveZone {
                    what: EffectTarget::Target(instant_or_sorcery_in_graveyard()),
                    to: ZoneDest { zone: Zone::Hand, pos: ZonePos::Any },
                },
            }],
        )
        .with_text("When this creature enters, return target instant or sorcery card from your graveyard to your hand."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn zealous_lorecaster_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(ZEALOUS_LORECASTER).unwrap();
        assert_eq!(def.chars.power, Some(4));
        assert!(def.fully_implemented);
        expect![[r#"
            [
                Triggered {
                    event: SelfEnters,
                    condition: None,
                    intervening_if: false,
                    effect: MoveZone {
                        what: Target(
                            TargetSpec {
                                kind: CardInZone {
                                    zone: Graveyard,
                                    filter: AnyOf(
                                        [
                                            HasCardType(
                                                Instant,
                                            ),
                                            HasCardType(
                                                Sorcery,
                                            ),
                                        ],
                                    ),
                                },
                                min: 1,
                                max: 1,
                                distinct: true,
                            },
                        ),
                        to: ZoneDest {
                            zone: Hand,
                            pos: Any,
                        },
                    },
                },
            ]"#]].assert_eq(&format!("{:#?}", def.abilities));
    }

    /// Behaviour: resolving the ETB returns the targeted instant from the graveyard to its owner's
    /// hand (exercises the `MoveZone` effect leaf).
    #[test]
    fn zealous_lorecaster_returns_instant_to_hand() {
        use crate::agent::RandomAgent;
        use crate::basics::{Target, Zone};
        use crate::cards::{build_game, grp};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        let mut state = build_game(1, &[&[], &[]]);
        // A Lightning Bolt sitting in P0's graveyard.
        let bolt = state.card_db().get(grp::LIGHTNING_BOLT).unwrap().chars.clone();
        let card = state.add_card(PlayerId(0), bolt, Zone::Graveyard);
        let chars = state.card_db().get(ZEALOUS_LORECASTER).unwrap().chars.clone();
        let src = state.add_card(PlayerId(0), chars, Zone::Battlefield);
        let etb = match &state.card_db().get(ZEALOUS_LORECASTER).unwrap().abilities[0] {
            Ability::Triggered { effect, .. } => effect.clone(),
            o => panic!("expected ETB Triggered, got {o:?}"),
        };
        let mut e = Engine::new(
            state,
            vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))],
        );
        assert!(e.state.players[0].graveyard.contains(&card));
        e.resolve_effect(
            &etb,
            &ResolutionCtx {
                controller: Some(PlayerId(0)),
                source: Some(src),
                chosen_targets: vec![Target::Object(card)],
                ..Default::default()
            },
            WbReason::Resolve(StackId(0)),
        );
        assert!(!e.state.players[0].graveyard.contains(&card), "left the graveyard");
        assert!(e.state.players[0].hand.contains(&card), "returned to owner's hand");
    }
}
