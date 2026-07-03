//! Withering Curse — `{1}{B}{B}` Sorcery (first printed SOS).
//!
//! Oracle: "All creatures get -2/-2 until end of turn. Infusion — If you gained life this turn,
//! destroy all creatures instead."
//!
//! **Fully implemented** — an Infusion `Conditional` on `GainedLifeThisTurn`: if you gained life this
//! turn, `ForEach` (the `PlayerRef::EachPlayer` all-creatures area selector, cf. Splatter Technique)
//! `Destroy`s each creature; otherwise each creature gets `-2/-2` until end of turn (`PumpPT`). No
//! targets — an area effect on every creature both players control.

use crate::basics::{CardType, Color, Zone};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::condition::{Condition, Duration};
use crate::effects::target::{CardFilter, SelectSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};

/// grp id (per-set ids live near their cards).
pub const WITHERING_CURSE: u32 = 346;

/// Every creature in play, both players (CR area effect) — the shared Splatter-Technique selector.
fn all_creatures() -> SelectSpec {
    SelectSpec {
        zone: Zone::Battlefield,
        filter: CardFilter::HasCardType(CardType::Creature),
        chooser: PlayerRef::EachPlayer,
        min: ValueExpr::Fixed(0),
        max: ValueExpr::Fixed(999),
    }
}

pub fn register(db: &mut CardDb) {
    let effect = Effect::Conditional {
        cond: Condition::GainedLifeThisTurn { who: PlayerRef::Controller },
        // "destroy all creatures instead" — Infusion upside.
        then: Box::new(Effect::ForEach {
            selector: all_creatures(),
            body: Box::new(Effect::Destroy { what: EffectTarget::Each }),
        }),
        // Default: all creatures get -2/-2 until end of turn.
        otherwise: Some(Box::new(Effect::ForEach {
            selector: all_creatures(),
            body: Box::new(Effect::PumpPT {
                what: EffectTarget::Each,
                power: ValueExpr::Fixed(-2),
                toughness: ValueExpr::Fixed(-2),
                duration: Duration::UntilEndOfTurn,
            }),
        })),
    };
    let mut def = spell(
        WITHERING_CURSE,
        "Withering Curse",
        CardType::Sorcery,
        Color::Black,
        mana_cost(1, &[(Color::Black, 2)]),
        effect,
    )
    .with_text("All creatures get -2/-2 until end of turn.\nInfusion — If you gained life this turn, destroy all creatures instead.");
    def.chars.colors = vec![Color::Black];
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::RandomAgent;
    use crate::basics::Zone;
    use crate::cards::{build_game, grp};
    use crate::effects::action::{ResolutionCtx, WbReason};
    use crate::ids::{PlayerId, StackId};
    use crate::priority::Engine;
    use expect_test::expect;

    #[test]
    fn withering_curse_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(WITHERING_CURSE).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Sorcery]);
        assert!(def.fully_implemented);
        expect![[r#"
            Conditional {
                cond: GainedLifeThisTurn {
                    who: Controller,
                },
                then: ForEach {
                    selector: SelectSpec {
                        zone: Battlefield,
                        filter: HasCardType(
                            Creature,
                        ),
                        chooser: EachPlayer,
                        min: Fixed(
                            0,
                        ),
                        max: Fixed(
                            999,
                        ),
                    },
                    body: Destroy {
                        what: Each,
                    },
                },
                otherwise: Some(
                    ForEach {
                        selector: SelectSpec {
                            zone: Battlefield,
                            filter: HasCardType(
                                Creature,
                            ),
                            chooser: EachPlayer,
                            min: Fixed(
                                0,
                            ),
                            max: Fixed(
                                999,
                            ),
                        },
                        body: PumpPT {
                            what: Each,
                            power: Fixed(
                                -2,
                            ),
                            toughness: Fixed(
                                -2,
                            ),
                            duration: UntilEndOfTurn,
                        },
                    },
                ),
            }"#]]
        .assert_eq(&format!("{:#?}", def.spell_effect().unwrap()));
    }

    fn resolve(gained_life: bool) -> Engine {
        let mut state = build_game(1, &[&[], &[]]);
        // A 3/3 on each battlefield (Hill Giant) so -2/-2 leaves a live 1/1 (visible without SBAs).
        let g0 = state.card_db().get(grp::HILL_GIANT).unwrap().chars.clone();
        state.add_card(PlayerId(0), g0.clone(), Zone::Battlefield);
        state.add_card(PlayerId(1), g0, Zone::Battlefield);
        if gained_life {
            state.players[0].life_gained_this_turn = 1;
        }
        let effect = state.card_db().get(WITHERING_CURSE).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
        e.resolve_effect(
            &effect,
            &ResolutionCtx { controller: Some(PlayerId(0)), ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        e
    }

    /// No life gained → all creatures get -2/-2 UEOT (a 3/3 becomes a computed 1/1, both sides).
    #[test]
    fn minus_two_when_no_life_gained() {
        let e = resolve(false);
        for p in [PlayerId(0), PlayerId(1)] {
            let c = e.state.player(p).battlefield[0];
            let cc = e.state.computed(c);
            assert_eq!((cc.power, cc.toughness), (Some(1), Some(1)), "3/3 → 1/1 under -2/-2");
        }
    }

    /// Life gained → destroy all creatures instead (both sides move to the graveyard).
    #[test]
    fn destroy_all_when_life_gained() {
        let e = resolve(true);
        assert!(e.state.player(PlayerId(0)).battlefield.is_empty(), "P0's creature destroyed");
        assert!(e.state.player(PlayerId(1)).battlefield.is_empty(), "P1's creature destroyed");
        assert_eq!(e.state.player(PlayerId(0)).graveyard.len(), 1);
        assert_eq!(e.state.player(PlayerId(1)).graveyard.len(), 1);
    }
}
