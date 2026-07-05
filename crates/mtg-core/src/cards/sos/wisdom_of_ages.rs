//! Wisdom of Ages — `{4}{U}{U}{U}` Sorcery.
//!
//! Oracle: "Return all instant and sorcery cards from your graveyard to your hand. You have no maximum
//! hand size for the rest of the game. Exile Wisdom of Ages."
//!
//! **Fully implemented** — three parts, each a small reusable piece:
//! - **Return all I/S from your graveyard** — a `ForEach{ selector: all instant|sorcery in your
//!   graveyard (max 999 = "all", no prompt), body: MoveZone{ Each → Hand } }` (the Jadzi "max 999"
//!   select-all idiom).
//! - **No maximum hand size for the rest of the game** — the new `Effect::SetNoMaxHandSize` (lifts the
//!   cleanup discard limit permanently, CR 402.2).
//! - **Exile Wisdom of Ages** — the new `Ability::ExileOnResolve` marker: `resolve_top` puts the card
//!   into exile instead of the graveyard as it finishes (CR 608.2n override).

use crate::basics::{CardType, Color, Zone, ZoneDest, ZonePos};
use crate::cards::helpers::instant_or_sorcery;
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::ability::Ability;
use crate::effects::target::SelectSpec;
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};

/// grp id (per-set ids live near their cards).
pub const WISDOM_OF_AGES: u32 = 433;

pub fn register(db: &mut CardDb) {
    let effect = Effect::Sequence(vec![
        Effect::ForEach {
            selector: SelectSpec {
                zone: Zone::Graveyard,
                filter: instant_or_sorcery(),
                chooser: PlayerRef::Controller,
                min: ValueExpr::Fixed(0),
                max: ValueExpr::Fixed(999),
            },
            body: Box::new(Effect::MoveZone {
                what: EffectTarget::Each,
                to: ZoneDest { zone: Zone::Hand, pos: ZonePos::Any },
                tapped: false,
            }),
        },
        Effect::SetNoMaxHandSize { who: PlayerRef::Controller },
    ]);
    let mut def = spell(
        WISDOM_OF_AGES,
        "Wisdom of Ages",
        CardType::Sorcery,
        Color::Blue,
        mana_cost(4, &[(Color::Blue, 3)]),
        effect,
    )
    .with_text("Return all instant and sorcery cards from your graveyard to your hand. You have no maximum hand size for the rest of the game.\nExile Wisdom of Ages.");
    def.abilities.push(Ability::ExileOnResolve);
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
    fn wisdom_shape() {
        let db = db_with_card();
        let def = db.get(WISDOM_OF_AGES).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Sorcery]);
        assert_eq!(def.chars.colors, vec![Color::Blue]);
        assert_eq!(def.chars.mana_value(), 7);
        assert!(def.fully_implemented);
        assert!(def.abilities.iter().any(|a| matches!(a, Ability::ExileOnResolve)));
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

    /// P0 has Wisdom of Ages in hand, an instant + a sorcery + a creature in the graveyard, and lands.
    /// Resolving it: the I/S cards return to hand (the creature stays in gy), hand-size limit lifts,
    /// and Wisdom of Ages itself is exiled (not in the graveyard).
    #[test]
    fn returns_instants_and_sorceries_lifts_hand_size_and_self_exiles() {
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(db_with_card()));
        let wis = {
            let c = state.card_db().get(WISDOM_OF_AGES).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Hand)
        };
        // Graveyard: a Lightning Bolt (instant), a Divination (sorcery), a Grizzly Bears (creature).
        let bolt = {
            let c = state.card_db().get(grp::LIGHTNING_BOLT).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Graveyard)
        };
        let divi = {
            let c = state.card_db().get(grp::DIVINATION).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Graveyard)
        };
        let bear = {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Graveyard)
        };
        for _ in 0..7 {
            let c = state.card_db().get(grp::ISLAND).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield); // {4}{U}{U}{U}
        }
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
        e.cast_spell(PlayerId(0), wis, CastVariant::Normal);
        drive(&mut e);

        let hand = &e.state.player(PlayerId(0)).hand;
        assert!(hand.contains(&bolt), "the instant returned to hand");
        assert!(hand.contains(&divi), "the sorcery returned to hand");
        assert!(!hand.contains(&bear), "the creature did NOT return");
        assert!(e.state.player(PlayerId(0)).graveyard.contains(&bear), "the creature stays in gy");
        assert!(e.state.player(PlayerId(0)).exile.contains(&wis), "Wisdom of Ages exiled itself");
        assert!(!e.state.player(PlayerId(0)).graveyard.contains(&wis), "not in the graveyard");
        assert_eq!(e.state.player(PlayerId(0)).hand_size_limit, usize::MAX, "no maximum hand size");
    }
}
