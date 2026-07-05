//! Lorehold, the Historian — `{3}{R}{W}` Legendary Creature — Elder Dragon 5/5.
//!
//! Oracle: "Flying, haste. Each instant and sorcery card in your hand has miracle {2}. (You may cast
//! a card for its miracle cost when you draw it if it's the first card you drew this turn.) At the
//! beginning of each opponent's upkeep, you may discard a card. If you do, draw a card."
//!
//! **Fully implemented** — the Lorehold (Miracle) Elder Dragon, over the new miracle subsystem:
//! - **5/5 flying, haste** body.
//! - **Grants miracle {2} to your I/S** = `Ability::GrantMiracle{ {2}, instant|sorcery }`. When you
//!   draw your first card of the turn and it's an I/S you control, `draw()` queues a
//!   `StackObjectKind::MiracleWindow`; on resolution you may cast it for {2} (`CastVariant::Miracle`).
//! - **Opp-upkeep loot** = `Triggered{ BeginningOfStep(Upkeep), if Not(YourTurn), Optional{ IfYouDo{
//!   Discard 1, Draw 1 } } }` — composes over existing machinery. (The looted draw can itself be your
//!   first draw of that turn and open a miracle window — the subsystem composes.)

use crate::basics::{CardType, Color, Zone};
use crate::cards::helpers::instant_or_sorcery;
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern, Keyword};
use crate::effects::condition::Condition;
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;
use crate::subtypes::{CreatureType, Supertype};

