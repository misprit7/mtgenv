//! Return the Favor — `{R}{R}` Instant (first printed OTJ; reprinted on the SOS Mystical Archive `soa`).
//!
//! Oracle: "Spree (Choose one or more additional costs.)
//! + {1} — Copy target instant spell, sorcery spell, activated ability, or triggered ability. You may
//!         choose new targets for the copy.
//! + {1} — Change the target of target spell or ability with a single target."
//!
//! **Fully implemented** — the Spree subsystem (`Effect::Spree`): mode 1 is `CopySpellOnStack` (CR
//! 707.10, "you may choose new targets"), mode 2 is the new `Effect::ChangeTarget` (CR 115.7). Only
//! *spell* stack objects are targetable in the first pass (abilities-on-stack targeting is out of
//! scope) — so both modes act on spells; a legal alternative is required for the change to happen.

use crate::basics::{CardType, Color};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::ValueExpr;
use crate::effects::{Effect, EffectTarget, SpreeMode};

/// grp id (bonus-sheet `soa` cards use the 600+ block).
pub const RETURN_THE_FAVOR: u32 = 649;

/// "target instant or sorcery spell" (a stack object) — the copy mode's victim.
fn instant_or_sorcery_spell() -> TargetSpec {
    TargetSpec {
        kind: TargetKind::StackObject(CardFilter::AnyOf(vec![
            CardFilter::HasCardType(CardType::Instant),
            CardFilter::HasCardType(CardType::Sorcery),
        ])),
        min: 1,
        max: 1,
        distinct: true,
    }
}

