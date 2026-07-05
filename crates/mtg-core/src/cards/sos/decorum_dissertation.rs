//! Decorum Dissertation — `{3}{B}{B}` Sorcery — Lesson (first printed SOS).
//!
//! Oracle: "Target player draws two cards and loses 2 life. Paradigm (Then exile this spell. After
//! you first resolve a spell with this name, you may cast a copy of it from exile without paying its
//! mana cost at the beginning of each of your first main phases.)"
//!
//! **Fully implemented.** The underlying effect is a single "target player" slot (CR 115.1) → draw 2
//! + lose 2 life. **Paradigm** is the shared bundle from [`crate::cards::helpers::paradigm_abilities`]
//! (the spell-copy subsystem, CR 707.12): on resolve the Lesson exiles itself, and thereafter at the
//! beginning of each of your first main phases you may cast a free copy of it from exile.

use crate::basics::{CardType, Color};
use crate::cards::{helpers, mana_cost, spell, CardDb};
use crate::effects::target::PlayerFilter;
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;
use crate::subtypes::{SpellType, Subtype};

/// grp id (per-set ids live near their cards).
pub const DECORUM_DISSERTATION: u32 = 367;

pub fn register(db: &mut CardDb) {
    let effect = Effect::Sequence(vec![
        // "Target player" (CR 115.1) — the slot the draw/lose reference via `ChosenTarget(0)`.
        Effect::TargetPlayer(PlayerFilter::Any),
        Effect::Draw { who: PlayerRef::ChosenTarget(0), count: ValueExpr::Fixed(2) },
        Effect::LoseLife { who: PlayerRef::ChosenTarget(0), amount: ValueExpr::Fixed(2) },
    ]);
    let mut def = spell(
        DECORUM_DISSERTATION,
        "Decorum Dissertation",
        CardType::Sorcery,
        Color::Black,
        mana_cost(3, &[(Color::Black, 2)]),
        effect,
    )
    .with_text(
        "Target player draws two cards and loses 2 life. Paradigm (Then exile this spell. After you first resolve a spell with this name, you may cast a copy of it from exile without paying its mana cost at the beginning of each of your first main phases.)",
    );
    def.chars.subtypes = vec![Subtype::Spell(SpellType::Lesson)];
    def.abilities.extend(helpers::paradigm_abilities());
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, CastVariant, DecisionRequest, DecisionResponse, PlayerView};
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
    fn decorum_dissertation_ir() {
        let db = db_with_card();
        let def = db.get(DECORUM_DISSERTATION).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Sorcery]);
        assert_eq!(def.chars.subtypes, vec![Subtype::Spell(SpellType::Lesson)]);
        assert!(def.fully_implemented);
        // Spell ability + the 3-part Paradigm bundle (marker / functions-from-exile / recurring cast).
        expect![[r#"
            [
                Spell {
                    effect: Sequence(
                        [
                            TargetPlayer(
                                Any,
                            ),
                            Draw {
                                who: ChosenTarget(
                                    0,
                                ),
                                count: Fixed(
                                    2,
                                ),
                            },
                            LoseLife {
                                who: ChosenTarget(
                                    0,
                                ),
                                amount: Fixed(
                                    2,
                                ),
                            },
                        ],
                    ),
                },
                Paradigm,
                FunctionsFrom(
                    [
                        Exile,
                    ],
                ),
                Triggered {
                    event: BeginningOfStep(
                        PrecombatMain,
                    ),
                    condition: None,
                    intervening_if: false,
                    effect: Optional {
                        prompt: "Cast a copy of this Lesson from exile without paying its mana cost?",
                        body: CastCopy {
                            source: SourceSelf,
                            controller: Controller,
                        },
                    },
                },
            ]"#]]
        .assert_eq(&format!("{:#?}", def.abilities));
    }

    /// Says "yes" to the optional Paradigm recast and always targets player slot 0 (self, P0).
    struct ParadigmAgent;
    impl Agent for ParadigmAgent {
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

    /// Full Paradigm lifecycle (CR 707.12 spell-copy): cast the Lesson → it resolves (draw 2 / lose 2)
    /// and **exiles itself** → on the next first main phase the exile-functioning trigger offers a free
    /// **copy**, which resolves the same effect again and then **ceases to exist**, while the original
    /// Lesson stays in exile for the turn after that.
    #[test]
    fn paradigm_self_exiles_then_recasts_a_free_copy_each_first_main() {
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(db_with_card()));
        // P0 casts it targeting itself; needs 4 library cards (2 per resolution) and {3}{B}{B} of mana.
        let card = {
            let c = state.card_db().get(DECORUM_DISSERTATION).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Hand)
        };
        for _ in 0..4 {
            let f = state.card_db().get(grp::FOREST).unwrap().chars.clone();
            state.add_card(PlayerId(0), f, Zone::Library);
        }
        for _ in 0..5 {
            let s = state.card_db().get(grp::SWAMP).unwrap().chars.clone();
            state.add_card(PlayerId(0), s, Zone::Battlefield);
        }
        let mut e = Engine::new(state, vec![Box::new(ParadigmAgent), Box::new(ParadigmAgent)]);
        e.state.phase = Phase::PrecombatMain;

        // Cast the original (paying {3}{B}{B}) targeting P0, then resolve it.
        e.cast_spell(PlayerId(0), card, CastVariant::Normal);
        e.resolve_top();
        assert_eq!(e.state.player(PlayerId(0)).hand.len(), 2, "drew 2");
        assert_eq!(e.state.player(PlayerId(0)).life, 18, "lost 2 life");
        assert!(
            e.state.player(PlayerId(0)).exile.contains(&card),
            "Paradigm exiled the Lesson instead of the graveyard"
        );
        assert!(e.state.player(PlayerId(0)).graveyard.is_empty(), "not in the graveyard");

        // The next first main phase: the exile-functioning trigger fires, is accepted, and casts a
        // free copy that resolves the effect again.
        let objs_before_copy = e.state.objects.len();
        e.broadcast(crate::agent::GameEvent::PhaseBegan {
            turn: 3,
            phase: Phase::PrecombatMain,
            active: PlayerId(0),
        });
        e.run_agenda(); // stacks the exile-functioning trigger
        e.resolve_top(); // resolve the trigger → optional yes → cast a free copy
        e.run_agenda();
        e.resolve_top(); // resolve the copy → draw 2 / lose 2, then it ceases to exist

        assert_eq!(e.state.player(PlayerId(0)).hand.len(), 4, "the copy drew 2 more");
        assert_eq!(e.state.player(PlayerId(0)).life, 16, "the copy cost 2 more life");
        assert!(
            e.state.player(PlayerId(0)).exile.contains(&card),
            "the original Lesson is still in exile — the recast is repeatable"
        );
        assert_eq!(
            e.state.objects.len(),
            objs_before_copy,
            "the copy ceased to exist (707.10a) — the object arena is back to baseline"
        );
        assert!(e.state.player(PlayerId(0)).graveyard.is_empty(), "a copy never hits the graveyard");
    }
}