/// grp id (per-set ids live near their cards).
pub const LOREHOLD_THE_HISTORIAN: u32 = 429;

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        LOREHOLD_THE_HISTORIAN,
        "Lorehold, the Historian",
        &[CreatureType::Elder, CreatureType::Dragon],
        Color::Red,
        mana_cost(3, &[(Color::Red, 1), (Color::White, 1)]),
        5,
        5,
        vec![
            // "Each instant and sorcery card in your hand has miracle {2}."
            Ability::GrantMiracle { cost: mana_cost(2, &[]), filter: instant_or_sorcery() },
            // "At the beginning of each opponent's upkeep, you may discard a card. If you do, draw a card."
            Ability::Triggered {
                event: EventPattern::BeginningOfStep(crate::basics::Phase::Upkeep),
                condition: Some(Condition::Not(Box::new(Condition::YourTurn))),
                intervening_if: false,
                effect: Effect::Optional {
                    prompt: "Discard a card to draw a card?".to_string(),
                    body: Box::new(Effect::IfYouDo {
                        cost: Box::new(Effect::Discard {
                            who: PlayerRef::Controller,
                            count: ValueExpr::Fixed(1),
                        }),
                        reward: Box::new(Effect::Draw {
                            who: PlayerRef::Controller,
                            count: ValueExpr::Fixed(1),
                        }),
                    }),
                },
            },
        ],
    );
    def.chars.supertypes = vec![Supertype::Legendary];
    def.chars.colors = vec![Color::Red, Color::White];
    def.chars.keywords = vec![Keyword::Flying, Keyword::Haste];
    def.text = "Flying, haste\nEach instant and sorcery card in your hand has miracle {2}. (You may cast a card for its miracle cost when you draw it if it's the first card you drew this turn.)\nAt the beginning of each opponent's upkeep, you may discard a card. If you do, draw a card.".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, ConfirmKind, DecisionRequest, DecisionResponse, PlayerView};
    use crate::basics::Phase;
    use crate::cards::{grp, mana_cost as mc, spell, starter_db};
    use crate::ids::{ObjId, PlayerId};
    use crate::priority::Engine;
    use crate::state::GameState;
    use expect_test::expect;
    use std::sync::Arc;

    /// A test-only generic instant/sorcery whose effect never touches the library — a `{5}` sorcery
    /// ("you gain 3 life"), so a successful cast on 2 lands proves the {2} miracle cost was used.
    const TEST_BIG_SORCERY: u32 = 990_429;

    fn db_with_card() -> CardDb {
        let mut db = starter_db();
        register(&mut db);
        db.insert(spell(
            TEST_BIG_SORCERY,
            "Big Study",
            CardType::Sorcery,
            Color::Blue,
            mc(5, &[]),
            Effect::GainLife { who: PlayerRef::Controller, amount: ValueExpr::Fixed(3) },
        ));
        db
    }

    #[test]
    fn lorehold_shape() {
        let db = db_with_card();
        let def = db.get(LOREHOLD_THE_HISTORIAN).unwrap();
        assert_eq!(def.chars.supertypes, vec![Supertype::Legendary]);
        assert_eq!(def.chars.colors, vec![Color::Red, Color::White]);
        assert_eq!(def.chars.keywords, vec![Keyword::Flying, Keyword::Haste]);
        assert_eq!((def.chars.power, def.chars.toughness), (Some(5), Some(5)));
        assert!(def.fully_implemented);
        assert!(matches!(def.abilities[0], Ability::GrantMiracle { .. }));
        assert!(matches!(
            def.abilities[1],
            Ability::Triggered { event: EventPattern::BeginningOfStep(Phase::Upkeep), .. }
        ));
    }

    /// Casts the miracle when offered (the `MayEffect` confirm during a MiracleWindow).
    #[derive(Clone)]
    struct MiracleAgent {
        cast: bool,
    }
    impl Agent for MiracleAgent {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::Confirm { kind: ConfirmKind::MayEffect } => DecisionResponse::Bool(self.cast),
                DecisionRequest::Confirm { .. } => DecisionResponse::Bool(true),
                _ => DecisionResponse::Pass,
            }
        }
    }

    fn drive(e: &mut Engine) {
        loop {
            e.run_agenda();
            if e.state.stack.items.is_empty() {
                break;
            }
            e.resolve_top();
        }
    }

    /// Base: Lorehold on P0's battlefield + `lands` Mountains; `library` seeded (top = last). Returns
    /// (engine, the library card ids in draw order [first drawn ... ]).
    fn setup(lands: usize, library_top_down: &[u32], cast: bool) -> (Engine, Vec<ObjId>) {
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(db_with_card()));
        {
            let c = state.card_db().get(LOREHOLD_THE_HISTORIAN).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield);
        }
        for _ in 0..lands {
            let m = state.card_db().get(grp::MOUNTAIN).unwrap().chars.clone();
            state.add_card(PlayerId(0), m, Zone::Battlefield);
        }
        // Seed the library so `library_top_down[0]` is drawn first: push bottom→top = reverse.
        let mut ids = Vec::new();
        for &g in library_top_down.iter().rev() {
            let c = state.card_db().get(g).unwrap().chars.clone();
            ids.push(state.add_card(PlayerId(0), c, Zone::Library));
        }
        ids.reverse(); // now ids[0] is the first-drawn (top) card
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let e = Engine::new(state, vec![Box::new(MiracleAgent { cast }), Box::new(MiracleAgent { cast })]);
        (e, ids)
    }

    /// The headline (CR 702.94): drawing your first card of the turn — a granted-miracle I/S — opens a
    /// window; casting it for {2} succeeds on just 2 lands (its printed cost is {5}, unaffordable).
    #[test]
    fn first_draw_of_a_granted_miracle_card_can_be_cast_for_two() {
        let (mut e, ids) = setup(2, &[TEST_BIG_SORCERY], true);
        let p0_start = e.state.player(PlayerId(0)).life;
        e.draw(PlayerId(0), 1); // first draw of the turn
        drive(&mut e);
        // The sorcery was cast (for {2}) and resolved → in the graveyard, +3 life.
        assert!(e.state.player(PlayerId(0)).graveyard.contains(&ids[0]), "miracle-cast + resolved");
        assert_eq!(e.state.player(PlayerId(0)).life, p0_start + 3, "Big Study resolved for +3 life");
    }

    /// Declining the miracle: the window resolves with no cast — the card stays in hand.
    #[test]
    fn declining_the_miracle_keeps_the_card_in_hand() {
        let (mut e, ids) = setup(2, &[TEST_BIG_SORCERY], false);
        e.draw(PlayerId(0), 1);
        drive(&mut e);
        assert!(e.state.player(PlayerId(0)).hand.contains(&ids[0]), "declined → card still in hand");
    }

    /// CR 702.94e — only the FIRST card of the first draw qualifies: drawing 2 at once (both grantable
    /// I/S) opens a window ONLY for the first card; the second is never castable via miracle.
    #[test]
    fn second_card_of_the_same_draw_is_not_miracle() {
        let (mut e, ids) = setup(2, &[TEST_BIG_SORCERY, TEST_BIG_SORCERY], true);
        e.draw(PlayerId(0), 2); // first draw event of the turn draws two cards
        drive(&mut e);
        // Exactly one was miracle-cast (the first); the second stayed in hand.
        assert!(e.state.player(PlayerId(0)).graveyard.contains(&ids[0]), "first card miracle-cast");
        assert!(e.state.player(PlayerId(0)).hand.contains(&ids[1]), "second card NOT castable via miracle");
    }

    /// Not the first draw: after any earlier draw this turn, a later drawn granted-miracle I/S opens
    /// no window.
    #[test]
    fn a_later_draw_is_not_miracle() {
        // Library top→down: a Mountain (drawn first, non-I/S), then the sorcery (2nd draw).
        let (mut e, ids) = setup(2, &[grp::MOUNTAIN, TEST_BIG_SORCERY], true);
        e.draw(PlayerId(0), 1); // first draw = the Mountain (uses up "first this turn")
        drive(&mut e);
        e.draw(PlayerId(0), 1); // second draw = the sorcery — NOT eligible
        drive(&mut e);
        assert!(e.state.player(PlayerId(0)).hand.contains(&ids[1]), "a later draw is not miracle");
    }
}
