//! Combat: begin / declare attackers / declare blockers / damage / end of combat
//! (CR 506–511).
//!
//! Milestone 3 scope (CLAUDE.md "Scope — first pass"): no keywords/evasion, no
//! first/double strike, no trample. A single combat-damage step (CR 510.4 default): each
//! creature assigns damage equal to its power; unblocked attackers hit the defending
//! player; blocked attackers split damage among their blockers (multi-block ⇒ an
//! `AssignCombatDamage` decision); blockers hit the attacker they block. All combat damage
//! is dealt simultaneously (CR 510.2), then the agenda's SBAs (sba.rs) destroy creatures
//! with lethal damage.
//!
//! The data types ([`CombatState`]) live in `GameState`; the step logic is `impl EngineCore`.

use serde::{Deserialize, Serialize};

use crate::agent::{
    AttackerOption, BlockerOption, DamageSlot, DecisionRequest, DecisionResponse, GameEvent,
};
use crate::basics::{CardType, DamageKind, Target, Zone};
use crate::effects::ability::{Keyword, Qualification};
use crate::effects::action::{Action, ResolutionCtx, Whiteboard, WbReason};
use crate::ids::{ObjId, PlayerId};
use crate::priority::EngineCore;

/// One declared attack (CR 508): a creature and what it is attacking.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attack {
    pub attacker: ObjId,
    pub defender: Target,
}

/// One declared block (CR 509): a blocker and the attacker it blocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Block {
    pub blocker: ObjId,
    pub attacker: ObjId,
}

/// Combat state for the current combat phase (CR 506–511).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CombatState {
    pub attackers: Vec<Attack>,
    pub blocks: Vec<Block>,
}

/// Which combat-damage substep is being dealt (CR 510.4).
#[derive(Clone, Copy, PartialEq, Eq)]
enum CombatStep {
    /// No first/double strike present — a single damage step; everyone deals.
    Single,
    /// First-strike step: first-strikers + double-strikers.
    FirstStrike,
    /// Regular step (after a first-strike step): double-strikers again + non-first-strikers.
    Regular,
}

impl CombatState {
    /// The blockers assigned to `attacker`, in declaration order.
    pub fn blockers_of(&self, attacker: ObjId) -> Vec<ObjId> {
        self.blocks
            .iter()
            .filter(|b| b.attacker == attacker)
            .map(|b| b.blocker)
            .collect()
    }

    /// Whether `id` is currently a declared attacker (CR 508.1) — the `CardFilter::Attacking` check.
    pub fn is_attacking(&self, id: ObjId) -> bool {
        self.attackers.iter().any(|a| a.attacker == id)
    }
}

impl EngineCore {
    /// The non-active player in a two-player game (the defender).
    pub(crate) fn opponent_of(&self, p: PlayerId) -> PlayerId {
        self.state
            .players
            .iter()
            .map(|q| q.id)
            .find(|&q| q != p)
            .unwrap_or(p)
    }

    fn creature_power(&self, id: ObjId) -> u32 {
        self.state.computed(id).power.unwrap_or(0).max(0) as u32
    }

    fn creature_lethal(&self, id: ObjId) -> u32 {
        let tough = self.state.computed(id).toughness.unwrap_or(0).max(0) as u32;
        let marked = self.state.objects.get(&id).map(|o| o.damage_marked).unwrap_or(0);
        tough.saturating_sub(marked)
    }

    /// Whether `blocker` may legally block `attacker` given evasion (CR 509.1b). Milestone 5:
    /// flying — a creature with flying can only be blocked by creatures with flying or reach.
    fn can_block(&self, blocker: ObjId, attacker: ObjId) -> bool {
        // "Can't block" (CR 509.1a, e.g. Pacifism) — never a legal blocker.
        if self.state.computed(blocker).has_qualification(Qualification::CantBlock) {
            return false;
        }
        // "Can't be blocked" (CR 509.1b, e.g. Escape Tunnel) — no creature may block this attacker.
        if self.state.computed(attacker).has_qualification(Qualification::CantBeBlocked) {
            return false;
        }
        // Protection from a colour (CR 702.16c): an attacker with protection from a colour can't be
        // blocked by a creature of that colour.
        let atk = self.state.computed(attacker);
        if !atk.protection_from.is_empty() {
            let bk_colors = self.state.computed(blocker).colors;
            if bk_colors.iter().any(|c| atk.protection_from.contains(c)) {
                return false;
            }
        }
        if self.state.computed(attacker).has_keyword(Keyword::Flying) {
            let bk = self.state.computed(blocker);
            bk.has_keyword(Keyword::Flying) || bk.has_keyword(Keyword::Reach)
        } else {
            true
        }
    }

