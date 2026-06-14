//! Levitation — `{2}{U}{U}` Enchantment. "Creatures you control have flying." (layer 6
//! GrantKeyword.) First printed ULG (Urza's Legacy).

use crate::basics::Color;
use crate::cards::helpers::creatures_you_control;
use crate::cards::{grp, enchantment, mana_cost, CardDb};
use crate::effects::ability::{Ability, Keyword, StaticContribution};
use crate::effects::condition::Duration;

pub fn register(db: &mut CardDb) {
    db.insert(enchantment(
        grp::LEVITATION,
        "Levitation",
        Color::Blue,
        mana_cost(2, &[(Color::Blue, 2)]),
        vec![Ability::Static {
            contribution: StaticContribution::GrantKeyword(Keyword::Flying),
            affects: creatures_you_control(),
            duration: Duration::WhileSourcePresent,
        }],
    ).with_text("Creatures you control have flying."));
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn levitation_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(grp::LEVITATION).unwrap();
        expect![[r#"
            [
                Static {
                    contribution: GrantKeyword(
                        Flying,
                    ),
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
