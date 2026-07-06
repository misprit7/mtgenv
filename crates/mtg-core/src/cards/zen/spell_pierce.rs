//! Spell Pierce — `{U}` Instant (first printed ZEN; reprinted on the SOS Mystical Archive `soa`).
//!
//! Oracle: "Counter target noncreature spell unless its controller pays {2}."
//!
//! **Fully implemented** — a soft counter (`CounterUnlessPay`) restricted to a stack target that is
//! not a creature spell (`StackObject(Not(Creature))`); the spell's controller may pay `{2}` to save it.

use crate::basics::{CardType, Color};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::ability::Cost;
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::{Effect, EffectTarget};

/// grp id (bonus-sheet `soa` cards use the 600+ block).
pub const SPELL_PIERCE: u32 = 608;

pub fn register(db: &mut CardDb) {
    let effect = Effect::CounterUnlessPay {
        what: EffectTarget::Target(TargetSpec {
            kind: TargetKind::StackObject(CardFilter::Not(Box::new(CardFilter::HasCardType(CardType::Creature)))),
            min: 1,
            max: 1,
            distinct: true,
        }),
        cost: Cost { mana: Some(mana_cost(2, &[])), components: vec![] },
    };
    db.insert(
        spell(
            SPELL_PIERCE,
            "Spell Pierce",
            CardType::Instant,
            Color::Blue,
            mana_cost(0, &[(Color::Blue, 1)]),
            effect,
        )
        .with_text("Counter target noncreature spell unless its controller pays {2}."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spell_pierce_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(SPELL_PIERCE).unwrap();
        assert!(def.fully_implemented);
        assert_eq!(def.chars.card_types, vec![CardType::Instant]);
        match def.spell_effect().unwrap() {
            Effect::CounterUnlessPay { cost, .. } => {
                assert_eq!(cost.mana.as_ref().unwrap().generic, 2);
            }
            o => panic!("expected CounterUnlessPay, got {o:?}"),
        }
    }
}