pub fn register(db: &mut CardDb) {
    let effect = Effect::Spree {
        modes: vec![
            SpreeMode {
                cost: mana_cost(1, &[]),
                label: "Copy target instant or sorcery spell. You may choose new targets for the copy."
                    .into(),
                effect: Effect::CopySpellOnStack {
                    what: EffectTarget::Target(instant_or_sorcery_spell()),
                    count: ValueExpr::Fixed(1),
                    choose_new_targets: true,
                },
            },
            SpreeMode {
                cost: mana_cost(1, &[]),
                label: "Change the target of target spell or ability with a single target.".into(),
                effect: Effect::ChangeTarget {
                    what: EffectTarget::Target(TargetSpec {
                        kind: TargetKind::StackObject(CardFilter::HasSingleTarget),
                        min: 1,
                        max: 1,
                        distinct: true,
                    }),
                },
            },
        ],
    };
    let def = spell(
        RETURN_THE_FAVOR,
        "Return the Favor",
        CardType::Instant,
        Color::Red,
        mana_cost(0, &[(Color::Red, 2)]),
        effect,
    )
    .with_text(
        "Spree (Choose one or more additional costs.)\n+ {1} — Copy target instant spell, sorcery spell, activated ability, or triggered ability. You may choose new targets for the copy.\n+ {1} — Change the target of target spell or ability with a single target.",
    );
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, CastVariant, DecisionRequest, DecisionResponse, PlayerView};
    use crate::basics::{Phase, Target};
    use crate::cards::{build_game, grp};
    use crate::ids::{ObjId, PlayerId};
    use crate::priority::Engine;

    /// Chooses the given Spree modes; for each `ChooseTargets` slot picks the first legal target that
    /// appears in `prefer` (in order), else the first legal. Passes otherwise.
    #[derive(Clone)]
    struct ScriptAgent {
        modes: Vec<u32>,
        prefer: Vec<Target>,
    }
    impl Agent for ScriptAgent {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::ChooseModes { .. } => DecisionResponse::Indices(self.modes.clone()),
                DecisionRequest::ChooseTargets { slots, .. } if !slots.is_empty() => {
                    let legal = &slots[0].legal;
                    let pick = self
                        .prefer
                        .iter()
                        .find_map(|want| legal.iter().position(|t| t == want))
                        .unwrap_or(0);
                    DecisionResponse::Pairs(vec![(0, pick as u32)])
                }
                _ => DecisionResponse::Pass,
            }
        }
    }

    fn add(state: &mut crate::state::GameState, p: PlayerId, grp: u32, zone: crate::basics::Zone) -> ObjId {
        state.add_card(p, state.card_db().get(grp).unwrap().chars.clone(), zone)
    }

    #[test]
    fn return_the_favor_registers() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(RETURN_THE_FAVOR).unwrap();
        assert!(def.fully_implemented);
        assert!(matches!(def.spell_effect(), Some(Effect::Spree { .. })));
    }

    /// Mode 1 — copy a Lightning Bolt on the stack (new target = same player). The player takes the
    /// copy's 3 AND the original's 3 = 6.
    #[test]
    fn spree_copy_mode_doubles_a_bolt() {
        use crate::basics::Zone;
        let mut state = build_game(1, &[&[], &[]]);
        let bolt = add(&mut state, PlayerId(0), grp::LIGHTNING_BOLT, Zone::Hand);
        let favor = add(&mut state, PlayerId(0), RETURN_THE_FAVOR, Zone::Hand);
        for _ in 0..3 {
            add(&mut state, PlayerId(0), grp::MOUNTAIN, Zone::Battlefield);
        }
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let prefer = vec![Target::Player(PlayerId(1))];
        let mut e = Engine::new(
            state,
            vec![
                Box::new(ScriptAgent { modes: vec![0], prefer: prefer.clone() }),
                Box::new(ScriptAgent { modes: vec![], prefer: prefer.clone() }),
            ],
        );
        // Cast the Bolt at P1, then Return the Favor (copy mode) targeting the Bolt.
        e.cast_spell(PlayerId(0), bolt, CastVariant::Normal);
        e.run_agenda();
        e.cast_spell(PlayerId(0), favor, CastVariant::Normal);
        e.run_agenda();
        while !e.state.stack.items.is_empty() {
            e.resolve_top();
            e.run_agenda();
        }
        assert_eq!(e.state.player(PlayerId(1)).life, 14, "copy (3) + original (3) = 6 damage");
    }

    /// Mode 2 — change a Bolt aimed at creature A onto creature B. B (a 2/2) dies; A survives.
    #[test]
    fn spree_change_target_redirects_a_bolt() {
        use crate::basics::Zone;
        let mut state = build_game(1, &[&[], &[]]);
        let bolt = add(&mut state, PlayerId(0), grp::LIGHTNING_BOLT, Zone::Hand);
        let favor = add(&mut state, PlayerId(0), RETURN_THE_FAVOR, Zone::Hand);
        for _ in 0..3 {
            add(&mut state, PlayerId(0), grp::MOUNTAIN, Zone::Battlefield);
        }
        let creature_a = add(&mut state, PlayerId(1), grp::GRIZZLY_BEARS, Zone::Battlefield);
        let creature_b = add(&mut state, PlayerId(1), grp::GRIZZLY_BEARS, Zone::Battlefield);
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        // Bolt cast prefers creature A; the retarget prefers creature B (A is excluded there).
        let prefer = vec![Target::Object(creature_a), Target::Object(creature_b)];
        let mut e = Engine::new(
            state,
            vec![
                Box::new(ScriptAgent { modes: vec![1], prefer: prefer.clone() }),
                Box::new(ScriptAgent { modes: vec![], prefer: prefer.clone() }),
            ],
        );
        e.cast_spell(PlayerId(0), bolt, CastVariant::Normal);
        e.run_agenda();
        e.cast_spell(PlayerId(0), favor, CastVariant::Normal);
        e.run_agenda();
        while !e.state.stack.items.is_empty() {
            e.resolve_top();
            e.run_agenda();
        }
        assert!(e.state.player(PlayerId(1)).graveyard.contains(&creature_b), "retargeted creature B died");
        assert!(e.state.player(PlayerId(1)).battlefield.contains(&creature_a), "original target A survived");
    }
}
