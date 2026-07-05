//! Conciliator's Duelist — `{W}{W}{B}{B}` Creature — Kor Warlock 4/3 (first printed SOS).
//!
//! Oracle: "When this creature enters, draw a card. Each player loses 1 life.
//! Repartee — Whenever you cast an instant or sorcery spell that targets a creature, exile up to one
//! target creature. Return that card to the battlefield under its owner's control at the beginning of
//! the next end step."
//!
//! **Fully implemented** — the lander for the **timed-blink** cap (`Effect::ExileReturnNextEndStep`,
//! CR 603.7 delayed return): the ETB draws + drains each player 1, and the Repartee trigger (the
//! `SpellCastTargetingCreature` event, shared by the cycle) exiles up to one target creature and arms
//! its return at the next end step. Multicolored (W/B).

use crate::basics::Color;
use crate::cards::helpers::instant_or_sorcery;
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern};
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const CONCILIATORS_DUELIST: u32 = 422;

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        CONCILIATORS_DUELIST,
        "Conciliator's Duelist",
        &[CreatureType::Kor, CreatureType::Warlock],
        Color::White,
        mana_cost(0, &[(Color::White, 2), (Color::Black, 2)]),
        4,
        3,
        vec![
            // ETB: draw a card; each player loses 1 life.
            Ability::Triggered {
                event: EventPattern::SelfEnters,
                condition: None,
                intervening_if: false,
                effect: Effect::Sequence(vec![
                    Effect::Draw { who: PlayerRef::Controller, count: ValueExpr::Fixed(1) },
                    Effect::LoseLife { who: PlayerRef::EachPlayer, amount: ValueExpr::Fixed(1) },
                ]),
            },
            // Repartee — timed blink of up to one target creature.
            Ability::Triggered {
                event: EventPattern::SpellCastTargetingCreature(instant_or_sorcery()),
                condition: None,
                intervening_if: false,
                effect: Effect::ExileReturnNextEndStep {
                    what: EffectTarget::Target(TargetSpec {
                        kind: TargetKind::Creature(CardFilter::Any),
                        min: 0,
                        max: 1,
                        distinct: true,
                    }),
                },
            },
        ],
    );
    def.chars.colors = vec![Color::White, Color::Black];
    def.text = "When this creature enters, draw a card. Each player loses 1 life.\nRepartee — Whenever you cast an instant or sorcery spell that targets a creature, exile up to one target creature. Return that card to the battlefield under its owner's control at the beginning of the next end step.".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView};
    use crate::basics::{Target, Zone};
    use crate::cards::{build_game, grp};
    use crate::effects::action::{ResolutionCtx, WbReason};
    use crate::ids::{PlayerId, StackId};
    use crate::priority::Engine;
    use expect_test::expect;

    #[test]
    fn conciliators_duelist_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(CONCILIATORS_DUELIST).unwrap();
        assert_eq!((def.chars.power, def.chars.toughness), (Some(4), Some(3)));
        assert_eq!(def.chars.colors, vec![Color::White, Color::Black]);
        assert!(def.fully_implemented);
        assert!(matches!(def.abilities[0], Ability::Triggered { event: EventPattern::SelfEnters, .. }));
        expect![[r#"
            ExileReturnNextEndStep {
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
            }"#]]
        .assert_eq(&format!("{:#?}", match &def.abilities[1] {
            Ability::Triggered { effect, .. } => effect,
            _ => panic!("repartee"),
        }));
    }

    /// The timed blink: `ExileReturnNextEndStep` exiles the creature now, and firing the next end
    /// step's delayed triggers returns it to its owner's battlefield.
    #[test]
    fn timed_blink_exiles_then_returns_at_next_end_step() {
        struct Noop;
        impl Agent for Noop {
            fn decide(&mut self, _v: &PlayerView, _r: &DecisionRequest) -> DecisionResponse {
                DecisionResponse::Pass
            }
        }
        let mut state = build_game(1, &[&[], &[]]);
        let bears = {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        let mut e = Engine::new(state, vec![Box::new(Noop), Box::new(Noop)]);

        let effect = Effect::ExileReturnNextEndStep {
            what: EffectTarget::Target(TargetSpec {
                kind: TargetKind::Creature(CardFilter::Any),
                min: 0,
                max: 1,
                distinct: true,
            }),
        };
        e.resolve_effect(
            &effect,
            &ResolutionCtx {
                controller: Some(PlayerId(0)),
                chosen_targets: vec![Target::Object(bears)],
                ..Default::default()
            },
            WbReason::Resolve(StackId(0)),
        );
        // Exiled now.
        assert!(e.state.player(PlayerId(0)).exile.contains(&bears), "exiled by the timed blink");
        assert!(!e.state.player(PlayerId(0)).battlefield.contains(&bears), "…gone from the battlefield");

        // At the next end step, the armed delayed trigger returns it.
        e.fire_end_step_delayed_triggers();
        e.run_agenda(); // move the queued delayed trigger onto the stack
        while !e.state.stack.is_empty() {
            e.resolve_top();
            e.run_agenda();
        }
        assert!(e.state.player(PlayerId(0)).battlefield.contains(&bears), "returned at the next end step");
        assert!(!e.state.player(PlayerId(0)).exile.contains(&bears), "…no longer exiled");
    }
}
