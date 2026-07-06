//! Ral Zarek, Guest Lecturer — `{1}{B}{B}` Legendary Planeswalker — Ral (first printed SOS).
//!
//! Oracle: "+1: Surveil 2.
//!          −1: Any number of target players each discard a card.
//!          −2: Return target creature card with mana value 3 or less from your graveyard to the
//!              battlefield.
//!          −7: Flip five coins. Target opponent skips their next X turns, where X is the number of
//!              coins that came up heads."
//! Starting loyalty 3.
//!
//! **Fully implemented.** All four loyalty abilities:
//! - `+1` Surveil 2 (`Effect::Surveil`).
//! - `−1` "any number of target players each discard a card" — `ForEachTarget` over a **player** slot
//!   (min 0, so "any number") whose body discards `PlayerRef::Each` (the per-iteration target player).
//! - `−2` sorcery-timed reanimation of a mana-value ≤ 3 creature card from your graveyard
//!   (`MoveZone` from `CardInZone{Graveyard}` filtered by `ManaValue{max:3}`).
//! - `−7` "Flip five coins. Target opponent skips their next X turns, where X = heads." — the new
//!   [`Effect::FlipCoinsSkipNextTurns`] (flips on the seeded engine RNG, adds the heads count to the
//!   opponent's `Player.skip_next_turns`; `advance_turn` consumes those, CR 720). `who` =
//!   `PlayerRef::Opponent` is the "target opponent" in the 2-player scope.

use crate::basics::{CardType, Color, Zone, ZoneDest, ZonePos};
use crate::cards::{loyalty_ability, mana_cost, planeswalker, CardDb};
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};
use crate::subtypes::PlaneswalkerType;

/// grp id (per-set ids live near their cards).
pub const RAL_ZAREK_GUEST_LECTURER: u32 = 365;

