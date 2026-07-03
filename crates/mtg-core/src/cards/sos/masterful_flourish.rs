//! Masterful Flourish — `{B}` Instant (first printed SOS).
//!
//! Oracle: "Target creature you control gets +1/+0 and gains indestructible until end of turn."
//!
//! **Fully implemented** — `PumpPT` (+1/+0 until end of turn) on a creature you control, plus
//! `GrantKeyword(Indestructible)` for the same duration (the keyword grant references the same
//! chosen target via `ChosenIndex(0)`).

use crate::basics::{CardType, Color};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::ability::Keyword;
use crate::effects::condition::Duration;
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};

/// grp id (per-set ids live near their cards).
pub const MASTERFUL_FLOURISH: u32 = 224;

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
            toughness: ValueExpr::Fixed(0),
            duration: Duration::UntilEndOfTurn,
        },
        Effect::GrantKeyword {
            what: EffectTarget::ChosenIndex(0),
            keyword: Keyword::Indestructible,
            duration: Duration::UntilEndOfTurn,
        },
    ]);
    db.insert(
        spell(
            MASTERFUL_FLOURISH,
            "Masterful Flourish",
            CardType::Instant,
            Color::Black,
            mana_cost(0, &[(Color::Black, 1)]),
            effect,
        )
        .with_text("Target creature you control gets +1/+0 and gains indestructible until end of turn."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn masterful_flourish_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(MASTERFUL_FLOURISH).unwrap();
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
                            0,
                        ),
                        duration: UntilEndOfTurn,
                    },
                    GrantKeyword {
                        what: ChosenIndex(
                            0,
                        ),
                        keyword: Indestructible,
                        duration: UntilEndOfTurn,
                    },
                ],
            )"#]].assert_eq(&format!("{:#?}", def.spell_effect().unwrap()));
    }

    /// Behaviour: a 2/2 becomes a 3/2 with indestructible (until EOT).
    #[test]
    fn masterful_flourish_pumps_and_grants_indestructible() {
        use crate::agent::RandomAgent;
        use crate::basics::{Target, Zone};
        use crate::cards::{build_game, grp};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        let mut state = build_game(1, &[&[], &[]]);
        let bears = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
        let target = state.add_card(PlayerId(0), bears, Zone::Battlefield);
        let effect = state.card_db().get(MASTERFUL_FLOURISH).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(
            state,
            vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))],
        );
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
        assert!(cc.has_keyword(Keyword::Indestructible), "gains indestructible");
    }
}
