//! Ark of Hunger — `{2}{R}{W}` Artifact (first printed SOS).
//!
//! Oracle: "Whenever one or more cards leave your graveyard, this artifact deals 1 damage to each
//! opponent and you gain 1 life.
//! {T}: Mill a card. You may play that card this turn."
//!
//! **Fully implemented** — a red-white artifact ({R}{W} pips colour it, CR 202.2) with:
//!  - a `CardsLeaveYourGraveyard` trigger draining each opponent for 1 and gaining you 1 life; and
//!  - a `{T}` mill-then-play ability (`Effect::MillThenPlay`) — mills the top card and lets you play
//!    that specific card from the graveyard until end of turn (the graveyard analogue of impulse-play).

use crate::basics::{Color, DamageKind};
use crate::cards::{artifact, mana_cost, CardDb};
use crate::effects::ability::{Ability, Cost, CostComponent, EventPattern, Timing};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget, PlayWindow};

/// grp id (per-set ids live near their cards).
pub const ARK_OF_HUNGER: u32 = 445;

pub fn register(db: &mut CardDb) {
    let mut def = artifact(
        ARK_OF_HUNGER,
        "Ark of Hunger",
        mana_cost(2, &[(Color::Red, 1), (Color::White, 1)]),
        vec![
            // "Whenever one or more cards leave your graveyard, this artifact deals 1 damage to each
            // opponent and you gain 1 life." (batched once per graveyard-shrink, CR — Lorehold gate.)
            Ability::Triggered {
                event: EventPattern::CardsLeaveYourGraveyard,
                condition: None,
                intervening_if: false,
                effect: Effect::Sequence(vec![
                    Effect::DealDamage {
                        amount: ValueExpr::Fixed(1),
                        to: EffectTarget::Player(PlayerRef::EachOpponent),
                        kind: DamageKind::Noncombat,
                    },
                    Effect::GainLife { who: PlayerRef::Controller, amount: ValueExpr::Fixed(1) },
                ]),
            },
            // "{T}: Mill a card. You may play that card this turn."
            Ability::Activated {
                cost: Cost { mana: None, components: vec![CostComponent::TapSelf] },
                effect: Effect::MillThenPlay {
                    who: PlayerRef::Controller,
                    window: PlayWindow::ThisTurn,
                },
                timing: Timing::Instant,
                restriction: None,
                is_mana: false,
            },
        ],
    );
    def.chars.colors = vec![Color::Red, Color::White]; // {2}{R}{W} → red-white artifact (CR 202.2)
    def.text = "Whenever one or more cards leave your graveyard, this artifact deals 1 damage to each opponent and you gain 1 life.\n{T}: Mill a card. You may play that card this turn.".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{PlayableAction, RandomAgent};
    use crate::basics::{Phase, Zone};
    use crate::cards::{build_game, grp};
    use crate::effects::action::{ResolutionCtx, WbReason};
    use crate::ids::{PlayerId, StackId};
    use crate::priority::Engine;

    #[test]
    fn ark_of_hunger_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(ARK_OF_HUNGER).unwrap();
        assert_eq!(def.chars.colors, vec![Color::Red, Color::White]);
        assert!(matches!(
            &def.abilities[0],
            Ability::Triggered { event: EventPattern::CardsLeaveYourGraveyard, .. }
        ));
        assert!(matches!(&def.abilities[1], Ability::Activated { is_mana: false, .. }));
        assert!(def.fully_implemented);
    }

    /// Core cap: resolving the `{T}` ability mills the top card into the graveyard and grants play
    /// permission on THAT card — a milled land is then offered as a `PlayLand` from the graveyard, and
    /// playing it puts it onto the battlefield.
    #[test]
    fn mill_then_play_a_land_from_graveyard() {
        let mut state = build_game(1, &[&[], &[]]);
        let ark = state.add_card(PlayerId(0), state.card_db().get(ARK_OF_HUNGER).unwrap().chars.clone(), Zone::Battlefield);
        // Put a Forest on TOP of the library (library.last() is the top).
        let forest = state.add_card(PlayerId(0), state.card_db().get(grp::FOREST).unwrap().chars.clone(), Zone::Library);
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let mill = match &state.card_db().get(ARK_OF_HUNGER).unwrap().abilities[1] {
            Ability::Activated { effect, .. } => effect.clone(),
            other => panic!("expected the mill ability, got {other:?}"),
        };
        let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);

        e.resolve_effect(
            &mill,
            &ResolutionCtx { controller: Some(PlayerId(0)), source: Some(ark), ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        assert_eq!(e.state.object(forest).zone, Zone::Graveyard, "the milled land is in the graveyard");
        assert!(e.state.object(forest).playable_from_graveyard, "it was granted play permission");

        // It's offered as a land play straight from the graveyard.
        let offered = e.legal_actions(PlayerId(0)).iter().any(|a| matches!(a, PlayableAction::PlayLand { card } if *card == forest));
        assert!(offered, "the milled land is offered to be played from the graveyard");
        e.play_land(PlayerId(0), forest);
        assert_eq!(e.state.object(forest).zone, Zone::Battlefield, "playing it moves it to the battlefield");
    }

    /// Once the turn passes, the play window closes — a milled card left unplayed is no longer offered.
    #[test]
    fn play_window_closes_next_turn() {
        let mut state = build_game(1, &[&[], &[]]);
        let ark = state.add_card(PlayerId(0), state.card_db().get(ARK_OF_HUNGER).unwrap().chars.clone(), Zone::Battlefield);
        let forest = state.add_card(PlayerId(0), state.card_db().get(grp::FOREST).unwrap().chars.clone(), Zone::Library);
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let mill = match &state.card_db().get(ARK_OF_HUNGER).unwrap().abilities[1] {
            Ability::Activated { effect, .. } => effect.clone(),
            _ => unreachable!(),
        };
        let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
        e.resolve_effect(
            &mill,
            &ResolutionCtx { controller: Some(PlayerId(0)), source: Some(ark), ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        // Simulate reaching a later turn: the window was "this turn" (turn_number at mill time).
        e.state.turn_number += 1;
        let offered = e.legal_actions(PlayerId(0)).iter().any(|a| matches!(a, PlayableAction::PlayLand { card } if *card == forest));
        assert!(!offered, "the window closed — the milled land is no longer playable from the graveyard");
    }

    /// The leave-graveyard trigger: when a card leaves your graveyard, Ark drains each opponent for 1
    /// and you gain 1 life. Driven by resolving a `MoveZone` off the graveyard (the engine broadcasts
    /// `LeftGraveyard`, queuing the trigger).
    #[test]
    fn leave_graveyard_drains_and_gains() {
        use crate::basics::{Target, ZoneDest, ZonePos};
        use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
        let mut state = build_game(1, &[&[], &[]]);
        state.add_card(PlayerId(0), state.card_db().get(ARK_OF_HUNGER).unwrap().chars.clone(), Zone::Battlefield);
        // A card sitting in your graveyard, to be moved out.
        let gy_card = state.add_card(PlayerId(0), state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Graveyard);
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let opp_life = state.player(PlayerId(1)).life;
        let my_life = state.player(PlayerId(0)).life;
        let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);

        // Return the graveyard card to hand → the graveyard shrinks → "cards leave your graveyard".
        let mv = Effect::MoveZone {
            what: EffectTarget::Target(TargetSpec { kind: TargetKind::CardInZone { zone: Zone::Graveyard, filter: CardFilter::Any }, min: 1, max: 1, distinct: true }),
            to: ZoneDest { zone: Zone::Hand, pos: ZonePos::Any },
            tapped: false,
        };
        e.resolve_effect(&mv, &ResolutionCtx { controller: Some(PlayerId(0)), chosen_targets: vec![Target::Object(gy_card)], ..Default::default() }, WbReason::Resolve(StackId(1)));
        e.run_agenda(); // put the trigger on the stack
        while !e.state.stack.items.is_empty() {
            e.resolve_top();
            e.run_agenda();
        }
        assert_eq!(e.state.player(PlayerId(1)).life, opp_life - 1, "each opponent took 1 damage");
        assert_eq!(e.state.player(PlayerId(0)).life, my_life + 1, "you gained 1 life");
    }
}
