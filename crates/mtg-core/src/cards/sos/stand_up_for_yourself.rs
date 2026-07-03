//! Stand Up for Yourself — `{2}{W}` Instant (first printed SOS).
//!
//! Oracle: "Destroy target creature with power 3 or greater."
//!
//! **Fully implemented** — a single targeted `Destroy` whose target filter restricts to creatures
//! with power 3+ (`Not(PowerAtMost(2))`, evaluated against computed power at targeting time).

use crate::basics::{CardType, Color};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::{Effect, EffectTarget};

/// grp id (per-set ids live near their cards).
pub const STAND_UP_FOR_YOURSELF: u32 = 312;

pub fn register(db: &mut CardDb) {
    let effect = Effect::Destroy {
        what: EffectTarget::Target(TargetSpec {
            // "power 3 or greater" = not (power ≤ 2).
            kind: TargetKind::Creature(CardFilter::Not(Box::new(CardFilter::PowerAtMost(2)))),
            min: 1,
            max: 1,
            distinct: true,
        }),
    };
    db.insert(
        spell(
            STAND_UP_FOR_YOURSELF,
            "Stand Up for Yourself",
            CardType::Instant,
            Color::White,
            mana_cost(2, &[(Color::White, 1)]),
            effect,
        )
        .with_text("Destroy target creature with power 3 or greater."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn stand_up_for_yourself_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(STAND_UP_FOR_YOURSELF).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Instant]);
        assert!(def.fully_implemented);
        expect![[r#"
            Destroy {
                what: Target(
                    TargetSpec {
                        kind: Creature(
                            Not(
                                PowerAtMost(
                                    2,
                                ),
                            ),
                        ),
                        min: 1,
                        max: 1,
                        distinct: true,
                    },
                ),
            }"#]]
        .assert_eq(&format!("{:#?}", def.spell_effect().unwrap()));
    }

    /// Behaviour: resolving it against a 4/4 destroys that creature (the power-3+ filter is captured in
    /// the IR above; generic target-filter enforcement — `PowerAtMost`/`Not` — is covered engine-side).
    #[test]
    fn stand_up_destroys_the_target() {
        use crate::agent::RandomAgent;
        use crate::basics::{Target, Zone};
        use crate::cards::{build_game, grp};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        let mut state = build_game(1, &[&[], &[]]);
        let mut big_chars = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
        big_chars.power = Some(4);
        big_chars.toughness = Some(4);
        let big = state.add_card(PlayerId(1), big_chars, Zone::Battlefield);
        let effect = state.card_db().get(STAND_UP_FOR_YOURSELF).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
        e.resolve_effect(
            &effect,
            &ResolutionCtx { controller: Some(PlayerId(0)), chosen_targets: vec![Target::Object(big)], ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        assert!(!e.state.players[1].battlefield.contains(&big), "the 4/4 was destroyed");
    }
}
