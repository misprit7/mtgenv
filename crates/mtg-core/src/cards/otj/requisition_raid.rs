//! Requisition Raid — `{W}` Sorcery (first printed OTJ; reprinted on the SOS Mystical Archive `soa`).
//!
//! Oracle: "Spree (Choose one or more additional costs.)
//! + {1} — Destroy target artifact.
//! + {1} — Destroy target enchantment.
//! + {1} — Put a +1/+1 counter on each creature target player controls."
//!
//! **Fully implemented** — the Spree subsystem (`Effect::Spree`): choose one or more modes at cast, each
//! adding its `{1}` to the total cost (CR 601.2b/f / 702.163), pay `{W}` + the chosen modes' costs, then
//! resolve each chosen mode's effect. Mode 3 is the `TargetPlayer` + `ForEach` counters idiom.

use crate::basics::{CardType, Color, CounterKind, Zone};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::target::{CardFilter, PlayerFilter, SelectSpec, TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget, SpreeMode};

/// grp id (bonus-sheet `soa` cards use the 600+ block).
pub const REQUISITION_RAID: u32 = 648;

/// "Destroy target [artifact|enchantment]" — a single-target destroy of a permanent of `ct`.
fn destroy_mode(ct: CardType) -> Effect {
    Effect::Destroy {
        what: EffectTarget::Target(TargetSpec {
            kind: TargetKind::Permanent(CardFilter::HasCardType(ct)),
            min: 1,
            max: 1,
            distinct: true,
        }),
    }
}

