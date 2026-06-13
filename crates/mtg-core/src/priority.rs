//! The priority loop + the agenda pipeline, plus the turn driver that ties the engine
//! together (the [`Engine`]). Run to a fixpoint between priority passes:
//! recompute continuous effects → state-based actions (loop) → put triggers on the
//! stack (APNAP) → grant priority. CR 117.5, 603.3, 704.3.
//!
//! See `docs/design/WHITEBOARD_MODEL.md` §2.2 — the agenda order is *law* and is covered
//! by tests at the bottom of this file.
//!
//! Milestone 2 scope: a lands-only game. The only priority action is "play a land"
//! (CR 116.2a, a special action). Casting/mana/combat declarations arrive in milestone 3;
//! the loop is written generically (it resolves the stack, drains triggers, etc.) so those
//! slot in without reshaping it.

use crate::agent::{
    ActionRef, Agent, CastVariant, DecisionRequest, DecisionResponse, GameEvent, PlayableAction,
    SelectReason, TargetSlot,
};
use crate::basics::{CardType, Phase, Target, Zone, ZonePos};
use crate::effects::ability::{Ability, EventPattern};
use crate::effects::action::{Action, MoveCause, ResolutionCtx, Whiteboard, WbReason};
use crate::effects::target::{TargetKind, TargetSpec};
use crate::effects::{Effect, EffectTarget};
use crate::ids::{ObjId, PlayerId};
use crate::mana;
use crate::sba::{self, LossReason, StateBasedAction};
use crate::stack::{StackObject, StackObjectKind};
use crate::state::view::view_for;
use crate::state::GameState;
use crate::turn::{is_main_phase, step_grants_priority, TURN_STEPS};

/// A hard cap on turns so a pathological game can never loop forever. Real games end far
/// sooner (a lands-only game ends when a player decks out, CR 704.5b). Reaching the cap
/// ends the game as a draw.
const MAX_TURNS: u32 = 2000;

/// Why the game ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndReason {
    /// A player reached 0 or less life (CR 704.5a).
    ZeroLife,
    /// A player drew from an empty library (CR 704.5b).
    Decked,
    /// A player had ten or more poison counters (CR 704.5c).
    Poison,
    /// No winner: a draw, or the turn-cap was reached.
    DrawOrCapped,
}

/// The result of a finished game (a convenience read of the final state).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Outcome {
    pub winner: Option<PlayerId>,
    pub turns: u32,
    pub reason: EndReason,
}

/// The engine: full [`GameState`] plus one [`Agent`] per seat (indexed by `PlayerId.0`).
/// All player choices flow through the agents; nothing else asks a player anything.
pub struct Engine {
    pub state: GameState,
    agents: Vec<Box<dyn Agent>>,
    /// Append-only record of every public event broadcast this game (the same stream sent
    /// to agents' `observe`). Handy for a CLI trace and for snapshot tests. Off by default.
    pub event_log: Vec<GameEvent>,
    record_events: bool,
    started: bool,
}

impl Engine {
    /// `agents` must have one entry per seat in `state`, in `PlayerId` order.
    pub fn new(state: GameState, agents: Vec<Box<dyn Agent>>) -> Self {
        assert_eq!(
            agents.len(),
            state.players.len(),
            "one agent per seat is required"
        );
        Engine {
            state,
            agents,
            event_log: Vec::new(),
            record_events: false,
            started: false,
        }
    }

    /// Enable recording of broadcast events into [`Engine::event_log`] (for tracing/tests).
    pub fn record_events(&mut self, on: bool) {
        self.record_events = on;
    }

    /// Skip the pre-game opening-hand deal: a later `run_game`/`start_game` will not shuffle
    /// or draw. Use this to play from a hand-built scenario (exact hands/board/libraries)
    /// without decking out on an empty library. (webui's scenario CLI / expect-tests.)
    pub fn skip_opening_deal(&mut self) {
        self.started = true;
    }

    /// The result of the game so far (meaningful once `game_over`). A convenience over
    /// reading `engine.state` directly.
    pub fn outcome(&self) -> Outcome {
        let reason = match self.state.end_reason {
            Some(crate::sba::LossReason::ZeroOrLessLife) => EndReason::ZeroLife,
            Some(crate::sba::LossReason::DrewFromEmptyLibrary) => EndReason::Decked,
            Some(crate::sba::LossReason::TenPoison) => EndReason::Poison,
            None => EndReason::DrawOrCapped,
        };
        Outcome {
            winner: self.state.winner,
            turns: self.state.turn_number,
            reason,
        }
    }

    /// The legal actions a player could take if they had priority right now (cast/play-land),
    /// already masked by the engine. Public so a UI can pre-render the options before a
    /// `Priority` decision arrives; the same list is delivered in the `Priority` request.
    pub fn legal_actions(&self, p: PlayerId) -> Vec<PlayableAction> {
        self.legal_priority_actions(p)
    }

    // ── top-level driver ────────────────────────────────────────────────────────────────

    /// Deal opening hands (once) and play turns until the game ends. Returns the winner
    /// (`None` = draw / no surviving player).
    pub fn run_game(&mut self) -> Option<PlayerId> {
        self.start_game();
        while !self.state.game_over && self.state.turn_number <= MAX_TURNS {
            self.take_turn();
        }
        if !self.state.game_over {
            // Hit the safety cap: end as a draw.
            self.state.game_over = true;
        }
        self.state.winner
    }

    /// Pre-game setup: shuffle libraries and draw opening hands (CR 103.5). Mulligans are
    /// deferred (milestone 2 keeps every opening hand). Idempotent.
    pub fn start_game(&mut self) {
        if self.started {
            return;
        }
        self.started = true;
        let hand_size = crate::state::DEFAULT_HAND_SIZE as u32;
        let seats: Vec<PlayerId> = self.state.players.iter().map(|p| p.id).collect();
        for &p in &seats {
            self.state.shuffle_library(p);
        }
        // Opening draws are not "draw step" draws and don't risk decking on a normal deck.
        for &p in &seats {
            self.draw(p, hand_size);
        }
    }

    /// Run one whole turn for the current active player.
    fn take_turn(&mut self) {
        self.begin_turn();
        for &step in TURN_STEPS.iter() {
            if self.state.game_over {
                return;
            }
            self.run_step(step);
        }
        if !self.state.game_over {
            self.advance_turn();
        }
    }

    /// Start-of-turn housekeeping (before the untap step's own actions): reset the
    /// land-drop and clear summoning sickness for permanents the active player has
    /// controlled since the turn began (CR 302.6).
    fn begin_turn(&mut self) {
        let ap = self.state.active_player;
        self.state.player_mut(ap).lands_played_this_turn = 0;
        let perms = self.state.player(ap).battlefield.clone();
        for id in perms {
            if let Some(o) = self.state.objects.get_mut(&id) {
                o.summoning_sick = false;
            }
        }
    }

    /// Move to the next player's turn (CR 500.1 wrap-around). In two-player this just
    /// alternates seats.
    fn advance_turn(&mut self) {
        self.empty_mana_pools();
        let n = self.state.players.len();
        let cur = self.state.active_player.0 as usize;
        let next = (cur + 1) % n;
        self.state.active_player = self.state.players[next].id;
        self.state.turn_number += 1;
        self.state.phase = Phase::Untap;
    }

    // ── steps & turn-based actions ────────────────────────────────────────────────────────

    fn run_step(&mut self, step: Phase) {
        self.state.phase = step;
        let ev = GameEvent::PhaseBegan {
            turn: self.state.turn_number,
            phase: step,
            active: self.state.active_player,
        };
        self.broadcast(ev);

        // Turn-based actions happen first, before any priority (CR 703.3 / 117.3a).
        self.turn_based_actions(step);

        if step == Phase::Cleanup {
            self.cleanup_step();
        } else if step_grants_priority(step) {
            self.priority_round();
        }
        // Mana pools empty as each step/phase ends (CR 500.5 / 514.3-era timing).
        self.empty_mana_pools();
    }

