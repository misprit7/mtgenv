//! Aziza, Mage Tower Captain — `{R}{W}` Legendary Creature — Djinn Sorcerer 2/2.
//!
//! Oracle: "Whenever you cast an instant or sorcery spell, you may tap three untapped creatures you
//! control. If you do, copy that spell. You may choose new targets for the copy."
//!
//! **Fully implemented** — a spell-copy consumer over the CR 707.10 copy cap (shared with Silverquill /
//! Mica), only the "cost" to copy is paid *during resolution*: a `Triggered{ SpellCast(instant|sorcery) }`
//! whose effect is `MayPayCost{ cost: Tap three creatures, then: CopySpellOnStack{ Triggering, count: 1,
//! new targets } }`. `MayPayCost` asks whether to pay, checks the cost is payable, taps three, then copies.
//!
//! ⚠️ Caveat: the `TapCreatures(3)` cost taps three **other** untapped creatures you control (it reuses
//! `crew_candidates`, which excludes the source). Aziza herself can't be one of the three, so in the rare
//! case where your only untapped creatures are Aziza + exactly two others, the copy can't be paid. Real
//! Aziza would let you tap her too. A "tap N including self" cost can lift this later; noted for the pool.

use crate::basics::{CardType, Color};
use crate::cards::helpers::instant_or_sorcery;
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, Cost, CostComponent, EventPattern};
use crate::effects::value::ValueExpr;
use crate::effects::{Effect, EffectTarget};
use crate::subtypes::{CreatureType, Supertype};

/// grp id (per-set ids live near their cards).
pub const AZIZA_MAGE_TOWER_CAPTAIN: u32 = 432;

