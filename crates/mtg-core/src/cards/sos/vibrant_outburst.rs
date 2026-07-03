//! Vibrant Outburst — `{U}{R}` Instant (first printed SOS).
//!
//! Oracle: "Vibrant Outburst deals 3 damage to any target. Tap up to one target creature."
//!
//! **Fully implemented** — `DealDamage 3` to one "any target" (slot 0), then `Tap` an "up to one
//! target creature" (slot 1, `min: 0`). Multicolored (U/R).

use crate::basics::{CardType, Color, DamageKind};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::ValueExpr;
use crate::effects::{Effect, EffectTarget};

/// grp id (per-set ids live near their cards).
pub const VIBRANT_OUTBURST: u32 = 211;

pub fn register(db: &mut CardDb) {
    let effect = Effect::Sequence(vec![
        Effect::DealDamage {
            amount: ValueExpr::Fixed(3),
            to: EffectTarget::Target(TargetSpec {
                kind: TargetKind::Any,
                min: 1,
                max: 1,
                distinct: true,
            }),
            kind: DamageKind::Noncombat,
        },
        Effect::Tap {
            what: EffectTarget::Target(TargetSpec {
                kind: TargetKind::Creature(CardFilter::Any),
                min: 0,
                max: 1,
                distinct: true,
            }),
            tap: true,
        },
    ]);
    let mut def = spell(
        VIBRANT_OUTBURST,
        "Vibrant Outburst",
        CardType::Instant,
        Color::Blue,
        mana_cost(0, &[(Color::Blue, 1), (Color::Red, 1)]),
        effect,
    )
    .with_text("Vibrant Outburst deals 3 damage to any target. Tap up to one target creature.");
    def.chars.colors = vec![Color::Blue, Color::Red];
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn vibrant_outburst_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(VIBRANT_OUTBURST).unwrap();
        assert_eq!(def.chars.colors, vec![Color::Blue, Color::Red]);
        assert!(def.fully_implemented);
        expect![[r#"
            Sequence(
                [
                    DealDamage {
                        amount: Fixed(
                            3,
                        ),
                        to: Target(
                            TargetSpec {
                                kind: Any,
                                min: 1,
                                max: 1,
                                distinct: true,
                            },
                        ),
                        kind: Noncombat,
                    },
                    Tap {
                        what: Target(
                            TargetSpec {
                                kind: Creature(
                                    Any,
                                ),
                                min: 0,
                                max: 1,
                                distinct: true,
                            },
                        ),
                        tap: true,
                    },
                ],
            )"#]].assert_eq(&format!("{:#?}", def.spell_effect().unwrap()));
    }

    /// Behaviour: 3 damage to the opponent's face and the targeted creature is tapped.
    #[test]
    fn vibrant_outburst_burns_and_taps() {
        use crate::agent::RandomAgent;
        use crate::basics::{Target, Zone};
        use crate::cards::{build_game, grp};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        let mut state = build_game(1, &[&[], &[]]);
        let bears = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
        let creature = state.add_card(PlayerId(1), bears, Zone::Battlefield);
        let effect = state.card_db().get(VIBRANT_OUTBURST).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(
            state,
            vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))],
        );
        let p1 = e.state.player(PlayerId(1)).life;
        e.resolve_effect(
            &effect,
            &ResolutionCtx {
                controller: Some(PlayerId(0)),
                chosen_targets: vec![Target::Player(PlayerId(1)), Target::Object(creature)],
                ..Default::default()
            },
            WbReason::Resolve(StackId(0)),
        );
        assert_eq!(e.state.player(PlayerId(1)).life, p1 - 3, "3 damage to the opponent");
        assert!(e.state.object(creature).status.tapped, "the targeted creature is tapped");
    }
}
