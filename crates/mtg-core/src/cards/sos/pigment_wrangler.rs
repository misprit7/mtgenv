//! Pigment Wrangler // Striking Palette — `{4}{R}` Creature — Orc Sorcerer 4/4 (Flying) // `{R}`
//! Sorcery (first printed SOS). A **Prepare** DFC whose back face is a **copy-a-spell** effect.
//!
//! Front oracle: "Flying. This creature enters prepared. (While it's prepared, you may cast a copy of
//! its spell. Doing so unprepares it.)"
//! Back oracle (Striking Palette): "When you next cast an instant or sorcery spell this turn, copy
//! that spell. You may choose new targets for the copy."
//!
//! **Fully implemented** — the front is the usual Prepare rails (a 4/4 flier carrying `Ability::Prepare`
//! + a `SelfEnters → BecomePrepared` trigger). The back is the lander for **CR 707.10 copy-a-spell-ON-
//! the-stack**: casting the prepared copy of Striking Palette resolves [`Effect::CopyNextSpellCast`],
//! which arms a one-shot delayed trigger ([`crate::effects::action::DelayedTriggerEvent::YouCastSpell`])
//! on the controller. The next instant/sorcery they cast this turn fires a
//! [`crate::stack::StackObjectKind::SpellCopyTrigger`] over that spell; on resolution the engine mints
//! an `is_copy` copy on the stack (with an optional "choose new targets" reselection) that is **not
//! cast** — no cast triggers, distinct from `Effect::CastCopy`'s 707.12 mint+cast.

use crate::basics::{CardType, Color};
use crate::cards::{creature, helpers, mana_cost, spell, CardDb};
use crate::effects::ability::Keyword;
use crate::effects::target::CardFilter;
use crate::effects::Effect;
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const PIGMENT_WRANGLER: u32 = 401;
/// The copy-only back-face spell (reserved 9700+ Prepare block).
pub const STRIKING_PALETTE: u32 = 9727;

/// "an instant or sorcery spell" — the filter the delayed copy trigger watches the next cast against.
fn instant_or_sorcery() -> CardFilter {
    CardFilter::AnyOf(vec![
        CardFilter::HasCardType(CardType::Instant),
        CardFilter::HasCardType(CardType::Sorcery),
    ])
}

