//! Exultant Cultist — `{2}{U}` Creature — Human Wizard 2/2. "When this creature dies, draw a
//! card." (dies/LTB trigger.) First printed EMN (Eldritch Moon).

use crate::basics::Color;
use crate::cards::{grp, creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;
use crate::subtypes::CreatureType;


pub fn register(db: &mut CardDb) {
    db.insert(creature(
        grp::EXULTANT_CULTIST,
        "Exultant Cultist",
        &[CreatureType::Human, CreatureType::Wizard],
        Color::Blue,
        mana_cost(2, &[(Color::Blue, 1)]),
        2,
        2,
        vec![Ability::Triggered {
            event: EventPattern::SelfDies,
            condition: None,
            intervening_if: false,
            effect: Effect::Draw {
                who: PlayerRef::Controller,
                count: ValueExpr::Fixed(1),
            },
        }],
    ).with_text("When this creature dies, draw a card."));
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn exultant_cultist_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(grp::EXULTANT_CULTIST).unwrap();
        assert_eq!(def.chars.power, Some(2));
        assert_eq!(def.chars.subtypes, vec![CreatureType::Human.into(), CreatureType::Wizard.into()]);
        expect![[r#"
            [
                Triggered {
                    event: SelfDies,
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
