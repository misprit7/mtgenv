//! The whiteboard: stage the intended `Action`s (design's `effects::action` vocabulary),
//! then commit, emitting `GameEvent`s. The heart of the whiteboard model
//! (WHITEBOARD_MODEL.md §2.1).
//!
//! Milestone 3 is the **minimal effect runtime**: an interpreter over design's `Effect` IR
//! that *materializes* a `Whiteboard` of `Action`s and *commits* them. The
//! replacement/prevention rewrite pass (CR 614/615/616) between materialize and commit is
//! deferred to milestone 4 — committing today applies the actions directly.
//!
//! Interpreted effects (the starter set's needs): `DealDamage`, `Draw`, `GainLife`,
//! `LoseLife`, `Sequence`. Other IR nodes are a graceful no-op until their cards arrive.

use crate::agent::GameEvent;
use crate::basics::{DamageKind, Target, Zone};
use crate::effects::ability::{Ability, ActionPattern, Rewrite};
use crate::effects::action::{Action, ResolutionCtx, Whiteboard, WbReason};
use crate::effects::target::CardFilter;
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};
use crate::ids::{ObjId, PlayerId};
use crate::priority::Engine;

/// The object an action's outcome lands on (for finding self-scoped replacement effects).
fn affected_object(action: &Action) -> Option<ObjId> {
    match action {
        Action::MoveZone { obj, to: Zone::Battlefield, .. } => Some(*obj),
        Action::Damage { target: Target::Object(o), .. } => Some(*o),
        _ => None,
    }
}

impl Engine {
    /// Resolve an `Effect`: materialize a whiteboard of `Action`s, then commit it.
    pub(crate) fn resolve_effect(&mut self, effect: &Effect, ctx: &ResolutionCtx, reason: WbReason) {
        let mut wb = Whiteboard::new(reason, ctx.clone());
        let mut cursor = 0usize;
        self.materialize(effect, ctx, &mut wb, &mut cursor);
        // (M4: run the replacement/prevention rewrite pass here.)
        self.commit(wb);
    }

    /// Walk an `Effect` tree, pushing the `Action`s it lowers to onto `wb`. `cursor` tracks
    /// which chosen target the next `EffectTarget::Target` consumes (declaration order).
    fn materialize(
        &self,
        effect: &Effect,
        ctx: &ResolutionCtx,
        wb: &mut Whiteboard,
        cursor: &mut usize,
    ) {
        match effect {
            Effect::Sequence(effects) => {
                for e in effects {
                    self.materialize(e, ctx, wb, cursor);
                }
            }
            Effect::DealDamage { amount, to, kind } => {
                let amount = self.eval_value(amount, ctx).max(0) as u32;
                if let Some(target) = self.resolve_target(to, ctx, cursor) {
                    wb.push(Action::Damage {
                        target,
                        amount,
                        source: ctx.source.unwrap_or(ObjId(0)),
                        kind: *kind,
                    });
                }
            }
            Effect::Draw { who, count } => {
                let count = self.eval_value(count, ctx).max(0) as u32;
                wb.push(Action::Draw {
                    player: self.eval_player(*who, ctx),
                    count,
                });
            }
            Effect::GainLife { who, amount } => {
                let amount = self.eval_value(amount, ctx).max(0) as u32;
                wb.push(Action::GainLife {
                    player: self.eval_player(*who, ctx),
                    amount,
                });
            }
            Effect::LoseLife { who, amount } => {
                let amount = self.eval_value(amount, ctx).max(0) as u32;
                wb.push(Action::LoseLife {
                    player: self.eval_player(*who, ctx),
                    amount,
                });
            }
            // Other IR nodes are not yet interpreted (milestone 3 minimal scope). They are a
            // no-op rather than a panic so a card carrying them degrades gracefully.
            _ => {}
        }
    }

    /// Commit a whiteboard (WHITEBOARD_MODEL §2.1): run the replacement/prevention rewrite
    /// pass, then apply each surviving action, emitting an event per completed one.
    pub(crate) fn commit(&mut self, mut wb: Whiteboard) {
        self.rewrite(&mut wb);
        for action in wb.actions {
            self.apply_action(action);
        }
    }

