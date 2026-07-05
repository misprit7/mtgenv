//! Zimone's Experiment — `{3}{G}` Sorcery.
//!
//! Oracle: "Look at the top five cards of your library. You may reveal up to two creature and/or land
//! cards from among them, then put the rest on the bottom of your library in a random order. Put all land
//! cards revealed this way onto the battlefield tapped and put all creature cards revealed this way into
//! your hand."
//!
//! **Fully implemented** — a single [`Effect::LookPickCreaturesLands`] (look at 5, take up to 2 creature/
//! land cards routed by type — lands → battlefield tapped, creatures → hand — rest random-bottom).

use crate::basics::{CardType, Color};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::value::ValueExpr;
use crate::effects::Effect;

/// grp id (per-set ids live near their cards).
pub const ZIMONES_EXPERIMENT: u32 = 458;

pub fn register(db: &mut CardDb) {
    let effect = Effect::LookPickCreaturesLands { count: ValueExpr::Fixed(5), take: ValueExpr::Fixed(2) };
    let def = spell(
        ZIMONES_EXPERIMENT,
        "Zimone's Experiment",
        CardType::Sorcery,
        Color::Green,
        mana_cost(3, &[(Color::Green, 1)]),
        effect,
    )
    .with_text("Look at the top five cards of your library. You may reveal up to two creature and/or land cards from among them, then put the rest on the bottom of your library in a random order. Put all land cards revealed this way onto the battlefield tapped and put all creature cards revealed this way into your hand.");
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, CastVariant, DecisionRequest, DecisionResponse, PlayerView, RandomAgent, SelectReason};
    use crate::basics::{Phase, Zone};
    use crate::cards::{grp, starter_db};
    use crate::ids::{ObjId, PlayerId};
    use crate::priority::Engine;
    use crate::state::GameState;
    use std::sync::Arc;

    fn db_with_card() -> CardDb {
        let mut db = starter_db();
        register(&mut db);
        db
    }

    #[test]
    fn zimone_shape() {
        let db = db_with_card();
        let def = db.get(ZIMONES_EXPERIMENT).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Sorcery]);
        assert_eq!(def.chars.colors, vec![Color::Green]);
        assert_eq!(def.chars.mana_value(), 4);
        assert!(matches!(def.spell_effect(), Some(Effect::LookPickCreaturesLands { .. })));
    }

    /// Takes the offered land + creature (by object id); passes otherwise.
    struct PickAgent {
        want: Vec<ObjId>,
    }
    impl Agent for PickAgent {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::SelectCards { reason: SelectReason::ScryStage, from, .. } => {
                    let idxs: Vec<u32> = from
                        .iter()
                        .enumerate()
                        .filter(|(_, o)| self.want.contains(o))
                        .map(|(i, _)| i as u32)
                        .collect();
                    DecisionResponse::Indices(idxs)
                }
                _ => DecisionResponse::Pass,
            }
        }
    }

    /// Real cast: top 5 = [Forest(land), Bears(creature), Bolt, Bolt, Bolt]. Take the Forest and Bears →
    /// Forest enters tapped, Bears to hand; the three Bolts go to the bottom.
    #[test]
    fn routes_land_to_battlefield_creature_to_hand() {
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(db_with_card()));
        for _ in 0..4 {
            let c = state.card_db().get(grp::FOREST).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield);
        }
        let zimone = {
            let c = state.card_db().get(ZIMONES_EXPERIMENT).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Hand)
        };
        // Library bottom→top (add_card appends to the top): three Bolts, then Bears, then Forest on top.
        for _ in 0..3 {
            let c = state.card_db().get(grp::LIGHTNING_BOLT).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Library);
        }
        let bears = {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Library)
        };
        let forest = {
            let c = state.card_db().get(grp::FOREST).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Library)
        };
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let mut e = Engine::new(state, vec![Box::new(PickAgent { want: vec![forest, bears] }), Box::new(RandomAgent::new(1))]);
        e.cast_spell(PlayerId(0), zimone, CastVariant::Normal);
        e.resolve_top();
        assert!(e.state.player(PlayerId(0)).battlefield.contains(&forest), "land entered the battlefield");
        assert!(e.state.object(forest).status.tapped, "…tapped");
        assert!(e.state.player(PlayerId(0)).hand.contains(&bears), "creature went to hand");
        // The 3 Bolts are on the bottom of the library (not taken).
        assert_eq!(e.state.player(PlayerId(0)).library.len(), 3, "three Bolts bottomed");
    }
}
