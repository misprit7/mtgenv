//! Comforting Counsel — `{1}{G}` Enchantment (first printed SOS).
//!
//! Oracle: "Whenever you gain life, put a growth counter on this enchantment. / As long as there are
//! five or more growth counters on this enchantment, creatures you control get +3/+3."
//!
//! **Fully implemented** — a `GainLife` trigger that adds a `growth` counter (`CounterKind::Named`),
//! plus a `ConditionalStatic` anthem (+3/+3 to creatures you control) gated on
//! `CountersOnSelf(growth) ≥ 5`. No new engine support needed.

use crate::basics::{Color, CounterKind};
use crate::cards::helpers::creatures_you_control;
use crate::cards::{enchantment, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern, StaticContribution};
use crate::effects::condition::{Condition, Duration};
use crate::effects::value::ValueExpr;
use crate::effects::{Effect, EffectTarget};

/// grp id (per-set ids live near their cards).
pub const COMFORTING_COUNSEL: u32 = 281;

fn growth() -> CounterKind {
    CounterKind::Named("growth".to_string())
}

pub fn register(db: &mut CardDb) {
    db.insert(
        enchantment(
            COMFORTING_COUNSEL,
            "Comforting Counsel",
            Color::Green,
            mana_cost(1, &[(Color::Green, 1)]),
            vec![
                Ability::Triggered {
                    event: EventPattern::GainLife,
                    condition: None,
                    intervening_if: false,
                    effect: Effect::PutCounters {
                        what: EffectTarget::SourceSelf,
                        kind: growth(),
                        n: ValueExpr::Fixed(1),
                    },
                },
                Ability::ConditionalStatic {
                    contribution: StaticContribution::ModifyPT { power: 3, toughness: 3 },
                    affects: creatures_you_control(),
                    duration: Duration::WhileSourcePresent,
                    condition: Condition::ValueAtLeast(
                        ValueExpr::CountersOnSelf(growth()),
                        ValueExpr::Fixed(5),
                    ),
                },
            ],
        )
        .with_text("Whenever you gain life, put a growth counter on this enchantment.\nAs long as there are five or more growth counters on this enchantment, creatures you control get +3/+3."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn comforting_counsel_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        assert!(db.get(COMFORTING_COUNSEL).unwrap().fully_implemented);
        expect![[r#"
            [
                Triggered {
                    event: GainLife,
                    condition: None,
                    intervening_if: false,
                    effect: PutCounters {
                        what: SourceSelf,
                        kind: Named(
                            "growth",
                        ),
                        n: Fixed(
                            1,
                        ),
                    },
                },
                ConditionalStatic {
                    contribution: ModifyPT {
                        power: 3,
                        toughness: 3,
                    },
                    affects: SelectSpec {
                        zone: Battlefield,
                        filter: All(
                            [
                                HasCardType(
                                    Creature,
                                ),
                                ControlledBy(
                                    Controller,
                                ),
                            ],
                        ),
                        chooser: Controller,
                        min: Fixed(
                            0,
                        ),
                        max: Fixed(
                            0,
                        ),
                    },
                    duration: WhileSourcePresent,
                    condition: ValueAtLeast(
                        CountersOnSelf(
                            Named(
                                "growth",
                            ),
                        ),
                        Fixed(
                            5,
                        ),
                    ),
                },
            ]"#]].assert_eq(&format!("{:#?}", db.get(COMFORTING_COUNSEL).unwrap().abilities));
    }

    /// Behaviour: the anthem is off until the enchantment has 5+ growth counters, then a controlled
    /// creature reads +3/+3. Gaining life adds counters that flip it on.
    #[test]
    fn comforting_counsel_anthem_gated_on_growth_counters() {
        use crate::agent::RandomAgent;
        use crate::basics::Zone;
        use crate::cards::{build_game, grp};
        use crate::ids::PlayerId;
        use crate::priority::Engine;

        let mut state = build_game(1, &[&[], &[]]);
        let counsel = state.add_card(PlayerId(0), state.card_db().get(COMFORTING_COUNSEL).unwrap().chars.clone(), Zone::Battlefield);
        let bear = state.add_card(PlayerId(0), state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Battlefield);
        let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
        // 0 counters → anthem off → bear is a plain 2/2.
        assert_eq!(e.state.computed(bear).power, Some(2), "anthem off below 5 counters");
        // Put 5 growth counters directly on the enchantment.
        if let Some(o) = e.state.objects.get_mut(&counsel) {
            *o.counters.counts.entry(growth()).or_insert(0) += 5;
        }
        e.state.mark_chars_dirty();
        assert_eq!(e.state.computed(bear).power, Some(5), "5 growth counters → +3/+3 anthem → 5/5");
    }
}
