//! Force of Will — `{3}{U}{U}` Instant (first printed Alliances; reprinted on the SOS Mystical Archive `soa`).
//!
//! Oracle: "You may pay 1 life and exile a blue card from your hand rather than pay this spell's mana
//! cost.
//! Counter target spell."
//!
//! **Fully implemented** — the alternative-cast subsystem (`Ability::AlternativeCast` +
//! `CastVariant::Alternative`): from hand you may pay 1 life + exile a blue card (`PayLife` + `Exile`
//! components) instead of `{3}{U}{U}`. The spell is a hard counter (`Effect::Counter`).

use crate::basics::{CardType, Color, Zone};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::ability::{Ability, Cost, CostComponent};
use crate::effects::target::{CardFilter, SelectSpec, TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};

/// grp id (bonus-sheet `soa` cards use the 600+ block).
pub const FORCE_OF_WILL: u32 = 653;

pub fn register(db: &mut CardDb) {
    // "Counter target spell."
    let effect = Effect::Counter {
        what: EffectTarget::Target(TargetSpec {
            kind: TargetKind::StackObject(CardFilter::Any),
            min: 1,
            max: 1,
            distinct: true,
        }),
    };
    let mut def = spell(
        FORCE_OF_WILL,
        "Force of Will",
        CardType::Instant,
        Color::Blue,
        mana_cost(3, &[(Color::Blue, 2)]),
        effect,
    )
    .with_text(
        "You may pay 1 life and exile a blue card from your hand rather than pay this spell's mana cost.\nCounter target spell.",
    );
    // "You may pay 1 life and exile a blue card from your hand rather than pay this spell's mana cost."
    def.abilities.push(Ability::AlternativeCast {
        cost: Cost {
            mana: None,
            components: vec![
                CostComponent::PayLife(ValueExpr::Fixed(1)),
                CostComponent::Exile(SelectSpec {
                    zone: Zone::Hand,
                    filter: CardFilter::HasColor(Color::Blue),
                    chooser: PlayerRef::Controller,
                    min: ValueExpr::Fixed(1),
                    max: ValueExpr::Fixed(1),
                }),
            ],
        },
    });
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, CastVariant, DecisionRequest, DecisionResponse, PlayableAction, PlayerView};
    use crate::basics::Phase;
    use crate::cards::{build_game, grp};
    use crate::ids::{ObjId, PlayerId};
    use crate::priority::Engine;

    #[derive(Clone)]
    struct TargetFirst;
    impl Agent for TargetFirst {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::ChooseTargets { slots, .. } if !slots.is_empty() && !slots[0].legal.is_empty() => {
                    DecisionResponse::Pairs(vec![(0, 0)])
                }
                _ => DecisionResponse::Pass,
            }
        }
    }

    /// A spell on the stack under P1's control (something for Force of Will to counter).
    fn push_spell(e: &mut Engine, owner: PlayerId, grp: u32) -> ObjId {
        let s = e.state.add_card(owner, e.state.card_db().get(grp).unwrap().chars.clone(), Zone::Stack);
        let sid = e.state.mint_stack();
        e.state.stack.push(crate::stack::StackObject {
            id: sid,
            controller: owner,
            source: Some(s),
            kind: crate::stack::StackObjectKind::Spell(s),
            targets: vec![],
            x: None,
            modes: vec![],
        });
        s
    }

    fn setup() -> (Engine, ObjId, ObjId) {
        let mut state = build_game(1, &[&[], &[]]);
        let fow = state.add_card(PlayerId(0), state.card_db().get(FORCE_OF_WILL).unwrap().chars.clone(), Zone::Hand);
        // Another blue card in hand to exile (Force of Will can't exile itself — it's on the stack).
        let blue = state.add_card(PlayerId(0), state.card_db().get(grp::DIVINATION).unwrap().chars.clone(), Zone::Hand);
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let e = Engine::new(state, vec![Box::new(TargetFirst), Box::new(TargetFirst)]);
        (e, fow, blue)
    }

    #[test]
    fn fow_offers_alternative_with_life_and_a_blue_card_no_mana() {
        let (mut e, fow, _blue) = setup();
        push_spell(&mut e, PlayerId(1), grp::LIGHTNING_BOLT);
        let offered: Vec<CastVariant> = e
            .legal_actions(PlayerId(0))
            .iter()
            .filter_map(|a| match a {
                PlayableAction::Cast { spell, variant } if *spell == fow => Some(*variant),
                _ => None,
            })
            .collect();
        assert!(offered.contains(&CastVariant::Alternative), "alt-cast offered (1 life + a blue card)");
        assert!(!offered.contains(&CastVariant::Normal), "normal cast not affordable (no mana)");
    }

    /// Alt-cast Force of Will: pay 1 life, exile the other blue card, counter the target spell.
    #[test]
    fn fow_pays_life_exiles_blue_and_counters() {
        let (mut e, fow, blue) = setup();
        let bolt = push_spell(&mut e, PlayerId(1), grp::LIGHTNING_BOLT);
        let life_before = e.state.player(PlayerId(0)).life;
        e.cast_spell(PlayerId(0), fow, CastVariant::Alternative);
        assert_eq!(e.state.player(PlayerId(0)).life, life_before - 1, "paid 1 life");
        assert!(e.state.player(PlayerId(0)).exile.contains(&blue), "blue card exiled as the cost");
        e.run_agenda();
        while !e.state.stack.items.is_empty() {
            e.resolve_top();
            e.run_agenda();
        }
        assert!(e.state.player(PlayerId(1)).graveyard.contains(&bolt), "target spell countered");
    }

    /// With NO other blue card in hand, the alt-cast is not offered (Force of Will can't exile itself).
    #[test]
    fn fow_alt_cast_needs_another_blue_card() {
        let mut state = build_game(1, &[&[], &[]]);
        let fow = state.add_card(PlayerId(0), state.card_db().get(FORCE_OF_WILL).unwrap().chars.clone(), Zone::Hand);
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let mut e = Engine::new(state, vec![Box::new(TargetFirst), Box::new(TargetFirst)]);
        push_spell(&mut e, PlayerId(1), grp::LIGHTNING_BOLT);
        let offered = e.legal_actions(PlayerId(0)).iter().any(|a| matches!(a, PlayableAction::Cast { spell, variant: CastVariant::Alternative } if *spell == fow));
        assert!(!offered, "no other blue card → alt-cast unpayable, not offered");
    }
}
