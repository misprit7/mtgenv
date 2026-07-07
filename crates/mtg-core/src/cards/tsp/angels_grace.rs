//! Angel's Grace — `{W}` Instant (first printed TSP; reprinted on the SOS Mystical Archive `soa`).
//!
//! Oracle: "Split second (As long as this spell is on the stack, players can't cast spells or activate
//! abilities that aren't mana abilities.)
//! You can't lose the game this turn and your opponents can't win the game this turn. Until end of
//! turn, damage that would reduce your life total to less than 1 reduces it to 1 instead."
//!
//! **Fully implemented** — three subsystems:
//! - **Split second** (`Keyword::SplitSecond`, CR 702.61): while this spell is on the stack,
//!   `legal_priority_actions` suppresses every non-mana action (casts, activated abilities, land plays).
//! - **Can't lose / opponents can't win** (`Effect::CantLoseThisTurn`): a per-turn flag suppressing the
//!   caster's loss SBAs and blocking opponents from being declared the winner.
//! - **Life floor** (`Effect::SetMinLifeThisTurn { min: 1 }`): `change_life` clamps a reduction so the
//!   caster's life can't drop below 1 this turn (modelled as a general floor — see `change_life`).

use crate::basics::{CardType, Color};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::ability::Keyword;
use crate::effects::value::PlayerRef;
use crate::effects::Effect;

/// grp id (bonus-sheet `soa` cards use the 600+ block).
pub const ANGELS_GRACE: u32 = 662;

pub fn register(db: &mut CardDb) {
    let effect = Effect::Sequence(vec![
        Effect::CantLoseThisTurn { who: PlayerRef::Controller },
        Effect::SetMinLifeThisTurn { who: PlayerRef::Controller, min: 1 },
    ]);
    let mut def = spell(
        ANGELS_GRACE,
        "Angel's Grace",
        CardType::Instant,
        Color::White,
        mana_cost(0, &[(Color::White, 1)]),
        effect,
    )
    .with_text(
        "Split second (As long as this spell is on the stack, players can't cast spells or activate abilities that aren't mana abilities.)\nYou can't lose the game this turn and your opponents can't win the game this turn. Until end of turn, damage that would reduce your life total to less than 1 reduces it to 1 instead.",
    );
    def.chars.keywords = vec![Keyword::SplitSecond];
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, CastVariant, DecisionRequest, DecisionResponse, PlayableAction, PlayerView};
    use crate::basics::{DamageKind, Target, Zone};
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

    #[test]
    fn shape_has_split_second() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(ANGELS_GRACE).unwrap();
        assert!(def.fully_implemented);
        assert!(def.chars.keywords.contains(&Keyword::SplitSecond), "printed split second");
    }

    /// Resolving sets the caster's can't-lose + life floor and every opponent's can't-win.
    #[test]
    fn resolving_sets_the_flags() {
        let mut state = build_game(1, &[&[], &[]]);
        let effect = state.card_db().get(ANGELS_GRACE).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(Passive), Box::new(Passive)]);
        e.resolve_effect(
            &effect,
            &ResolutionCtx { controller: Some(PlayerId(0)), ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        assert!(e.state.player(PlayerId(0)).cant_lose_this_turn, "caster can't lose");
        assert!(e.state.player(PlayerId(1)).cant_win_this_turn, "opponent can't win");
        assert_eq!(e.state.player(PlayerId(0)).min_life_this_turn, Some(1), "life floor 1");
    }

    /// The lead's edge: lethal damage at 1 life with Angel's Grace resolved leaves the caster ALIVE —
    /// the life-floor clamps the total to 1 and the can't-lose flag suppresses the loss SBA.
    #[test]
    fn lethal_at_one_life_survives() {
        let mut state = build_game(1, &[&[], &[]]);
        state.players[0].life = 1;
        let ogre = state.add_card(PlayerId(1), state.card_db().get(grp::HILL_GIANT).unwrap().chars.clone(), Zone::Battlefield);
        let effect = state.card_db().get(ANGELS_GRACE).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(Passive), Box::new(Passive)]);
        e.resolve_effect(
            &effect,
            &ResolutionCtx { controller: Some(PlayerId(0)), ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        // A lethal 5 damage to P0 at 1 life.
        e.apply_damage(Target::Player(PlayerId(0)), 5, ogre, DamageKind::Combat);
        e.run_agenda(); // process state-based actions
        assert_eq!(e.state.player(PlayerId(0)).life, 1, "life clamped to 1");
        assert!(!e.state.player(PlayerId(0)).has_lost, "can't-lose keeps the caster alive");
    }

    /// Split second: while Angel's Grace is on the stack, no player may cast or play a land.
    #[test]
    fn split_second_locks_out_responses() {
        let mut state = build_game(1, &[&[], &[]]);
        let ag = state.add_card(PlayerId(0), state.card_db().get(ANGELS_GRACE).unwrap().chars.clone(), Zone::Hand);
        // P0 mana + a second castable instant to prove it's suppressed while AG is on the stack.
        for _ in 0..2 {
            state.add_card(PlayerId(0), state.card_db().get(grp::PLAINS).unwrap().chars.clone(), Zone::Battlefield);
        }
        let bolt = state.add_card(PlayerId(0), state.card_db().get(ANGELS_GRACE).unwrap().chars.clone(), Zone::Hand);
        state.active_player = PlayerId(0);
        state.phase = crate::basics::Phase::PrecombatMain;
        let mut e = Engine::new(state, vec![Box::new(Passive), Box::new(Passive)]);
        // Before casting: the second Angel's Grace IS castable.
        assert!(e.legal_actions(PlayerId(0)).iter().any(|a| matches!(a, PlayableAction::Cast { spell, .. } if *spell == bolt)), "castable before");
        // Cast the first Angel's Grace → now on the stack.
        e.cast_spell(PlayerId(0), ag, CastVariant::Normal);
        assert!(!e.state.stack.is_empty(), "Angel's Grace on the stack");
        // Split second: no Cast or PlayLand offered to anyone.
        let no_casts = |acts: &[PlayableAction]| acts.iter().all(|a| !matches!(a, PlayableAction::Cast { .. } | PlayableAction::PlayLand { .. }));
        assert!(no_casts(&e.legal_actions(PlayerId(0))), "P0 locked out by split second");
        assert!(no_casts(&e.legal_actions(PlayerId(1))), "P1 locked out by split second");
    }
}
