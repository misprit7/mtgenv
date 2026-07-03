//! Registered **token defs** — the reserved 9000+ `grp_id` block (see [`super::grp`]). A token created
//! from a [`TokenSpec`](crate::effects::target::TokenSpec) with a nonzero `grp_id` points at one of
//! these defs, so `def_of` supplies its **triggered/activated abilities** (keywords ride on the spec).
//! Each def carries `Supertype::Token`, so the deck-builder / `/api/cards` catalog filters it out.
//!
//! Card-agnostic law: a token's behaviour is still *data* (this def's `Ability` list), never a
//! name-match in the core.

use crate::basics::{CardType, Color};
use crate::cards::{grp, CardDb, CardDef};
use crate::effects::ability::{Ability, EventPattern};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;
use crate::state::Characteristics;
use crate::subtypes::{CreatureType, Supertype};

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
