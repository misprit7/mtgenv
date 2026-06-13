//! Sazh's Chocobo — `{G}` Creature — Bird 0/1 (first printed FIN, Final Fantasy).
//!
//! "Landfall — Whenever a land you control enters, put a +1/+1 counter on this creature."
//! Fully implemented: a triggered ability on the landfall event (a land you control entering,
//! C4) putting a fixed +1/+1 counter on itself (C2). No deferred clauses.

use crate::basics::{Color, CounterKind};
use crate::cards::helpers::land_you_control;
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern};
use crate::effects::value::ValueExpr;
use crate::effects::{Effect, EffectTarget};

/// grp id (per-set ids live near their cards).
pub const SAZHS_CHOCOBO: u32 = 102;

pub fn register(db: &mut CardDb) {
    db.insert(
        creature(
            SAZHS_CHOCOBO,
            "Sazh's Chocobo",
            "Bird",
            Color::Green,
            mana_cost(0, &[(Color::Green, 1)]),
            0,
            1,
            vec![Ability::Triggered {
                event: EventPattern::PermanentEnters(land_you_control()),
                condition: None,
                intervening_if: false,
                effect: Effect::PutCounters {
                    what: EffectTarget::SourceSelf,
                    kind: CounterKind::PlusOnePlusOne,
                    n: ValueExpr::Fixed(1),
                },
            }],
        )
        .with_text("Landfall — Whenever a land you control enters, put a +1/+1 counter on this creature."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn sazhs_chocobo_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(SAZHS_CHOCOBO).unwrap();
        assert_eq!(def.chars.power, Some(0));
        assert_eq!(def.chars.toughness, Some(1));
        assert_eq!(def.chars.subtypes, vec!["Bird".to_string()]);
        assert!(!def.is_mana_source());
        expect![[r#"
            [
                Triggered {
                    event: PermanentEnters(
                        All(
                            [
                                HasCardType(
                                    Land,
                                ),
                                ControlledBy(
                                    Controller,
                                ),
                            ],
                        ),
                    ),
                    condition: None,
                    intervening_if: false,
                    effect: PutCounters {
                        what: SourceSelf,
                        kind: PlusOnePlusOne,
                        n: Fixed(
                            1,
                        ),
                    },
                },
            ]"#]].assert_eq(&format!("{:#?}", def.abilities));
    }
}
