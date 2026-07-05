//! Ajani's Response — `{4}{W}` Instant (first printed SOS).
//!
//! Oracle: "This spell costs {3} less to cast if it targets a tapped creature. Destroy target
//! creature."
//!
//! **Fully implemented** — the lander for **target-dependent cost reduction** (the S12 sub-cap):
//! an `Ability::CostReduction { amount: Generic(3), condition: TargetMatches(Tapped) }` alongside a
//! plain `Destroy` of one target creature. Because the discount depends on the *chosen* target (CR
//! 601.2f), the cost is finalized after targets are chosen: the offer gate applies the reduction
//! optimistically (offered iff a tapped creature could be targeted) and `cast_spell` constrains the
//! target choice so the caster can always pay — targeting an untapped creature costs the full {4}{W},
//! so it isn't offered when only the reduced {1}{W} is affordable (no rewind).

use crate::basics::{CardType, Color};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::ability::{
    Ability, CostReductionAmount, CostReductionCondition, CostReductionScope,
};
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::{Effect, EffectTarget};

/// grp id (per-set ids live near their cards).
pub const AJANIS_RESPONSE: u32 = 359;

pub fn register(db: &mut CardDb) {
    let effect = Effect::Destroy {
        what: EffectTarget::Target(TargetSpec {
            kind: TargetKind::Creature(CardFilter::Any),
            min: 1,
            max: 1,
            distinct: true,
        }),
    };
    let mut def = spell(
        AJANIS_RESPONSE,
        "Ajani's Response",
        CardType::Instant,
        Color::White,
        mana_cost(4, &[(Color::White, 1)]),
        effect,
    )
    .with_text("This spell costs {3} less to cast if it targets a tapped creature.\nDestroy target creature.");
    // "This spell costs {3} less to cast if it targets a tapped creature." (CR 601.2f — the discount
    // is settled on the chosen target, so the condition reads the spell's targets.)
    def.abilities.push(Ability::CostReduction {
        amount: CostReductionAmount::Generic(3),
        condition: CostReductionCondition::TargetMatches(CardFilter::Tapped),
        scope: CostReductionScope::Cast,
    });
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, CastVariant, DecisionRequest, DecisionResponse, PlayerView, RandomAgent};
    use crate::basics::{Phase, Target, Zone};
    use crate::cards::{build_game, grp};
    use crate::ids::{ObjId, PlayerId};
    use crate::priority::{Engine, TargetCtx};
    use expect_test::expect;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[test]
    fn ajanis_response_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(AJANIS_RESPONSE).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Instant]);
        assert!(def.fully_implemented);
        assert!(matches!(&def.abilities[1], Ability::CostReduction { .. }));
        expect![[r#"
            Destroy {
                what: Target(
                    TargetSpec {
                        kind: Creature(
                            Any,
                        ),
                        min: 1,
                        max: 1,
                        distinct: true,
                    },
                ),
            }"#]]
        .assert_eq(&format!("{:#?}", def.spell_effect().unwrap()));
    }

    /// Put Ajani's Response in P0's hand, `n_lands` untapped Plains+Island pairs of mana, plus a
    /// tapped and an untapped creature on the battlefield; return `(state, ajani, tapped, untapped)`.
    fn setup(lands: &[u32]) -> (crate::state::GameState, ObjId, ObjId, ObjId) {
        let mut state = build_game(1, &[&[], &[]]);
        let ajani = state.add_card(
            PlayerId(0),
            state.card_db().get(AJANIS_RESPONSE).unwrap().chars.clone(),
            Zone::Hand,
        );
        for &grp_id in lands {
            let c = state.card_db().get(grp_id).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield);
        }
        let mut mk = |state: &mut crate::state::GameState, tapped: bool| {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            let id = state.add_card(PlayerId(1), c, Zone::Battlefield);
            state.objects.get_mut(&id).unwrap().status.tapped = tapped;
            id
        };
        let tapped = mk(&mut state, true);
        let untapped = mk(&mut state, false);
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        (state, ajani, tapped, untapped)
    }

    /// The reduction applies iff a **chosen** target is tapped, and optimistically iff a tapped
    /// creature *could* be targeted. Targeting an untapped creature pays the full {4}{W}.
    #[test]
    fn reduces_only_when_targeting_a_tapped_creature() {
        let (state, ajani, tapped, untapped) = setup(&[]);
        let base = state.object(ajani).chars.mana_cost.clone().unwrap();
        let e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);

        let chosen = |t: Target| {
            e.effective_cast_cost(PlayerId(0), ajani, &base, TargetCtx::Chosen(&[t])).generic
        };
        assert_eq!(chosen(Target::Object(tapped)), 1, "targets a tapped creature → {{3}} off → {{1}}");
        assert_eq!(chosen(Target::Object(untapped)), 4, "targets an untapped creature → no discount");

        // Optimistic: a tapped creature exists on the board → the offer gate sees the discount.
        assert_eq!(
            e.effective_cast_cost(PlayerId(0), ajani, &base, TargetCtx::Optimistic).generic,
            1,
            "a tapped creature is targetable → optimistic {{1}}"
        );
    }

    /// Offer gate: with only {1}{U} of mana (enough for the reduced {1}{W}, not the full {4}{W}),
    /// Ajani's Response is castable only when a tapped creature exists to target.
    #[test]
    fn offered_only_when_a_tapped_target_makes_it_affordable() {
        use crate::agent::PlayableAction;
        let offered = |tapped: bool| {
            let mut state = build_game(1, &[&[], &[]]);
            state.add_card(
                PlayerId(0),
                state.card_db().get(AJANIS_RESPONSE).unwrap().chars.clone(),
                Zone::Hand,
            );
            // Exactly {1}{W}: a Plains + an Island.
            for grp_id in [grp::PLAINS, grp::ISLAND] {
                let c = state.card_db().get(grp_id).unwrap().chars.clone();
                state.add_card(PlayerId(0), c, Zone::Battlefield);
            }
            // One creature to target, tapped or not.
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            let cr = state.add_card(PlayerId(1), c, Zone::Battlefield);
            state.objects.get_mut(&cr).unwrap().status.tapped = tapped;
            state.active_player = PlayerId(0);
            state.phase = Phase::PrecombatMain;
            let e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
            e.legal_actions(PlayerId(0))
                .iter()
                .any(|a| matches!(a, PlayableAction::Cast { .. }))
        };
        assert!(offered(true), "a tapped creature → castable via the {{3}} reduction");
        assert!(!offered(false), "only an untapped creature → full {{4}}{{U}} unaffordable → not offered");
    }

    /// An agent that records the ChooseTargets candidate set it's shown and targets the object whose
    /// id is `pick` (falling back to slot 0). Lets the test assert which targets the engine offered.
    struct CaptureAgent {
        seen: Rc<RefCell<Vec<Target>>>,
        pick: ObjId,
    }
    impl Agent for CaptureAgent {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::ChooseTargets { slots, .. } => {
                    let legal = &slots[0].legal;
                    *self.seen.borrow_mut() = legal.clone();
                    let idx = legal
                        .iter()
                        .position(|t| *t == Target::Object(self.pick))
                        .unwrap_or(0);
                    DecisionResponse::Pairs(vec![(0, idx as u32)])
                }
                _ => DecisionResponse::Pass,
            }
        }
    }

    /// Real cast path with only {1}{W}: the engine offers **only the tapped creature** as a target
    /// (targeting the untapped one would cost the full {4}{W}, which the caster can't pay — the
    /// no-rewind constraint), then the cast pays the reduced {1}{W} and destroys the tapped creature.
    #[test]
    fn constrains_targets_to_the_affordable_tapped_creature() {
        // Exactly {1}{W}: a Plains + an Island (both untapped).
        let (state, ajani, tapped, untapped) = setup(&[grp::PLAINS, grp::ISLAND]);
        let seen = Rc::new(RefCell::new(Vec::new()));
        let mut e = Engine::new(
            state,
            vec![
                Box::new(CaptureAgent { seen: seen.clone(), pick: tapped }),
                Box::new(RandomAgent::new(1)),
            ],
        );

        e.cast_spell(PlayerId(0), ajani, CastVariant::Normal);
        // The offered target set excluded the unaffordable untapped creature.
        let offered = seen.borrow().clone();
        assert_eq!(offered, vec![Target::Object(tapped)], "only the affordable tapped creature offered");
        assert!(!offered.contains(&Target::Object(untapped)), "untapped creature not offered");
        // Both lands were tapped to pay the reduced {1}{W} (2 mana).
        let tapped_lands = e
            .state
            .player(PlayerId(0))
            .battlefield
            .iter()
            .filter(|&&id| e.state.object(id).chars.is_land() && e.state.object(id).status.tapped)
            .count();
        assert_eq!(tapped_lands, 2, "paid {{1}}{{W}} = 2 mana");

        e.resolve_top();
        assert!(
            !e.state.player(PlayerId(1)).battlefield.contains(&tapped),
            "the tapped creature was destroyed"
        );
    }
}
