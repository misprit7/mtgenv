//! Interjection — `{W}` Instant (first printed SOS).
//!
//! Oracle: "Target creature gets +2/+2 and gains first strike until end of turn."
//!
//! **Fully implemented** — one declared target (601.2c) receives both a `PumpPT` (+2/+2 until end
//! of turn, CR 611) and a `GrantKeyword(FirstStrike)` for the same duration. The pump declares the
//! target (slot 0); the keyword grant references that same chosen target via `ChosenIndex(0)`, so
//! it's a single "target creature", not two.

use crate::basics::{CardType, Color};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::ability::Keyword;
use crate::effects::condition::Duration;
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::ValueExpr;
use crate::effects::{Effect, EffectTarget};

/// grp id (per-set ids live near their cards).
pub const INTERJECTION: u32 = 203;

pub fn register(db: &mut CardDb) {
    let effect = Effect::Sequence(vec![
        Effect::PumpPT {
            what: EffectTarget::Target(TargetSpec {
                kind: TargetKind::Creature(CardFilter::Any),
                min: 1,
                max: 1,
                distinct: true,
            }),
            power: ValueExpr::Fixed(2),
            toughness: ValueExpr::Fixed(2),
            duration: Duration::UntilEndOfTurn,
        },
        Effect::GrantKeyword {
            what: EffectTarget::ChosenIndex(0),
            keyword: Keyword::FirstStrike,
            duration: Duration::UntilEndOfTurn,
        },
    ]);
    db.insert(
        spell(
            INTERJECTION,
            "Interjection",
            CardType::Instant,
            Color::White,
            mana_cost(0, &[(Color::White, 1)]),
            effect,
        )
        .with_text("Target creature gets +2/+2 and gains first strike until end of turn."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn interjection_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(INTERJECTION).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Instant]);
        assert!(def.fully_implemented);
        expect![[r#"
            Sequence(
                [
                    PumpPT {
                        what: Target(
                            TargetSpec {
                                kind: Creature(
                                    Any,
                                ),
                                min: 1,
                                max: 1,
                                distinct: true,
                            },
                        ),
                        power: Fixed(
                            2,
                        ),
                        toughness: Fixed(
                            2,
                        ),
                        duration: UntilEndOfTurn,
                    },
                    GrantKeyword {
                        what: ChosenIndex(
                            0,
                        ),
                        keyword: FirstStrike,
                        duration: UntilEndOfTurn,
                    },
                ],
            )"#]]
        .assert_eq(&format!("{:#?}", def.spell_effect().unwrap()));
    }

    /// Behaviour: resolving Interjection on a 2/2 makes it a 4/4 with first strike (until EOT).
    #[test]
    fn interjection_pumps_and_grants_first_strike() {
        use crate::agent::RandomAgent;
        use crate::basics::{Target, Zone};
        use crate::cards::{build_game, grp};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        let mut state = build_game(1, &[&[], &[]]);
        let bears = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
        let target = state.add_card(PlayerId(0), bears, Zone::Battlefield);
        let effect = state.card_db().get(INTERJECTION).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(
            state,
            vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))],
        );
        assert_eq!(e.state.computed(target).power, Some(2));
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
        assert_eq!(cc.power, Some(4), "+2 power");
        assert_eq!(cc.toughness, Some(4), "+2 toughness");
        assert!(cc.has_keyword(Keyword::FirstStrike), "gains first strike");
    }
}
