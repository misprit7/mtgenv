//! Skycoach Conductor // All Aboard — `{2}{U}` Creature — Bird Pilot 2/3 (Flash, Flying, vigilance) //
//! `{U}` Instant (first printed SOS). A **Prepare** DFC whose back face is a **blink**.
//!
//! Front oracle: "Flash. Flying, vigilance. This creature enters prepared. (While it's prepared, you
//! may cast a copy of its spell. Doing so unprepares it.)"
//! Back oracle (All Aboard): "Exile target non-Pilot creature you control, then return that card to the
//! battlefield under its owner's control."
//!
//! **Fully implemented** — the front is the usual Prepare rails on a 2/3 with Flash/Flying/vigilance
//! (the Flash back-face timing means the prepared cast is offered at instant speed). The back is the
//! lander for [`Effect::Blink`] (CR 603.6e): the targeted non-Pilot creature is exiled and immediately
//! returned as a **new** object — enters-the-battlefield triggers re-fire and counters / marked damage /
//! auras / summoning-sickness all reset (CR 400.7).

use crate::basics::{CardType, Color};
use crate::cards::{creature, helpers, mana_cost, spell, CardDb};
use crate::effects::ability::Keyword;
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::PlayerRef;
use crate::effects::{Effect, EffectTarget};
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const SKYCOACH_CONDUCTOR: u32 = 403;
/// The copy-only back-face spell (reserved 9700+ Prepare block).
pub const ALL_ABOARD: u32 = 9728;