    /// Declare Attackers step (CR 508): a turn-based action, no stack. The active player
    /// chooses untapped, non-summoning-sick creatures to attack the defending player.
    pub(crate) fn declare_attackers(&mut self) {
        let ap = self.state.active_player;
        let defender = self.opponent_of(ap);
        let eligible_ids: Vec<ObjId> = self
            .state
            .player(ap)
            .battlefield
            .iter()
            .copied()
            .filter(|&id| {
                let o = &self.state.objects[&id];
                let cc = self.state.computed(id);
                // Computed type (CR 613) — a land that became a creature can attack. Defender
                // (CR 702.3) can't attack. Haste (CR 702.10) ignores summoning sickness.
                cc.is_creature()
                    && !cc.has_keyword(Keyword::Defender)
                    && !cc.has_qualification(Qualification::CantAttack)
                    && !o.status.tapped
                    && (!o.summoning_sick || cc.has_keyword(Keyword::Haste))
            })
            .collect();
        if eligible_ids.is_empty() {
            return;
        }
        // An attacker may attack the defending player or any planeswalker they control
        // (CR 508.1a / 306.8). The defender list is the same for every eligible attacker.
        let mut may_attack = vec![Target::Player(defender)];
        for &id in &self.state.player(defender).battlefield {
            if self.state.computed(id).card_types.contains(&CardType::Planeswalker) {
                may_attack.push(Target::Object(id));
            }
        }
        let eligible: Vec<AttackerOption> = eligible_ids
            .iter()
            .map(|&id| AttackerOption {
                creature: id,
                may_attack: may_attack.clone(),
                required: false,
                attack_cost: None,
                may_exert: false,
                may_enlist: false,
            })
            .collect();
        let req = DecisionRequest::DeclareAttackers {
            eligible: eligible.clone(),
        };
        let resp = self.ask(ap, &req);

        let mut attacks: Vec<Attack> = Vec::new();
        if let DecisionResponse::Pairs(pairs) = resp {
            for (atk_idx, def_idx) in pairs {
                if let Some(opt) = eligible.get(atk_idx as usize) {
                    // One attack per creature (ignore duplicates).
                    if attacks.iter().any(|a| a.attacker == opt.creature) {
                        continue;
                    }
                    if let Some(&defender) = opt.may_attack.get(def_idx as usize) {
                        attacks.push(Attack {
                            attacker: opt.creature,
                            defender,
                        });
                    }
                }
            }
        }
        if attacks.is_empty() {
            return;
        }
        // Tap attackers (CR 508.1f — not a cost). Vigilance (CR 702.20) doesn't tap.
        for a in &attacks {
            if self.state.computed(a.attacker).has_keyword(Keyword::Vigilance) {
                continue;
            }
            if let Some(o) = self.state.objects.get_mut(&a.attacker) {
                o.status.tapped = true;
            }
        }
        let attacker_ids: Vec<ObjId> = attacks.iter().map(|a| a.attacker).collect();
        for &id in &attacker_ids {
            if let Some(o) = self.state.objects.get_mut(&id) {
                o.attacked_this_turn = true; // CR 508.1 — read at the end step (Berserk).
            }
        }
        self.state.combat = Some(CombatState {
            attackers: attacks,
            blocks: Vec::new(),
        });
        // CR 508.1: attackers are declared → fire "whenever you attack" / "whenever this attacks"
        // triggers (queued, then put on the stack when a player next gets priority, CR 508.2).
        self.broadcast(GameEvent::AttackersDeclared { attackers: attacker_ids, by: ap });
    }

    /// Declare exactly `attackers` (all attacking the defending player), bypassing the agent prompt
    /// — for in-crate tests / the #60 audit harness that need to drive a specific attack so the
    /// "whenever you attack" / "whenever this creature attacks" triggers fire (Lumbering, Dyadrine).
    /// Mirrors [`declare_attackers`] minus eligibility/prompt: taps non-vigilant attackers, sets up
    /// combat, and fires `AttackersDeclared` (the triggers are queued for the next `run_agenda`).
    #[allow(dead_code)] // test/audit-harness primitive (in-crate tests only)
    pub(crate) fn declare_attackers_explicit(&mut self, attackers: &[ObjId]) {
        if attackers.is_empty() {
            return;
        }
        let ap = self.state.active_player;
        let defender = self.opponent_of(ap);
        let attacks: Vec<Attack> = attackers
            .iter()
            .map(|&id| Attack { attacker: id, defender: Target::Player(defender) })
            .collect();
        // Tap non-vigilant attackers (CR 508.1f — not a cost).
        for a in &attacks {
            if self.state.computed(a.attacker).has_keyword(Keyword::Vigilance) {
                continue;
            }
            if let Some(o) = self.state.objects.get_mut(&a.attacker) {
                o.status.tapped = true;
            }
        }
        let attacker_ids: Vec<ObjId> = attacks.iter().map(|a| a.attacker).collect();
        for &id in &attacker_ids {
            if let Some(o) = self.state.objects.get_mut(&id) {
                o.attacked_this_turn = true; // CR 508.1 — read at the end step (Berserk).
            }
        }
        self.state.combat = Some(CombatState { attackers: attacks, blocks: Vec::new() });
        self.broadcast(GameEvent::AttackersDeclared { attackers: attacker_ids, by: ap });
    }