/// "you may tap three untapped creatures you control. If you do, copy that spell" — a SpellCast(I/S)
/// trigger paying a tap-three cost during resolution to copy the triggering spell (new targets).
fn copy_via_tap() -> Ability {
    Ability::Triggered {
        event: EventPattern::SpellCast(instant_or_sorcery()),
        condition: None,
        intervening_if: false,
        effect: Effect::MayPayCost {
            cost: Cost { mana: None, components: vec![CostComponent::TapCreatures(3)] },
            then: Box::new(Effect::CopySpellOnStack {
                what: EffectTarget::Triggering,
                count: ValueExpr::Fixed(1),
                choose_new_targets: true,
            }),
        },
    }
}

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        AZIZA_MAGE_TOWER_CAPTAIN,
        "Aziza, Mage Tower Captain",
        &[CreatureType::Djinn, CreatureType::Sorcerer],
        Color::Red,
        mana_cost(0, &[(Color::Red, 1), (Color::White, 1)]),
        2,
        2,
        vec![copy_via_tap()],
    );
    def.chars.supertypes = vec![Supertype::Legendary];
    def.chars.colors = vec![Color::Red, Color::White];
    def.text = "Whenever you cast an instant or sorcery spell, you may tap three untapped creatures you control. If you do, copy that spell. You may choose new targets for the copy.".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, CastVariant, ConfirmKind, DecisionRequest, DecisionResponse, PlayerView};
    use crate::basics::{Phase, Target, Zone};
    use crate::cards::{grp, starter_db};
    use crate::ids::PlayerId;
    use crate::priority::Engine;
    use crate::state::GameState;
    use crate::stack::StackObjectKind;
    use std::sync::Arc;

    fn db_with_card() -> CardDb {
        let mut db = starter_db();
        register(&mut db);
        db
    }

    #[test]
    fn aziza_shape() {
        let db = db_with_card();
        let def = db.get(AZIZA_MAGE_TOWER_CAPTAIN).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Creature]);
        assert_eq!(def.chars.supertypes, vec![Supertype::Legendary]);
        assert_eq!(def.chars.colors, vec![Color::Red, Color::White]);
        assert_eq!((def.chars.power, def.chars.toughness), (Some(2), Some(2)));
        assert!(def.fully_implemented);
        assert!(matches!(
            def.abilities[0],
            Ability::Triggered { event: EventPattern::SpellCast(_), .. }
        ));
    }

    /// Targets P1 with the bolt, confirms the tap-three, and re-aims the copy at P1.
    #[derive(Clone)]
    struct AzizaAgent {
        confirm: bool,
    }
    impl Agent for AzizaAgent {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::Confirm { kind: ConfirmKind::MayEffect } => DecisionResponse::Bool(self.confirm),
                DecisionRequest::Confirm { .. } => DecisionResponse::Bool(true),
                // Tap-three cost: pick the first three offered.
                DecisionRequest::SelectCards { from, min, .. } => {
                    let n = (*min as usize).min(from.len());
                    DecisionResponse::Indices((0..n as u32).collect())
                }
                DecisionRequest::ChooseTargets { slots, .. } => {
                    let idx = slots[0]
                        .legal
                        .iter()
                        .position(|t| *t == Target::Player(PlayerId(1)))
                        .unwrap_or(0);
                    DecisionResponse::Pairs(vec![(0, idx as u32)])
                }
                _ => DecisionResponse::Pass,
            }
        }
    }

    fn drive(e: &mut Engine) {
        loop {
            e.run_agenda();
            if e.state.stack.items.is_empty() {
                break;
            }
            e.resolve_top();
        }
    }

    /// P0 has Aziza + `others` extra untapped Bears, a Lightning Bolt + Mountain. Returns engine + bolt.
    fn setup(confirm: bool, others: usize) -> (Engine, crate::ids::ObjId) {
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(db_with_card()));
        {
            let c = state.card_db().get(AZIZA_MAGE_TOWER_CAPTAIN).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield);
        }
        for _ in 0..others {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield);
        }
        let bolt = {
            let c = state.card_db().get(grp::LIGHTNING_BOLT).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Hand)
        };
        {
            let m = state.card_db().get(grp::MOUNTAIN).unwrap().chars.clone();
            state.add_card(PlayerId(0), m, Zone::Battlefield);
        }
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let e = Engine::new(state, vec![Box::new(AzizaAgent { confirm }), Box::new(AzizaAgent { confirm })]);
        (e, bolt)
    }

    /// Confirm + three other creatures available: the bolt is copied once (P1 takes 6) and three Bears
    /// end tapped.
    #[test]
    fn tapping_three_copies_the_spell() {
        let (mut e, bolt) = setup(true, 3);
        let p1 = e.state.player(PlayerId(1)).life;
        e.cast_spell(PlayerId(0), bolt, CastVariant::Normal);
        drive(&mut e);
        assert_eq!(e.state.player(PlayerId(1)).life, p1 - 6, "bolt + one copy = 6");
        let tapped = e
            .state
            .player(PlayerId(0))
            .battlefield
            .iter()
            .filter(|&&o| e.state.object(o).chars.is_creature() && e.state.object(o).status.tapped)
            .count();
        assert_eq!(tapped, 3, "three creatures tapped to pay");
    }

    /// Declining the tap makes no copy — P1 takes just 3.
    #[test]
    fn declining_makes_no_copy() {
        let (mut e, bolt) = setup(false, 3);
        let p1 = e.state.player(PlayerId(1)).life;
        e.cast_spell(PlayerId(0), bolt, CastVariant::Normal);
        drive(&mut e);
        assert_eq!(e.state.player(PlayerId(1)).life, p1 - 3, "declined: no copy");
        assert!(
            !e.state.stack.items.iter().any(|s| matches!(s.kind, StackObjectKind::Spell(_))),
            "no copy on the stack"
        );
    }

    /// Too few other creatures (only two): the cost is unpayable, so no copy even if confirmed.
    #[test]
    fn not_enough_creatures_no_copy() {
        let (mut e, bolt) = setup(true, 2);
        let p1 = e.state.player(PlayerId(1)).life;
        e.cast_spell(PlayerId(0), bolt, CastVariant::Normal);
        drive(&mut e);
        assert_eq!(e.state.player(PlayerId(1)).life, p1 - 3, "unpayable: no copy");
    }
}
