//! The whiteboard: stage the intended `Action`s (design's `effects::action` vocabulary),
//! then commit, emitting `GameEvent`s. The heart of the whiteboard model
//! (WHITEBOARD_MODEL.md §2.1).
//!
//! The effect runtime: an interpreter over design's `Effect` IR that *materializes* a
//! `Whiteboard` of `Action`s, runs the **replacement/prevention rewrite pass** (CR 614/615/616,
//! `rewrite` — a fixpoint over both self and global replacements), then *commits* the survivors,
//! emitting a `GameEvent` per applied action.
//!
//! Resolution splits across two methods: [`EngineCore::interpret`] handles the interactive /
//! control-flow nodes (`Sequence`, `Optional`, `IfYouDo`, `Modal`, `ForEach`, `Search`,
//! `AddMana`) — asking the controller for resolution-time choices and returning whether the
//! effect actually *performed* (so `IfYouDo` can gate a reward on its cost) — while
//! [`EngineCore::materialize`] lowers the pure leaves (`DealDamage`, `Draw`, `Destroy`, `PutCounters`,
//! `CreateToken`, `Conditional`, …) into `Action`s. IR nodes with no card using them yet are a
//! graceful no-op rather than a panic.

use crate::agent::{
    ActionRef, ConfirmKind, DecisionRequest, DecisionResponse, GameEvent, ModeOption,
    ReplacementOption, SelectReason,
};
use crate::basics::{CardType, Color, CounterKind, DamageKind, Target, Zone, ZoneDest, ZonePos};
use crate::effects::ability::{Ability, ActionPattern, Keyword, Rewrite, StaticContribution};
use crate::effects::condition::{Condition, Duration};
use crate::effects::action::{
    Action, DelayedTriggerEvent, MoveCause, ResolutionCtx, Whiteboard, WbReason,
};
use crate::effects::target::{CardFilter, ManaSpec, SelectSpec, TokenSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget, Mode};
use crate::ids::{ObjId, PlayerId, StackId};
use crate::state::Characteristics;
use crate::priority::EngineCore;

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
        Rewrite::EntersWithCountersValue { kind, .. } => format!("enters with N {kind:?}"),
        Rewrite::EntersTapped => "enters tapped".to_string(),
        Rewrite::EntersTappedUnless(_) => "enters tapped unless …".to_string(),
        Rewrite::EntersTappedUnlessPay { life } => format!("enters tapped unless you pay {life} life"),
    }
}

impl EngineCore {
    /// Resolve an `Effect`: interpret its tree (asking the controller for any resolution-time
    /// choices — modal modes, search selections) while materializing a whiteboard of `Action`s,
    /// then commit it. Pure leaves lower in [`EngineCore::materialize`]; interactive/control-flow
    /// nodes are handled in [`EngineCore::interpret`].
    pub(crate) fn resolve_effect(&mut self, effect: &Effect, ctx: &ResolutionCtx, reason: WbReason) {
        let sid = match &reason {
            WbReason::Resolve(s) => *s,
            _ => StackId(0),
        };
        self.searched_this_resolution.clear();
        // Snapshot each player's graveyard size so we can fire the "cards leave your graveyard"
        // trigger (CR — SoS Lorehold) once per resolution in which a graveyard shrank (batched).
        let gy_before: Vec<usize> = self.state.players.iter().map(|p| p.graveyard.len()).collect();
        let mut wb = Whiteboard::new(reason, ctx.clone());
        let mut cursor = 0usize;
        self.interpret(effect, ctx, sid, &mut wb, &mut cursor);
        // (M4: run the replacement/prevention rewrite pass here.)
        self.commit(wb);
        // Fire graveyard-leave for any player whose graveyard net-shrank this resolution.
        for (i, before) in gy_before.iter().enumerate() {
            if self.state.players.get(i).is_some_and(|p| p.graveyard.len() < *before) {
                self.broadcast(GameEvent::LeftGraveyard { player: PlayerId(i as u32) });
            }
        }
    }

    /// Commit the deferred actions accumulated in `wb` SO FAR, then leave it empty (same reason/ctx)
    /// to keep accumulating. Called before each *imperative* effect in a sequence so a resolving
    /// spell's instructions take effect IN ORDER across the imperative/deferred boundary (#61):
    /// without it, `Sequence[Destroy, fetch a land]` lets the land enter (imperative) while the
    /// doomed creature is still on the battlefield, wrongly firing its landfall. Deferred-only runs
    /// still batch into one commit (replacement/prevention, CR 614/616, unaffected) — only a
    /// deferred→imperative hand-off splits the batch, which is exactly the ordering the rules want.
    fn flush_pending(&mut self, wb: &mut Whiteboard) {
        if wb.actions.is_empty() {
            return;
        }
        let actions = std::mem::take(&mut wb.actions);
        self.commit(Whiteboard { reason: wb.reason.clone(), actions, ctx: wb.ctx.clone() });
    }

