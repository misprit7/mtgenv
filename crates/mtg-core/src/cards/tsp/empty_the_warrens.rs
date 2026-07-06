//! Empty the Warrens — `{3}{R}` Sorcery (first printed TSP; reprinted on the SOS Mystical Archive `soa`).
//!
//! Oracle: "Create two 1/1 red Goblin creature tokens.
//! Storm (When you cast this spell, copy it for each spell cast before it this turn.)"
//!
//! **Fully implemented** — the spell effect creates two 1/1 red Goblin tokens; **storm** is a
//! `Triggered { SelfCast → CopySpellOnStack{ count: SpellsCastThisTurn − 1 } }` (the shared storm
//! idiom, see Prismari / Social Snub). The tokens have no targets, so the copies need no reselection.

use crate::basics::{CardType, Color};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::ability::{Ability, EventPattern};
use crate::effects::target::TokenSpec;
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};
use crate::subtypes::CreatureType;

/// grp id (bonus-sheet `soa` cards use the 600+ block).
pub const EMPTY_THE_WARRENS: u32 = 605;

pub fn register(db: &mut CardDb) {
    let effect = Effect::CreateToken {
        spec: TokenSpec {
            name: "Goblin".to_string(),
            card_types: vec![CardType::Creature],
            subtypes: vec![CreatureType::Goblin.into()],
            colors: vec![Color::Red],
            power: 1,
            toughness: 1,
            keywords: vec![],
            counters: vec![],
            grp_id: 0,
        },
        count: ValueExpr::Fixed(2),
        controller: PlayerRef::Controller,
        dynamic_counters: vec![],
    };
    let mut def = spell(
        EMPTY_THE_WARRENS,
        "Empty the Warrens",
        CardType::Sorcery,
        Color::Red,
        mana_cost(3, &[(Color::Red, 1)]),
        effect,
    )
    .with_text("Create two 1/1 red Goblin creature tokens.\nStorm (When you cast this spell, copy it for each spell cast before it this turn.)");
    // Storm (CR 702.40): when you cast this spell, copy it once per spell cast BEFORE it this turn
    // (= SpellsCastThisTurn − 1, since the count already includes this spell). No targets → no reselection.
    def.abilities.push(Ability::Triggered {
        event: EventPattern::SelfCast,
        condition: None,
        intervening_if: false,
        effect: Effect::CopySpellOnStack {
            what: EffectTarget::Triggering,
            count: ValueExpr::Sum(
                Box::new(ValueExpr::SpellsCastThisTurn { who: PlayerRef::Controller }),
                Box::new(ValueExpr::Fixed(-1)),
            ),
            choose_new_targets: false,
        },
    });
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, CastVariant, DecisionRequest, DecisionResponse, PlayerView};
    use crate::basics::{Phase, Zone};
    use crate::cards::{grp, starter_db};
    use crate::ids::PlayerId;
    use crate::priority::Engine;
    use crate::state::GameState;
    use std::sync::Arc;

    #[derive(Clone)]
    struct Passive;
    impl Agent for Passive {
        fn decide(&mut self, _v: &PlayerView, _r: &DecisionRequest) -> DecisionResponse {
            DecisionResponse::Pass
        }
    }

    fn db_with_card() -> CardDb {
        let mut db = starter_db();
        register(&mut db);
        db
    }

    #[test]
    fn empty_the_warrens_shape() {
        let db = db_with_card();
        let def = db.get(EMPTY_THE_WARRENS).unwrap();
        assert!(def.fully_implemented);
        assert!(def.abilities.iter().any(|a| matches!(a, Ability::Triggered { event: EventPattern::SelfCast, .. })), "storm trigger present");
    }

    /// Storm behaviour: after casting one prior spell this turn, casting Empty the Warrens copies it
    /// once — the original + one copy each make two Goblins → four Goblins total.
    #[test]
    fn storm_copies_once_after_one_prior_spell() {
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(db_with_card()));
        // A prior spell already cast this turn.
        state.players[0].spells_cast_this_turn = 1;
        let empty = {
            let c = state.card_db().get(EMPTY_THE_WARRENS).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Hand)
        };
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let mut e = Engine::new(state, vec![Box::new(Passive), Box::new(Passive)]);
        e.cast_spell(PlayerId(0), empty, CastVariant::WithoutPayingManaCost);
        e.run_agenda();
        while !e.state.stack.items.is_empty() {
            e.resolve_top();
            e.run_agenda();
        }
        let goblins = e
            .state
            .player(PlayerId(0))
            .battlefield
            .iter()
            .filter(|&&id| e.state.object(id).chars.name == "Goblin")
            .count();
        assert_eq!(goblins, 4, "original (2) + one storm copy (2) = 4 Goblins");
    }
}
