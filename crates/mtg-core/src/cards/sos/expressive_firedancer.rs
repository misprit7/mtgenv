//! Expressive Firedancer — `{1}{R}` Creature — Human Sorcerer 2/2 (first printed SOS).
//!
//! Oracle: "Opus — Whenever you cast an instant or sorcery spell, this creature gets +1/+1 until
//! end of turn. If five or more mana was spent to cast that spell, this creature also gains double
//! strike until end of turn."
//!
//! **Fully implemented** — an `Opus` cast-trigger (`SpellCast(instant|sorcery)`, fires only for the
//! caster's own spells): base `PumpPT +1/+1` until end of turn, plus a `Conditional` on
//! `ManaSpentOnTrigger ≥ 5` that also grants double strike. The mana-spent value reads the
//! triggering spell recorded by the SpellCast trigger machinery.

use crate::basics::Color;
use crate::cards::helpers::instant_or_sorcery;
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern, Keyword};
use crate::effects::condition::{Condition, Duration};
use crate::effects::value::ValueExpr;
use crate::effects::{Effect, EffectTarget};
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const EXPRESSIVE_FIREDANCER: u32 = 254;

pub fn register(db: &mut CardDb) {
    db.insert(
        creature(
            EXPRESSIVE_FIREDANCER,
            "Expressive Firedancer",
            &[CreatureType::Human, CreatureType::Sorcerer],
            Color::Red,
            mana_cost(1, &[(Color::Red, 1)]),
            2,
            2,
            vec![Ability::Triggered {
                event: EventPattern::SpellCast(instant_or_sorcery()),
                condition: None,
                intervening_if: false,
                effect: Effect::Sequence(vec![
                    Effect::PumpPT {
                        what: EffectTarget::SourceSelf,
                        power: ValueExpr::Fixed(1),
                        toughness: ValueExpr::Fixed(1),
                        duration: Duration::UntilEndOfTurn,
                    },
                    Effect::Conditional {
                        cond: Condition::ValueAtLeast(ValueExpr::ManaSpentOnTrigger, ValueExpr::Fixed(5)),
                        then: Box::new(Effect::GrantKeyword {
                            what: EffectTarget::SourceSelf,
                            keyword: Keyword::DoubleStrike,
                            duration: Duration::UntilEndOfTurn,
                        }),
                        otherwise: None,
                    },
                ]),
            }],
        )
        .with_text("Opus — Whenever you cast an instant or sorcery spell, this creature gets +1/+1 until end of turn. If five or more mana was spent to cast that spell, this creature also gains double strike until end of turn."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn expressive_firedancer_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(EXPRESSIVE_FIREDANCER).unwrap();
        assert!(def.fully_implemented);
        expect![[r#"
            [
                Triggered {
                    event: SpellCast(
                        AnyOf(
                            [
                                HasCardType(
                                    Instant,
                                ),
                                HasCardType(
                                    Sorcery,
                                ),
                            ],
                        ),
                    ),
                    condition: None,
                    intervening_if: false,
                    effect: Sequence(
                        [
                            PumpPT {
                                what: SourceSelf,
                                power: Fixed(
                                    1,
                                ),
                                toughness: Fixed(
                                    1,
                                ),
                                duration: UntilEndOfTurn,
                            },
                            Conditional {
                                cond: ValueAtLeast(
                                    ManaSpentOnTrigger,
                                    Fixed(
                                        5,
                                    ),
                                ),
                                then: GrantKeyword {
                                    what: SourceSelf,
                                    keyword: DoubleStrike,
                                    duration: UntilEndOfTurn,
                                },
                                otherwise: None,
                            },
                        ],
                    ),
                },
            ]"#]].assert_eq(&format!("{:#?}", def.abilities));
    }

    /// End-to-end: casting an instant fires the Opus trigger through the real agenda. A cheap spell
    /// (<5 mana) gives +1/+1 only; a 5-mana spell also grants double strike.
    #[test]
    fn expressive_firedancer_opus_scales_with_mana() {
        use crate::agent::{Agent, DecisionRequest, DecisionResponse, GameEvent, PlayerView};
        use crate::basics::Zone;
        use crate::cards::{build_game, grp};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        use crate::stack::{StackObject, StackObjectKind};

        #[derive(Clone)]
        struct PassiveAgent;
        impl Agent for PassiveAgent {
            fn decide(&mut self, _v: &PlayerView, _req: &DecisionRequest) -> DecisionResponse {
                DecisionResponse::Pass
            }
        }

        // Cast an instant "on the stack" with `mana_spent`, fire the SpellCast event, and settle.
        let run = |mana_spent: u32| {
            let mut state = build_game(1, &[&[], &[]]);
            let dancer = {
                let c = state.card_db().get(EXPRESSIVE_FIREDANCER).unwrap().chars.clone();
                state.add_card(PlayerId(0), c, Zone::Battlefield)
            };
            // A Lightning Bolt (an instant) as the triggering spell, with its mana-spent recorded.
            let bolt = {
                let c = state.card_db().get(grp::LIGHTNING_BOLT).unwrap().chars.clone();
                state.add_card(PlayerId(0), c, Zone::Stack)
            };
            state.objects.get_mut(&bolt).unwrap().mana_spent = mana_spent;
            let sid = StackId(1);
            state.stack.push(StackObject {
                id: sid,
                controller: PlayerId(0),
                source: None,
                kind: StackObjectKind::Spell(bolt),
                targets: vec![],
                x: None,
                modes: Vec::new(),
            });
            let mut e = Engine::new(state, vec![Box::new(PassiveAgent), Box::new(PassiveAgent)]);
            // Fire "you cast an instant/sorcery spell" → queues the Opus trigger.
            e.broadcast(GameEvent::SpellCast { spell: sid, controller: PlayerId(0) });
            // Move the Opus trigger onto the stack (above the Bolt), then resolve just the trigger.
            e.run_agenda();
            e.resolve_top();
            let cc = e.state.computed(dancer);
            (cc.power, cc.has_keyword(Keyword::DoubleStrike))
        };
        assert_eq!(run(3), (Some(3), false), "cheap spell → +1/+1, no double strike");
        assert_eq!(run(5), (Some(3), true), "5+ mana → +1/+1 and double strike");
    }
}
