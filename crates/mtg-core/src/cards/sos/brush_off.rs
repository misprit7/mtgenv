//! Brush Off — `{2}{U}{U}` Instant (first printed SOS).
//!
//! Oracle: "This spell costs {1}{U} less to cast if it targets an instant or sorcery spell.
//! Counter target spell."
//!
//! **Fully implemented** — the first counterspell exercised through the **real cast path**. Two
//! reused subsystems meet here: (1) `Effect::Counter` over a `TargetKind::StackObject(Any)` ("counter
//! target spell"), now that `target_candidates`/`target_matches_filter` enumerate + filter spells on
//! the stack (previously counterspells were only reachable via `resolve_effect` with a hand-built
//! `Target::Stack`); (2) the S12 target-dependent cost reduction (`Ability::CostReduction { amount:
//! Cost({1}{U}), condition: TargetMatches(instant-or-sorcery spell) }`) — the `Cost` (coloured) arm's
//! first card. Because the discount depends on the *chosen* stack target (CR 601.2f), the offer gate
//! applies it optimistically and `cast_spell` constrains the target choice so the caster can always
//! pay: with only {1}{U} up, Brush Off can be aimed only at an instant/sorcery spell (aiming it at a
//! creature spell would cost the full {2}{U}{U} — not offered; no rewind). A spell with
//! `CantBeCountered` (Surrak) is a legal target but is left on the stack at resolution (CR 701.5f).

use crate::basics::{CardType, Color};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::ability::{
    Ability, CostReductionAmount, CostReductionCondition, CostReductionScope,
};
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::{Effect, EffectTarget};

/// grp id (per-set ids live near their cards).
pub const BRUSH_OFF: u32 = 400;

/// "an instant or sorcery spell" — the filter the cost reduction reads against the chosen stack
/// target. Applied to the target's underlying spell card, so it reads the spell's computed types.
fn instant_or_sorcery() -> CardFilter {
    CardFilter::AnyOf(vec![
        CardFilter::HasCardType(CardType::Instant),
        CardFilter::HasCardType(CardType::Sorcery),
    ])
}

