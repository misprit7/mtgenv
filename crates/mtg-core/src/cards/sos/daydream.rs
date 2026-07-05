//! Daydream — `{W}` Sorcery (first printed SOS).
//!
//! Oracle: "Exile target creature you control, then return that card to the battlefield under its
//! owner's control with a +1/+1 counter on it. Flashback {2}{W}."
//!
//! **Fully implemented** — NO new engine cap: a blink (`Effect::Blink`, CR 603.6e — exile then
//! return, ETB re-fires, keeping the object id) followed by `PutCounters{ ChosenIndex(0) }` on the
//! returned creature (the blink reuses the object id, so the locked target still points at it), plus
//! a mana `Flashback` (the shared `cards::flashback` helper).

use crate::basics::{CardType, Color, CounterKind};
use crate::cards::{flashback, mana_cost, spell, CardDb};
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};

/// grp id (per-set ids live near their cards).
pub const DAYDREAM: u32 = 419;

pub fn register(db: &mut CardDb) {
    let effect = Effect::Sequence(vec![
        Effect::Blink {
            what: EffectTarget::Target(TargetSpec {
                kind: TargetKind::Creature(CardFilter::ControlledBy(PlayerRef::Controller)),
                min: 1,
                max: 1,
                distinct: true,
            }),
        },
        // "with a +1/+1 counter on it" — the blinked creature keeps its object id, so the locked
        // target (index 0) still names it after it returns.
        Effect::PutCounters {
            what: EffectTarget::ChosenIndex(0),
            kind: CounterKind::PlusOnePlusOne,
            n: ValueExpr::Fixed(1),
        },
    ]);
    let mut def = spell(
        DAYDREAM,
        "Daydream",
        CardType::Sorcery,
        Color::White,
        mana_cost(0, &[(Color::White, 1)]),
        effect,
    )
    .with_text("Exile target creature you control, then return that card to the battlefield under its owner's control with a +1/+1 counter on it.\nFlashback {2}{W}");
    def.abilities.push(flashback(mana_cost(2, &[(Color::White, 1)])));
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, CastVariant, DecisionRequest, DecisionResponse, PlayableAction, PlayerView, RandomAgent};
    use crate::basics::{Phase, Target, Zone};
    use crate::cards::{build_game, grp};
    use crate::effects::ability::Ability;
    use crate::ids::{ObjId, PlayerId};
    use crate::priority::Engine;

    #[test]
    fn daydream_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(DAYDREAM).unwrap();
        assert!(matches!(def.spell_effect(), Some(Effect::Sequence(_))));
        assert!(def.abilities.iter().any(|a| matches!(a, Ability::Flashback { .. })));
        assert!(def.fully_implemented);
    }

    /// An agent that targets `pick` at ChooseTargets, else passes.
    struct PickAgent(ObjId);
    impl Agent for PickAgent {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::ChooseTargets { slots, .. } => {
                    let idx = slots[0].legal.iter().position(|t| *t == Target::Object(self.0)).unwrap_or(0);
                    DecisionResponse::Pairs(vec![(0, idx as u32)])
                }
                _ => DecisionResponse::Pass,
            }
        }
    }

    /// Real cast: a 2/2 creature is blinked and returns with a +1/+1 counter (computed 3/3), keeping
    /// its object id.
    #[test]
    fn blinks_and_returns_with_a_counter() {
        let mut state = build_game(1, &[&[], &[]]);
        let dd = state.add_card(
            PlayerId(0),
            state.card_db().get(DAYDREAM).unwrap().chars.clone(),
            Zone::Hand,
        );
        let plains = state.card_db().get(grp::PLAINS).unwrap().chars.clone();
        state.add_card(PlayerId(0), plains, Zone::Battlefield);
        let bears = {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;

        let mut e = Engine::new(state, vec![Box::new(PickAgent(bears)), Box::new(RandomAgent::new(1))]);
        e.cast_spell(PlayerId(0), dd, CastVariant::Normal);
        e.resolve_top();

        assert!(e.state.player(PlayerId(0)).battlefield.contains(&bears), "returned to the battlefield (same id)");
        assert_eq!(
            e.state.object(bears).counters.get(&CounterKind::PlusOnePlusOne),
            1,
            "returned with a +1/+1 counter"
        );
        assert_eq!(e.state.computed(bears).power, Some(3), "2/2 + counter = 3/3");
    }

    /// Flashback is offered from the graveyard for its {2}{W} cost.
    #[test]
    fn flashback_offered_from_graveyard() {
        let mut state = build_game(1, &[&[], &[]]);
        let dd = state.add_card(
            PlayerId(0),
            state.card_db().get(DAYDREAM).unwrap().chars.clone(),
            Zone::Graveyard,
        );
        // {2}{W} = 3 mana; and a creature to target so the cast isn't withheld for lack of a target.
        for _ in 0..3 {
            let c = state.card_db().get(grp::PLAINS).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield);
        }
        let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
        state.add_card(PlayerId(0), c, Zone::Battlefield);
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
        assert!(
            e.legal_actions(PlayerId(0)).iter().any(|a| matches!(
                a,
                PlayableAction::Cast { spell, variant: CastVariant::Flashback } if *spell == dd
            )),
            "flashback offered from the graveyard"
        );
    }
}
