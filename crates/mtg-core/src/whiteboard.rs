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

use crate::agent::{DecisionRequest, DecisionResponse, GameEvent, ReplacementOption};
use crate::basics::{DamageKind, Target, Zone};
use crate::effects::ability::{Ability, ActionPattern, Rewrite};
use crate::effects::action::{Action, ResolutionCtx, Whiteboard, WbReason};
use crate::effects::target::CardFilter;
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};
use crate::ids::{ObjId, PlayerId};
use crate::priority::Engine;

/// A replacement effect that applies to a pending action (for the CR 616.1f choice).
struct Applicable {
    source: ObjId,
    idx: usize,
    rewrite: Rewrite,
    description: String,
}

/// The object an action's outcome lands on (for finding applicable replacement effects).
fn affected_object(action: &Action) -> Option<ObjId> {
    match action {
        Action::MoveZone { obj, to: Zone::Battlefield, .. } => Some(*obj),
        Action::Damage { target: Target::Object(o), .. } => Some(*o),
        Action::AddCounters { obj, .. } => Some(*obj),
        _ => None,
    }
}

/// A short human-readable label for a rewrite (for the `ChooseReplacement` decision/UI).
fn describe_rewrite(rw: &Rewrite) -> String {
    match rw {
        Rewrite::Prevent => "prevent".to_string(),
        Rewrite::Skip => "skip".to_string(),
        Rewrite::ReplaceWith(_) => "instead".to_string(),
        Rewrite::ScaleAmount { numerator, denominator } => format!("scale {numerator}/{denominator}"),
        Rewrite::AddAmount(n) => format!("add {n}"),
        Rewrite::Redirect => "redirect".to_string(),
        Rewrite::EntersWithCounters { kind, n } => format!("enters with {n} {kind:?}"),
        Rewrite::EntersTapped => "enters tapped".to_string(),
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

    /// The replacement/prevention rewrite pass (CR 614/616). Scans every applicable
    /// replacement — both the affected object's own (self / `ItSelf`) and GLOBAL ones on any
    /// battlefield permanent (e.g. Hardened Scales) — and rewrites the staged actions to a
    /// fixpoint. Each replacement modifies a given event at most once (CR 614.5; keyed by
    /// (source, ability, affected)). When >1 applies to one event, the affected object's
    /// controller chooses which applies first (CR 616.1f), then we re-check.
    fn rewrite(&mut self, wb: &mut Whiteboard) {
        let mut applied: Vec<(ObjId, usize, ObjId)> = Vec::new();
        loop {
            // First action with ≥1 applicable, not-yet-applied replacement.
            let mut hit: Option<(usize, ObjId, Vec<Applicable>)> = None;
            for (ai, action) in wb.actions.iter().enumerate() {
                let Some(affected) = affected_object(action) else {
                    continue;
                };
                let applicable = self.applicable_replacements(action, affected, &applied);
                if !applicable.is_empty() {
                    hit = Some((ai, affected, applicable));
                    break;
                }
            }
            let Some((ai, affected, applicable)) = hit else { break };

            // CR 616.1f: the affected object's controller picks which applies first.
            let pick = if applicable.len() == 1 {
                0
            } else {
                self.choose_replacement(affected, &applicable)
            };
            let chosen = &applicable[pick];
            applied.push((chosen.source, chosen.idx, affected));
            let rw = chosen.rewrite.clone();
            self.apply_rewrite(&rw, wb, ai, affected);
        }
    }

    /// Every replacement (self + global) that applies to `action` (affecting `affected`) and
    /// hasn't already fired for this (source, ability, affected) event.
    fn applicable_replacements(
        &self,
        action: &Action,
        affected: ObjId,
        applied: &[(ObjId, usize, ObjId)],
    ) -> Vec<Applicable> {
        // Candidate sources: the affected object itself (covers self-replacements on an
        // object that isn't on the battlefield yet, e.g. an ETB) + every battlefield
        // permanent (global replacements).
        let mut sources = vec![affected];
        for p in &self.state.players {
            for &id in &p.battlefield {
                if id != affected {
                    sources.push(id);
                }
            }
        }
        let mut out = Vec::new();
        for src in sources {
            let Some(def) = self.state.def_of(src) else {
                continue;
            };
            for (idx, ab) in def.abilities.iter().enumerate() {
                if let Ability::Replacement { pattern, rewrite } = ab {
                    if applied.contains(&(src, idx, affected)) {
                        continue;
                    }
                    if self.pattern_matches(pattern, action, affected, src) {
                        out.push(Applicable {
                            source: src,
                            idx,
                            rewrite: rewrite.clone(),
                            description: describe_rewrite(rewrite),
                        });
                    }
                }
            }
        }
        out
    }

    /// Ask the affected object's controller which replacement to apply first (CR 616.1f).
    fn choose_replacement(&mut self, affected: ObjId, applicable: &[Applicable]) -> usize {
        let controller = self
            .state
            .objects
            .get(&affected)
            .map(|o| o.controller)
            .unwrap_or(self.state.active_player);
        let options = applicable
            .iter()
            .map(|a| ReplacementOption {
                source: a.source,
                description: a.description.clone(),
            })
            .collect();
        let req = DecisionRequest::ChooseReplacement {
            event: "replacement".to_string(),
            applicable: options,
        };
        match self.ask(controller, &req) {
            DecisionResponse::Index(i) => (i as usize).min(applicable.len() - 1),
            _ => 0,
        }
    }

    /// Whether `pattern` (a replacement on `source`) matches `action` (affecting `affected`).
    fn pattern_matches(
        &self,
        pattern: &ActionPattern,
        action: &Action,
        affected: ObjId,
        source: ObjId,
    ) -> bool {
        match (pattern, action) {
            (
                ActionPattern::WouldEnterBattlefield(filter),
                Action::MoveZone { obj: o, to: Zone::Battlefield, .. },
            ) => *o == affected && self.filter_matches(filter, affected, source),
            (
                ActionPattern::WouldBeDealtDamage { to, kind },
                Action::Damage { target: Target::Object(o), kind: dk, .. },
            ) => {
                *o == affected
                    && self.filter_matches(to, affected, source)
                    && match kind {
                        Some(k) => k == dk,
                        None => true,
                    }
            }
            (
                ActionPattern::WouldAddCounters { kind, to },
                Action::AddCounters { obj: o, kind: k, .. },
            ) => *o == affected && k == kind && self.filter_matches(to, affected, source),
            _ => false,
        }
    }

    /// Apply one rewrite to the staged actions (CR 614.1).
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
            Rewrite::EntersTapped => {
                // The permanent enters tapped: tap it right after it enters, in the same
                // commit (before SBAs), so it's tapped as it arrives (CR 614.1c/d).
                wb.actions
                    .insert(ai + 1, Action::TapUntap { obj, tap: true });
            }
            Rewrite::AddAmount(delta) => match &mut wb.actions[ai] {
                Action::Damage { amount, .. } => {
                    *amount = (*amount as i64 + delta).max(0) as u32;
                }
                Action::AddCounters { n, .. } => {
                    *n = (*n + *delta as i32).max(0);
                }
                _ => {}
            },
            Rewrite::ScaleAmount { numerator, denominator } => {
                let den = (*denominator).max(1);
                match &mut wb.actions[ai] {
                    Action::Damage { amount, .. } => {
                        *amount = *amount * *numerator / den;
                    }
                    Action::AddCounters { n, .. } => {
                        *n = *n * *numerator as i32 / den as i32;
                    }
                    _ => {}
                }
            }
            // ReplaceWith / Redirect: future work.
            _ => {}
        }
    }

    /// Evaluate a `CardFilter` against object `obj`, where the filter belongs to a replacement
    /// on `source` (so `ItSelf` and `ControlledBy(Controller)` resolve relative to `source`).
    fn filter_matches(&self, filter: &CardFilter, obj: ObjId, source: ObjId) -> bool {
        let Some(o) = self.state.objects.get(&obj) else {
            return false;
        };
        match filter {
            CardFilter::Any => true,
            CardFilter::ItSelf => obj == source,
            CardFilter::All(fs) => fs.iter().all(|f| self.filter_matches(f, obj, source)),
            CardFilter::AnyOf(fs) => fs.iter().any(|f| self.filter_matches(f, obj, source)),
            CardFilter::Not(f) => !self.filter_matches(f, obj, source),
            CardFilter::HasCardType(t) => o.chars.card_types.contains(t),
            CardFilter::HasSubtype(s) => o.chars.subtypes.contains(s),
            CardFilter::ControlledBy(pref) => {
                let src_controller = self
                    .state
                    .objects
                    .get(&source)
                    .map(|s| s.controller)
                    .unwrap_or(o.controller);
                let want = match pref {
                    PlayerRef::Controller | PlayerRef::Owner => src_controller,
                    PlayerRef::Opponent | PlayerRef::EachOpponent => self.opponent_of(src_controller),
                    _ => src_controller,
                };
                o.controller == want
            }
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
                // +1/+1 / -1/-1 counters change computed P/T (CR 613 layer 7c).
                self.state.mark_chars_dirty();
            }
            Action::TapUntap { obj, tap } => {
                if let Some(o) = self.state.objects.get_mut(&obj) {
                    o.status.tapped = tap;
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
    /// Keyword hooks on the SOURCE (CR 702): deathtouch marks the target lethal (704.5h);
    /// lifelink gains the source's controller that much life (702.15).
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
        let src = crate::chars::compute(&self.state, source);
        let deathtouch = src.has_keyword(crate::effects::ability::Keyword::Deathtouch);
        let lifelink = src.has_keyword(crate::effects::ability::Keyword::Lifelink);

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
                        if deathtouch {
                            x.dealt_deathtouch = true; // CR 702.2 / 704.5h
                        }
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
        // Lifelink (CR 702.15): the source's controller gains life equal to the damage dealt.
        if lifelink {
            if let Some(controller) = self.state.objects.get(&source).map(|s| s.controller) {
                self.change_life(controller, amount as i32);
            }
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
