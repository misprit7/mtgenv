use std::collections::HashMap;
use crate::types::{PlayerId, Color, ManaPool, CardId, PermanentId};
use crate::card::Card;

#[derive(Debug, Clone)]
pub struct Player {
    pub id: PlayerId,
    pub life_total: i32,
    pub hand: Vec<Card>,
    pub library: Vec<Card>,
    pub graveyard: Vec<Card>,
    pub battlefield: Vec<Permanent>,
    pub mana_pool: ManaPool,
    pub max_hand_size: usize,
}

#[derive(Debug, Clone)]
pub struct Permanent {
    pub id: PermanentId,
    pub card: Card,
    pub controller: PlayerId,
    pub owner: PlayerId,
    pub tapped: bool,
    pub damage: u32,
    pub counters: HashMap<String, u32>,
}

impl Player {
    pub fn new(id: PlayerId, library: Vec<Card>) -> Self {
        Self {
            id,
            life_total: 20,
            hand: Vec::new(),
            library,
            graveyard: Vec::new(),
            battlefield: Vec::new(),
            mana_pool: ManaPool {
                generic: 0,
                colored: HashMap::new(),
            },
            max_hand_size: 7,
        }
    }

    pub fn draw_card(&mut self) -> Option<Card> {
        self.library.pop().map(|card| {
            self.hand.push(card.clone());
            card
        })
    }

    pub fn discard_card(&mut self, card_id: CardId) -> Option<Card> {
        if let Some(pos) = self.hand.iter().position(|c| c.id == card_id) {
            let card = self.hand.remove(pos);
            self.graveyard.push(card.clone());
            Some(card)
        } else {
            None
        }
    }

    pub fn add_mana(&mut self, color: Color, amount: u32) {
        *self.mana_pool.colored.entry(color).or_insert(0) += amount;
    }

    pub fn add_generic_mana(&mut self, amount: u32) {
        self.mana_pool.generic += amount;
    }

    pub fn can_pay_mana_cost(&self, cost: &crate::types::ManaCost) -> bool {
        // Check if we have enough generic mana
        if self.mana_pool.generic < cost.generic {
            return false;
        }

        // Check if we have enough of each colored mana
        for (color, amount) in &cost.colored {
            if self.mana_pool.colored.get(color).unwrap_or(&0) < amount {
                return false;
            }
        }

        true
    }
} 