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
    ActionRef, CastVariant, ConfirmKind, DecisionRequest, DecisionResponse, GameEvent, ModeOption,
    OptionLabel, OptionReason, ReplacementOption, SelectReason,
};
use crate::basics::{CardType, Color, CounterKind, DamageKind, Target, Zone, ZoneDest, ZonePos};
use crate::effects::ability::{
    Ability, ActionPattern, FloatingRewrite, Keyword, Rewrite, StaticContribution,
};
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
    /// `Some(i)` if this comes from `GameState.floating_replacements[i]` (a resolution-created
    /// floating rider) rather than a printed static; the loop removes a one-shot floating on apply.
    floating_idx: Option<usize>,
}

/// The object an action's outcome lands on (for finding applicable replacement effects).
fn affected_object(action: &Action) -> Option<ObjId> {
    match action {
        Action::MoveZone { obj, to: Zone::Battlefield, .. } => Some(*obj),
        Action::Damage { target: Target::Object(o), .. } => Some(*o),
        Action::AddCounters { obj, .. } => Some(*obj),
        // Death actions (CR 700.4 — battlefield→graveyard): destruction, sacrifice, and a direct
        // "put into its owner's graveyard" move. The `WouldDie` pattern (which enforces that the
        // object is on the battlefield) filters non-death graveyard moves like mill/discard.
        Action::Destroy { obj, .. } => Some(*obj),
        Action::Sacrifice { obj, .. } => Some(*obj),
        Action::MoveZone { obj, to: Zone::Graveyard, .. } => Some(*obj),
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
        Rewrite::ExileInstead => "exile instead of dying".to_string(),
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
        self.discarded_this_resolution.clear();
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

    /// The distinct land-card names currently present in the game, in a deterministic (sorted) order —
    /// the engine-enumerated option list for "choose a land card name" (Petrified Hamlet). A "land
    /// card" is any object whose printed card type includes Land (CR 305), across every zone. Empty if
    /// no land cards exist (then the choice is skipped).
    fn land_card_names(&self) -> Vec<String> {
        let mut names: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for o in self.state.objects.values() {
            if o.chars.card_types.contains(&CardType::Land) {
                names.insert(o.chars.name.clone());
            }
        }
        names.into_iter().collect()
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
            // Spree (CR 702.163) — the modes were chosen (and paid for) at cast; resolve each chosen
            // mode's effect in order, reading `ctx.chosen_modes` (the same set `choose_modes` reads).
            // Each mode's targets are ITS OWN (CR 700.2): give every chosen mode a mode-local ctx whose
            // `chosen_targets`/`target_controllers` are just that mode's slice of the spell's targets,
            // with a fresh cursor — so a mode's `Target`/`ChosenTarget(n)`/`TargetPlayer` references are
            // relative to the mode, not to the whole spell (else mode 0's target would shift mode 2's
            // `ChosenTarget(0)`). Targets were collected in this same chosen-mode order at cast.
            Effect::Spree { modes } => {
                let mut any = false;
                let mut offset = 0usize;
                for &idx in &ctx.chosen_modes {
                    if let Some(m) = modes.get(idx as usize) {
                        let n = crate::priority::collect_target_specs(&m.effect).len();
                        let sub = ResolutionCtx {
                            chosen_targets: ctx.chosen_targets.iter().skip(offset).take(n).copied().collect(),
                            target_controllers: ctx
                                .target_controllers
                                .iter()
                                .skip(offset)
                                .take(n)
                                .copied()
                                .collect(),
                            ..ctx.clone()
                        };
                        let mut sub_cursor = 0usize;
                        any |= self.interpret(&m.effect, &sub, sid, wb, &mut sub_cursor);
                        offset += n;
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
            // "Discard any number of cards" (CR 701.8) — the player picks how many + which. Imperative +
            // asks the agent, so it lives here. Flush staged actions first (mirrors `Discard`). Always
            // "performs" (returns true) even for zero, so a following "draw that many + 1" still runs.
            Effect::DiscardChosen { who } => {
                self.flush_pending(wb);
                let player = self.eval_player(*who, ctx);
                self.interpret_discard_chosen(player);
                true
            }
            // "Put up to `max` [filter] card(s) discarded this way onto the battlefield tapped under your
            // control" (Mind Roots). Selects among this resolution's discard scratch. Imperative + asks
            // the controller, so it lives here. Flush staged actions first.
            Effect::PutDiscardedOntoBattlefield { filter, max } => {
                self.flush_pending(wb);
                let controller = ctx.controller.unwrap_or(self.state.active_player);
                let filter = self.resolve_dynamic_filter(filter, ctx);
                let candidates: Vec<ObjId> = self
                    .discarded_this_resolution
                    .iter()
                    .copied()
                    .filter(|&id| self.count_filter_matches(id, &filter))
                    .collect();
                if !candidates.is_empty() && *max > 0 {
                    let chosen = match self.ask(
                        controller,
                        &DecisionRequest::SelectCards {
                            reason: SelectReason::Generic,
                            from: candidates.clone(),
                            min: 0,
                            max: *max,
                            description: "Put discarded card(s) onto the battlefield".into(),
                        },
                    ) {
                        DecisionResponse::Indices(v) => v
                            .iter()
                            .filter_map(|&i| candidates.get(i as usize).copied())
                            .take(*max as usize)
                            .collect::<Vec<_>>(),
                        DecisionResponse::Index(i) => candidates.get(i as usize).copied().into_iter().collect(),
                        _ => Vec::new(),
                    };
                    for card in chosen {
                        // Under YOUR control (owner unchanged): move to the battlefield keyed to the
                        // controller, then tap it (CR "onto the battlefield tapped under your control").
                        if self.state.move_object(card, Zone::Battlefield, controller) {
                            if let Some(o) = self.state.objects.get_mut(&card) {
                                o.status.tapped = true;
                            }
                            self.broadcast(GameEvent::ObjectMoved { obj: card, to: Zone::Battlefield });
                        }
                    }
                }
                true
            }
            // "You have no maximum hand size for the rest of the game" (CR 402.2) — Wisdom of Ages.
            // Imperative single-field mutation; flush staged actions first for ordering.
            Effect::SetNoMaxHandSize { who } => {
                self.flush_pending(wb);
                let player = self.eval_player(*who, ctx);
                if let Some(pl) = self.state.players.get_mut(player.0 as usize) {
                    pl.hand_size_limit = usize::MAX;
                }
                true
            }
            // "`what` gains your choice of [one of `options`] until [duration]" (Practiced Offense).
            // Interactive (asks the controller which keyword); lowers to the same `GrantContinuous`
            // path as `GrantKeyword`. Resolve the (cast-locked) target FIRST so the cursor advances.
            Effect::GrantChosenKeyword { what, options, duration } => {
                let target = self.resolve_target(what, ctx, cursor);
                if let (Some(Target::Object(obj)), false) = (target, options.is_empty()) {
                    let controller = ctx.controller.unwrap_or(self.state.active_player);
                    let mode_opts: Vec<ModeOption> =
                        options.iter().map(|k| ModeOption { label: format!("{k:?}") }).collect();
                    let idx = match self.ask(
                        controller,
                        &DecisionRequest::ChooseModes {
                            for_action: ActionRef(sid),
                            modes: mode_opts,
                            min: 1,
                            max: 1,
                            allow_repeat: false,
                        },
                    ) {
                        DecisionResponse::Indices(v) => v.first().copied().unwrap_or(0) as usize,
                        DecisionResponse::Index(i) => i as usize,
                        _ => 0,
                    };
                    let keyword = *options.get(idx).unwrap_or(&options[0]);
                    wb.push(Action::GrantContinuous {
                        source: ctx.source,
                        controller,
                        affected: vec![obj],
                        contributions: vec![StaticContribution::GrantKeyword(keyword)],
                        duration: *duration,
                    });
                }
                true
            }
            // Choose a land card name and note it on `what` (Petrified Hamlet). Enumerate the distinct
            // land-card names present in the game, ask the controller, and store the pick in the
            // target's `chosen_name` (read by its name-keyed statics). Direct state mutation like
            // `CastForFree` — it's plain object state, not a continuous effect.
            Effect::ChooseLandName { what } => {
                if let Some(Target::Object(obj)) = self.resolve_target(what, ctx, cursor) {
                    let controller = ctx.controller.unwrap_or(self.state.active_player);
                    let names = self.land_card_names();
                    if !names.is_empty() {
                        let options: Vec<OptionLabel> =
                            names.iter().map(|n| OptionLabel { label: n.clone() }).collect();
                        let idx = match self.ask(
                            controller,
                            &DecisionRequest::ChooseOption {
                                reason: OptionReason::NameCard,
                                options,
                                min: 1,
                                max: 1,
                            },
                        ) {
                            DecisionResponse::Indices(v) => v.first().copied().unwrap_or(0) as usize,
                            DecisionResponse::Index(i) => i as usize,
                            _ => 0,
                        };
                        if let Some(name) = names.get(idx).or(names.first()) {
                            if let Some(o) = self.state.objects.get_mut(&obj) {
                                o.chosen_name = Some(name.clone());
                            }
                        }
                    }
                }
                true
            }
            // Directed discard (CR 701.8): `who` reveals their hand, `chooser` picks up to `count`
            // cards matching `filter`, and `who` discards them (Render Speechless's "you choose").
            // Imperative + asks two different players, so it lives here. Flush staged first.
            Effect::DirectedDiscard { who, chooser, count, filter } => {
                self.flush_pending(wb);
                let discarder = self.eval_player(*who, ctx);
                let chooser = self.eval_player(*chooser, ctx);
                let count = self.eval_value(count, ctx).max(0) as u32;
                self.interpret_directed_discard(discarder, chooser, count, filter) > 0
            }
            // Counter a target spell/ability on the stack (CR 701.5). Imperative (mutates the stack),
            // so it lives here. A spell with the `CantBeCountered` qualification (CR 701.5f) is left
            // on the stack — the counterspell still resolved, it just did nothing to that spell.
            // Flush first so any earlier staged actions apply before the stack changes.
            Effect::ReturnSpellToHand { what } => {
                self.flush_pending(wb);
                if let Some(Target::Stack(sid)) = self.resolve_target(what, ctx, cursor) {
                    self.interpret_return_spell_to_hand(sid);
                }
                true
            }
            Effect::Counter { what } => {
                self.flush_pending(wb);
                if let Some(Target::Stack(sid)) = self.resolve_target(what, ctx, cursor) {
                    self.interpret_counter(sid);
                }
                true
            }
            // "Change the target of target spell or ability with a single target" (Return the Favor,
            // CR 115.7). Imperative (reads/mutates the victim stack object, asks the controller), so it
            // lives here; the new target is validated against the victim's OWN spec, and an impossible
            // retarget leaves it unchanged (see `Engine::change_target`). Flush staged actions first.
            Effect::ChangeTarget { what } => {
                self.flush_pending(wb);
                if let Some(Target::Stack(vsid)) = self.resolve_target(what, ctx, cursor) {
                    self.change_target(vsid, ctx);
                }
                true
            }
            // "Create a Role token attached to `attach_to`" (Monstrous Rage, Royal Treatment). Imperative
            // (mints an object, attaches it, moves any prior Role) — so it lives here. Flush staged
            // actions first so the target's pump/hexproof (earlier in the sequence) is committed before
            // the Role enters and its statics recompute.
            Effect::CreateRoleToken { role, attach_to } => {
                self.flush_pending(wb);
                if let Some(Target::Object(host)) = self.resolve_target(attach_to, ctx, cursor) {
                    let controller = ctx.controller.unwrap_or(self.state.active_player);
                    self.create_role_token(*role, host, controller);
                }
                true
            }
            // Cast a copy of `source` without paying its mana cost (CR 707.12). Mint a fresh object
            // from the source's copiable characteristics (CR 707.2 — grp_id carries abilities/effect/
            // mana cost), put it on the stack marked `is_copy` (so it ceases to exist off the stack,
            // 707.10a), and cast it for free through the real pipeline — new modes/targets/X=0 are
            // chosen there, SpellCast fires. Imperative (mints an object, asks the caster, pushes to
            // the stack), so it lives here. Flush staged actions first.
            Effect::CastCopy { source, controller } => {
                self.flush_pending(wb);
                let caster = self.eval_player(*controller, ctx);
                if let Some(Target::Object(src)) = self.resolve_target(source, ctx, cursor) {
                    let chars = self.state.object(src).chars.clone();
                    let copy = self.state.add_card(caster, chars, Zone::Stack);
                    if let Some(o) = self.state.objects.get_mut(&copy) {
                        o.is_copy = true;
                    }
                    self.cast_spell(caster, copy, CastVariant::WithoutPayingManaCost);
                }
                true
            }
            // Copy a spell that's on the stack `count` times (CR 707.10) — the storm / casualty /
            // infusion engine over `copy_spell_on_stack`. `what` names the stack spell: `Triggering` =
            // the spell that fired this "whenever you cast …" trigger (`ctx.triggering_spell`); a
            // resolved `Target`/`Select` that names a spell on the stack covers "copy target instant or
            // sorcery spell." Each copy is minted OVER the original (so copies resolve first), is NOT
            // cast, and ceases to exist off the stack; `choose_new_targets` offers the 707.10c "you may
            // choose new targets" reselection per copy. count ≤ 0 / source gone → no copies. Interactive
            // (may ask for new targets), so it lives here. Flush staged actions first.
            Effect::CopySpellOnStack { what, count, choose_new_targets } => {
                self.flush_pending(wb);
                let by = ctx.controller.unwrap_or(self.state.active_player);
                let is_spell = |st: &Self, o: ObjId| {
                    st.state
                        .stack
                        .items
                        .iter()
                        .any(|it| matches!(it.kind, crate::stack::StackObjectKind::Spell(x) if x == o))
                };
                // The spell's card object (what `copy_spell_on_stack` wants), verified still on the stack.
                let source = match what {
                    EffectTarget::Triggering => ctx.triggering_spell.filter(|s| is_spell(self, *s)),
                    _ => self.resolve_target(what, ctx, cursor).and_then(|t| match t {
                        Target::Stack(sid) => self
                            .state
                            .stack
                            .items
                            .iter()
                            .find(|it| it.id == sid)
                            .and_then(|it| match it.kind {
                                crate::stack::StackObjectKind::Spell(o) => Some(o),
                                _ => None,
                            }),
                        Target::Object(o) => is_spell(self, o).then_some(o),
                        _ => None,
                    }),
                };
                if let Some(spell) = source {
                    let n = self.eval_value(count, ctx).max(0);
                    for _ in 0..n {
                        self.copy_spell_on_stack(spell, by, *choose_new_targets);
                    }
                }
                true
            }
            // Copy a creature (permanent) spell you control → a token (CR 707.10/707.10f), granting the
            // copy haste and arming its "sacrifice at the next end step" clause (Choreographed Sparks mode
            // 2). Mint over the original via `copy_spell_on_stack`, then decorate the returned copy while
            // it's still on the stack so both riders carry onto the token. Interactive (mints an object),
            // so it lives here. Flush staged actions first.
            Effect::CopySpellAsToken { what, haste, sacrifice_at_next_end_step } => {
                self.flush_pending(wb);
                let by = ctx.controller.unwrap_or(self.state.active_player);
                let source = self.resolve_target(what, ctx, cursor).and_then(|t| match t {
                    Target::Stack(sid) => self
                        .state
                        .stack
                        .items
                        .iter()
                        .find(|it| it.id == sid)
                        .and_then(|it| match it.kind {
                            crate::stack::StackObjectKind::Spell(o) => Some(o),
                            _ => None,
                        }),
                    Target::Object(o) => self
                        .state
                        .stack
                        .items
                        .iter()
                        .any(|it| matches!(it.kind, crate::stack::StackObjectKind::Spell(x) if x == o))
                        .then_some(o),
                    _ => None,
                });
                if let Some(spell) = source {
                    if let Some(copy) = self.copy_spell_on_stack(spell, by, false) {
                        if *haste {
                            if let Some(o) = self.state.objects.get_mut(&copy) {
                                let hk = crate::effects::ability::Keyword::Haste;
                                if !o.chars.keywords.contains(&hk) {
                                    o.chars.keywords.push(hk);
                                }
                            }
                            self.state.mark_chars_dirty();
                        }
                        if *sacrifice_at_next_end_step {
                            self.state.register_delayed_trigger(
                                copy,
                                crate::effects::action::DelayedTriggerEvent::AtBeginningOfNextEndStep,
                                by,
                                Some(copy),
                                vec![Action::Sacrifice { obj: copy, by }],
                            );
                        }
                    }
                }
                true
            }
            // "Target creature's owner puts it on their choice of the top or bottom of their library"
            // (Run Behind). The OWNER (not the caster) chooses; then the object moves to their library
            // at that end. Interactive (asks the owner), so it lives here. Flush staged actions first.
            Effect::PutOnTopOrBottom { what } => {
                self.flush_pending(wb);
                if let Some(Target::Object(obj)) = self.resolve_target(what, ctx, cursor) {
                    let owner = self.state.object(obj).owner;
                    let on_top = matches!(
                        self.ask(owner, &DecisionRequest::Confirm { kind: ConfirmKind::PutOnTop(obj) }),
                        DecisionResponse::Bool(true)
                    );
                    let pos = if on_top { ZonePos::Top } else { ZonePos::Bottom };
                    wb.push(Action::MoveZone {
                        obj,
                        to: Zone::Library,
                        pos,
                        cause: MoveCause::Returned,
                        tapped: false,
                    });
                }
                true
            }
            // "Put `count` cards from your hand on top of your library in any order" (Brainstorm).
            // Interactive: the controller selects the cards (ordered) then they go onto the library top.
            Effect::PutFromHandOnTop { who, count } => {
                self.flush_pending(wb);
                let pl = self.eval_player(*who, ctx);
                let n = self.eval_value(count, ctx).max(0) as usize;
                self.interpret_put_from_hand_on_top(pl, n);
                true
            }
            // "You may tap or untap target creature" (Rejoinder). The target was chosen at cast; its
            // controller may decline, else choose the direction. Interactive, so it lives here.
            Effect::MayTapOrUntap { what } => {
                self.flush_pending(wb);
                if let Some(Target::Object(obj)) = self.resolve_target(what, ctx, cursor) {
                    let decider = ctx.controller.unwrap_or_else(|| self.state.object(obj).controller);
                    // "you may" — decline is allowed (CR 701.20a).
                    let opt_in = matches!(
                        self.ask(decider, &DecisionRequest::Confirm { kind: ConfirmKind::MayEffect }),
                        DecisionResponse::Bool(true)
                    );
                    if opt_in {
                        // Direction: tap (true) or untap (false).
                        let tap = matches!(
                            self.ask(decider, &DecisionRequest::Confirm { kind: ConfirmKind::Generic }),
                            DecisionResponse::Bool(true)
                        );
                        wb.push(Action::TapUntap { obj, tap });
                    }
                }
                true
            }
            // Cast the ACTUAL targeted card for free (CR 601.2f) — a granted flashback-style recast
            // (The Dawning Archaic). Resolve the (up-to-one) target; cast it for {0} through the real
            // pipeline; if `exile_on_leave`, flag it (via the flashback exile-on-leave-stack path) so
            // it exiles rather than hitting the graveyard as it leaves the stack. An unchosen up-to-one
            // target (declined) resolves to `None` — no cast. Flush staged actions first.
            Effect::CastForFree { what, exile_on_leave } => {
                self.flush_pending(wb);
                if let Some(Target::Object(card)) = self.resolve_target(what, ctx, cursor) {
                    let caster = ctx.controller.unwrap_or_else(|| self.state.object(card).owner);
                    self.cast_spell(caster, card, CastVariant::WithoutPayingManaCost);
                    if *exile_on_leave && self.state.object(card).zone == Zone::Stack {
                        if let Some(o) = self.state.objects.get_mut(&card) {
                            o.flashback_cast = true;
                        }
                    }
                }
                true
            }
            // "Exile from the top until total MV ≥ N, then you may cast any number of them for free"
            // (CR 601.3e cast-during-resolution — Improvisation Capstone). Exile one at a time until the
            // running total mana value reaches the threshold (or the library empties), then loop offering
            // the controller to cast any number of the exiled NONLAND cards for free (each a real
            // `cast_spell`, so it picks its own targets/X and goes on the stack). Uncast cards stay
            // exiled. Imperative + interactive, so it lives here. Flush staged actions first.
            Effect::ExileTopUntilManaValueMayCastFree { who, total_mana_value } => {
                self.flush_pending(wb);
                let player = self.eval_player(*who, ctx);
                // Exile from the top until the exiled cards' total mana value ≥ threshold.
                let mut total = 0u32;
                let mut exiled: Vec<ObjId> = Vec::new();
                while total < *total_mana_value {
                    let Some(&top) = self.state.player(player).library.last() else {
                        break; // library empty (CR 701.x — exile as many as you can)
                    };
                    total += self.state.object(top).chars.mana_value();
                    self.state.move_object(top, Zone::Exile, player);
                    self.broadcast(GameEvent::ObjectMoved { obj: top, to: Zone::Exile });
                    exiled.push(top);
                }
                // "You may cast any number of them without paying their mana costs" — one at a time
                // (nothing resolves between casts, so the only choice is which to cast and the stack
                // order). Only nonland cards are spells.
                let mut castable: Vec<ObjId> =
                    exiled.into_iter().filter(|&o| !self.state.object(o).chars.is_land()).collect();
                let mut guard = 0;
                while !castable.is_empty() {
                    guard += 1;
                    if guard > 256 {
                        break; // safety ceiling
                    }
                    let resp = self.ask(
                        player,
                        &DecisionRequest::SelectCards {
                            reason: SelectReason::Generic,
                            from: castable.clone(),
                            min: 0,
                            max: 1,
                            description: "cast without paying its mana cost".to_string(),
                        },
                    );
                    let pick = match resp {
                        DecisionResponse::Indices(ix) => ix.into_iter().next(),
                        _ => None,
                    };
                    let Some(i) = pick.filter(|&i| (i as usize) < castable.len()) else {
                        break; // declined (or bad index) → stop casting
                    };
                    let card = castable.remove(i as usize);
                    self.cast_spell(player, card, CastVariant::WithoutPayingManaCost);
                }
                true
            }
            // Cascade (CR 702.83): exile from the top until a nonland card with MV < the cast spell's
            // MV (the "cheaper" hit); you may cast that hit for free; the rest go to the bottom in a
            // random order. Threshold = `ctx.triggering_spell`'s MV. Imperative + interactive, so it
            // lives here. Flush staged actions first.
            Effect::Cascade => {
                self.flush_pending(wb);
                let player = ctx.controller.unwrap_or(self.state.active_player);
                // The cascading spell's mana value (the SelfCast spell, or the granted trigger's spell).
                let threshold = ctx
                    .triggering_spell
                    .and_then(|s| self.state.objects.get(&s))
                    .map(|o| o.chars.mana_value())
                    .unwrap_or(0);
                // Exile from the top one at a time until a nonland with MV < threshold (or empty).
                let mut exiled: Vec<ObjId> = Vec::new();
                let mut hit: Option<ObjId> = None;
                let mut guard = 0;
                while hit.is_none() {
                    guard += 1;
                    if guard > 1024 {
                        break; // safety ceiling (a library can't exceed this)
                    }
                    let Some(&top) = self.state.player(player).library.last() else {
                        break; // library empty — exile as many as you can (CR 702.83e)
                    };
                    let mv = self.state.object(top).chars.mana_value();
                    let is_land = self.state.object(top).chars.is_land();
                    self.state.move_object(top, Zone::Exile, player);
                    self.broadcast(GameEvent::ObjectMoved { obj: top, to: Zone::Exile });
                    exiled.push(top);
                    if !is_land && mv < threshold {
                        hit = Some(top);
                    }
                }
                // "You may cast it without paying its mana cost" (CR 702.83e).
                let mut cast: Option<ObjId> = None;
                if let Some(h) = hit {
                    let yes = matches!(
                        self.ask(player, &DecisionRequest::Confirm { kind: ConfirmKind::MayEffect }),
                        DecisionResponse::Bool(true)
                    );
                    if yes {
                        self.cast_spell(player, h, CastVariant::WithoutPayingManaCost);
                        cast = Some(h);
                    }
                }
                // "Put the exiled cards on the bottom of your library in a random order" — everything
                // exiled except the one cast (which is now on the stack). Bottom = front of the vec.
                let mut rest: Vec<ObjId> =
                    exiled.into_iter().filter(|o| Some(*o) != cast).collect();
                self.state.rng.shuffle(&mut rest);
                for &c in &rest {
                    let owner = self.state.object(c).owner;
                    self.state.move_object(c, Zone::Library, owner); // appends to the top …
                }
                let libv = &mut self.state.player_mut(player).library;
                libv.retain(|o| !rest.contains(o)); // … so pull them back out …
                for &c in rest.iter().rev() {
                    libv.insert(0, c); // … and put them on the bottom (front).
                }
                true
            }
            // "Target I/S card in your graveyard gains flashback until end of turn, cost = its mana cost"
            // (Flashback). Set the target's `flashback_until_turn` to this turn; the flashback offer reads
            // it. Imperative (flag flip on a chosen target), so it lives here. Flush staged actions first.
            Effect::GrantFlashbackUntilEndOfTurn { what } => {
                self.flush_pending(wb);
                if let Some(Target::Object(obj)) = self.resolve_target(what, ctx, cursor) {
                    let turn = self.state.turn_number;
                    if let Some(o) = self.state.objects.get_mut(&obj) {
                        o.flashback_until_turn = Some(turn);
                    }
                }
                true
            }
            // "Reveal from the top until you reveal a `filter` card; put it into hand, rest on the bottom
            // "Reveal the top card, put it into your hand, lose life = its mana value; you may repeat
            // any number of times" (Ad Nauseam). Ask before each iteration (so it may run 0+ times).
            Effect::RevealTopLoseLifeMayRepeat => {
                self.flush_pending(wb);
                let player = ctx.controller.unwrap_or(self.state.active_player);
                loop {
                    let Some(top) = self.state.player(player).library.last().copied() else { break };
                    let yes = matches!(
                        self.ask(player, &DecisionRequest::Confirm { kind: ConfirmKind::MayEffect }),
                        DecisionResponse::Bool(true)
                    );
                    if !yes {
                        break;
                    }
                    let mv = self.state.object(top).chars.mana_value() as i32;
                    self.state.move_object(top, Zone::Hand, player);
                    self.broadcast(GameEvent::ObjectMoved { obj: top, to: Zone::Hand });
                    self.change_life(player, -mv);
                }
                true
            }
            // in random order" (Page, Loose Leaf's Grandeur). Reveal-until analogue of Cascade. Imperative
            // (library scan + rng), so it lives here. Flush staged actions first.
            Effect::RevealFromTopUntilToHand { filter } => {
                self.flush_pending(wb);
                let player = ctx.controller.unwrap_or(self.state.active_player);
                // Snapshot the library top-first to avoid borrowing `self` twice under the filter check.
                let top_first: Vec<ObjId> =
                    self.state.player(player).library.iter().rev().copied().collect();
                let mut revealed_nonmatch: Vec<ObjId> = Vec::new();
                let mut hit: Option<ObjId> = None;
                for card in top_first {
                    if self.count_filter_matches(card, filter) {
                        hit = Some(card);
                        break;
                    }
                    revealed_nonmatch.push(card);
                }
                // The matching card goes to hand (CR 701.18 reveal, then move).
                if let Some(h) = hit {
                    let owner = self.state.object(h).owner;
                    self.state.move_object(h, Zone::Hand, owner);
                    self.broadcast(GameEvent::ObjectMoved { obj: h, to: Zone::Hand });
                }
                // The rest go on the bottom (front of the vec) in a random order.
                let mut rest = revealed_nonmatch;
                self.state.rng.shuffle(&mut rest);
                let libv = &mut self.state.player_mut(player).library;
                libv.retain(|o| !rest.contains(o));
                for &c in rest.iter().rev() {
                    libv.insert(0, c);
                }
                true
            }
            // "Mill `count`, then put a creature card from among them onto the battlefield" (Bind to
            // Life). Mill from `who`'s own library, capturing the milled cards, then (mandatory, if any
            // creature was milled) let them choose one to put onto the battlefield — theirs (owner ==
            // controller), so no control override. Imperative, so it lives here. Flush staged first.
            Effect::MillThenPutCreatureOntoBattlefield { who, count } => {
                self.flush_pending(wb);
                let player = self.eval_player(*who, ctx);
                let n = self.eval_value(count, ctx).max(0) as u32;
                let mut milled: Vec<ObjId> = Vec::new();
                for _ in 0..n {
                    let Some(top) = self.state.player(player).library.last().copied() else {
                        break;
                    };
                    if self.state.move_object(top, Zone::Graveyard, player) {
                        self.broadcast(GameEvent::ObjectMoved { obj: top, to: Zone::Graveyard });
                        milled.push(top);
                    }
                }
                // "put a creature card from among them onto the battlefield" — the eligible set is the
                // just-milled creature cards (read printed chars — they're in the graveyard now).
                let creatures: Vec<ObjId> =
                    milled.into_iter().filter(|&o| self.state.object(o).chars.is_creature()).collect();
                if !creatures.is_empty() {
                    let resp = self.ask(
                        player,
                        &DecisionRequest::SelectCards {
                            reason: SelectReason::Generic,
                            from: creatures.clone(),
                            min: 1,
                            max: 1,
                            description: "put a creature card onto the battlefield".to_string(),
                        },
                    );
                    let i = match resp {
                        DecisionResponse::Indices(ix) => ix.into_iter().next().unwrap_or(0),
                        _ => 0,
                    };
                    if let Some(&card) = creatures.get(i as usize) {
                        if self.state.move_object(card, Zone::Battlefield, player) {
                            self.broadcast(GameEvent::ObjectMoved { obj: card, to: Zone::Battlefield });
                        }
                    }
                }
                true
            }
            // "Put target card onto the battlefield under your control" (Reanimate). Move the chosen
            // target to the CONTROLLER's battlefield (owner unchanged) so it enters under your control
            // even from an opponent's graveyard; ETB triggers fire via the broadcast. Imperative, so it
            // lives here. Flush staged actions first.
            Effect::ReanimateUnderControl { what } => {
                self.flush_pending(wb);
                if let Some(Target::Object(obj)) = self.resolve_target(what, ctx, cursor) {
                    let controller =
                        ctx.controller.unwrap_or_else(|| self.state.object(obj).owner);
                    if self.state.move_object(obj, Zone::Battlefield, controller) {
                        self.broadcast(GameEvent::ObjectMoved { obj, to: Zone::Battlefield });
                    }
                }
                true
            }
            // "Exile `what`, then return it to the battlefield under its owner's control" (CR 603.6e
            // blink — All Aboard). Exile it (LTB fires), then return it as a NEW object (ETB fires;
            // `move_object` resets status/counters/damage and re-applies summoning sickness). Imperative,
            // so it lives here. Flush staged actions first.
            Effect::Blink { what } => {
                self.flush_pending(wb);
                if let Some(Target::Object(obj)) = self.resolve_target(what, ctx, cursor) {
                    let owner = self.state.object(obj).owner;
                    self.state.move_object(obj, Zone::Exile, owner);
                    self.broadcast(GameEvent::ObjectMoved { obj, to: Zone::Exile });
                    self.state.move_object(obj, Zone::Battlefield, owner);
                    self.broadcast(GameEvent::ObjectMoved { obj, to: Zone::Battlefield });
                }
                true
            }
            // Timed blink (CR 603.7): exile now, arm a delayed "return at the beginning of the next
            // end step" trigger carrying the return `MoveZone` (to the owner's battlefield).
            Effect::ExileReturnNextEndStep { what } => {
                self.flush_pending(wb);
                if let Some(Target::Object(obj)) = self.resolve_target(what, ctx, cursor) {
                    let owner = self.state.object(obj).owner;
                    self.state.move_object(obj, Zone::Exile, owner);
                    self.broadcast(GameEvent::ObjectMoved { obj, to: Zone::Exile });
                    let controller = ctx.controller.unwrap_or(PlayerId(0));
                    self.state.register_delayed_trigger(
                        obj,
                        DelayedTriggerEvent::AtBeginningOfNextEndStep,
                        controller,
                        Some(obj),
                        vec![Action::MoveZone {
                            obj,
                            to: Zone::Battlefield,
                            pos: crate::basics::ZonePos::Any,
                            cause: MoveCause::Resolved,
                            tapped: false,
                        }],
                    );
                }
                true
            }
            // "When you next cast a [filter] spell this turn, copy that spell" (CR 707.10 / 603.7) —
            // arm a one-shot delayed trigger on the controller (Striking Palette). Non-interactive
            // (just registers the trigger); when it later fires the engine mints a `SpellCopyTrigger`
            // over the just-cast spell. `watching` is inert here (the dies/exile firing path never
            // matches a `YouCastSpell` event — it fires from the `SpellCast` broadcast instead).
            Effect::CopyNextSpellCast { filter, choose_new_targets } => {
                let controller = ctx.controller.unwrap_or(PlayerId(0));
                wb.push(Action::RegisterDelayedTrigger {
                    watching: ctx.source.unwrap_or(ObjId(0)),
                    event: DelayedTriggerEvent::YouCastSpell {
                        filter: filter.clone(),
                        reaction: crate::effects::action::YouCastSpellReaction::CopySpell {
                            choose_new_targets: *choose_new_targets,
                        },
                        until_end_of_turn: false, // one-shot: "when you NEXT cast …"
                    },
                    controller,
                    source: ctx.source,
                    actions: Vec::new(),
                });
                true
            }
            // "This turn, whenever you cast a spell matching `filter`, do `effect`" (Glimpse of Nature).
            // Registers a RECURRING YouCastSpell delayed trigger whose `actions` are `effect` lowered;
            // it fires for every matching cast this turn and expires at the next turn's start.
            Effect::WheneverYouCastThisTurn { filter, effect } => {
                let controller = ctx.controller.unwrap_or(PlayerId(0));
                let actions = self.lower_effect_to_actions(effect, ctx);
                wb.push(Action::RegisterDelayedTrigger {
                    watching: ctx.source.unwrap_or(ObjId(0)),
                    event: DelayedTriggerEvent::YouCastSpell {
                        filter: filter.clone(),
                        reaction: crate::effects::action::YouCastSpellReaction::RunActions,
                        until_end_of_turn: true,
                    },
                    controller,
                    source: ctx.source,
                    actions,
                });
                true
            }
            // Ward soft-counter (CR 702.21): counter `what` unless *its controller* (the targeting
            // player, not the Ward controller) pays `cost`. They're only offered the choice if they
            // can afford it; declining or being unable to pay counters the spell/ability. Imperative
            // (asks a player, taps mana, mutates the stack), so it lives here. Flush staged first.
            Effect::CounterUnlessPay { what, cost } => {
                self.flush_pending(wb);
                if let Some(Target::Stack(target_sid)) = self.resolve_target(what, ctx, cursor) {
                    // The controller of the targeting spell/ability pays (or the object is countered).
                    let payer =
                        self.state.stack.items.iter().find(|s| s.id == target_sid).map(|s| s.controller);
                    if let Some(payer) = payer {
                        // `source` is only used by `{T}`/sacrifice-self cost components (Ward costs
                        // have none), so for pure mana/life the value is irrelevant. `ctx.source` (the
                        // Ward permanent) is always present for a triggered ability; `ObjId(0)` is a
                        // dead fallback.
                        let src = ctx.source.unwrap_or(ObjId(0));
                        let can_pay = self.can_pay_cost(payer, src, cost);
                        let paid = can_pay
                            && matches!(
                                self.ask(
                                    payer,
                                    &DecisionRequest::Confirm { kind: ConfirmKind::PayToPrevent },
                                ),
                                DecisionResponse::Bool(true),
                            );
                        if paid {
                            self.pay_cost(payer, src, cost);
                        } else {
                            self.interpret_counter(target_sid);
                        }
                    }
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
            // Scry N (CR 701.17): look at the top N, put any number on the bottom, keep the rest on
            // top. Imperative (asks which to bottom). Flush first.
            Effect::Scry { count } => {
                self.flush_pending(wb);
                let player = ctx.controller.unwrap_or(PlayerId(0));
                let n = self.eval_value(count, ctx).max(0) as usize;
                self.interpret_scry(player, n);
                true
            }
            // Impulse-play the top `count` cards (Jeska's Will). Imperative: exile the current top,
            // repeat, so each iteration sees the updated top. Flush any staged actions first.
            Effect::ExileTopForPlay { who, count, window } => {
                self.flush_pending(wb);
                let player = self.eval_player(*who, ctx);
                let n = self.eval_value(count, ctx).max(0) as usize;
                let until = match window {
                    crate::effects::PlayWindow::ThisTurn => self.state.turn_number,
                    crate::effects::PlayWindow::YourNextTurn => {
                        if self.state.active_player == player {
                            self.state.turn_number + 2
                        } else {
                            self.state.turn_number + 1
                        }
                    }
                };
                for _ in 0..n {
                    let Some(top) = self.state.player(player).library.last().copied() else { break };
                    let owner = self.state.object(top).owner;
                    if self.state.move_object(top, Zone::Exile, owner) {
                        if let Some(o) = self.state.objects.get_mut(&top) {
                            o.castable_from_exile = true;
                            o.play_until_turn = Some(until);
                        }
                        self.broadcast(GameEvent::ObjectMoved { obj: top, to: Zone::Exile });
                    }
                }
                true
            }
            // Ral Zarek −7: flip `coins` coins on the seeded RNG; `who` skips that many of their next
            // turns (CR 720). Reads `state.rng`, so it's an imperative effect (flush first).
            Effect::FlipCoinsSkipNextTurns { who, coins } => {
                self.flush_pending(wb);
                let target = self.eval_player(*who, ctx);
                let mut heads = 0u32;
                for _ in 0..*coins {
                    if self.state.rng.below(2) == 1 {
                        heads += 1;
                    }
                }
                self.state.player_mut(target).skip_next_turns += heads;
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
            // "Look at the top `count`; put `to_hand` into hand, `to_exile_play` into exile (playable
            // until `window`), rest on the bottom" (Expressive Iteration). Imperative; flush first.
            Effect::LookDistribute { count, to_hand, to_exile_play, window } => {
                self.flush_pending(wb);
                let player = ctx.controller.unwrap_or(PlayerId(0));
                let n = (self.eval_value(count, ctx).max(0) as usize)
                    .min(self.state.player(player).library.len());
                if n == 0 {
                    return true;
                }
                // Top-first snapshot of the looked-at cards.
                let mut pool: Vec<ObjId> =
                    self.state.player(player).library.iter().rev().take(n).copied().collect();
                // 1) Choose `to_hand` for hand.
                let want_hand = (*to_hand as usize).min(pool.len());
                let hand: Vec<ObjId> = self.choose_n(player, &pool, want_hand, SelectReason::Generic, "put into your hand");
                pool.retain(|o| !hand.contains(o));
                for &c in &hand {
                    self.state.move_object(c, Zone::Hand, player);
                    self.broadcast(GameEvent::ObjectMoved { obj: c, to: Zone::Hand });
                }
                // 2) Choose `to_exile_play` to exile with play permission until `window`.
                let want_exile = (*to_exile_play as usize).min(pool.len());
                let exiled: Vec<ObjId> = self.choose_n(player, &pool, want_exile, SelectReason::Generic, "exile to play this turn");
                pool.retain(|o| !exiled.contains(o));
                let until = match window {
                    crate::effects::PlayWindow::ThisTurn => self.state.turn_number,
                    crate::effects::PlayWindow::YourNextTurn => {
                        if self.state.active_player == player { self.state.turn_number + 2 } else { self.state.turn_number + 1 }
                    }
                };
                for &c in &exiled {
                    if self.state.move_object(c, Zone::Exile, player) {
                        if let Some(o) = self.state.objects.get_mut(&c) {
                            o.castable_from_exile = true;
                            o.play_until_turn = Some(until);
                        }
                        self.broadcast(GameEvent::ObjectMoved { obj: c, to: Zone::Exile });
                    }
                }
                // 3) The rest go on the bottom (front of the library vec).
                if !pool.is_empty() {
                    let libv = &mut self.state.player_mut(player).library;
                    libv.retain(|o| !pool.contains(o));
                    for &c in pool.iter().rev() {
                        libv.insert(0, c);
                    }
                }
                true
            }
            // Zimone's Experiment: look at the top `count`, pick up to `take` creature/land cards, route
            // lands → battlefield tapped and creatures → hand, rest to the bottom in random order.
            Effect::LookPickCreaturesLands { count, take } => {
                self.flush_pending(wb);
                let player = ctx.controller.unwrap_or(self.state.active_player);
                let n = self.eval_value(count, ctx).max(0) as usize;
                let take = self.eval_value(take, ctx).max(0) as usize;
                self.interpret_look_pick_creatures_lands(player, n, take);
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
            // "Exile a card from a zone" chosen at resolution (a `Select`, not the word "target") —
            // e.g. Heated Argument's "you may exile a card from your graveyard". Handled here (not just
            // `materialize`) so it (a) actually resolves the selection and (b) reports **performed** =
            // the selection reached its `min`, so an empty graveyard withholds a wrapping `IfYouDo`
            // reward. The "exile target …" case (an `EffectTarget::Target`) stays in `materialize`.
            Effect::Exile { what: EffectTarget::Select(spec) } => {
                self.flush_pending(wb);
                let min = self.eval_value(&spec.min, ctx).max(0) as usize;
                let chosen = self.select_for_each(spec, ctx);
                let performed = chosen.len() >= min;
                for obj in chosen {
                    wb.push(Action::Exile { obj, source: ctx.source });
                }
                performed
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
            // "You may pay `cost`. If you do, `then`." — the mana/cost analogue of `IfYouDo`. Ask the
            // resolving ability's controller (only if the cost is payable); on payment, run `then`.
            // Flush staged actions first so `then` sees committed state (mirrors CounterUnlessPay).
            //
            // {X} support (Tester of the Tangential's "you may pay {X}. When you do, move X counters …"):
            // when the mana cost has `{X}`, announce X (bounded by affordable mana) INSTEAD of a yes/no
            // confirm — X = 0 is the decline — then fold it into the concrete cost and thread it into
            // `then` via `ctx.x` (so `ValueExpr::X` reads the paid amount). NB: a target inside `then` is
            // collected as a normal ability target (see `collect_specs_into`), so it's chosen when the
            // trigger goes on the stack — a beat before the pay decision — rather than reflexively (CR
            // 603.7c). Acceptable for the pool; noted.
            Effect::MayPayCost { cost, then } => {
                self.flush_pending(wb);
                let payer = ctx.controller.unwrap_or(PlayerId(0));
                let src = ctx.source.unwrap_or(ObjId(0));
                let x_pips = cost.mana.as_ref().map_or(0, |m| m.x);
                let (concrete, chosen_x) = if x_pips > 0 {
                    let m = cost.mana.as_ref().unwrap();
                    let fixed = m.generic + m.colored.values().sum::<u32>();
                    let max_x = crate::mana::available_mana(&self.state, payer).saturating_sub(fixed) / x_pips;
                    let x = match self.ask(
                        payer,
                        &DecisionRequest::ChooseNumber {
                            reason: crate::agent::NumberReason::ChooseX,
                            min: 0,
                            max: max_x as i64,
                            step: 1,
                            forbidden: Vec::new(),
                            disallow_even: false,
                            disallow_odd: false,
                        },
                    ) {
                        DecisionResponse::Number(n) => n.clamp(0, max_x as i64) as u32,
                        _ => 0,
                    };
                    // Fold the chosen X into the generic part of the mana cost (CR 601.2f-style).
                    let mut cm = m.clone();
                    cm.generic += x * cm.x;
                    cm.x = 0;
                    (crate::effects::ability::Cost { mana: Some(cm), components: cost.components.clone() }, x)
                } else {
                    (cost.clone(), 0)
                };
                // Pay decision: an {X} cost is "paid" iff X > 0 (X = 0 is the decline); a fixed cost asks
                // a yes/no confirm. Either way the concrete cost must be payable.
                let pay = if x_pips > 0 {
                    chosen_x > 0 && self.can_pay_cost(payer, src, &concrete)
                } else {
                    self.can_pay_cost(payer, src, &concrete)
                        && matches!(
                            self.ask(payer, &DecisionRequest::Confirm { kind: ConfirmKind::MayEffect }),
                            DecisionResponse::Bool(true),
                        )
                };
                if pay {
                    self.pay_cost(payer, src, &concrete);
                    if x_pips > 0 {
                        let then_ctx = ResolutionCtx { x: Some(chosen_x), ..ctx.clone() };
                        self.interpret(then, &then_ctx, sid, wb, cursor)
                    } else {
                        self.interpret(then, ctx, sid, wb, cursor)
                    }
                } else {
                    false
                }
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
                    let prev = self.foreach_current.replace(Target::Object(item));
                    self.interpret(body, ctx, sid, wb, cursor);
                    self.foreach_current = prev;
                }
                performed
            }
            // "For each player …" (CR 101.4 APNAP) — run `body` once per player, binding that player to
            // `Each` so the body reads their own state (Pox Plague). Starts from the active player and
            // wraps, so choices happen in turn order.
            Effect::ForEachPlayer { body } => {
                let n = self.state.players.len() as u32;
                let ap = self.state.active_player.0;
                for i in 0..n {
                    let pl = PlayerId((ap + i) % n);
                    let prev = self.foreach_current.replace(Target::Player(pl));
                    self.interpret(body, ctx, sid, wb, cursor);
                    self.foreach_current = prev;
                }
                true
            }
            // "for each of the up-to-N target creatures, run `body`" (Homesickness). Bind each chosen
            // target of the multi-target slot to `EffectTarget::Each` in turn; the loop consumes the
            // slot's cursor positions (an "up to N" slot may have fewer picks, so stop when they run
            // out). `body` references its per-iteration target via `Each`.
            Effect::ForEachTarget { slot, body } => {
                let probe = EffectTarget::Target(slot.clone());
                let mut performed = false;
                for _ in 0..slot.max.max(1) {
                    // A target in the slot may be an object OR a player ("1 damage to each of one or
                    // two targets" — Prismari Charm); bind whichever to `Each`. `None` = the "up to N"
                    // slot had fewer picks, so stop.
                    match self.resolve_target(&probe, ctx, cursor) {
                        Some(t) => {
                            let prev = self.foreach_current.replace(t);
                            performed |= self.interpret(body, ctx, sid, wb, cursor);
                            self.foreach_current = prev;
                        }
                        None => break,
                    }
                }
                performed
            }
            // A no-op never counts as "performed" (so it can't satisfy an `IfYouDo` cost).
            Effect::Nothing => false,
            // Create token(s), then COMMIT them immediately (deferred→imperative boundary, #61) so a
            // LATER step in the same resolution can see them on the battlefield — Antiquities on the
            // Loose's "create two Spirits, then put a +1/+1 counter on each Spirit you control." The
            // rewrite pass still runs on the flushed batch, so "enters with counters" replacements are
            // unaffected. (A standalone CreateToken flushes what it staged and is otherwise unchanged.)
            Effect::CreateToken { .. } => {
                self.materialize(effect, ctx, wb, cursor);
                self.flush_pending(wb);
                true
            }
            // Put counters, but FLUSH prior staged actions first (#61 deferred→imperative ordering) so
            // this step's `n` reads post-prior-step state — Growth Curve's "put a +1/+1 counter, then
            // double the number of +1/+1 counters" needs the first counter committed before the doubling
            // step evaluates `CountersOnTarget`. Still lowers via `materialize` (staged, committed at the
            // resolution's end or by the next flush); a lone `PutCounters` is unchanged (empty flush).
            Effect::PutCounters { .. } => {
                self.flush_pending(wb);
                self.materialize(effect, ctx, wb, cursor);
                true
            }
            // "Deal N damage to target; exile that many cards from the top of your library as its
            // EXCESS damage, playable until `window`" (Archaic's Agony). Excess is read from the
            // target's pre-damage state, so flush first, then stage the damage and impulse-exile.
            Effect::DealDamageExcessImpulse { amount, to, window } => {
                self.flush_pending(wb);
                if let Some(Target::Object(obj)) = self.resolve_target(to, ctx, cursor) {
                    let amt = self.eval_value(amount, ctx).max(0) as u32;
                    // Excess = amount − lethal-needed (toughness − damage already marked), floored at 0.
                    let toughness = self.state.computed(obj).toughness.unwrap_or(0).max(0) as u32;
                    let marked = self.state.object(obj).damage_marked;
                    let remaining = toughness.saturating_sub(marked);
                    let excess = amt.saturating_sub(remaining);
                    wb.push(Action::Damage {
                        target: Target::Object(obj),
                        amount: amt,
                        source: ctx.source.unwrap_or(ObjId(0)),
                        kind: DamageKind::Noncombat,
                    });
                    // Impulse-exile `excess` top-of-library cards with play permission through `window`.
                    let controller = ctx.controller.unwrap_or(self.state.active_player);
                    let until = match window {
                        crate::effects::PlayWindow::ThisTurn => self.state.turn_number,
                        crate::effects::PlayWindow::YourNextTurn => {
                            if self.state.active_player == controller {
                                self.state.turn_number + 2
                            } else {
                                self.state.turn_number + 1
                            }
                        }
                    };
                    for _ in 0..excess {
                        let top = match self.state.player(controller).library.last().copied() {
                            Some(c) => c,
                            None => break,
                        };
                        if self.state.move_object(top, Zone::Exile, controller) {
                            if let Some(o) = self.state.objects.get_mut(&top) {
                                o.castable_from_exile = true;
                                o.play_until_turn = Some(until);
                            }
                            self.broadcast(GameEvent::ObjectMoved { obj: top, to: Zone::Exile });
                        }
                    }
                }
                true
            }
            // "Then IT deals damage equal to its power …" (Burrog Barrage) — FLUSH first so a
            // same-resolution pump on the source (the "+1/+0 until end of turn" step above it in the
            // sequence) is committed before `amount` reads the source's now-buffed power (CR 608.2h).
            // Still lowers via `materialize` (staged); a lone `SourcedDamage` is unchanged (empty flush).
            Effect::SourcedDamage { .. } => {
                self.flush_pending(wb);
                self.materialize(effect, ctx, wb, cursor);
                true
            }
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
        // Resolve any dynamic (X-keyed) filter to a concrete one against this resolution's ctx, so a
        // "permanent card with mana value X or less" search (Mind into Matter) actually matches (the
        // ctx-free `count_filter_matches` only understands the static `ManaValue` form).
        let filter = self.resolve_dynamic_filter(filter, ctx);
        let from: Vec<ObjId> = self
            .zone_cards(searcher, zone)
            .into_iter()
            .filter(|&id| self.count_filter_matches(id, &filter))
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
        // Tutor back into your own library at a fixed end — "shuffle, THEN put on top/bottom" (CR
        // 701.19, Vampiric Tutor). The picks are already in the library; pull them out, shuffle the
        // rest, then place them at the chosen end, so the shuffle can't scatter the tutored card.
        let into_library_pos = zone == Zone::Library
            && to.zone == Zone::Library
            && matches!(to.pos, ZonePos::Top | ZonePos::Bottom);
        if into_library_pos {
            {
                let libv = &mut self.state.player_mut(searcher).library;
                libv.retain(|o| !picks.contains(o));
            }
            self.state.shuffle_library(searcher);
            let libv = &mut self.state.player_mut(searcher).library;
            match to.pos {
                // Top = the vec's tail; push in pick order so the last pick sits on top.
                ZonePos::Top => libv.extend(picks.iter().copied()),
                // Bottom = the vec's front; insert in reverse so the first pick sits deepest.
                ZonePos::Bottom => {
                    for &c in picks.iter().rev() {
                        libv.insert(0, c);
                    }
                }
                _ => {}
            }
            for &c in &picks {
                self.searched_this_resolution.push(c);
                self.broadcast(GameEvent::ObjectMoved { obj: c, to: Zone::Library });
            }
            return;
        }
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
        self.discard_cards(chosen)
    }

    /// "Discard any number of cards" (CR 701.8) — `player` chooses how many (0..hand) and which. The
    /// discarded cards are recorded in `discarded_this_resolution` so a following effect can read the
    /// count (Colossus of the Blood Age / Borrowed Knowledge). Returns the number discarded.
    fn interpret_discard_chosen(&mut self, player: PlayerId) -> usize {
        let hand = self.state.player(player).hand.clone();
        if hand.is_empty() {
            return 0;
        }
        let req = DecisionRequest::SelectCards {
            reason: SelectReason::Discard,
            from: hand.clone(),
            min: 0,
            max: hand.len() as u32,
            description: "Discard any number".into(),
        };
        let mut seen = std::collections::BTreeSet::new();
        let chosen: Vec<ObjId> = match self.ask(player, &req) {
            DecisionResponse::Indices(idxs) => {
                idxs.iter().filter_map(|&i| hand.get(i as usize).copied()).filter(|o| seen.insert(*o)).collect()
            }
            DecisionResponse::Index(i) => {
                hand.get(i as usize).copied().filter(|o| seen.insert(*o)).into_iter().collect()
            }
            _ => Vec::new(),
        };
        self.discard_cards(chosen)
    }

    /// Move `cards` from hand to their owners' graveyards, record them in the per-resolution discard
    /// scratch (`ValueExpr::DiscardedThisResolution`), and broadcast one move event each. Returns the
    /// number discarded. Shared by the fixed-count and "any number" discard paths.
    fn discard_cards(&mut self, cards: Vec<ObjId>) -> usize {
        let discarded = cards.len();
        for card in cards {
            let owner = self.state.object(card).owner;
            self.state.move_object(card, Zone::Graveyard, owner);
            self.discarded_this_resolution.push(card);
            self.broadcast(GameEvent::ObjectMoved { obj: card, to: Zone::Graveyard });
        }
        discarded
    }

    /// Directed discard (CR 701.8, chooser ≠ discarder): reveal `discarder`'s hand, let `chooser`
    /// pick up to `count` cards matching `filter` (revealing is implicit — the chooser's agent sees
    /// the eligible cards), then `discarder` discards the picks. Mandatory up to the number of
    /// eligible cards: if `chooser` under-selects, the front of the eligible set fills in. Returns
    /// the number discarded (for the `IfYouDo`/"performed" flag).
    fn interpret_directed_discard(
        &mut self,
        discarder: PlayerId,
        chooser: PlayerId,
        count: u32,
        filter: &CardFilter,
    ) -> usize {
        let hand = self.state.player(discarder).hand.clone();
        let eligible: Vec<ObjId> =
            hand.iter().copied().filter(|&o| self.count_filter_matches(o, filter)).collect();
        let n = (count as usize).min(eligible.len());
        if n == 0 {
            return 0;
        }
        let req = DecisionRequest::SelectCards {
            reason: SelectReason::Reveal,
            from: eligible.clone(),
            min: n as u32,
            max: n as u32,
            description: "Choose card(s) for that player to discard".into(),
        };
        let mut seen = std::collections::BTreeSet::new();
        let mut chosen: Vec<ObjId> = match self.ask(chooser, &req) {
            DecisionResponse::Indices(idxs) => idxs
                .iter()
                .filter_map(|&i| eligible.get(i as usize).copied())
                .filter(|o| seen.insert(*o))
                .take(n)
                .collect(),
            DecisionResponse::Index(i) => eligible
                .get(i as usize)
                .copied()
                .filter(|o| seen.insert(*o))
                .into_iter()
                .collect(),
            _ => Vec::new(),
        };
        // Mandatory: if the chooser under-picked, fill from the front of the eligible cards.
        for &o in &eligible {
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

    /// Scry N (CR 701.17): show `pl` the top `n` cards of their library (top-first), let them put any
    /// number on the **bottom** of their library, and leave the rest on top. The scry twin of
    /// [`Self::interpret_surveil`] — the same `ScryStage` decision, but the chosen cards go to the
    /// bottom (front of the vec, since the top is the tail) instead of the graveyard.
    fn interpret_scry(&mut self, pl: PlayerId, n: usize) {
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
            description: "Scry".into(),
        };
        let mut seen = std::collections::BTreeSet::new();
        let to_bottom: Vec<ObjId> = match self.ask(pl, &req) {
            DecisionResponse::Indices(idxs) => idxs
                .iter()
                .filter_map(|&i| top.get(i as usize).copied())
                .filter(|o| seen.insert(*o))
                .collect(),
            _ => Vec::new(),
        };
        if to_bottom.is_empty() {
            return;
        }
        // Bottom = front of the vec (top is the tail). Pull the chosen cards out, then re-insert them
        // at the front in reverse so the first-chosen ends up deepest (any order is legal for scry).
        let libv = &mut self.state.player_mut(pl).library;
        libv.retain(|o| !to_bottom.contains(o));
        for &c in to_bottom.iter().rev() {
            libv.insert(0, c);
        }
    }

    /// "Put `n` cards from `pl`'s hand on top of their library in any order" (Brainstorm). The player
    /// selects `n` hand cards (mandatory, capped at hand size) in the order they want them on top; the
    /// first selected ends up on top (drawn first). The library's top is the vec's tail, so the cards
    /// are pushed in reverse of the chosen order (last-chosen pushed first, first-chosen pushed last).
    fn interpret_put_from_hand_on_top(&mut self, pl: PlayerId, n: usize) {
        let hand = self.state.player(pl).hand.clone();
        let count = n.min(hand.len());
        if count == 0 {
            return;
        }
        let req = DecisionRequest::SelectCards {
            reason: SelectReason::Generic,
            from: hand.clone(),
            min: count as u32,
            max: count as u32,
            description: "put cards on top of your library".into(),
        };
        let mut seen = std::collections::BTreeSet::new();
        let mut chosen: Vec<ObjId> = match self.ask(pl, &req) {
            DecisionResponse::Indices(idxs) => idxs
                .iter()
                .filter_map(|&i| hand.get(i as usize).copied())
                .filter(|o| seen.insert(*o))
                .take(count)
                .collect(),
            _ => Vec::new(),
        };
        // Top up to `count` (agent under-picked) from the remaining hand, preserving determinism.
        for &card in &hand {
            if chosen.len() >= count {
                break;
            }
            if !chosen.contains(&card) {
                chosen.push(card);
            }
        }
        // Push in reverse so the first-chosen card ends up on top (the library's top is the tail).
        for card in chosen.iter().rev() {
            let owner = self.state.object(*card).owner;
            self.state.move_object(*card, Zone::Library, owner);
            self.broadcast(GameEvent::ObjectMoved { obj: *card, to: Zone::Library });
        }
    }

    /// "Look at the top `n`, put `take` into `take_to`, the rest into `rest_to`" (SoS look-and-pick).
    /// The controller chooses which of the looked-at cards to take. `rest_to == Library` places the
    /// remainder on the **bottom** of the library.
    /// Ask `player` to choose exactly `want` cards from `pool` (mandatory; if the agent under-selects,
    /// the front of `pool` fills in). Returns the chosen `ObjId`s. Used by multi-destination look effects.
    fn choose_n(&mut self, player: PlayerId, pool: &[ObjId], want: usize, reason: SelectReason, desc: &str) -> Vec<ObjId> {
        if want == 0 || pool.is_empty() {
            return Vec::new();
        }
        let want = want.min(pool.len());
        let resp = self.ask(
            player,
            &DecisionRequest::SelectCards {
                reason,
                from: pool.to_vec(),
                min: want as u32,
                max: want as u32,
                description: desc.into(),
            },
        );
        let mut seen = std::collections::BTreeSet::new();
        let mut picks: Vec<ObjId> = match resp {
            DecisionResponse::Indices(idxs) => idxs
                .iter()
                .filter_map(|&i| pool.get(i as usize).copied())
                .filter(|o| seen.insert(*o))
                .take(want)
                .collect(),
            _ => Vec::new(),
        };
        // Mandatory: fill from the front if the agent under-selected.
        for &c in pool {
            if picks.len() >= want {
                break;
            }
            if seen.insert(c) {
                picks.push(c);
            }
        }
        picks
    }

    /// Lower a **simple** player-scoped effect to concrete `Action`s at registration time, for a
    /// delayed trigger's `actions` (Glimpse of Nature's "draw a card"). Handles the player-targeted
    /// leaves such a "whenever you cast …" reaction uses; a `Sequence` lowers element-wise. An effect
    /// with no arm here yields no actions and `debug_assert!`s loudly (extend as new reactions need it).
    fn lower_effect_to_actions(&self, effect: &Effect, ctx: &ResolutionCtx) -> Vec<Action> {
        match effect {
            Effect::Draw { who, count } => vec![Action::Draw {
                player: self.eval_player(*who, ctx),
                count: self.eval_value(count, ctx).max(0) as u32,
            }],
            Effect::LoseLife { who, amount } => vec![Action::LoseLife {
                player: self.eval_player(*who, ctx),
                amount: self.eval_value(amount, ctx).max(0) as u32,
            }],
            Effect::Sequence(es) => es.iter().flat_map(|e| self.lower_effect_to_actions(e, ctx)).collect(),
            other => {
                debug_assert!(false, "lower_effect_to_actions: unsupported delayed-cast reaction {other:?}");
                Vec::new()
            }
        }
    }

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

    /// Zimone's Experiment: look at the top `n`, choose up to `take` creature/land cards, route lands to
    /// the battlefield tapped and creatures to hand, and bottom the rest in a random order.
    fn interpret_look_pick_creatures_lands(&mut self, pl: PlayerId, n: usize, take: usize) {
        let lib = &self.state.player(pl).library;
        let count = n.min(lib.len());
        if count == 0 {
            return;
        }
        // Top-first (the library's top is the vec's tail).
        let top: Vec<ObjId> = lib.iter().rev().take(count).copied().collect();
        // Takeable = creature and/or land cards among them.
        let is_cl = |st: &Self, o: ObjId| {
            let cc = st.state.computed(o);
            cc.is_creature() || cc.card_types.contains(&CardType::Land)
        };
        let takeable: Vec<ObjId> = top.iter().copied().filter(|&o| is_cl(self, o)).collect();
        let take_n = take.min(takeable.len());
        // "You may reveal up to `take`" — optional (min 0).
        let mut seen = std::collections::BTreeSet::new();
        let taken: Vec<ObjId> = if take_n == 0 {
            Vec::new()
        } else {
            let req = DecisionRequest::SelectCards {
                reason: SelectReason::ScryStage,
                from: takeable.clone(),
                min: 0,
                max: take_n as u32,
                description: "Reveal up to two creature and/or land cards".into(),
            };
            match self.ask(pl, &req) {
                DecisionResponse::Indices(idxs) => idxs
                    .iter()
                    .filter_map(|&i| takeable.get(i as usize).copied())
                    .filter(|o| seen.insert(*o))
                    .take(take_n)
                    .collect(),
                _ => Vec::new(),
            }
        };
        // Route each taken card by type: lands → battlefield tapped, creatures → hand.
        for &card in &taken {
            let owner = self.state.object(card).owner;
            if self.state.computed(card).card_types.contains(&CardType::Land) {
                self.state.move_object(card, Zone::Battlefield, owner);
                if let Some(o) = self.state.objects.get_mut(&card) {
                    o.status.tapped = true;
                }
                self.broadcast(GameEvent::ObjectMoved { obj: card, to: Zone::Battlefield });
            } else {
                self.state.move_object(card, Zone::Hand, owner);
                self.broadcast(GameEvent::ObjectMoved { obj: card, to: Zone::Hand });
            }
        }
        self.state.mark_chars_dirty();
        // The rest — the looked-at cards not taken — go to the bottom in a random order.
        let mut rest: Vec<ObjId> = top.iter().filter(|o| !taken.contains(o)).copied().collect();
        self.state.rng.shuffle(&mut rest);
        let libv = &mut self.state.player_mut(pl).library;
        libv.retain(|o| !rest.contains(o));
        for card in rest.iter().rev() {
            libv.insert(0, *card);
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
            // A "would die → exile instead" rider (CR 614) redirects a sacrifice — a battlefield→
            // graveyard death (CR 700.4) — to exile too (constraint: "dies" is any such move, not
            // just destruction). `death_zone_for` returns Graveyard for an unaffected creature.
            let dest = self.death_zone_for(obj);
            self.state.move_object(obj, dest, owner);
            self.broadcast(GameEvent::ObjectMoved { obj, to: dest });
        }
        sacrificed
    }

    /// Counter the stack object with id `sid` (CR 701.5): remove it from the stack; a countered
    /// **spell** goes to its owner's graveyard (701.5a), a countered **ability** simply ceases to
    /// exist. A spell that "can't be countered" (`CantBeCountered`, CR 701.5f — read from its
    /// computed characteristics, which now include stack-zone statics like Surrak's) is left on the
    /// stack untouched, so it will still resolve.
    /// Return a target spell on the stack to its owner's hand (CR 701 — Reprieve). The spell leaves
    /// the stack without resolving; a copy ceases to exist instead. Not a counter, so it isn't stopped
    /// by can't-be-countered.
    fn interpret_return_spell_to_hand(&mut self, sid: StackId) {
        let Some(so) = self.state.stack.items.iter().find(|s| s.id == sid).cloned() else {
            return;
        };
        self.state.stack.items.retain(|s| s.id != sid);
        if let crate::stack::StackObjectKind::Spell(card) = so.kind {
            if self.state.object(card).is_copy {
                self.state.cease_to_exist(card); // a copy (CR 707.10a) can't go to hand.
            } else {
                let owner = self.state.object(card).owner;
                self.state.move_object(card, Zone::Hand, owner);
                self.broadcast(GameEvent::ObjectMoved { obj: card, to: Zone::Hand });
            }
        }
    }

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
            // A countered copy ceases to exist (CR 707.10a) rather than going to a graveyard.
            if self.state.object(card).is_copy {
                self.state.cease_to_exist(card);
            } else {
                let owner = self.state.object(card).owner;
                // A flashback-cast spell — or a Nita exile-on-leave cast — is exiled instead of going
                // to the graveyard even when countered (CR 702.34d — the exile replacement applies
                // whenever it would leave the stack, including a counter).
                let dest = if self.state.object(card).flashback_cast {
                    Zone::Exile
                } else {
                    Zone::Graveyard
                };
                self.state.move_object(card, dest, owner);
                self.broadcast(GameEvent::ObjectMoved { obj: card, to: dest });
            }
        } else {
            // An activated/triggered ability that is countered just leaves the stack (CR 701.5b).
            self.state.stack.items.retain(|s| s.id != sid);
        }
    }

    /// Select the objects a `ForEach`/`Select` ranges over: the `chooser`'s objects in `selector.zone`
    /// matching its filter, narrowed to `[min, max]` (asking which when there are more than `max`).
    /// Returns empty if fewer than `min` candidates exist (the "for each of two …" can't be met).
    /// Resolve a [`CardFilter`]'s dynamic parts (`ManaValueExpr`) into concrete predicates against
    /// the resolution context, so a ctx-free matcher (`count_filter_matches`) only ever sees static
    /// filters. Recurses through the boolean combinators; every other variant is returned unchanged.
    fn resolve_dynamic_filter(&self, filter: &CardFilter, ctx: &ResolutionCtx) -> CardFilter {
        match filter {
            CardFilter::ManaValueExpr { min, max } => CardFilter::ManaValue {
                min: min.as_ref().map(|e| self.eval_value(e, ctx).max(0) as u32),
                max: max.as_ref().map(|e| self.eval_value(e, ctx).max(0) as u32),
            },
            CardFilter::All(fs) => {
                CardFilter::All(fs.iter().map(|f| self.resolve_dynamic_filter(f, ctx)).collect())
            }
            CardFilter::AnyOf(fs) => {
                CardFilter::AnyOf(fs.iter().map(|f| self.resolve_dynamic_filter(f, ctx)).collect())
            }
            CardFilter::Not(f) => CardFilter::Not(Box::new(self.resolve_dynamic_filter(f, ctx))),
            other => other.clone(),
        }
    }

    fn select_for_each(&mut self, selector: &SelectSpec, ctx: &ResolutionCtx) -> Vec<ObjId> {
        let min = self.eval_value(&selector.min, ctx).max(0) as usize;
        let max = self.eval_value(&selector.max, ctx).max(0) as usize;
        // Which players' objects the selector spans, and who decides when there's a real choice.
        // `EachPlayer` spans ALL players (an "each creature and planeswalker" area effect — Splatter
        // Technique); every other `PlayerRef` is a single player who is both the source and the decider.
        let (source_players, decider): (Vec<PlayerId>, PlayerId) = match selector.chooser {
            PlayerRef::EachPlayer => (
                (0..self.state.players.len() as u32).map(PlayerId).collect(),
                ctx.controller.unwrap_or(PlayerId(0)),
            ),
            other => {
                let p = self.eval_player(other, ctx);
                (vec![p], p)
            }
        };
        // Resolve any dynamic (X-keyed) filter to a concrete one against this resolution's ctx.
        let filter = self.resolve_dynamic_filter(&selector.filter, ctx);
        let candidates: Vec<ObjId> = source_players
            .iter()
            .flat_map(|&p| self.state.player(p).zone_ids(selector.zone).to_vec())
            .filter(|&id| self.count_filter_matches(id, &filter))
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
        let idxs: Vec<usize> = match self.ask(decider, &req) {
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
    pub(crate) fn add_mana(&mut self, player: PlayerId, mana: &ManaSpec, ctx: &ResolutionCtx) {
        // Restricted mana (CR 106.6, "spend only to cast instant/sorcery spells") floats in the pool's
        // separate `restricted` bucket so it can't later pay a creature spell or an ability cost.
        let restricted = mana.restriction.is_some();
        let mut changed = false;
        for (color, amount) in &mana.produces {
            let amt = self.eval_value(amount, ctx).max(0) as u32;
            if amt > 0 {
                let pool = &mut self.state.player_mut(player).mana_pool;
                let bucket = if restricted { &mut pool.restricted } else { &mut pool.amounts };
                *bucket.entry(*color).or_insert(0) += amt;
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
                let pool = &mut self.state.player_mut(player).mana_pool;
                let bucket = if restricted { &mut pool.restricted } else { &mut pool.amounts };
                *bucket.entry(color).or_insert(0) += amt;
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
            // "`source` deals `amount` damage to `to`" (CR 119.2) — the damaging object is `source`,
            // not the resolving spell. Resolve `source` then `to` (in that cursor order — matching the
            // slot order `collect_specs_into` produces), so `to` can be a declined "up to one" (no
            // damage). A missing `source` object degrades to the spell (`ctx.source`).
            Effect::SourcedDamage { source, to, amount, kind } => {
                let amount = self.eval_value(amount, ctx).max(0) as u32;
                let src_obj = match self.resolve_target(source, ctx, cursor) {
                    Some(Target::Object(o)) => o,
                    _ => ctx.source.unwrap_or(ObjId(0)),
                };
                if let Some(target) = self.resolve_target(to, ctx, cursor) {
                    wb.push(Action::Damage { target, amount, source: src_obj, kind: *kind });
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
            // "This creature becomes prepared" (SoS Prepare) — set the status on the ability's source.
            // Every "becomes prepared" clause funnels through this one leaf, so any prepare-trigger
            // variant is an ordinary ability with no bespoke machinery.
            Effect::BecomePrepared => {
                if let Some(obj) = ctx.source {
                    wb.push(Action::SetPrepared { obj, prepared: true });
                }
            }
            // "Target creature becomes prepared / unprepared" — set the status on a chosen target.
            Effect::SetPrepared { what, prepared } => {
                if let Some(Target::Object(obj)) = self.resolve_target(what, ctx, cursor) {
                    wb.push(Action::SetPrepared { obj, prepared: *prepared });
                }
            }
            // C17: exile a target (e.g. "{1}: Exile target card from a graveyard"). `source` is
            // carried so the exile can later be associated with its source (linked-exile sets).
            Effect::Exile { what } => {
                if let Some(Target::Object(obj)) = self.resolve_target(what, ctx, cursor) {
                    wb.push(Action::Exile { obj, source: ctx.source });
                }
            }
            // Nita: exile the target (opponent's gy card) and grant the CONTROLLER cross-player
            // permission to cast it this turn, with the any-mana / exile-on-leave riders.
            Effect::ExileTargetThenMayCast { what, any_mana, exile_on_leave } => {
                if let Some(Target::Object(obj)) = self.resolve_target(what, ctx, cursor) {
                    let by = ctx.controller.unwrap_or_else(|| self.state.object(obj).owner);
                    wb.push(Action::ExileForCastBy {
                        obj,
                        by,
                        until: self.state.turn_number, // "this turn" (CR 601.3e window)
                        any_mana: *any_mana,
                        exile_on_leave: *exile_on_leave,
                    });
                }
            }
            // Impulse-play: exile the target and grant its owner play-from-exile permission through
            // the end of `window` (CR 400.7 turn arithmetic; 2-player alternation).
            Effect::ExileForPlay { what, window } => {
                if let Some(Target::Object(obj)) = self.resolve_target(what, ctx, cursor) {
                    let owner = self.state.object(obj).owner;
                    let until = match window {
                        crate::effects::PlayWindow::ThisTurn => self.state.turn_number,
                        // The owner's next turn: +2 if it's already their turn, else +1 (2-player).
                        crate::effects::PlayWindow::YourNextTurn => {
                            if self.state.active_player == owner {
                                self.state.turn_number + 2
                            } else {
                                self.state.turn_number + 1
                            }
                        }
                    };
                    wb.push(Action::ExileForPlay { obj, until });
                }
            }
            // "Mill a card. You may play that card this turn." (Ark of Hunger / Tablet of Discovery) —
            // the window is anchored to the *milling player* (CR 400.7), not a target's owner.
            Effect::MillThenPlay { who, window } => {
                let player = self.eval_player(*who, ctx);
                let until = match window {
                    crate::effects::PlayWindow::ThisTurn => self.state.turn_number,
                    crate::effects::PlayWindow::YourNextTurn => {
                        if self.state.active_player == player {
                            self.state.turn_number + 2
                        } else {
                            self.state.turn_number + 1
                        }
                    }
                };
                wb.push(Action::MillForPlay { player, until });
            }
            // "Add … mana at the beginning of your next main phase" (Mana Sculpt) — evaluate the amount
            // NOW (e.g. the still-on-stack countered spell's mana_spent) and arm a delayed trigger.
            Effect::AddManaAtNextMainPhase { who, color, amount } => {
                let player = self.eval_player(*who, ctx);
                let amt = self.eval_value(amount, ctx).max(0) as u32;
                if amt > 0 {
                    wb.push(Action::RegisterDelayedTrigger {
                        watching: ctx.source.unwrap_or(ObjId(0)),
                        event: crate::effects::action::DelayedTriggerEvent::AtBeginningOfYourNextMainPhase,
                        controller: player,
                        source: ctx.source,
                        actions: vec![Action::AddMana { player, color: *color, amount: amt }],
                    });
                }
            }
            // Move targeted object(s) to another zone (CR 400.7 / 608.2) — "return target permanent
            // to its owner's hand" (bounce), "return target creature card from your graveyard to the
            // battlefield" (reanimate), "return up to two target creature cards … to your hand"
            // (Pull from the Grave), etc. Each object lowers to one `Action::MoveZone` with
            // `MoveCause::Returned` (a non-death leave, so LTB — not dies — triggers fire, and an
            // enter fires ETB).
            //
            // Multi-target: a `max > 1` slot flattens all its picks into `chosen_targets` (one entry
            // per chosen candidate, in slot order — see `parse_targets`), so it occupies several
            // consecutive cursor positions. Emit one move per chosen object, taking up to `max`.
            // `max == 1` is the ordinary single-target return. Invariant: a `max > 1` slot must be
            // the LAST targeting sub-effect of its spell — the flat cursor can't tell where one
            // multi-slot's picks end and a following targeting slot begins; every real card ("return
            // up to N …") satisfies this (later clauses are non-targeting, e.g. "You gain 2 life").
            Effect::MoveZone { what, to, tapped } => {
                let max = match what {
                    EffectTarget::Target(spec) => spec.max.max(1),
                    _ => 1,
                };
                for _ in 0..max {
                    match self.resolve_target(what, ctx, cursor) {
                        Some(Target::Object(obj)) => wb.push(Action::MoveZone {
                            obj,
                            to: to.zone,
                            pos: to.pos,
                            cause: MoveCause::Returned,
                            tapped: *tapped,
                        }),
                        _ => break,
                    }
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
            // "Move `count` `kind` counters from `from` onto `to`" (CR 121.6). Cap at what's on `from`
            // (move-more-than-exists → move all); remove n from `from`, add n to `to` — both in this one
            // whiteboard step so nothing observes a half-moved state. Resolve `from` before `to` so the
            // cursor advances in source order.
            Effect::MoveCounters { from, to, kind, count } => {
                let want = self.eval_value(count, ctx).max(0);
                let from_obj = match self.resolve_target(from, ctx, cursor) {
                    Some(Target::Object(o)) => Some(o),
                    _ => None,
                };
                let to_obj = match self.resolve_target(to, ctx, cursor) {
                    Some(Target::Object(o)) => Some(o),
                    _ => None,
                };
                if let (Some(from_obj), Some(to_obj)) = (from_obj, to_obj) {
                    let available =
                        self.state.objects.get(&from_obj).map(|o| o.counters.get(kind) as i64).unwrap_or(0);
                    let n = want.min(available) as i32;
                    if n > 0 {
                        wb.push(Action::AddCounters { obj: from_obj, kind: kind.clone(), n: -n });
                        wb.push(Action::AddCounters { obj: to_obj, kind: kind.clone(), n });
                    }
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
                                tapped: false,
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
            // Grant a triggered ability for a duration (CR 613.1f) — "gains 'When this dies, draw a
            // card' until end of turn". The template def (9800+) supplies the granted trigger.
            Effect::GrantAbility { what, template_grp, duration } => {
                if let Some(Target::Object(obj)) = self.resolve_target(what, ctx, cursor) {
                    let controller = ctx.controller.unwrap_or(PlayerId(0));
                    wb.push(Action::GrantContinuous {
                        source: ctx.source,
                        controller,
                        affected: vec![obj],
                        contributions: vec![StaticContribution::GrantAbility {
                            template_grp: *template_grp,
                        }],
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
            // "[target] becomes …" (CR 611.2 / 613) — grant a bag of layer contributions plus an
            // optional concrete base P/T (its `ValueExpr`s resolved now — Fractalize's X+1) as one
            // continuous effect. Fractalize; Great Hall's {5} animation.
            Effect::Becomes { what, contributions, base_pt, duration } => {
                if let Some(Target::Object(obj)) = self.resolve_target(what, ctx, cursor) {
                    let controller = ctx.controller.unwrap_or(PlayerId(0));
                    let mut contribs = contributions.clone();
                    if let Some((p, t)) = base_pt {
                        contribs.push(StaticContribution::SetBasePT {
                            power: self.eval_value(p, ctx) as i32,
                            toughness: self.eval_value(t, ctx) as i32,
                        });
                    }
                    wb.push(Action::GrantContinuous {
                        source: ctx.source,
                        controller,
                        affected: vec![obj],
                        contributions: contribs,
                        duration: *duration,
                    });
                }
            }
            // Set the target's base P/T for a duration (CR 613 layer 7b) — "base power and toughness
            // 5/5 until end of turn" (Quandrix Charm).
            Effect::SetBasePT { what, power, toughness, duration } => {
                if let Some(Target::Object(obj)) = self.resolve_target(what, ctx, cursor) {
                    let controller = ctx.controller.unwrap_or(PlayerId(0));
                    wb.push(Action::GrantContinuous {
                        source: ctx.source,
                        controller,
                        affected: vec![obj],
                        contributions: vec![StaticContribution::SetBasePT {
                            power: *power,
                            toughness: *toughness,
                        }],
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
            Effect::CreateToken { spec, count, controller, dynamic_counters } => {
                let count = self.eval_value(count, ctx).max(0) as u32;
                let controller = self.eval_player(*controller, ctx);
                // Bake resolution-time counter counts (e.g. "X +1/+1 counters on it") onto the token's
                // spec, so each created token enters with them (CR 614.1e / 111.4). Fixed-counter
                // tokens leave `dynamic_counters` empty and keep only `spec.counters`.
                let mut spec = spec.clone();
                for (kind, n) in dynamic_counters {
                    let n = self.eval_value(n, ctx).max(0);
                    if n > 0 {
                        spec.counters.push((kind.clone(), n as u32));
                    }
                }
                for _ in 0..count {
                    wb.push(Action::CreateToken {
                        spec: spec.clone(),
                        controller,
                    });
                }
            }
            // "You get an emblem with …" (CR 114) — put an emblem carrying the registered def's
            // abilities into the controller's command zone.
            Effect::CreateEmblem { emblem } => {
                let controller = ctx.controller.unwrap_or(PlayerId(0));
                wb.push(Action::CreateEmblem { emblem_grp: *emblem, controller });
            }
            // "If [what] would die this turn, exile it instead" (CR 614) — register a one-shot floating
            // replacement scoped to the object for the rest of this turn (Wilt in the Heat).
            Effect::ExileIfWouldDie { what } => {
                if let Some(Target::Object(obj)) = self.resolve_target(what, ctx, cursor) {
                    wb.push(Action::AddFloatingReplacement {
                        scope: obj,
                        pattern: ActionPattern::WouldDie(CardFilter::Any),
                        rewrite: FloatingRewrite::ExileInstead,
                        until_turn: self.state.turn_number,
                        one_shot: true,
                    });
                }
            }
            // "That [cast] creature enters with N extra counters" (CR 614.1e) — arm a one-shot floating
            // replacement scoped to the (still-on-stack) spell; `n` is fixed now (Wildgrowth Archaic).
            // `Triggering` names the "whenever you cast …" spell via `ctx.triggering_spell` directly
            // (like CopySpellOnStack — `resolve_target(Triggering)` is the Ward targeting-source, not
            // this); a resolved Target/Stack naming a spell also works.
            Effect::EntersWithCountersRider { what, kind, n } => {
                let spell = match what {
                    EffectTarget::Triggering => ctx.triggering_spell,
                    _ => match self.resolve_target(what, ctx, cursor) {
                        Some(Target::Object(o)) => Some(o),
                        Some(Target::Stack(sid)) => self
                            .state
                            .stack
                            .items
                            .iter()
                            .find(|it| it.id == sid)
                            .and_then(|it| match it.kind {
                                crate::stack::StackObjectKind::Spell(o) => Some(o),
                                _ => None,
                            }),
                        _ => None,
                    },
                };
                if let Some(obj) = spell {
                    let count = self.eval_value(n, ctx).max(0) as u32;
                    if count > 0 {
                        wb.push(Action::AddFloatingReplacement {
                            scope: obj,
                            pattern: ActionPattern::WouldEnterBattlefield(CardFilter::ItSelf),
                            rewrite: FloatingRewrite::EntersWithCounters { kind: kind.clone(), n: count },
                            until_turn: self.state.turn_number,
                            one_shot: true,
                        });
                    }
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
            Effect::TargetPlayer(_) => {
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
            | Effect::Spree { .. }
            | Effect::ChangeTarget { .. }
            | Effect::CreateRoleToken { .. }
            | Effect::Optional { .. }
            | Effect::IfYouDo { .. }
            | Effect::ForEach { .. }
            | Effect::ForEachPlayer { .. }
            | Effect::ForEachTarget { .. }
            | Effect::DealDamageExcessImpulse { .. }
            | Effect::ExileTopForPlay { .. }
            | Effect::Search { .. }
            | Effect::AddMana { .. }
            | Effect::Discard { .. }
            | Effect::DiscardChosen { .. }
            | Effect::PutDiscardedOntoBattlefield { .. }
            | Effect::SetNoMaxHandSize { .. }
            | Effect::GrantChosenKeyword { .. }
            | Effect::Counter { .. }
            | Effect::ReturnSpellToHand { .. }
            | Effect::CounterUnlessPay { .. }
            | Effect::CastCopy { .. }
            | Effect::CastForFree { .. }
            | Effect::ExileTopUntilManaValueMayCastFree { .. }
            | Effect::MillThenPutCreatureOntoBattlefield { .. }
            | Effect::RevealFromTopUntilToHand { .. }
            | Effect::RevealTopLoseLifeMayRepeat
            | Effect::GrantFlashbackUntilEndOfTurn { .. }
            | Effect::ReanimateUnderControl { .. }
            | Effect::Blink { .. }
            | Effect::ExileReturnNextEndStep { .. }
            | Effect::CopyNextSpellCast { .. }
            | Effect::WheneverYouCastThisTurn { .. }
            | Effect::CopySpellOnStack { .. }
            | Effect::CopySpellAsToken { .. }
            | Effect::Cascade
            | Effect::MayTapOrUntap { .. }
            | Effect::PutOnTopOrBottom { .. }
            | Effect::PutFromHandOnTop { .. }
            | Effect::MayPayCost { .. }
            | Effect::Sacrifice { .. }
            | Effect::Surveil { .. }
            | Effect::Scry { .. }
            | Effect::FlipCoinsSkipNextTurns { .. }
            | Effect::LookAndPick { .. }
            | Effect::LookDistribute { .. }
            | Effect::LookPickCreaturesLands { .. }
            | Effect::DirectedDiscard { .. }
            | Effect::ChooseLandName { .. }
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
            let floating_idx = chosen.floating_idx;
            let rw = chosen.rewrite.clone();
            // Printed statics dedup by (source, ability, affected) (CR 614.5). Floating riders don't
            // (they dedup by removal below); a floating whose rewrite changes the action — e.g.
            // ExileInstead turning a death into an Exile — also can't re-match, so it can't loop.
            if floating_idx.is_none() {
                applied.push((chosen.source, chosen.idx, affected));
            }
            self.apply_rewrite(&rw, wb, ai, affected);
            // A one-shot floating replacement (CR 614.5) is consumed on application. `floating_idx`
            // stays valid: nothing between the scan above and here mutates `floating_replacements`.
            if let Some(fi) = floating_idx {
                if self.state.floating_replacements.get(fi).is_some_and(|f| f.one_shot) {
                    self.state.floating_replacements.remove(fi);
                }
            }
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
                            floating_idx: None,
                        });
                    }
                }
            }
        }
        // Floating replacements (CR 614) created at resolution (Wilt in the Heat), scoped to a single
        // object. Consulted by the SAME pass so CR 616.1 ordering (ChooseReplacement) applies uniformly.
        for (fi, fr) in self.state.floating_replacements.iter().enumerate() {
            if fr.scope != affected {
                continue;
            }
            // The scope IS the source for filter/`ItSelf` purposes.
            if self.pattern_matches(&fr.pattern, action, affected, fr.scope) {
                let rewrite = match &fr.rewrite {
                    FloatingRewrite::ExileInstead => Rewrite::ExileInstead,
                    FloatingRewrite::EntersWithCounters { kind, n } => {
                        Rewrite::EntersWithCounters { kind: kind.clone(), n: *n }
                    }
                };
                out.push(Applicable {
                    source: fr.scope,
                    idx: usize::MAX, // synthetic; floating dedup is by removal, not the `applied` key
                    rewrite: rewrite.clone(),
                    description: describe_rewrite(&rewrite),
                    floating_idx: Some(fi),
                });
            }
        }
        out
    }

    /// The zone a dying permanent actually moves to, after replacement effects (CR 614): `Graveyard`
    /// normally, or `Exile` if a "would die → exile instead" rider applies (Wilt in the Heat). Used by
    /// the death paths that take a **direct `move_object`** rather than a whiteboard `Action` — the SBA
    /// creature-death (CR 704.5f/g/h) and sacrifice (`interpret_sacrifice`) — so they reuse the SAME
    /// replacement machinery ([`Self::applicable_replacements`]) as the rewrite pass, keeping floating
    /// riders + printed statics on one pipeline. A one-shot floating rider is consumed here.
    /// (Destroy-*effect* deaths already run `Action::Destroy` through the rewrite pass — its
    /// `Rewrite::ExileInstead` arm does the same redirect.) Only handles the exile-redirect today; a
    /// future death-*preventing* rewrite would add the CR 616.1f `choose_replacement` path here.
    pub(crate) fn death_zone_for(&mut self, creature: ObjId) -> Zone {
        // Query the replacement set with a canonical death action; `WouldDie` matches it.
        let query = Action::Destroy { obj: creature, source: None };
        let applicable = self.applicable_replacements(&query, creature, &[]);
        let Some(chosen) = applicable
            .iter()
            .find(|a| matches!(a.rewrite, Rewrite::ExileInstead))
        else {
            return Zone::Graveyard;
        };
        // Consume a one-shot floating rider (CR 614.5) as it applies.
        if let Some(fi) = chosen.floating_idx {
            if self.state.floating_replacements.get(fi).is_some_and(|f| f.one_shot) {
                self.state.floating_replacements.remove(fi);
            }
        }
        Zone::Exile
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
            // "would die" = a battlefield→graveyard move of the affected object (CR 700.4). Matches
            // any death action (Destroy / Sacrifice / MoveZone→graveyard); requires the object to be
            // ON the battlefield, so mill (library→graveyard) / discard (hand→graveyard) don't count.
            (ActionPattern::WouldDie(filter), Action::Destroy { obj: o, .. })
            | (ActionPattern::WouldDie(filter), Action::Sacrifice { obj: o, .. })
            | (
                ActionPattern::WouldDie(filter),
                Action::MoveZone { obj: o, to: Zone::Graveyard, .. },
            ) => {
                *o == affected
                    && self.state.objects.get(o).is_some_and(|x| x.zone == Zone::Battlefield)
                    && self.filter_matches(filter, affected, source)
            }
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
                // "Enters tapped unless <condition>": tap iff the condition fails, evaluated for the
                // entering permanent's controller. Threads the object as source and the resolution's X
                // (so "enters tapped if X ≤ 2" = `EntersTappedUnless(ValueAtLeast(X, 3))` reads the
                // creature's own cast X — Slumbering Trudge). Non-value conditions (check lands' "unless
                // you control a basic") still route controller-relative through `cond_holds`. No choice.
                let controller =
                    self.state.objects.get(&obj).map(|o| o.controller).unwrap_or(PlayerId(0));
                let ctx = ResolutionCtx {
                    controller: Some(controller),
                    source: Some(obj),
                    x: wb.ctx.x,
                    ..Default::default()
                };
                if !self.cond_holds(cond, &ctx) {
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
            // "Exile it instead of dying" (CR 614): replace the death action (Destroy / Sacrifice /
            // MoveZone→graveyard) with an Exile of the same object, in place, so it never reaches the
            // graveyard. Preserves the exiling source when the death action carried one.
            Rewrite::ExileInstead => {
                let source = match &wb.actions[ai] {
                    Action::Destroy { source, .. } => *source,
                    _ => None,
                };
                wb.actions[ai] = Action::Exile { obj, source };
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
                let mut on_battlefield = false;
                if let Some(o) = self.state.objects.get_mut(&obj) {
                    let cur = o.counters.counts.entry(kind.clone()).or_insert(0);
                    *cur = (*cur as i32 + n).max(0) as u32;
                    // "you put a counter on this creature this turn" (Fractal Tender) — any counter
                    // kind, only actual additions (positive `n`, not a removal).
                    if n > 0 {
                        o.counter_added_this_turn = true;
                        on_battlefield = o.zone == Zone::Battlefield;
                    }
                }
                // +1/+1 / -1/-1 counters change computed P/T (CR 613 layer 7c).
                self.state.mark_chars_dirty();
                // "Whenever one or more counters are put on this permanent" (CR 603.2) — fire once per
                // counter-adding event (Pensive Professor / Berta). Only for permanents in play.
                if n > 0 && on_battlefield {
                    self.broadcast(GameEvent::CountersPut { obj, kind, count: n as u32 });
                }
            }
            Action::TapUntap { obj, tap } => {
                if let Some(o) = self.state.objects.get_mut(&obj) {
                    o.status.tapped = tap;
                }
            }
            Action::MoveZone { obj, to, tapped, .. } => {
                let owner = match self.state.objects.get(&obj) {
                    Some(o) => o.owner,
                    None => return,
                };
                if self.state.move_object(obj, to, owner) {
                    // Enter tapped (CR 110.5) — `move_object` reset status to untapped, so apply the
                    // tap now that it's on the battlefield (Teacher's Pest's tapped reanimation).
                    if tapped && to == Zone::Battlefield {
                        if let Some(o) = self.state.objects.get_mut(&obj) {
                            o.status.tapped = true;
                        }
                    }
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
            // Sacrifice a permanent as an applied action (CR 701.16) — the effect-side analogue of
            // the sacrifice *cost*, so a delayed/reflexive trigger can sacrifice a permanent ("at the
            // beginning of the end step, sacrifice this token" — Choreographed Sparks). Mirrors
            // `interpret_sacrifice`'s per-object body: route through `death_zone_for` so a "would die →
            // exile instead" rider (CR 614) redirects it, then broadcast the death move (drives
            // dies-triggers). No indestructible check — sacrifice ignores indestructible (CR 701.16b).
            Action::Sacrifice { obj, .. } => {
                if let Some(o) = self.state.objects.get(&obj) {
                    if o.zone == Zone::Battlefield {
                        let owner = o.owner;
                        let dest = self.death_zone_for(obj);
                        self.state.move_object(obj, dest, owner);
                        self.broadcast(GameEvent::ObjectMoved { obj, to: dest });
                    }
                }
            }
            Action::Mill { player, count } => self.mill(player, count),
            // Mill the top card and grant permission to play it from the graveyard until `until`.
            Action::MillForPlay { player, until } => {
                if let Some(top) = self.state.player(player).library.last().copied() {
                    // move_object resets the flags (400.7), so set them AFTER the move.
                    if self.state.move_object(top, Zone::Graveyard, player) {
                        if let Some(o) = self.state.objects.get_mut(&top) {
                            o.playable_from_graveyard = true;
                            o.play_until_turn = Some(until);
                        }
                        self.broadcast(GameEvent::ObjectMoved { obj: top, to: Zone::Graveyard });
                    }
                }
            }
            // Add concrete mana to a pool (a delayed-trigger action — Mana Sculpt's delayed {C}).
            Action::AddMana { player, color, amount } => {
                if amount > 0 {
                    *self.state.player_mut(player).mana_pool.amounts.entry(color).or_insert(0) += amount;
                    self.broadcast(GameEvent::ManaPoolChanged { player });
                }
            }
            Action::CreateToken { spec, controller } => self.create_token(&spec, controller),
            Action::CreateEmblem { emblem_grp, controller } => self.create_emblem(emblem_grp, controller),
            Action::SetPrepared { obj, prepared } => {
                if let Some(o) = self.state.objects.get_mut(&obj) {
                    o.prepared = prepared;
                }
            }
            Action::AddFloatingReplacement { scope, pattern, rewrite, until_turn, one_shot } => {
                self.state.floating_replacements.push(crate::state::FloatingReplacement {
                    scope,
                    pattern,
                    rewrite,
                    until_turn,
                    one_shot,
                });
            }
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
            Action::ExileForPlay { obj, until } => {
                let owner = match self.state.objects.get(&obj) {
                    Some(o) => o.owner,
                    None => return,
                };
                // move_object resets the flags (400.7), so set them AFTER the move.
                if self.state.move_object(obj, Zone::Exile, owner) {
                    if let Some(o) = self.state.objects.get_mut(&obj) {
                        o.castable_from_exile = true;
                        o.play_until_turn = Some(until);
                    }
                    self.broadcast(GameEvent::ObjectMoved { obj, to: Zone::Exile });
                }
            }
            Action::ExileForCastBy { obj, by, until, any_mana, exile_on_leave } => {
                // Exile to the OWNER's exile (CR 400.7) but grant the cast permission to `by` (Nita's
                // controller). Set the flags AFTER the move (move_object resets them).
                let owner = match self.state.objects.get(&obj) {
                    Some(o) => o.owner,
                    None => return,
                };
                if self.state.move_object(obj, Zone::Exile, owner) {
                    if let Some(o) = self.state.objects.get_mut(&obj) {
                        o.castable_from_exile = true;
                        o.castable_by = Some(by);
                        o.play_until_turn = Some(until);
                        o.spend_any_mana = any_mana;
                        o.exile_on_leave = exile_on_leave;
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
        // Only creatures carry P/T (CR 208.1) — a non-creature token (a Treasure artifact) has no P/T,
        // so it isn't mistaken for a 0/0 by power/toughness readers or the toughness-0 SBA.
        let is_creature_spec = spec.card_types.contains(&CardType::Creature);
        let chars = Characteristics {
            name: spec.name.clone(),
            card_types: spec.card_types.clone(),
            subtypes: spec.subtypes.clone(),
            // Every created token IS a token (CR 111.1) — stamp the Token supertype so filters can tell
            // token from nontoken (Sheoldred's Edict, Lorehold Charm's "nontoken artifact", and the
            // "tokens you control enter" trigger class all depend on this).
            supertypes: vec![crate::subtypes::Supertype::Token],
            colors: spec.colors.clone(),
            power: is_creature_spec.then_some(spec.power),
            toughness: is_creature_spec.then_some(spec.toughness),
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

    /// Create a **Role Aura token** (`role_grp`, a registered def in the 9000+ token block) attached to
    /// `host`, under `controller` (CR 111 / the Role subsystem). First enforces the "one Role per
    /// controller on a permanent" rule (CR 303.4k reminder): any Role token `controller` already
    /// controls attached to `host` is put into its owner's graveyard (where the token cease-to-exist SBA
    /// removes it, CR 111.7) — newest survives. The new token enters the battlefield already attached.
    ///
    /// ⚠️ Timing approximation: the prior Role is moved to the graveyard **immediately** at attach, not
    /// via the general CR 303.4k "put the one entering the graveyard as an SBA after both are on the
    /// battlefield" batch. Observationally identical in the SoS pool (no card cares about the interstitial
    /// two-Roles state), and it keeps the "newest survives" outcome exact.
    fn create_role_token(&mut self, role_grp: u32, host: ObjId, controller: PlayerId) {
        use crate::subtypes::{EnchantmentType, Subtype};
        // "If you control another Role on it, put that one into the graveyard." A Role is any token
        // carrying the Role enchantment subtype; scope to this controller + this host.
        let priors: Vec<ObjId> = self
            .state
            .objects
            .values()
            .filter(|o| {
                o.controller == controller
                    && o.attached_to == Some(host)
                    && o.chars.subtypes.contains(&Subtype::Enchantment(EnchantmentType::Role))
            })
            .map(|o| o.id)
            .collect();
        for role in priors {
            let owner = self.state.object(role).owner;
            if self.state.move_object(role, Zone::Graveyard, owner) {
                self.broadcast(GameEvent::ObjectMoved { obj: role, to: Zone::Graveyard });
            }
        }
        // Mint the new Role from its registered def's copiable characteristics (grp_id carries its
        // statics via `def_of`, like a token/emblem) and attach it to the host.
        let Some(chars) = self.state.def_by_grp(role_grp).map(|d| d.chars.clone()) else { return };
        let id = self.state.add_card(controller, chars, Zone::Battlefield);
        if let Some(o) = self.state.objects.get_mut(&id) {
            o.attached_to = Some(host);
        }
        self.state.mark_chars_dirty();
        self.broadcast(GameEvent::ObjectMoved { obj: id, to: Zone::Battlefield });
    }

    /// Put an emblem (CR 114) into `controller`'s command zone. The emblem is an object with no
    /// characteristics other than the abilities of the registered def `emblem_grp` (CR 114.2): its
    /// `Ability`s (including `FunctionsFrom(vec![Zone::Command])`) come via `def_of`, so `collect_
    /// triggers`' command-zone scan fires them. Emblems are permanent and untouchable by removal/SBAs.
    fn create_emblem(&mut self, emblem_grp: u32, controller: PlayerId) {
        let Some(chars) = self.state.card_db().get(emblem_grp).map(|d| d.chars.clone()) else {
            return; // unknown emblem def — nothing to create (defensive; every emblem is registered)
        };
        let id = self.state.add_card(controller, chars, Zone::Command);
        self.broadcast(GameEvent::ObjectMoved { obj: id, to: Zone::Command });
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
            // 2ˣ (Mathemagics) — `1 << exp`, exponent clamped to [0, 62] so it never overflows i64.
            ValueExpr::Pow2(exp) => 1i64 << self.eval_value(exp, ctx).clamp(0, 62),
            // Half, rounded down (Pox Plague, "round down each time").
            ValueExpr::Half(v) => self.eval_value(v, ctx).max(0) / 2,
            // The current life total of `who` (Pox Plague's "half their life").
            ValueExpr::LifeTotal { who } => {
                let p = self.eval_player(*who, ctx);
                self.state.players.get(p.0 as usize).map(|pl| pl.life as i64).unwrap_or(0)
            }
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
            // Distinct card names (CR 201.2) among matching objects — Emil's "differently named lands".
            ValueExpr::DistinctNames { zone, filter, controller } => {
                let who = controller.map(|r| self.eval_player(r, ctx));
                let names: std::collections::BTreeSet<&str> = self
                    .state
                    .objects
                    .values()
                    .filter(|o| o.zone == *zone)
                    .filter(|o| who.is_none_or(|p| o.controller == p))
                    .filter(|o| self.count_filter_matches(o.id, filter))
                    .map(|o| o.chars.name.as_str())
                    .collect();
                names.len() as i64
            }
            // C9b: the number of `kind` counters on the effect's source (e.g. Mossborn Hydra
            // doubling its own +1/+1 counters). For a CDA computing P/T, chars evaluates this
            // against the object being computed (see chars::compute) — here it's the resolver.
            // Counters on the source: live counts while it's on the battlefield; otherwise its
            // last-known counter bag (CR 603.10a) — so a dies/LTB trigger reads "this creature's
            // counters" it had at death (Ambitious Augmenter / Scolding Administrator), which are 0
            // on the fresh graveyard object.
            ValueExpr::CountersOnSelf(kind) => ctx
                .source
                .map(|s| match self.state.objects.get(&s) {
                    Some(o) if o.zone == Zone::Battlefield => o.counters.get(kind) as i64,
                    _ => self.state.last_known.get(&s).map(|l| l.counters.get(kind)).unwrap_or(0) as i64,
                })
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
            // The mana value of the Nth chosen target (Reanimate's "life equal to that card's mana
            // value"). Reads the object's characteristics mana value — a printed/copiable value stable
            // across a zone move. `0` if the target isn't an object.
            ValueExpr::ManaValueOfTarget(n) => match ctx.chosen_targets.get(*n as usize) {
                Some(Target::Object(id)) => {
                    self.state.objects.get(id).map(|o| o.chars.mana_value() as i64).unwrap_or(0)
                }
                _ => 0,
            },
            // The total mana spent to cast the Nth chosen target (Mana Sculpt's "mana spent to cast that
            // spell"). A "counter target spell" target is a `Target::Stack(sid)` — map it to the spell's
            // card and read `Object.mana_spent` while the spell is still on the stack (before it's
            // countered). Also accepts a plain object target. `0` otherwise.
            ValueExpr::ManaSpentOfTarget(n) => {
                let card = match ctx.chosen_targets.get(*n as usize) {
                    Some(Target::Object(id)) => Some(*id),
                    Some(Target::Stack(sid)) => self.state.stack.items.iter().find(|so| so.id == *sid).and_then(
                        |so| match so.kind {
                            crate::stack::StackObjectKind::Spell(c) => Some(c),
                            _ => None,
                        },
                    ),
                    _ => None,
                };
                card.and_then(|c| self.state.objects.get(&c)).map(|o| o.mana_spent as i64).unwrap_or(0)
            }
            // The `kind` counters on the Nth chosen target — live state (the `PutCounters` interpret
            // arm flushes prior counter-adds first, so Growth Curve's "then double" reads the fresh
            // count). `0` if the target isn't an object.
            ValueExpr::CountersOnTarget { target, kind } => match ctx.chosen_targets.get(*target as usize) {
                Some(Target::Object(id)) => {
                    self.state.objects.get(id).map(|o| o.counters.get(kind) as i64).unwrap_or(0)
                }
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
            // Cards discarded so far during this resolution — Borrowed Knowledge / Colossus.
            ValueExpr::DiscardedThisResolution => self.discarded_this_resolution.len() as i64,
            // Cards the controller has drawn this turn (CR 120) — Fractal Anomaly.
            ValueExpr::CardsDrawnThisTurn => ctx
                .controller
                .and_then(|p| self.state.players.get(p.0 as usize))
                .map(|pl| pl.cards_drawn_this_turn as i64)
                .unwrap_or(0),
            // Life `who` gained this turn (CR 119) — Scheming Silvertongue's gate.
            ValueExpr::LifeGainedThisTurn { who } => {
                let p = self.eval_player(*who, ctx);
                self.state.players.get(p.0 as usize).map(|pl| pl.life_gained_this_turn as i64).unwrap_or(0)
            }
            // The number of life-gain events `who` has had this turn — Leech Collector's "first time".
            ValueExpr::LifeGainEventsThisTurn { who } => {
                let p = self.eval_player(*who, ctx);
                self.state.players.get(p.0 as usize).map(|pl| pl.life_gain_events_this_turn as i64).unwrap_or(0)
            }
            // Creatures that died this turn, any controller — Emeritus of Woe's gate.
            ValueExpr::CreaturesDiedThisTurn => {
                self.state.players.iter().map(|pl| pl.creatures_died_this_turn as i64).sum()
            }
            // Cards put into exile this turn, any owner — Ennis's end-step gate.
            ValueExpr::CardsExiledThisTurn => {
                self.state.players.iter().map(|pl| pl.cards_exiled_this_turn as i64).sum()
            }
            // Cards in `who`'s hand — Joined Researchers' hand-size comparison.
            ValueExpr::HandSize { who } => {
                let p = self.eval_player(*who, ctx);
                self.state.players.get(p.0 as usize).map(|pl| pl.hand.len() as i64).unwrap_or(0)
            }
            // Spells `who` cast this turn — Emeritus of Conflict's "your third spell" gate.
            ValueExpr::SpellsCastThisTurn { who } => {
                let p = self.eval_player(*who, ctx);
                self.state.players.get(p.0 as usize).map(|pl| pl.spells_cast_this_turn as i64).unwrap_or(0)
            }
            // Instant/sorcery spells `who` cast this turn (incl. the resolving spell itself) — Burrog
            // Barrage's "if you've cast another instant or sorcery this turn" (≥2) gate.
            ValueExpr::InstantsSorceriesCastThisTurn { who } => {
                let p = self.eval_player(*who, ctx);
                self.state.players.get(p.0 as usize).map(|pl| pl.instants_sorceries_cast_this_turn as i64).unwrap_or(0)
            }
            // The {X} chosen for the triggering spell of a cast-with-{X} trigger — Geometer's Arthropod.
            ValueExpr::XOfTriggeringSpell => ctx
                .triggering_spell
                .and_then(|s| self.state.objects.get(&s))
                .and_then(|o| o.cast_x)
                .map(|x| x as i64)
                .unwrap_or(0),
            // Total computed toughness of matching battlefield permanents — Orysa's cost-reduction gate.
            ValueExpr::TotalToughness { filter, controller } => {
                let want = controller
                    .map(|r| self.eval_player(r, ctx));
                self.state
                    .objects
                    .values()
                    .filter(|o| o.zone == Zone::Battlefield)
                    .filter(|o| want.is_none_or(|p| o.controller == p))
                    .map(|o| o.id)
                    .filter(|&id| self.count_filter_matches(id, filter))
                    .map(|id| self.state.computed(id).toughness.unwrap_or(0) as i64)
                    .sum()
            }
            // The greatest mana value among matching battlefield objects (End of the Hunt); `0` if none.
            ValueExpr::GreatestManaValue { filter, controller } => {
                let want = controller.map(|r| self.eval_player(r, ctx));
                self.state
                    .objects
                    .values()
                    .filter(|o| o.zone == Zone::Battlefield)
                    .filter(|o| want.is_none_or(|p| o.controller == p))
                    .map(|o| o.id)
                    .filter(|&id| self.count_filter_matches(id, filter))
                    .map(|id| self.state.objects.get(&id).map_or(0, |o| o.chars.mana_value()) as i64)
                    .max()
                    .unwrap_or(0)
            }
        }
    }

    /// Evaluate a `CardFilter` against a single object's computed characteristics, for the subset
    /// `ValueExpr::Count` needs (`ControlledBy` is handled by Count's `controller` restriction). Also
    /// used by the priority-action builder to filter a free-cast permission's eligible hand cards.
    pub(crate) fn count_filter_matches(&self, id: ObjId, filter: &CardFilter) -> bool {
        let cc = self.state.computed(id);
        match filter {
            CardFilter::Any => true,
            CardFilter::HasCardType(t) => cc.card_types.contains(t),
            CardFilter::HasSubtype(s) => cc.subtypes.contains(s),
            CardFilter::HasKeyword(k) => cc.has_keyword(*k),
            CardFilter::HasColor(c) => cc.colors.contains(c),
            CardFilter::Colorless => cc.colors.is_empty(),
            CardFilter::Multicolored => cc.colors.len() >= 2,
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
            // Owner-scoping isn't wired into `Count`/`Select` (this matcher is ctx-free — no player to
            // resolve the `PlayerRef` against). `OwnedBy` is currently only used by a cast-trigger filter
            // (via `enter_filter_matches`, which is ctx-aware); no select/count uses it. Match `true` like
            // `ControlledBy`; a future owner-scoped select adds a ctx-aware path here.
            CardFilter::OwnedBy(_) => true,
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
            CardFilter::ToughnessAtMost(n) => cc.toughness.unwrap_or(0) <= *n,
            CardFilter::PowerAtLeast(n) => cc.power.unwrap_or(0) >= *n,
            CardFilter::ToughnessAtLeast(n) => cc.toughness.unwrap_or(0) >= *n,
            CardFilter::ManaValue { min, max } => {
                let mv = self.state.objects.get(&id).map_or(0, |o| o.chars.mana_value());
                min.is_none_or(|lo| mv >= lo) && max.is_none_or(|hi| mv <= hi)
            }
            // `ManaValueExpr` is dynamic (X-keyed) and must be resolved to `ManaValue` against a
            // resolution ctx before reaching this ctx-free matcher (`resolve_dynamic_filter`). A
            // *constant* bound is honored; a non-constant bound needs ctx we don't have here, so
            // fail closed (never over-match) — the real card paths always pre-resolve.
            CardFilter::ManaValueExpr { min, max } => {
                let bound = |e: &Option<Box<ValueExpr>>| match e.as_deref() {
                    None => Some(None),
                    Some(ValueExpr::Fixed(n)) => Some(Some((*n).max(0) as u32)),
                    Some(_) => None,
                };
                match (bound(min), bound(max)) {
                    (Some(lo), Some(hi)) => {
                        let mv = self.state.objects.get(&id).map_or(0, |o| o.chars.mana_value());
                        lo.is_none_or(|l| mv >= l) && hi.is_none_or(|h| mv <= h)
                    }
                    _ => false,
                }
            }
            CardFilter::Tapped => self.state.objects.get(&id).is_some_and(|o| o.status.tapped),
            CardFilter::Untapped => self.state.objects.get(&id).is_some_and(|o| !o.status.tapped),
            CardFilter::Attacking => self.state.combat.as_ref().is_some_and(|c| c.is_attacking(id)),
            CardFilter::All(fs) => fs.iter().all(|f| self.count_filter_matches(id, f)),
            CardFilter::AnyOf(fs) => fs.iter().any(|f| self.count_filter_matches(id, f)),
            CardFilter::Not(f) => !self.count_filter_matches(id, f),
            // `ItSelf`/`AttachedHost`/`NamedAsChooser` resolve against the effect's source (its
            // attachment / chosen name), which a bare `Count`/`ForEach` enumeration doesn't carry — no
            // such filter is used in that context, so treat as no match. `HasSingleTarget` reads a stack
            // object's targets (only meaningful for a `StackObject` *target* candidate, handled in
            // `target_matches_filter`), never a battlefield object here. Exhaustive by design (no
            // wildcard): a NEW `CardFilter` without an arm is a compile error here, not a silent `false`.
            CardFilter::ItSelf
            | CardFilter::AttachedHost
            | CardFilter::NamedAsChooser
            | CardFilter::HasSingleTarget => false,
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
            // The player bound by an enclosing `ForEachTarget` over a player slot (CR "each of them")
            // — reads the same `foreach_current` cursor as `EffectTarget::Each`. Falls back to the
            // controller outside such a loop or if the current binding isn't a player.
            PlayerRef::Each => match self.foreach_current {
                Some(Target::Player(p)) => p,
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
            // "Its controller", per `ForEach` iteration — the current object's controller (overloaded
            // Winds of Abandon's per-exiled-creature land search). Falls back outside a loop.
            PlayerRef::ControllerOfEach => match self.foreach_current {
                Some(Target::Object(obj)) => self.state.objects.get(&obj).map(|o| o.controller).unwrap_or(controller),
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
            EffectTarget::Searched(n) => {
                self.searched_this_resolution.get(*n as usize).copied().map(Target::Object)
            }
            EffectTarget::Each => self.foreach_current,
            EffectTarget::Player(who) => Some(Target::Player(self.eval_player(*who, ctx))),
            EffectTarget::SourceSelf => ctx.source.map(Target::Object),
            // The top card of the player's library (last element) — no-op on an empty library.
            EffectTarget::TopOfLibrary(who) => {
                let pl = self.eval_player(*who, ctx);
                self.state.player(pl).library.last().copied().map(Target::Object)
            }
            // The spell/ability that triggered this ability (Ward, CR 702.21) — read from the
            // resolution ctx; `None` if it already left the stack.
            EffectTarget::Triggering => ctx.triggering_stack.map(Target::Stack),
            EffectTarget::Select(_) => None,
        }
    }
}