    /// The turn-based actions for a step (CR 703 / the RULES_SUMMARY §3 table).
    fn turn_based_actions(&mut self, step: Phase) {
        match step {
            Phase::Untap => {
                // (1) phasing and (2) day/night are no-ops for milestone 2. (3) Untap all
                // of the active player's permanents (CR 502.3).
                let ap = self.state.active_player;
                let perms = self.state.player(ap).battlefield.clone();
                for id in perms {
                    if let Some(o) = self.state.objects.get_mut(&id) {
                        o.status.tapped = false;
                    }
                }
            }
            Phase::Draw => {
                // The active player draws a card (CR 504.1), unless this is the first turn
                // and they are the starting player in a two-player game (CR 103.8a).
                let ap = self.state.active_player;
                let skip = self.state.turn_number == 1 && ap == self.state.starting_player;
                if !skip {
                    self.draw(ap, 1);
                }
            }
            // Combat turn-based actions (CR 508/509/510/511 — see combat/).
            Phase::DeclareAttackers => self.declare_attackers(),
            Phase::DeclareBlockers => self.declare_blockers(),
            Phase::CombatDamage => self.combat_damage(),
            Phase::EndCombat => self.end_combat(),
            // Untap/Upkeep/Begin/main phases/End have no further turn-based actions here.
            _ => {}
        }
    }

    /// Cleanup (CR 514): discard to maximum hand size, then remove marked damage and end
    /// "until end of turn" effects, simultaneously. Normally no priority (514.3); the
    /// 514.3a exception (pending SBAs/triggers ⇒ grant priority, then repeat) is handled
    /// by running the agenda and, only if it left something on the stack, a priority round.
    fn cleanup_step(&mut self) {
        let ap = self.state.active_player;
        // (1) Discard to maximum hand size (CR 514.1).
        self.discard_to_hand_size(ap);
        // (2) Remove all marked damage; end "until end of turn"/"this turn" effects (514.2).
        // (No such effects exist yet in milestone 2.)
        for o in self.state.objects.values_mut() {
            if o.zone == Zone::Battlefield {
                o.damage_marked = 0;
            }
        }
        // CR 514.3 / 514.3a: check SBAs and triggers. If that puts something on the stack,
        // the active player gets priority and we repeat cleanup; otherwise the step ends.
        self.run_agenda();
        if !self.state.game_over && !self.state.stack.is_empty() {
            self.priority_round();
            if !self.state.game_over {
                self.cleanup_step();
            }
        }
    }

    fn discard_to_hand_size(&mut self, p: PlayerId) {
        let limit = self.state.player(p).hand_size_limit;
        let hand = self.state.player(p).hand.clone();
        if hand.len() <= limit {
            return;
        }
        let excess = (hand.len() - limit) as u32;
        let req = DecisionRequest::SelectCards {
            reason: SelectReason::DiscardToHandSize,
            from: hand.clone(),
            min: excess,
            max: excess,
            description: format!("discard down to {limit} cards"),
        };
        let chosen = match self.ask(p, &req) {
            DecisionResponse::Indices(idxs) => self.distinct_valid_indices(&idxs, hand.len(), excess),
            _ => (0..excess as usize).collect(),
        };
        // Discard highest index first so earlier indices stay valid.
        let mut to_discard: Vec<usize> = chosen;
        to_discard.sort_unstable();
        to_discard.dedup();
        for &i in to_discard.iter().rev() {
            let card = hand[i];
            let owner = self.state.object(card).owner;
            self.state.move_object(card, Zone::Graveyard, owner);
            self.broadcast(GameEvent::ObjectMoved {
                obj: card,
                to: Zone::Graveyard,
            });
        }
    }

    /// Pick exactly `want` distinct in-range indices from an agent's (possibly malformed)
    /// response — defensive so a buggy agent can never panic the engine (the contract says
    /// it won't, but we don't trust it). Falls back to the lowest indices.
    fn distinct_valid_indices(&self, idxs: &[u32], n: usize, want: u32) -> Vec<usize> {
        let mut out: Vec<usize> = Vec::new();
        for &i in idxs {
            let i = i as usize;
            if i < n && !out.contains(&i) {
                out.push(i);
            }
            if out.len() == want as usize {
                break;
            }
        }
        let mut fill = 0;
        while out.len() < want as usize && fill < n {
            if !out.contains(&fill) {
                out.push(fill);
            }
            fill += 1;
        }
        out
    }

    // ── the priority round ────────────────────────────────────────────────────────────────

    /// One step's priority round (CR 117). The active player gets priority first; players
    /// pass in turn order. When all pass in succession, the top of the stack resolves (or,
    /// if empty, the step/phase ends — CR 117.4, 500.2). Before *any* player receives
    /// priority, the agenda fixpoint runs (CR 117.5).
    fn priority_round(&mut self) {
        let order = self.turn_order();
        let n = order.len();
        let mut idx = 0usize; // whose priority: index into `order` (starts at active player)
        let mut passes = 0usize; // consecutive passes

        loop {
            self.run_agenda();
            if self.state.game_over {
                self.state.priority_player = None;
                return;
            }
            let p = order[idx];
            self.state.priority_player = Some(p);

            let actions = self.legal_priority_actions(p);
            let req = DecisionRequest::Priority {
                actions: actions.clone(),
                can_pass: true,
            };
            match self.ask(p, &req) {
                DecisionResponse::Action(i) if (i as usize) < actions.len() => {
                    self.perform_priority_action(p, &actions[i as usize]);
                    // The player who acted retains priority (CR 117.3c / 116.3): `idx`
                    // stays put, and any prior passes are voided.
                    passes = 0;
                }
                // Pass (explicit, or any out-of-range/ill-typed response treated as a pass
                // so a misbehaving agent can never wedge the loop).
                _ => {
                    passes += 1;
                    if passes >= n {
                        if self.state.stack.is_empty() {
                            // All passed with an empty stack: the step/phase ends (CR 500.2).
                            self.state.priority_player = None;
                            return;
                        }
                        // All passed: resolve the top of the stack (CR 117.4), then the
                        // active player gets priority again (CR 117.3b).
                        self.resolve_top();
                        passes = 0;
                        idx = 0;
                    } else {
                        idx = (idx + 1) % n;
                    }
                }
            }
        }
    }

    /// Turn order for priority purposes (CR 101.4): active player first, then the others in
    /// turn order. Two-player: `[active, other]`.
    fn turn_order(&self) -> Vec<PlayerId> {
        let n = self.state.players.len();
        let ap = self.state.active_player.0 as usize;
        (0..n)
            .map(|k| self.state.players[(ap + k) % n].id)
            .collect()
    }

    /// Enumerate the legal actions `p` may take with priority right now (the engine's job:
    /// masking, CR 117): play a land (CR 116.2a) and cast a spell (CR 601), at the right
    /// timing and only if affordable + (if it targets) has a legal target.
    fn legal_priority_actions(&self, p: PlayerId) -> Vec<PlayableAction> {
        let mut actions = Vec::new();
        let s = &self.state;
        let sorcery_speed = p == s.active_player && is_main_phase(s.phase) && s.stack.is_empty();

        // Play a land (CR 116.2a / 505.6b: one per turn, main phase, empty stack, your turn).
        if sorcery_speed && s.player(p).lands_played_this_turn < 1 {
            for &card in &s.player(p).hand {
                if s.object(card).chars.is_land() {
                    actions.push(PlayableAction::PlayLand { card });
                }
            }
        }

        // Cast a spell (CR 601). Instants any time you have priority; everything else at
        // sorcery speed (CR 117.1a).
        for &card in &s.player(p).hand {
            let chars = &s.object(card).chars;
            if chars.is_land() {
                continue;
            }
            let cost = match &chars.mana_cost {
                Some(c) => c,
                None => continue,
            };
            let timing_ok = chars.has_type(CardType::Instant) || sorcery_speed;
            if !timing_ok || !mana::can_pay(s, p, cost) {
                continue;
            }
            // Must have a legal target for each "target" the spell requires (CR 601.2c).
            let has_targets = match s.def_of(card).and_then(|d| d.spell_effect()) {
                Some(eff) => collect_target_specs(eff)
                    .iter()
                    .all(|spec| self.target_candidates(spec, p).len() as u32 >= spec.min.max(1)),
                None => true,
            };
            if has_targets {
                actions.push(PlayableAction::Cast {
                    spell: card,
                    variant: CastVariant::Normal,
                });
            }
        }
        actions
    }

