//! Pterafractyl — `{X}{G}{U}` Creature — Dinosaur Fractal 1/0 (first printed SOS).
//!
//! Oracle: "Flying / This creature enters with X +1/+1 counters on it. / When this creature enters,
//! you gain 2 life."
//!
//! **Fully implemented** — printed Flying + an enters-with-`X` +1/+1 counters self-replacement (so the
//! 1/0 base becomes a (1+X)/X) + an ETB gain-2-life trigger. `X` is the value chosen at cast, read from
//! the resolution context. Multicolored (G/U).

use crate::basics::{Color, CounterKind};
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, ActionPattern, EventPattern, Keyword, Rewrite};
use crate::effects::target::CardFilter;
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const PTERAFRACTYL: u32 = 313;

pub fn register(db: &mut CardDb) {
    let mut cost = mana_cost(0, &[(Color::Green, 1), (Color::Blue, 1)]);
    cost.x = 1;
    let mut def = creature(
        PTERAFRACTYL,
        "Pterafractyl",
        &[CreatureType::Dinosaur, CreatureType::Fractal],
        Color::Green,
        cost,
        1,
        0,
        vec![
            // "enters with X +1/+1 counters" — X is the chosen value at cast (CR 614.1e).
            Ability::Replacement {
                pattern: ActionPattern::WouldEnterBattlefield(CardFilter::ItSelf),
                rewrite: Rewrite::EntersWithCountersValue {
                    kind: CounterKind::PlusOnePlusOne,
                    n: ValueExpr::X,
                },
            },
            Ability::Triggered {
                event: EventPattern::SelfEnters,
                condition: None,
                intervening_if: false,
                effect: Effect::GainLife { who: PlayerRef::Controller, amount: ValueExpr::Fixed(2) },
            },
        ],
    );
    def.chars.colors = vec![Color::Green, Color::Blue];
    def.chars.keywords = vec![Keyword::Flying];
    def.text = "Flying\nThis creature enters with X +1/+1 counters on it.\nWhen this creature enters, you gain 2 life.".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pterafractyl_shape() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(PTERAFRACTYL).unwrap();
        assert_eq!(def.chars.colors, vec![Color::Green, Color::Blue]);
        assert_eq!(def.chars.keywords, vec![Keyword::Flying]);
        assert_eq!(def.chars.mana_cost.as_ref().unwrap().x, 1, "{{X}}{{G}}{{U}} has one {{X}}");
        assert!(def.fully_implemented);
        assert!(matches!(
            def.abilities[0],
            Ability::Replacement { rewrite: Rewrite::EntersWithCountersValue { n: ValueExpr::X, .. }, .. }
        ));
    }

    /// End-to-end: casting Pterafractyl with X=3 (from three Forests + {G}{U}) enters it with three
    /// +1/+1 counters — a 4/3 — and gains its controller 2 life.
    #[test]
    fn pterafractyl_enters_with_x_counters_and_gains_life() {
        use crate::agent::{Agent, CastVariant, DecisionRequest, DecisionResponse, PlayerView};
        use crate::basics::{CounterKind, Zone};
        use crate::cards::{grp, starter_db};
        use crate::ids::PlayerId;
        use crate::priority::Engine;
        use crate::state::GameState;
        use std::sync::Arc;
        #[derive(Clone)]
        struct XThree;
        impl Agent for XThree {
            fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
                match req {
                    // Choose X = 3 when casting.
                    DecisionRequest::ChooseNumber { .. } => DecisionResponse::Number(3),
                    _ => DecisionResponse::Pass,
                }
            }
        }
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(starter_db()));
        let ptera = {
            let c = state.card_db().get(PTERAFRACTYL).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Hand)
        };
        // {X=3}{G}{U} → three Forests + one Island (blue) + … actually need G, U, and 3 generic.
        let island = state.card_db().get(grp::ISLAND).unwrap().chars.clone();
        state.add_card(PlayerId(0), island, Zone::Battlefield); // U
        for _ in 0..4 {
            let f = state.card_db().get(grp::FOREST).unwrap().chars.clone();
            state.add_card(PlayerId(0), f, Zone::Battlefield); // one G pip + three generic
        }
        let life0 = state.player(PlayerId(0)).life;
        let mut e = Engine::new(state, vec![Box::new(XThree), Box::new(XThree)]);
        e.cast_spell(PlayerId(0), ptera, CastVariant::Normal);
        e.resolve_top(); // enters → enters-with-X replacement applies, ETB gain-life trigger fires
        // Settle: resolve the ETB gain-2-life trigger off the stack.
        e.run_agenda();
        while !e.state.stack.items.is_empty() {
            e.resolve_top();
            e.run_agenda();
        }
        assert_eq!(
            e.state.object(ptera).counters.get(&CounterKind::PlusOnePlusOne),
            3,
            "entered with X=3 +1/+1 counters"
        );
        assert_eq!(e.state.computed(ptera).power, Some(4), "1/0 base + three +1/+1 = 4/3");
        assert_eq!(e.state.computed(ptera).toughness, Some(3));
        assert_eq!(e.state.player(PlayerId(0)).life, life0 + 2, "ETB gained 2 life");
    }
}
