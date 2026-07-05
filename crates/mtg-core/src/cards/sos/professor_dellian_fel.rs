//! Professor Dellian Fel — `{2}{B}{G}` Legendary Planeswalker — Dellian (first printed SOS).
//!
//! Oracle: "+2: You gain 3 life.
//!          0: You draw a card and lose 1 life.
//!          −3: Destroy target creature.
//!          −6: You get an emblem with 'Whenever you gain life, target opponent loses that much life.'"
//! Starting loyalty 5.
//!
//! **Fully implemented.** All four loyalty abilities are sorcery-timed, once/turn across all of them
//! (CR 606.3), paying `+2`/`0`/`−3`/`−6` loyalty (CR 606.2). The `−6` emblem (CR 114) is a real
//! command-zone object (`Effect::CreateEmblem` → [`grp::DELLIAN_EMBLEM`](crate::cards::grp::DELLIAN_EMBLEM),
//! defined in `cards::emblems`) whose "whenever you gain life, target opponent loses that much"
//! triggered ability functions from `Zone::Command`.

use crate::basics::Color;
use crate::cards::{grp, loyalty_ability, mana_cost, planeswalker, CardDb};
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};
use crate::subtypes::PlaneswalkerType;

/// grp id (per-set ids live near their cards).
pub const PROFESSOR_DELLIAN_FEL: u32 = 364;