    /// Declare Blockers step (CR 509): the defending player assigns untapped creatures to
    /// block declared attackers. No evasion in milestone 3, so any blocker may block any
    /// attacker. (`RandomAgent` never blocks; a human can.)
    pub(crate) fn declare_blockers(&mut self) {
        let combat = match self.state.combat.clone() {
            Some(c) if !c.attackers.is_empty() => c,
            _ => return,
        };
        let ap = self.state.active_player;
        let defender = self.opponent_of(ap);
        let attacker_ids: Vec<ObjId> = combat.attackers.iter().map(|a| a.attacker).collect();
        let eligible_ids: Vec<ObjId> = self
            .state
            .player(defender)
            .battlefield
            .iter()
            .copied()
            .filter(|&id| {
                let o = &self.state.objects[&id];
                self.state.computed(id).is_creature() && !o.status.tapped
            })
            .collect();
        if eligible_ids.is_empty() {
            return;
        }
        let eligible: Vec<BlockerOption> = eligible_ids
            .iter()
            .map(|&id| BlockerOption {
                creature: id,
                // Evasion-filtered (CR 509.1b): only the attackers this creature may block.
                may_block: attacker_ids
                    .iter()
                    .copied()
                    .filter(|&atk| self.can_block(id, atk))
                    .collect(),
                required: false,
                block_cost: None,
            })
            .collect();
        let req = DecisionRequest::DeclareBlockers {
            eligible: eligible.clone(),
            attackers: attacker_ids.clone(),
        };
        let resp = self.ask(defender, &req);

        let mut blocks: Vec<Block> = Vec::new();
        if let DecisionResponse::Pairs(pairs) = resp {
            for (blk_idx, atk_local) in pairs {
                if let Some(opt) = eligible.get(blk_idx as usize) {
                    // A creature can block only one attacker (keep the first assignment).
                    if blocks.iter().any(|b| b.blocker == opt.creature) {
                        continue;
                    }
                    if let Some(&attacker) = opt.may_block.get(atk_local as usize) {
                        blocks.push(Block {
                            blocker: opt.creature,
                            attacker,
                        });
                    }
                }
            }
        }
        // Menace (CR 702.111): a creature with menace can't be blocked except by two or more.
        // An attacker blocked by exactly one is illegally blocked → that block is dropped (it
        // becomes unblocked).
        let menace_attackers: Vec<ObjId> = combat
            .attackers
            .iter()
            .map(|a| a.attacker)
            .filter(|&atk| self.state.computed(atk).has_keyword(Keyword::Menace))
            .collect();
        for atk in menace_attackers {
            if blocks.iter().filter(|b| b.attacker == atk).count() == 1 {
                blocks.retain(|b| b.attacker != atk);
            }
        }

        if let Some(c) = self.state.combat.as_mut() {
            c.blocks = blocks;
        }
    }

    /// Combat Damage step (CR 510), with first/double strike substeps (510.4). If any
    /// combatant has first or double strike there are two damage substeps: first-strikers +
    /// double-strikers deal in step 1, then SBAs are applied (dead creatures don't deal in
    /// step 2), then double-strikers (again) + the remaining normal creatures deal in step 2.
    pub(crate) fn combat_damage(&mut self) {
        let combat = match self.state.combat.clone() {
            Some(c) if !c.attackers.is_empty() => c,
            _ => return,
        };
        let two_steps = self.any_first_or_double_strike(&combat);
        if two_steps {
            self.deal_combat_substep(&combat, CombatStep::FirstStrike);
            self.apply_combat_deaths(); // CR 510.4 — SBAs between the two damage steps
            self.deal_combat_substep(&combat, CombatStep::Regular);
        } else {
            self.deal_combat_substep(&combat, CombatStep::Single);
        }
    }

