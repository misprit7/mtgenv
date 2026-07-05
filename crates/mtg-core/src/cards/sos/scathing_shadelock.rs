//! Scathing Shadelock // Venomous Words — `{4}{B}` Creature — Snake Warlock 4/6 // `{B}` Sorcery
//! (first printed SOS). A **Prepare** DFC — the "at the beginning of your first main phase" variant.
//!
//! Front oracle: "At the beginning of your first main phase, this creature becomes prepared. (While
//! it's prepared, you may cast a copy of its spell. Doing so unprepares it.)"
//! Back oracle (Venomous Words): "Target creature you control gets +2/+0 and gains deathtouch until
//! end of turn."
//!
//! **Fully implemented** — the prepare trigger is a `BeginningOfStep(PrecombatMain)` ability gated by
//! `Condition::YourTurn` (so "your first main phase", not an opponent's) whose effect is
//! [`Effect::BecomePrepared`]. The back face Venomous Words pumps +2/+0 and grants deathtouch until
//! end of turn to a single "target creature you control" (mirrors Chase Inspiration's pump+grant).

use crate::basics::{CardType, Color};
use crate::cards::{creature, mana_cost, spell, CardDb};
use crate::effects::ability::{Ability, EventPattern, Keyword};
use crate::effects::condition::{Condition, Duration};
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const SCATHING_SHADELOCK: u32 = 375;
/// The copy-only back-face spell (reserved 9700+ Prepare block).
pub const VENOMOUS_WORDS: u32 = 9702;

pub fn register(db: &mut CardDb) {
    // Back face — "Venomous Words" ({B} Sorcery): +2/+0 and deathtouch until EOT on a creature you
    // control (both clauses reference the same chosen target via `ChosenIndex(0)`).
    let venomous_words = Effect::Sequence(vec![
        Effect::PumpPT {
            what: EffectTarget::Target(TargetSpec {
                kind: TargetKind::Creature(CardFilter::ControlledBy(PlayerRef::Controller)),
                min: 1,
                max: 1,
                distinct: true,
            }),
            power: ValueExpr::Fixed(2),
            toughness: ValueExpr::Fixed(0),
            duration: Duration::UntilEndOfTurn,
        },
        Effect::GrantKeyword {
            what: EffectTarget::ChosenIndex(0),
            keyword: Keyword::Deathtouch,
            duration: Duration::UntilEndOfTurn,
        },
    ]);
    db.insert(
        spell(
            VENOMOUS_WORDS,
            "Venomous Words",
            CardType::Sorcery,
            Color::Black,
            mana_cost(0, &[(Color::Black, 1)]),
            venomous_words,
        )
        .with_text("Target creature you control gets +2/+0 and gains deathtouch until end of turn."),
    );

    // Front face — becomes prepared at the beginning of your first main phase.
    let mut front = creature(
        SCATHING_SHADELOCK,
        "Scathing Shadelock",
        &[CreatureType::Snake, CreatureType::Warlock],
        Color::Black,
        mana_cost(4, &[(Color::Black, 1)]),
        4,
        6,
        vec![
            Ability::Prepare { spell: VENOMOUS_WORDS },
            Ability::Triggered {
                event: EventPattern::BeginningOfStep(crate::basics::Phase::PrecombatMain),
                condition: Some(Condition::YourTurn),
                intervening_if: false,
                effect: Effect::BecomePrepared,
            },
        ],
    );
    front.text = "At the beginning of your first main phase, this creature becomes prepared. (While it's prepared, you may cast a copy of its spell. Doing so unprepares it.)\n// Venomous Words {B} Sorcery — Target creature you control gets +2/+0 and gains deathtouch until end of turn.".to_string();
    db.insert(front);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, DecisionRequest, DecisionResponse, GameEvent, PlayableAction, PlayerView};
    use crate::basics::{Phase, Zone};
    use crate::cards::grp;
    use crate::ids::PlayerId;
    use crate::priority::Engine;
    use crate::state::GameState;
    use expect_test::expect;
    use std::sync::Arc;

    fn db_with_card() -> CardDb {
        let mut db = crate::cards::starter_db();
        register(&mut db);
        db
    }

    #[test]
    fn scathing_shadelock_ir() {
        let db = db_with_card();
        let front = db.get(SCATHING_SHADELOCK).unwrap();
        expect![[r#"
            [
                Prepare {
                    spell: 9702,
                },
                Triggered {
                    event: BeginningOfStep(
                        PrecombatMain,
                    ),
                    condition: Some(
                        YourTurn,
                    ),
                    intervening_if: false,
                    effect: BecomePrepared,
                },
            ]"#]]
        .assert_eq(&format!("{:#?}", front.abilities));
    }

    struct PrepareAgent;
    impl Agent for PrepareAgent {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::Confirm { .. } => DecisionResponse::Bool(true),
                DecisionRequest::ChooseTargets { slots, .. } => DecisionResponse::Pairs(
                    slots.iter().enumerate().map(|(si, _)| (si as u32, 0u32)).collect(),
                ),
                _ => DecisionResponse::Pass,
            }
        }
    }

    /// At your first main phase → prepared → cast the copy of Venomous Words on the shadelock (its
    /// only creature) → it becomes a 6/6 with deathtouch, and is unprepared.
    #[test]
    fn first_main_prepares_then_casts_copy() {
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(db_with_card()));
        let shade = {
            let c = state.card_db().get(SCATHING_SHADELOCK).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        let s = state.card_db().get(grp::SWAMP).unwrap().chars.clone();
        state.add_card(PlayerId(0), s, Zone::Battlefield);
        let mut e = Engine::new(state, vec![Box::new(PrepareAgent), Box::new(PrepareAgent)]);
        e.state.active_player = PlayerId(0);
        e.state.phase = Phase::PrecombatMain;

        // The beginning of P0's first main phase fires the (YourTurn-gated) prepare trigger.
        e.broadcast(GameEvent::PhaseBegan {
            turn: 1,
            phase: Phase::PrecombatMain,
            active: PlayerId(0),
        });
        e.run_agenda();
        e.resolve_top();
        assert!(e.state.object(shade).prepared, "your-first-main trigger prepared it");
        assert!(
            e.legal_actions(PlayerId(0))
                .iter()
                .any(|a| matches!(a, PlayableAction::CastPrepared { source } if *source == shade)),
        );

        // Cast the copy on the shadelock: 4/6 → 6/6 with deathtouch; unprepared afterward.
        e.cast_prepared(PlayerId(0), shade);
        e.resolve_top();
        let cc = e.state.computed(shade);
        assert_eq!(cc.power, Some(6), "+2 power from the copy");
        assert_eq!(cc.toughness, Some(6), "+0 toughness");
        assert!(cc.has_keyword(Keyword::Deathtouch), "gained deathtouch");
        assert!(!e.state.object(shade).prepared, "casting the copy unprepared it");
    }
}
