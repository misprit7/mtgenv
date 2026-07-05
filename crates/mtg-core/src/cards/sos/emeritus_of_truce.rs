//! Emeritus of Truce // Swords to Plowshares — `{1}{W}{W}` Creature — Cat Cleric 3/3 // `{W}` Instant
//! (first printed SOS). A **Prepare** DFC that prepares CONDITIONALLY on enter.
//!
//! Front oracle: "When this creature enters, target player creates a 1/1 white and black Inkling
//! creature token with flying. Then if an opponent controls more creatures than you, this creature
//! becomes prepared. (While it's prepared, you may cast a copy of its spell. Doing so unprepares it.)"
//! Back oracle (Swords to Plowshares): "Exile target creature. Its controller gains life equal to its
//! power."
//!
//! **Fully implemented.** The front is a plain Prepare marker + a `SelfEnters` trigger whose effect is a
//! sequence: a target player makes the shared Inkling token, then a `Conditional` (opp controls more
//! creatures than you → `Effect::BecomePrepared`). The back exiles a creature and gains its controller
//! life equal to its power — the life gain is sequenced BEFORE the exile so "its power" reads the
//! creature while it's still on the battlefield (its last-known power, since the same resolution then
//! removes it — an identical value with no LKI plumbing); "its controller" reads the resolution-start
//! controller snapshot (`PlayerRef::ControllerOfTarget`), so it's correct even after the exile.