pub fn register(db: &mut CardDb) {
    let effect = Effect::Counter {
        what: EffectTarget::Target(TargetSpec {
            kind: TargetKind::StackObject(CardFilter::Any),
            min: 1,
            max: 1,
            distinct: true,
        }),
    };
    let mut def = spell(
        BRUSH_OFF,
        "Brush Off",
        CardType::Instant,
        Color::Blue,
        mana_cost(2, &[(Color::Blue, 2)]),
        effect,
    )
    .with_text(
        "This spell costs {1}{U} less to cast if it targets an instant or sorcery spell.\nCounter target spell.",
    );
    // "This spell costs {1}{U} less to cast if it targets an instant or sorcery spell." (CR 601.2f —
    // the discount is settled on the chosen stack target, so the condition reads the spell's targets.)
    def.abilities.push(Ability::CostReduction {
        amount: CostReductionAmount::Cost(mana_cost(1, &[(Color::Blue, 1)])),
        condition: CostReductionCondition::TargetMatches(instant_or_sorcery()),
        scope: CostReductionScope::Cast,
    });
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{
        Agent, CastVariant, DecisionRequest, DecisionResponse, PlayerView, RandomAgent,
    };
    use crate::basics::{Phase, Target, Zone};
    use crate::cards::{build_game, grp};
    use crate::ids::{ObjId, PlayerId, StackId};
    use crate::priority::{Engine, TargetCtx};
    use crate::stack::{StackObject, StackObjectKind};
    use expect_test::expect;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[test]
    fn brush_off_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(BRUSH_OFF).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Instant]);
        assert!(def.fully_implemented);
        assert!(matches!(&def.abilities[1], Ability::CostReduction { .. }));
        expect![[r#"
            Counter {
                what: Target(
                    TargetSpec {
                        kind: StackObject(
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

    /// Put `card`'s object on the stack as a spell controlled by `controller`; return its `StackId`
    /// (minted so it never collides with a subsequent real cast's `mint_stack`).
    fn put_spell_on_stack(
        state: &mut crate::state::GameState,
        card: ObjId,
        controller: PlayerId,
    ) -> StackId {
        let sid = state.mint_stack();
        state.stack.push(StackObject {
            id: sid,
            controller,
            source: Some(card),
            kind: StackObjectKind::Spell(card),
            targets: vec![],
            x: None,
            modes: Vec::new(),
        });
        sid
    }

    /// Add Brush Off to P0's hand and `n` untapped Islands to P0's battlefield; set the turn.
    fn setup(islands: usize) -> (crate::state::GameState, ObjId) {
        let mut state = build_game(1, &[&[], &[]]);
        let brush = state.add_card(
            PlayerId(0),
            state.card_db().get(BRUSH_OFF).unwrap().chars.clone(),
            Zone::Hand,
        );
        for _ in 0..islands {
            let c = state.card_db().get(grp::ISLAND).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield);
        }
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        (state, brush)
    }

    fn put_on_battlefield_stack(
        state: &mut crate::state::GameState,
        grp_id: u32,
        controller: PlayerId,
    ) -> (ObjId, StackId) {
        let chars = state.card_db().get(grp_id).unwrap().chars.clone();
        let obj = state.add_card(controller, chars, Zone::Stack);
        let sid = put_spell_on_stack(state, obj, controller);
        (obj, sid)
    }

    /// An agent that records the `ChooseTargets` candidate set it's shown and picks slot 0's first
    /// legal target. Lets a test assert exactly which stack targets the engine offered.
    struct CaptureAgent {
        seen: Rc<RefCell<Vec<Target>>>,
    }
    impl Agent for CaptureAgent {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::ChooseTargets { slots, .. } => {
                    *self.seen.borrow_mut() = slots[0].legal.clone();
                    DecisionResponse::Pairs(vec![(0, 0)])
                }
                _ => DecisionResponse::Pass,
            }
        }
    }

    /// The headline: Brush Off cast through the **real** cast path counters an opposing creature
    /// spell on the stack — the spell leaves the stack to its owner's graveyard (CR 701.5a), and
    /// Brush Off itself resolves to its own graveyard.
    #[test]
    fn counters_a_spell_through_the_real_cast_path() {
        // Full {2}{U}{U} for a creature spell (no discount): 4 Islands.
        let (mut state, brush) = setup(4);
        let (bears, _bears_sid) = put_on_battlefield_stack(&mut state, grp::GRIZZLY_BEARS, PlayerId(1));
        let mut e = Engine::new(
            state,
            vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))],
        );

        e.cast_spell(PlayerId(0), brush, CastVariant::Normal);
        // Brush Off is on top; resolving it counters the bears below.
        e.resolve_top();
        assert!(
            !e.state.stack.items.iter().any(|s| matches!(s.kind, StackObjectKind::Spell(o) if o == bears)),
            "the creature spell was countered off the stack"
        );
        assert!(
            e.state.player(PlayerId(1)).graveyard.contains(&bears),
            "countered → owner's graveyard"
        );
        // Brush Off resolved and hit its own graveyard.
        e.resolve_top();
        assert!(
            e.state.player(PlayerId(0)).graveyard.contains(&brush),
            "Brush Off resolved into its own graveyard"
        );
    }

    /// Self-exclusion: while Brush Off is being cast it's on the stack too, but the engine never
    /// offers a spell as a target of its own targeting requirement — only the opposing spell is
    /// offered.
    #[test]
    fn does_not_offer_itself_as_a_target() {
        let (mut state, brush) = setup(4);
        let (_bears, bears_sid) = put_on_battlefield_stack(&mut state, grp::GRIZZLY_BEARS, PlayerId(1));
        let seen = Rc::new(RefCell::new(Vec::new()));
        let mut e = Engine::new(
            state,
            vec![
                Box::new(CaptureAgent { seen: seen.clone() }),
                Box::new(RandomAgent::new(1)),
            ],
        );
        e.cast_spell(PlayerId(0), brush, CastVariant::Normal);
        assert_eq!(
            seen.borrow().clone(),
            vec![Target::Stack(bears_sid)],
            "only the opposing spell offered — Brush Off is not a target of itself"
        );
    }

    /// CR 701.5f: a spell that can't be countered (Surrak, Elusive Hunter, whose stack-zone static
    /// paints `CantBeCountered`) is a legal target but is left on the stack when Brush Off resolves.
    #[test]
    fn cannot_counter_a_cant_be_countered_spell() {
        use crate::cards::tdm::surrak_elusive_hunter::SURRAK_ELUSIVE_HUNTER;
        let (mut state, brush) = setup(4);
        let (surrak, surrak_sid) = put_on_battlefield_stack(&mut state, SURRAK_ELUSIVE_HUNTER, PlayerId(1));
        let mut e = Engine::new(
            state,
            vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))],
        );
        assert!(
            e.state
                .computed(surrak)
                .has_qualification(crate::effects::ability::Qualification::CantBeCountered),
            "Surrak's spell carries CantBeCountered on the stack"
        );
        e.cast_spell(PlayerId(0), brush, CastVariant::Normal);
        e.resolve_top(); // Brush Off resolves, does nothing to Surrak.
        assert!(
            e.state.stack.items.iter().any(|s| s.id == surrak_sid),
            "Surrak can't be countered — still on the stack"
        );
        assert!(
            !e.state.player(PlayerId(1)).graveyard.contains(&surrak),
            "not put into the graveyard"
        );
    }

    /// The cost reduction settles on the chosen stack target: targeting an instant/sorcery spell
    /// takes {1}{U} off (→ {1}{U}); targeting a creature spell pays the full {2}{U}{U}. Optimistic
    /// (offer gate): the discount shows iff a legal instant/sorcery target exists.
    #[test]
    fn reduces_only_when_targeting_an_instant_or_sorcery_spell() {
        let (mut state, brush) = setup(0);
        let base = state.object(brush).chars.mana_cost.clone().unwrap();
        let (_bolt, bolt_sid) = put_on_battlefield_stack(&mut state, grp::LIGHTNING_BOLT, PlayerId(1));
        let (_bears, bears_sid) = put_on_battlefield_stack(&mut state, grp::GRIZZLY_BEARS, PlayerId(1));
        let e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);

        let cost = |t: Target| e.effective_cast_cost(PlayerId(0), brush, &base, TargetCtx::Chosen(&[t]));
        let is = cost(Target::Stack(bolt_sid));
        assert_eq!((is.generic, is.colored.get(&Color::Blue).copied()), (1, Some(1)),
            "targets an instant → {{1}}{{U}} off → {{1}}{{U}}");
        let cr = cost(Target::Stack(bears_sid));
        assert_eq!((cr.generic, cr.colored.get(&Color::Blue).copied()), (2, Some(2)),
            "targets a creature spell → no discount → {{2}}{{U}}{{U}}");
        // Optimistic: an instant is on the stack → the offer gate sees the discount.
        let opt = e.effective_cast_cost(PlayerId(0), brush, &base, TargetCtx::Optimistic);
        assert_eq!((opt.generic, opt.colored.get(&Color::Blue).copied()), (1, Some(1)),
            "an instant is targetable → optimistic {{1}}{{U}}");
    }

    /// No-rewind masking through the real cast path: with only {1}{U} up (2 Islands), and both an
    /// instant and a creature spell on the stack, Brush Off is offered **only** the instant — aiming
    /// it at the creature spell would need the full {2}{U}{U}, which the caster can't pay.
    #[test]
    fn constrains_targets_to_the_affordable_instant_spell() {
        let (mut state, brush) = setup(2); // exactly {1}{U} of mana.
        let (_bolt, bolt_sid) = put_on_battlefield_stack(&mut state, grp::LIGHTNING_BOLT, PlayerId(1));
        let (_bears, _bears_sid) = put_on_battlefield_stack(&mut state, grp::GRIZZLY_BEARS, PlayerId(1));
        let seen = Rc::new(RefCell::new(Vec::new()));
        let mut e = Engine::new(
            state,
            vec![
                Box::new(CaptureAgent { seen: seen.clone() }),
                Box::new(RandomAgent::new(1)),
            ],
        );
        e.cast_spell(PlayerId(0), brush, CastVariant::Normal);
        assert_eq!(
            seen.borrow().clone(),
            vec![Target::Stack(bolt_sid)],
            "only the affordable instant offered — the creature spell needs the full {{2}}{{U}}{{U}}"
        );
        // Paid the reduced {1}{U} = 2 mana (both Islands tapped).
        let tapped = e
            .state
            .player(PlayerId(0))
            .battlefield
            .iter()
            .filter(|&&id| e.state.object(id).chars.is_land() && e.state.object(id).status.tapped)
            .count();
        assert_eq!(tapped, 2, "paid {{1}}{{U}} = 2 mana");
    }
}
