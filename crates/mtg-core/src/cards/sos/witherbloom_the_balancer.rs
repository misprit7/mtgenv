//! Witherbloom, the Balancer — `{6}{B}{G}` Legendary Creature — Elder Dragon 5/5.
//!
//! Oracle: "Affinity for creatures (This spell costs {1} less to cast for each creature you control.)
//! Flying, deathtouch. Instant and sorcery spells you cast have affinity for creatures."
//!
//! **Fully implemented** — the Witherbloom (Affinity) Elder Dragon, a composition over the CR 118
//! cost-reduction pipeline:
//! - **5/5 flying, deathtouch** body.
//! - **Own affinity for creatures** = `CostReduction{ GenericValue(Count{ creatures you control }),
//!   Always, Cast }` (the Dawning-Archaic-proven self-cost-reduction idiom; read by
//!   `effective_cast_cost`). Reduces generic only (CR 702.40) and never below {0}.
//! - **Granted affinity to your I/S spells** = the new [`Ability::GrantCostReduction`]`{ GenericValue(
//!   Count{ creatures you control }), spell_filter: instant|sorcery }` — a cost-modification static
//!   that `effective_cast_cost` gathers from every permanent the caster controls and applies to a cast
//!   card matching its filter. So each instant/sorcery you cast while Witherbloom is out costs {1} less
//!   per creature you control (Witherbloom included).

use crate::basics::{CardType, Color, Zone};
use crate::cards::helpers::instant_or_sorcery;
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{
    Ability, CostReductionAmount, CostReductionCondition, CostReductionScope, Keyword,
};
use crate::effects::condition::Condition;
use crate::effects::target::CardFilter;
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::subtypes::{CreatureType, Supertype};

/// grp id (per-set ids live near their cards).
pub const WITHERBLOOM_THE_BALANCER: u32 = 425;

