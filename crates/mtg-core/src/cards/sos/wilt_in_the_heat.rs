//! Wilt in the Heat — `{2}{R}{W}` Instant (first printed SOS).
//!
//! Oracle: "This spell costs {2} less to cast if one or more cards left your graveyard this turn.
//! Wilt in the Heat deals 5 damage to target creature. If that creature would die this turn, exile
//! it instead."
//!
//! **Fully implemented** — the lander for the **floating delayed-replacement** cap (CR 614): a
//! resolution-created, object-scoped, one-shot replacement stored in `GameState.floating_replacements`
//! and consulted by the same rewrite pass as printed statics. `Effect::ExileIfWouldDie` registers "if
//! that creature would die (battlefield→graveyard, CR 700.4 — destruction / sacrifice / legend rule)
//! this turn, exile it instead". The S12 cost reduction ({2} less if a card left your graveyard this
//! turn) reuses the state-condition pipeline.

use crate::basics::{CardType, Color, DamageKind};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::ability::{
    Ability, CostReductionAmount, CostReductionCondition, CostReductionScope,
};
use crate::effects::condition::Condition;
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};

/// grp id (per-set ids live near their cards).
pub const WILT_IN_THE_HEAT: u32 = 366;

pub fn register(db: &mut CardDb) {
    // 5 damage to target creature, then "if that creature would die this turn, exile it instead"
    // (scoped to the same chosen target, slot 0).
    let effect = Effect::Sequence(vec![
        Effect::DealDamage {
            amount: ValueExpr::Fixed(5),
            to: EffectTarget::Target(TargetSpec {
                kind: TargetKind::Creature(CardFilter::Any),
                min: 1,
                max: 1,
                distinct: true,
            }),
            kind: DamageKind::Noncombat,
        },
        Effect::ExileIfWouldDie { what: EffectTarget::ChosenIndex(0) },
    ]);
    let mut def = spell(
        WILT_IN_THE_HEAT,
        "Wilt in the Heat",
        CardType::Instant,
        Color::Red, // {2}{R}{W} — primary colour for the single-colour slot; W pip carried in the cost
        mana_cost(2, &[(Color::Red, 1), (Color::White, 1)]),
        effect,
    )
    .with_text("This spell costs {2} less to cast if one or more cards left your graveyard this turn.\nWilt in the Heat deals 5 damage to target creature. If that creature would die this turn, exile it instead.");
    // "This spell costs {2} less to cast if one or more cards left your graveyard this turn." (CR
    // 601.2f — a caster-relative state condition, so affordability == payment.)
    def.abilities.push(Ability::CostReduction {
        amount: CostReductionAmount::Generic(2),
        condition: CostReductionCondition::State(Condition::CardLeftGraveyardThisTurn {
            who: PlayerRef::Controller,
        }),
        scope: CostReductionScope::Cast,
    });
    def.chars.colors = vec![Color::Red, Color::White]; // {2}{R}{W} is red-white
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{CastVariant, RandomAgent};
    use crate::basics::{Phase, Zone};
    use crate::cards::{build_game, grp};
    use crate::ids::PlayerId;
    use crate::priority::Engine;

    #[test]
    fn wilt_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(WILT_IN_THE_HEAT).unwrap();
        assert!(def.fully_implemented);
        assert_eq!(def.chars.card_types, vec![CardType::Instant]);
        assert!(matches!(&def.abilities[1], Ability::CostReduction { .. }));
    }

    /// Real-path: cast Wilt at a 2/2 → 5 damage kills it → the floating "exile instead of dying"
    /// rider sends it to **exile**, not the graveyard.
    #[test]
    fn wilt_kills_and_exiles() {
        let mut state = build_game(1, &[&[], &[]]);
        let wilt = state.add_card(PlayerId(0), state.card_db().get(WILT_IN_THE_HEAT).unwrap().chars.clone(), Zone::Hand);
        // {2}{R}{W}: two Mountains + two Plains, untapped.
        for grp_id in [grp::MOUNTAIN, grp::MOUNTAIN, grp::PLAINS, grp::PLAINS] {
            let c = state.card_db().get(grp_id).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield);
        }
        let bears = state.add_card(PlayerId(1), state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Battlefield); // 2/2
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
        e.cast_spell(PlayerId(0), wilt, CastVariant::Normal); // targets the only creature
        e.resolve_top(); // 5 damage + register the rider
        // Lethal damage is an SBA — run the agenda so it's collected + the death→exile rewrite fires.
        e.run_agenda();
        assert_eq!(e.state.object(bears).zone, Zone::Exile, "the creature was exiled, not put in the graveyard");
        assert!(!e.state.player(PlayerId(1)).graveyard.contains(&bears), "not in the graveyard");
    }

    /// Constraint 1 — "dies" is ANY battlefield→graveyard move, not just destruction: a creature under
    /// the rider that is **sacrificed** is exiled too (the sacrifice runs through the whiteboard
    /// rewrite pass, which redirects it to exile).
    #[test]
    fn rider_exiles_on_sacrifice_too() {
        use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView};
        use crate::basics::Target;
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::effects::target::SelectSpec;
        use crate::effects::value::PlayerRef;
        use crate::ids::StackId;

        struct SelAgent;
        impl Agent for SelAgent {
            fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
                match req {
                    DecisionRequest::SelectCards { min, .. } => DecisionResponse::Indices((0..(*min).max(1)).collect()),
                    _ => DecisionResponse::Pass,
                }
            }
        }

        let mut state = build_game(1, &[&[], &[]]);
        let victim = state.add_card(PlayerId(0), state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Battlefield);
        state.active_player = PlayerId(0);
        let mut e = Engine::new(state, vec![Box::new(SelAgent), Box::new(SelAgent)]);
        // Register the "if it would die this turn, exile it instead" rider on the creature.
        e.resolve_effect(
            &Effect::ExileIfWouldDie { what: EffectTarget::ChosenIndex(0) },
            &ResolutionCtx {
                controller: Some(PlayerId(0)),
                chosen_targets: vec![Target::Object(victim)],
                ..Default::default()
            },
            WbReason::Resolve(StackId(0)),
        );
        // Sacrifice it — a non-destruction death; the rider must still exile it.
        e.resolve_effect(
            &Effect::Sacrifice {
                who: PlayerRef::Controller,
                what: SelectSpec {
                    zone: Zone::Battlefield,
                    filter: CardFilter::All(vec![
                        CardFilter::ControlledBy(PlayerRef::Controller),
                        CardFilter::HasCardType(CardType::Creature),
                    ]),
                    chooser: PlayerRef::Controller,
                    min: ValueExpr::Fixed(1),
                    max: ValueExpr::Fixed(1),
                },
            },
            &ResolutionCtx { controller: Some(PlayerId(0)), ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        assert_eq!(e.state.object(victim).zone, Zone::Exile, "a sacrificed creature under the rider is exiled");
        assert!(!e.state.player(PlayerId(0)).graveyard.contains(&victim), "not in the graveyard");
    }

    /// Constraint 2 — CR 400.7: the rider is scoped by ObjId and invalidated the moment the object
    /// leaves the battlefield (it becomes a new object), so a bounced-then-returned creature isn't
    /// chased back. Here the creature is bounced to hand → the floating rider is dropped.
    #[test]
    fn rider_invalidated_when_the_object_leaves_the_battlefield() {
        use crate::basics::Target;
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::StackId;

        let mut state = build_game(1, &[&[], &[]]);
        let cr = state.add_card(PlayerId(0), state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Battlefield);
        let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
        e.resolve_effect(
            &Effect::ExileIfWouldDie { what: EffectTarget::ChosenIndex(0) },
            &ResolutionCtx {
                controller: Some(PlayerId(0)),
                chosen_targets: vec![Target::Object(cr)],
                ..Default::default()
            },
            WbReason::Resolve(StackId(0)),
        );
        assert_eq!(e.state.floating_replacements.len(), 1, "rider registered");
        // Bounce the creature to hand — it leaves the battlefield.
        e.state.move_object(cr, Zone::Hand, PlayerId(0));
        assert!(e.state.floating_replacements.is_empty(), "the rider was invalidated when its object left the battlefield");
    }

    /// Constraint 3 — floating riders are consulted by the SAME rewrite pass as printed statics, so
    /// when more than one applies to a death the CR 616.1f `ChooseReplacement` choice is invoked. Two
    /// "exile instead of dying" riders on one creature, destroyed by an effect (which runs through the
    /// rewrite pass): the controller is asked which applies first, then the creature is exiled.
    #[test]
    fn multiple_riders_go_through_choose_replacement() {
        use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView};
        use crate::basics::Target;
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::StackId;
        use std::cell::Cell;
        use std::rc::Rc;

        let asked = Rc::new(Cell::new(false));
        struct ChooseAgent(Rc<Cell<bool>>);
        impl Agent for ChooseAgent {
            fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
                match req {
                    DecisionRequest::ChooseReplacement { .. } => {
                        self.0.set(true);
                        DecisionResponse::Index(0)
                    }
                    _ => DecisionResponse::Pass,
                }
            }
        }

        let mut state = build_game(1, &[&[], &[]]);
        let cr = state.add_card(PlayerId(0), state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Battlefield);
        let mut e = Engine::new(state, vec![Box::new(ChooseAgent(asked.clone())), Box::new(ChooseAgent(asked.clone()))]);
        let ctx = ResolutionCtx {
            controller: Some(PlayerId(0)),
            chosen_targets: vec![Target::Object(cr)],
            ..Default::default()
        };
        // Two riders on the same creature.
        for _ in 0..2 {
            e.resolve_effect(&Effect::ExileIfWouldDie { what: EffectTarget::ChosenIndex(0) }, &ctx, WbReason::Resolve(StackId(0)));
        }
        // Destroy it via an effect — the rewrite pass sees both riders → CR 616.1f choice.
        e.resolve_effect(&Effect::Destroy { what: EffectTarget::ChosenIndex(0) }, &ctx, WbReason::Resolve(StackId(0)));
        assert!(asked.get(), "two applicable replacements → the controller was asked which applies first");
        assert_eq!(e.state.object(cr).zone, Zone::Exile, "the creature was exiled");
    }
}