    fn perform_priority_action(&mut self, p: PlayerId, action: &PlayableAction) {
        match action {
            PlayableAction::PlayLand { card } => self.play_land(p, *card),
            PlayableAction::Cast { spell, .. } => self.cast_spell(p, *spell),
            // Activate / ActivateMana / Special: milestone 4+. Never enumerated yet.
            _ => {}
        }
    }

    /// Play a land: a special action (CR 116.2a), no stack. The land enters the battlefield
    /// under `p`'s control and counts against the one-land-per-turn limit.
    fn play_land(&mut self, p: PlayerId, card: ObjId) {
        self.state.move_object(card, Zone::Battlefield, p);
        self.state.player_mut(p).lands_played_this_turn += 1;
        self.broadcast(GameEvent::ObjectMoved {
            obj: card,
            to: Zone::Battlefield,
        });
    }

    /// Cast a spell from `p`'s hand (CR 601, minimal): put it on the stack (601.2a), choose
    /// targets (601.2c), auto-pay its mana cost (601.2f–h), and announce it cast (601.2i).
    /// Affordability + target availability are pre-checked in `legal_priority_actions`, so no
    /// rewind (CR 732) is needed. The caller keeps priority with the caster (CR 601.2i).
    fn cast_spell(&mut self, p: PlayerId, card: ObjId) {
        let cost = match self.state.object(card).chars.mana_cost.clone() {
            Some(c) => c,
            None => return,
        };
        let effect = self.state.def_of(card).and_then(|d| d.spell_effect().cloned());
        let specs = effect.as_ref().map(collect_target_specs).unwrap_or_default();

        // 601.2a: the card becomes a spell on top of the stack.
        let sid = self.state.mint_stack();
        self.move_to_stack(card, p);
        self.state.stack.push(StackObject {
            id: sid,
            controller: p,
            source: Some(card),
            kind: StackObjectKind::Spell(card),
            targets: Vec::new(),
        });

        // 601.2c: choose targets (locked now).
        if !specs.is_empty() {
            let slots: Vec<TargetSlot> = specs
                .iter()
                .map(|spec| TargetSlot {
                    description: String::new(),
                    legal: self.target_candidates(spec, p),
                    min: spec.min,
                    max: spec.max,
                })
                .collect();
            let req = DecisionRequest::ChooseTargets {
                for_action: ActionRef(sid),
                slots: slots.clone(),
            };
            let resp = self.ask(p, &req);
            let chosen = parse_targets(&slots, &resp);
            if let Some(obj) = self.state.stack.items.iter_mut().find(|s| s.id == sid) {
                obj.targets = chosen;
            }
        }

        // 601.2f–h: pay the total cost (auto-tap lands).
        mana::auto_pay(&mut self.state, p, &cost);

        // 601.2i: the spell has been cast.
        self.broadcast(GameEvent::SpellCast {
            spell: sid,
            controller: p,
        });
    }

    /// Move a card from its owner's hand onto the stack zone (the object's `ObjId` is kept;
    /// the [`StackObject`] wraps it with a `StackId`).
    fn move_to_stack(&mut self, card: ObjId, controller: PlayerId) {
        let owner = self.state.object(card).owner;
        let hand = &mut self.state.player_mut(owner).hand;
        if let Some(pos) = hand.iter().position(|&x| x == card) {
            hand.remove(pos);
        }
        if let Some(o) = self.state.objects.get_mut(&card) {
            o.zone = Zone::Stack;
            o.controller = controller;
        }
    }

    /// The legal target candidates for one target spec (the engine pre-filters; masking is
    /// the engine's job). Milestone 3 supports "any target" (CR 115.4) and player/creature.
    fn target_candidates(&self, spec: &TargetSpec, _caster: PlayerId) -> Vec<Target> {
        let creatures = || {
            self.state
                .objects
                .values()
                .filter(|o| o.zone == Zone::Battlefield && o.chars.is_creature())
                .map(|o| Target::Object(o.id))
        };
        let players = || {
            self.state
                .players
                .iter()
                .filter(|p| !p.has_lost)
                .map(|p| Target::Player(p.id))
        };
        match &spec.kind {
            TargetKind::Any => creatures().chain(players()).collect(),
            TargetKind::Player => players().collect(),
            TargetKind::Creature(_) | TargetKind::Permanent(_) => creatures().collect(),
            // StackObject / CardInZone: not needed by the starter set.
            _ => Vec::new(),
        }
    }

    /// CR 608.2b: a spell/ability resolves unless *every* target is illegal. (Returns true if
    /// it has no targets.)
    fn targets_still_legal(&self, targets: &[Target]) -> bool {
        targets.is_empty() || targets.iter().any(|t| self.target_legal(t))
    }

    fn target_legal(&self, t: &Target) -> bool {
        match t {
            Target::Player(p) => self
                .state
                .players
                .get(p.0 as usize)
                .is_some_and(|pl| !pl.has_lost),
            Target::Object(o) => self
                .state
                .objects
                .get(o)
                .is_some_and(|x| x.zone == Zone::Battlefield),
            Target::Stack(_) => false,
        }
    }

    /// Resolve the top object of the stack (CR 608). Milestone 2 performs only the
    /// *structural* part — a permanent spell enters the battlefield, an instant/sorcery
    /// goes to its owner's graveyard, an ability ceases to exist (608.2n/608.3). Running
    /// the object's effect IR is the effect runtime's job (milestone 4). In a lands-only
    /// game the stack stays empty, so this is exercised only by unit tests.
    fn resolve_top(&mut self) {
        let Some(obj) = self.state.stack.pop() else {
            return;
        };
        match obj.kind {
            StackObjectKind::Spell(id) => {
                let owner = self.state.object(id).owner;
                let is_perm = self.state.object(id).chars.is_permanent();
                if is_perm {
                    // Permanent spell → enters the battlefield (CR 608.3), routed through the
                    // whiteboard so ETB replacement effects (enters-with-counters / -tapped)
                    // apply and the ETB event (→ triggers) fires from commit.
                    let ctx = ResolutionCtx {
                        controller: Some(obj.controller),
                        source: Some(id),
                        ..Default::default()
                    };
                    let mut wb = Whiteboard::new(WbReason::Resolve(obj.id), ctx);
                    wb.push(Action::MoveZone {
                        obj: id,
                        to: Zone::Battlefield,
                        pos: ZonePos::Any,
                        cause: MoveCause::Resolved,
                    });
                    self.commit(wb);
                } else {
                    // Instant/sorcery: recheck targets (608.2b), run the effect (608.2c),
                    // then put it into its owner's graveyard (608.2n).
                    let effect = self.state.def_of(id).and_then(|d| d.spell_effect().cloned());
                    if let Some(effect) = effect {
                        if self.targets_still_legal(&obj.targets) {
                            let ctx = ResolutionCtx {
                                controller: Some(obj.controller),
                                source: Some(id),
                                x: None,
                                chosen_targets: obj.targets.clone(),
                                chosen_modes: Vec::new(),
                            };
                            self.resolve_effect(&effect, &ctx, WbReason::Resolve(obj.id));
                        }
                        // else: all targets illegal ⇒ countered by game rules, no effect.
                    }
                    self.state.move_object(id, Zone::Graveyard, owner);
                    self.broadcast(GameEvent::ObjectMoved {
                        obj: id,
                        to: Zone::Graveyard,
                    });
                }
            }
            StackObjectKind::Ability { index } => {
                // A triggered ability on the stack: run its effect, then it ceases to exist
                // (CR 608.2n). The effect is looked up from the source's CardDef by `grp_id`
                // (persists across zones, so dies-triggers resolve too).
                let effect = obj.source.and_then(|src| {
                    self.state.def_of(src).and_then(|d| match d.abilities.get(index as usize) {
                        Some(Ability::Triggered { effect, .. }) => Some(effect.clone()),
                        _ => None,
                    })
                });
                if let (Some(effect), Some(src)) = (effect, obj.source) {
                    if self.targets_still_legal(&obj.targets) {
                        let ctx = ResolutionCtx {
                            controller: Some(obj.controller),
                            source: Some(src),
                            x: None,
                            chosen_targets: obj.targets.clone(),
                            chosen_modes: Vec::new(),
                        };
                        self.resolve_effect(&effect, &ctx, WbReason::Resolve(obj.id));
                    }
                }
            }
        }
    }