    /// The replacement/prevention rewrite pass (CR 614/616). Each applicable replacement
    /// modifies the staged actions **at most once** (CR 614.5), looping to a fixpoint.
    ///
    /// Milestone-4 prototype scope: SELF-scoped replacements only — the affected object's own
    /// `Ability::Replacement`s (the entering object for an ETB; the damaged object for damage).
    /// Global replacements on other permanents (Hardened Scales, "prevent damage to creatures
    /// you control") + a `CardFilter::ItSelf` are the documented generalization. No player
    /// choice among multiple replacements yet (CR 616.1f) — applies in ability order.
    fn rewrite(&self, wb: &mut Whiteboard) {
        let mut applied: Vec<(ObjId, usize)> = Vec::new();
        loop {
            let hit = wb.actions.iter().enumerate().find_map(|(ai, action)| {
                let obj = affected_object(action)?;
                let (idx, rw) = self.find_replacement(obj, action, &applied)?;
                Some((ai, obj, idx, rw))
            });
            let Some((ai, obj, idx, rw)) = hit else { break };
            applied.push((obj, idx));
            self.apply_rewrite(&rw, wb, ai, obj);
        }
    }

    /// The first unapplied self-replacement on `obj` whose pattern matches `action`.
    fn find_replacement(
        &self,
        obj: ObjId,
        action: &Action,
        applied: &[(ObjId, usize)],
    ) -> Option<(usize, Rewrite)> {
        let def = self.state.def_of(obj)?;
        def.abilities.iter().enumerate().find_map(|(idx, ab)| match ab {
            Ability::Replacement { pattern, rewrite }
                if !applied.contains(&(obj, idx)) && self.pattern_matches(pattern, action, obj) =>
            {
                Some((idx, rewrite.clone()))
            }
            _ => None,
        })
    }

    fn pattern_matches(&self, pattern: &ActionPattern, action: &Action, obj: ObjId) -> bool {
        match (pattern, action) {
            (
                ActionPattern::WouldEnterBattlefield(filter),
                Action::MoveZone { obj: o, to: Zone::Battlefield, .. },
            ) => *o == obj && self.filter_matches(filter, *o),
            (
                ActionPattern::WouldBeDealtDamage { to, kind },
                Action::Damage { target: Target::Object(o), kind: dk, .. },
            ) => {
                *o == obj
                    && self.filter_matches(to, *o)
                    && match kind {
                        Some(k) => k == dk,
                        None => true,
                    }
            }
            _ => false,
        }
    }

    /// Apply one rewrite to the staged actions (CR 614.1). Milestone-4 prototype: prevention
    /// (delete the action) and enters-with-counters (stage an `AddCounters` right after the
    /// ETB so the permanent is on the battlefield with its counters before SBAs run).
    fn apply_rewrite(&self, rw: &Rewrite, wb: &mut Whiteboard, ai: usize, obj: ObjId) {
        match rw {
            Rewrite::Prevent | Rewrite::Skip => {
                wb.actions.remove(ai);
            }
            Rewrite::EntersWithCounters { kind, n } => {
                wb.actions.insert(
                    ai + 1,
                    Action::AddCounters {
                        obj,
                        kind: kind.clone(),
                        n: *n as i32,
                    },
                );
            }
            // ReplaceWith / ScaleAmount / AddAmount / Redirect / EntersTapped: future work.
            _ => {}
        }
    }

    /// A minimal `CardFilter` evaluator (prototype). `Any` matches; the common structural
    /// predicates are supported; anything else is `false` until a card needs it.
    fn filter_matches(&self, filter: &CardFilter, obj: ObjId) -> bool {
        let Some(o) = self.state.objects.get(&obj) else {
            return false;
        };
        match filter {
            CardFilter::Any => true,
            CardFilter::All(fs) => fs.iter().all(|f| self.filter_matches(f, obj)),
            CardFilter::AnyOf(fs) => fs.iter().any(|f| self.filter_matches(f, obj)),
            CardFilter::Not(f) => !self.filter_matches(f, obj),
            CardFilter::HasCardType(t) => o.chars.card_types.contains(t),
            CardFilter::HasSubtype(s) => o.chars.subtypes.contains(s),
            _ => false,
        }
    }

    fn apply_action(&mut self, action: Action) {
        match action {
            Action::Damage {
                target,
                amount,
                source,
                kind,
            } => self.apply_damage(target, amount, source, kind),
            Action::Draw { player, count } => self.draw(player, count),
            Action::GainLife { player, amount } => self.change_life(player, amount as i32),
            Action::LoseLife { player, amount } => self.change_life(player, -(amount as i32)),
            Action::AddCounters { obj, kind, n } => {
                if let Some(o) = self.state.objects.get_mut(&obj) {
                    let cur = o.counters.counts.entry(kind).or_insert(0);
                    *cur = (*cur as i32 + n).max(0) as u32;
                }
            }
            Action::MoveZone { obj, to, .. } => {
                let owner = match self.state.objects.get(&obj) {
                    Some(o) => o.owner,
                    None => return,
                };
                if self.state.move_object(obj, to, owner) {
                    self.broadcast(GameEvent::ObjectMoved { obj, to });
                }
            }
            Action::Destroy { obj, .. } => {
                let owner = match self.state.objects.get(&obj) {
                    Some(o) => o.owner,
                    None => return,
                };
                if self.state.move_object(obj, Zone::Graveyard, owner) {
                    self.broadcast(GameEvent::PermanentDied { obj });
                    self.broadcast(GameEvent::ObjectMoved {
                        obj,
                        to: Zone::Graveyard,
                    });
                }
            }
            // Remaining Action variants are not produced by the milestone-3 interpreter.
            _ => {}
        }
    }

