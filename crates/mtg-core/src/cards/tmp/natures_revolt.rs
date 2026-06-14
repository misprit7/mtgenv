//! Nature's Revolt — `{3}{G}{G}` Enchantment. "All lands are 2/2 creatures that are still
//! lands." TWO statics: AddType(Creature) (layer 4) + SetBasePT{2,2} (7b), both over all lands.
//! The layer-4 type change is what makes an anthem ("creatures you control") see a land as a
//! creature — the affects-reads-computed (CR 613.8) case. First printed TMP (Tempest).

use crate::basics::{CardType, Color, Zone};
use crate::cards::{grp, enchantment, mana_cost, CardDb};
use crate::effects::ability::{Ability, StaticContribution};
use crate::effects::condition::Duration;
use crate::effects::target::{CardFilter, SelectSpec};
use crate::effects::value::{PlayerRef, ValueExpr};


pub fn register(db: &mut CardDb) {
    let all_lands = || SelectSpec {
        zone: Zone::Battlefield,
        filter: CardFilter::HasCardType(CardType::Land),
        chooser: PlayerRef::Controller,
        min: ValueExpr::Fixed(0),
        max: ValueExpr::Fixed(0),
    };
    db.insert(enchantment(
        grp::NATURES_REVOLT,
        "Nature's Revolt",
        Color::Green,
        mana_cost(3, &[(Color::Green, 2)]),
        vec![
            Ability::Static {
                contribution: StaticContribution::AddType(CardType::Creature),
                affects: all_lands(),
                duration: Duration::WhileSourcePresent,
            },
            Ability::Static {
                contribution: StaticContribution::SetBasePT { power: 2, toughness: 2 },
                affects: all_lands(),
                duration: Duration::WhileSourcePresent,
            },
        ],
    ).with_text("All lands are 2/2 creatures that are still lands."));
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn natures_revolt_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(grp::NATURES_REVOLT).unwrap();
        expect![[r#"
            [
                Static {
                    contribution: AddType(
                        Creature,
                    ),
                    affects: SelectSpec {
                        zone: Battlefield,
                        filter: HasCardType(
                            Land,
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
                Static {
                    contribution: SetBasePT {
                        power: 2,
                        toughness: 2,
                    },
                    affects: SelectSpec {
                        zone: Battlefield,
                        filter: HasCardType(
                            Land,
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
