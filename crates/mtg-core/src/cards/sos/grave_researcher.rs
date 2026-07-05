//! Grave Researcher // Reanimate — `{2}{B}` Creature — Troll Warlock 3/3 // `{B}` Sorcery (first
//! printed SOS). A **Prepare** DFC whose back is a control-override reanimation.
//!
//! Front oracle: "At the beginning of your upkeep, surveil 1. Then if there are three or more creature
//! cards in your graveyard, this creature becomes prepared. (While it's prepared, you may cast a copy of
//! its spell. Doing so unprepares it.)"
//! Back oracle (Reanimate): "Put target creature card from a graveyard onto the battlefield under your
//! control. You lose life equal to that card's mana value."
//!
//! **Fully implemented.** The front is an upkeep trigger (`BeginningOfStep(Upkeep)` gated `YourTurn`)
//! whose effect surveils 1 then conditionally prepares (three-or-more creature cards in your graveyard —
//! checked AFTER the surveil, which may add one). The back reanimates a creature from **any** graveyard
//! under your control via [`Effect::ReanimateUnderControl`] (owner unchanged, controller = you) and then
//! loses you life equal to its [`ValueExpr::ManaValueOfTarget`] — the reanimation and the life loss share
//! the one chosen target (slot 0), and mana value is a printed value so it reads correctly post-move.

use crate::basics::{CardType, Color, Zone};
use crate::cards::{creature, mana_cost, spell, CardDb};
use crate::effects::ability::{Ability, EventPattern};
use crate::effects::condition::Condition;
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const GRAVE_RESEARCHER: u32 = 408;
/// The copy-only back-face spell (reserved 9700+ Prepare block).
pub const REANIMATE: u32 = 9733;

/// "creature cards in your graveyard" — for the "three or more" prepare gate.
fn creatures_in_your_graveyard() -> ValueExpr {
    ValueExpr::Count {
        zone: Zone::Graveyard,
        filter: CardFilter::HasCardType(CardType::Creature),
        controller: Some(PlayerRef::Controller),
    }
}