    // ── the agenda pipeline (CR 117.5) ────────────────────────────────────────────────────

    /// Run the agenda to a fixpoint: recompute continuous effects → perform SBAs (loop
    /// until none) → put waiting triggers on the stack (APNAP) → repeat until stable.
    /// This is the law from WHITEBOARD_MODEL §2.2; run before any player receives priority.
    fn run_agenda(&mut self) {
        loop {
            // (1) Recompute layered characteristics if dirty — no-op until the layer
            // system arrives (chars/, milestone 5).
            self.recompute_continuous_if_dirty();

            // (2) State-based actions, performed as one event, repeated until none apply
            // (CR 704.3).
            let sbas = sba::collect(&self.state);
            if !sbas.is_empty() {
                self.perform_sbas(&sbas);
                if self.state.game_over {
                    return;
                }
                continue;
            }

            // (3) Put any waiting triggered abilities on the stack (CR 603.3), then loop:
            // doing so may itself enable new SBAs/triggers.
            let triggers = self.drain_pending_triggers();
            if !triggers.is_empty() {
                for t in triggers {
                    self.put_trigger_on_stack(t);
                }
                continue;
            }

            break; // game state stable → a player may receive priority
        }
    }

    fn recompute_continuous_if_dirty(&mut self) {
        // Milestone 5 (the layer system) lives here. Lands-only has no continuous effects.
    }

    /// Apply a batch of state-based actions simultaneously (CR 704.3). Milestone 2 handles
    /// only player losses; in two-player a loss ends the game (CR 104.2a).
    fn perform_sbas(&mut self, sbas: &[StateBasedAction]) {
        for sba in sbas {
            match sba {
                StateBasedAction::PlayerLoses { player, reason } => {
                    let pl = self.state.player_mut(*player);
                    if pl.has_lost {
                        continue;
                    }
                    pl.has_lost = true;
                    // Clear the decking flag so the SBA isn't re-collected forever.
                    if *reason == LossReason::DrewFromEmptyLibrary {
                        pl.drew_from_empty = false;
                    }
                    // Record the first loss reason for the game's Outcome.
                    if self.state.end_reason.is_none() {
                        self.state.end_reason = Some(*reason);
                    }
                }
                StateBasedAction::CreatureDies { creature, .. } => {
                    let owner = match self.state.objects.get(creature) {
                        Some(o) if o.zone == Zone::Battlefield => o.owner,
                        _ => continue,
                    };
                    if self.state.move_object(*creature, Zone::Graveyard, owner) {
                        self.broadcast(GameEvent::PermanentDied { obj: *creature });
                        self.broadcast(GameEvent::ObjectMoved {
                            obj: *creature,
                            to: Zone::Graveyard,
                        });
                    }
                }
            }
        }
        self.check_game_end();
    }

    /// Put a triggered ability on the stack, choosing its targets now if it targets
    /// (CR 603.3d). A trigger that requires a target but has none is removed (not put on the
    /// stack, CR 603.3c).
    fn put_trigger_on_stack(&mut self, mut t: StackObject) {
        let effect = match (t.source, &t.kind) {
            (Some(src), StackObjectKind::Ability { index }) => {
                self.state.def_of(src).and_then(|d| match d.abilities.get(*index as usize) {
                    Some(Ability::Triggered { effect, .. }) => Some(effect.clone()),
                    _ => None,
                })
            }
            _ => None,
        };
        if let Some(effect) = effect {
            let specs = collect_target_specs(&effect);
            if !specs.is_empty() {
                let slots: Vec<TargetSlot> = specs
                    .iter()
                    .map(|spec| TargetSlot {
                        description: String::new(),
                        legal: self.target_candidates(spec, t.controller),
                        min: spec.min,
                        max: spec.max,
                    })
                    .collect();
                // No legal target for a required slot ⇒ the trigger is removed (CR 603.3c).
                if slots.iter().any(|s| s.min > 0 && s.legal.is_empty()) {
                    return;
                }
                let req = DecisionRequest::ChooseTargets {
                    for_action: ActionRef(t.id),
                    slots: slots.clone(),
                };
                let resp = self.ask(t.controller, &req);
                t.targets = parse_targets(&slots, &resp);
            }
        }
        self.state.stack.push(t);
    }

    /// Drain triggers waiting to go on the stack, APNAP-ordered (CR 603.3b): the active
    /// player's triggers first, then the others in turn order; each player's are kept in
    /// the order they were queued. Empty until the effect runtime arrives (M4).
    fn drain_pending_triggers(&mut self) -> Vec<crate::stack::StackObject> {
        if self.state.pending_triggers.is_empty() {
            return Vec::new();
        }
        let pending = std::mem::take(&mut self.state.pending_triggers);
        let order = self.turn_order();
        let mut ordered = Vec::with_capacity(pending.len());
        for seat in order {
            for t in pending.iter() {
                if t.controller == seat {
                    ordered.push(t.clone());
                }
            }
        }
        // Any whose controller isn't a current seat (shouldn't happen) appended last.
        for t in pending {
            if !ordered.iter().any(|o| o.id == t.id) {
                ordered.push(t);
            }
        }
        ordered
    }

    /// End the game if ≤1 player remains (CR 104.2a). The sole survivor wins.
    fn check_game_end(&mut self) {
        let living = self.state.living_players();
        if living.len() <= 1 {
            self.state.game_over = true;
            self.state.winner = living.first().copied();
            self.state.priority_player = None;
            let ev = GameEvent::GameEnded {
                winner: self.state.winner,
            };
            self.broadcast(ev);
        }
    }

    // ── primitives ────────────────────────────────────────────────────────────────────────

    /// Draw `count` cards for `p` from the top of their library (CR 120/121). A draw from an
    /// empty library sets the decking flag; the player loses on the next SBA check
    /// (CR 704.5b) — drawing-from-empty itself is not the loss.
    pub(crate) fn draw(&mut self, p: PlayerId, count: u32) {
        let mut drawn = 0;
        for _ in 0..count {
            let top = self.state.player_mut(p).library.pop();
            match top {
                Some(card) => {
                    if let Some(o) = self.state.objects.get_mut(&card) {
                        o.zone = Zone::Hand;
                    }
                    self.state.player_mut(p).hand.push(card);
                    drawn += 1;
                }
                None => {
                    self.state.player_mut(p).drew_from_empty = true;
                }
            }
        }
        if drawn > 0 {
            self.broadcast(GameEvent::DrewCards {
                player: p,
                count: drawn,
            });
        }
    }

    fn empty_mana_pools(&mut self) {
        for pl in &mut self.state.players {
            pl.mana_pool.amounts.clear();
        }
    }

    // ── the agent boundary ────────────────────────────────────────────────────────────────

    /// Ask seat `p` to decide `req`, presenting its information-filtered view. The single
    /// place the engine consults an agent for a choice.
    pub(crate) fn ask(&mut self, p: PlayerId, req: &DecisionRequest) -> DecisionResponse {
        let view = view_for(&self.state, p);
        self.agents[p.0 as usize].decide(&view, req)
    }

    /// Push a public event to every seat's `observe` channel (CR: the GRE diff stream), and
    /// collect any triggered abilities that watch this event (CR 603.2).
    pub(crate) fn broadcast(&mut self, ev: GameEvent) {
        if self.record_events {
            self.event_log.push(ev.clone());
        }
        for seat in 0..self.state.players.len() {
            let pid = self.state.players[seat].id;
            let view = view_for(&self.state, pid);
            self.agents[seat].observe(&view, &ev);
        }
        self.collect_triggers(&ev);
    }