    fn any_first_or_double_strike(&self, combat: &CombatState) -> bool {
        combat
            .attackers
            .iter()
            .map(|a| a.attacker)
            .chain(combat.blocks.iter().map(|b| b.blocker))
            .any(|id| {
                let k = self.state.computed(id);
                k.has_keyword(Keyword::FirstStrike) || k.has_keyword(Keyword::DoubleStrike)
            })
    }

    /// Whether `id` deals damage in this substep (CR 510.4).
    fn deals_in(&self, id: ObjId, step: CombatStep) -> bool {
        let k = self.state.computed(id);
        let fs = k.has_keyword(Keyword::FirstStrike);
        let ds = k.has_keyword(Keyword::DoubleStrike);
        match step {
            CombatStep::Single => true,
            CombatStep::FirstStrike => fs || ds,
            CombatStep::Regular => ds || !fs,
        }
    }

    fn alive(&self, id: ObjId) -> bool {
        self.state
            .objects
            .get(&id)
            .is_some_and(|o| o.zone == Zone::Battlefield)
    }

    /// Gather and deal one combat-damage substep through the whiteboard (so prevention applies).
    fn deal_combat_substep(&mut self, combat: &CombatState, step: CombatStep) {
        let mut pending: Vec<(Target, u32, ObjId)> = Vec::new();

        for atk in &combat.attackers {
            if self.alive(atk.attacker) && self.deals_in(atk.attacker, step) {
                let power = self.creature_power(atk.attacker);
                let kc = self.state.computed(atk.attacker);
                let trample = kc.has_keyword(Keyword::Trample);
                let deathtouch = kc.has_keyword(Keyword::Deathtouch);
                let was_blocked = combat.blocks.iter().any(|b| b.attacker == atk.attacker);
                let live_blockers: Vec<ObjId> = combat
                    .blockers_of(atk.attacker)
                    .into_iter()
                    .filter(|&b| self.alive(b))
                    .collect();

                if !was_blocked {
                    // Unblocked (CR 510.1b).
                    if power > 0 {
                        pending.push((atk.defender, power, atk.attacker));
                    }
                } else if live_blockers.is_empty() {
                    // Blocked but all blockers gone: only trample assigns (to the defender).
                    if trample && power > 0 {
                        pending.push((atk.defender, power, atk.attacker));
                    }
                } else {
                    for (recip, amt) in self.assign_blocked_damage(
                        atk.attacker,
                        power,
                        &live_blockers,
                        deathtouch,
                        if trample { Some(atk.defender) } else { None },
                    ) {
                        if amt > 0 {
                            pending.push((recip, amt, atk.attacker));
                        }
                    }
                }
            }
            // Blockers deal to the attacker they block (CR 510.1d).
            if self.alive(atk.attacker) {
                for &blk in &combat.blockers_of(atk.attacker) {
                    if self.alive(blk) && self.deals_in(blk, step) {
                        let bp = self.creature_power(blk);
                        if bp > 0 {
                            pending.push((Target::Object(atk.attacker), bp, blk));
                        }
                    }
                }
            }
        }

        if pending.is_empty() {
            return;
        }
        // Controllers whose creatures dealt combat damage to a player this step (CR 510.1c) — for the
        // batched `YouDealCombatDamageToPlayer` trigger, fired once per such controller (Killian's).
        // Also the individual SOURCE creatures that dealt to a player — for the per-creature
        // `SelfDealsCombatDamageToPlayer` trigger (Snooping Page), fired once each.
        let mut dealt_to_player: std::collections::BTreeSet<PlayerId> = std::collections::BTreeSet::new();
        let mut sources_to_player: Vec<ObjId> = Vec::new();
        for (target, amount, source) in &pending {
            if *amount > 0 && matches!(target, Target::Player(_)) {
                if let Some(o) = self.state.objects.get(source) {
                    dealt_to_player.insert(o.controller);
                }
                if !sources_to_player.contains(source) {
                    sources_to_player.push(*source);
                }
            }
        }
        let mut wb = Whiteboard::new(WbReason::CombatDamage, ResolutionCtx::default());
        for (target, amount, source) in pending {
            wb.push(Action::Damage {
                target,
                amount,
                source,
                kind: DamageKind::Combat,
            });
        }
        self.commit(wb);
        // Per-creature "this creature deals combat damage to a player" (CR 603.2) — queued from the
        // battlefield source directly (before the batched controller event), so a creature still on
        // the battlefield fires its own draw/lose trigger.
        for source in sources_to_player {
            self.queue_self_triggers(source, crate::effects::ability::EventPattern::SelfDealsCombatDamageToPlayer);
        }
        for controller in dealt_to_player {
            self.broadcast(crate::agent::GameEvent::CombatDamageToPlayerBy { controller });
        }
    }

