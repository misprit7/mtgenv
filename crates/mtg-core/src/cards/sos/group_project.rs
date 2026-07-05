//! Group Project — `{1}{W}` Sorcery (first printed SOS).
//!
//! Oracle: "Create a 2/2 red and white Spirit creature token. Flashback—Tap three untapped
//! creatures you control. (You may cast this card from your graveyard for its flashback cost. Then
//! exile it.)"
//!
//! **Fully implemented** — the lander for a **non-mana flashback cost** (CR 702.34). The spell is a
//! plain `CreateToken` of the shared [`helpers::spirit_token`]; its flashback cost is `{T}`-free —
//! `CostComponent::TapCreatures(3)` (the count-based tap-others cost) — paid through the real cost
//! machinery when cast from the graveyard, then Group Project is exiled as it leaves the stack.

use crate::basics::{CardType, Color};
use crate::cards::{helpers, mana_cost, spell, CardDb};
use crate::effects::ability::{Ability, Cost, CostComponent};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;

/// grp id (per-set ids live near their cards).
pub const GROUP_PROJECT: u32 = 417;

pub fn register(db: &mut CardDb) {
    let mut def = spell(
        GROUP_PROJECT,
        "Group Project",
        CardType::Sorcery,
        Color::White,
        mana_cost(1, &[(Color::White, 1)]),
        Effect::CreateToken {
            spec: helpers::spirit_token(),
            count: ValueExpr::Fixed(1),
            controller: PlayerRef::Controller,
            dynamic_counters: vec![],
        },
    )
    .with_text("Create a 2/2 red and white Spirit creature token.\nFlashback—Tap three untapped creatures you control. (You may cast this card from your graveyard for its flashback cost. Then exile it.)");
    // Flashback with a NON-mana cost: tap three untapped creatures you control (CR 702.34).
    def.abilities.push(Ability::Flashback {
        cost: Cost { mana: None, components: vec![CostComponent::TapCreatures(3)] },
    });
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, CastVariant, DecisionRequest, DecisionResponse, PlayableAction, PlayerView, RandomAgent};
    use crate::basics::{Phase, Zone};
    use crate::cards::{build_game, grp};
    use crate::ids::{ObjId, PlayerId};
    use crate::priority::Engine;

    #[test]
    fn group_project_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(GROUP_PROJECT).unwrap();
        assert!(matches!(def.spell_effect(), Some(Effect::CreateToken { .. })));
        let fb = def.abilities.iter().find_map(|a| match a {
            Ability::Flashback { cost } => Some(cost),
            _ => None,
        });
        let fb = fb.expect("a flashback ability");
        assert!(fb.mana.is_none(), "flashback cost is non-mana");
        assert!(matches!(fb.components[0], CostComponent::TapCreatures(3)));
    }

    /// Put Group Project in P0's graveyard with `creatures` untapped Grizzly Bears. Returns
    /// `(engine, group_project, bear_ids)`.
    fn setup(creatures: usize, agent: Box<dyn Agent>) -> (Engine, ObjId, Vec<ObjId>) {
        let mut state = build_game(1, &[&[], &[]]);
        let gp = state.add_card(
            PlayerId(0),
            state.card_db().get(GROUP_PROJECT).unwrap().chars.clone(),
            Zone::Graveyard,
        );
        let mut bears = Vec::new();
        for _ in 0..creatures {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            bears.push(state.add_card(PlayerId(0), c, Zone::Battlefield));
        }
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let e = Engine::new(state, vec![agent, Box::new(RandomAgent::new(1))]);
        (e, gp, bears)
    }

    /// Offer gate: flashback is offered only when three untapped creatures can pay the tap-three cost.
    #[test]
    fn flashback_offered_only_with_three_creatures() {
        let offered = |creatures: usize| {
            let (e, gp, _) = setup(creatures, Box::new(RandomAgent::new(0)));
            e.legal_actions(PlayerId(0))
                .iter()
                .any(|a| matches!(a, PlayableAction::Cast { spell, variant: CastVariant::Flashback } if *spell == gp))
        };
        assert!(!offered(2), "only 2 creatures → tap-three unpayable → flashback not offered");
        assert!(offered(3), "3 creatures → flashback offered");
    }

    /// An agent that taps the first three offered creatures (SelectCards), else passes.
    #[derive(Clone)]
    struct TapAgent;
    impl Agent for TapAgent {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::SelectCards { min, max, from, .. } => {
                    let n = (*min).max(3).min(*max).min(from.len() as u32);
                    DecisionResponse::Indices((0..n).collect())
                }
                _ => DecisionResponse::Pass,
            }
        }
    }

    /// Real flashback cast: tapping three creatures pays the cost, a Spirit token is created, and
    /// Group Project is exiled (not returned to the graveyard) as it leaves the stack.
    #[test]
    fn flashback_taps_three_and_exiles() {
        let (mut e, gp, bears) = setup(3, Box::new(TapAgent));
        e.cast_spell(PlayerId(0), gp, CastVariant::Flashback);
        // The three creatures were tapped to pay the cost.
        assert_eq!(
            bears.iter().filter(|&&b| e.state.object(b).status.tapped).count(),
            3,
            "all three creatures tapped for the flashback cost"
        );
        e.resolve_top();
        // A Spirit token was created.
        assert!(
            e.state
                .player(PlayerId(0))
                .battlefield
                .iter()
                .any(|&id| e.state.object(id).chars.name == "Spirit"),
            "a Spirit token was created"
        );
        // Group Project was exiled (flashback), not put back in the graveyard.
        assert!(e.state.player(PlayerId(0)).exile.contains(&gp), "Group Project exiled after flashback");
        assert!(!e.state.player(PlayerId(0)).graveyard.contains(&gp), "…not in the graveyard");
    }
}