    /// The interactive interpreter: handles control-flow + resolution-time-decision nodes
    /// (Sequence/Modal/Search), delegating every pure leaf to [`Engine::materialize`] with the
    /// shared `cursor` (so a multi-target sequence still distributes its locked targets in order).
    ///
    /// Returns whether the effect was **actually performed** — used by [`Effect::IfYouDo`] to gate
    /// a reward on its cost ("you may … If you do, …", CR). Most effects "perform" unconditionally
    /// (return `true`); the ones that can fail to do so are a declined `Optional`, a `ForEach`/
    /// `Select` that can't reach its `min`, and `Nothing`.
    fn interpret(
        &mut self,
        effect: &Effect,
        ctx: &ResolutionCtx,
        sid: StackId,
        wb: &mut Whiteboard,
        cursor: &mut usize,
    ) -> bool {
        match effect {
            Effect::Sequence(effects) => {
                // Performed iff every step performed (an empty sequence is vacuously done).
                let mut all = true;
                for e in effects {
                    all &= self.interpret(e, ctx, sid, wb, cursor);
                }
                all
            }
            // C7: modal — ask which mode(s), then resolve each chosen mode's effect (CR 700.2).
            Effect::Modal { modes, min, max, allow_repeat } => {
                let mut any = false;
                for idx in self.choose_modes(ctx, sid, modes, *min, *max, *allow_repeat) {
                    if let Some(m) = modes.get(idx as usize) {
                        any |= self.interpret(&m.effect, ctx, sid, wb, cursor);
                    }
                }
                any
            }
            // C5: search a zone (asks the searcher which card(s)), move the picks to `to`, then
            // shuffle a searched library. Done imperatively (search/shuffle aren't whiteboard
            // actions). Flush any deferred actions staged so far FIRST so they take effect before
            // this imperative step (#61): e.g. Erode's `Sequence[Destroy, fetch a land]` must destroy
            // the creature before the fetched land enters, or the doomed creature's landfall fires.
            Effect::Search { who, zone, filter, min, max, to, tapped } => {
                self.flush_pending(wb);
                self.interpret_search(ctx, *who, *zone, filter, *min, *max, to, *tapped);
                true
            }
            // C19: add mana to a player's pool (a mana ability resolving, or a ritual). Imperative
            // (mana isn't a whiteboard action); `any_color` asks the player which colour. Flush first
            // so prior deferred actions are applied before this imperative step (#61).
            Effect::AddMana { who, mana } => {
                self.flush_pending(wb);
                let player = self.eval_player(*who, ctx);
                self.add_mana(player, mana, ctx);
                true
            }
            // Discard N cards (CR 701.8): the discarding player chooses which. Imperative + asks the
            // agent, so it lives here (not `materialize`). Flush staged actions FIRST so a loot's
            // "draw two, then discard a card" chooses from the post-draw hand. Performed iff at least
            // one card was discarded (an empty hand performs nothing).
            Effect::Discard { who, count } => {
                self.flush_pending(wb);
                let player = self.eval_player(*who, ctx);
                let count = self.eval_value(count, ctx).max(0) as u32;
                self.interpret_discard(player, count) > 0
            }
            // Counter a target spell/ability on the stack (CR 701.5). Imperative (mutates the stack),
            // so it lives here. A spell with the `CantBeCountered` qualification (CR 701.5f) is left
            // on the stack — the counterspell still resolved, it just did nothing to that spell.
            // Flush first so any earlier staged actions apply before the stack changes.
            Effect::Counter { what } => {
                self.flush_pending(wb);
                if let Some(Target::Stack(sid)) = self.resolve_target(what, ctx, cursor) {
                    self.interpret_counter(sid);
                }
                true
            }
            // Sacrifice permanents as an effect (CR 701.17) — "sacrifice two lands", "each player
            // sacrifices a creature of their choice." Imperative + asks each sacrificing player which
            // of their own permanents to sacrifice, so it lives here. Flush staged actions first.
            // Performed iff at least one permanent was sacrificed (an unmet min sacrifices what it can).
            // Surveil N (CR 701.42): look at the top N of your library, bin any number to the
            // graveyard, keep the rest on top. Imperative (asks which to bin). Flush first.
            Effect::Surveil { count } => {
                self.flush_pending(wb);
                let player = ctx.controller.unwrap_or(PlayerId(0));
                let n = self.eval_value(count, ctx).max(0) as usize;
                self.interpret_surveil(player, n);
                true
            }
            // "Look at the top `count`, put `take` into `take_to`, the rest into `rest_to`." Imperative
            // (asks which to take). Flush first.
            Effect::LookAndPick { count, take, take_to, rest_to, take_filter } => {
                self.flush_pending(wb);
                let player = ctx.controller.unwrap_or(PlayerId(0));
                let n = self.eval_value(count, ctx).max(0) as usize;
                let take = self.eval_value(take, ctx).max(0) as usize;
                self.interpret_look_and_pick(player, n, take, *take_to, *rest_to, take_filter);
                true
            }
            Effect::Sacrifice { who, what } => {
                self.flush_pending(wb);
                let controller = ctx.controller.unwrap_or(PlayerId(0));
                let players: Vec<PlayerId> = match who {
                    PlayerRef::EachPlayer => {
                        (0..self.state.players.len() as u32).map(PlayerId).collect()
                    }
                    PlayerRef::EachOpponent => vec![self.opponent_of(controller)],
                    other => vec![self.eval_player(*other, ctx)],
                };
                let mut any = 0usize;
                for pl in players {
                    any += self.interpret_sacrifice(pl, what, ctx);
                }
                any > 0
            }
            // "You may …" (CR 603.5 / optional effect): ask the controller; run `body` on yes.
            // Performed iff the controller said yes AND the body itself performed (so a "may" whose
            // body can't be carried out still reports "not done" to a wrapping `IfYouDo`).
            Effect::Optional { prompt: _, body } => {
                let controller = ctx.controller.unwrap_or(PlayerId(0));
                let yes = matches!(
                    self.ask(controller, &DecisionRequest::Confirm { kind: ConfirmKind::MayEffect }),
                    DecisionResponse::Bool(true)
                );
                yes && self.interpret(body, ctx, sid, wb, cursor)
            }
            // "[do `cost`]. If you do, [`reward`]" (CR "you may … If you do, …"): run the cost, and
            // run the reward ONLY if the cost was actually performed. Gating ties to the cost's real
            // execution (a `ForEach` reaching its `min`, an `Optional` accepted), never a separate
            // state predicate that could disagree (e.g. counter-based filters the condition system
            // can't see). Returns the cost's performed flag so nested `IfYouDo`s compose.
            Effect::IfYouDo { cost, reward } => {
                let did = self.interpret(cost, ctx, sid, wb, cursor);
                if did {
                    self.interpret(reward, ctx, sid, wb, cursor);
                }
                did
            }
            // Conditional (CR 603.4 / intervening-if / "if …"): evaluated here (not only in
            // `materialize`) so an *interactive* branch (a conditional Discard/Surveil/Search — Muse
            // Seeker's "discard a card unless five or more mana …") actually runs. A **targeted**
            // `then` inside an ability is still a reflexive sub-trigger (CR 603.7c): delegate the whole
            // node to `materialize`, which registers the reflexive. `cond` is evaluated ctx-aware
            // (so `ManaSpentOnTrigger`/`X` resolve).
            Effect::Conditional { cond, then, otherwise } => {
                let reflexive = ctx
                    .source
                    .zip(ctx.ability_index)
                    .filter(|_| !crate::priority::collect_target_specs(then).is_empty());
                if reflexive.is_some() {
                    self.materialize(effect, ctx, wb, cursor);
                    true
                } else if self.cond_holds(cond, ctx) {
                    self.interpret(then, ctx, sid, wb, cursor)
                } else if let Some(otherwise) = otherwise {
                    self.interpret(otherwise, ctx, sid, wb, cursor)
                } else {
                    true
                }
            }
            // "For each [selector] …" (CR): select the objects (asking if it's a choice), then run
            // `body` once per object with it bound as `EffectTarget::Each` (Dyadrine's "remove a
            // counter from each of two creatures you control"). Performed iff the selection reached
            // its `min` — a "from each of two" that can't find two reports "not done".
            Effect::ForEach { selector, body } => {
                let min = self.eval_value(&selector.min, ctx).max(0) as usize;
                let selected = self.select_for_each(selector, ctx);
                let performed = selected.len() >= min;
                for item in selected {
                    let prev = self.foreach_current.replace(item);
                    self.interpret(body, ctx, sid, wb, cursor);
                    self.foreach_current = prev;
                }
                performed
            }
            // A no-op never counts as "performed" (so it can't satisfy an `IfYouDo` cost).
            Effect::Nothing => false,
            // Pure leaves (and not-yet-interactive nodes) lower without agent interaction; a leaf
            // that lowers is considered performed.
            _ => {
                self.materialize(effect, ctx, wb, cursor);
                true
            }
        }
    }

    /// C5: resolve a `Search` — enumerate the searcher's matching cards in `zone`, ask which to
    /// take (`SelectCards`), move them to `to`, and shuffle a searched library (CR 701.19).
    /// (Entering tapped is wired once `Effect::Search` carries the flag — pending design IR.)
    #[allow(clippy::too_many_arguments)]
    fn interpret_search(
        &mut self,
        ctx: &ResolutionCtx,
        who: PlayerRef,
        zone: Zone,
        filter: &CardFilter,
        min: u32,
        max: u32,
        to: &ZoneDest,
        tapped: bool,
    ) {
        let searcher = self.eval_player(who, ctx);
        let from: Vec<ObjId> = self
            .zone_cards(searcher, zone)
            .into_iter()
            .filter(|&id| self.count_filter_matches(id, filter))
            .collect();
        let picks: Vec<ObjId> = if from.is_empty() {
            Vec::new()
        } else {
            let resp = self.ask(
                searcher,
                &DecisionRequest::SelectCards {
                    reason: SelectReason::Search,
                    from: from.clone(),
                    min,
                    max,
                    description: "Search".into(),
                },
            );
            let idxs = match resp {
                DecisionResponse::Indices(v) => v,
                DecisionResponse::Index(i) => vec![i],
                _ => Vec::new(),
            };
            idxs.iter()
                .filter_map(|&i| from.get(i as usize).copied())
                .take(max as usize)
                .collect()
        };
        for card in &picks {
            if self.state.move_object(*card, to.zone, searcher) {
                // Fetch lands enter tapped (CR — Fabled Passage / Escape Tunnel).
                if tapped && to.zone == Zone::Battlefield {
                    if let Some(o) = self.state.objects.get_mut(card) {
                        o.status.tapped = true;
                    }
                }
                // Record it so a follow-up effect can reference "that land" (Fabled Passage).
                self.searched_this_resolution.push(*card);
                self.broadcast(GameEvent::ObjectMoved { obj: *card, to: to.zone });
            }
        }
        if zone == Zone::Library {
            self.state.shuffle_library(searcher);
        }
    }