pub fn register(db: &mut CardDb) {
    // +1: Surveil 2.
    let plus_one = loyalty_ability(1, Effect::Surveil { count: ValueExpr::Fixed(2) });
    // −1: Any number of target players each discard a card. (2-player format → up to two targets.)
    let minus_one = loyalty_ability(
        -1,
        Effect::ForEachTarget {
            slot: TargetSpec {
                kind: TargetKind::Player(crate::effects::target::PlayerFilter::Any),
                min: 0,
                max: 2,
                distinct: true,
            },
            body: Box::new(Effect::Discard {
                who: PlayerRef::Each,
                count: ValueExpr::Fixed(1),
            }),
        },
    );
    // −2: Return target creature card with mana value 3 or less from your graveyard to the battlefield.
    let minus_two = loyalty_ability(
        -2,
        Effect::MoveZone {
            what: EffectTarget::Target(TargetSpec {
                kind: TargetKind::CardInZone {
                    zone: Zone::Graveyard,
                    filter: CardFilter::All(vec![
                        CardFilter::ControlledBy(PlayerRef::Controller),
                        CardFilter::HasCardType(CardType::Creature),
                        CardFilter::ManaValue { min: None, max: Some(3) },
                    ]),
                },
                min: 1,
                max: 1,
                distinct: true,
            }),
            to: ZoneDest { zone: Zone::Battlefield, pos: ZonePos::Any },
            tapped: false,
        },
    );
    // −7: Flip five coins. Target opponent skips their next X turns, where X = heads.
    let minus_seven = loyalty_ability(
        -7,
        Effect::FlipCoinsSkipNextTurns { who: PlayerRef::Opponent, coins: 5 },
    );
    db.insert(
        planeswalker(
            RAL_ZAREK_GUEST_LECTURER,
            "Ral Zarek, Guest Lecturer",
            PlaneswalkerType::Ral,
            &[Color::Black],
            mana_cost(1, &[(Color::Black, 2)]),
            3,
            vec![plus_one, minus_one, minus_two, minus_seven],
        )
        .with_text(
            "+1: Surveil 2.\n−1: Any number of target players each discard a card.\n−2: Return target creature card with mana value 3 or less from your graveyard to the battlefield.\n−7: Flip five coins. Target opponent skips their next X turns, where X is the number of coins that came up heads.",
        ),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayableAction, PlayerView};
    use crate::basics::{CounterKind, Phase};
    use crate::cards::{build_game, grp};
    use crate::ids::{ObjId, PlayerId};
    use crate::priority::Engine;
    use crate::subtypes::{Subtype, Supertype};

    /// For `ChooseTargets`, selects **every** legal candidate of each slot (up to `max`) — so an
    /// "any number of target players" slot targets all offered players. Passes on anything else.
    struct TargetAllAgent;
    impl Agent for TargetAllAgent {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::ChooseTargets { slots, .. } => {
                    let mut pairs = Vec::new();
                    for (si, slot) in slots.iter().enumerate() {
                        let take = (slot.max as usize).min(slot.legal.len());
                        for ci in 0..take {
                            pairs.push((si as u32, ci as u32));
                        }
                    }
                    DecisionResponse::Pairs(pairs)
                }
                _ => DecisionResponse::Pass,
            }
        }
    }

    fn find_ability(e: &Engine, source: ObjId, want: u32) -> Option<crate::agent::AbilityRef> {
        e.legal_actions(PlayerId(0)).iter().find_map(|a| match a {
            PlayableAction::Activate { source: s, ability } if *s == source && ability.0 == want => Some(*ability),
            _ => None,
        })
    }

    #[test]
    fn ral_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(RAL_ZAREK_GUEST_LECTURER).unwrap();
        assert!(def.fully_implemented, "all four loyalty abilities implemented");
        assert_eq!(def.chars.card_types, vec![CardType::Planeswalker]);
        assert!(def.chars.supertypes.contains(&Supertype::Legendary));
        assert!(def.chars.subtypes.contains(&Subtype::Planeswalker(PlaneswalkerType::Ral)));
        assert_eq!(def.chars.loyalty, Some(3));
        assert_eq!(def.abilities.len(), 4, "+1, −1, −2, −7");
    }

    /// Real-path `−7`: with Ral at 7 loyalty, the ultimate flips five coins and the opponent's
    /// `skip_next_turns` rises by the heads count (0..=5); loyalty drops 7 → 0.
    #[test]
    fn minus_seven_flips_coins_and_queues_opponent_turn_skips() {
        let mut state = build_game(7, &[&[], &[]]); // seed 7
        let ral = state.add_card(
            PlayerId(0),
            state.card_db().get(RAL_ZAREK_GUEST_LECTURER).unwrap().chars.clone(),
            Zone::Battlefield,
        );
        // Raise Ral to 7 loyalty so the −7 is payable.
        state.objects.get_mut(&ral).unwrap().counters.counts.insert(CounterKind::Loyalty, 7);
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let mut e = Engine::new(state, vec![Box::new(TargetAllAgent), Box::new(TargetAllAgent)]);
        let minus7 = find_ability(&e, ral, 3).expect("the −7 ability is offered at 7 loyalty");
        e.activate_ability(PlayerId(0), ral, minus7);
        e.resolve_top();
        let skips = e.state.player(PlayerId(1)).skip_next_turns;
        assert!(skips <= 5, "0..=5 heads among five coin flips (got {skips})");
        assert_eq!(e.state.object(ral).counters.get(&CounterKind::Loyalty), 0, "−7 loyalty (7 → 0)");
    }

    /// The skip-turns mechanism: with the opponent's `skip_next_turns` = 2, the active player takes
    /// consecutive turns until those two skips drain (CR 720 / `advance_turn`).
    #[test]
    fn queued_skips_make_the_active_player_take_consecutive_turns() {
        let mut state = build_game(1, &[&[], &[]]);
        state.player_mut(PlayerId(1)).skip_next_turns = 2;
        state.active_player = PlayerId(0);
        let mut e = Engine::new(state, vec![Box::new(TargetAllAgent), Box::new(TargetAllAgent)]);
        // P0 ends turn → P1 would go but skips (2→1) → back to P0.
        e.advance_turn();
        assert_eq!(e.state.active_player, PlayerId(0), "P1's first turn skipped");
        assert_eq!(e.state.player(PlayerId(1)).skip_next_turns, 1);
        // P0 ends again → P1 skips (1→0) → back to P0.
        e.advance_turn();
        assert_eq!(e.state.active_player, PlayerId(0), "P1's second turn skipped");
        assert_eq!(e.state.player(PlayerId(1)).skip_next_turns, 0);
        // P0 ends again → P1 finally takes a turn.
        e.advance_turn();
        assert_eq!(e.state.active_player, PlayerId(1), "skips drained; P1 takes its turn");
    }

    /// Real-path `−2`: a mana-value-≤3 creature card in your graveyard returns to the battlefield;
    /// loyalty drops 3 → 1.
    #[test]
    fn minus_two_reanimates_from_graveyard() {
        let mut state = build_game(1, &[&[], &[]]);
        let ral = state.add_card(
            PlayerId(0),
            state.card_db().get(RAL_ZAREK_GUEST_LECTURER).unwrap().chars.clone(),
            Zone::Battlefield,
        );
        // Grizzly Bears ({1}{G}, MV 2) in your graveyard is a legal −2 target.
        let bears = state.add_card(PlayerId(0), state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Graveyard);
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let mut e = Engine::new(state, vec![Box::new(TargetAllAgent), Box::new(TargetAllAgent)]);
        let minus2 = find_ability(&e, ral, 2).expect("the −2 ability is offered"); // index 2 == `−2`
        e.activate_ability(PlayerId(0), ral, minus2);
        e.resolve_top();
        assert_eq!(e.state.object(bears).zone, Zone::Battlefield, "the creature card returned to the battlefield");
        assert_eq!(e.state.object(ral).counters.get(&CounterKind::Loyalty), 1, "−2 loyalty (3 → 1)");
    }

    /// Real-path `−1`: targets both players; each discards a card (exercises `PlayerRef::Each` over
    /// the `ForEachTarget` player slot). Loyalty drops 3 → 2.
    #[test]
    fn minus_one_each_target_player_discards() {
        let mut state = build_game(1, &[&[], &[]]);
        let ral = state.add_card(
            PlayerId(0),
            state.card_db().get(RAL_ZAREK_GUEST_LECTURER).unwrap().chars.clone(),
            Zone::Battlefield,
        );
        // A card in each player's hand to discard.
        state.add_card(PlayerId(0), state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Hand);
        state.add_card(PlayerId(1), state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Hand);
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let (h0, h1) = (state.player(PlayerId(0)).hand.len(), state.player(PlayerId(1)).hand.len());
        let mut e = Engine::new(state, vec![Box::new(TargetAllAgent), Box::new(TargetAllAgent)]);
        let minus1 = find_ability(&e, ral, 1).expect("the −1 ability is offered"); // index 1 == `−1`
        e.activate_ability(PlayerId(0), ral, minus1);
        e.resolve_top();
        assert_eq!(e.state.player(PlayerId(0)).hand.len(), h0 - 1, "you discarded a card");
        assert_eq!(e.state.player(PlayerId(1)).hand.len(), h1 - 1, "the opponent discarded a card");
        assert_eq!(e.state.object(ral).counters.get(&CounterKind::Loyalty), 2, "−1 loyalty (3 → 2)");
    }
}
