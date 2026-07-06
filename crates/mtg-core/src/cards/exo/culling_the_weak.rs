//! Culling the Weak — `{B}` Instant (first printed EXO; reprinted on the SOS Mystical Archive `soa`).
//!
//! Oracle: "As an additional cost to cast this spell, sacrifice a creature. Add {B}{B}{B}{B}."
//!
//! **Fully implemented** — a spell-level additional "sacrifice a creature" cost (paid at cast) + a
//! ritual effect adding four black mana.

use crate::basics::{CardType, Color, Zone};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::ability::{AdditionalCost, Ability, Cost, CostComponent};
use crate::effects::target::{CardFilter, ManaSpec, SelectSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;

/// grp id (bonus-sheet `soa` cards use the 600+ block).
pub const CULLING_THE_WEAK: u32 = 615;

pub fn register(db: &mut CardDb) {
    let effect = Effect::AddMana {
        who: PlayerRef::Controller,
        mana: ManaSpec {
            produces: vec![(Color::Black, ValueExpr::Fixed(4))],
            any_color: None,
            restriction: None,
        },
    };
    let mut def = spell(
        CULLING_THE_WEAK,
        "Culling the Weak",
        CardType::Instant,
        Color::Black,
        mana_cost(0, &[(Color::Black, 1)]),
        effect,
    )
    .with_text("As an additional cost to cast this spell, sacrifice a creature.\nAdd {B}{B}{B}{B}.");
    def.abilities.push(Ability::AdditionalCost(AdditionalCost {
        options: vec![Cost {
            mana: None,
            components: vec![CostComponent::Sacrifice(SelectSpec {
                zone: Zone::Battlefield,
                filter: CardFilter::All(vec![
                    CardFilter::HasCardType(CardType::Creature),
                    CardFilter::ControlledBy(PlayerRef::Controller),
                ]),
                chooser: PlayerRef::Controller,
                min: ValueExpr::Fixed(1),
                max: ValueExpr::Fixed(1),
            })],
        }],
    }));
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn culling_the_weak_shape() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(CULLING_THE_WEAK).unwrap();
        assert!(def.fully_implemented);
        assert_eq!(def.additional_costs().len(), 1, "sacrifice-a-creature clause");
        match def.spell_effect().unwrap() {
            Effect::AddMana { mana, .. } => {
                assert_eq!(mana.produces, vec![(Color::Black, ValueExpr::Fixed(4))]);
            }
            o => panic!("expected AddMana, got {o:?}"),
        }
    }
}