pub fn register(db: &mut CardDb) {
    // Back face — "Striking Palette" ({R} Sorcery): arm the "copy your next I/S spell this turn" delayed
    // trigger (CR 707.10), offering new targets for the copy (707.10c).
    db.insert(
        spell(
            STRIKING_PALETTE,
            "Striking Palette",
            CardType::Sorcery,
            Color::Red,
            mana_cost(0, &[(Color::Red, 1)]),
            Effect::CopyNextSpellCast {
                filter: instant_or_sorcery(),
                choose_new_targets: true,
            },
        )
        .with_text(
            "When you next cast an instant or sorcery spell this turn, copy that spell. You may choose new targets for the copy.",
        ),
    );

    // Front face — the 4/4 flier; enters-prepared via a `SelfEnters → BecomePrepared` trigger.
    let mut front = creature(
        PIGMENT_WRANGLER,
        "Pigment Wrangler",
        &[CreatureType::Orc, CreatureType::Sorcerer],
        Color::Red,
        mana_cost(4, &[(Color::Red, 1)]),
        4,
        4,
        helpers::enters_prepared(STRIKING_PALETTE),
    );
    front.chars.keywords = vec![Keyword::Flying];
    front.text = "Flying\nThis creature enters prepared. (While it's prepared, you may cast a copy of its spell. Doing so unprepares it.)\n// Striking Palette {R} Sorcery — When you next cast an instant or sorcery spell this turn, copy that spell. You may choose new targets for the copy.".to_string();
    db.insert(front);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, CastVariant, DecisionRequest, DecisionResponse, PlayableAction, PlayerView};
    use crate::basics::{Phase, Target, Zone};
    use crate::cards::grp;
    use crate::effects::ability::{Ability, EventPattern};
    use crate::ids::PlayerId;
    use crate::priority::Engine;
    use crate::state::GameState;
    use crate::stack::StackObjectKind;
    use expect_test::expect;
    use std::sync::Arc;

    fn db_with_card() -> CardDb {
        let mut db = crate::cards::starter_db();
        register(&mut db);
        db
    }

    #[test]
    fn pigment_wrangler_ir() {
        let db = db_with_card();
        let front = db.get(PIGMENT_WRANGLER).unwrap();
        assert_eq!(front.chars.card_types, vec![CardType::Creature]);
        assert_eq!(front.chars.keywords, vec![Keyword::Flying]);
        assert!(front.fully_implemented);
        assert!(matches!(front.abilities[0], Ability::Prepare { spell: STRIKING_PALETTE }));
        assert!(matches!(
            front.abilities[1],
            Ability::Triggered { event: EventPattern::SelfEnters, .. }
        ));
        expect![[r#"
            CopyNextSpellCast {
                filter: AnyOf(
                    [
                        HasCardType(
                            Instant,
                        ),
                        HasCardType(
                            Sorcery,
                        ),
                    ],
                ),
                choose_new_targets: true,
            }"#]]
        .assert_eq(&format!("{:#?}", db.get(STRIKING_PALETTE).unwrap().spell_effect().unwrap()));
    }

    /// Says yes to confirms; for `ChooseTargets` always picks the candidate that is `Target::Player(1)`
    /// (falling back to slot-0 candidate 0), so both the original bolt and its copy hit the opponent.
    struct BoltP1Agent;
    impl Agent for BoltP1Agent {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::Confirm { .. } => DecisionResponse::Bool(true),
                DecisionRequest::ChooseTargets { slots, .. } => {
                    let idx = slots[0]
                        .legal
                        .iter()
                        .position(|t| *t == Target::Player(PlayerId(1)))
                        .unwrap_or(0);
                    DecisionResponse::Pairs(vec![(0, idx as u32)])
                }
                _ => DecisionResponse::Pass,
            }
        }
    }

    /// The headline (CR 707.10 copy-a-spell-on-the-stack): a prepared Pigment Wrangler casts a copy of
    /// Striking Palette → it arms "copy your next I/S this turn." The controller then casts Lightning
    /// Bolt at the opponent → the delayed trigger fires a copy over the bolt; the copy (new targets
    /// offered → also the opponent) resolves for 3 and ceases to exist, then the original bolt for 3.
    /// The opponent takes 6, and only the real bolt (not the copy) reaches a graveyard.
    #[test]
    fn copies_the_next_instant_you_cast() {
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(db_with_card()));
        // Pigment Wrangler already on the battlefield, prepared, for P0.
        let wrangler = {
            let c = state.card_db().get(PIGMENT_WRANGLER).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        state.objects.get_mut(&wrangler).unwrap().prepared = true;
        // Lightning Bolt in hand + mana: {R} for the Striking Palette copy + {R} for the bolt.
        let bolt = {
            let c = state.card_db().get(grp::LIGHTNING_BOLT).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Hand)
        };
        for _ in 0..2 {
            let m = state.card_db().get(grp::MOUNTAIN).unwrap().chars.clone();
            state.add_card(PlayerId(0), m, Zone::Battlefield);
        }
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let p1_life = state.player(PlayerId(1)).life;
        let mut e = Engine::new(state, vec![Box::new(BoltP1Agent), Box::new(BoltP1Agent)]);

        // Cast the prepared copy of Striking Palette and resolve it → arms the delayed copy trigger.
        e.cast_prepared(PlayerId(0), wrangler);
        e.resolve_top();
        assert!(!e.state.object(wrangler).prepared, "casting the copy unprepared the creature");
        assert_eq!(e.state.delayed_triggers.len(), 1, "the 'copy your next I/S' trigger is armed");

        // Cast Lightning Bolt at the opponent; the delayed trigger fires a copy over it.
        e.cast_spell(PlayerId(0), bolt, CastVariant::Normal);
        e.run_agenda(); // put the SpellCopyTrigger on the stack (above the bolt)
        assert!(
            e.state.stack.items.iter().any(|s| matches!(s.kind, StackObjectKind::SpellCopyTrigger { .. })),
            "the copy trigger is on the stack above the bolt"
        );
        assert!(e.state.delayed_triggers.is_empty(), "one-shot: the delayed trigger was consumed");

        e.resolve_top(); // copy trigger resolves → mints the bolt copy on the stack
        let copies = e.state.stack.items.iter().filter(|s| matches!(s.kind, StackObjectKind::Spell(_))).count();
        assert_eq!(copies, 2, "a copy of the bolt now sits above the original");
        e.resolve_top(); // the copy resolves (3 to P1), then ceases to exist
        e.resolve_top(); // the original bolt resolves (3 to P1)
        e.run_agenda();

        assert_eq!(
            e.state.player(PlayerId(1)).life,
            p1_life - 6,
            "both the bolt and its copy hit the opponent for 3"
        );
        // The copy never reaches a graveyard (707.10a); the real bolt does.
        assert!(e.state.player(PlayerId(0)).graveyard.contains(&bolt), "the real bolt is in the graveyard");
        assert_eq!(
            e.state.player(PlayerId(0)).graveyard.len(),
            1,
            "only the real bolt hit the graveyard — the copy ceased to exist"
        );
    }

    /// A prepared Pigment Wrangler offers `CastPrepared` at sorcery speed (masking).
    #[test]
    fn prepared_offers_cast_prepared() {
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(db_with_card()));
        let wrangler = {
            let c = state.card_db().get(PIGMENT_WRANGLER).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        state.objects.get_mut(&wrangler).unwrap().prepared = true;
        let m = state.card_db().get(grp::MOUNTAIN).unwrap().chars.clone();
        state.add_card(PlayerId(0), m, Zone::Battlefield);
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let e = Engine::new(state, vec![Box::new(BoltP1Agent), Box::new(BoltP1Agent)]);
        assert!(
            e.legal_actions(PlayerId(0))
                .iter()
                .any(|a| matches!(a, PlayableAction::CastPrepared { source } if *source == wrangler)),
            "a prepared creature offers CastPrepared"
        );
    }
}
