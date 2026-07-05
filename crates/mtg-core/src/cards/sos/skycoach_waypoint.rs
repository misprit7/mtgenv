//! Skycoach Waypoint — Land (first printed SOS).
//!
//! Oracle:
//!   {T}: Add {C}.
//!   {3}, {T}: Target creature becomes prepared. (Only creatures with prepare spells can become
//!   prepared.)
//!
//! **Fully implemented** — a colorless utility land: an intrinsic `{T}: Add {C}` mana ability plus a
//! `{3}, {T}` activated ability that makes a **target** creature prepared via `Effect::SetPrepared`
//! (the targeted analogue of `Effect::BecomePrepared`). Setting the status on a creature with no
//! prepare spell is inert, matching the reminder text.

use crate::basics::{CardType, Color};
use crate::cards::{mana_ability, mana_cost, CardDb, CardDef};
use crate::effects::ability::{Ability, Cost, CostComponent, Timing};
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::{Effect, EffectTarget};
use crate::state::Characteristics;

/// grp id (per-set ids live near their cards).
pub const SKYCOACH_WAYPOINT: u32 = 449;

pub fn register(db: &mut CardDb) {
    let chars = Characteristics {
        name: "Skycoach Waypoint".to_string(),
        card_types: vec![CardType::Land],
        grp_id: SKYCOACH_WAYPOINT,
        ..Default::default()
    };
    db.insert(CardDef {
        chars,
        abilities: vec![
            // "{T}: Add {C}."
            mana_ability(Color::Colorless),
            // "{3}, {T}: Target creature becomes prepared."
            Ability::Activated {
                cost: Cost {
                    mana: Some(mana_cost(3, &[])),
                    components: vec![CostComponent::TapSelf],
                },
                effect: Effect::SetPrepared {
                    what: EffectTarget::Target(TargetSpec {
                        kind: TargetKind::Creature(CardFilter::Any),
                        min: 1,
                        max: 1,
                        distinct: true,
                    }),
                    prepared: true,
                },
                timing: Timing::Instant,
                restriction: None,
                is_mana: false,
            },
        ],
        text: "{T}: Add {C}.\n{3}, {T}: Target creature becomes prepared. (Only creatures with prepare spells can become prepared.)".to_string(),
        fully_implemented: true,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{AbilityRef, Agent, DecisionRequest, DecisionResponse, PlayerView};
    use crate::basics::Zone;
    use crate::cards::sos::emeritus_of_truce::EMERITUS_OF_TRUCE;
    use crate::cards::{grp, starter_db};
    use crate::ids::PlayerId;
    use crate::priority::Engine;
    use crate::state::GameState;
    use std::sync::Arc;

    #[test]
    fn skycoach_waypoint_shape() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(SKYCOACH_WAYPOINT).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Land]);
        assert!(def.is_mana_source(), "the {{T}}: Add {{C}} mana ability");
        assert!(matches!(&def.abilities[1], Ability::Activated { effect: Effect::SetPrepared { prepared: true, .. }, is_mana: false, .. }));
        assert!(def.fully_implemented);
    }

    /// Picks the offered target (slot 0, cand 0), accepts confirms.
    #[derive(Clone)]
    struct PlayAgent;
    impl Agent for PlayAgent {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::ChooseTargets { .. } => DecisionResponse::Pairs(vec![(0, 0)]),
                DecisionRequest::Confirm { .. } => DecisionResponse::Bool(true),
                _ => DecisionResponse::Pass,
            }
        }
    }

    /// The REAL activate path: activate `{3},{T}` targeting an Emeritus of Truce (a Prepare DFC) → it
    /// becomes prepared, so its controller may then cast a copy of its back-face spell.
    #[test]
    fn makes_a_prepare_creature_prepared_via_full_activation() {
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(starter_db()));
        let waypoint = {
            let c = state.card_db().get(SKYCOACH_WAYPOINT).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        // {3}: three more mana sources (basic lands) to pay the activation.
        for _ in 0..3 {
            let c = state.card_db().get(grp::FOREST).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield);
        }
        let emeritus = {
            let c = state.card_db().get(EMERITUS_OF_TRUCE).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        let mut e = Engine::new(state, vec![Box::new(PlayAgent), Box::new(PlayAgent)]);

        assert!(!e.state.object(emeritus).prepared, "not prepared before the ability");
        e.activate_ability(PlayerId(0), waypoint, AbilityRef(1)); // pays {3} + {T}, targets the Emeritus
        e.resolve_top();
        assert!(e.state.object(emeritus).prepared, "the targeted Prepare creature became prepared");
        // And the mana source was tapped as a cost.
        assert!(e.state.object(waypoint).status.tapped, "Skycoach Waypoint tapped for the cost");
    }
}