pub fn register(db: &mut CardDb) {
    let effect = Effect::Spree {
        modes: vec![
            SpreeMode {
                cost: mana_cost(1, &[]),
                label: "Destroy target artifact.".into(),
                effect: destroy_mode(CardType::Artifact),
            },
            SpreeMode {
                cost: mana_cost(1, &[]),
                label: "Destroy target enchantment.".into(),
                effect: destroy_mode(CardType::Enchantment),
            },
            SpreeMode {
                cost: mana_cost(1, &[]),
                label: "Put a +1/+1 counter on each creature target player controls.".into(),
                // "target player" (slot) then a per-player-scoped ForEach over their creatures.
                effect: Effect::Sequence(vec![
                    Effect::TargetPlayer(PlayerFilter::Any),
                    Effect::ForEach {
                        selector: SelectSpec {
                            zone: Zone::Battlefield,
                            filter: CardFilter::HasCardType(CardType::Creature),
                            chooser: PlayerRef::ChosenTarget(0),
                            min: ValueExpr::Fixed(0),
                            max: ValueExpr::Fixed(999),
                        },
                        body: Box::new(Effect::PutCounters {
                            what: EffectTarget::Each,
                            kind: CounterKind::PlusOnePlusOne,
                            n: ValueExpr::Fixed(1),
                        }),
                    },
                ]),
            },
        ],
    };
    let def = spell(
        REQUISITION_RAID,
        "Requisition Raid",
        CardType::Sorcery,
        Color::White,
        mana_cost(0, &[(Color::White, 1)]),
        effect,
    )
    .with_text(
        "Spree (Choose one or more additional costs.)\n+ {1} — Destroy target artifact.\n+ {1} — Destroy target enchantment.\n+ {1} — Put a +1/+1 counter on each creature target player controls.",
    );
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, CastVariant, DecisionRequest, DecisionResponse, PlayableAction, PlayerView};
    use crate::basics::{Phase, Target};
    use crate::cards::{build_game, grp};
    use crate::ids::{ObjId, PlayerId};
    use crate::priority::Engine;

    /// Chooses the given Spree modes (by original index), the first legal target for each slot, and
    /// the given player for a "target player" slot. Passes otherwise.
    #[derive(Clone)]
    struct SpreeAgent {
        modes: Vec<u32>,
        target_player: PlayerId,
    }
    impl Agent for SpreeAgent {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::ChooseModes { .. } => DecisionResponse::Indices(self.modes.clone()),
                DecisionRequest::ChooseTargets { slots, .. } if !slots.is_empty() => {
                    // Answer EVERY slot: prefer the target player where legal, else the first legal.
                    let want = Target::Player(self.target_player);
                    let pairs = slots
                        .iter()
                        .enumerate()
                        .filter(|(_, s)| !s.legal.is_empty())
                        .map(|(si, s)| {
                            let pick = s.legal.iter().position(|t| *t == want).unwrap_or(0);
                            (si as u32, pick as u32)
                        })
                        .collect();
                    DecisionResponse::Pairs(pairs)
                }
                _ => DecisionResponse::Pass,
            }
        }
    }

    fn creature(state: &mut crate::state::GameState, p: PlayerId) -> ObjId {
        state.add_card(p, state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Battlefield)
    }

    #[test]
    fn requisition_raid_registers() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(REQUISITION_RAID).unwrap();
        assert!(def.fully_implemented);
        assert!(matches!(def.spell_effect(), Some(Effect::Spree { .. })));
    }

    /// One mode: "+1/+1 counter on each creature target player controls." Pays {W} + {1}; both of the
    /// target player's creatures get a counter; the caster's own creature is untouched.
    #[test]
    fn spree_one_mode_counters_target_players_creatures() {
        let mut state = build_game(1, &[&[], &[]]);
        let raid = state.add_card(PlayerId(0), state.card_db().get(REQUISITION_RAID).unwrap().chars.clone(), Zone::Hand);
        for _ in 0..2 {
            state.add_card(PlayerId(0), state.card_db().get(grp::PLAINS).unwrap().chars.clone(), Zone::Battlefield);
        }
        let theirs_a = creature(&mut state, PlayerId(1));
        let theirs_b = creature(&mut state, PlayerId(1));
        let mine = creature(&mut state, PlayerId(0));
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let mut e = Engine::new(
            state,
            vec![Box::new(SpreeAgent { modes: vec![2], target_player: PlayerId(1) }), Box::new(SpreeAgent { modes: vec![], target_player: PlayerId(1) })],
        );
        e.cast_spell(PlayerId(0), raid, CastVariant::Normal);
        e.run_agenda();
        while !e.state.stack.items.is_empty() {
            e.resolve_top();
            e.run_agenda();
        }
        assert_eq!(e.state.object(theirs_a).counters.get(&CounterKind::PlusOnePlusOne), 1, "their creature A");
        assert_eq!(e.state.object(theirs_b).counters.get(&CounterKind::PlusOnePlusOne), 1, "their creature B");
        assert_eq!(e.state.object(mine).counters.get(&CounterKind::PlusOnePlusOne), 0, "caster's creature untouched");
        // {W} + {1} = 2 mana spent → both Plains tapped.
        let tapped = e.state.player(PlayerId(0)).battlefield.iter().filter(|&&id| e.state.object(id).status.tapped).count();
        assert_eq!(tapped, 2, "both Plains tapped for {{W}}+{{1}}");
    }

    /// Two modes: destroy target artifact AND counter each of target player's creatures. All three
    /// modes are legal here (an artifact + a creature are present), so the chosen mode indices are the
    /// printed order; picking modes 0 and 2 pays {W} + {1} + {1} = 3 mana (three Plains tapped).
    #[test]
    fn spree_two_modes_destroy_artifact_and_counters() {
        let mut state = build_game(1, &[&[], &[]]);
        let raid = state.add_card(PlayerId(0), state.card_db().get(REQUISITION_RAID).unwrap().chars.clone(), Zone::Hand);
        for _ in 0..3 {
            state.add_card(PlayerId(0), state.card_db().get(grp::PLAINS).unwrap().chars.clone(), Zone::Battlefield);
        }
        // An artifact (mode 0's target) + an attached enchantment (mode 1's target) make all three
        // modes legal, so the offered mode list equals the printed order.
        let artifact = state.add_card(PlayerId(1), state.card_db().get(grp::BONESPLITTER).unwrap().chars.clone(), Zone::Battlefield);
        let theirs = creature(&mut state, PlayerId(1));
        let pacifism = state.add_card(PlayerId(1), state.card_db().get(grp::PACIFISM).unwrap().chars.clone(), Zone::Battlefield);
        state.objects.get_mut(&pacifism).unwrap().attached_to = Some(theirs); // keep the Aura on-board (no SBA fall-off)
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let mut e = Engine::new(
            state,
            vec![Box::new(SpreeAgent { modes: vec![0, 2], target_player: PlayerId(1) }), Box::new(SpreeAgent { modes: vec![], target_player: PlayerId(1) })],
        );
        e.cast_spell(PlayerId(0), raid, CastVariant::Normal);
        e.run_agenda();
        while !e.state.stack.items.is_empty() {
            e.resolve_top();
            e.run_agenda();
        }
        assert!(e.state.player(PlayerId(1)).graveyard.contains(&artifact), "artifact destroyed");
        assert_eq!(e.state.object(theirs).counters.get(&CounterKind::PlusOnePlusOne), 1, "their creature got a counter");
        let tapped = e.state.player(PlayerId(0)).battlefield.iter().filter(|&&id| e.state.object(id).status.tapped).count();
        assert_eq!(tapped, 3, "{{W}}+{{1}}+{{1}} = 3 mana");
    }

    /// Spree offer gate (CR 702.163): with only {W} available (one Plains) the base cost is payable but
    /// no mode's additional {1} is → the cast is NOT offered. Adding a second Plains makes it offerable.
    #[test]
    fn spree_offer_requires_base_plus_one_mode() {
        let mut state = build_game(1, &[&[], &[]]);
        let raid = state.add_card(PlayerId(0), state.card_db().get(REQUISITION_RAID).unwrap().chars.clone(), Zone::Hand);
        let plains1 = state.add_card(PlayerId(0), state.card_db().get(grp::PLAINS).unwrap().chars.clone(), Zone::Battlefield);
        creature(&mut state, PlayerId(1)); // a legal target for mode 3
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let e = Engine::new(state, vec![Box::new(SpreeAgent { modes: vec![], target_player: PlayerId(1) }), Box::new(SpreeAgent { modes: vec![], target_player: PlayerId(1) })]);
        let offered = |e: &Engine| {
            e.legal_actions(PlayerId(0))
                .iter()
                .any(|a| matches!(a, PlayableAction::Cast { spell, .. } if *spell == raid))
        };
        assert!(!offered(&e), "only {{W}} available — no mode's {{1}} affordable, not offered");
        let mut e = e;
        e.state.add_card(PlayerId(0), e.state.card_db().get(grp::PLAINS).unwrap().chars.clone(), Zone::Battlefield);
        let _ = plains1;
        assert!(offered(&e), "with {{W}}+{{1}} available the Spree cast is offered");
    }
}
