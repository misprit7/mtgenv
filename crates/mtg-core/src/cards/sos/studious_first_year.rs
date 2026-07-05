//! Studious First-Year // Rampant Growth — `{G}` Creature — Bear Wizard 1/1 // `{1}{G}` Sorcery
//! (first printed SOS). A **Prepare** DFC (enters prepared).
//!
//! Front: "This creature enters prepared."
//! Back (Rampant Growth): "Search your library for a basic land card, put that card onto the
//! battlefield tapped, then shuffle."
//!
//! **Fully implemented** — enters-prepared via [`helpers::enters_prepared`]; the back face is the
//! classic Rampant Growth search (basic land → battlefield tapped), reusing [`helpers::basic_land_filter`].

use crate::basics::{CardType, Color, Zone, ZoneDest, ZonePos};
use crate::cards::{creature, helpers, mana_cost, spell, CardDb};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;
use crate::subtypes::CreatureType;

pub const STUDIOUS_FIRST_YEAR: u32 = 377;
pub const RAMPANT_GROWTH: u32 = 9704;

pub fn register(db: &mut CardDb) {
    let rampant_growth = Effect::Search {
        who: PlayerRef::Controller,
        zone: Zone::Library,
        filter: helpers::basic_land_filter(),
        min: 0,
        max: 1,
        to: ZoneDest { zone: Zone::Battlefield, pos: ZonePos::Any },
        tapped: true,
    };
    db.insert(
        spell(
            RAMPANT_GROWTH,
            "Rampant Growth",
            CardType::Sorcery,
            Color::Green,
            mana_cost(1, &[(Color::Green, 1)]),
            rampant_growth,
        )
        .with_text("Search your library for a basic land card, put that card onto the battlefield tapped, then shuffle."),
    );

    let mut front = creature(
        STUDIOUS_FIRST_YEAR,
        "Studious First-Year",
        &[CreatureType::Bear, CreatureType::Wizard],
        Color::Green,
        mana_cost(0, &[(Color::Green, 1)]),
        1,
        1,
        helpers::enters_prepared(RAMPANT_GROWTH),
    );
    front.text = "This creature enters prepared. (While it's prepared, you may cast a copy of its spell. Doing so unprepares it.)\n// Rampant Growth {1}{G} Sorcery — Search your library for a basic land card, put that card onto the battlefield tapped, then shuffle.".to_string();
    db.insert(front);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effects::ability::Ability;
    use expect_test::expect;

    #[test]
    fn studious_first_year_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let front = db.get(STUDIOUS_FIRST_YEAR).unwrap();
        assert!(matches!(front.abilities[0], Ability::Prepare { spell: RAMPANT_GROWTH }));
        assert!(matches!(
            front.abilities[1],
            Ability::Triggered { event: crate::effects::ability::EventPattern::SelfEnters, .. }
        ));
        expect![[r#"
            Search {
                who: Controller,
                zone: Library,
                filter: All(
                    [
                        HasCardType(
                            Land,
                        ),
                        Supertype(
                            Basic,
                        ),
                    ],
                ),
                min: 0,
                max: 1,
                to: ZoneDest {
                    zone: Battlefield,
                    pos: Any,
                },
                tapped: true,
            }"#]]
        .assert_eq(&format!("{:#?}", db.get(RAMPANT_GROWTH).unwrap().spell_effect().unwrap()));
    }
}
