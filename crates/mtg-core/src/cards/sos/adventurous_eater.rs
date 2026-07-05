//! Adventurous Eater // Have a Bite — `{2}{B}` Creature — Human Warlock 3/2 // `{B}` Sorcery
//! (first printed SOS). A **Prepare** DFC (the flagship rails card).
//!
//! Front oracle: "This creature enters prepared. (While it's prepared, you may cast a copy of its
//! spell. Doing so unprepares it.)"
//! Back oracle (Have a Bite): "Put a +1/+1 counter on target creature. You gain 1 life."
//!
//! **Fully implemented** — the front creature is an ordinary 3/2 carrying the `Ability::Prepare` marker
//! (linking it to the [`HAVE_A_BITE`] back-face def) plus a `SelfEnters` trigger whose effect is
//! [`Effect::BecomePrepared`]. "Enters prepared" needs no bespoke machinery: it's just that trigger.
//! While prepared, `legal_priority_actions` offers a [`crate::agent::PlayableAction::CastPrepared`] at
//! Have a Bite's sorcery-speed timing; taking it mints a **paid** copy of the back-face spell (CR
//! 707.12), which resolves (+1/+1 counter + gain 1) and ceases to exist, and unprepares the creature.

use crate::basics::{CardType, Color, CounterKind};
use crate::cards::{creature, mana_cost, spell, CardDb};
use crate::effects::ability::{Ability, EventPattern};
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const ADVENTUROUS_EATER: u32 = 373;
/// The copy-only back-face spell (reserved 9700+ Prepare block).
pub const HAVE_A_BITE: u32 = 9700;

pub fn register(db: &mut CardDb) {
    // Back face — "Have a Bite" ({B} Sorcery): +1/+1 counter on target creature, then gain 1 life.
    let have_a_bite = Effect::Sequence(vec![
        Effect::PutCounters {
            what: EffectTarget::Target(TargetSpec {
                kind: TargetKind::Creature(CardFilter::Any),
                min: 1,
                max: 1,
                distinct: true,
            }),
            kind: CounterKind::PlusOnePlusOne,
            n: ValueExpr::Fixed(1),
        },
        Effect::GainLife { who: PlayerRef::Controller, amount: ValueExpr::Fixed(1) },
    ]);
    db.insert(
        spell(
            HAVE_A_BITE,
            "Have a Bite",
            CardType::Sorcery,
            Color::Black,
            mana_cost(0, &[(Color::Black, 1)]),
            have_a_bite,
        )
        .with_text("Put a +1/+1 counter on target creature. You gain 1 life."),
    );

    // Front face — the creature; enters-prepared via a `SelfEnters` → `BecomePrepared` trigger.
    let mut front = creature(
        ADVENTUROUS_EATER,
        "Adventurous Eater",
        &[CreatureType::Human, CreatureType::Warlock],
        Color::Black,
        mana_cost(2, &[(Color::Black, 1)]),
        3,
        2,
        vec![
            Ability::Prepare { spell: HAVE_A_BITE },
            Ability::Triggered {
                event: EventPattern::SelfEnters,
                condition: None,
                intervening_if: false,
                effect: Effect::BecomePrepared,
            },
        ],
    );
    front.text = "This creature enters prepared. (While it's prepared, you may cast a copy of its spell. Doing so unprepares it.)\n// Have a Bite {B} Sorcery — Put a +1/+1 counter on target creature. You gain 1 life.".to_string();
    db.insert(front);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, CastVariant, DecisionRequest, DecisionResponse, PlayableAction, PlayerView};
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

    /// The front creature carries the Prepare marker (linking the back face) + the enters-prepared
    /// `SelfEnters → BecomePrepared` trigger; the back face is a registered copy-only spell def.
    #[test]
    fn adventurous_eater_ir() {
        let db = db_with_card();
        let front = db.get(ADVENTUROUS_EATER).unwrap();
        assert_eq!(front.chars.card_types, vec![CardType::Creature]);
        assert!(front.fully_implemented);
        expect![[r#"
            [
                Prepare {
                    spell: 9700,
                },
                Triggered {
                    event: SelfEnters,
                    condition: None,
                    intervening_if: false,
                    effect: BecomePrepared,
                },
            ]"#]]
        .assert_eq(&format!("{:#?}", front.abilities));
        let back = db.get(HAVE_A_BITE).unwrap();
        assert_eq!(back.chars.card_types, vec![CardType::Sorcery]);
    }

    /// Yes to any confirm; always targets candidate 0 of every slot.
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

    /// Full Prepare lifecycle through the REAL paths (CR 707.12 spell-copy consumer): cast the creature
    /// from hand → it enters and the `SelfEnters` trigger sets it **prepared** → `legal_actions` offers
    /// a `CastPrepared` at sorcery speed → taking it casts a **paid** copy of Have a Bite (targeting the
    /// creature), which resolves (+1/+1 counter + gain 1 life) and then **ceases to exist**, leaving the
    /// creature **unprepared** so the offer is gone.
    #[test]
    fn enters_prepared_then_casts_a_paid_copy_and_unprepares() {
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(db_with_card()));
        let eater = {
            let c = state.card_db().get(ADVENTUROUS_EATER).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Hand)
        };
        // {2}{B} to cast the creature + {B} for the copy = 4 black sources suffice.
        for _ in 0..4 {
            let s = state.card_db().get(grp::SWAMP).unwrap().chars.clone();
            state.add_card(PlayerId(0), s, Zone::Battlefield);
        }
        let mut e = Engine::new(state, vec![Box::new(PrepareAgent), Box::new(PrepareAgent)]);
        e.state.phase = Phase::PrecombatMain;

        // Cast the creature; resolve it (it enters), run the agenda (queue the ETB trigger onto the
        // stack), then resolve the trigger → the creature becomes prepared.
        e.cast_spell(PlayerId(0), eater, CastVariant::Normal);
        e.resolve_top();
        e.run_agenda();
        e.resolve_top();
        assert!(e.state.object(eater).prepared, "the SelfEnters trigger set it prepared");

        // While prepared, the sorcery-speed prepared cast is offered (masking).
        let offered = e.legal_actions(PlayerId(0));
        assert!(
            offered.iter().any(|a| matches!(a, PlayableAction::CastPrepared { source } if *source == eater)),
            "a prepared creature offers CastPrepared: {offered:?}"
        );

        // Take it: mint + pay for the copy of Have a Bite, targeting the eater, then resolve it.
        let objs_before = e.state.objects.len();
        let life_before = e.state.player(PlayerId(0)).life;
        e.cast_prepared(PlayerId(0), eater);
        e.resolve_top();

        assert_eq!(
            e.state.object(eater).counters.get(&CounterKind::PlusOnePlusOne),
            1,
            "the copy put a +1/+1 counter on the target creature"
        );
        assert_eq!(e.state.player(PlayerId(0)).life, life_before + 1, "the copy gained 1 life");
        assert!(!e.state.object(eater).prepared, "casting the copy unprepared the creature");
        assert_eq!(
            e.state.objects.len(),
            objs_before,
            "the copy ceased to exist (707.10a) — arena back to baseline"
        );
        assert!(
            !e.legal_actions(PlayerId(0))
                .iter()
                .any(|a| matches!(a, PlayableAction::CastPrepared { .. })),
            "no longer prepared → no CastPrepared offered"
        );
        // The back-face spell never touched a real zone (copy-only).
        assert!(e.state.player(PlayerId(0)).graveyard.is_empty(), "a copy never hits the graveyard");
    }
}