    /// Scan for triggered abilities whose pattern matches `ev` and queue them (CR 603.2/603.3):
    /// they go on the stack the next time a player would get priority (the agenda loop drains
    /// `pending_triggers`). Milestone-4 prototype handles `SelfEnters` and `SelfDies`.
    fn collect_triggers(&mut self, ev: &GameEvent) {
        // Map the event to the object whose own triggers might fire, and the pattern.
        let (subject, want): (ObjId, EventPattern) = match ev {
            GameEvent::ObjectMoved { obj, to: Zone::Battlefield } => (*obj, EventPattern::SelfEnters),
            GameEvent::PermanentDied { obj } => (*obj, EventPattern::SelfDies),
            _ => return,
        };
        let Some(def) = self.state.def_of(subject) else {
            return;
        };
        // Indices of this object's triggered abilities matching the event.
        let matches: Vec<u32> = def
            .abilities
            .iter()
            .enumerate()
            .filter(|(_, a)| matches!(a, Ability::Triggered { event, .. } if *event == want))
            .map(|(i, _)| i as u32)
            .collect();
        if matches.is_empty() {
            return;
        }
        let controller = self.state.object(subject).controller;
        for index in matches {
            let id = self.state.mint_stack();
            self.state.pending_triggers.push(StackObject {
                id,
                controller,
                source: Some(subject),
                kind: StackObjectKind::Ability { index },
                targets: Vec::new(),
            });
        }
    }
}

/// Collect the `TargetSpec`s an `Effect` requires, in declaration order (CR 601.2c). The
/// milestone-3 starter set only needs the `DealDamage` target; `Sequence` recurses. Other
/// targeted IR nodes are added as their cards arrive.
fn collect_target_specs(effect: &Effect) -> Vec<TargetSpec> {
    let mut out = Vec::new();
    collect_specs_into(effect, &mut out);
    out
}

fn collect_specs_into(effect: &Effect, out: &mut Vec<TargetSpec>) {
    match effect {
        Effect::DealDamage {
            to: EffectTarget::Target(spec),
            ..
        } => out.push(spec.clone()),
        Effect::Sequence(effects) => {
            for e in effects {
                collect_specs_into(e, out);
            }
        }
        _ => {}
    }
}

/// Turn a `ChooseTargets` response into the chosen concrete targets (in slot order). Defensive
/// against a malformed/empty response: falls back to the first legal candidate of each
/// required slot so a misbehaving agent can't produce an under-targeted spell.
fn parse_targets(slots: &[TargetSlot], resp: &DecisionResponse) -> Vec<Target> {
    let mut chosen = Vec::new();
    if let DecisionResponse::Pairs(pairs) = resp {
        for (slot_idx, cand_idx) in pairs {
            if let Some(slot) = slots.get(*slot_idx as usize) {
                if let Some(t) = slot.legal.get(*cand_idx as usize) {
                    chosen.push(*t);
                }
            }
        }
    }
    if chosen.is_empty() {
        for slot in slots {
            if slot.min > 0 {
                if let Some(t) = slot.legal.first() {
                    chosen.push(*t);
                }
            }
        }
    }
    chosen
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::RandomAgent;
    use crate::basics::Zone;
    use crate::ids::{PlayerId, StackId};
    use crate::stack::{StackObject, StackObjectKind};
    use crate::state::{Characteristics, GameState};

    /// Build a two-player lands-only game: `lib` basic lands each, two `RandomAgent`s.
    fn lands_only_game(lib: usize, seed: u64) -> Engine {
        let mut state = GameState::new(2, seed);
        for seat in 0..2u32 {
            for _ in 0..lib {
                state.add_card(
                    PlayerId(seat),
                    Characteristics::basic_land("Forest"),
                    Zone::Library,
                );
            }
        }
        let agents: Vec<Box<dyn Agent>> = vec![
            Box::new(RandomAgent::new(seed ^ 0xA)),
            Box::new(RandomAgent::new(seed ^ 0xB)),
        ];
        Engine::new(state, agents)
    }

    #[test]
    fn lands_only_game_runs_to_completion_without_panic() {
        // Run many seeds; every game must terminate (someone decks) and conserve cards.
        for seed in 0..40u64 {
            let mut engine = lands_only_game(12, seed);
            let total_before: usize = engine.state.objects.len();
            let winner = engine.run_game();
            assert!(engine.state.game_over, "game must end (seed {seed})");
            // Card conservation: no object created or destroyed (lands-only).
            assert_eq!(engine.state.objects.len(), total_before, "seed {seed}");
            // Exactly one player should have decked out (the loser); the other won.
            let living = engine.state.living_players();
            assert_eq!(living.len(), 1, "two-player game has one survivor (seed {seed})");
            assert_eq!(winner, Some(living[0]), "winner is the survivor (seed {seed})");
        }
    }

    #[test]
    fn opening_hands_are_dealt_and_starting_player_skips_first_draw() {
        let mut engine = lands_only_game(30, 7);
        engine.start_game();
        // Seven-card opening hands (CR 103.5).
        assert_eq!(engine.state.player(PlayerId(0)).hand.len(), 7);
        assert_eq!(engine.state.player(PlayerId(1)).hand.len(), 7);

        // Run just the first turn's draw step: the starting player skips it (CR 103.8a).
        engine.begin_turn();
        engine.run_step(Phase::Untap);
        engine.run_step(Phase::Upkeep);
        engine.run_step(Phase::Draw);
        assert_eq!(
            engine.state.player(PlayerId(0)).hand.len(),
            7,
            "starting player skips the first draw"
        );
    }

    #[test]
    fn turn_advances_and_alternates_active_player() {
        let mut engine = lands_only_game(30, 3);
        engine.start_game();
        assert_eq!(engine.state.active_player, PlayerId(0));
        assert_eq!(engine.state.turn_number, 1);
        engine.take_turn();
        assert_eq!(engine.state.active_player, PlayerId(1));
        assert_eq!(engine.state.turn_number, 2);
        engine.take_turn();
        assert_eq!(engine.state.active_player, PlayerId(0));
        assert_eq!(engine.state.turn_number, 3);
    }

    #[test]
    fn decking_loses_via_state_based_action() {
        // Player 0 has an empty library; on their draw step they deck out and lose.
        let mut engine = lands_only_game(0, 1);
        // Give player 1 a card so they don't also deck immediately.
        engine.state.add_card(PlayerId(1), Characteristics::basic_land("Island"), Zone::Library);
        engine.started = true; // skip opening draws (libraries are tiny/empty)
        engine.run_game();
        assert!(engine.state.game_over);
        assert!(engine.state.player(PlayerId(0)).has_lost);
        assert_eq!(engine.state.winner, Some(PlayerId(1)));
    }

    #[test]
    fn stack_resolution_moves_a_permanent_spell_to_the_battlefield() {
        // Directly exercise resolve_top (a lands-only game never fills the stack).
        let mut engine = lands_only_game(0, 9);
        let card = engine.state.add_card(
            PlayerId(0),
            Characteristics::basic_land("Mountain"),
            Zone::Hand,
        );
        // Pretend it was cast: move to the stack zone + push a stack object.
        engine.state.objects.get_mut(&card).unwrap().zone = Zone::Stack;
        let pos = engine.state.player(PlayerId(0)).hand.iter().position(|&x| x == card).unwrap();
        engine.state.player_mut(PlayerId(0)).hand.remove(pos);
        let sid = engine.state.mint_stack();
        engine.state.stack.push(StackObject {
            id: sid,
            controller: PlayerId(0),
            source: Some(card),
            kind: StackObjectKind::Spell(card),
            targets: vec![],
        });
        assert_eq!(StackId(1), sid);
        engine.resolve_top();
        assert!(engine.state.stack.is_empty());
        assert_eq!(engine.state.object(card).zone, Zone::Battlefield);
        assert!(engine.state.player(PlayerId(0)).battlefield.contains(&card));
    }

    #[test]
    fn agenda_orders_sbas_before_triggers_and_reaches_fixpoint() {
        // With a lethal life total queued, the agenda must end the game (SBA), not hang.
        // (Library ≥ opening hand so the opening draw doesn't deck anyone.)
        let mut engine = lands_only_game(12, 2);
        engine.start_game();
        engine.state.player_mut(PlayerId(1)).life = 0;
        engine.run_agenda();
        assert!(engine.state.game_over);
        assert_eq!(engine.state.winner, Some(PlayerId(0)));
    }

    #[test]
    fn demo_deck_self_play_runs_to_completion() {
        // The milestone-3 EXIT: a real game (lands → creatures → attack → damage → 0 life,
        // or decking) plays to completion with RandomAgents, no panics, cards conserved.
        for seed in 0..40u64 {
            let state = crate::cards::two_player_demo_game(seed);
            let total = state.objects.len();
            let agents: Vec<Box<dyn Agent>> = vec![
                Box::new(RandomAgent::new(seed ^ 0xA11CE)),
                Box::new(RandomAgent::new(seed ^ 0xB0B)),
            ];
            let mut engine = Engine::new(state, agents);
            engine.run_game();
            assert!(engine.state.game_over, "game must end (seed {seed})");
            // No tokens/copies in the starter set ⇒ object count is conserved.
            assert_eq!(engine.state.objects.len(), total, "card conservation (seed {seed})");
            assert!(
                engine.state.living_players().len() <= 1,
                "≤1 survivor (seed {seed})"
            );
        }
    }

    #[test]
    fn burn_vs_bears_self_play_completes() {
        // The user's hand-test matchup, under RandomAgents: must terminate, no panic, cards
        // conserved. (Burn = 40 Bolt + 20 Mountain vs Bears = 40 Grizzly Bears + 20 Forest.)
        for seed in 0..20u64 {
            let state = crate::cards::burn_vs_bears_game(seed);
            let total = state.objects.len();
            let agents: Vec<Box<dyn Agent>> = vec![
                Box::new(RandomAgent::new(seed ^ 0xB)),
                Box::new(RandomAgent::new(seed ^ 0xE)),
            ];
            let mut engine = Engine::new(state, agents);
            engine.run_game();
            assert!(engine.state.game_over, "seed {seed}");
            assert_eq!(engine.state.objects.len(), total, "card conservation (seed {seed})");
        }
    }

    #[test]
    fn skip_opening_deal_leaves_a_built_scenario_untouched() {
        // The scenario hook (webui): no shuffle, no opening draw, so an exact hand/board can
        // be placed without decking out.
        let state = crate::cards::two_player_demo_game(1);
        let mut engine = Engine::new(
            state,
            vec![Box::new(RandomAgent::new(1)), Box::new(RandomAgent::new(2))],
        );
        engine.skip_opening_deal();
        engine.start_game(); // no-op now
        assert!(engine.state.player(PlayerId(0)).hand.is_empty(), "no opening deal");
        assert_eq!(
            engine.state.player(PlayerId(0)).library.len(),
            30,
            "library untouched (no shuffle/draw)"
        );
    }

    #[test]
    fn outcome_reports_winner_and_reason() {
        let mut engine = lands_only_game(12, 5);
        engine.start_game();
        engine.state.player_mut(PlayerId(1)).life = 0;
        engine.run_agenda();
        let outcome = engine.outcome();
        assert_eq!(outcome.winner, Some(PlayerId(0)));
        assert_eq!(outcome.reason, super::EndReason::ZeroLife);
    }
}

