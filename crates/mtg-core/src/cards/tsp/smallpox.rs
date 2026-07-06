//! Smallpox — `{B}{B}` Sorcery (first printed TSP; reprinted on the SOS Mystical Archive `soa`).
//!
//! Oracle: "Each player loses 1 life, discards a card, sacrifices a creature of their choice, then
//! sacrifices a land of their choice."
//!
//! **Fully implemented** — a symmetric edict as four `ForEachPlayer` passes (each step finishes for
//! all players before the next, CR 608.2), keyed to the iterated player via `PlayerRef::Each`: lose
//! 1 life, discard a card, sacrifice a creature (their choice), sacrifice a land (their choice). A
//! player with no creature/land simply can't sacrifice one. Mirrors the shipped `pox_plague` idiom.

use crate::basics::{CardType, Color, Zone};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::target::{CardFilter, SelectSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;

/// grp id (bonus-sheet `soa` cards use the 600+ block).
pub const SMALLPOX: u32 = 606;

/// "each player sacrifices a [creature|land] of their choice" — the iterated player picks one of theirs.
fn each_sacrifices(filter: CardFilter) -> Effect {
    Effect::ForEachPlayer {
        body: Box::new(Effect::Sacrifice {
            who: PlayerRef::Each,
            what: SelectSpec {
                zone: Zone::Battlefield,
                filter,
                chooser: PlayerRef::Each,
                min: ValueExpr::Fixed(1),
                max: ValueExpr::Fixed(1),
            },
        }),
    }
}

pub fn register(db: &mut CardDb) {
    let effect = Effect::Sequence(vec![
        Effect::ForEachPlayer { body: Box::new(Effect::LoseLife { who: PlayerRef::Each, amount: ValueExpr::Fixed(1) }) },
        Effect::ForEachPlayer { body: Box::new(Effect::Discard { who: PlayerRef::Each, count: ValueExpr::Fixed(1) }) },
        each_sacrifices(CardFilter::HasCardType(CardType::Creature)),
        each_sacrifices(CardFilter::HasCardType(CardType::Land)),
    ]);
    db.insert(
        spell(
            SMALLPOX,
            "Smallpox",
            CardType::Sorcery,
            Color::Black,
            mana_cost(0, &[(Color::Black, 2)]),
            effect,
        )
        .with_text("Each player loses 1 life, discards a card, sacrifices a creature of their choice, then sacrifices a land of their choice."),
    );
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
    fn smallpox_shape() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(SMALLPOX).unwrap();
        assert!(def.fully_implemented);
        let Some(Effect::Sequence(steps)) = def.spell_effect() else { panic!("sequence") };
        assert_eq!(steps.len(), 4);
        assert!(steps.iter().all(|s| matches!(s, Effect::ForEachPlayer { .. })));
    }

    /// Real cast + resolution: each player (P0 controller, P1) starts with 1 creature + 1 land + 1
    /// card in hand. After Smallpox both lose 1 life, discard the card, and sacrifice their creature
    /// and their land (nothing left on the battlefield).
    #[test]
    fn each_player_pays_the_edict() {
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(db_with_card()));
        for p in [PlayerId(0), PlayerId(1)] {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            state.add_card(p, c, Zone::Hand);
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            state.add_card(p, c, Zone::Battlefield);
            let c = state.card_db().get(grp::FOREST).unwrap().chars.clone();
            state.add_card(p, c, Zone::Battlefield);
        }
        let pox = {
            let c = state.card_db().get(SMALLPOX).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Hand)
        };
        state.players[0].life = 20;
        state.players[1].life = 20;
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
        e.cast_spell(PlayerId(0), pox, CastVariant::WithoutPayingManaCost);
        e.run_agenda();
        while !e.state.stack.items.is_empty() {
            e.resolve_top();
            e.run_agenda();
        }
        for p in [PlayerId(0), PlayerId(1)] {
            assert_eq!(e.state.player(p).life, 19, "lost 1 life");
            assert_eq!(e.state.player(p).hand.len(), 0, "discarded the card");
            assert_eq!(e.state.player(p).battlefield.len(), 0, "sacrificed a creature and a land");
        }
    }
}