    /// Apply creature-death SBAs immediately (between the two combat-damage steps, CR 510.4):
    /// move lethally-damaged creatures to the graveyard so they don't deal in the next step.
    fn apply_combat_deaths(&mut self) {
        use crate::sba::StateBasedAction;
        let dead: Vec<ObjId> = crate::sba::collect(&self.state)
            .into_iter()
            .filter_map(|s| match s {
                StateBasedAction::CreatureDies { creature, .. } => Some(creature),
                _ => None,
            })
            .collect();
        for id in dead {
            let owner = match self.state.objects.get(&id) {
                Some(o) => o.owner,
                None => continue,
            };
            if self.state.move_object(id, Zone::Graveyard, owner) {
                self.broadcast(crate::agent::GameEvent::PermanentDied { obj: id });
                self.broadcast(crate::agent::GameEvent::ObjectMoved {
                    obj: id,
                    to: Zone::Graveyard,
                });
            }
        }
    }

    /// Assign a blocked attacker's `power` among its (living) `blockers`. Deathtouch makes 1
    /// lethal (CR 702.2); trample (CR 702.19) sends excess past lethal to `trample_to`.
    /// Multi-block without trample uses an `AssignCombatDamage` decision (CR 510.1c).
    fn assign_blocked_damage(
        &mut self,
        attacker: ObjId,
        power: u32,
        blockers: &[ObjId],
        deathtouch: bool,
        trample_to: Option<Target>,
    ) -> Vec<(Target, u32)> {
        let lethal = |s: &Self, b: ObjId| if deathtouch { 1 } else { s.creature_lethal(b).max(1) };

        if let Some(defender) = trample_to {
            // Assign lethal to each blocker (in order), excess to the defender.
            let mut out = Vec::new();
            let mut left = power;
            for &b in blockers {
                let need = lethal(self, b).min(left);
                out.push((Target::Object(b), need));
                left -= need;
            }
            if left > 0 {
                out.push((defender, left));
            }
            return out;
        }

        if blockers.len() == 1 {
            return vec![(Target::Object(blockers[0]), power)];
        }
        // Multi-block, no trample: the attacker's controller divides the damage (CR 510.1c).
        let controller = self
            .state
            .objects
            .get(&attacker)
            .map(|o| o.controller)
            .unwrap_or(self.state.active_player);
        let recipients: Vec<DamageSlot> = blockers
            .iter()
            .map(|&b| DamageSlot {
                recipient: Target::Object(b),
                lethal: lethal(self, b),
            })
            .collect();
        let req = DecisionRequest::AssignCombatDamage {
            source: attacker,
            recipients,
            total: power,
            deathtouch,
            trample_to: None,
        };
        let mut out: Vec<(Target, u32)> = blockers.iter().map(|&b| (Target::Object(b), 0)).collect();
        let mut assigned = 0u32;
        if let DecisionResponse::Amounts(amounts) = self.ask(controller, &req) {
            for (idx, amt) in amounts {
                if let Some(slot) = out.get_mut(idx as usize) {
                    let amt = amt.min(power.saturating_sub(assigned));
                    slot.1 += amt;
                    assigned += amt;
                }
            }
        }
        if assigned < power {
            out[0].1 += power - assigned;
        }
        out
    }

    /// End of Combat step (CR 511): combat ends, attackers/blockers are no longer in combat,
    /// "until end of combat" effects expire (none in milestone 3).
    pub(crate) fn end_combat(&mut self) {
        self.state.combat = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // The public engine type (the alias for `EngineCore` today; the blocking driver wrapper after
    // the M3 agent-removal split). Tests construct games via `Engine::new(state, agents)`.
    use crate::priority::Engine;
    use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView, RandomAgent};
    use crate::basics::Zone;
    use crate::cards::{self, grp};

    fn put_bf(state: &mut crate::state::GameState, owner: PlayerId, grp_id: u32) -> ObjId {
        let chars = state.card_db().get(grp_id).unwrap().chars.clone();
        let id = state.add_card(owner, chars, Zone::Battlefield);
        state.objects.get_mut(&id).unwrap().summoning_sick = false; // can attack/block
        id
    }

    fn engine() -> Engine {
        let state = cards::build_game(1, &[&[], &[]]);
        let agents: Vec<Box<dyn Agent>> =
            vec![Box::new(RandomAgent::new(1)), Box::new(RandomAgent::new(2))];
        Engine::new(state, agents)
    }

