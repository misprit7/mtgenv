use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use crate::types::{CardId, CardType, Color, ManaCost};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Card {
    pub id: CardId,
    pub name: String,
    pub mana_cost: ManaCost,
    pub card_type: CardType,
    pub colors: Vec<Color>,
    pub power: Option<u32>,
    pub toughness: Option<u32>,
    pub abilities: Vec<Ability>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ability {
    pub name: String,
    pub description: String,
    pub cost: Option<ManaCost>,
    pub effect: Effect,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Effect {
    // Basic effects
    DealDamage { amount: u32 },
    GainLife { amount: u32 },
    DrawCards { amount: u32 },
    AddMana { colors: HashMap<Color, u32> },
    
    // Complex effects will be added later
    // These will include things like:
    // - Target selection
    // - Conditional effects
    // - Multiple effects
    // - etc.
}

impl Card {
    pub fn new(
        id: CardId,
        name: String,
        mana_cost: ManaCost,
        card_type: CardType,
        colors: Vec<Color>,
        power: Option<u32>,
        toughness: Option<u32>,
        abilities: Vec<Ability>,
    ) -> Self {
        Self {
            id,
            name,
            mana_cost,
            card_type,
            colors,
            power,
            toughness,
            abilities,
        }
    }

    pub fn is_creature(&self) -> bool {
        matches!(self.card_type, CardType::Creature)
    }

    pub fn is_land(&self) -> bool {
        matches!(self.card_type, CardType::Land)
    }
} 