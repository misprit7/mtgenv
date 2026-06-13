mod types;
mod card;
mod player;
mod game;

use types::{PlayerId, CardId, CardType, Color, ManaCost};
use card::Card;
use player::Player;
use game::GameState;
use std::collections::HashMap;

fn main() {
    println!("Magic: The Gathering Engine");
    
    // Example of creating a simple game
    let mut game = create_sample_game();
    
    // Example of playing a turn
    play_sample_turn(&mut game);
}

fn create_sample_game() -> GameState {
    // Create two players with simple decks
    let player1 = create_sample_player(PlayerId(0));
    let player2 = create_sample_player(PlayerId(1));
    
    GameState::new(vec![player1, player2])
}

fn create_sample_player(id: PlayerId) -> Player {
    // Create a simple deck with basic cards
    let mut library = Vec::new();
    
    // Add some basic lands
    for i in 0..20 {
        library.push(Card::new(
            CardId(i),
            format!("Plains {}", i + 1),
            ManaCost {
                generic: 0,
                colored: HashMap::new(),
            },
            CardType::Land,
            vec![Color::White],
            None,
            None,
            Vec::new(),
        ));
    }
    
    // Add some basic creatures
    for i in 20..30 {
        library.push(Card::new(
            CardId(i),
            format!("Soldier {}", i - 19),
            ManaCost {
                generic: 1,
                colored: {
                    let mut m = HashMap::new();
                    m.insert(Color::White, 1);
                    m
                },
            },
            CardType::Creature,
            vec![Color::White],
            Some(1),
            Some(1),
            Vec::new(),
        ));
    }
    
    Player::new(id, library)
}

fn play_sample_turn(game: &mut GameState) {
    println!("Starting a new turn");
    
    // Untap step
    println!("Untap step");
    // TODO: Implement untap logic
    
    // Upkeep step
    println!("Upkeep step");
    // TODO: Implement upkeep triggers
    
    // Draw step
    println!("Draw step");
    if let Some(player) = game.get_player_mut(game.current_player) {
        if let Some(card) = player.draw_card() {
            println!("Drew card: {}", card.name);
        }
    }
    
    // Main phase 1
    println!("Main phase 1");
    // TODO: Implement main phase actions
    
    // Combat phase
    println!("Combat phase");
    // TODO: Implement combat
    
    // Main phase 2
    println!("Main phase 2");
    // TODO: Implement main phase actions
    
    // End step
    println!("End step");
    // TODO: Implement end step triggers
    
    // Cleanup step
    println!("Cleanup step");
    // TODO: Implement cleanup
    
    game.next_phase();
    println!("Turn completed");
} 