    fn aggro_engine() -> Engine {
        let state = cards::build_game(1, &[&[], &[]]);
        let agents: Vec<Box<dyn Agent>> = vec![Box::new(Aggressive), Box::new(Aggressive)];
        Engine::new(state, agents)
    }

    /// Attacks with everything; blocks each attacker with the first creature able to.
    struct Aggressive;
    impl Agent for Aggressive {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::DeclareAttackers { eligible } => DecisionResponse::Pairs(
                    eligible
                        .iter()
                        .enumerate()
                        .filter(|(_, o)| !o.may_attack.is_empty())
                        .map(|(i, _)| (i as u32, 0))
                        .collect(),
                ),
                DecisionRequest::DeclareBlockers { eligible, .. } => DecisionResponse::Pairs(
                    eligible
                        .iter()
                        .enumerate()
                        .filter(|(_, o)| !o.may_block.is_empty())
                        .map(|(i, _)| (i as u32, 0))
                        .collect(),
                ),
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

    fn solo_combat(e: &mut Engine, attacker: ObjId, blockers: &[ObjId]) {
        e.state.active_player = PlayerId(0);
        e.state.combat = Some(CombatState {
            attackers: vec![Attack {
                attacker,
                defender: Target::Player(PlayerId(1)),
            }],
            blocks: blockers
                .iter()
                .map(|&b| Block { blocker: b, attacker })
                .collect(),
        });
    }

    #[test]
    fn cant_be_blocked_until_eot_escape_tunnel() {
        use crate::effects::ability::Qualification;
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::effects::condition::Duration;
        use crate::effects::{Effect, EffectTarget};
        // Escape Tunnel: "target creature can't be blocked this turn." Granting CantBeBlocked makes
        // can_block(any blocker) false until cleanup (CR 509.1b / 514.2).
        let mut e = engine();
        let atk = put_bf(&mut e.state, PlayerId(0), grp::GRIZZLY_BEARS);
        let blk = put_bf(&mut e.state, PlayerId(1), grp::GRIZZLY_BEARS);
        assert!(e.can_block(blk, atk), "normally the blocker can block");

        e.resolve_effect(
            &Effect::GrantQualification {
                what: EffectTarget::ChosenIndex(0),
                qualification: Qualification::CantBeBlocked,
                duration: Duration::UntilEndOfTurn,
            },
            &ResolutionCtx {
                controller: Some(PlayerId(0)),
                chosen_targets: vec![Target::Object(atk)],
                ..Default::default()
            },
            WbReason::Resolve(crate::ids::StackId(0)),
        );
        assert!(!e.can_block(blk, atk), "the attacker can't be blocked this turn");

        e.state.end_of_turn_continuous_cleanup();
        assert!(e.can_block(blk, atk), "blockable again after cleanup");
    }

    #[test]
    fn anthem_boosts_combat_damage() {
        // A 2/2 under Glorious Anthem (computed 3/3) attacks unblocked → 3 damage (CR 613 P/T
        // reaches combat).
        let mut e = engine();
        let bears = put_bf(&mut e.state, PlayerId(0), grp::GRIZZLY_BEARS);
        put_bf(&mut e.state, PlayerId(0), grp::GLORIOUS_ANTHEM);
        e.state.active_player = PlayerId(0);
        e.state.combat = Some(CombatState {
            attackers: vec![Attack {
                attacker: bears,
                defender: Target::Player(PlayerId(1)),
            }],
            blocks: vec![],
        });
        e.combat_damage();
        assert_eq!(e.state.player(PlayerId(1)).life, 17, "2/2 + anthem = 3 damage");
    }

    #[test]
    fn flying_evasion_masks_blocks() {
        // A granted-flying attacker (Levitation) can't be blocked by a ground creature, but a
        // flyer can block it; a non-flying attacker can be blocked normally (CR 509.1b / 702.9).
        let mut e = engine();
        let atk = put_bf(&mut e.state, PlayerId(0), grp::GRIZZLY_BEARS);
        let blk = put_bf(&mut e.state, PlayerId(1), grp::GRIZZLY_BEARS);
        assert!(e.can_block(blk, atk), "ground blocks ground");

        put_bf(&mut e.state, PlayerId(0), grp::LEVITATION); // P0's creatures gain flying
        assert!(!e.can_block(blk, atk), "ground can't block a flyer");

        put_bf(&mut e.state, PlayerId(1), grp::LEVITATION); // P1's creatures gain flying too
        assert!(e.can_block(blk, atk), "a flyer can block a flyer");
    }

