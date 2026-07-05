//! Emeritus of Conflict // Lightning Bolt — `{1}{R}` Creature — Human Wizard 2/2 (First strike) //
//! `{R}` Instant (first printed SOS). A **Prepare** DFC — the "cast your third spell each turn" variant.
//!
//! Front: "First strike. Whenever you cast your third spell each turn, this creature becomes prepared."
//! Back (Lightning Bolt): "Lightning Bolt deals 3 damage to any target."
//!
//! **Fully implemented** — First strike plus a `SpellCast(any)` trigger gated to *exactly* your third
//! spell (`SpellsCastThisTurn == 3`, via `All(≥3, Not(≥4))`) whose effect is [`Effect::BecomePrepared`].
//! The back is Lightning Bolt (3 damage to any target).

use crate::basics::{CardType, Color};
use crate::cards::{creature, deal_to_any, helpers, mana_cost, spell, CardDb};
use crate::effects::ability::{EventPattern, Keyword};
use crate::effects::condition::Condition;
use crate::effects::target::CardFilter;
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::subtypes::CreatureType;

pub const EMERITUS_OF_CONFLICT: u32 = 399;
pub const LIGHTNING_BOLT_BACK: u32 = 9726;

pub fn register(db: &mut CardDb) {
    db.insert(
        spell(LIGHTNING_BOLT_BACK, "Lightning Bolt", CardType::Instant, Color::Red, mana_cost(0, &[(Color::Red, 1)]), deal_to_any(3))
            .with_text("Lightning Bolt deals 3 damage to any target."),
    );
    // "your third spell each turn" — fires only when the running count is exactly 3.
    let third = Condition::All(vec![
        Condition::ValueAtLeast(ValueExpr::SpellsCastThisTurn { who: PlayerRef::Controller }, ValueExpr::Fixed(3)),
        Condition::Not(Box::new(Condition::ValueAtLeast(
            ValueExpr::SpellsCastThisTurn { who: PlayerRef::Controller },
            ValueExpr::Fixed(4),
        ))),
    ]);
    let mut front = creature(
        EMERITUS_OF_CONFLICT,
        "Emeritus of Conflict",
        &[CreatureType::Human, CreatureType::Wizard],
        Color::Red,
        mana_cost(1, &[(Color::Red, 1)]),
        2,
        2,
        // intervening_if: true — a SpellCast trigger's condition is only enforced at resolution
        // (`queue_watching_spellcast_triggers` doesn't check it at queue, unlike begin-of-step).
        helpers::prepared_abilities(LIGHTNING_BOLT_BACK, EventPattern::SpellCast(CardFilter::Any), Some(third), true),
    );
    front.chars.keywords = vec![Keyword::FirstStrike];
    front.text = "First strike\nWhenever you cast your third spell each turn, this creature becomes prepared. (While it's prepared, you may cast a copy of its spell. Doing so unprepares it.)\n// Lightning Bolt {R} Instant — Lightning Bolt deals 3 damage to any target.".to_string();
    db.insert(front);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::RandomAgent;
    use crate::basics::{Target, Zone};
    use crate::cards::{build_game, grp};
    use crate::effects::ability::Ability;
    use crate::effects::action::{ResolutionCtx, WbReason};
    use crate::ids::{PlayerId, StackId};
    use crate::priority::Engine;

    #[test]
    fn emeritus_of_conflict_ir_and_bolt() {
        let mut db = CardDb::default();
        register(&mut db);
        let f = db.get(EMERITUS_OF_CONFLICT).unwrap();
        assert_eq!(f.chars.keywords, vec![Keyword::FirstStrike]);
        assert!(matches!(f.abilities[0], Ability::Prepare { spell: LIGHTNING_BOLT_BACK }));
        assert!(matches!(f.abilities[1], Ability::Triggered { event: EventPattern::SpellCast(_), .. }));
        // Behaviour: the back bolt deals 3 to a creature.
        let mut state = build_game(1, &[&[], &[]]);
        let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
        let bears = state.add_card(PlayerId(0), c, Zone::Battlefield);
        let effect = state.card_db().get(LIGHTNING_BOLT_BACK).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
        e.resolve_effect(
            &effect,
            &ResolutionCtx {
                controller: Some(PlayerId(0)),
                chosen_targets: vec![Target::Object(bears)],
                ..Default::default()
            },
            WbReason::Resolve(StackId(0)),
        );
        assert_eq!(e.state.object(bears).damage_marked, 3, "bolt dealt 3 to the creature");
    }

    /// Real cast path: it becomes prepared only when the THIRD spell is cast this turn, not the 1st/2nd.
    #[test]
    fn becomes_prepared_only_on_the_third_spell() {
        use crate::agent::{Agent, CastVariant, DecisionRequest, DecisionResponse, PlayerView};
        use crate::basics::Phase;
        use crate::state::GameState;
        use std::sync::Arc;

        struct Pass;
        impl Agent for Pass {
            fn decide(&mut self, _v: &PlayerView, _req: &DecisionRequest) -> DecisionResponse {
                DecisionResponse::Pass
            }
        }

        let mut db = crate::cards::starter_db();
        register(&mut db);
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(db));
        let emeritus = {
            let c = state.card_db().get(EMERITUS_OF_CONFLICT).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        // Three Divinations ({2}{U}, no targets) + enough Islands to pay + a library to draw from.
        let divs: Vec<_> = (0..3)
            .map(|_| {
                let c = state.card_db().get(grp::DIVINATION).unwrap().chars.clone();
                state.add_card(PlayerId(0), c, Zone::Hand)
            })
            .collect();
        for _ in 0..9 {
            let c = state.card_db().get(grp::ISLAND).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield);
        }
        for _ in 0..10 {
            let c = state.card_db().get(grp::ISLAND).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Library);
        }
        let mut e = Engine::new(state, vec![Box::new(Pass), Box::new(Pass)]);
        e.state.active_player = PlayerId(0);
        e.state.phase = Phase::PrecombatMain;

        for (i, &div) in divs.iter().enumerate() {
            e.cast_spell(PlayerId(0), div, CastVariant::Normal);
            e.run_agenda(); // queue + stack the SpellCast trigger
            e.resolve_top(); // resolve the trigger (intervening-if: count == 3?)
            e.run_agenda();
            e.resolve_top(); // resolve the Divination
            let expect_prepared = i == 2;
            assert_eq!(
                e.state.object(emeritus).prepared,
                expect_prepared,
                "after spell #{}: prepared should be {expect_prepared}",
                i + 1
            );
        }
    }
}