    /// Discard `count` cards from `player`'s hand (CR 701.8): the discarding player chooses which
    /// (fewer if the hand is smaller). Mandatory — if the agent under-selects, the front of the
    /// hand fills in. Returns the number actually discarded (for the `IfYouDo`/"performed" flag).
    fn interpret_discard(&mut self, player: PlayerId, count: u32) -> usize {
        let hand = self.state.player(player).hand.clone();
        let n = (count as usize).min(hand.len());
        if n == 0 {
            return 0;
        }
        let req = DecisionRequest::SelectCards {
            reason: SelectReason::Discard,
            from: hand.clone(),
            min: n as u32,
            max: n as u32,
            description: "Discard".into(),
        };
        let mut seen = std::collections::BTreeSet::new();
        let mut chosen: Vec<ObjId> = match self.ask(player, &req) {
            DecisionResponse::Indices(idxs) => idxs
                .iter()
                .filter_map(|&i| hand.get(i as usize).copied())
                .filter(|o| seen.insert(*o))
                .take(n)
                .collect(),
            DecisionResponse::Index(i) => hand
                .get(i as usize)
                .copied()
                .filter(|o| seen.insert(*o))
                .into_iter()
                .collect(),
            _ => Vec::new(),
        };
        // Discard is mandatory: if the agent under-picked, fill from the front of the hand.
        for &o in &hand {
            if chosen.len() >= n {
                break;
            }
            if seen.insert(o) {
                chosen.push(o);
            }
        }
        let discarded = chosen.len();
        for card in chosen {
            let owner = self.state.object(card).owner;
            self.state.move_object(card, Zone::Graveyard, owner);
            self.broadcast(GameEvent::ObjectMoved { obj: card, to: Zone::Graveyard });
        }
        discarded
    }

    /// Surveil N (CR 701.42): show `pl` the top `n` cards of their library (top-first — the library's
    /// last element is the top), let them put any number into the graveyard, and leave the rest on
    /// top. The kept cards stay in their current order (surveil permits any order).
    fn interpret_surveil(&mut self, pl: PlayerId, n: usize) {
        let lib = &self.state.player(pl).library;
        let count = n.min(lib.len());
        if count == 0 {
            return;
        }
        let top: Vec<ObjId> = lib.iter().rev().take(count).copied().collect();
        let req = DecisionRequest::SelectCards {
            reason: SelectReason::ScryStage,
            from: top.clone(),
            min: 0,
            max: count as u32,
            description: "Surveil".into(),
        };
        let mut seen = std::collections::BTreeSet::new();
        let to_gy: Vec<ObjId> = match self.ask(pl, &req) {
            DecisionResponse::Indices(idxs) => idxs
                .iter()
                .filter_map(|&i| top.get(i as usize).copied())
                .filter(|o| seen.insert(*o))
                .collect(),
            _ => Vec::new(),
        };
        for card in to_gy {
            let owner = self.state.object(card).owner;
            self.state.move_object(card, Zone::Graveyard, owner);
            self.broadcast(GameEvent::ObjectMoved { obj: card, to: Zone::Graveyard });
        }
    }

    /// "Look at the top `n`, put `take` into `take_to`, the rest into `rest_to`" (SoS look-and-pick).
    /// The controller chooses which of the looked-at cards to take. `rest_to == Library` places the
    /// remainder on the **bottom** of the library.
    fn interpret_look_and_pick(
        &mut self,
        pl: PlayerId,
        n: usize,
        take: usize,
        take_to: Zone,
        rest_to: Zone,
        take_filter: &CardFilter,
    ) {
        let lib = &self.state.player(pl).library;
        let count = n.min(lib.len());
        if count == 0 {
            return;
        }
        // Top-first (the library's top is the vec's tail).
        let top: Vec<ObjId> = lib.iter().rev().take(count).copied().collect();
        // Only cards matching `take_filter` may be taken (Paradox Surveyor). A restrictive filter also
        // makes the take optional (min 0 — you may find nothing that qualifies).
        let any = matches!(take_filter, CardFilter::Any);
        let takeable: Vec<ObjId> =
            top.iter().copied().filter(|&o| self.count_filter_matches(o, take_filter)).collect();
        let take_n = take.min(takeable.len());
        let min = if any { take_n } else { 0 };
        let mut seen = std::collections::BTreeSet::new();
        let taken: Vec<ObjId> = if take_n == 0 {
            Vec::new()
        } else {
            let req = DecisionRequest::SelectCards {
                reason: SelectReason::ScryStage,
                from: takeable.clone(),
                min: min as u32,
                max: take_n as u32,
                description: "Look and pick".into(),
            };
            match self.ask(pl, &req) {
                DecisionResponse::Indices(idxs) => idxs
                    .iter()
                    .filter_map(|&i| takeable.get(i as usize).copied())
                    .filter(|o| seen.insert(*o))
                    .take(take_n)
                    .collect(),
                _ if any => takeable.iter().take(take_n).copied().collect(),
                _ => Vec::new(),
            }
        };
        // Move the taken cards to `take_to`.
        for card in &taken {
            let owner = self.state.object(*card).owner;
            self.state.move_object(*card, take_to, owner);
            self.broadcast(GameEvent::ObjectMoved { obj: *card, to: take_to });
        }
        // The rest — the looked-at cards not taken.
        let rest: Vec<ObjId> = top.iter().filter(|o| !taken.contains(o)).copied().collect();
        if rest_to == Zone::Library {
            // Put them on the bottom (bottom = front of the vec, since the top is the tail).
            let libv = &mut self.state.player_mut(pl).library;
            libv.retain(|o| !rest.contains(o));
            for card in rest.iter().rev() {
                libv.insert(0, *card);
            }
        } else {
            for card in &rest {
                let owner = self.state.object(*card).owner;
                self.state.move_object(*card, rest_to, owner);
                self.broadcast(GameEvent::ObjectMoved { obj: *card, to: rest_to });
            }
        }
    }

    /// Have `pl` sacrifice permanents matching `spec` (CR 701.17), choosing which of their own (up
    /// to `spec.max`, at least `spec.min` when able). The sacrificing player is always the chooser.
    /// Returns the number actually sacrificed (for the `IfYouDo`/"performed" flag).
    fn interpret_sacrifice(&mut self, pl: PlayerId, spec: &SelectSpec, ctx: &ResolutionCtx) -> usize {
        let candidates: Vec<ObjId> = self
            .state
            .player(pl)
            .battlefield
            .iter()
            .copied()
            .filter(|&id| self.count_filter_matches(id, &spec.filter))
            .collect();
        let max = self.eval_value(&spec.max, ctx).max(0) as usize;
        let want = max.min(candidates.len());
        if want == 0 {
            return 0;
        }
        let chosen: Vec<ObjId> = if candidates.len() <= want {
            candidates
        } else {
            let req = DecisionRequest::SelectCards {
                reason: SelectReason::Sacrifice,
                from: candidates.clone(),
                min: want as u32,
                max: want as u32,
                description: "Sacrifice".into(),
            };
            let mut seen = std::collections::BTreeSet::new();
            let mut picks: Vec<ObjId> = match self.ask(pl, &req) {
                DecisionResponse::Indices(idxs) => idxs
                    .iter()
                    .filter_map(|&i| candidates.get(i as usize).copied())
                    .filter(|o| seen.insert(*o))
                    .take(want)
                    .collect(),
                _ => Vec::new(),
            };
            // Sacrifice is mandatory (for a fixed count): fill from the front if the agent under-picked.
            for &o in &candidates {
                if picks.len() >= want {
                    break;
                }
                if seen.insert(o) {
                    picks.push(o);
                }
            }
            picks
        };
        let sacrificed = chosen.len();
        for obj in chosen {
            let owner = self.state.object(obj).owner;
            self.state.move_object(obj, Zone::Graveyard, owner);
            self.broadcast(GameEvent::ObjectMoved { obj, to: Zone::Graveyard });
        }
        sacrificed
    }

