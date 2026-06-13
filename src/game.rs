use std::collections::HashMap;
use crate::types::{PlayerId, Phase, CardId, PermanentId, Color};
use crate::card::Card;
use crate::player::{Player, Permanent};

#[derive(Debug, Clone)]
pub struct GameState {
    pub players: Vec<Player>,
    pub current_player: PlayerId,
    pub phase: Phase,
    pub stack: Vec<StackObject>,
    pub turn_number: u32,
    pub next_card_id: CardId,
    pub next_permanent_id: PermanentId,
}

#[derive(Debug, Clone)]
pub struct StackObject {
    pub card: Card,
    pub controller: PlayerId,
    pub targets: Vec<Target>,
}

#[derive(Debug, Clone)]
pub enum Target {
    Player(PlayerId),
    Permanent(PermanentId),
    Card(CardId),
}

#[derive(Debug, Clone)]
pub enum Decision {
    // Combat decisions
    DeclareAttackers(Vec<PermanentId>),
    DeclareBlockers(HashMap<PermanentId, PermanentId>), // attacker -> blocker
    
    // Spell casting decisions
    CastSpell {
        card_id: CardId,
        targets: Vec<Target>,
    },
    
    // Mana decisions
    PayManaCost {
        card_id: CardId,
        mana_payments: HashMap<Color, u32>,
        generic_mana: u32,
    },
    
    // Discard decisions
    DiscardCard(CardId),
    
    // Ability decisions
    ActivateAbility {
        permanent_id: PermanentId,
        ability_index: usize,
        targets: Vec<Target>,
    },
}

pub trait DecisionPoint {
    type Options;
    type Decision;
    
    fn get_options(&self, game_state: &GameState) -> Self::Options;
    fn apply_decision(&self, game_state: &mut GameState, decision: Self::Decision) -> Result<(), GameError>;
}

#[derive(Debug, thiserror::Error)]
pub enum GameError {
    #[error("Invalid decision: {0}")]
    InvalidDecision(String),
    
    #[error("Invalid target: {0}")]
    InvalidTarget(String),
    
    #[error("Not enough mana")]
    NotEnoughMana,
    
    #[error("Invalid phase for action")]
    InvalidPhase,
    
    #[error("Player not found: {0}")]
    PlayerNotFound(PlayerId),
    
    #[error("Card not found: {0}")]
    CardNotFound(CardId),
    
    #[error("Permanent not found: {0}")]
    PermanentNotFound(PermanentId),
}

impl GameState {
    pub fn new(players: Vec<Player>) -> Self {
        Self {
            players,
            current_player: PlayerId(0),
            phase: Phase::Untap,
            stack: Vec::new(),
            turn_number: 1,
            next_card_id: CardId(0),
            next_permanent_id: PermanentId(0),
        }
    }

    pub fn get_player(&self, player_id: PlayerId) -> Option<&Player> {
        self.players.iter().find(|p| p.id == player_id)
    }

    pub fn get_player_mut(&mut self, player_id: PlayerId) -> Option<&mut Player> {
        self.players.iter_mut().find(|p| p.id == player_id)
    }

    pub fn get_permanent(&self, permanent_id: PermanentId) -> Option<&Permanent> {
        self.players
            .iter()
            .flat_map(|p| &p.battlefield)
            .find(|p| p.id == permanent_id)
    }

    pub fn get_permanent_mut(&mut self, permanent_id: PermanentId) -> Option<&mut Permanent> {
        self.players
            .iter_mut()
            .flat_map(|p| &mut p.battlefield)
            .find(|p| p.id == permanent_id)
    }

    pub fn next_phase(&mut self) {
        self.phase = match self.phase {
            Phase::Untap => Phase::Upkeep,
            Phase::Upkeep => Phase::Draw,
            Phase::Draw => Phase::Main1,
            Phase::Main1 => Phase::BeginningOfCombat,
            Phase::BeginningOfCombat => Phase::DeclareAttackers,
            Phase::DeclareAttackers => Phase::DeclareBlockers,
            Phase::DeclareBlockers => Phase::CombatDamage,
            Phase::CombatDamage => Phase::EndOfCombat,
            Phase::EndOfCombat => Phase::Main2,
            Phase::Main2 => Phase::End,
            Phase::End => Phase::Cleanup,
            Phase::Cleanup => {
                self.turn_number += 1;
                Phase::Untap
            }
        };
    }
} 