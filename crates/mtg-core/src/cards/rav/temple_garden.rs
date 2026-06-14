//! Temple Garden — Land — Forest Plains (first printed RAV, Ravnica: City of Guilds). A Selesnya
//! shock land.
//!
//! Oracle: "({T}: Add {G} or {W}.) As this land enters, you may pay 2 life. If you don't, it
//! enters tapped."
//!
//! Fully implemented (no deferrals):
//! - {G}/{W} mana is **intrinsic** — the `Forest`+`Plains` land subtypes (CR 305.6), so there's no
//!   mana ability on the card at all; the engine grants `{T}: Add {G}`/`{T}: Add {W}` from the types.
//! - The shock clause is a `WouldEnterBattlefield(ItSelf)` replacement → `EntersTappedUnlessPay{2}`
//!   (C11): the engine asks the controller to pay 2 life as it enters — pay → untapped, decline → tapped.

use crate::basics::CardType;
use crate::cards::{CardDb, CardDef};
use crate::effects::ability::{Ability, ActionPattern, Rewrite};
use crate::effects::target::CardFilter;
use crate::state::Characteristics;
use crate::subtypes::LandType;

/// grp id (per-set ids live near their cards).
pub const TEMPLE_GARDEN: u32 = 109;

pub fn register(db: &mut CardDb) {
    let chars = Characteristics {
        name: "Temple Garden".to_string(),
        card_types: vec![CardType::Land],
        subtypes: vec![LandType::Forest.into(), LandType::Plains.into()],
        grp_id: TEMPLE_GARDEN,
        ..Default::default()
    };
    db.insert(CardDef {
        chars,
        abilities: vec![Ability::Replacement {
            pattern: ActionPattern::WouldEnterBattlefield(CardFilter::ItSelf),
            rewrite: Rewrite::EntersTappedUnlessPay { life: 2 },
        }],
        text: "({T}: Add {G} or {W}.)\nAs this land enters, you may pay 2 life. If you don't, it enters tapped.".to_string(),
        fully_implemented: true,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::subtypes::Subtype;
    use expect_test::expect;

    #[test]
    fn temple_garden_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(TEMPLE_GARDEN).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Land]);
        // Forest + Plains subtypes → intrinsic G/W mana (no mana ability).
        assert_eq!(
            def.chars.subtypes,
            vec![Subtype::from(LandType::Forest), Subtype::from(LandType::Plains)]
        );
        assert!(!def.is_mana_source()); // mana is intrinsic from the types, not an IR ability
        assert!(def.fully_implemented);
        expect![[r#"
            [
                Replacement {
                    pattern: WouldEnterBattlefield(
                        ItSelf,
                    ),
                    rewrite: EntersTappedUnlessPay {
                        life: 2,
                    },
                },
            ]"#]].assert_eq(&format!("{:#?}", def.abilities));
    }
}
