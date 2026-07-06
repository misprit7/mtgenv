//! Disdainful Stroke — `{1}{U}` Instant (first printed KTK; reprinted on the SOS Mystical Archive `soa`).
//!
//! Oracle: "Counter target spell with mana value 4 or greater."
//!
//! **Fully implemented** — a hard `Counter` restricted to a stack target of mana value 4+
//! (`TargetKind::StackObject` filtered by `ManaValue { min: 4 }`).

use crate::basics::{CardType, Color};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::{Effect, EffectTarget};

/// grp id (bonus-sheet `soa` cards use the 600+ block).
pub const DISDAINFUL_STROKE: u32 = 607;

pub fn register(db: &mut CardDb) {
    let effect = Effect::Counter {
        what: EffectTarget::Target(TargetSpec {
            kind: TargetKind::StackObject(CardFilter::ManaValue { min: Some(4), max: None }),
            min: 1,
            max: 1,
            distinct: true,
        }),
    };
    db.insert(
        spell(
            DISDAINFUL_STROKE,
            "Disdainful Stroke",
            CardType::Instant,
            Color::Blue,
            mana_cost(1, &[(Color::Blue, 1)]),
            effect,
        )
        .with_text("Counter target spell with mana value 4 or greater."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn disdainful_stroke_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(DISDAINFUL_STROKE).unwrap();
        assert!(def.fully_implemented);
        assert_eq!(def.chars.card_types, vec![CardType::Instant]);
        expect![[r#"
            Counter {
                what: Target(
                    TargetSpec {
                        kind: StackObject(
                            ManaValue {
                                min: Some(
                                    4,
                                ),
                                max: None,
                            },
                        ),
                        min: 1,
                        max: 1,
                        distinct: true,
                    },
                ),
            }"#]]
        .assert_eq(&format!("{:#?}", def.spell_effect().unwrap()));
    }
}
