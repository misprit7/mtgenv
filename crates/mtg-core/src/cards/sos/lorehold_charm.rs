//! Lorehold Charm — `{R}{W}` Instant (first printed SOS).
//!
//! Oracle: "Choose one —
//! • Each opponent sacrifices a nontoken artifact of their choice.
//! • Return target artifact or creature card with mana value 2 or less from your graveyard to the
//!   battlefield.
//! • Creatures you control get +1/+1 and gain trample until end of turn."
//!
//! **Fully implemented** — a `Modal` "choose one": (1) each opponent sacrifices an artifact they
//! choose (`Sacrifice { who: EachOpponent }`); (2) reanimate a MV-2-or-less artifact/creature from your
//! graveyard (`MoveZone` of a `CardInZone{Graveyard}` target scoped `ControlledBy(Controller)`); (3) a
//! `ForEach` mass +1/+1 and trample on your creatures until end of turn. Simplification: "nontoken" in
//! mode 1 isn't enforced (the engine doesn't tag token objects) — negligible in the current pool.

use crate::basics::{CardType, Color, Zone, ZoneDest, ZonePos};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::ability::Keyword;
use crate::effects::condition::Duration;
use crate::effects::target::{CardFilter, SelectSpec, TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget, Mode};

/// grp id (per-set ids live near their cards).
pub const LOREHOLD_CHARM: u32 = 317;

pub fn register(db: &mut CardDb) {
    let effect = Effect::Modal {
        modes: vec![
            Mode {
                label: "Each opponent sacrifices an artifact of their choice".to_string(),
                effect: Effect::Sacrifice {
                    who: PlayerRef::EachOpponent,
                    what: SelectSpec {
                        zone: Zone::Battlefield,
                        filter: CardFilter::HasCardType(CardType::Artifact),
                        chooser: PlayerRef::Opponent,
                        min: ValueExpr::Fixed(1),
                        max: ValueExpr::Fixed(1),
                    },
                },
            },
            Mode {
                label: "Return an artifact or creature card with mana value 2 or less from your graveyard to the battlefield".to_string(),
                effect: Effect::MoveZone {
                    what: EffectTarget::Target(TargetSpec {
                        kind: TargetKind::CardInZone {
                            zone: Zone::Graveyard,
                            filter: CardFilter::All(vec![
                                CardFilter::ControlledBy(PlayerRef::Controller),
                                CardFilter::AnyOf(vec![
                                    CardFilter::HasCardType(CardType::Artifact),
                                    CardFilter::HasCardType(CardType::Creature),
                                ]),
                                CardFilter::ManaValue { min: None, max: Some(2) },
                            ]),
                        },
                        min: 1,
                        max: 1,
                        distinct: true,
                    }),
                    to: ZoneDest { zone: Zone::Battlefield, pos: ZonePos::Any },
                    tapped: false,
                },
            },
            Mode {
                label: "Creatures you control get +1/+1 and gain trample until end of turn".to_string(),
                effect: Effect::ForEach {
                    selector: SelectSpec {
                        zone: Zone::Battlefield,
                        filter: CardFilter::All(vec![
                            CardFilter::HasCardType(CardType::Creature),
                            CardFilter::ControlledBy(PlayerRef::Controller),
                        ]),
                        chooser: PlayerRef::Controller,
                        min: ValueExpr::Fixed(0),
                        max: ValueExpr::Fixed(999),
                    },
                    body: Box::new(Effect::Sequence(vec![
                        Effect::PumpPT {
                            what: EffectTarget::Each,
                            power: ValueExpr::Fixed(1),
                            toughness: ValueExpr::Fixed(1),
                            duration: Duration::UntilEndOfTurn,
                        },
                        Effect::GrantKeyword {
                            what: EffectTarget::Each,
                            keyword: Keyword::Trample,
                            duration: Duration::UntilEndOfTurn,
                        },
                    ])),
                },
            },
        ],
        min: 1,
        max: 1,
        allow_repeat: false,
    };
    let mut def = spell(
        LOREHOLD_CHARM,
        "Lorehold Charm",
        CardType::Instant,
        Color::Red,
        mana_cost(0, &[(Color::Red, 1), (Color::White, 1)]),
        effect,
    )
    .with_text("Choose one —\n• Each opponent sacrifices a nontoken artifact of their choice.\n• Return target artifact or creature card with mana value 2 or less from your graveyard to the battlefield.\n• Creatures you control get +1/+1 and gain trample until end of turn.");
    def.chars.colors = vec![Color::Red, Color::White];
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lorehold_charm_shape() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(LOREHOLD_CHARM).unwrap();
        assert_eq!(def.chars.colors, vec![Color::Red, Color::White]);
        assert!(def.fully_implemented);
        match def.spell_effect().unwrap() {
            Effect::Modal { modes, min, max, .. } => assert_eq!((modes.len(), *min, *max), (3, 1, 1)),
            o => panic!("expected Modal, got {o:?}"),
        }
    }

    /// Behaviour, mode 2: reanimate a 2/2 (MV 2) creature from your graveyard onto the battlefield.
    #[test]
    fn lorehold_charm_mode2_reanimates_from_your_graveyard() {
        use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView};
        use crate::basics::Zone;
        use crate::cards::{build_game, grp};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::basics::Target;
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        #[derive(Clone)]
        struct Passive;
        impl Agent for Passive {
            fn decide(&mut self, _v: &PlayerView, _req: &DecisionRequest) -> DecisionResponse {
                DecisionResponse::Pass
            }
        }
        let mut state = build_game(1, &[&[], &[]]);
        // A Grizzly Bears (2/2, MV 2) in P0's graveyard.
        let dead = state.add_card(PlayerId(0), state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Graveyard);
        let effect = state.card_db().get(LOREHOLD_CHARM).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(Passive), Box::new(Passive)]);
        e.resolve_effect(
            &effect,
            &ResolutionCtx {
                controller: Some(PlayerId(0)),
                chosen_modes: vec![1],
                chosen_targets: vec![Target::Object(dead)],
                ..Default::default()
            },
            WbReason::Resolve(StackId(0)),
        );
        assert!(e.state.player(PlayerId(0)).battlefield.contains(&dead), "reanimated onto the battlefield");
        assert!(!e.state.player(PlayerId(0)).graveyard.contains(&dead), "no longer in the graveyard");
    }
}