    /// Deal `amount` damage to `target` (CR 120). 0 damage is a non-event (CR 120.8). To a
    /// player: lose that much life. To a creature: mark damage (SBAs destroy it later).
    pub(crate) fn apply_damage(
        &mut self,
        target: Target,
        amount: u32,
        source: ObjId,
        _kind: DamageKind,
    ) {
        if amount == 0 {
            return;
        }
        match target {
            Target::Player(p) => {
                self.broadcast(GameEvent::DamageDealt {
                    target,
                    amount,
                    source,
                });
                self.change_life(p, -(amount as i32));
            }
            Target::Object(o) => {
                let is_bf_creature = self
                    .state
                    .objects
                    .get(&o)
                    .map(|x| x.zone == Zone::Battlefield && x.chars.is_creature())
                    .unwrap_or(false);
                if is_bf_creature {
                    if let Some(x) = self.state.objects.get_mut(&o) {
                        x.damage_marked += amount;
                    }
                    self.broadcast(GameEvent::DamageDealt {
                        target,
                        amount,
                        source,
                    });
                }
            }
            Target::Stack(_) => {}
        }
    }

    /// Apply a life-total delta and emit `LifeChanged` (CR 119). Loss to ≤0 is handled by the
    /// SBA on the next agenda pass (CR 704.5a).
    pub(crate) fn change_life(&mut self, p: PlayerId, delta: i32) {
        if delta == 0 {
            return;
        }
        let new_total = {
            let pl = self.state.player_mut(p);
            pl.life += delta;
            pl.life
        };
        self.broadcast(GameEvent::LifeChanged {
            player: p,
            delta,
            new_total,
        });
    }

    // ── IR resolution helpers ─────────────────────────────────────────────────────────────

    // `&self` is unused today beyond recursion, but `ValueExpr::Count` (M4) will read state.
    #[allow(clippy::only_used_in_recursion)]
    fn eval_value(&self, v: &ValueExpr, ctx: &ResolutionCtx) -> i64 {
        match v {
            ValueExpr::Fixed(n) => *n,
            ValueExpr::X => ctx.x.unwrap_or(0) as i64,
            ValueExpr::XTimes(k) => k * ctx.x.unwrap_or(0) as i64,
            ValueExpr::NumTargets => ctx.chosen_targets.len() as i64,
            ValueExpr::Sum(a, b) => self.eval_value(a, ctx) + self.eval_value(b, ctx),
            // Count and any future dynamic values: 0 until implemented (M4+).
            ValueExpr::Count { .. } => 0,
        }
    }

    fn eval_player(&self, who: PlayerRef, ctx: &ResolutionCtx) -> PlayerId {
        let controller = ctx.controller.unwrap_or(PlayerId(0));
        match who {
            PlayerRef::Controller => controller,
            PlayerRef::Owner => ctx
                .source
                .and_then(|s| self.state.objects.get(&s))
                .map(|o| o.owner)
                .unwrap_or(controller),
            PlayerRef::Opponent | PlayerRef::EachOpponent => self.opponent_of(controller),
            PlayerRef::EachPlayer => controller,
            PlayerRef::ChosenTarget(n) => match ctx.chosen_targets.get(n as usize) {
                Some(Target::Player(p)) => *p,
                _ => controller,
            },
        }
    }

    /// Resolve an `EffectTarget` to a concrete `Target`. `Target(_)` consumes the next chosen
    /// target (locked at cast, CR 601.2c). `Select` is not yet supported (returns `None`).
    fn resolve_target(
        &self,
        t: &EffectTarget,
        ctx: &ResolutionCtx,
        cursor: &mut usize,
    ) -> Option<Target> {
        match t {
            EffectTarget::Target(_) => {
                let target = ctx.chosen_targets.get(*cursor).copied();
                *cursor += 1;
                target
            }
            EffectTarget::ChosenIndex(n) => ctx.chosen_targets.get(*n as usize).copied(),
            EffectTarget::Player(who) => Some(Target::Player(self.eval_player(*who, ctx))),
            EffectTarget::SourceSelf => ctx.source.map(Target::Object),
            EffectTarget::Select(_) => None,
        }
    }
}
