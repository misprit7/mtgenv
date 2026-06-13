//! Mossborn Hydra — `{2}{G}` Creature — Elemental Hydra 0/0 (first printed FDN, Foundations).
//!
//! "Trample. This creature enters with a +1/+1 counter on it. Landfall — Whenever a land you
//! control enters, double the number of +1/+1 counters on this creature."
//!
//! Fully implemented:
//! - Trample (keyword).
//! - Enters with a +1/+1 counter: a self-replacement (CR 614.12 via `ItSelf`) — without it the
//!   0/0 would die to the toughness-0 SBA. Its P/T comes from counters via the normal layer 7c
//!   (no CDA needed).
//! - Landfall double: a triggered ability (C4) that puts `CountersOnSelf(+1/+1)` *more* counters
//!   on itself (C9b) — adding the current count doubles it.

use crate::basics::{Color, CounterKind};
use crate::cards::fin::sazhs_chocobo::land_you_control;
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, ActionPattern, EventPattern, Keyword, Rewrite};
use crate::effects::target::CardFilter;
use crate::effects::value::ValueExpr;
use crate::effects::{Effect, EffectTarget};

/// grp id (per-set ids live near their cards).
pub const MOSSBORN_HYDRA: u32 = 103;

pub fn register(db: &mut CardDb) {
    let mut hydra = creature(
        MOSSBORN_HYDRA,
        "Mossborn Hydra",
        "Elemental",
        Color::Green,
        mana_cost(2, &[(Color::Green, 1)]),
        0,
        0,
        vec![
            // Enters with a +1/+1 counter (self-replacement, CR 614.12 scoped to `ItSelf`).
            Ability::Replacement {
                pattern: ActionPattern::WouldEnterBattlefield(CardFilter::ItSelf),
                rewrite: Rewrite::EntersWithCounters {
                    kind: CounterKind::PlusOnePlusOne,
                    n: 1,
                },
            },
            // Landfall — "double the +1/+1 counters" = add (current count) more of them.
            Ability::Triggered {
                event: EventPattern::PermanentEnters(land_you_control()),
                condition: None,
                intervening_if: false,
                effect: Effect::PutCounters {
                    what: EffectTarget::SourceSelf,
                    kind: CounterKind::PlusOnePlusOne,
                    n: ValueExpr::CountersOnSelf(CounterKind::PlusOnePlusOne),
                },
            },
        ],
    );
    hydra.chars.subtypes = vec!["Elemental".to_string(), "Hydra".to_string()];
    hydra.chars.keywords = vec![Keyword::Trample];
    db.insert(hydra.with_text(
        "Trample\nThis creature enters with a +1/+1 counter on it.\nLandfall — Whenever a land you control enters, double the number of +1/+1 counters on this creature.",
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn mossborn_hydra_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(MOSSBORN_HYDRA).unwrap();
        assert_eq!(def.chars.power, Some(0));
        assert_eq!(def.chars.toughness, Some(0));
        assert_eq!(def.chars.subtypes, vec!["Elemental".to_string(), "Hydra".to_string()]);
        assert_eq!(def.chars.keywords, vec![Keyword::Trample]);
        assert!(!def.is_mana_source());
        expect![[r#"
            [
                Replacement {
                    pattern: WouldEnterBattlefield(
                        ItSelf,
                    ),
                    rewrite: EntersWithCounters {
                        kind: PlusOnePlusOne,
                        n: 1,
                    },
                },
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
                        n: CountersOnSelf(
                            PlusOnePlusOne,
                        ),
                    },
                },
            ]"#]].assert_eq(&format!("{:#?}", def.abilities));
    }
}
