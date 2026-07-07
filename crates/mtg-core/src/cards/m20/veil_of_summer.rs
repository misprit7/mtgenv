//! Veil of Summer — `{G}` Instant (first printed M20; reprinted on the SOS Mystical Archive `soa`).
//!
//! Oracle: "Draw a card if an opponent has cast a blue or black spell this turn. Spells you control
//! can't be countered this turn. You and permanents you control gain hexproof from blue and from black
//! until end of turn. (You and they can't be the targets of blue or black spells or abilities your
//! opponents control.)"
//!
//! **Fully implemented** — a `Sequence`:
//! 1. a `Conditional` draw gated on `Condition::OpponentCastColorThisTurn([Blue, Black])` (the per-turn
//!    per-player cast-colour tracker);
//! 2. `SetSpellsUncounterableThisTurn` (read at the counter choke);
//! 3. `GrantPlayerHexproofFromThisTurn` (the "you" half — a player-level hexproof-from consulted in the
//!    player-target path) plus a `ForEach` `Becomes` granting each permanent you control
//!    `HexproofFromColor(Blue/Black)` — riding the protection/hexproof-from-colour subsystem.

use crate::basics::{CardType, Color, Zone};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::ability::StaticContribution;
use crate::effects::condition::{Condition, Duration};
use crate::effects::target::{CardFilter, SelectSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};

/// grp id (bonus-sheet `soa` cards use the 600+ block).
pub const VEIL_OF_SUMMER: u32 = 664;

pub fn register(db: &mut CardDb) {
    let blue_black = || vec![Color::Blue, Color::Black];
    let effect = Effect::Sequence(vec![
        // "Draw a card if an opponent has cast a blue or black spell this turn."
        Effect::Conditional {
            cond: Condition::OpponentCastColorThisTurn(blue_black()),
            then: Box::new(Effect::Draw { who: PlayerRef::Controller, count: ValueExpr::Fixed(1) }),
            otherwise: None,
        },
        // "Spells you control can't be countered this turn."
        Effect::SetSpellsUncounterableThisTurn { who: PlayerRef::Controller },
        // "You … gain hexproof from blue and from black until end of turn." (the player half)
        Effect::GrantPlayerHexproofFromThisTurn { who: PlayerRef::Controller, colors: blue_black() },
        // "… and permanents you control gain hexproof from blue and from black until end of turn."
        Effect::ForEach {
            selector: SelectSpec {
                zone: Zone::Battlefield,
                filter: CardFilter::ControlledBy(PlayerRef::Controller),
                chooser: PlayerRef::Controller,
                min: ValueExpr::Fixed(0),
                max: ValueExpr::Fixed(999),
            },
            body: Box::new(Effect::Becomes {
                what: EffectTarget::Each,
                contributions: vec![
                    StaticContribution::HexproofFromColor(Color::Blue),
                    StaticContribution::HexproofFromColor(Color::Black),
                ],
                base_pt: None,
                duration: Duration::UntilEndOfTurn,
            }),
        },
    ]);
    let def = spell(
        VEIL_OF_SUMMER,
        "Veil of Summer",
        CardType::Instant,
        Color::Green,
        mana_cost(0, &[(Color::Green, 1)]),
        effect,
    )
    .with_text(
        "Draw a card if an opponent has cast a blue or black spell this turn. Spells you control can't be countered this turn. You and permanents you control gain hexproof from blue and from black until end of turn.",
    );
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView};
    use crate::basics::Zone;
    use crate::cards::{build_game, grp};
    use crate::effects::action::{ResolutionCtx, WbReason};
    use crate::ids::{PlayerId, StackId};
    use crate::priority::Engine;

    #[derive(Clone)]
    struct Passive;
    impl Agent for Passive {
        fn decide(&mut self, _v: &PlayerView, _r: &DecisionRequest) -> DecisionResponse {
            DecisionResponse::Pass
        }
    }

    fn resolve(state: crate::state::GameState) -> Engine {
        let effect = state.card_db().get(VEIL_OF_SUMMER).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(Passive), Box::new(Passive)]);
        e.resolve_effect(
            &effect,
            &ResolutionCtx { controller: Some(PlayerId(0)), ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        e
    }

    #[test]
    fn shape_is_a_sequence() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(VEIL_OF_SUMMER).unwrap();
        assert!(def.fully_implemented);
        assert!(matches!(def.spell_effect().unwrap(), Effect::Sequence(v) if v.len() == 4));
    }

    /// Sets the caster's uncounterable + player-level hexproof-from, and grants each of the caster's
    /// permanents hexproof from blue and black.
    #[test]
    fn grants_uncounterable_and_hexproof_from_blue_black() {
        let mut state = build_game(1, &[&[], &[]]);
        let bear = state.add_card(PlayerId(0), state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Battlefield);
        let e = resolve(state);
        assert!(e.state.player(PlayerId(0)).spells_uncounterable_this_turn, "spells can't be countered");
        let hp = &e.state.player(PlayerId(0)).hexproof_from_this_turn;
        assert!(hp.contains(&Color::Blue) && hp.contains(&Color::Black), "player hexproof from U/B");
        let cc = e.state.computed(bear);
        assert!(cc.hexproof_from.contains(&Color::Blue) && cc.hexproof_from.contains(&Color::Black), "permanent hexproof from U/B");
    }

    /// The counter-choke seam: with "spells you control can't be countered this turn" set, a Counter
    /// effect leaves the caster's spell on the stack (CR 701.5f).
    #[test]
    fn uncounterable_leaves_the_spell_on_the_stack() {
        use crate::basics::Target;
        use crate::stack::{StackObject, StackObjectKind};
        let mut state = build_game(1, &[&[], &[]]);
        let victim = state.add_card(PlayerId(0), state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Stack);
        let sid = StackId(7);
        state.stack.push(StackObject { id: sid, controller: PlayerId(0), source: None, kind: StackObjectKind::Spell(victim), targets: vec![], x: None, modes: Vec::new() });
        // P0's Veil of Summer already resolved this turn.
        state.player_mut(PlayerId(0)).spells_uncounterable_this_turn = true;
        let mut e = Engine::new(state, vec![Box::new(Passive), Box::new(Passive)]);
        // P1 tries to counter P0's spell.
        e.resolve_effect(
            &Effect::Counter { what: EffectTarget::ChosenIndex(0) },
            &ResolutionCtx { controller: Some(PlayerId(1)), chosen_targets: vec![Target::Stack(sid)], ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        assert!(e.state.stack.items.iter().any(|s| s.id == sid), "uncounterable spell stayed on the stack");
    }

    /// The conditional draw: fires only if an opponent cast a blue or black spell this turn.
    #[test]
    fn draws_only_when_an_opponent_cast_blue_or_black() {
        // No opponent blue/black cast → no draw.
        let mut state = build_game(1, &[&[grp::FOREST], &[]]);
        let hand_before = state.player(PlayerId(0)).hand.len();
        let e = resolve(state);
        assert_eq!(e.state.player(PlayerId(0)).hand.len(), hand_before, "no opponent U/B spell → no draw");

        // Opponent (P1) cast a black spell this turn → draw one.
        let mut state = build_game(1, &[&[grp::FOREST], &[]]);
        state.player_mut(PlayerId(1)).colors_cast_this_turn = vec![Color::Black];
        let hand_before = state.player(PlayerId(0)).hand.len();
        let e = resolve(state);
        assert_eq!(e.state.player(PlayerId(0)).hand.len(), hand_before + 1, "opponent cast black → draw one");
    }
}