pub fn register(db: &mut CardDb) {
    // Back face — "Reanimate" ({B} Sorcery): put target creature card from a graveyard onto the
    // battlefield under your control, then lose life equal to its mana value.
    db.insert(
        spell(
            REANIMATE,
            "Reanimate",
            CardType::Sorcery,
            Color::Black,
            mana_cost(0, &[(Color::Black, 1)]),
            Effect::Sequence(vec![
                Effect::ReanimateUnderControl {
                    what: EffectTarget::Target(TargetSpec {
                        kind: TargetKind::CardInZone {
                            zone: Zone::Graveyard,
                            filter: CardFilter::HasCardType(CardType::Creature),
                        },
                        min: 1,
                        max: 1,
                        distinct: true,
                    }),
                },
                Effect::LoseLife {
                    who: PlayerRef::Controller,
                    amount: ValueExpr::ManaValueOfTarget(0),
                },
            ]),
        )
        .with_text("Put target creature card from a graveyard onto the battlefield under your control. You lose life equal to that card's mana value."),
    );

    // Front face — the 3/3; upkeep surveil 1, then conditionally becomes prepared.
    let upkeep = Effect::Sequence(vec![
        Effect::Surveil { count: ValueExpr::Fixed(1) },
        Effect::Conditional {
            cond: Condition::ValueAtLeast(creatures_in_your_graveyard(), ValueExpr::Fixed(3)),
            then: Box::new(Effect::BecomePrepared),
            otherwise: None,
        },
    ]);
    let mut front = creature(
        GRAVE_RESEARCHER,
        "Grave Researcher",
        &[CreatureType::Troll, CreatureType::Warlock],
        Color::Black,
        mana_cost(2, &[(Color::Black, 1)]),
        3,
        3,
        vec![
            Ability::Prepare { spell: REANIMATE },
            Ability::Triggered {
                event: EventPattern::BeginningOfStep(crate::basics::Phase::Upkeep),
                condition: Some(Condition::YourTurn),
                intervening_if: false,
                effect: upkeep,
            },
        ],
    );
    front.text = "At the beginning of your upkeep, surveil 1. Then if there are three or more creature cards in your graveyard, this creature becomes prepared. (While it's prepared, you may cast a copy of its spell. Doing so unprepares it.)\n// Reanimate {B} Sorcery — Put target creature card from a graveyard onto the battlefield under your control. You lose life equal to that card's mana value.".to_string();
    db.insert(front);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView};
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

    fn add(state: &mut GameState, who: PlayerId, grp_id: u32, zone: Zone) -> ObjId {
        let c = state.card_db().get(grp_id).unwrap().chars.clone();
        state.add_card(who, c, zone)
    }

    #[test]
    fn grave_researcher_ir() {
        let db = db_with_card();
        let front = db.get(GRAVE_RESEARCHER).unwrap();
        assert!(matches!(front.abilities[0], Ability::Prepare { spell: REANIMATE }));
        assert!(matches!(
            front.abilities[1],
            Ability::Triggered { event: EventPattern::BeginningOfStep(Phase::Upkeep), .. }
        ));
        let back = db.get(REANIMATE).unwrap();
        assert_eq!(back.chars.card_types, vec![CardType::Sorcery]);
        assert!(matches!(back.spell_effect(), Some(Effect::Sequence(_))));
    }

    struct PickFirst;
    impl Agent for PickFirst {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::SelectCards { from, .. } if !from.is_empty() => {
                    DecisionResponse::Indices(vec![0])
                }
                _ => DecisionResponse::Pass,
            }
        }
    }

    /// Headline: Reanimate steals a creature from the OPPONENT's graveyard onto YOUR battlefield under
    /// your control, and you lose life equal to its mana value. (A Hill Giant, MV 4 → lose 4.)
    #[test]
    fn reanimate_steals_under_your_control_and_loses_mv_life() {
        let mut state = GameState::new(2, 1);
        state.set_card_db(std::sync::Arc::new(db_with_card()));
        // The opponent (P1) owns a Hill Giant ({3}{R} 3/3, MV 4) in their graveyard.
        let giant = add(&mut state, PlayerId(1), grp::HILL_GIANT, Zone::Graveyard);
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let p0_life = state.player(PlayerId(0)).life;
        let effect = state.card_db().get(REANIMATE).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(PickFirst), Box::new(PickFirst)]);
        e.resolve_effect(
            &effect,
            &crate::effects::action::ResolutionCtx {
                controller: Some(PlayerId(0)),
                chosen_targets: vec![Target::Object(giant)],
                ..Default::default()
            },
            crate::effects::action::WbReason::Resolve(crate::ids::StackId(99)),
        );

        assert!(
            e.state.player(PlayerId(0)).battlefield.contains(&giant),
            "the reanimated creature is on YOUR battlefield"
        );
        assert!(
            !e.state.player(PlayerId(1)).graveyard.contains(&giant),
            "it left the opponent's graveyard"
        );
        assert_eq!(e.state.object(giant).controller, PlayerId(0), "controlled by you");
        assert_eq!(e.state.object(giant).owner, PlayerId(1), "still owned by the opponent");
        assert!(e.state.object(giant).summoning_sick, "enters summoning sick");
        assert_eq!(e.state.player(PlayerId(0)).life, p0_life - 4, "lost life = its mana value (4)");
    }

    /// The reanimated opponent-owned creature, once it dies, is removed from YOUR battlefield vec and
    /// returns to its OWNER's graveyard (exercises the `move_object` control-vs-owner source-removal fix).
    #[test]
    fn reanimated_creature_dies_to_its_owners_graveyard() {
        let mut state = GameState::new(2, 1);
        state.set_card_db(std::sync::Arc::new(db_with_card()));
        let giant = add(&mut state, PlayerId(1), grp::HILL_GIANT, Zone::Graveyard);
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let effect = state.card_db().get(REANIMATE).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(PickFirst), Box::new(PickFirst)]);
        e.resolve_effect(
            &effect,
            &crate::effects::action::ResolutionCtx {
                controller: Some(PlayerId(0)),
                chosen_targets: vec![Target::Object(giant)],
                ..Default::default()
            },
            crate::effects::action::WbReason::Resolve(crate::ids::StackId(99)),
        );
        assert!(e.state.player(PlayerId(0)).battlefield.contains(&giant));
        // Now it dies (battlefield → graveyard). It must leave YOUR battlefield vec and land in the
        // OWNER's (P1's) graveyard — CR 400.7 / the control-vs-owner source-removal fix.
        e.state.move_object(giant, Zone::Graveyard, PlayerId(1));
        assert!(!e.state.player(PlayerId(0)).battlefield.contains(&giant), "removed from your battlefield");
        assert!(e.state.player(PlayerId(1)).graveyard.contains(&giant), "back in the owner's graveyard");
    }

    /// The front's upkeep trigger surveils, then prepares only when three-or-more creature cards sit in
    /// your graveyard. With three creatures already there, it becomes prepared.
    #[test]
    fn upkeep_prepares_with_three_creatures_in_graveyard() {
        let mut state = GameState::new(2, 1);
        state.set_card_db(std::sync::Arc::new(db_with_card()));
        let gr = add(&mut state, PlayerId(0), GRAVE_RESEARCHER, Zone::Battlefield);
        for _ in 0..3 {
            add(&mut state, PlayerId(0), grp::GRIZZLY_BEARS, Zone::Graveyard);
        }
        // A library card so surveil 1 has something to look at.
        add(&mut state, PlayerId(0), grp::FOREST, Zone::Library);
        state.active_player = PlayerId(0);
        state.phase = Phase::Upkeep;
        let mut e = Engine::new(state, vec![Box::new(PickFirst), Box::new(PickFirst)]);
        // Fire the beginning-of-upkeep trigger for the active player.
        e.run_step(Phase::Upkeep);
        assert!(e.state.object(gr).prepared, "three creatures in graveyard → prepared after upkeep surveil");
    }
}
