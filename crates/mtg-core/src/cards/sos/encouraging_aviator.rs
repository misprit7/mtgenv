//! Encouraging Aviator // Jump — `{2}{U}` Creature — Bird Wizard 2/3 // `{U}` Instant
//! (first printed SOS). A **Prepare** DFC — the "becomes prepared on attack" variant.
//!
//! Front oracle: "Flying. Whenever this creature attacks, it becomes prepared. (While it's prepared,
//! you may cast a copy of its spell. Doing so unprepares it.)"
//! Back oracle (Jump): "Target creature gains flying until end of turn."
//!
//! **Fully implemented** — printed Flying plus a `SelfAttacks → BecomePrepared` trigger (an ordinary
//! attack trigger; no bespoke prepare machinery). The back face Jump is an **instant**, so the
//! while-prepared cast is offered at instant speed. Because the trigger re-fires on each attack, this
//! card is **re-preparable**: attack → prepared → cast the copy (unprepares) → attack again → prepared.

use crate::basics::{CardType, Color};
use crate::cards::{creature, mana_cost, spell, CardDb};
use crate::effects::ability::{Ability, EventPattern, Keyword};
use crate::effects::condition::Duration;
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::{Effect, EffectTarget};
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const ENCOURAGING_AVIATOR: u32 = 374;
/// The copy-only back-face spell (reserved 9700+ Prepare block).
pub const JUMP: u32 = 9701;

pub fn register(db: &mut CardDb) {
    // Back face — "Jump" ({U} Instant): target creature gains flying until end of turn.
    let jump = Effect::GrantKeyword {
        what: EffectTarget::Target(TargetSpec {
            kind: TargetKind::Creature(CardFilter::Any),
            min: 1,
            max: 1,
            distinct: true,
        }),
        keyword: Keyword::Flying,
        duration: Duration::UntilEndOfTurn,
    };
    db.insert(
        spell(JUMP, "Jump", CardType::Instant, Color::Blue, mana_cost(0, &[(Color::Blue, 1)]), jump)
            .with_text("Target creature gains flying until end of turn."),
    );

    // Front face — Flying creature; becomes prepared whenever it attacks.
    let mut front = creature(
        ENCOURAGING_AVIATOR,
        "Encouraging Aviator",
        &[CreatureType::Bird, CreatureType::Wizard],
        Color::Blue,
        mana_cost(2, &[(Color::Blue, 1)]),
        2,
        3,
        vec![
            Ability::Prepare { spell: JUMP },
            Ability::Triggered {
                event: EventPattern::SelfAttacks,
                condition: None,
                intervening_if: false,
                effect: Effect::BecomePrepared,
            },
        ],
    );
    front.chars.keywords = vec![Keyword::Flying];
    front.text = "Flying\nWhenever this creature attacks, it becomes prepared. (While it's prepared, you may cast a copy of its spell. Doing so unprepares it.)\n// Jump {U} Instant — Target creature gains flying until end of turn.".to_string();
    db.insert(front);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, DecisionRequest, DecisionResponse, GameEvent, PlayableAction, PlayerView};
    use crate::basics::{Phase, Target, Zone};
    use crate::cards::grp;
    use crate::ids::{ObjId, PlayerId};
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
    fn encouraging_aviator_ir() {
        let db = db_with_card();
        let front = db.get(ENCOURAGING_AVIATOR).unwrap();
        assert_eq!(front.chars.keywords, vec![Keyword::Flying]);
        expect![[r#"
            [
                Prepare {
                    spell: 9701,
                },
                Triggered {
                    event: SelfAttacks,
                    condition: None,
                    intervening_if: false,
                    effect: BecomePrepared,
                },
            ]"#]]
        .assert_eq(&format!("{:#?}", front.abilities));
    }

    /// Yes to confirms; targets a specific object id in every slot (falls back to candidate 0).
    struct TargetAgent(ObjId);
    impl Agent for TargetAgent {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::Confirm { .. } => DecisionResponse::Bool(true),
                DecisionRequest::ChooseTargets { slots, .. } => DecisionResponse::Pairs(
                    slots
                        .iter()
                        .enumerate()
                        .map(|(si, slot)| {
                            let idx = slot
                                .legal
                                .iter()
                                .position(|t| matches!(t, Target::Object(o) if *o == self.0))
                                .unwrap_or(0) as u32;
                            (si as u32, idx)
                        })
                        .collect(),
                ),
                _ => DecisionResponse::Pass,
            }
        }
    }

    /// SelfAttacks → prepared → cast the **instant-speed** copy of Jump (targeting the bears, which
    /// gains flying) → unprepared → attack again → prepared again (re-preparable).
    #[test]
    fn attacks_prepares_casts_copy_then_reprepares() {
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(db_with_card()));
        let aviator = {
            let c = state.card_db().get(ENCOURAGING_AVIATOR).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        let bears = {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        // Two Islands: one {U} per prepared copy.
        for _ in 0..2 {
            let i = state.card_db().get(grp::ISLAND).unwrap().chars.clone();
            state.add_card(PlayerId(0), i, Zone::Battlefield);
        }
        let mut e = Engine::new(state, vec![Box::new(TargetAgent(bears)), Box::new(TargetAgent(bears))]);
        e.state.phase = Phase::DeclareAttackers;

        // Attack → the SelfAttacks trigger fires and prepares the aviator.
        e.broadcast(GameEvent::AttackersDeclared { attackers: vec![aviator], by: PlayerId(0) });
        e.run_agenda();
        e.resolve_top();
        assert!(e.state.object(aviator).prepared, "attacking prepared it");
        assert!(
            e.legal_actions(PlayerId(0))
                .iter()
                .any(|a| matches!(a, PlayableAction::CastPrepared { source } if *source == aviator)),
            "instant-speed back → CastPrepared offered even outside a main phase"
        );

        // Cast the copy of Jump on the bears; it gains flying, and the aviator is unprepared.
        assert!(!e.state.computed(bears).has_keyword(Keyword::Flying));
        e.cast_prepared(PlayerId(0), aviator);
        e.resolve_top();
        assert!(e.state.computed(bears).has_keyword(Keyword::Flying), "the Jump copy granted flying");
        assert!(!e.state.object(aviator).prepared, "casting the copy unprepared it");

        // Attack again → it re-prepares (the trigger fires every attack).
        e.broadcast(GameEvent::AttackersDeclared { attackers: vec![aviator], by: PlayerId(0) });
        e.run_agenda();
        e.resolve_top();
        assert!(e.state.object(aviator).prepared, "re-preparable — attacking prepares it again");
    }
}
