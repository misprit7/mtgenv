//! Cyclonic Rift — `{1}{U}` Instant (first printed RTR; reprinted on the SOS Mystical Archive `soa`).
//!
//! Oracle: "Return target nonland permanent you don't control to its owner's hand.
//! Overload {6}{U}."
//!
//! **Fully implemented** — normal cast returns one target nonland permanent you don't control to hand;
//! **overload** (`Ability::Overload { {6}{U} }`) casts with no targets and the engine broadens the
//! effect to "each nonland permanent you don't control" via `overload_rewrite` (CR 702.96b).

use crate::basics::{CardType, Color, Zone, ZoneDest, ZonePos};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::ability::Ability;
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::PlayerRef;
use crate::effects::{Effect, EffectTarget};

/// grp id (bonus-sheet `soa` cards use the 600+ block).
pub const CYCLONIC_RIFT: u32 = 646;

pub fn register(db: &mut CardDb) {
    let effect = Effect::MoveZone {
        what: EffectTarget::Target(TargetSpec {
            kind: TargetKind::Permanent(CardFilter::All(vec![
                CardFilter::Not(Box::new(CardFilter::HasCardType(CardType::Land))),
                CardFilter::ControlledBy(PlayerRef::Opponent),
            ])),
            min: 1,
            max: 1,
            distinct: true,
        }),
        to: ZoneDest { zone: Zone::Hand, pos: ZonePos::Any },
        tapped: false,
    };
    let mut def = spell(
        CYCLONIC_RIFT,
        "Cyclonic Rift",
        CardType::Instant,
        Color::Blue,
        mana_cost(1, &[(Color::Blue, 1)]),
        effect,
    )
    .with_text("Return target nonland permanent you don't control to its owner's hand.\nOverload {6}{U} (You may cast this spell for its overload cost. If you do, change \"target\" in its text to \"each.\")");
    def.abilities.push(Ability::Overload { cost: mana_cost(6, &[(Color::Blue, 1)]) });
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
    use crate::stack::StackObjectKind;

    /// Picks the first legal target for the single slot; else passes.
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

    #[test]
    fn cyclonic_rift_has_overload() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(CYCLONIC_RIFT).unwrap();
        assert!(def.fully_implemented);
        assert!(def.abilities.iter().any(|a| matches!(a, Ability::Overload { .. })), "overload ability present");
    }

    fn nonland(state: &mut crate::state::GameState, p: PlayerId) -> ObjId {
        state.add_card(p, state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Battlefield)
    }

    /// Normal cast: bounce ONE target nonland you don't control; your own permanent stays.
    #[test]
    fn normal_bounces_one_opponent_permanent() {
        let mut state = build_game(1, &[&[], &[]]);
        let rift = state.add_card(PlayerId(0), state.card_db().get(CYCLONIC_RIFT).unwrap().chars.clone(), Zone::Hand);
        for _ in 0..2 { state.add_card(PlayerId(0), state.card_db().get(grp::ISLAND).unwrap().chars.clone(), Zone::Battlefield); }
        let theirs = nonland(&mut state, PlayerId(1));
        let mine = nonland(&mut state, PlayerId(0));
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let mut e = Engine::new(state, vec![Box::new(TargetFirst), Box::new(TargetFirst)]);
        e.cast_spell(PlayerId(0), rift, CastVariant::Normal);
        e.run_agenda();
        while !e.state.stack.items.is_empty() { e.resolve_top(); e.run_agenda(); }
        assert!(e.state.player(PlayerId(1)).hand.contains(&theirs), "opponent's permanent bounced");
        assert!(e.state.player(PlayerId(0)).battlefield.contains(&mine), "your own permanent stays");
    }

    /// Overload: NO target chosen (the stack spell has empty targets — no Targeted events, per CR
    /// 702.96b), and EACH nonland you don't control is bounced while your own permanents stay.
    #[test]
    fn overload_bounces_each_opponent_permanent_no_targets() {
        let mut state = build_game(1, &[&[], &[]]);
        let rift = state.add_card(PlayerId(0), state.card_db().get(CYCLONIC_RIFT).unwrap().chars.clone(), Zone::Hand);
        for _ in 0..7 { state.add_card(PlayerId(0), state.card_db().get(grp::ISLAND).unwrap().chars.clone(), Zone::Battlefield); }
        let opp1 = nonland(&mut state, PlayerId(1));
        let opp2 = nonland(&mut state, PlayerId(1));
        let mine = nonland(&mut state, PlayerId(0));
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let mut e = Engine::new(state, vec![Box::new(TargetFirst), Box::new(TargetFirst)]);
        e.cast_spell(PlayerId(0), rift, CastVariant::Overload);
        // The overloaded spell is on the stack with NO chosen targets.
        let on_stack = e.state.stack.items.iter().find(|s| matches!(s.kind, StackObjectKind::Spell(c) if c == rift)).unwrap();
        assert!(on_stack.targets.is_empty(), "overloaded cast chose no targets (CR 702.96b)");
        let _ = Target::Object(rift);
        e.run_agenda();
        while !e.state.stack.items.is_empty() { e.resolve_top(); e.run_agenda(); }
        assert!(e.state.player(PlayerId(1)).hand.contains(&opp1) && e.state.player(PlayerId(1)).hand.contains(&opp2), "each opponent nonland bounced");
        assert!(e.state.player(PlayerId(0)).battlefield.contains(&mine), "your own permanents stay");
    }
}
