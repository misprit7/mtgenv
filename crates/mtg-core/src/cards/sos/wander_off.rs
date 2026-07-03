//! Wander Off — `{3}{B}` Instant (first printed SOS).
//!
//! Oracle: "Exile target creature."
//!
//! **Fully implemented** — a single targeted `Effect::Exile` over "target creature". The engine
//! prompts for the one declared target at cast (601.2c) and `resolve_top` moves it to exile.

use crate::basics::{CardType, Color};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::{Effect, EffectTarget};

/// grp id (per-set ids live near their cards).
pub const WANDER_OFF: u32 = 201;

pub fn register(db: &mut CardDb) {
    let effect = Effect::Exile {
        what: EffectTarget::Target(TargetSpec {
            kind: TargetKind::Creature(CardFilter::Any),
            min: 1,
            max: 1,
            distinct: true,
        }),
    };
    db.insert(
        spell(
            WANDER_OFF,
            "Wander Off",
            CardType::Instant,
            Color::Black,
            mana_cost(3, &[(Color::Black, 1)]),
            effect,
        )
        .with_text("Exile target creature."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn wander_off_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(WANDER_OFF).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Instant]);
        assert!(def.fully_implemented);
        expect![[r#"
            Exile {
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
            }"#]]
        .assert_eq(&format!("{:#?}", def.spell_effect().unwrap()));
    }

    /// Behaviour: resolving Wander Off exiles the targeted creature (off the battlefield, into its
    /// owner's exile zone).
    #[test]
    fn wander_off_exiles_the_target() {
        use crate::agent::RandomAgent;
        use crate::basics::{Target, Zone};
        use crate::cards::{build_game, grp};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        let mut state = build_game(1, &[&[], &[]]);
        let bears = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
        let victim = state.add_card(PlayerId(1), bears, Zone::Battlefield);
        let effect = state.card_db().get(WANDER_OFF).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(
            state,
            vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))],
        );
        e.resolve_effect(
            &effect,
            &ResolutionCtx {
                controller: Some(PlayerId(0)),
                chosen_targets: vec![Target::Object(victim)],
                ..Default::default()
            },
            WbReason::Resolve(StackId(0)),
        );
        assert!(!e.state.players[1].battlefield.contains(&victim), "off the battlefield");
        assert!(e.state.players[1].exile.contains(&victim), "in its owner's exile zone");
    }
}
