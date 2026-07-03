//! Oracle's Restoration — `{G}` Sorcery (first printed SOS).
//!
//! Oracle: "Target creature you control gets +1/+1 until end of turn. You draw a card and gain 1
//! life."
//!
//! **Fully implemented** — `PumpPT` (+1/+1 until end of turn) on one declared "creature you
//! control", then `Draw 1` + `GainLife 1` for the caster.

use crate::basics::{CardType, Color};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::condition::Duration;
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};

/// grp id (per-set ids live near their cards).
pub const ORACLES_RESTORATION: u32 = 205;

pub fn register(db: &mut CardDb) {
    let effect = Effect::Sequence(vec![
        Effect::PumpPT {
            what: EffectTarget::Target(TargetSpec {
                kind: TargetKind::Creature(CardFilter::ControlledBy(PlayerRef::Controller)),
                min: 1,
                max: 1,
                distinct: true,
            }),
            power: ValueExpr::Fixed(1),
            toughness: ValueExpr::Fixed(1),
            duration: Duration::UntilEndOfTurn,
        },
        Effect::Draw {
            who: PlayerRef::Controller,
            count: ValueExpr::Fixed(1),
        },
        Effect::GainLife {
            who: PlayerRef::Controller,
            amount: ValueExpr::Fixed(1),
        },
    ]);
    db.insert(
        spell(
            ORACLES_RESTORATION,
            "Oracle's Restoration",
            CardType::Sorcery,
            Color::Green,
            mana_cost(0, &[(Color::Green, 1)]),
            effect,
        )
        .with_text("Target creature you control gets +1/+1 until end of turn. You draw a card and gain 1 life."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn oracles_restoration_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(ORACLES_RESTORATION).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Sorcery]);
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
                            1,
                        ),
                        toughness: Fixed(
                            1,
                        ),
                        duration: UntilEndOfTurn,
                    },
                    Draw {
                        who: Controller,
                        count: Fixed(
                            1,
                        ),
                    },
                    GainLife {
                        who: Controller,
                        amount: Fixed(
                            1,
                        ),
                    },
                ],
            )"#]].assert_eq(&format!("{:#?}", def.spell_effect().unwrap()));
    }

    /// Behaviour: on a 2/2, resolving pumps it to 3/3 (until EOT) and the caster draws 1 and gains 1.
    #[test]
    fn oracles_restoration_pumps_draws_gains() {
        use crate::agent::RandomAgent;
        use crate::basics::{Target, Zone};
        use crate::cards::{build_game, grp};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        // Give the caster a one-card library so the Draw has something to take.
        let mut state = build_game(1, &[&[grp::FOREST], &[]]);
        let bears = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
        let target = state.add_card(PlayerId(0), bears, Zone::Battlefield);
        let effect = state.card_db().get(ORACLES_RESTORATION).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(
            state,
            vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))],
        );
        let life0 = e.state.player(PlayerId(0)).life;
        let hand0 = e.state.players[0].hand.len();
        e.resolve_effect(
            &effect,
            &ResolutionCtx {
                controller: Some(PlayerId(0)),
                chosen_targets: vec![Target::Object(target)],
                ..Default::default()
            },
            WbReason::Resolve(StackId(0)),
        );
        let cc = e.state.computed(target);
        assert_eq!(cc.power, Some(3), "+1 power");
        assert_eq!(cc.toughness, Some(3), "+1 toughness");
        assert_eq!(e.state.players[0].hand.len(), hand0 + 1, "drew a card");
        assert_eq!(e.state.player(PlayerId(0)).life, life0 + 1, "gained 1 life");
    }
}
