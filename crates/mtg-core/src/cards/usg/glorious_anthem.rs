//! Glorious Anthem — `{1}{W}{W}` Enchantment. "Creatures you control get +1/+1." (layer 7c
//! ModifyPT.) First printed USG (Urza's Saga).

use crate::basics::Color;
use crate::cards::helpers::creatures_you_control;
use crate::cards::{grp, enchantment, mana_cost, CardDb};
use crate::effects::ability::{Ability, StaticContribution};
use crate::effects::condition::Duration;


pub fn register(db: &mut CardDb) {
    db.insert(enchantment(
        grp::GLORIOUS_ANTHEM,
        "Glorious Anthem",
        Color::White,
        mana_cost(1, &[(Color::White, 2)]),
        vec![Ability::Static {
            contribution: StaticContribution::ModifyPT { power: 1, toughness: 1 },
            affects: creatures_you_control(),
            duration: Duration::WhileSourcePresent,
        }],
    ).with_text("Creatures you control get +1/+1."));
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn glorious_anthem_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(grp::GLORIOUS_ANTHEM).unwrap();
        expect![[r#"
            [
                Static {
                    contribution: ModifyPT {
                        power: 1,
                        toughness: 1,
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
                },
            ]"#]].assert_eq(&format!("{:#?}", def.abilities));
    }
}