pub fn register(db: &mut CardDb) {
    // Back face — "All Aboard" ({U} Instant): blink a non-Pilot creature you control.
    db.insert(
        spell(
            ALL_ABOARD,
            "All Aboard",
            CardType::Instant,
            Color::Blue,
            mana_cost(0, &[(Color::Blue, 1)]),
            Effect::Blink {
                what: EffectTarget::Target(TargetSpec {
                    // "non-Pilot creature you control".
                    kind: TargetKind::Creature(CardFilter::All(vec![
                        CardFilter::ControlledBy(PlayerRef::Controller),
                        CardFilter::Not(Box::new(CardFilter::HasSubtype(CreatureType::Pilot.into()))),
                    ])),
                    min: 1,
                    max: 1,
                    distinct: true,
                }),
            },
        )
        .with_text(
            "Exile target non-Pilot creature you control, then return that card to the battlefield under its owner's control.",
        ),
    );

    // Front face — the 2/3 flier; enters-prepared via a `SelfEnters → BecomePrepared` trigger.
    let mut front = creature(
        SKYCOACH_CONDUCTOR,
        "Skycoach Conductor",
        &[CreatureType::Bird, CreatureType::Pilot],
        Color::Blue,
        mana_cost(2, &[(Color::Blue, 1)]),
        2,
        3,
        helpers::enters_prepared(ALL_ABOARD),
    );
    front.chars.keywords = vec![Keyword::Flying, Keyword::Vigilance, Keyword::Flash];
    front.text = "Flash\nFlying, vigilance\nThis creature enters prepared. (While it's prepared, you may cast a copy of its spell. Doing so unprepares it.)\n// All Aboard {U} Instant — Exile target non-Pilot creature you control, then return that card to the battlefield under its owner's control.".to_string();
    db.insert(front);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayableAction, PlayerView};
    use crate::basics::{CounterKind, Phase, Zone};
    use crate::cards::grp;
    use crate::effects::ability::{Ability, EventPattern};
    use crate::ids::{ObjId, PlayerId};
    use crate::priority::Engine;
    use crate::state::GameState;

    fn db_with_card() -> CardDb {
        let mut db = crate::cards::starter_db();
        register(&mut db);
        db
    }

    #[test]
    fn skycoach_conductor_ir() {
        let db = db_with_card();
        let front = db.get(SKYCOACH_CONDUCTOR).unwrap();
        assert_eq!(front.chars.card_types, vec![CardType::Creature]);
        assert_eq!(
            front.chars.keywords,
            vec![Keyword::Flying, Keyword::Vigilance, Keyword::Flash]
        );
        assert!(matches!(front.abilities[0], Ability::Prepare { spell: ALL_ABOARD }));
        assert!(matches!(
            front.abilities[1],
            Ability::Triggered { event: EventPattern::SelfEnters, .. }
        ));
        assert!(matches!(db.get(ALL_ABOARD).unwrap().spell_effect(), Some(Effect::Blink { .. })));
    }

    /// Picks slot-0 candidate 0 for ChooseTargets; passes otherwise.
    struct PickFirst;
    impl Agent for PickFirst {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::ChooseTargets { .. } => DecisionResponse::Pairs(vec![(0, 0)]),
                _ => DecisionResponse::Pass,
            }
        }
    }

    fn add(state: &mut GameState, who: PlayerId, grp_id: u32, zone: Zone) -> ObjId {
        let c = state.card_db().get(grp_id).unwrap().chars.clone();
        state.add_card(who, c, zone)
    }

    /// Headline (CR 603.6e blink): a prepared Skycoach casts All Aboard on an Elvish Visionary carrying
    /// a +1/+1 counter and marked damage → the Visionary is exiled and returned as a NEW object: its
    /// ETB "draw a card" re-fires, and the counter + damage are gone. The Pilot Skycoach itself is not a
    /// legal target ("non-Pilot"), so the Visionary is the only candidate.
    #[test]
    fn blinks_a_creature_refiring_etb_and_clearing_state() {
        let mut state = GameState::new(2, 1);
        state.set_card_db(std::sync::Arc::new(db_with_card()));
        let skycoach = add(&mut state, PlayerId(0), SKYCOACH_CONDUCTOR, Zone::Battlefield);
        state.objects.get_mut(&skycoach).unwrap().prepared = true;
        let visionary = add(&mut state, PlayerId(0), grp::ELVISH_VISIONARY, Zone::Battlefield);
        // Give the Visionary a counter + marked damage to prove blink resets them (CR 400.7).
        *state.objects.get_mut(&visionary).unwrap().counters.counts.entry(CounterKind::PlusOnePlusOne).or_insert(0) += 1;
        state.objects.get_mut(&visionary).unwrap().damage_marked = 1;
        // {U} for the All Aboard copy + a couple of library cards to draw from.
        add(&mut state, PlayerId(0), grp::ISLAND, Zone::Battlefield);
        add(&mut state, PlayerId(0), grp::FOREST, Zone::Library);
        add(&mut state, PlayerId(0), grp::FOREST, Zone::Library);
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let hand_before = state.player(PlayerId(0)).hand.len();
        let mut e = Engine::new(state, vec![Box::new(PickFirst), Box::new(PickFirst)]);

        e.cast_prepared(PlayerId(0), skycoach);
        e.resolve_top(); // All Aboard resolves → blink the Visionary
        e.run_agenda(); // queue its re-entered ETB "draw a card"
        e.resolve_top(); // resolve the ETB draw

        assert!(
            e.state.player(PlayerId(0)).battlefield.contains(&visionary),
            "the blinked creature returned to the battlefield"
        );
        assert_eq!(
            e.state.object(visionary).counters.get(&CounterKind::PlusOnePlusOne),
            0,
            "blink reset its +1/+1 counter (a new object, CR 400.7)"
        );
        assert_eq!(e.state.object(visionary).damage_marked, 0, "blink cleared marked damage");
        assert!(e.state.object(visionary).summoning_sick, "the returned creature is summoning sick");
        assert_eq!(
            e.state.player(PlayerId(0)).hand.len(),
            hand_before + 1,
            "the re-entered Visionary's ETB drew a card"
        );
    }

    /// The "non-Pilot" filter: with only the Pilot Skycoach on the battlefield, All Aboard has no legal
    /// target, so a prepared Skycoach is NOT offered the prepared cast.
    #[test]
    fn does_not_offer_prepared_cast_without_a_nonpilot_target() {
        let mut state = GameState::new(2, 1);
        state.set_card_db(std::sync::Arc::new(db_with_card()));
        let skycoach = add(&mut state, PlayerId(0), SKYCOACH_CONDUCTOR, Zone::Battlefield);
        state.objects.get_mut(&skycoach).unwrap().prepared = true;
        add(&mut state, PlayerId(0), grp::ISLAND, Zone::Battlefield);
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let e = Engine::new(state, vec![Box::new(PickFirst), Box::new(PickFirst)]);
        assert!(
            !e.legal_actions(PlayerId(0))
                .iter()
                .any(|a| matches!(a, PlayableAction::CastPrepared { .. })),
            "the only creature is a Pilot → All Aboard has no legal target → no prepared cast offered"
        );
        // Add a non-Pilot creature → now it's offered.
        let mut state2 = GameState::new(2, 1);
        state2.set_card_db(std::sync::Arc::new(db_with_card()));
        let sky2 = add(&mut state2, PlayerId(0), SKYCOACH_CONDUCTOR, Zone::Battlefield);
        state2.objects.get_mut(&sky2).unwrap().prepared = true;
        add(&mut state2, PlayerId(0), grp::GRIZZLY_BEARS, Zone::Battlefield);
        add(&mut state2, PlayerId(0), grp::ISLAND, Zone::Battlefield);
        state2.active_player = PlayerId(0);
        state2.phase = Phase::PrecombatMain;
        let e2 = Engine::new(state2, vec![Box::new(PickFirst), Box::new(PickFirst)]);
        assert!(
            e2.legal_actions(PlayerId(0))
                .iter()
                .any(|a| matches!(a, PlayableAction::CastPrepared { source } if *source == sky2)),
            "a non-Pilot creature exists → All Aboard is targetable → prepared cast offered"
        );
    }
}
