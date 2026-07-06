//! Daze — `{1}{U}` Instant (first printed Nemesis; reprinted on the SOS Mystical Archive `soa`).
//!
//! Oracle: "You may return an Island you control to its owner's hand rather than pay this spell's mana
//! cost.
//! Counter target spell unless its controller pays {1}."
//!
//! **Fully implemented** — the alternative-cast subsystem (`Ability::AlternativeCast` +
//! `CastVariant::Alternative` + `CostComponent::ReturnToHand`): from hand you may pay "return an Island"
//! instead of `{1}{U}`. The spell is a soft counter (`CounterUnlessPay`).

use crate::basics::{CardType, Color};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::ability::{Ability, Cost, CostComponent};
use crate::effects::target::{CardFilter, SelectSpec, TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};
use crate::subtypes::{LandType, Subtype};

/// grp id (bonus-sheet `soa` cards use the 600+ block).
pub const DAZE: u32 = 652;

pub fn register(db: &mut CardDb) {
    // "Counter target spell unless its controller pays {1}."
    let effect = Effect::CounterUnlessPay {
        what: EffectTarget::Target(TargetSpec {
            kind: TargetKind::StackObject(CardFilter::Any),
            min: 1,
            max: 1,
            distinct: true,
        }),
        cost: Cost { mana: Some(mana_cost(1, &[])), components: Vec::new() },
    };
    let mut def = spell(
        DAZE,
        "Daze",
        CardType::Instant,
        Color::Blue,
        mana_cost(1, &[(Color::Blue, 1)]),
        effect,
    )
    .with_text(
        "You may return an Island you control to its owner's hand rather than pay this spell's mana cost.\nCounter target spell unless its controller pays {1}.",
    );
    // "You may return an Island you control to its owner's hand rather than pay this spell's mana cost."
    def.abilities.push(Ability::AlternativeCast {
        cost: Cost {
            mana: None,
            components: vec![CostComponent::ReturnToHand(SelectSpec {
                zone: crate::basics::Zone::Battlefield,
                filter: CardFilter::HasSubtype(Subtype::Land(LandType::Island)),
                chooser: PlayerRef::Controller,
                min: ValueExpr::Fixed(1),
                max: ValueExpr::Fixed(1),
            })],
        },
    });
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, CastVariant, DecisionRequest, DecisionResponse, PlayableAction, PlayerView};
    use crate::basics::{Phase, Zone};
    use crate::cards::{build_game, grp};
    use crate::ids::{ObjId, PlayerId};
    use crate::priority::Engine;

    /// Targets the first legal option; never pays a soft-counter's demand (returns Pass to a Confirm).
    #[derive(Clone)]
    struct NoPay;
    impl Agent for NoPay {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::ChooseTargets { slots, .. } if !slots.is_empty() && !slots[0].legal.is_empty() => {
                    DecisionResponse::Pairs(vec![(0, 0)])
                }
                _ => DecisionResponse::Pass,
            }
        }
    }

    fn setup() -> (Engine, ObjId, ObjId) {
        let mut state = build_game(1, &[&[], &[]]);
        let daze = state.add_card(PlayerId(0), state.card_db().get(DAZE).unwrap().chars.clone(), Zone::Hand);
        // P0 controls an Island (the alt-cost fuel) but no other mana.
        let island = state.add_card(PlayerId(0), state.card_db().get(grp::ISLAND).unwrap().chars.clone(), Zone::Battlefield);
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let e = Engine::new(state, vec![Box::new(NoPay), Box::new(NoPay)]);
        (e, daze, island)
    }

    /// Push a spell onto the stack under `owner`'s control (something for Daze to counter).
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

    #[test]
    fn daze_offers_alternative_with_only_an_island() {
        let (mut e, daze, _island) = setup();
        // A spell on the stack to counter (so `card_castable_targets` is satisfied).
        push_spell(&mut e, PlayerId(1), grp::LIGHTNING_BOLT);
        let offered: Vec<CastVariant> = e
            .legal_actions(PlayerId(0))
            .iter()
            .filter_map(|a| match a {
                PlayableAction::Cast { spell, variant } if *spell == daze => Some(*variant),
                _ => None,
            })
            .collect();
        assert!(offered.contains(&CastVariant::Alternative), "alt-cast offered with an Island, no mana");
        assert!(!offered.contains(&CastVariant::Normal), "normal cast not affordable (no mana)");
    }

    /// Alt-cast Daze: return the Island (→ hand), and counter the target spell whose controller can't
    /// pay the {1} (they have no mana). The countered spell hits its owner's graveyard.
    #[test]
    fn daze_returns_island_and_counters_unpaid_spell() {
        let (mut e, daze, island) = setup();
        // Opponent casts a spell we'll Daze. Give them no mana so they can't pay the {1}.
        let bolt = push_spell(&mut e, PlayerId(1), grp::LIGHTNING_BOLT);
        e.cast_spell(PlayerId(0), daze, CastVariant::Alternative);
        // Island returned to hand as the cost (CR 118.9).
        assert!(e.state.player(PlayerId(0)).hand.contains(&island), "Island returned to hand");
        assert!(!e.state.player(PlayerId(0)).battlefield.contains(&island), "Island left the battlefield");
        e.run_agenda();
        while !e.state.stack.items.is_empty() {
            e.resolve_top();
            e.run_agenda();
        }
        assert!(e.state.player(PlayerId(1)).graveyard.contains(&bolt), "unpaid spell countered");
    }
}
