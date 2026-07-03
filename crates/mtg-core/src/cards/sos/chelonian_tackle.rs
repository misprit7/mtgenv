//! Chelonian Tackle — `{2}{G}` Sorcery (first printed SOS).
//!
//! Oracle: "Target creature you control gets +0/+10 until end of turn. Then it fights up to one
//! target creature an opponent controls."
//!
//! **Fully implemented** — `PumpPT` (+0/+10 until end of turn) on a creature you control (slot 0),
//! then `Fight` between that same creature (`ChosenIndex(0)`) and "up to one" (`min: 0`) target
//! creature an opponent controls (slot 1). If no second target is chosen, the fight simply doesn't
//! happen (the pump still applied).

use crate::basics::{CardType, Color};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::condition::Duration;
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};

/// grp id (per-set ids live near their cards).
pub const CHELONIAN_TACKLE: u32 = 222;

pub fn register(db: &mut CardDb) {
    let effect = Effect::Sequence(vec![
        Effect::PumpPT {
            what: EffectTarget::Target(TargetSpec {
                kind: TargetKind::Creature(CardFilter::ControlledBy(PlayerRef::Controller)),
                min: 1,
                max: 1,
                distinct: true,
            }),
            power: ValueExpr::Fixed(0),
            toughness: ValueExpr::Fixed(10),
            duration: Duration::UntilEndOfTurn,
        },
        Effect::Fight {
            a: EffectTarget::ChosenIndex(0),
            b: EffectTarget::Target(TargetSpec {
                kind: TargetKind::Creature(CardFilter::ControlledBy(PlayerRef::Opponent)),
                min: 0,
                max: 1,
                distinct: true,
            }),
        },
    ]);
    db.insert(
        spell(
            CHELONIAN_TACKLE,
            "Chelonian Tackle",
            CardType::Sorcery,
            Color::Green,
            mana_cost(2, &[(Color::Green, 1)]),
            effect,
        )
        .with_text("Target creature you control gets +0/+10 until end of turn. Then it fights up to one target creature an opponent controls."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn chelonian_tackle_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(CHELONIAN_TACKLE).unwrap();
        assert!(def.fully_implemented);
        expect![[r#"
            Sequence(
                [
                    PumpPT {
                        what: Target(
                            TargetSpec {
                                kind: Creature(
                                    ControlledBy(
                                        Controller,
                                    ),
                                ),
                                min: 1,
                                max: 1,
                                distinct: true,
                            },
                        ),
                        power: Fixed(
                            0,
                        ),
                        toughness: Fixed(
                            10,
                        ),
                        duration: UntilEndOfTurn,
                    },
                    Fight {
                        a: ChosenIndex(
                            0,
                        ),
                        b: Target(
                            TargetSpec {
                                kind: Creature(
                                    ControlledBy(
                                        Opponent,
                                    ),
                                ),
                                min: 0,
                                max: 1,
                                distinct: true,
                            },
                        ),
                    },
                ],
            )"#]].assert_eq(&format!("{:#?}", def.spell_effect().unwrap()));
    }

    /// Behaviour: the +0/+10 creature fights the opponent's — a 2/2 becomes 2/12 and deals 2 to the
    /// opponent's 2/2 (killing it via lethal SBA), taking 2 back (survives at 12 toughness).
    #[test]
    fn chelonian_tackle_pumps_then_fights() {
        use crate::agent::RandomAgent;
        use crate::basics::{Target, Zone};
        use crate::cards::{build_game, grp};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        let mut state = build_game(1, &[&[], &[]]);
        let mine = {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        let theirs = {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            state.add_card(PlayerId(1), c, Zone::Battlefield)
        };
        let effect = state.card_db().get(CHELONIAN_TACKLE).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(
            state,
            vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))],
        );
        e.resolve_effect(
            &effect,
            &ResolutionCtx {
                controller: Some(PlayerId(0)),
                chosen_targets: vec![Target::Object(mine), Target::Object(theirs)],
                ..Default::default()
            },
            WbReason::Resolve(StackId(0)),
        );
        assert_eq!(e.state.computed(mine).toughness, Some(12), "+0/+10 → 2/12");
        // The fight dealt 2 to each: the opponent's 2/2 took 2 (lethal), mine took 2 of 12 (survives).
        e.run_agenda(); // process the lethal-damage SBA
        assert!(e.state.players[1].graveyard.contains(&theirs), "opponent's creature died to the fight");
        assert!(e.state.players[0].battlefield.contains(&mine), "our 2/12 survived");
    }
}
