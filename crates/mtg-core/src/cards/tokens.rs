//! Registered **token defs** — the reserved 9000+ `grp_id` block (see [`super::grp`]). A token created
//! from a [`TokenSpec`](crate::effects::target::TokenSpec) with a nonzero `grp_id` points at one of
//! these defs, so `def_of` supplies its **triggered/activated abilities** (keywords ride on the spec).
//! Each def carries `Supertype::Token`, so the deck-builder / `/api/cards` catalog filters it out.
//!
//! Card-agnostic law: a token's behaviour is still *data* (this def's `Ability` list), never a
//! name-match in the core.

use crate::basics::{CardType, Color};
use crate::cards::helpers::sacrifice_self;
use crate::cards::{grp, CardDb, CardDef};
use crate::effects::ability::{Ability, Cost, CostComponent, EventPattern, Timing};
use crate::effects::target::ManaSpec;
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;
use crate::state::Characteristics;
use crate::subtypes::{ArtifactType, CreatureType, Supertype};

pub fn register(db: &mut CardDb) {
    // 1/1 black-and-green Pest — "Whenever this token attacks, you gain 1 life." (SoS Witherbloom).
    db.insert(CardDef {
        chars: Characteristics {
            name: "Pest".to_string(),
            card_types: vec![CardType::Creature],
            subtypes: vec![CreatureType::Pest.into()],
            supertypes: vec![Supertype::Token],
            colors: vec![Color::Black, Color::Green],
            power: Some(1),
            toughness: Some(1),
            grp_id: grp::PEST_TOKEN,
            ..Default::default()
        },
        abilities: vec![Ability::Triggered {
            event: EventPattern::SelfAttacks,
            condition: None,
            intervening_if: false,
            effect: Effect::GainLife { who: PlayerRef::Controller, amount: ValueExpr::Fixed(1) },
        }],
        text: "Whenever this token attacks, you gain 1 life.".to_string(),
        fully_implemented: true,
    });

    // Treasure — colourless artifact token: "{T}, Sacrifice this token: Add one mana of any color."
    // (CR 111.3 / Treasure). A cost-bearing mana ability (the sacrifice) — usable only via manual mana
    // activation, kept out of the auto-pay pool (`mana::mana_sources_kind` skips non-`{T}` mana costs).
    db.insert(CardDef {
        chars: Characteristics {
            name: "Treasure".to_string(),
            card_types: vec![CardType::Artifact],
            subtypes: vec![ArtifactType::Treasure.into()],
            supertypes: vec![Supertype::Token],
            colors: vec![], // colourless
            grp_id: grp::TREASURE_TOKEN,
            ..Default::default()
        },
        abilities: vec![Ability::Activated {
            cost: Cost {
                mana: None,
                components: vec![CostComponent::TapSelf, CostComponent::Sacrifice(sacrifice_self())],
            },
            effect: Effect::AddMana {
                who: PlayerRef::Controller,
                mana: ManaSpec { produces: vec![], any_color: Some(ValueExpr::Fixed(1)), restriction: None },
            },
            timing: Timing::Instant,
            restriction: None,
            is_mana: true,
        }],
        text: "{T}, Sacrifice this token: Add one mana of any color.".to_string(),
        fully_implemented: true,
    });

    // Clue — colourless artifact token: "{2}, Sacrifice this token: Draw a card." (CR 111.3 / Clue,
    // Investigate). A non-mana activated ability offered at priority like any other.
    db.insert(CardDef {
        chars: Characteristics {
            name: "Clue".to_string(),
            card_types: vec![CardType::Artifact],
            subtypes: vec![ArtifactType::Clue.into()],
            supertypes: vec![Supertype::Token],
            colors: vec![], // colourless
            grp_id: grp::CLUE_TOKEN,
            ..Default::default()
        },
        abilities: vec![Ability::Activated {
            cost: Cost {
                mana: Some(crate::cards::mana_cost(2, &[])),
                components: vec![CostComponent::Sacrifice(sacrifice_self())],
            },
            effect: Effect::Draw { who: PlayerRef::Controller, count: ValueExpr::Fixed(1) },
            timing: Timing::Instant,
            restriction: None,
            is_mana: false,
        }],
        text: "{2}, Sacrifice this token: Draw a card.".to_string(),
        fully_implemented: true,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pest_token_def_is_registered_and_token_supertyped() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(grp::PEST_TOKEN).unwrap();
        assert!(def.chars.supertypes.contains(&Supertype::Token), "Token supertype → excluded from the catalog");
        assert!(matches!(def.abilities[0], Ability::Triggered { event: EventPattern::SelfAttacks, .. }));
    }
}
