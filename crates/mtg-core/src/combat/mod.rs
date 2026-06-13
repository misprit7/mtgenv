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
//! The data types ([`CombatState`]) live in `GameState`; the step logic is `impl Engine`.

use serde::{Deserialize, Serialize};

use crate::agent::{
    AttackerOption, BlockerOption, DamageSlot, DecisionRequest, DecisionResponse,
};
use crate::basics::{DamageKind, Target};
use crate::effects::ability::Keyword;
use crate::effects::action::{Action, ResolutionCtx, Whiteboard, WbReason};
use crate::ids::{ObjId, PlayerId};
use crate::priority::Engine;

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

impl CombatState {
    /// The blockers assigned to `attacker`, in declaration order.
    pub fn blockers_of(&self, attacker: ObjId) -> Vec<ObjId> {
        self.blocks
            .iter()
            .filter(|b| b.attacker == attacker)
            .map(|b| b.blocker)
            .collect()
    }
}

impl Engine {
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
                // Computed type (CR 613) — a land that became a creature can attack.
                self.state.computed(id).is_creature() && !o.status.tapped && !o.summoning_sick
            })
            .collect();
        if eligible_ids.is_empty() {
            return;
        }
        let eligible: Vec<AttackerOption> = eligible_ids
            .iter()
            .map(|&id| AttackerOption {
                creature: id,
                may_attack: vec![Target::Player(defender)],
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
        // Tap attackers (CR 508.1f — not a cost; vigilance deferred so all attackers tap).
        for a in &attacks {
            if let Some(o) = self.state.objects.get_mut(&a.attacker) {
                o.status.tapped = true;
            }
        }
        self.state.combat = Some(CombatState {
            attackers: attacks,
            blocks: Vec::new(),
        });
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
        if let Some(c) = self.state.combat.as_mut() {
            c.blocks = blocks;
        }
    }

    /// Combat Damage step (CR 510): assign and deal all combat damage simultaneously, then
    /// the following priority round's SBAs destroy lethally-damaged creatures.
    pub(crate) fn combat_damage(&mut self) {
        let combat = match self.state.combat.clone() {
            Some(c) if !c.attackers.is_empty() => c,
            _ => return,
        };

        // (recipient, amount, source) — gathered, then applied all at once (CR 510.2).
        let mut pending: Vec<(Target, u32, ObjId)> = Vec::new();

        for atk in &combat.attackers {
            let power = self.creature_power(atk.attacker);
            let blockers = combat.blockers_of(atk.attacker);

            if blockers.is_empty() {
                // Unblocked: damage to what it is attacking (CR 510.1b).
                if power > 0 {
                    pending.push((atk.defender, power, atk.attacker));
                }
            } else {
                // Blocked: assign the attacker's power among its blockers (CR 510.1c).
                let assignment = self.assign_among_blockers(atk.attacker, power, &blockers);
                for (blocker, amount) in assignment {
                    if amount > 0 {
                        pending.push((Target::Object(blocker), amount, atk.attacker));
                    }
                }
            }
            // Each blocker deals its power to the attacker it blocks (CR 510.1d).
            for &blocker in &blockers {
                let bp = self.creature_power(blocker);
                if bp > 0 {
                    pending.push((Target::Object(atk.attacker), bp, blocker));
                }
            }
        }

        // Deal it through the whiteboard so replacement/prevention effects (e.g. Fog Bank)
        // can rewrite the combat-damage event before it happens (CR 510.2 / 614/615).
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
    }

    /// Decide how a blocked attacker assigns its `power` among `blockers`. A single blocker
    /// gets it all; multiple blockers trigger an `AssignCombatDamage` decision (controller's
    /// choice, CR 510.1c). Defensive against a malformed response (falls back to dumping all
    /// on the first blocker).
    fn assign_among_blockers(
        &mut self,
        attacker: ObjId,
        power: u32,
        blockers: &[ObjId],
    ) -> Vec<(ObjId, u32)> {
        if blockers.len() == 1 {
            return vec![(blockers[0], power)];
        }
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
                lethal: self.creature_lethal(b),
            })
            .collect();
        let req = DecisionRequest::AssignCombatDamage {
            source: attacker,
            recipients,
            total: power,
            deathtouch: false,
            trample_to: None,
        };
        let mut out: Vec<(ObjId, u32)> = blockers.iter().map(|&b| (b, 0)).collect();
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
        // Dump any unassigned remainder on the first blocker (keeps total == power).
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
    use crate::agent::{Agent, RandomAgent};
    use crate::basics::Zone;
    use crate::cards::{self, grp};

    fn put_bf(state: &mut crate::state::GameState, owner: PlayerId, grp_id: u32) -> ObjId {
        let chars = state.card_db().get(grp_id).unwrap().chars.clone();
        state.add_card(owner, chars, Zone::Battlefield)
    }

    fn engine() -> Engine {
        let state = cards::build_game(1, &[&[], &[]]);
        let agents: Vec<Box<dyn Agent>> =
            vec![Box::new(RandomAgent::new(1)), Box::new(RandomAgent::new(2))];
        Engine::new(state, agents)
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
}
