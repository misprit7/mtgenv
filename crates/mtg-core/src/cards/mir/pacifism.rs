//! Pacifism — `{1}{W}` Aura. "Enchant creature. Enchanted creature can't attack or block." Two
//! AttachedHost statics painting the CantAttack/CantBlock qualifications (CR §2.4), read by
//! combat. First printed MIR (Mirage).

use crate::basics::Color;
use crate::cards::helpers::attached_host;
use crate::cards::{grp, aura, mana_cost, CardDb};
use crate::effects::ability::{Ability, Qualification, StaticContribution};
use crate::effects::condition::Duration;

pub fn register(db: &mut CardDb) {
    db.insert(aura(
        grp::PACIFISM,
        "Pacifism",
        Color::White,
        mana_cost(1, &[(Color::White, 1)]),
        vec![
            Ability::Static {
                contribution: StaticContribution::Qualification(Qualification::CantAttack),
                affects: attached_host(),
                duration: Duration::WhileSourcePresent,
            },
            Ability::Static {
                contribution: StaticContribution::Qualification(Qualification::CantBlock),
                affects: attached_host(),
                duration: Duration::WhileSourcePresent,
            },
        ],
    ).with_text("Enchant creature. Enchanted creature can't attack or block."));
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn pacifism_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(grp::PACIFISM).unwrap();
        expect![[r#"
            [
                Static {
                    contribution: Qualification(
                        CantAttack,
                    ),
                    affects: SelectSpec {
                        zone: Battlefield,
                        filter: AttachedHost,
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
                    contribution: Qualification(
                        CantBlock,
                    ),
                    affects: SelectSpec {
                        zone: Battlefield,
                        filter: AttachedHost,
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
