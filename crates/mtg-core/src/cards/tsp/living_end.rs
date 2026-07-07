//! Living End — Sorcery, no mana cost (Suspend 3—`{2}{B}{B}`) (first printed TSP; reprinted on the
//! SOS Mystical Archive `soa`).
//!
//! Oracle: "Suspend 3—{2}{B}{B}. Each player exiles all creature cards from their graveyard, then
//! sacrifices all creatures they control, then puts all cards they exiled this way onto the battlefield."
//!
//! **Fully implemented** — the Suspend subsystem (`Ability::Suspend` + `CastVariant::Suspend`: exile
//! from hand with 3 time counters, a per-upkeep sweep removes one, and at zero the owner casts it free —
//! the suspended cast IS a cast, so cast triggers fire). The effect is a `Sequence`:
//! 1. a `ForEach` exiling every creature card from all graveyards (into the per-resolution exile scratch);
//! 2. `Sacrifice { EachPlayer }` of every creature on the battlefield;
//! 3. `Effect::ReturnExiledThisResolution` — the parked graveyard creatures return under their owners'
//!    control (the exile-park is what protects them from step 2's board wipe).
//!
//! It has no printed mana cost (`chars.mana_cost = None`), so it can only enter the game via Suspend.

use crate::basics::{CardType, Color, Zone};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::ability::Ability;
use crate::effects::target::{CardFilter, SelectSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};

/// grp id (bonus-sheet `soa` cards use the 600+ block).
pub const LIVING_END: u32 = 661;

pub fn register(db: &mut CardDb) {
    let all_creatures = |zone: Zone, chooser: PlayerRef| SelectSpec {
        zone,
        filter: CardFilter::HasCardType(CardType::Creature),
        chooser,
        min: ValueExpr::Fixed(0),
        max: ValueExpr::Fixed(999),
    };
    let effect = Effect::Sequence(vec![
        // 1. Each player exiles all creature cards from their graveyard.
        Effect::ForEach {
            selector: all_creatures(Zone::Graveyard, PlayerRef::EachPlayer),
            body: Box::new(Effect::Exile { what: EffectTarget::Each }),
        },
        // 2. then sacrifices all creatures they control.
        Effect::Sacrifice {
            who: PlayerRef::EachPlayer,
            what: all_creatures(Zone::Battlefield, PlayerRef::Controller),
        },
        // 3. then puts all cards they exiled this way onto the battlefield (under their owners).
        Effect::ReturnExiledThisResolution,
    ]);
    // Living End has no printed mana cost — built with a placeholder, then cleared so only Suspend
    // (and other alternative casts) can bring it into the game.
    let mut def = spell(LIVING_END, "Living End", CardType::Sorcery, Color::Black, mana_cost(0, &[]), effect)
        .with_text(
            "Suspend 3—{2}{B}{B}\nEach player exiles all creature cards from their graveyard, then sacrifices all creatures they control, then puts all cards they exiled this way onto the battlefield.",
        );
    def.chars.mana_cost = None;
    def.chars.colors = vec![Color::Black];
    def.abilities.push(Ability::Suspend { n: 3, cost: mana_cost(2, &[(Color::Black, 2)]) });
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, CastVariant, DecisionRequest, DecisionResponse, PlayerView};
    use crate::basics::{CounterKind, Phase};
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
    fn shape_no_mana_cost_suspend_3() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(LIVING_END).unwrap();
        assert!(def.fully_implemented);
        assert!(def.chars.mana_cost.is_none(), "no printed mana cost");
        let (n, cost) = def.abilities.iter().find_map(|a| match a {
            Ability::Suspend { n, cost } => Some((*n, cost.clone())),
            _ => None,
        }).expect("has Suspend");
        assert_eq!(n, 3);
        assert_eq!(cost.generic, 2, "{{2}} generic");
        assert_eq!(cost.colored.get(&Color::Black).copied().unwrap_or(0), 2, "{{B}}{{B}}");
    }

    /// Suspending exiles the card from hand with 3 time counters and pays {2}{B}{B} (taps 4 Swamps).
    #[test]
    fn suspend_exiles_with_time_counters_and_pays() {
        let mut state = build_game(1, &[&[], &[]]);
        let le = state.add_card(PlayerId(0), state.card_db().get(LIVING_END).unwrap().chars.clone(), Zone::Hand);
        let swamps: Vec<_> = (0..4)
            .map(|_| state.add_card(PlayerId(0), state.card_db().get(grp::SWAMP).unwrap().chars.clone(), Zone::Battlefield))
            .collect();
        let mut e = Engine::new(state, vec![Box::new(Passive), Box::new(Passive)]);
        e.cast_spell(PlayerId(0), le, CastVariant::Suspend);
        assert_eq!(e.state.object(le).zone, Zone::Exile, "suspended → exile");
        assert_eq!(e.state.object(le).counters.get(&CounterKind::Time), 3, "3 time counters");
        assert!(swamps.iter().all(|&s| e.state.object(s).status.tapped), "the 4 Swamps paid {{2}}{{B}}{{B}}");
    }

    /// End-to-end at the upkeep: a Living End suspended with its LAST time counter is swept to zero,
    /// cast for free, and resolves — graveyard creatures return, battlefield creatures are wiped.
    #[test]
    fn last_counter_casts_and_resolves_the_swap() {
        let mut state = build_game(1, &[&[], &[]]);
        // P0's Living End suspended with 1 time counter left.
        let le = state.add_card(PlayerId(0), state.card_db().get(LIVING_END).unwrap().chars.clone(), Zone::Exile);
        state.objects.get_mut(&le).unwrap().counters.counts.insert(CounterKind::Time, 1);
        // A creature card in P0's graveyard (should return) and a creature on P1's board (should die).
        let gy_bear = state.add_card(PlayerId(0), state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Graveyard);
        let board_giant = state.add_card(PlayerId(1), state.card_db().get(grp::HILL_GIANT).unwrap().chars.clone(), Zone::Battlefield);
        state.active_player = PlayerId(0);
        let mut e = Engine::new(state, vec![Box::new(Passive), Box::new(Passive)]);
        e.run_step(Phase::Upkeep);
        // Living End was cast (left exile) and resolved.
        assert_ne!(e.state.object(le).zone, Zone::Exile, "the suspended card was cast");
        assert_eq!(e.state.object(gy_bear).zone, Zone::Battlefield, "graveyard creature returned to the battlefield");
        assert_ne!(e.state.object(board_giant).zone, Zone::Battlefield, "battlefield creature was sacrificed");
    }

    /// The body in isolation (via `resolve_effect`): exile gy creatures → sac board → return exiled.
    #[test]
    fn body_swaps_graveyard_and_battlefield_creatures() {
        let mut state = build_game(1, &[&[], &[]]);
        let gy_a = state.add_card(PlayerId(0), state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Graveyard);
        let gy_b = state.add_card(PlayerId(1), state.card_db().get(grp::HILL_GIANT).unwrap().chars.clone(), Zone::Graveyard);
        let board = state.add_card(PlayerId(0), state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Battlefield);
        let effect = state.card_db().get(LIVING_END).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(Passive), Box::new(Passive)]);
        e.resolve_effect(
            &effect,
            &ResolutionCtx { controller: Some(PlayerId(0)), ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        assert_eq!(e.state.object(gy_a).zone, Zone::Battlefield, "P0's gy creature returned");
        assert_eq!(e.state.object(gy_b).zone, Zone::Battlefield, "P1's gy creature returned");
        assert_ne!(e.state.object(board).zone, Zone::Battlefield, "the on-board creature was sacrificed");
    }
}