    /// Counter the stack object with id `sid` (CR 701.5): remove it from the stack; a countered
    /// **spell** goes to its owner's graveyard (701.5a), a countered **ability** simply ceases to
    /// exist. A spell that "can't be countered" (`CantBeCountered`, CR 701.5f — read from its
    /// computed characteristics, which now include stack-zone statics like Surrak's) is left on the
    /// stack untouched, so it will still resolve.
    fn interpret_counter(&mut self, sid: StackId) {
        let Some(so) = self.state.stack.items.iter().find(|s| s.id == sid).cloned() else {
            return;
        };
        if let crate::stack::StackObjectKind::Spell(card) = so.kind {
            if self
                .state
                .computed(card)
                .has_qualification(crate::effects::ability::Qualification::CantBeCountered)
            {
                return; // CR 701.5f — unaffected; stays on the stack.
            }
            self.state.stack.items.retain(|s| s.id != sid);
            let owner = self.state.object(card).owner;
            self.state.move_object(card, Zone::Graveyard, owner);
            self.broadcast(GameEvent::ObjectMoved { obj: card, to: Zone::Graveyard });
        } else {
            // An activated/triggered ability that is countered just leaves the stack (CR 701.5b).
            self.state.stack.items.retain(|s| s.id != sid);
        }
    }

    /// Select the objects a `ForEach`/`Select` ranges over: the `chooser`'s objects in `selector.zone`
    /// matching its filter, narrowed to `[min, max]` (asking which when there are more than `max`).
    /// Returns empty if fewer than `min` candidates exist (the "for each of two …" can't be met).
    fn select_for_each(&mut self, selector: &SelectSpec, ctx: &ResolutionCtx) -> Vec<ObjId> {
        let chooser = self.eval_player(selector.chooser, ctx);
        let min = self.eval_value(&selector.min, ctx).max(0) as usize;
        let max = self.eval_value(&selector.max, ctx).max(0) as usize;
        let candidates: Vec<ObjId> = self
            .state
            .player(chooser)
            .zone_ids(selector.zone)
            .iter()
            .copied()
            .filter(|&id| self.count_filter_matches(id, &selector.filter))
            .collect();
        if candidates.len() < min {
            return Vec::new();
        }
        let want = max.min(candidates.len());
        if candidates.len() <= want {
            return candidates;
        }
        let req = DecisionRequest::SelectCards {
            reason: SelectReason::Generic,
            from: candidates.clone(),
            min: want as u32,
            max: want as u32,
            description: "choose".into(),
        };
        let mut seen = std::collections::BTreeSet::new();
        let idxs: Vec<usize> = match self.ask(chooser, &req) {
            DecisionResponse::Indices(i) => i
                .iter()
                .map(|&x| x as usize)
                .filter(|&x| x < candidates.len() && seen.insert(x))
                .take(want)
                .collect(),
            _ => Vec::new(),
        };
        let idxs = if idxs.len() == want { idxs } else { (0..want).collect() };
        idxs.into_iter().map(|i| candidates[i]).collect()
    }

    /// C19: add a `ManaSpec`'s mana to `player`'s pool (CR 106.4). `produces` is fixed colours;
    /// `any_color` asks the player to pick. (The simplified payment path taps sources directly,
    /// so this is used by explicit mana-ability activation / ritual effects.)
    fn add_mana(&mut self, player: PlayerId, mana: &ManaSpec, ctx: &ResolutionCtx) {
        let mut changed = false;
        for (color, amount) in &mana.produces {
            let amt = self.eval_value(amount, ctx).max(0) as u32;
            if amt > 0 {
                *self.state.player_mut(player).mana_pool.amounts.entry(*color).or_insert(0) += amt;
                changed = true;
            }
        }
        if changed {
            // Live-view refresh so the client shows mana entering the pool as it resolves (#59/#62).
            self.broadcast(GameEvent::ManaPoolChanged { player });
        }
        if let Some(amount) = &mana.any_color {
            let amt = self.eval_value(amount, ctx).max(0) as u32;
            if amt > 0 {
                let all =
                    vec![Color::White, Color::Blue, Color::Black, Color::Red, Color::Green];
                let resp = self.ask(
                    player,
                    &DecisionRequest::ChooseColor { allowed: all.clone(), min: 1, max: 1 },
                );
                let color = match resp {
                    DecisionResponse::Indices(v) => {
                        v.first().and_then(|&i| all.get(i as usize)).copied().unwrap_or(Color::White)
                    }
                    _ => Color::White,
                };
                *self.state.player_mut(player).mana_pool.amounts.entry(color).or_insert(0) += amt;
            }
        }
    }

    /// The `ObjId`s in one of a player's zones (for selection enumeration).
    fn zone_cards(&self, p: PlayerId, zone: Zone) -> Vec<ObjId> {
        let pl = self.state.player(p);
        match zone {
            Zone::Library => pl.library.clone(),
            Zone::Hand => pl.hand.clone(),
            Zone::Graveyard => pl.graveyard.clone(),
            Zone::Exile => pl.exile.clone(),
            _ => Vec::new(),
        }
    }