    #[test]
    fn double_strike_deals_twice() {
        // Fencing Ace (1/1 double strike), unblocked → 1 (first-strike step) + 1 (regular) = 2.
        let mut e = engine();
        let ace = put_bf(&mut e.state, PlayerId(0), grp::FENCING_ACE);
        solo_combat(&mut e, ace, &[]);
        e.combat_damage();
        assert_eq!(e.state.player(PlayerId(1)).life, 18);
    }

    #[test]
    fn first_strike_kills_before_retaliation() {
        // Elvish Archers (2/1 first strike) blocked by Grizzly Bears (2/2): Archers deals 2 in
        // the first-strike step → Bears dies before it can deal back → Archers takes no damage.
        let mut e = engine();
        let archers = put_bf(&mut e.state, PlayerId(0), grp::ELVISH_ARCHERS);
        let bears = put_bf(&mut e.state, PlayerId(1), grp::GRIZZLY_BEARS);
        solo_combat(&mut e, archers, &[bears]);
        e.combat_damage();
        assert_eq!(e.state.object(bears).zone, Zone::Graveyard, "Bears died to first strike");
        assert_eq!(e.state.object(archers).zone, Zone::Battlefield, "Archers survived");
        assert_eq!(e.state.object(archers).damage_marked, 0, "took no damage back");
    }

    #[test]
    fn trample_assigns_excess_to_player() {
        // Argothian Swine (3/3 trample) blocked by a 2/2: 2 lethal to the blocker, 1 tramples.
        let mut e = engine();
        let swine = put_bf(&mut e.state, PlayerId(0), grp::ARGOTHIAN_SWINE);
        let bears = put_bf(&mut e.state, PlayerId(1), grp::GRIZZLY_BEARS);
        solo_combat(&mut e, swine, &[bears]);
        e.combat_damage();
        assert_eq!(e.state.player(PlayerId(1)).life, 19, "1 excess trampled over");
        assert_eq!(e.state.object(bears).damage_marked, 2, "blocker took lethal");
    }

    #[test]
    fn deathtouch_makes_any_damage_lethal() {
        // Typhoid Rats (1/1 deathtouch) blocked by Hill Giant (3/3): the 1 damage is lethal.
        let mut e = engine();
        let rats = put_bf(&mut e.state, PlayerId(0), grp::TYPHOID_RATS);
        let giant = put_bf(&mut e.state, PlayerId(1), grp::HILL_GIANT);
        solo_combat(&mut e, rats, &[giant]);
        e.combat_damage();
        let dies = |id| {
            crate::sba::collect(&e.state).iter().any(|s| matches!(
                s,
                crate::sba::StateBasedAction::CreatureDies { creature, .. } if *creature == id
            ))
        };
        assert!(dies(giant), "Hill Giant dies to 1 deathtouch damage (704.5h)");
        assert!(dies(rats), "Rats dies to the Giant's 3 (lethal)");
    }

    #[test]
    fn lifelink_gains_controller_life() {
        // Child of Night (2/1 lifelink) unblocked → 2 to opponent, +2 to its controller.
        let mut e = engine();
        let cn = put_bf(&mut e.state, PlayerId(0), grp::CHILD_OF_NIGHT);
        solo_combat(&mut e, cn, &[]);
        e.combat_damage();
        assert_eq!(e.state.player(PlayerId(1)).life, 18, "2 damage dealt");
        assert_eq!(e.state.player(PlayerId(0)).life, 22, "lifelink gained 2");
    }

    #[test]
    fn defender_cant_attack() {
        let mut e = aggro_engine();
        put_bf(&mut e.state, PlayerId(0), grp::WALL_OF_STONE); // 0/8 defender
        e.state.active_player = PlayerId(0);
        e.declare_attackers();
        assert!(e.state.combat.is_none(), "Defender cannot attack → no attackers declared");
    }

    #[test]
    fn vigilance_attacks_without_tapping() {
        let mut e = aggro_engine();
        let grenadier = put_bf(&mut e.state, PlayerId(0), grp::ALABORN_GRENADIER); // vigilance
        e.state.active_player = PlayerId(0);
        e.declare_attackers();
        assert!(e.state.combat.is_some(), "attacked");
        assert!(!e.state.object(grenadier).status.tapped, "vigilance: not tapped");
    }

    #[test]
    fn menace_needs_two_blockers() {
        // Alley Strangler (menace) attacks; the defender's single blocker can't block it alone.
        let mut e = aggro_engine();
        let strangler = put_bf(&mut e.state, PlayerId(0), grp::ALLEY_STRANGLER);
        put_bf(&mut e.state, PlayerId(1), grp::GRIZZLY_BEARS); // lone blocker
        e.state.active_player = PlayerId(0);
        e.declare_attackers();
        e.declare_blockers();
        let blocks = &e.state.combat.as_ref().unwrap().blocks;
        assert!(
            !blocks.iter().any(|b| b.attacker == strangler),
            "menace: a single block is dropped (attacker stays unblocked)"
        );
    }

