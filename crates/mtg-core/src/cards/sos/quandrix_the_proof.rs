//! Quandrix, the Proof — `{4}{G}{U}` Legendary Creature — Elder Dragon 6/6.
//!
//! Oracle: "Flying, trample. Cascade (When you cast this spell, exile cards from the top of your
//! library until you exile a nonland card that costs less. You may cast it without paying its mana
//! cost. Put the exiled cards on the bottom in a random order.) Instant and sorcery spells you cast
//! from your hand have cascade."
//!
//! **Fully implemented** — the Quandrix (Cascade) Elder Dragon, over the new cascade machinery:
//! - **6/6 flying, trample** body.
//! - **Cascade on itself** = `Triggered{ SelfCast → Cascade }` (the cascade keyword; fires when
//!   Quandrix is cast, threshold = its own MV 6).
//! - **Granted cascade to your I/S** = `Triggered{ SpellCast(instant|sorcery) → Cascade }` — a
//!   watcher on the battlefield Quandrix; when you cast an I/S spell it cascades using that spell's MV
//!   (via `ctx.triggering_spell`).
//!
//! ⚠️ Caveat: the "from your hand" restriction on the granted clause is NOT enforced (the cast-zone
//! isn't threaded through the SpellCast broadcast). It over-triggers only on the rare I/S spell cast
//! from a non-hand zone (flashback/exile) — none in the current pool. Noted for a cast-zone cap.

use crate::basics::Color;
use crate::cards::helpers::instant_or_sorcery;
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern, Keyword};
use crate::effects::Effect;
use crate::subtypes::{CreatureType, Supertype};

/// grp id (per-set ids live near their cards).
pub const QUANDRIX_THE_PROOF: u32 = 426;

