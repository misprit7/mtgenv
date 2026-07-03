//! Hydro-Channeler — `{1}{U}` Creature — Merfolk Wizard 1/3 (first printed SOS).
//!
//! Oracle:
//!   "{T}: Add {U}. Spend this mana only to cast an instant or sorcery spell.
//!    {1}, {T}: Add one mana of any color. Spend this mana only to cast an instant or sorcery spell."
//!
//! **Tracked-partial** (`.incomplete()`): the first ability — `{T}: Add {U}`, spendable only on an
//! instant/sorcery spell — is fully implemented and is the first consumer of the **S13 restricted-mana
//! cap** (`ManaSpec.restriction = InstantSorceryOnly` → the pool's `restricted` bucket; the payment
//! path counts it toward an I/S cast only, never a creature spell or an ability cost).
//!
//! The **second** ability (`{1}, {T}: Add one mana of any color`, restricted) is **deferred**: it's a
//! mana ability with a *mana activation cost*, which the auto-pay source model doesn't yet handle
//! (`mana_sources` treats a source as free-to-tap). Authoring it naively would make the engine offer
//! its rainbow mana for free — so it's omitted here rather than shipped broken. Needs a
//! mana-ability-with-activation-cost cap (also blocks filter lands generally).

use crate::basics::Color;
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, Cost, CostComponent, Timing};
use crate::effects::target::{ManaSpec, SpendRestriction};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const HYDRO_CHANNELER: u32 = 321;

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        HYDRO_CHANNELER,
        "Hydro-Channeler",
        &[CreatureType::Merfolk, CreatureType::Wizard],
        Color::Blue,
        mana_cost(1, &[(Color::Blue, 1)]),
        1,
        3,
        vec![Ability::Activated {
            cost: Cost { mana: None, components: vec![CostComponent::TapSelf] },
            effect: Effect::AddMana {
                who: PlayerRef::Controller,
                mana: ManaSpec {
                    produces: vec![(Color::Blue, ValueExpr::Fixed(1))],
                    any_color: None,
                    // "Spend this mana only to cast an instant or sorcery spell." (CR 106.6)
                    restriction: Some(SpendRestriction::InstantSorceryOnly),
                },
            },
            timing: Timing::Instant,
            restriction: None,
            is_mana: true,
        }],
    );
    def.text = "{T}: Add {U}. Spend this mana only to cast an instant or sorcery spell.\n{1}, {T}: Add one mana of any color. Spend this mana only to cast an instant or sorcery spell.".to_string();
    db.insert(def.incomplete());
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn hydro_channeler_shape() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(HYDRO_CHANNELER).unwrap();
        assert_eq!(def.chars.colors, vec![Color::Blue]);
        assert_eq!(def.chars.mana_value(), 2);
        // Tracked-partial: the second (mana-cost) ability is deferred.
        assert!(!def.fully_implemented);
        // The one shipped ability is a restricted-mana `{T}: Add {U}`.
        let restricted = matches!(&def.abilities[0], Ability::Activated { effect: Effect::AddMana { mana, .. }, is_mana: true, .. }
            if mana.restriction == Some(SpendRestriction::InstantSorceryOnly));
        assert!(restricted, "ability 0 adds instant/sorcery-restricted mana");
    }

    #[test]
    fn hydro_channeler_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(HYDRO_CHANNELER).unwrap();
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
                                    Blue,
                                    Fixed(
                                        1,
                                    ),
                                ),
                            ],
                            any_color: None,
                            restriction: Some(
                                InstantSorceryOnly,
                            ),
                        },
                    },
                    timing: Instant,
                    restriction: None,
                    is_mana: true,
                },
            ]"#]].assert_eq(&format!("{:#?}", def.abilities));
    }

    /// The cap's core behaviour: Hydro-Channeler's restricted {U} can pay an instant/sorcery cost but
    /// NOT a creature spell or an ability cost — driven through the real `can_pay`/`auto_pay` seam.
    #[test]
    fn restricted_mana_pays_instant_sorcery_only() {
        use crate::basics::Zone;
        use crate::cards::build_game;
        use crate::ids::PlayerId;
        use crate::mana;

        let mut state = build_game(1, &[&[], &[]]);
        let chars = state.card_db().get(HYDRO_CHANNELER).unwrap().chars.clone();
        let hydro = state.add_card(PlayerId(0), chars, Zone::Battlefield);
        // A creature can't tap for mana while summoning-sick (CR 302.6); clear it for the test.
        state.objects.get_mut(&hydro).unwrap().summoning_sick = false;

        let u_cost = mana_cost(0, &[(Color::Blue, 1)]);
        // The restricted source funds a {U} I/S cast (allow_restricted = true) …
        assert!(mana::can_pay_ex(&state, PlayerId(0), &u_cost, true), "restricted {{U}} pays an I/S {{U}}");
        // … but NOT a creature spell / ability cost (allow_restricted = false).
        assert!(!mana::can_pay_ex(&state, PlayerId(0), &u_cost, false), "restricted {{U}} can't pay a non-I/S {{U}}");

        // Auto-paying an I/S {U} taps Hydro and routes its {U} through the restricted bucket, leaving
        // the pool empty afterward (produced then spent).
        assert!(mana::auto_pay_ex(&mut state, PlayerId(0), &u_cost, true).is_some(), "I/S cast pays via Hydro");
        assert!(state.object(hydro).status.tapped, "Hydro tapped to pay the I/S cost");
        assert_eq!(state.player(PlayerId(0)).mana_pool.total(), 0, "produced-and-spent, nothing floating");
    }

    /// Floating restricted mana (as Abstract Paintmage will add) obeys the same rule: it pays an I/S
    /// cost but is invisible to a creature spell / ability cost.
    #[test]
    fn floating_restricted_mana_is_instant_sorcery_only() {
        use crate::cards::build_game;
        use crate::ids::PlayerId;
        use crate::mana;
        let mut state = build_game(1, &[&[], &[]]);
        *state.player_mut(PlayerId(0)).mana_pool.restricted.entry(Color::Blue).or_insert(0) += 1;
        let u_cost = mana_cost(0, &[(Color::Blue, 1)]);
        assert!(mana::can_pay_ex(&state, PlayerId(0), &u_cost, true), "floating restricted {{U}} pays an I/S");
        assert!(!mana::can_pay(&state, PlayerId(0), &u_cost), "…but not a creature/ability cost");
        // Spending it on an I/S empties the restricted bucket.
        assert!(mana::auto_pay_ex(&mut state, PlayerId(0), &u_cost, true).is_some());
        assert_eq!(state.player(PlayerId(0)).mana_pool.total(), 0, "restricted {{U}} spent");
    }
}
