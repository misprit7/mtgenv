//! Aberrant Manawurm — `{3}{G}` Creature — Wurm 2/5 (first printed SOS).
//!
//! Oracle: "Trample / Whenever you cast an instant or sorcery spell, this creature gets +X/+0 until
//! end of turn, where X is the amount of mana spent to cast that spell."
//!
//! **Fully implemented** — printed Trample + an Opus-style cast-trigger whose pump scales with
//! `ValueExpr::ManaSpentOnTrigger` (the mana spent on the *triggering* spell), +0 toughness, until
//! end of turn. Same primitives as the authored Opus pumps.

use crate::basics::Color;
use crate::cards::helpers::instant_or_sorcery;
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern, Keyword};
use crate::effects::condition::Duration;
use crate::effects::value::ValueExpr;
use crate::effects::{Effect, EffectTarget};
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const ABERRANT_MANAWURM: u32 = 337;

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        ABERRANT_MANAWURM,
        "Aberrant Manawurm",
        &[CreatureType::Wurm],
        Color::Green,
        mana_cost(3, &[(Color::Green, 1)]),
        2,
        5,
        vec![Ability::Triggered {
            event: EventPattern::SpellCast(instant_or_sorcery()),
            condition: None,
            intervening_if: false,
            effect: Effect::PumpPT {
                what: EffectTarget::SourceSelf,
                power: ValueExpr::ManaSpentOnTrigger,
                toughness: ValueExpr::Fixed(0),
                duration: Duration::UntilEndOfTurn,
            },
        }],
    );
    def.chars.keywords = vec![Keyword::Trample];
    def.text = "Trample\nWhenever you cast an instant or sorcery spell, this creature gets +X/+0 until end of turn, where X is the amount of mana spent to cast that spell.".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, CastVariant, DecisionRequest, DecisionResponse, PlayerView};
    use crate::basics::{Target, Zone};
    use crate::cards::{grp, starter_db};
    use crate::ids::PlayerId;
    use crate::priority::Engine;
    use crate::state::GameState;
    use std::sync::Arc;

    /// Aims any targeted spell at the opponent's face; passes otherwise.
    #[derive(Clone)]
    struct BoltFace;
    impl Agent for BoltFace {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::ChooseTargets { slots, .. } => {
                    let i = slots[0]
                        .legal
                        .iter()
                        .position(|t| matches!(t, Target::Player(p) if *p == PlayerId(1)))
                        .unwrap_or(0);
                    DecisionResponse::Pairs(vec![(0, i as u32)])
                }
                _ => DecisionResponse::Pass,
            }
        }
    }

    #[test]
    fn aberrant_manawurm_shape() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(ABERRANT_MANAWURM).unwrap();
        assert_eq!(def.chars.keywords, vec![Keyword::Trample]);
        assert_eq!((def.chars.power, def.chars.toughness), (Some(2), Some(5)));
        assert!(def.fully_implemented);
    }

    /// Real cast path: casting a 1-mana instant (Lightning Bolt, `{R}`, at the opponent's face) pumps
    /// the Wurm by the mana spent on that spell (+1/+0 → 3/5 until end of turn).
    #[test]
    fn cast_trigger_pumps_by_mana_spent() {
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(starter_db()));
        let wurm = {
            let c = state.card_db().get(ABERRANT_MANAWURM).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        let bolt = {
            let c = state.card_db().get(grp::LIGHTNING_BOLT).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Hand)
        };
        {
            let c = state.card_db().get(grp::MOUNTAIN).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield); // pays {R}
        }
        let mut e = Engine::new(state, vec![Box::new(BoltFace), Box::new(BoltFace)]);
        e.cast_spell(PlayerId(0), bolt, CastVariant::Normal);
        e.run_agenda();
        while !e.state.stack.items.is_empty() {
            e.resolve_top();
            e.run_agenda();
        }
        assert_eq!(
            e.state.computed(wurm).power,
            Some(3),
            "cast an instant with 1 mana spent → +1/+0 → 3/5"
        );
    }
}
