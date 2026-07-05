//! Biblioplex Tomekeeper — `{4}` Artifact Creature — Construct 3/4 (first printed SOS).
//!
//! Oracle: "When this creature enters, choose up to one —
//!  • Target creature becomes prepared. (Only creatures with prepare spells can become prepared.)
//!  • Target creature becomes unprepared."
//!
//! **Fully implemented** — the lander for **modal *triggered* abilities** (CR 603.3d / 700.2): its ETB
//! trigger's effect is an `Effect::Modal` (`min: 0, max: 1`), and the trigger-placement path now
//! chooses the mode as the trigger goes on the stack and collects ONLY the chosen mode's target
//! (mirroring the modal-SPELL cast path; the chosen mode rides on the stack object's `modes` and is
//! threaded into the resolution ctx). Each mode is a targeted `Effect::SetPrepared` (prepare / unprepare).

use crate::basics::{CardType, Color};
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern};
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::{Effect, EffectTarget, Mode};
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const BIBLIOPLEX_TOMEKEEPER: u32 = 450;

/// "target creature becomes prepared / unprepared" — the two modes share this target shape.
fn set_prepared_mode(label: &str, prepared: bool) -> Mode {
    Mode {
        label: label.to_string(),
        effect: Effect::SetPrepared {
            what: EffectTarget::Target(TargetSpec {
                kind: TargetKind::Creature(CardFilter::Any),
                min: 1,
                max: 1,
                distinct: true,
            }),
            prepared,
        },
    }
}

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        BIBLIOPLEX_TOMEKEEPER,
        "Biblioplex Tomekeeper",
        &[CreatureType::Construct],
        Color::White, // placeholder — cleared to colorless below
        mana_cost(4, &[]),
        3,
        4,
        vec![Ability::Triggered {
            event: EventPattern::SelfEnters,
            condition: None,
            intervening_if: false,
            // "choose up to one — • prepare a target • unprepare a target"
            effect: Effect::Modal {
                modes: vec![
                    set_prepared_mode("Target creature becomes prepared.", true),
                    set_prepared_mode("Target creature becomes unprepared.", false),
                ],
                min: 0,
                max: 1,
                allow_repeat: false,
            },
        }],
    );
    def.chars.card_types = vec![CardType::Artifact, CardType::Creature];
    def.chars.colors = Vec::new(); // colorless artifact
    def.text = "When this creature enters, choose up to one —\n• Target creature becomes prepared. (Only creatures with prepare spells can become prepared.)\n• Target creature becomes unprepared.".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, CastVariant, DecisionRequest, DecisionResponse, PlayerView};
    use crate::basics::{Phase, Target, Zone};
    use crate::cards::sos::emeritus_of_truce::EMERITUS_OF_TRUCE;
    use crate::cards::{grp, starter_db};
    use crate::ids::{ObjId, PlayerId};
    use crate::priority::Engine;
    use crate::state::GameState;
    use std::sync::Arc;

    #[test]
    fn biblioplex_tomekeeper_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(BIBLIOPLEX_TOMEKEEPER).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Artifact, CardType::Creature]);
        assert_eq!((def.chars.power, def.chars.toughness), (Some(3), Some(4)));
        assert!(matches!(
            &def.abilities[0],
            Ability::Triggered { event: EventPattern::SelfEnters, effect: Effect::Modal { min: 0, max: 1, .. }, .. }
        ));
        assert!(def.fully_implemented);
    }

    /// Chooses mode `mode` (prepare/unprepare) and targets `want`.
    #[derive(Clone)]
    struct ModeAgent {
        mode: u32,
        want: ObjId,
    }
    impl Agent for ModeAgent {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::ChooseModes { .. } => DecisionResponse::Indices(vec![self.mode]),
                DecisionRequest::ChooseTargets { slots, .. } => {
                    let i = slots[0].legal.iter().position(|t| matches!(t, Target::Object(o) if *o == self.want)).unwrap_or(0);
                    DecisionResponse::Pairs(vec![(0, i as u32)])
                }
                _ => DecisionResponse::Pass,
            }
        }
    }

    fn setup() -> (GameState, ObjId, ObjId) {
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(starter_db()));
        let tome = state.add_card(PlayerId(0), state.card_db().get(BIBLIOPLEX_TOMEKEEPER).unwrap().chars.clone(), Zone::Hand);
        for _ in 0..4 {
            state.add_card(PlayerId(0), state.card_db().get(grp::PLAINS).unwrap().chars.clone(), Zone::Battlefield); // {4}
        }
        let emeritus = state.add_card(PlayerId(0), state.card_db().get(EMERITUS_OF_TRUCE).unwrap().chars.clone(), Zone::Battlefield);
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        (state, tome, emeritus)
    }

    /// Mode 0 via the REAL cast + ETB trigger: the modal trigger chooses "prepare" and targets the
    /// Emeritus of Truce (a Prepare DFC) → it becomes prepared.
    #[test]
    fn etb_modal_prepares_a_target() {
        let (state, tome, emeritus) = setup();
        let mut e = Engine::new(state, vec![Box::new(ModeAgent { mode: 0, want: emeritus }), Box::new(ModeAgent { mode: 0, want: emeritus })]);
        e.cast_spell(PlayerId(0), tome, CastVariant::Normal);
        e.resolve_top(); // Tomekeeper enters, ETB trigger queues
        e.run_agenda(); // trigger placed: choose the mode + target
        e.resolve_top(); // resolve the chosen mode
        assert!(e.state.object(emeritus).prepared, "the ETB's chosen mode prepared the target");
    }

    /// Mode 1 (unprepare): an already-prepared creature is made unprepared by the second mode.
    #[test]
    fn etb_modal_unprepares_a_target() {
        let (mut state, tome, emeritus) = setup();
        state.objects.get_mut(&emeritus).unwrap().prepared = true; // start prepared
        let mut e = Engine::new(state, vec![Box::new(ModeAgent { mode: 1, want: emeritus }), Box::new(ModeAgent { mode: 1, want: emeritus })]);
        e.cast_spell(PlayerId(0), tome, CastVariant::Normal);
        e.resolve_top();
        e.run_agenda();
        e.resolve_top();
        assert!(!e.state.object(emeritus).prepared, "the second mode unprepared the target");
    }
}