use crate::basics::{CardType, Color, Zone};
use crate::cards::{creature, helpers, mana_cost, spell, CardDb};
use crate::effects::ability::{Ability, EventPattern};
use crate::effects::condition::Condition;
use crate::effects::target::{CardFilter, PlayerFilter, TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const EMERITUS_OF_TRUCE: u32 = 404;
/// The copy-only back-face spell (reserved 9700+ Prepare block).
pub const SWORDS_TO_PLOWSHARES: u32 = 9729;

/// "creatures a given player controls" — for the "opp controls more creatures than you" check.
fn creatures_controlled(by: PlayerRef) -> ValueExpr {
    ValueExpr::Count {
        zone: Zone::Battlefield,
        filter: CardFilter::HasCardType(CardType::Creature),
        controller: Some(by),
    }
}

pub fn register(db: &mut CardDb) {
    // Back face — "Swords to Plowshares" ({W} Instant): its controller gains life = its power, THEN
    // exile it (gain-before-exile so "its power" reads the live creature — same value, no LKI).
    db.insert(
        spell(
            SWORDS_TO_PLOWSHARES,
            "Swords to Plowshares",
            CardType::Instant,
            Color::White,
            mana_cost(0, &[(Color::White, 1)]),
            Effect::Sequence(vec![
                Effect::GainLife {
                    who: PlayerRef::ControllerOfTarget(0),
                    amount: ValueExpr::PowerOfTarget(0),
                },
                Effect::Exile {
                    what: EffectTarget::Target(TargetSpec {
                        kind: TargetKind::Creature(CardFilter::Any),
                        min: 1,
                        max: 1,
                        distinct: true,
                    }),
                },
            ]),
        )
        .with_text("Exile target creature. Its controller gains life equal to its power."),
    );

    // Front face — the 3/3; a target player makes an Inkling, then conditionally becomes prepared.
    let etb = Effect::Sequence(vec![
        // "target player" (CR 115.1) — the slot the token's controller references via ChosenTarget(0).
        Effect::TargetPlayer(PlayerFilter::Any),
        Effect::CreateToken {
            spec: helpers::inkling_token(),
            count: ValueExpr::Fixed(1),
            controller: PlayerRef::ChosenTarget(0),
            dynamic_counters: vec![],
        },
        // "Then if an opponent controls more creatures than you, this creature becomes prepared."
        // opp > you ⟺ opp ≥ you + 1.
        Effect::Conditional {
            cond: Condition::ValueAtLeast(
                creatures_controlled(PlayerRef::Opponent),
                ValueExpr::Sum(
                    Box::new(creatures_controlled(PlayerRef::Controller)),
                    Box::new(ValueExpr::Fixed(1)),
                ),
            ),
            then: Box::new(Effect::BecomePrepared),
            otherwise: None,
        },
    ]);
    let mut front = creature(
        EMERITUS_OF_TRUCE,
        "Emeritus of Truce",
        &[CreatureType::Cat, CreatureType::Cleric],
        Color::White,
        mana_cost(1, &[(Color::White, 2)]),
        3,
        3,
        vec![
            Ability::Prepare { spell: SWORDS_TO_PLOWSHARES },
            Ability::Triggered {
                event: EventPattern::SelfEnters,
                condition: None,
                intervening_if: false,
                effect: etb,
            },
        ],
    );
    front.text = "When this creature enters, target player creates a 1/1 white and black Inkling creature token with flying. Then if an opponent controls more creatures than you, this creature becomes prepared. (While it's prepared, you may cast a copy of its spell. Doing so unprepares it.)\n// Swords to Plowshares {W} Instant — Exile target creature. Its controller gains life equal to its power.".to_string();
    db.insert(front);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, CastVariant, DecisionRequest, DecisionResponse, PlayerView};
    use crate::basics::{Phase, Target};
    use crate::cards::grp;
    use crate::ids::{ObjId, PlayerId};
    use crate::priority::Engine;
    use crate::state::GameState;

    fn db_with_card() -> CardDb {
        let mut db = crate::cards::starter_db();
        register(&mut db);
        db
    }

    #[test]
    fn emeritus_of_truce_ir() {
        let db = db_with_card();
        let front = db.get(EMERITUS_OF_TRUCE).unwrap();
        assert_eq!(front.chars.card_types, vec![CardType::Creature]);
        assert!(matches!(front.abilities[0], Ability::Prepare { spell: SWORDS_TO_PLOWSHARES }));
        assert!(matches!(
            front.abilities[1],
            Ability::Triggered { event: EventPattern::SelfEnters, .. }
        ));
        let back = db.get(SWORDS_TO_PLOWSHARES).unwrap();
        assert_eq!(back.chars.card_types, vec![CardType::Instant]);
    }

    fn add(state: &mut GameState, who: PlayerId, grp_id: u32, zone: crate::basics::Zone) -> ObjId {
        let c = state.card_db().get(grp_id).unwrap().chars.clone();
        state.add_card(who, c, zone)
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

    /// Swords to Plowshares exiles a target creature and its controller gains life equal to its power —
    /// read while the creature is still on the battlefield, so a buffed creature gains the right amount.
    #[test]
    fn swords_exiles_and_gains_life_equal_to_power() {
        use crate::basics::{CounterKind, Zone};
        let mut state = GameState::new(2, 1);
        state.set_card_db(std::sync::Arc::new(db_with_card()));
        // A Grizzly Bears (2/2) with a +1/+1 counter (so it's a 3/3) controlled by P1.
        let bears = add(&mut state, PlayerId(1), grp::GRIZZLY_BEARS, Zone::Battlefield);
        *state.objects.get_mut(&bears).unwrap().counters.counts.entry(CounterKind::PlusOnePlusOne).or_insert(0) += 1;
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let p1_life = state.player(PlayerId(1)).life;
        let effect = state.card_db().get(SWORDS_TO_PLOWSHARES).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(PickFirst), Box::new(PickFirst)]);
        // Resolve the effect directly with the bears as the chosen target (its controller = P1).
        e.resolve_effect(
            &effect,
            &crate::effects::action::ResolutionCtx {
                controller: Some(PlayerId(0)),
                chosen_targets: vec![Target::Object(bears)],
                target_controllers: vec![Some(PlayerId(1))],
                ..Default::default()
            },
            crate::effects::action::WbReason::Resolve(crate::ids::StackId(99)),
        );
        assert!(e.state.player(PlayerId(1)).exile.contains(&bears), "the creature was exiled");
        assert!(!e.state.player(PlayerId(1)).battlefield.contains(&bears), "it left the battlefield");
        assert_eq!(
            e.state.player(PlayerId(1)).life,
            p1_life + 3,
            "its controller gained life equal to its (buffed) power = 3"
        );
    }

    /// The front's ETB through the real cast path: cast the Emeritus → it enters and its trigger makes a
    /// target player (P0) an Inkling; with an opponent controlling more creatures than you, it then
    /// becomes prepared.
    #[test]
    fn enters_makes_inkling_then_conditionally_prepares() {
        use crate::basics::Zone;
        let mut state = GameState::new(2, 1);
        state.set_card_db(std::sync::Arc::new(db_with_card()));
        // The opponent controls three creatures → opp has more than you even after your token.
        for _ in 0..3 {
            add(&mut state, PlayerId(1), grp::GRIZZLY_BEARS, Zone::Battlefield);
        }
        let emeritus = add(&mut state, PlayerId(0), EMERITUS_OF_TRUCE, Zone::Hand);
        for _ in 0..3 {
            add(&mut state, PlayerId(0), grp::PLAINS, Zone::Battlefield); // {1}{W}{W}
        }
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let mut e = Engine::new(state, vec![Box::new(PickFirst), Box::new(PickFirst)]);

        e.cast_spell(PlayerId(0), emeritus, CastVariant::Normal);
        e.resolve_top(); // the Emeritus enters (its ETB trigger queues)
        e.run_agenda(); // put the trigger on the stack (target player = P0)
        e.resolve_top(); // resolve it: make the Inkling, then conditionally prepare

        assert!(
            e.state.player(PlayerId(0)).battlefield.iter().any(|&o| e.state.object(o).chars.name == "Inkling"),
            "the target player created an Inkling token"
        );
        assert!(
            e.state.object(emeritus).prepared,
            "opp controls more creatures than you → the Emeritus became prepared"
        );
    }
}