/// The cascade trigger, shared by the own (SelfCast) and granted (SpellCast I/S) clauses.
fn cascade(event: EventPattern) -> Ability {
    Ability::Triggered { event, condition: None, intervening_if: false, effect: Effect::Cascade }
}

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        QUANDRIX_THE_PROOF,
        "Quandrix, the Proof",
        &[CreatureType::Elder, CreatureType::Dragon],
        Color::Green,
        mana_cost(4, &[(Color::Green, 1), (Color::Blue, 1)]),
        6,
        6,
        vec![
            cascade(EventPattern::SelfCast),                          // own cascade
            cascade(EventPattern::SpellCast(instant_or_sorcery())),  // granted to your I/S
        ],
    );
    def.chars.supertypes = vec![Supertype::Legendary];
    def.chars.colors = vec![Color::Green, Color::Blue];
    def.chars.keywords = vec![Keyword::Flying, Keyword::Trample];
    def.text = "Flying, trample\nCascade (When you cast this spell, exile cards from the top of your library until you exile a nonland card that costs less. You may cast it without paying its mana cost. Put the exiled cards on the bottom in a random order.)\nInstant and sorcery spells you cast from your hand have cascade.".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, CastVariant, ConfirmKind, DecisionRequest, DecisionResponse, PlayerView};
    use crate::basics::{CardType, Phase, Zone};
    use crate::cards::{grp, spell, starter_db};
    use crate::effects::value::{PlayerRef, ValueExpr};
    use crate::ids::{ObjId, PlayerId};
    use crate::priority::Engine;
    use crate::state::GameState;
    use expect_test::expect;
    use std::sync::Arc;

    /// A test-only `{4}` sorcery whose effect never touches the library (so cascade assertions aren't
    /// muddied by a draw) — a clean "you gain 1 life" I/S with MV 4 for the granted-cascade test.
    const TEST_RITUAL: u32 = 990_426;

    fn db_with_card() -> CardDb {
        let mut db = starter_db();
        register(&mut db);
        db.insert(
            spell(
                TEST_RITUAL,
                "Test Ritual",
                CardType::Sorcery,
                Color::Blue,
                mana_cost(4, &[]),
                Effect::GainLife { who: PlayerRef::Controller, amount: ValueExpr::Fixed(1) },
            ),
        );
        db
    }

    #[test]
    fn quandrix_shape() {
        let db = db_with_card();
        let def = db.get(QUANDRIX_THE_PROOF).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Creature]);
        assert_eq!(def.chars.supertypes, vec![Supertype::Legendary]);
        assert_eq!(def.chars.colors, vec![Color::Green, Color::Blue]);
        assert_eq!(def.chars.keywords, vec![Keyword::Flying, Keyword::Trample]);
        assert_eq!((def.chars.power, def.chars.toughness), (Some(6), Some(6)));
        assert!(def.fully_implemented);
    }

    #[test]
    fn cascade_ir() {
        let db = db_with_card();
        let def = db.get(QUANDRIX_THE_PROOF).unwrap();
        expect![[r#"
            [
                Triggered {
                    event: SelfCast,
                    condition: None,
                    intervening_if: false,
                    effect: Cascade,
                },
                Triggered {
                    event: SpellCast(
                        AnyOf(
                            [
                                HasCardType(
                                    Instant,
                                ),
                                HasCardType(
                                    Sorcery,
                                ),
                            ],
                        ),
                    ),
                    condition: None,
                    intervening_if: false,
                    effect: Cascade,
                },
            ]"#]]
        .assert_eq(&format!("{:#?}", def.abilities));
    }

    /// Agent: says yes to the cascade "you may cast it" confirm.
    #[derive(Clone)]
    struct CascadeAgent {
        cast_hit: bool,
    }
    impl Agent for CascadeAgent {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::Confirm { kind: ConfirmKind::MayEffect } => {
                    DecisionResponse::Bool(self.cast_hit)
                }
                DecisionRequest::Confirm { .. } => DecisionResponse::Bool(true),
                _ => DecisionResponse::Pass,
            }
        }
    }

    fn drive(e: &mut Engine) {
        loop {
            e.run_agenda();
            if e.state.stack.items.is_empty() {
                break;
            }
            e.resolve_top();
        }
    }

    /// Put `n` untapped Islands + `n_forest` Forests on P0's battlefield to pay costs.
    fn add_lands(state: &mut GameState, islands: usize, forests: usize) {
        for _ in 0..islands {
            let c = state.card_db().get(grp::ISLAND).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield);
        }
        for _ in 0..forests {
            let c = state.card_db().get(grp::FOREST).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield);
        }
    }

    /// Cascade on Quandrix itself (SelfCast, threshold MV 6): casting Quandrix exiles from the top
    /// until a nonland cheaper than {6} — a Grizzly Bears ({1}{G}, MV 2) — which is cast for free; the
    /// exiled Forest is bottomed. Exercises BOTH the SelfCast trigger and Effect::Cascade.
    #[test]
    fn self_cast_cascade_casts_the_cheaper_card() {
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(db_with_card()));
        // Library (bottom → top): [Grizzly Bears, Forest] — so Forest (land) is exiled first, then the
        // Bears (the cheaper-than-{6} nonland hit).
        let bears = {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Library)
        };
        let forest_in_lib = {
            let c = state.card_db().get(grp::FOREST).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Library)
        };
        let quandrix = {
            let c = state.card_db().get(QUANDRIX_THE_PROOF).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Hand)
        };
        add_lands(&mut state, 5, 1); // {4}{G}{U}: 4 generic + {U} (Islands) + {G} (Forest)
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let mut e = Engine::new(state, vec![Box::new(CascadeAgent { cast_hit: true }), Box::new(CascadeAgent { cast_hit: true })]);

        e.cast_spell(PlayerId(0), quandrix, CastVariant::Normal);
        drive(&mut e);

        assert!(e.state.player(PlayerId(0)).battlefield.contains(&bears), "the cascade cast the Bears");
        assert!(e.state.player(PlayerId(0)).battlefield.contains(&quandrix), "Quandrix resolved");
        // The exiled Forest went to the bottom of the library (nothing else left in it).
        assert_eq!(e.state.player(PlayerId(0)).library, vec![forest_in_lib], "the Forest was bottomed");
        // Nothing stranded in exile (the Bears left exile onto the stack; the Forest onto the library).
        assert!(e.state.player(PlayerId(0)).exile.is_empty(), "cascade leaves nothing in exile");
    }

    /// Declining the free cast: the cheaper card is NOT cast; all exiled cards (Bears + Forest) go to
    /// the bottom of the library.
    #[test]
    fn declining_cascade_bottoms_everything() {
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(db_with_card()));
        let bears = {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Library)
        };
        let _forest = {
            let c = state.card_db().get(grp::FOREST).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Library)
        };
        let quandrix = {
            let c = state.card_db().get(QUANDRIX_THE_PROOF).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Hand)
        };
        add_lands(&mut state, 5, 1);
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let mut e = Engine::new(state, vec![Box::new(CascadeAgent { cast_hit: false }), Box::new(CascadeAgent { cast_hit: false })]);

        e.cast_spell(PlayerId(0), quandrix, CastVariant::Normal);
        drive(&mut e);

        assert!(!e.state.player(PlayerId(0)).battlefield.contains(&bears), "declined → Bears not cast");
        assert_eq!(e.state.player(PlayerId(0)).library.len(), 2, "both exiled cards bottomed");
        assert!(e.state.player(PlayerId(0)).exile.is_empty(), "nothing stranded in exile");
        assert!(e.state.player(PlayerId(0)).battlefield.contains(&quandrix), "Quandrix resolved");
    }

    /// Granted cascade: with Quandrix on the battlefield, casting a {4} instant/sorcery (Test Ritual,
    /// MV 4) cascades using THAT spell's MV — exiling until a nonland under {4} (the Bears) and casting
    /// it. Exercises the granted SpellCast(I/S) → Cascade watcher clause.
    #[test]
    fn granted_cascade_on_your_instant_sorcery() {
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(db_with_card()));
        {
            let c = state.card_db().get(QUANDRIX_THE_PROOF).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield);
        }
        let bears = {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Library)
        };
        {
            let c = state.card_db().get(grp::FOREST).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Library);
        }
        let ritual: ObjId = {
            let c = state.card_db().get(TEST_RITUAL).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Hand)
        };
        add_lands(&mut state, 4, 0); // {4}
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let mut e = Engine::new(state, vec![Box::new(CascadeAgent { cast_hit: true }), Box::new(CascadeAgent { cast_hit: true })]);

        e.cast_spell(PlayerId(0), ritual, CastVariant::Normal);
        drive(&mut e);

        assert!(e.state.player(PlayerId(0)).battlefield.contains(&bears), "granted cascade cast the Bears");
        assert!(e.state.player(PlayerId(0)).graveyard.contains(&ritual), "the ritual resolved to gy");
    }
}
