//! Flusterstorm — `{U}` Instant (first printed CMD; reprinted on the SOS Mystical Archive `soa`).
//!
//! Oracle: "Counter target instant or sorcery spell unless its controller pays {1}.
//! Storm (When you cast this spell, copy it for each spell cast before it this turn. You may choose
//! new targets for the copies.)"
//!
//! **Fully implemented** — a soft counter (`CounterUnlessPay`, cost `{1}`) targeting an instant/sorcery
//! spell, plus **storm** (`Triggered { SelfCast → CopySpellOnStack{ SpellsCastThisTurn − 1, new targets } }`).

use crate::basics::{CardType, Color};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::ability::{Ability, Cost, EventPattern};
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};

/// grp id (bonus-sheet `soa` cards use the 600+ block).
pub const FLUSTERSTORM: u32 = 610;

pub fn register(db: &mut CardDb) {
    let effect = Effect::CounterUnlessPay {
        what: EffectTarget::Target(TargetSpec {
            kind: TargetKind::StackObject(CardFilter::AnyOf(vec![
                CardFilter::HasCardType(CardType::Instant),
                CardFilter::HasCardType(CardType::Sorcery),
            ])),
            min: 1,
            max: 1,
            distinct: true,
        }),
        cost: Cost { mana: Some(mana_cost(1, &[])), components: vec![] },
    };
    let mut def = spell(
        FLUSTERSTORM,
        "Flusterstorm",
        CardType::Instant,
        Color::Blue,
        mana_cost(0, &[(Color::Blue, 1)]),
        effect,
    )
    .with_text("Counter target instant or sorcery spell unless its controller pays {1}.\nStorm (When you cast this spell, copy it for each spell cast before it this turn. You may choose new targets for the copies.)");
    // Storm (CR 702.40): copy this spell once per spell cast before it this turn, new targets allowed.
    def.abilities.push(Ability::Triggered {
        event: EventPattern::SelfCast,
        condition: None,
        intervening_if: false,
        effect: Effect::CopySpellOnStack {
            what: EffectTarget::Triggering,
            count: ValueExpr::Sum(
                Box::new(ValueExpr::SpellsCastThisTurn { who: PlayerRef::Controller }),
                Box::new(ValueExpr::Fixed(-1)),
            ),
            choose_new_targets: true,
        },
    });
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flusterstorm_shape() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(FLUSTERSTORM).unwrap();
        assert!(def.fully_implemented);
        assert!(def.abilities.iter().any(|a| matches!(a, Ability::Triggered { event: EventPattern::SelfCast, .. })), "storm trigger present");
        match def.spell_effect().unwrap() {
            Effect::CounterUnlessPay { cost, .. } => assert_eq!(cost.mana.as_ref().unwrap().generic, 1),
            o => panic!("expected CounterUnlessPay, got {o:?}"),
        }
    }
}