/// "for each creature you control" — the affinity count, shared by the own and granted clauses.
fn creatures_you_control() -> ValueExpr {
    ValueExpr::Count {
        zone: Zone::Battlefield,
        filter: CardFilter::HasCardType(CardType::Creature),
        controller: Some(PlayerRef::Controller),
    }
}

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        WITHERBLOOM_THE_BALANCER,
        "Witherbloom, the Balancer",
        &[CreatureType::Elder, CreatureType::Dragon],
        Color::Black,
        mana_cost(6, &[(Color::Black, 1), (Color::Green, 1)]),
        5,
        5,
        vec![
            // Own affinity for creatures (reduces the cost to cast Witherbloom itself).
            Ability::CostReduction {
                amount: CostReductionAmount::GenericValue(creatures_you_control()),
                condition: CostReductionCondition::State(Condition::Always),
                scope: CostReductionScope::Cast,
            },
            // "Instant and sorcery spells you cast have affinity for creatures" — granted to others.
            Ability::GrantCostReduction {
                amount: CostReductionAmount::GenericValue(creatures_you_control()),
                spell_filter: instant_or_sorcery(),
            },
        ],
    );
    def.chars.supertypes = vec![Supertype::Legendary];
    def.chars.colors = vec![Color::Black, Color::Green];
    def.chars.keywords = vec![Keyword::Flying, Keyword::Deathtouch];
    def.text = "Affinity for creatures (This spell costs {1} less to cast for each creature you control.)\nFlying, deathtouch\nInstant and sorcery spells you cast have affinity for creatures.".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView};
    use crate::basics::Color;
    use crate::cards::{grp, mana_cost, starter_db};
    use crate::ids::PlayerId;
    use crate::priority::{Engine, TargetCtx};
    use crate::state::GameState;
    use std::sync::Arc;

    fn db_with_card() -> CardDb {
        let mut db = starter_db();
        register(&mut db);
        db
    }

    /// `effective_cast_cost` never asks the agent — a placeholder seat is all these cost tests need.
    #[derive(Clone)]
    struct Passer;
    impl Agent for Passer {
        fn decide(&mut self, _v: &PlayerView, _r: &DecisionRequest) -> DecisionResponse {
            DecisionResponse::Pass
        }
    }

    #[test]
    fn witherbloom_shape() {
        let db = db_with_card();
        let def = db.get(WITHERBLOOM_THE_BALANCER).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Creature]);
        assert_eq!(def.chars.supertypes, vec![Supertype::Legendary]);
        assert_eq!(def.chars.colors, vec![Color::Black, Color::Green]);
        assert_eq!(def.chars.keywords, vec![Keyword::Flying, Keyword::Deathtouch]);
        assert_eq!((def.chars.power, def.chars.toughness), (Some(5), Some(5)));
        assert!(def.fully_implemented);
        assert!(matches!(def.abilities[0], Ability::CostReduction { .. }));
        assert!(matches!(def.abilities[1], Ability::GrantCostReduction { .. }));
    }

    /// Build a game with `n_bears` Grizzly Bears on P0's battlefield; return the engine.
    fn engine_with_bears(n_bears: usize) -> Engine {
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(db_with_card()));
        for _ in 0..n_bears {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield);
        }
        state.active_player = PlayerId(0);
        Engine::new(state, vec![Box::new(Passer), Box::new(Passer)])
    }

    /// Own affinity: casting Witherbloom costs {1} less generic per OTHER creature you control (it's
    /// not on the battlefield yet, so it doesn't count itself).
    #[test]
    fn own_affinity_reduces_generic_per_creature() {
        let mut e = engine_with_bears(3);
        let witherbloom = {
            let c = e.state.card_db().get(WITHERBLOOM_THE_BALANCER).unwrap().chars.clone();
            e.state.add_card(PlayerId(0), c, Zone::Hand)
        };
        let base = mana_cost(6, &[(Color::Black, 1), (Color::Green, 1)]);
        let cost = e.effective_cast_cost(PlayerId(0), witherbloom, &base, TargetCtx::Optimistic);
        assert_eq!(cost.generic, 3, "{{6}} − 3 creatures = {{3}}");
        assert_eq!(cost.colored.get(&Color::Black), Some(&1), "coloured pips untouched (CR 702.40)");
        assert_eq!(cost.colored.get(&Color::Green), Some(&1));
    }

    /// Own affinity never takes the cost below {0} generic (CR 118.7): with 8 creatures out, a {6}
    /// generic floors at {0}, coloured pips still required.
    #[test]
    fn own_affinity_floors_at_zero_generic() {
        let mut e = engine_with_bears(8);
        let witherbloom = {
            let c = e.state.card_db().get(WITHERBLOOM_THE_BALANCER).unwrap().chars.clone();
            e.state.add_card(PlayerId(0), c, Zone::Hand)
        };
        let base = mana_cost(6, &[(Color::Black, 1), (Color::Green, 1)]);
        let cost = e.effective_cast_cost(PlayerId(0), witherbloom, &base, TargetCtx::Optimistic);
        assert_eq!(cost.generic, 0, "floored at {{0}}");
        assert_eq!(cost.colored.get(&Color::Black), Some(&1), "still need {{B}}{{G}}");
    }

    /// Granted affinity: an instant/sorcery you cast while Witherbloom is out costs {1} less per
    /// creature you control — and Witherbloom counts itself (it IS a creature on the battlefield).
    #[test]
    fn granted_affinity_reduces_your_instant() {
        let mut e = engine_with_bears(2); // 2 bears …
        let witherbloom = {
            let c = e.state.card_db().get(WITHERBLOOM_THE_BALANCER).unwrap().chars.clone();
            e.state.add_card(PlayerId(0), c, Zone::Battlefield) // … + Witherbloom = 3 creatures
        };
        // A generic-heavy instant to see the reduction (Lightning Bolt is {R}, no generic to shave).
        let bolt = {
            let c = e.state.card_db().get(grp::LIGHTNING_BOLT).unwrap().chars.clone();
            e.state.add_card(PlayerId(0), c, Zone::Hand)
        };
        let base = mana_cost(5, &[(Color::Red, 1)]); // pretend a {5}{R} instant
        let cost = e.effective_cast_cost(PlayerId(0), bolt, &base, TargetCtx::Optimistic);
        assert_eq!(cost.generic, 2, "{{5}} − 3 creatures (2 bears + Witherbloom) = {{2}}");
        assert_eq!(cost.colored.get(&Color::Red), Some(&1), "the {{R}} pip is untouched");
        assert!(e.state.player(PlayerId(0)).battlefield.contains(&witherbloom));
    }

    /// The granted affinity is scoped by filter: it does NOT reduce a creature (non-I/S) spell you cast.
    #[test]
    fn granted_affinity_skips_non_instant_sorcery() {
        let mut e = engine_with_bears(2);
        {
            let c = e.state.card_db().get(WITHERBLOOM_THE_BALANCER).unwrap().chars.clone();
            e.state.add_card(PlayerId(0), c, Zone::Battlefield);
        }
        // Another Grizzly Bears cast (a creature spell) — the I/S-scoped grant must not touch it.
        let bears = {
            let c = e.state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            e.state.add_card(PlayerId(0), c, Zone::Hand)
        };
        let base = mana_cost(5, &[(Color::Green, 1)]);
        let cost = e.effective_cast_cost(PlayerId(0), bears, &base, TargetCtx::Optimistic);
        assert_eq!(cost.generic, 5, "a creature spell gets no affinity from the I/S-scoped grant");
    }

    /// The grant is "spells YOU cast": an opponent's instant gets no reduction from your Witherbloom.
    #[test]
    fn granted_affinity_only_helps_its_controller() {
        let mut e = engine_with_bears(2);
        {
            let c = e.state.card_db().get(WITHERBLOOM_THE_BALANCER).unwrap().chars.clone();
            e.state.add_card(PlayerId(0), c, Zone::Battlefield);
        }
        let opp_bolt = {
            let c = e.state.card_db().get(grp::LIGHTNING_BOLT).unwrap().chars.clone();
            e.state.add_card(PlayerId(1), c, Zone::Hand)
        };
        let base = mana_cost(5, &[(Color::Red, 1)]);
        // P1 (the opponent) casts it — only P1's own battlefield is scanned, which has no Witherbloom.
        let cost = e.effective_cast_cost(PlayerId(1), opp_bolt, &base, TargetCtx::Optimistic);
        assert_eq!(cost.generic, 5, "the opponent's spell is not helped by your Witherbloom");
    }
}