    #[test]
    fn haste_ignores_summoning_sickness() {
        let mut e = aggro_engine();
        let goblin = put_bf(&mut e.state, PlayerId(0), grp::RAGING_GOBLIN); // haste
        e.state.objects.get_mut(&goblin).unwrap().summoning_sick = true;
        let bears = put_bf(&mut e.state, PlayerId(0), grp::GRIZZLY_BEARS); // no haste
        e.state.objects.get_mut(&bears).unwrap().summoning_sick = true;
        e.state.active_player = PlayerId(0);
        e.declare_attackers();
        let atks: Vec<ObjId> = e
            .state
            .combat
            .as_ref()
            .map(|c| c.attackers.iter().map(|a| a.attacker).collect())
            .unwrap_or_default();
        assert!(atks.contains(&goblin), "haste attacks despite summoning sickness");
        assert!(!atks.contains(&bears), "a summoning-sick non-haste creature can't attack");
    }

    #[test]
    fn pacifism_prevents_attacking_and_blocking() {
        // "Enchanted creature can't attack or block" → the CantAttack/CantBlock qualifications
        // remove the host from the attacker pool and from every blocker's eligibility.
        let mut e = aggro_engine();
        let free = put_bf(&mut e.state, PlayerId(0), grp::GRIZZLY_BEARS);
        let pacified = put_bf(&mut e.state, PlayerId(0), grp::GRIZZLY_BEARS);
        let pac = put_bf(&mut e.state, PlayerId(0), grp::PACIFISM);
        e.state.objects.get_mut(&pac).unwrap().attached_to = Some(pacified);
        e.state.mark_chars_dirty();
        e.state.active_player = PlayerId(0);
        e.declare_attackers();
        let atks: Vec<ObjId> = e
            .state
            .combat
            .as_ref()
            .map(|c| c.attackers.iter().map(|a| a.attacker).collect())
            .unwrap_or_default();
        assert!(atks.contains(&free), "an unpacified creature can attack");
        assert!(!atks.contains(&pacified), "a pacified creature can't attack");

        // Blocking side: a pacified creature on defense can't be assigned as a blocker.
        let mut e2 = aggro_engine();
        let attacker = put_bf(&mut e2.state, PlayerId(0), grp::GRIZZLY_BEARS);
        let blocker = put_bf(&mut e2.state, PlayerId(1), grp::GRIZZLY_BEARS);
        let pac2 = put_bf(&mut e2.state, PlayerId(1), grp::PACIFISM);
        e2.state.objects.get_mut(&pac2).unwrap().attached_to = Some(blocker);
        e2.state.mark_chars_dirty();
        e2.state.active_player = PlayerId(0);
        e2.state.combat = Some(CombatState {
            attackers: vec![Attack { attacker, defender: Target::Player(PlayerId(1)) }],
            blocks: Vec::new(),
        });
        e2.declare_blockers();
        assert!(
            e2.state.combat.as_ref().unwrap().blocks.is_empty(),
            "a pacified creature can't block"
        );
    }

    #[test]
    fn a_planeswalker_can_be_attacked_and_loses_loyalty() {
        // CR 508.1a/306.8/120.3: a creature may attack a planeswalker; combat damage to it
        // removes that many loyalty counters, and 0 loyalty kills it (704.5i).
        use crate::basics::{CardType, CounterKind};
        let mut e = engine();
        let giant = put_bf(&mut e.state, PlayerId(0), grp::HILL_GIANT); // 3/3
        let chars = crate::state::Characteristics {
            name: "Walker".into(),
            card_types: vec![CardType::Planeswalker],
            loyalty: Some(3),
            ..Default::default()
        };
        let pw = e.state.add_card(PlayerId(1), chars, Zone::Battlefield);
        e.state.active_player = PlayerId(0);
        e.state.combat = Some(CombatState {
            attackers: vec![Attack { attacker: giant, defender: Target::Object(pw) }],
            blocks: Vec::new(),
        });
        e.combat_damage();
        assert_eq!(
            e.state.object(pw).counters.get(&CounterKind::Loyalty),
            0,
            "3 combat damage removed 3 loyalty"
        );
        assert!(
            crate::sba::collect(&e.state)
                .iter()
                .any(|s| matches!(s, crate::sba::StateBasedAction::PlaneswalkerDies { pw: x } if *x == pw)),
            "a 0-loyalty planeswalker is collected for death"
        );
    }
}