pub fn register(db: &mut CardDb) {
    // +2: You gain 3 life.
    let plus_two = loyalty_ability(
        2,
        Effect::GainLife { who: PlayerRef::Controller, amount: ValueExpr::Fixed(3) },
    );
    // 0: You draw a card and lose 1 life.
    let zero = loyalty_ability(
        0,
        Effect::Sequence(vec![
            Effect::Draw { who: PlayerRef::Controller, count: ValueExpr::Fixed(1) },
            Effect::LoseLife { who: PlayerRef::Controller, amount: ValueExpr::Fixed(1) },
        ]),
    );
    // −3: Destroy target creature.
    let minus_three = loyalty_ability(
        -3,
        Effect::Destroy {
            what: EffectTarget::Target(TargetSpec {
                kind: TargetKind::Creature(CardFilter::Any),
                min: 1,
                max: 1,
                distinct: true,
            }),
        },
    );
    // −6: You get an emblem with "Whenever you gain life, target opponent loses that much life."
    let minus_six = loyalty_ability(-6, Effect::CreateEmblem { emblem: grp::DELLIAN_EMBLEM });
    db.insert(
        planeswalker(
            PROFESSOR_DELLIAN_FEL,
            "Professor Dellian Fel",
            PlaneswalkerType::Dellian,
            &[Color::Black, Color::Green],
            mana_cost(2, &[(Color::Black, 1), (Color::Green, 1)]),
            5,
            vec![plus_two, zero, minus_three, minus_six],
        )
        .with_text(
            "+2: You gain 3 life.\n0: You draw a card and lose 1 life.\n−3: Destroy target creature.\n−6: You get an emblem with \"Whenever you gain life, target opponent loses that much life.\"",
        ),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{PlayableAction, RandomAgent};
    use crate::basics::{CardType, CounterKind, Phase, Zone};
    use crate::cards::{build_game, grp};
    use crate::ids::PlayerId;
    use crate::priority::Engine;
    use crate::subtypes::{Subtype, Supertype};

    /// The activated ability of `source` whose index == `want`, if currently legal for player 0.
    fn find_ability(e: &Engine, source: crate::ids::ObjId, want: u32) -> Option<crate::agent::AbilityRef> {
        e.legal_actions(PlayerId(0)).iter().find_map(|a| match a {
            PlayableAction::Activate { source: s, ability } if *s == source && ability.0 == want => Some(*ability),
            _ => None,
        })
    }

    #[test]
    fn dellian_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(PROFESSOR_DELLIAN_FEL).unwrap();
        assert!(def.fully_implemented, "all four loyalty abilities incl. the −6 emblem are built");
        assert_eq!(def.chars.card_types, vec![CardType::Planeswalker]);
        assert!(def.chars.supertypes.contains(&Supertype::Legendary));
        assert!(def.chars.subtypes.contains(&Subtype::Planeswalker(PlaneswalkerType::Dellian)));
        assert_eq!(def.chars.loyalty, Some(5));
        assert_eq!(def.abilities.len(), 4, "+2, 0, −3, −6");
    }

    /// Real-path `−6`: activate the ultimate to get the emblem, then (next turn) gain life via `+2` —
    /// the emblem drains the opponent for that much. All through the real activation/trigger paths.
    #[test]
    fn minus_six_emblem_drains_on_life_gain() {
        let mut state = build_game(1, &[&[], &[]]);
        let mut dellian_chars = state.card_db().get(PROFESSOR_DELLIAN_FEL).unwrap().chars.clone();
        dellian_chars.loyalty = Some(8); // start at 8 so −6 leaves Dellian alive (→ 2)
        let dellian = state.add_card(PlayerId(0), dellian_chars, Zone::Battlefield);
        state.objects.get_mut(&dellian).unwrap().counters.counts.insert(crate::basics::CounterKind::Loyalty, 8);
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
        // −6 (index 3): create the emblem.
        let minus6 = find_ability(&e, dellian, 3).expect("the −6 ability is offered at loyalty 8");
        e.activate_ability(PlayerId(0), dellian, minus6);
        e.resolve_top();
        assert_eq!(e.state.player(PlayerId(0)).command.len(), 1, "the emblem is in your command zone");
        assert_eq!(e.state.object(dellian).counters.get(&crate::basics::CounterKind::Loyalty), 2, "−6 loyalty (8 → 2)");
        // Next turn (loyalty is once/turn): +2 gains 3 life → the emblem drains the opponent for 3.
        e.state.objects.get_mut(&dellian).unwrap().used_once_per_turn = false;
        let opp_life = e.state.player(PlayerId(1)).life;
        let plus2 = find_ability(&e, dellian, 0).expect("the +2 ability is offered");
        e.activate_ability(PlayerId(0), dellian, plus2);
        e.resolve_top(); // +2 resolves → gain 3 life → queues the emblem's GainLife trigger
        e.run_agenda(); // the emblem trigger onto the stack (targets the opponent)
        e.resolve_top(); // resolve → opponent loses 3
        assert_eq!(
            e.state.player(PlayerId(1)).life,
            opp_life - 3,
            "the emblem drained the opponent for the 3 life you gained",
        );
    }

    /// Real-path: activate the `0` ability (draw a card, lose 1 life) and resolve it.
    #[test]
    fn zero_ability_draws_and_loses_life() {
        let mut state = build_game(1, &[&[], &[]]);
        let dellian = state.add_card(
            PlayerId(0),
            state.card_db().get(PROFESSOR_DELLIAN_FEL).unwrap().chars.clone(),
            Zone::Battlefield,
        );
        // Give player 0 a card to draw.
        state.add_card(PlayerId(0), state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Library);
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let life = state.player(PlayerId(0)).life;
        let hand = state.player(PlayerId(0)).hand.len();
        let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
        let zero = find_ability(&e, dellian, 1).expect("the 0 ability is offered"); // index 1 == `0`
        e.activate_ability(PlayerId(0), dellian, zero);
        e.resolve_top();
        assert_eq!(e.state.player(PlayerId(0)).hand.len(), hand + 1, "drew a card");
        assert_eq!(e.state.player(PlayerId(0)).life, life - 1, "lost 1 life");
        assert_eq!(
            e.state.object(dellian).counters.get(&CounterKind::Loyalty),
            5,
            "0 loyalty cost — unchanged at 5",
        );
    }

    /// Real-path: `−3` destroys a target creature and pays 3 loyalty (5 → 2).
    #[test]
    fn minus_three_destroys_a_creature() {
        let mut state = build_game(1, &[&[], &[]]);
        let dellian = state.add_card(
            PlayerId(0),
            state.card_db().get(PROFESSOR_DELLIAN_FEL).unwrap().chars.clone(),
            Zone::Battlefield,
        );
        let bears = state.add_card(PlayerId(1), state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Battlefield);
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
        let minus3 = find_ability(&e, dellian, 2).expect("the −3 ability is offered"); // index 2 == `−3`
        e.activate_ability(PlayerId(0), dellian, minus3);
        e.resolve_top();
        assert_eq!(e.state.object(bears).zone, Zone::Graveyard, "the targeted creature was destroyed");
        assert_eq!(e.state.object(dellian).counters.get(&CounterKind::Loyalty), 2, "−3 loyalty (5 → 2)");
    }
}
