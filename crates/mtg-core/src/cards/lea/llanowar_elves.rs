//! Llanowar Elves — `{G}` Creature — Elf Druid 1/1 (first printed LEA). "{T}: Add {G}."
//!
//! A green mana dork. Its mana ability is first-class Effect IR (`{T}: Add {G}` via
//! `mana_ability`, an `Ability::Activated{is_mana:true}` + `Effect::AddMana`) — not the legacy
//! `mana_colors` shortcut. The engine gates activation by summoning sickness (C1, CR 302.6) so a
//! freshly-cast Llanowar can't tap the turn it enters.

use crate::basics::Color;
use crate::cards::{creature, mana_ability, mana_cost, CardDb};
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const LLANOWAR_ELVES: u32 = 100;

pub fn register(db: &mut CardDb) {
    let mut elf = creature(
        LLANOWAR_ELVES,
        "Llanowar Elves",
        CreatureType::Elf,
        Color::Green,
        mana_cost(0, &[(Color::Green, 1)]),
        1,
        1,
        vec![mana_ability(Color::Green)],
    );
    elf.chars.subtypes = vec![CreatureType::Elf.into(), CreatureType::Druid.into()];
    db.insert(elf.with_text("{T}: Add {G}."));
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn llanowar_elves_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(LLANOWAR_ELVES).unwrap();
        // A 1/1 Elf Druid whose `{T}: Add {G}` is a real mana ability (no `mana_colors` shortcut).
        assert_eq!(def.chars.power, Some(1));
        assert_eq!(def.chars.toughness, Some(1));
        assert_eq!(def.chars.subtypes, vec![CreatureType::Elf.into(), CreatureType::Druid.into()]);
        assert!(def.is_mana_source());
        expect![[r#"
            [
                Activated {
                    cost: Cost {
                        mana: None,
                        components: [
                            TapSelf,
                        ],
                    },
                    effect: AddMana {
                        who: Controller,
                        mana: ManaSpec {
                            produces: [
                                (
                                    Green,
                                    Fixed(
                                        1,
                                    ),
                                ),
                            ],
                            any_color: None,
                        },
                    },
                    timing: Instant,
                    restriction: None,
                    is_mana: true,
                },
            ]"#]].assert_eq(&format!("{:#?}", def.abilities));
    }
}
