//! Elvish Visionary — `{1}{G}` Creature — Elf Shaman 1/1. "When this creature enters, draw a
//! card." (ETB trigger.) First printed ALA (Shards of Alara).

use crate::basics::Color;
use crate::cards::{grp, creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;
use crate::subtypes::CreatureType;


pub fn register(db: &mut CardDb) {
    db.insert(creature(
        grp::ELVISH_VISIONARY,
        "Elvish Visionary",
        &[CreatureType::Elf, CreatureType::Shaman],
        Color::Green,
        mana_cost(1, &[(Color::Green, 1)]),
        1,
        1,
        vec![Ability::Triggered {
            event: EventPattern::SelfEnters,
            condition: None,
            intervening_if: false,
            effect: Effect::Draw {
                who: PlayerRef::Controller,
                count: ValueExpr::Fixed(1),
            },
        }],
    ).with_text("When this creature enters, draw a card."));
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn elvish_visionary_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(grp::ELVISH_VISIONARY).unwrap();
        assert_eq!(def.chars.power, Some(1));
        assert_eq!(def.chars.subtypes, vec![CreatureType::Elf.into(), CreatureType::Shaman.into()]);
        expect![[r#"
            [
                Triggered {
                    event: SelfEnters,
                    condition: None,
                    intervening_if: false,
                    effect: Draw {
                        who: Controller,
                        count: Fixed(
                            1,
                        ),
                    },
                },
            ]"#]].assert_eq(&format!("{:#?}", def.abilities));
    }
}