/// Inline snapshot ("expect") tests for milestone-2 behaviour: the enumerated legal options
/// at a decision point (masking is the engine's job) and the CR-500s turn-structure trace.
#[cfg(test)]
mod expect_tests {
    use super::*;
    use crate::agent::{DecisionResponse, PlayerView, RandomAgent};
    use crate::basics::{Phase, Target, Zone};
    use crate::cards::{self, grp};
    use crate::ids::PlayerId;
    use crate::state::{Characteristics, GameState};
    use expect_test::expect;

    /// An agent that always passes priority and declines/minimises every other choice — so a
    /// trace shows pure turn structure with no random land plays.
    struct PassAgent;
    impl Agent for PassAgent {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::SelectCards { min, .. } => {
                    DecisionResponse::Indices((0..*min).collect())
                }
                _ => DecisionResponse::Pass,
            }
        }
    }

    /// A deterministic, aggressive agent for casting/combat tests: at priority it casts the
    /// first castable spell, else plays the first land, else passes; it attacks with
    /// everything; never blocks; targets an opponent (player) when choosing a target.
    struct AggroAgent;
    impl Agent for AggroAgent {
        fn decide(&mut self, view: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::Priority { actions, .. } => {
                    if let Some(i) = actions
                        .iter()
                        .position(|a| matches!(a, PlayableAction::Cast { .. }))
                    {
                        return DecisionResponse::Action(i as u32);
                    }
                    if let Some(i) = actions
                        .iter()
                        .position(|a| matches!(a, PlayableAction::PlayLand { .. }))
                    {
                        return DecisionResponse::Action(i as u32);
                    }
                    DecisionResponse::Pass
                }
                DecisionRequest::ChooseTargets { slots, .. } => {
                    let me = view.seat;
                    let mut pairs = Vec::new();
                    for (si, slot) in slots.iter().enumerate() {
                        let idx = slot
                            .legal
                            .iter()
                            .position(|t| matches!(t, Target::Player(p) if *p != me))
                            .or_else(|| {
                                slot.legal.iter().position(|t| matches!(t, Target::Player(_)))
                            })
                            .unwrap_or(0);
                        pairs.push((si as u32, idx as u32));
                    }
                    DecisionResponse::Pairs(pairs)
                }
                DecisionRequest::DeclareAttackers { eligible } => {
                    let pairs = eligible
                        .iter()
                        .enumerate()
                        .filter(|(_, o)| !o.may_attack.is_empty())
                        .map(|(i, _)| (i as u32, 0u32))
                        .collect();
                    DecisionResponse::Pairs(pairs)
                }
                DecisionRequest::AssignCombatDamage { total, .. } => {
                    DecisionResponse::Amounts(vec![(0, *total)])
                }
                DecisionRequest::SelectCards { min, .. } => {
                    DecisionResponse::Indices((0..*min).collect())
                }
                _ => DecisionResponse::Pass,
            }
        }
    }

    fn pass_engine(state: GameState) -> Engine {
        Engine::new(state, vec![Box::new(PassAgent), Box::new(PassAgent)])
    }

    /// Put a card (by grp_id) directly into a player's zone, returning its id.
    fn put(state: &mut GameState, owner: PlayerId, grp_id: u32, zone: Zone) -> crate::ids::ObjId {
        let chars = state.card_db().get(grp_id).unwrap().chars.clone();
        state.add_card(owner, chars, zone)
    }

    /// Compact render of the decisive events for a scenario (for expect traces).
    fn event_trace(events: &[GameEvent]) -> String {
        let mut out = String::new();
        for ev in events {
            let line = match ev {
                GameEvent::SpellCast { spell, controller } => {
                    Some(format!("{controller:?} casts {spell:?}"))
                }
                GameEvent::ObjectMoved { obj, to } => Some(format!("{obj:?} -> {to:?}")),
                GameEvent::DrewCards { player, count } => Some(format!("{player:?} draws {count}")),
                GameEvent::DamageDealt { target, amount, source } => {
                    Some(format!("{amount} dmg {source:?} -> {target:?}"))
                }
                GameEvent::LifeChanged { player, new_total, .. } => {
                    Some(format!("{player:?} life -> {new_total}"))
                }
                GameEvent::PermanentDied { obj } => Some(format!("{obj:?} dies")),
                GameEvent::GameEnded { winner } => Some(format!("game over: {winner:?}")),
                _ => None,
            };
            if let Some(l) = line {
                out.push_str(&l);
                out.push('\n');
            }
        }
        out
    }

    #[test]
    fn etb_trigger_draws_a_card() {
        // Elvish Visionary: "When this creature enters, draw a card." (CR 603.6a ETB trigger.)
        // Library card is a creature (not a land the aggro agent would then play), so the
        // drawn card stays in hand and the trigger's effect is observable.
        let mut state = cards::build_game(1, &[&[grp::GRIZZLY_BEARS], &[]]);
        put(&mut state, PlayerId(0), grp::FOREST, Zone::Battlefield);
        put(&mut state, PlayerId(0), grp::FOREST, Zone::Battlefield);
        let viz = put(&mut state, PlayerId(0), grp::ELVISH_VISIONARY, Zone::Hand);
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let mut e = Engine::new(state, vec![Box::new(AggroAgent), Box::new(PassAgent)]);
        e.record_events(true);
        e.priority_round();

        assert_eq!(e.state.object(viz).zone, Zone::Battlefield, "Visionary entered");
        assert_eq!(e.state.player(PlayerId(0)).hand.len(), 1, "ETB trigger drew a card");
        assert!(e.state.player(PlayerId(0)).library.is_empty(), "drew the only library card");
        expect![[r#"
            PlayerId(0) casts StackId(1)
            ObjId(4) -> Battlefield
            PlayerId(0) draws 1
        "#]]
        .assert_eq(&event_trace(&e.event_log));
    }

    #[test]
    fn etb_trigger_targets_and_kills_a_creature() {
        // Flametongue Kavu: ETB deals 4 to target creature. The trigger targets when it goes
        // on the stack (603.3d); the aggro agent picks the enemy 2/2, which then dies to the
        // lethal-damage SBA (704.5g).
        let mut state = cards::build_game(2, &[&[], &[]]);
        let prey = put(&mut state, PlayerId(1), grp::GRIZZLY_BEARS, Zone::Battlefield);
        for _ in 0..4 {
            put(&mut state, PlayerId(0), grp::MOUNTAIN, Zone::Battlefield); // pay {3}{R}
        }
        let ftk = put(&mut state, PlayerId(0), grp::FLAMETONGUE_KAVU, Zone::Hand);
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let mut e = Engine::new(state, vec![Box::new(AggroAgent), Box::new(PassAgent)]);
        e.record_events(true);
        e.priority_round();

        assert_eq!(e.state.object(ftk).zone, Zone::Battlefield, "FTK entered");
        assert_eq!(e.state.object(prey).zone, Zone::Graveyard, "FTK's ETB killed the enemy 2/2");
        expect![[r#"
            PlayerId(0) casts StackId(1)
            ObjId(6) -> Battlefield
            4 dmg ObjId(6) -> Object(ObjId(1))
            ObjId(1) dies
            ObjId(1) -> Graveyard
        "#]]
        .assert_eq(&event_trace(&e.event_log));
    }

    #[test]
    fn enters_with_counters_replacement_keeps_a_0_0_alive() {
        // Servant of the Scale: a 0/0 that "enters with a +1/+1 counter". The whiteboard
        // rewrite pass turns its ETB into entering-with-a-counter, so it's a 1/1 that survives
        // the toughness-0 SBA. (Straight-through commit would let a 0/0 die immediately.)
        use crate::basics::CounterKind;
        let mut state = cards::build_game(3, &[&[], &[]]);
        put(&mut state, PlayerId(0), grp::FOREST, Zone::Battlefield); // pay {G}
        let servant = put(&mut state, PlayerId(0), grp::SERVANT_OF_THE_SCALE, Zone::Hand);
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let mut e = Engine::new(state, vec![Box::new(AggroAgent), Box::new(PassAgent)]);
        e.priority_round();

        let s = e.state.object(servant);
        assert_eq!(s.zone, Zone::Battlefield, "Servant survived (entered with a counter)");
        assert_eq!(s.counters.get(&CounterKind::PlusOnePlusOne), 1, "entered with one +1/+1");
        assert_eq!(s.effective_power(), 1);
        assert_eq!(s.effective_toughness(), 1);
    }

    #[test]
    fn legal_priority_actions_are_enumerated_and_masked() {
        // Active player in their precombat main with two lands in hand, empty stack.
        let mut state = GameState::new(2, 1);
        state.add_card(PlayerId(0), Characteristics::basic_land("Forest"), Zone::Hand);
        state.add_card(PlayerId(0), Characteristics::basic_land("Island"), Zone::Hand);
        state.phase = Phase::PrecombatMain;
        let engine = pass_engine(state);

        // One PlayLand action per land in hand (CR 116.2a / 505.6).
        let actions = engine.legal_priority_actions(PlayerId(0));
        expect![[r#"
            [
                PlayLand {
                    card: ObjId(
                        1,
                    ),
                },
                PlayLand {
                    card: ObjId(
                        2,
                    ),
                },
            ]"#]]
        .assert_eq(&format!("{actions:#?}"));

        // Masking: the non-active player gets nothing at sorcery speed (CR 117.1a)…
        assert!(engine.legal_priority_actions(PlayerId(1)).is_empty());
        // …and after a land is played, the limit (CR 505.6b) removes the option.
        let mut state2 = GameState::new(2, 1);
        state2.add_card(PlayerId(0), Characteristics::basic_land("Forest"), Zone::Hand);
        state2.phase = Phase::PrecombatMain;
        state2.player_mut(PlayerId(0)).lands_played_this_turn = 1;
        assert!(pass_engine(state2).legal_priority_actions(PlayerId(0)).is_empty());
    }

    #[test]
    fn one_turn_walks_the_cr500_step_sequence() {
        // A single turn of pass/pass through every step, traced via PhaseBegan events.
        let mut state = GameState::new(2, 5);
        for _ in 0..10 {
            state.add_card(PlayerId(0), Characteristics::basic_land("Forest"), Zone::Library);
            state.add_card(PlayerId(1), Characteristics::basic_land("Forest"), Zone::Library);
        }
        let mut engine = pass_engine(state);
        engine.start_game();
        engine.record_events(true);
        engine.take_turn();

        let trace: String = engine
            .event_log
            .iter()
            .filter_map(|e| match e {
                GameEvent::PhaseBegan { turn, phase, active } => {
                    Some(format!("T{turn} {active:?} {phase:?}"))
                }
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");
        expect![[r#"
            T1 PlayerId(0) Untap
            T1 PlayerId(0) Upkeep
            T1 PlayerId(0) Draw
            T1 PlayerId(0) PrecombatMain
            T1 PlayerId(0) BeginCombat
            T1 PlayerId(0) DeclareAttackers
            T1 PlayerId(0) DeclareBlockers
            T1 PlayerId(0) CombatDamage
            T1 PlayerId(0) EndCombat
            T1 PlayerId(0) PostcombatMain
            T1 PlayerId(0) End
            T1 PlayerId(0) Cleanup"#]]
        .assert_eq(&trace);
    }

    /// A whole deterministic lands-only game (two seeded `RandomAgent`s, 8-card libraries)
    /// rendered as a turn-by-turn trace of its meaningful events: draws, land plays, and the
    /// decking loss that ends it (CR 704.5b). This snapshots the milestone-2 game loop.
    #[test]
    fn full_lands_only_game_trace() {
        let mut state = GameState::new(2, 7);
        for seat in 0..2u32 {
            for _ in 0..8 {
                state.add_card(PlayerId(seat), Characteristics::basic_land("Forest"), Zone::Library);
            }
        }
        let agents: Vec<Box<dyn Agent>> = vec![
            Box::new(RandomAgent::new(7 ^ 0xA11CE)),
            Box::new(RandomAgent::new(7 ^ 0xB0B)),
        ];
        let mut engine = Engine::new(state, agents);
        engine.record_events(true);
        let winner = engine.run_game();
        assert_eq!(winner, Some(PlayerId(0)));

        let mut out = String::new();
        let mut cur_turn = 0u32;
        for ev in &engine.event_log {
            match ev {
                GameEvent::PhaseBegan { turn, active, .. } if *turn != cur_turn => {
                    cur_turn = *turn;
                    out.push_str(&format!("== turn {turn} (active {active:?}) ==\n"));
                }
                GameEvent::PhaseBegan { .. } => {}
                GameEvent::DrewCards { player, count } => {
                    out.push_str(&format!("  {player:?} draws {count}\n"))
                }
                GameEvent::ObjectMoved { obj, to } => {
                    out.push_str(&format!("  {obj:?} -> {to:?}\n"))
                }
                GameEvent::GameEnded { winner } => {
                    out.push_str(&format!("game over, winner {winner:?}\n"))
                }
                other => out.push_str(&format!("  {other:?}\n")),
            }
        }
        expect![[r#"
              PlayerId(0) draws 7
              PlayerId(1) draws 7
            == turn 1 (active PlayerId(0)) ==
              ObjId(3) -> Battlefield
            == turn 2 (active PlayerId(1)) ==
              PlayerId(1) draws 1
              ObjId(9) -> Battlefield
            == turn 3 (active PlayerId(0)) ==
              PlayerId(0) draws 1
            == turn 4 (active PlayerId(1)) ==
            game over, winner Some(PlayerId(0))
        "#]]
        .assert_eq(&out);
    }

    /// Casting an instant: P0 Shocks P1 (the opponent player) for 2. Exercises legal-action
    /// enumeration of `Cast`, target choice (601.2c), auto-tap payment, the stack, resolution,
    /// and the `DealDamage` interpreter.
    #[test]
    fn cast_shock_damages_opponent() {
        let mut state = cards::build_game(1, &[&[], &[]]);
        let mountain = put(&mut state, PlayerId(0), grp::MOUNTAIN, Zone::Battlefield);
        let shock = put(&mut state, PlayerId(0), grp::SHOCK, Zone::Hand);
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let mut e = Engine::new(state, vec![Box::new(AggroAgent), Box::new(PassAgent)]);
        e.priority_round();

        assert_eq!(e.state.player(PlayerId(1)).life, 18, "Shock dealt 2 to the opponent");
        assert_eq!(e.state.object(shock).zone, Zone::Graveyard, "instant to graveyard");
        assert!(e.state.object(mountain).status.tapped, "land tapped to pay {{R}}");
        assert!(e.state.stack.is_empty());
    }

    /// Lightning Bolt {R}: deals 3 to any target (here, the opponent's face).
    #[test]
    fn cast_lightning_bolt_to_the_face() {
        let mut state = cards::build_game(1, &[&[], &[]]);
        put(&mut state, PlayerId(0), grp::MOUNTAIN, Zone::Battlefield);
        put(&mut state, PlayerId(0), grp::LIGHTNING_BOLT, Zone::Hand);
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let mut e = Engine::new(state, vec![Box::new(AggroAgent), Box::new(PassAgent)]);
        e.priority_round();
        assert_eq!(e.state.player(PlayerId(1)).life, 17, "Bolt dealt 3 to the opponent");
    }

    /// A creature that has been under control since the turn began can attack; an unblocked
    /// attacker deals its power to the defending player and is tapped.
    #[test]
    fn creature_attacks_unblocked_for_damage() {
        let mut state = cards::build_game(2, &[&[], &[]]);
        let bears = put(&mut state, PlayerId(0), grp::GRIZZLY_BEARS, Zone::Battlefield);
        state.objects.get_mut(&bears).unwrap().summoning_sick = false;
        state.active_player = PlayerId(0);
        let mut e = Engine::new(state, vec![Box::new(AggroAgent), Box::new(PassAgent)]);

        e.run_step(Phase::DeclareAttackers);
        e.run_step(Phase::DeclareBlockers);
        e.run_step(Phase::CombatDamage);

        assert_eq!(e.state.player(PlayerId(1)).life, 18, "2/2 dealt 2 to the defender");
        assert!(e.state.object(bears).status.tapped, "attacker tapped (CR 508.1f)");
    }

    /// A 2/2 attacker blocked by a 2/2: both take 2 lethal damage and both are reported dead
    /// by the SBA check (CR 510.1c/d, 704.5g).
    #[test]
    fn blocked_attacker_and_blocker_trade() {
        use crate::combat::{Attack, Block, CombatState};
        use crate::sba::{self, DeathReason, StateBasedAction};

        let mut state = cards::build_game(3, &[&[], &[]]);
        let attacker = put(&mut state, PlayerId(0), grp::GRIZZLY_BEARS, Zone::Battlefield);
        let blocker = put(&mut state, PlayerId(1), grp::GRIZZLY_BEARS, Zone::Battlefield);
        state.active_player = PlayerId(0);
        state.combat = Some(CombatState {
            attackers: vec![Attack {
                attacker,
                defender: Target::Player(PlayerId(1)),
            }],
            blocks: vec![Block { blocker, attacker }],
        });
        let mut e = Engine::new(state, vec![Box::new(PassAgent), Box::new(PassAgent)]);
        e.combat_damage();

        assert_eq!(e.state.object(attacker).damage_marked, 2);
        assert_eq!(e.state.object(blocker).damage_marked, 2);
        assert_eq!(e.state.player(PlayerId(1)).life, 20, "blocked: no damage to the player");
        let sbas = sba::collect(&e.state);
        for id in [attacker, blocker] {
            assert!(sbas.contains(&StateBasedAction::CreatureDies {
                creature: id,
                reason: DeathReason::LethalDamage,
            }));
        }
    }

    /// A full deterministic R/G demo game (two `AggroAgent`s) rendered as a combat trace of
    /// its decisive events — casts, combat/burn damage, deaths, the lethal blow, game end.
    #[test]
    fn demo_game_combat_trace() {
        let state = cards::two_player_demo_game(11);
        let mut e = Engine::new(state, vec![Box::new(AggroAgent), Box::new(AggroAgent)]);
        e.record_events(true);
        e.run_game();

        let mut out = String::new();
        let mut cur_turn = 0u32;
        for ev in &e.event_log {
            match ev {
                GameEvent::PhaseBegan { turn, active, .. } if *turn != cur_turn => {
                    cur_turn = *turn;
                    out.push_str(&format!("T{turn} (active {active:?})\n"));
                }
                GameEvent::PhaseBegan { .. } | GameEvent::DrewCards { .. } => {}
                GameEvent::SpellCast { spell, controller } => {
                    out.push_str(&format!("  {controller:?} casts spell {spell:?}\n"))
                }
                GameEvent::DamageDealt { target, amount, .. } => {
                    out.push_str(&format!("  {amount} damage to {target:?}\n"))
                }
                GameEvent::LifeChanged { player, new_total, .. } => {
                    out.push_str(&format!("  {player:?} life -> {new_total}\n"))
                }
                GameEvent::PermanentDied { obj } => out.push_str(&format!("  {obj:?} dies\n")),
                GameEvent::GameEnded { winner } => {
                    out.push_str(&format!("game over, winner {winner:?}\n"))
                }
                _ => {}
            }
        }
        expect![[r#"
            T1 (active PlayerId(0))
              PlayerId(0) casts spell StackId(1)
              2 damage to Player(PlayerId(1))
              PlayerId(1) life -> 18
            T2 (active PlayerId(1))
            T3 (active PlayerId(0))
              PlayerId(0) casts spell StackId(2)
            T4 (active PlayerId(1))
            T5 (active PlayerId(0))
              2 damage to Player(PlayerId(1))
              PlayerId(1) life -> 16
            T6 (active PlayerId(1))
            T7 (active PlayerId(0))
              2 damage to Player(PlayerId(1))
              PlayerId(1) life -> 14
            T8 (active PlayerId(1))
              PlayerId(1) casts spell StackId(3)
            T9 (active PlayerId(0))
              2 damage to Player(PlayerId(1))
              PlayerId(1) life -> 12
            T10 (active PlayerId(1))
              3 damage to Player(PlayerId(0))
              PlayerId(0) life -> 17
            T11 (active PlayerId(0))
              2 damage to Player(PlayerId(1))
              PlayerId(1) life -> 10
            T12 (active PlayerId(1))
              PlayerId(1) casts spell StackId(4)
              3 damage to Player(PlayerId(0))
              PlayerId(0) life -> 14
            T13 (active PlayerId(0))
              2 damage to Player(PlayerId(1))
              PlayerId(1) life -> 8
            T14 (active PlayerId(1))
              PlayerId(1) casts spell StackId(5)
              2 damage to Player(PlayerId(0))
              PlayerId(0) life -> 12
              3 damage to Player(PlayerId(0))
              PlayerId(0) life -> 9
              2 damage to Player(PlayerId(0))
              PlayerId(0) life -> 7
            T15 (active PlayerId(0))
              2 damage to Player(PlayerId(1))
              PlayerId(1) life -> 6
            T16 (active PlayerId(1))
              3 damage to Player(PlayerId(0))
              PlayerId(0) life -> 4
              2 damage to Player(PlayerId(0))
              PlayerId(0) life -> 2
            T17 (active PlayerId(0))
              PlayerId(0) casts spell StackId(6)
              2 damage to Player(PlayerId(1))
              PlayerId(1) life -> 4
            T18 (active PlayerId(1))
              3 damage to Player(PlayerId(0))
              PlayerId(0) life -> -1
              2 damage to Player(PlayerId(0))
              PlayerId(0) life -> -3
            game over, winner Some(PlayerId(1))
        "#]]
        .assert_eq(&out);
    }
}
