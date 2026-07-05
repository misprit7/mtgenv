//! Pox Plague — `{B}{B}{B}{B}{B}` Sorcery.
//!
//! Oracle: "Each player loses half their life, then discards half the cards in their hand, then
//! sacrifices half the permanents they control of their choice. Round down each time."
//!
//! **Fully implemented in pure IR** — the ledger tagged this "Native (halving)", but halving is a generic
//! value and "each player does X to their own stuff" is a generic loop. Three [`Effect::ForEachPlayer`]
//! passes (each step finishes for all players before the next, CR 608.2), each body keyed to the iterated
//! player via [`PlayerRef::Each`]:
//! - lose `Half(LifeTotal{Each})` life,
//! - discard `Half(HandSize{Each})` cards (their choice),
//! - sacrifice `Half(Count{ their permanents })` permanents (their choice).
//!
//! New generic pieces (all evergreen — halving effects recur across sets): [`ValueExpr::Half`],
//! [`ValueExpr::LifeTotal`], and [`Effect::ForEachPlayer`] (the player analogue of `ForEach`, binding
//! `foreach_current` per player). No Native hatch.

use crate::basics::{Color, CardType, Zone};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::target::{CardFilter, SelectSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;

/// grp id (per-set ids live near their cards).
pub const POX_PLAGUE: u32 = 460;

/// `Half(Count{ battlefield permanents controlled by the iterated player })`.
fn half_permanents() -> ValueExpr {
    ValueExpr::Half(Box::new(ValueExpr::Count {
        zone: Zone::Battlefield,
        filter: CardFilter::Any,
        controller: Some(PlayerRef::Each),
    }))
}

pub fn register(db: &mut CardDb) {
    let lose_half_life = Effect::ForEachPlayer {
        body: Box::new(Effect::LoseLife {
            who: PlayerRef::Each,
            amount: ValueExpr::Half(Box::new(ValueExpr::LifeTotal { who: PlayerRef::Each })),
        }),
    };
    let discard_half = Effect::ForEachPlayer {
        body: Box::new(Effect::Discard {
            who: PlayerRef::Each,
            count: ValueExpr::Half(Box::new(ValueExpr::HandSize { who: PlayerRef::Each })),
        }),
    };
    let sacrifice_half = Effect::ForEachPlayer {
        body: Box::new(Effect::Sacrifice {
            who: PlayerRef::Each,
            what: SelectSpec {
                zone: Zone::Battlefield,
                filter: CardFilter::Any,
                chooser: PlayerRef::Each,
                min: half_permanents(),
                max: half_permanents(),
            },
        }),
    };
    let effect = Effect::Sequence(vec![lose_half_life, discard_half, sacrifice_half]);
    let def = spell(
        POX_PLAGUE,
        "Pox Plague",
        CardType::Sorcery,
        Color::Black,
        mana_cost(0, &[(Color::Black, 5)]),
        effect,
    )
    .with_text("Each player loses half their life, then discards half the cards in their hand, then sacrifices half the permanents they control of their choice. Round down each time.");
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{CastVariant, RandomAgent};
    use crate::basics::Phase;
    use crate::cards::{grp, starter_db};
    use crate::ids::PlayerId;
    use crate::priority::Engine;
    use crate::state::GameState;
    use std::sync::Arc;

    fn db_with_card() -> CardDb {
        let mut db = starter_db();
        register(&mut db);
        db
    }

    #[test]
    fn pox_shape() {
        let db = db_with_card();
        let def = db.get(POX_PLAGUE).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Sorcery]);
        assert_eq!(def.chars.mana_value(), 5);
        let Some(Effect::Sequence(steps)) = def.spell_effect() else { panic!("sequence") };
        assert_eq!(steps.len(), 3);
        assert!(steps.iter().all(|s| matches!(s, Effect::ForEachPlayer { .. })));
    }

    /// Real resolution: P0 (life 20, 4 cards, 3 permanents) and P1 (life 18, 3 cards, 5 permanents).
    /// After Pox: each loses half life (10 / 9 → 10 / 9), discards half hand (2 / 1), sacrifices half
    /// permanents (1 / 2). Round down each time.
    #[test]
    fn each_player_halves_life_hand_and_permanents() {
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(db_with_card()));
        // Hands (Grizzly Bears as generic fodder): P0 = 4, P1 = 3.
        for _ in 0..4 {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Hand);
        }
        for _ in 0..3 {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            state.add_card(PlayerId(1), c, Zone::Hand);
        }
        // Permanents (creatures, no lands so the count is exact): P0 = 3, P1 = 5.
        for _ in 0..3 {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield);
        }
        for _ in 0..5 {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            state.add_card(PlayerId(1), c, Zone::Battlefield);
        }
        // The Pox spell on the stack, cast by P0. Life: P0 = 20, P1 = 18.
        let pox = {
            let c = state.card_db().get(POX_PLAGUE).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Hand)
        };
        state.players[0].life = 20;
        state.players[1].life = 18;
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);

        // Cast Pox for free (bypass its {B}{B}{B}{B}{B}) and resolve.
        e.cast_spell(PlayerId(0), pox, CastVariant::WithoutPayingManaCost);
        e.resolve_top();

        assert_eq!(e.state.player(PlayerId(0)).life, 10, "P0 lost half of 20");
        assert_eq!(e.state.player(PlayerId(1)).life, 9, "P1 lost half of 18");
        assert_eq!(e.state.player(PlayerId(0)).hand.len(), 4 - 2, "P0 discarded half of 4");
        assert_eq!(e.state.player(PlayerId(1)).hand.len(), 3 - 1, "P1 discarded half of 3 (round down)");
        assert_eq!(e.state.player(PlayerId(0)).battlefield.len(), 3 - 1, "P0 sacrificed half of 3 (round down)");
        assert_eq!(e.state.player(PlayerId(1)).battlefield.len(), 5 - 2, "P1 sacrificed half of 5 (round down)");
    }
}