    /// Ask the controller to choose `min..=max` of a modal spell/ability's modes (CR 700.2),
    /// returning the chosen mode indices (clamped to the legal set / filled to `min`).
    pub(crate) fn choose_modes(
        &mut self,
        ctx: &ResolutionCtx,
        sid: StackId,
        modes: &[Mode],
        min: u32,
        max: u32,
        allow_repeat: bool,
    ) -> Vec<u32> {
        // Modes already chosen at cast/activation (CR 601.2b / 700.2) — use those, don't re-ask.
        // (Targets were collected for exactly these modes; re-asking could desync them.)
        if !ctx.chosen_modes.is_empty() {
            return ctx.chosen_modes.iter().copied().filter(|&i| (i as usize) < modes.len()).collect();
        }
        let controller = ctx.controller.unwrap_or(PlayerId(0));
        // CR 601.2b / 700.2d: a player may only choose a mode whose targets can be legally chosen.
        // Offer just the legal modes (an untargeted mode is always legal); without this the engine
        // would let a player pick e.g. Bushwhack's "fight" mode with no creatures in play and then
        // emit a `ChooseTargets` with zero legal candidates (CR 601.2c violation, #49). The agent
        // picks among the offered options; we map its choice back to original mode indices.
        let legal: Vec<u32> =
            (0..modes.len() as u32).filter(|&i| self.mode_is_legal(&modes[i as usize], controller)).collect();
        let options: Vec<ModeOption> =
            legal.iter().map(|&i| ModeOption { label: modes[i as usize].label.clone() }).collect();
        let resp = self.ask(
            controller,
            &DecisionRequest::ChooseModes {
                for_action: ActionRef(sid),
                modes: options,
                min,
                max,
                allow_repeat,
            },
        );
        let raw: Vec<u32> = match resp {
            DecisionResponse::Indices(v) => v,
            DecisionResponse::Index(i) => vec![i],
            _ => Vec::new(),
        };
        // Responses index into the offered (legal) list — map back to original mode indices.
        let mut chosen: Vec<u32> = raw.into_iter().filter_map(|i| legal.get(i as usize).copied()).collect();
        if !allow_repeat {
            chosen.sort_unstable();
            chosen.dedup();
        }
        chosen.truncate(max as usize);
        // Fill up to `min` with the first unused LEGAL modes so a malformed/empty response can't
        // under-resolve a "choose one" (CR 700.2d — you must choose the minimum, from legal modes).
        while (chosen.len() as u32) < min {
            match legal.iter().copied().find(|i| !chosen.contains(i)) {
                Some(i) => chosen.push(i),
                None => break,
            }
        }
        chosen
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
            Effect::Destroy { what } => {
                if let Some(Target::Object(obj)) = self.resolve_target(what, ctx, cursor) {
                    wb.push(Action::Destroy {
                        obj,
                        source: ctx.source,
                    });
                }
            }
            // C17: exile a target (e.g. "{1}: Exile target card from a graveyard"). `source` is
            // carried so the exile can later be associated with its source (linked-exile sets).
            Effect::Exile { what } => {
                if let Some(Target::Object(obj)) = self.resolve_target(what, ctx, cursor) {
                    wb.push(Action::Exile { obj, source: ctx.source });
                }
            }
            // Move a single targeted object to another zone (CR 400.7 / 608.2) — "return target
            // permanent to its owner's hand" (bounce), "return target creature card from your
            // graveyard to the battlefield" (reanimate), etc. Lowers to one `Action::MoveZone` with
            // `MoveCause::Returned` (a non-death leave, so LTB — not dies — triggers fire, and an
            // enter fires ETB). Single-object only for now (one `target` word → one chosen slot).
            Effect::MoveZone { what, to } => {
                if let Some(Target::Object(obj)) = self.resolve_target(what, ctx, cursor) {
                    wb.push(Action::MoveZone {
                        obj,
                        to: to.zone,
                        pos: to.pos,
                        cause: MoveCause::Returned,
                    });
                }
            }
            Effect::Attach { what, to } => {
                // `what` (usually SourceSelf) is resolved first; `to` consumes the chosen target.
                let attachment = self.resolve_target(what, ctx, cursor);
                let target = self.resolve_target(to, ctx, cursor);
                if let (Some(Target::Object(attachment)), Some(target)) = (attachment, target) {
                    wb.push(Action::AttachTo { attachment, target });
                }
            }
            // C2: put N counters on a single object (e.g. "put a +1/+1 counter on this").
            // `what` is usually SourceSelf; Select-based "each creature you control" comes with
            // ForEach later.
            Effect::PutCounters { what, kind, n } => {
                let n = self.eval_value(n, ctx) as i32;
                if let Some(Target::Object(obj)) = self.resolve_target(what, ctx, cursor) {
                    wb.push(Action::AddCounters { obj, kind: kind.clone(), n });
                }
            }
            // C3: mill — put the top N cards of a library into its owner's graveyard (CR 701.13).
            Effect::Mill { who, count } => {
                let count = self.eval_value(count, ctx).max(0) as u32;
                wb.push(Action::Mill {
                    player: self.eval_player(*who, ctx),
                    count,
                });
            }
            // C8: two creatures fight (CR 701.12) — each deals damage equal to its power to the
            // other, simultaneously (both Damage actions in this one whiteboard, so deathtouch /
            // lethal interact). `a`/`b` are usually ChosenIndex (the spell's two locked targets).
            Effect::Fight { a, b } => {
                let oa = self.resolve_target(a, ctx, cursor);
                let ob = self.resolve_target(b, ctx, cursor);
                if let (Some(Target::Object(ca)), Some(Target::Object(cb))) = (oa, ob) {
                    let pa = self.state.computed(ca).power.unwrap_or(0).max(0) as u32;
                    let pb = self.state.computed(cb).power.unwrap_or(0).max(0) as u32;
                    wb.push(Action::Damage {
                        target: Target::Object(cb),
                        amount: pa,
                        source: ca,
                        kind: DamageKind::Noncombat,
                    });
                    wb.push(Action::Damage {
                        target: Target::Object(ca),
                        amount: pb,
                        source: cb,
                        kind: DamageKind::Noncombat,
                    });
                }
            }
            // C12: Earthbend N — the chosen land becomes a 0/0 creature with haste that's still a
            // land (a resolution-granted continuous effect, CR 611) and gets N +1/+1 counters.
            // The companion delayed "dies/exiled → return tapped" trigger is registered separately.
            Effect::Earthbend { target, n } => {
                let n = self.eval_value(n, ctx).max(0);
                if let Some(Target::Object(land)) = self.resolve_target(target, ctx, cursor) {
                    let controller = ctx.controller.unwrap_or(PlayerId(0));
                    wb.push(Action::GrantContinuous {
                        source: ctx.source,
                        controller,
                        affected: vec![land],
                        contributions: vec![
                            StaticContribution::AddType(CardType::Creature),
                            StaticContribution::SetBasePT { power: 0, toughness: 0 },
                            StaticContribution::GrantKeyword(Keyword::Haste),
                        ],
                        duration: Duration::Permanent,
                    });
                    if n > 0 {
                        wb.push(Action::AddCounters {
                            obj: land,
                            kind: CounterKind::PlusOnePlusOne,
                            n: n as i32,
                        });
                    }
                    // The delayed clause (CR 603.7): "when it dies or is exiled, return it to the
                    // battlefield tapped." Concrete actions — move it back, then tap it.
                    wb.push(Action::RegisterDelayedTrigger {
                        watching: land,
                        event: DelayedTriggerEvent::DiesOrExiled,
                        controller,
                        source: ctx.source,
                        actions: vec![
                            Action::MoveZone {
                                obj: land,
                                to: Zone::Battlefield,
                                pos: ZonePos::Any,
                                cause: MoveCause::Returned,
                            },
                            Action::TapUntap { obj: land, tap: true },
                        ],
                    });
                }
            }
            // C15: pump a creature's P/T for a duration (CR 611) — "gets +X/+Y until end of turn".
            // A P/T change is inherently continuous, so it lowers to a floating ModifyPT effect
            // over the target. `power`/`toughness` are snapshotted now (e.g. PowerOfTarget for
            // "double its power"); the layer system applies them in 7c.
            Effect::PumpPT { what, power, toughness, duration } => {
                if let Some(Target::Object(obj)) = self.resolve_target(what, ctx, cursor) {
                    let p = self.eval_value(power, ctx) as i32;
                    let t = self.eval_value(toughness, ctx) as i32;
                    if p != 0 || t != 0 {
                        let controller = ctx.controller.unwrap_or(PlayerId(0));
                        wb.push(Action::GrantContinuous {
                            source: ctx.source,
                            controller,
                            affected: vec![obj],
                            contributions: vec![StaticContribution::ModifyPT {
                                power: p,
                                toughness: t,
                            }],
                            duration: *duration,
                        });
                    }
                }
            }
            // Tap or untap a permanent (e.g. Fabled Passage's "untap that land").
            Effect::Tap { what, tap } => {
                if let Some(Target::Object(obj)) = self.resolve_target(what, ctx, cursor) {
                    wb.push(Action::TapUntap { obj, tap: *tap });
                }
            }
            // Grant a keyword for a duration (CR 611) — "it gains trample until end of turn".
            Effect::GrantKeyword { what, keyword, duration } => {
                if let Some(Target::Object(obj)) = self.resolve_target(what, ctx, cursor) {
                    let controller = ctx.controller.unwrap_or(PlayerId(0));
                    wb.push(Action::GrantContinuous {
                        source: ctx.source,
                        controller,
                        affected: vec![obj],
                        contributions: vec![StaticContribution::GrantKeyword(*keyword)],
                        duration: *duration,
                    });
                }
            }
            // A crewed Vehicle becomes a creature for a duration (CR 702.122) — AddType(Creature).
            Effect::BecomeCreature { what, duration } => {
                if let Some(Target::Object(obj)) = self.resolve_target(what, ctx, cursor) {
                    let controller = ctx.controller.unwrap_or(PlayerId(0));
                    wb.push(Action::GrantContinuous {
                        source: ctx.source,
                        controller,
                        affected: vec![obj],
                        contributions: vec![StaticContribution::AddType(CardType::Creature)],
                        duration: *duration,
                    });
                }
            }
            // Paint a qualification for a duration — "can't be blocked this turn" (Escape Tunnel).
            Effect::GrantQualification { what, qualification, duration } => {
                if let Some(Target::Object(obj)) = self.resolve_target(what, ctx, cursor) {
                    let controller = ctx.controller.unwrap_or(PlayerId(0));
                    wb.push(Action::GrantContinuous {
                        source: ctx.source,
                        controller,
                        affected: vec![obj],
                        contributions: vec![StaticContribution::Qualification(*qualification)],
                        duration: *duration,
                    });
                }
            }
            // Intervening-"if" (CR 603.4) / conditional effect: run `then` when the condition holds
            // (evaluated source-aware), else `otherwise`. A *targeted* `then` is a reflexive trigger
            // (CR 603.7c): its target is chosen only if/when the condition is met, so it's deferred
            // to a reflexive sub-trigger that goes on the stack (`RegisterReflexive`) rather than
            // resolving inline. A non-targeted `then` resolves inline.
            Effect::Conditional { cond, then, otherwise } => {
                let controller = ctx.controller.unwrap_or(PlayerId(0));
                // A targeted `then` inside an ability is a reflexive trigger (603.7c): defer the
                // WHOLE conditional — its `cond` is re-checked and its target chosen on the
                // sub-trigger, AFTER this resolution's other actions (e.g. the quest counter) commit.
                let reflexive = ctx
                    .source
                    .zip(ctx.ability_index)
                    .filter(|_| !crate::priority::collect_target_specs(then).is_empty());
                if let Some((source, ability_index)) = reflexive {
                    wb.push(Action::RegisterReflexive { source, ability_index, controller });
                } else if self.cond_holds(cond, ctx) {
                    self.materialize(then, ctx, wb, cursor);
                } else if let Some(otherwise) = otherwise {
                    self.materialize(otherwise, ctx, wb, cursor);
                }
            }
            // C6: create N copies of a token (CR 111).
            Effect::CreateToken { spec, count, controller } => {
                let count = self.eval_value(count, ctx).max(0) as u32;
                let controller = self.eval_player(*controller, ctx);
                for _ in 0..count {
                    wb.push(Action::CreateToken {
                        spec: spec.clone(),
                        controller,
                    });
                }
            }
            // C6-copy: create a token that's a copy of a permanent (CR 707.9e / 111.3). Snapshot the
            // source's copiable characteristics (its base `chars` — NOT counters/damage/auras/other
            // continuous effects, CR 707.2) into a `TokenSpec` (abilities ride along via the copied
            // `grp_id`), apply the `mods` "except" overrides, then create it like any other token.
            Effect::CreateTokenCopy { source, controller, mods } => {
                let controller = self.eval_player(*controller, ctx);
                if let Some(Target::Object(obj)) = self.resolve_target(source, ctx, cursor) {
                    if let Some(src) = self.state.objects.get(&obj) {
                        let c = &src.chars;
                        let mut spec = TokenSpec {
                            name: c.name.clone(),
                            card_types: c.card_types.clone(),
                            subtypes: c.subtypes.clone(),
                            colors: c.colors.clone(),
                            power: c.power.unwrap_or(0),
                            toughness: c.toughness.unwrap_or(0),
                            keywords: c.keywords.clone(),
                            counters: Vec::new(),
                            // The copy's abilities come from the copied permanent's def (CR 707.2).
                            grp_id: c.grp_id,
                        };
                        for t in &mods.add_card_types {
                            if !spec.card_types.contains(t) {
                                spec.card_types.push(t.clone());
                            }
                        }
                        for s in &mods.add_subtypes {
                            if !spec.subtypes.contains(s) {
                                spec.subtypes.push(s.clone());
                            }
                        }
                        if let Some((p, t)) = mods.set_power_toughness {
                            spec.power = p;
                            spec.toughness = t;
                        }
                        for (kind, n) in &mods.counters {
                            let n = self.eval_value(n, ctx).max(0) as u32;
                            spec.counters.push((kind.clone(), n));
                        }
                        wb.push(Action::CreateToken { spec, controller });
                    }
                }
            }
            // "Target player" declaration (CR 115.1): no action — it just consumes its target slot so
            // later `Target(...)` slots line up. The player was chosen at cast (a `Player` spec) and is
            // read by the following effects via `PlayerRef::ChosenTarget`.
            Effect::TargetPlayer => {
                *cursor += 1;
            }
            // ── Leaves defined in the IR but not yet given a whiteboard runtime. These fail
            // LOUD in debug/tests so a card using one can never silently no-op — the exact bug
            // class that hid Traumatic Critique's "then discard a card" (a defined-but-unwired
            // leaf that vanished). Release builds degrade to a no-op rather than crash a live
            // game (`debug_assert!` compiles out). As each leaf is wired it gets a real arm
            // above and leaves this list. **The match is exhaustive by design (no wildcard):**
            // a NEW `Effect` variant added without an interpreter arm is a *compile* error
            // here, not a silent gap.
            Effect::Repeat { .. } | Effect::Distribute { .. } | Effect::Native { .. } => {
                debug_assert!(false, "uninterpreted Effect leaf in materialize(): {effect:?}");
            }
            // Control-flow / interactive nodes are driven by `interpret` (which asks the agent);
            // they reach `materialize` only when nested where no interpreter runs (e.g. a
            // `Conditional`/`Sequence` `then`). Inert here — `interpret` handled the top level.
            Effect::Modal { .. }
            | Effect::Optional { .. }
            | Effect::IfYouDo { .. }
            | Effect::ForEach { .. }
            | Effect::Search { .. }
            | Effect::AddMana { .. }
            | Effect::Discard { .. }
            | Effect::Counter { .. }
            | Effect::Sacrifice { .. }
            | Effect::Surveil { .. }
            | Effect::LookAndPick { .. }
            | Effect::Nothing => {}
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
        let mut iters = 0usize;
        loop {
            // Safety ceiling (#55): this fixpoint runs below the priority/agenda loops, so it carries
            // its own guard — a pathological replacement chain can't wedge resolution.
            if self.loop_guard_tripped(
                iters,
                crate::priority::REWRITE_LOOP_LIMIT,
                "rewrite (replacement/prevention fixpoint)",
            ) {
                return;
            }
            iters += 1;
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

    /// Apply one rewrite to the staged actions (CR 614.1). `&mut self` because some rewrites are
    /// interactive (a shock land asks the controller whether to pay life as it enters).
    fn apply_rewrite(&mut self, rw: &Rewrite, wb: &mut Whiteboard, ai: usize, obj: ObjId) {
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
            // Dynamic count: evaluate `n` as the object enters, with the entering object as source
            // (so `ValueExpr::ManaSpent` reads what was paid to cast it) and the resolution's `x`
            // carried through (so "enters with X +1/+1 counters" reads the chosen X — Pterafractyl).
            // CR 614.1e.
            Rewrite::EntersWithCountersValue { kind, n } => {
                let ctx = ResolutionCtx { source: Some(obj), x: wb.ctx.x, ..Default::default() };
                let count = self.eval_value(n, &ctx).max(0) as i32;
                wb.actions.insert(
                    ai + 1,
                    Action::AddCounters { obj, kind: kind.clone(), n: count },
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
            Rewrite::EntersTappedUnless(cond) => {
                // "Enters tapped unless <condition>" (check lands): tap iff the condition fails,
                // evaluated for the entering permanent's controller. No choice.
                let controller =
                    self.state.objects.get(&obj).map(|o| o.controller).unwrap_or(PlayerId(0));
                if !crate::conditions::holds(&self.state, cond, controller) {
                    wb.actions.insert(ai + 1, Action::TapUntap { obj, tap: true });
                }
            }
            Rewrite::EntersTappedUnlessPay { life } => {
                // "You may pay N life; if you don't, it enters tapped" (shock lands): ask the
                // controller as it enters — pay → lose the life (untapped); decline → tapped.
                let controller =
                    self.state.objects.get(&obj).map(|o| o.controller).unwrap_or(PlayerId(0));
                let paid = matches!(
                    self.ask(controller, &DecisionRequest::Confirm { kind: ConfirmKind::PayToPrevent }),
                    DecisionResponse::Bool(true)
                );
                if paid {
                    wb.actions
                        .insert(ai + 1, Action::LoseLife { player: controller, amount: *life });
                } else {
                    wb.actions.insert(ai + 1, Action::TapUntap { obj, tap: true });
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
            Action::GainLife { player, amount } => {
                self.change_life(player, amount as i32);
                // Track life gained this turn (CR 118.9) for the "Infusion" condition.
                if let Some(p) = self.state.players.get_mut(player.0 as usize) {
                    p.life_gained_this_turn = p.life_gained_this_turn.saturating_add(amount);
                }
            }
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
                // Indestructible (CR 702.12): can't be destroyed.
                if self
                    .state
                    .computed(obj)
                    .has_keyword(crate::effects::ability::Keyword::Indestructible)
                {
                    return;
                }
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
            Action::Mill { player, count } => self.mill(player, count),
            Action::CreateToken { spec, controller } => self.create_token(&spec, controller),
            Action::AttachTo { attachment, target: Target::Object(host) } => {
                // Move `attachment` (an Aura/Equipment) onto `host` (CR 701.3). Re-attaching
                // simply overwrites the old host. Marks chars dirty so the "while attached"
                // static (AttachedHost) recomputes.
                if self.state.objects.contains_key(&host) {
                    if let Some(o) = self.state.objects.get_mut(&attachment) {
                        o.attached_to = Some(host);
                    }
                    self.state.mark_chars_dirty();
                }
            }
            Action::Exile { obj, source } => {
                let owner = match self.state.objects.get(&obj) {
                    Some(o) => o.owner,
                    None => return,
                };
                if self.state.move_object(obj, Zone::Exile, owner) {
                    // Record which permanent exiled it (move_object cleared the field) — Keen-Eyed's
                    // "cards exiled with this creature".
                    if let Some(o) = self.state.objects.get_mut(&obj) {
                        o.exiled_with = source;
                    }
                    self.broadcast(GameEvent::ObjectMoved { obj, to: Zone::Exile });
                }
            }
            Action::WarpExile { obj } => {
                let owner = match self.state.objects.get(&obj) {
                    Some(o) => o.owner,
                    None => return,
                };
                if self.state.move_object(obj, Zone::Exile, owner) {
                    // Warp grants recast-from-exile permission (CR 702.x).
                    if let Some(o) = self.state.objects.get_mut(&obj) {
                        o.castable_from_exile = true;
                    }
                    self.broadcast(GameEvent::ObjectMoved { obj, to: Zone::Exile });
                }
            }
            Action::GrantContinuous { source, controller, affected, contributions, duration } => {
                // Register a resolution-granted continuous effect (CR 611). The layer system folds
                // it in; `add_continuous_effect` marks the chars cache dirty.
                self.state
                    .add_continuous_effect(source, controller, affected, contributions, duration);
            }
            Action::RegisterReflexive { source, ability_index, controller } => {
                // Queue a reflexive "when you do" sub-trigger (CR 603.7c). It goes on the stack the
                // next time a player would get priority; its target is chosen there.
                let id = self.state.mint_stack();
                self.state.pending_triggers.push(crate::stack::StackObject {
                    id,
                    controller,
                    source: Some(source),
                    kind: crate::stack::StackObjectKind::ReflexiveAbility { source, ability_index },
                    targets: Vec::new(),
                    x: None,
                    modes: Vec::new(),
                });
            }
            Action::RegisterDelayedTrigger { watching, event, controller, source, actions } => {
                // Arm a delayed triggered ability (CR 603.7); the engine fires it when `watching`
                // leaves the battlefield matching `event`.
                self.state
                    .register_delayed_trigger(watching, event, controller, source, actions);
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
                let (is_bf_creature, is_bf_pw) = self
                    .state
                    .objects
                    .get(&o)
                    .map(|x| {
                        let bf = x.zone == Zone::Battlefield;
                        (
                            bf && x.chars.is_creature(),
                            bf && x.chars.card_types.contains(&CardType::Planeswalker),
                        )
                    })
                    .unwrap_or((false, false));
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
                } else if is_bf_pw {
                    // CR 120.3 / 306.8: damage to a planeswalker removes that many loyalty
                    // counters; the 0-loyalty SBA (704.5i) handles its death.
                    if let Some(x) = self.state.objects.get_mut(&o) {
                        let cur = x.counters.counts.entry(CounterKind::Loyalty).or_insert(0);
                        *cur = cur.saturating_sub(amount);
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

    /// C3: mill `count` cards from `player`'s library into their graveyard (CR 701.13). The top
    /// of the library is the last element; milling an empty library simply stops (milling is not
    /// drawing, so it never triggers the draw-from-empty loss).
    pub(crate) fn mill(&mut self, player: PlayerId, count: u32) {
        for _ in 0..count {
            let top = match self.state.player(player).library.last().copied() {
                Some(c) => c,
                None => break,
            };
            if self.state.move_object(top, Zone::Graveyard, player) {
                self.broadcast(GameEvent::ObjectMoved { obj: top, to: Zone::Graveyard });
            }
        }
    }

    /// C6: put a token onto the battlefield from its [`TokenSpec`] (CR 111.3). A token has no
    /// printing (`grp_id` 0) — its characteristics live entirely on the object, including its
    /// printed keyword abilities (`TokenSpec.keywords`, e.g. an Inkling's Flying).
    fn create_token(&mut self, spec: &TokenSpec, controller: PlayerId) {
        let chars = Characteristics {
            name: spec.name.clone(),
            card_types: spec.card_types.clone(),
            subtypes: spec.subtypes.clone(),
            colors: spec.colors.clone(),
            power: Some(spec.power),
            toughness: Some(spec.toughness),
            keywords: spec.keywords.clone(),
            // A registered token def (reserved 9000+ block) supplies the token's triggered/activated
            // abilities via `def_of`; `0` = vanilla/keyword-only.
            grp_id: spec.grp_id,
            ..Default::default()
        };
        let is_creature = chars.is_creature();
        let id = self.state.add_card(controller, chars, Zone::Battlefield);
        if let Some(o) = self.state.objects.get_mut(&id) {
            // A token creature enters summoning sick (CR 302.6); `add_card` doesn't infer this.
            o.summoning_sick = is_creature;
            for (kind, n) in &spec.counters {
                *o.counters.counts.entry(kind.clone()).or_insert(0) += n;
            }
        }
        self.state.mark_chars_dirty();
        self.broadcast(GameEvent::ObjectMoved { obj: id, to: Zone::Battlefield });
    }

    // ── IR resolution helpers ─────────────────────────────────────────────────────────────

    /// Evaluate a `Condition` **with the resolution context** (so `ValueExpr`s like
    /// `ManaSpentOnTrigger` / `X` that only make sense at resolution resolve correctly). `ValueAtLeast`
    /// and the boolean combinators are handled here via `eval_value`; every other condition delegates
    /// to the state-only [`crate::conditions::holds_for_source`].
    fn cond_holds(&self, cond: &Condition, ctx: &ResolutionCtx) -> bool {
        use crate::effects::condition::Condition as C;
        match cond {
            C::All(cs) => cs.iter().all(|c| self.cond_holds(c, ctx)),
            C::AnyOf(cs) => cs.iter().any(|c| self.cond_holds(c, ctx)),
            C::Not(c) => !self.cond_holds(c, ctx),
            C::ValueAtLeast(a, b) => self.eval_value(a, ctx) >= self.eval_value(b, ctx),
            other => crate::conditions::holds_for_source(
                &self.state,
                other,
                ctx.controller.unwrap_or(PlayerId(0)),
                ctx.source,
            ),
        }
    }

    pub(crate) fn eval_value(&self, v: &ValueExpr, ctx: &ResolutionCtx) -> i64 {
        match v {
            ValueExpr::Fixed(n) => *n,
            ValueExpr::X => ctx.x.unwrap_or(0) as i64,
            ValueExpr::XTimes(k) => k * ctx.x.unwrap_or(0) as i64,
            ValueExpr::NumTargets => ctx.chosen_targets.len() as i64,
            ValueExpr::Sum(a, b) => self.eval_value(a, ctx) + self.eval_value(b, ctx),
            // C9: count objects in a zone matching the filter, optionally restricted to a
            // player's permanents (e.g. "the number of lands you control").
            ValueExpr::Count { zone, filter, controller } => {
                let who = controller.map(|r| self.eval_player(r, ctx));
                self.state
                    .objects
                    .values()
                    .filter(|o| o.zone == *zone)
                    .filter(|o| who.is_none_or(|p| o.controller == p))
                    .filter(|o| self.count_filter_matches(o.id, filter))
                    .count() as i64
            }
            // C9b: the number of `kind` counters on the effect's source (e.g. Mossborn Hydra
            // doubling its own +1/+1 counters). For a CDA computing P/T, chars evaluates this
            // against the object being computed (see chars::compute) — here it's the resolver.
            ValueExpr::CountersOnSelf(kind) => ctx
                .source
                .and_then(|s| self.state.objects.get(&s))
                .map(|o| o.counters.get(kind) as i64)
                .unwrap_or(0),
            // The computed power/toughness of the source itself (Increment's stat comparison).
            ValueExpr::PowerOfSelf => ctx
                .source
                .map(|s| self.state.computed(s).power.unwrap_or(0) as i64)
                .unwrap_or(0),
            ValueExpr::ToughnessOfSelf => ctx
                .source
                .map(|s| self.state.computed(s).toughness.unwrap_or(0) as i64)
                .unwrap_or(0),
            // C15: the computed power of the Nth chosen target, read once at resolution (608.2h).
            ValueExpr::PowerOfTarget(n) => match ctx.chosen_targets.get(*n as usize) {
                Some(Target::Object(id)) => self.state.computed(*id).power.unwrap_or(0) as i64,
                _ => 0,
            },
            // The mana spent to cast the source object (recorded at cast, CR 601.2f–h) — Dyadrine.
            ValueExpr::ManaSpent => ctx
                .source
                .and_then(|s| self.state.objects.get(&s))
                .map(|o| o.mana_spent as i64)
                .unwrap_or(0),
            // The number of distinct colours of mana spent to cast the source — Converge (Archaic).
            ValueExpr::ColorsSpent => ctx
                .source
                .and_then(|s| self.state.objects.get(&s))
                .map(|o| o.colors_spent as i64)
                .unwrap_or(0),
            // The mana spent to cast the triggering spell of a "whenever you cast …" ability — Opus.
            ValueExpr::ManaSpentOnTrigger => ctx
                .triggering_spell
                .and_then(|s| self.state.objects.get(&s))
                .map(|o| o.mana_spent as i64)
                .unwrap_or(0),
            // Distinct colours of mana spent to cast the triggering spell — Converge on a cast-trigger
            // (Magmablood Archaic).
            ValueExpr::ColorsSpentOnTrigger => ctx
                .triggering_spell
                .and_then(|s| self.state.objects.get(&s))
                .map(|o| o.colors_spent as i64)
                .unwrap_or(0),
            // Distinct card types among cards exiled with the source — Keen-Eyed Curator.
            ValueExpr::DistinctCardTypesAmongExiledWith => {
                crate::conditions::distinct_card_types_among_exiled_with(&self.state, ctx.source)
            }
        }
    }

    /// Evaluate a `CardFilter` against a single object's computed characteristics, for the subset
    /// `ValueExpr::Count` needs (`ControlledBy` is handled by Count's `controller` restriction).
    fn count_filter_matches(&self, id: ObjId, filter: &CardFilter) -> bool {
        let cc = self.state.computed(id);
        match filter {
            CardFilter::Any => true,
            CardFilter::HasCardType(t) => cc.card_types.contains(t),
            CardFilter::HasSubtype(s) => cc.subtypes.contains(s),
            CardFilter::HasColor(c) => cc.colors.contains(c),
            CardFilter::Colorless => cc.colors.is_empty(),
            // Supertype (Basic/Legendary/Snow) reads base chars — not a layered characteristic.
            CardFilter::Supertype(s) => {
                self.state.objects.get(&id).is_some_and(|o| o.chars.supertypes.contains(s))
            }
            CardFilter::HasCounter(kind) => {
                self.state.objects.get(&id).is_some_and(|o| o.counters.get(kind) > 0)
            }
            // The enumeration scope already restricts by controller (a `Count`'s controller
            // restriction / a `ForEach`'s chooser), so a `ControlledBy` in the filter is redundant.
            CardFilter::ControlledBy(_) => true,
            // Name equality (CR 201) — e.g. "cards named Ancestral Anger in your graveyard". Name is a
            // base characteristic (read from the object's chars, like `Supertype`).
            CardFilter::Named(name) => {
                self.state.objects.get(&id).is_some_and(|o| o.chars.name == *name)
            }
            // A card with `{X}` in its printed cost (Paradox Surveyor).
            CardFilter::HasXInCost => self
                .state
                .objects
                .get(&id)
                .and_then(|o| o.chars.mana_cost.as_ref())
                .is_some_and(|mc| mc.x > 0),
            CardFilter::PowerAtMost(n) => cc.power.unwrap_or(0) <= *n,
            CardFilter::ManaValue { min, max } => {
                let mv = self.state.objects.get(&id).map_or(0, |o| o.chars.mana_value());
                min.is_none_or(|lo| mv >= lo) && max.is_none_or(|hi| mv <= hi)
            }
            CardFilter::Tapped => self.state.objects.get(&id).is_some_and(|o| o.status.tapped),
            CardFilter::Untapped => self.state.objects.get(&id).is_some_and(|o| !o.status.tapped),
            CardFilter::All(fs) => fs.iter().all(|f| self.count_filter_matches(id, f)),
            CardFilter::AnyOf(fs) => fs.iter().any(|f| self.count_filter_matches(id, f)),
            CardFilter::Not(f) => !self.count_filter_matches(id, f),
            // `ItSelf`/`AttachedHost` resolve against the effect's source/attachment, which a bare
            // `Count`/`ForEach` enumeration doesn't carry — no such filter is used in that context, so
            // treat as no match. Exhaustive by design (no wildcard): a NEW `CardFilter` without an arm
            // is a compile error here, not a silent `false`.
            CardFilter::ItSelf | CardFilter::AttachedHost => false,
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
            // The controller of the Nth object target, snapshotted at resolution start (so it
            // survives that object leaving play this resolution — CR 608.2). Falls back to the
            // effect's controller if the snapshot is missing.
            PlayerRef::ControllerOfTarget(n) => ctx
                .target_controllers
                .get(n as usize)
                .copied()
                .flatten()
                .unwrap_or(controller),
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
            EffectTarget::Searched(n) => {
                self.searched_this_resolution.get(*n as usize).copied().map(Target::Object)
            }
            EffectTarget::Each => self.foreach_current.map(Target::Object),
            EffectTarget::Player(who) => Some(Target::Player(self.eval_player(*who, ctx))),
            EffectTarget::SourceSelf => ctx.source.map(Target::Object),
            EffectTarget::Select(_) => None,
        }
    }
}
