//! Wildgrowth Archaic — `{2/G}{2/G}` Creature — Avatar 0/0 (first printed SOS).
//!
//! Oracle: "Trample, reach / Converge — This creature enters with a +1/+1 counter on it for each
//! color of mana spent to cast it. / Whenever you cast a creature spell, that creature enters with
//! X additional +1/+1 counters on it, where X is the number of colors of mana spent to cast it."
//!
//! **Tracked-partial** (`.incomplete()`): the monocolour-hybrid cost (`{2/G}` pips) + Trample/Reach +
//! the Converge enters-with (a `ColorsSpent` self-replacement, so the 0/0 enters as an X/X and
//! survives the toughness SBA) are implemented. The second clause — "that [other] creature enters
//! with X additional +1/+1 counters" — is **deferred**: it needs a *delayed enters-with-counters
//! replacement keyed to another spell still on the stack* (the cast creature hasn't entered when the
//! trigger resolves), a mechanism the engine doesn't have yet. Omitted rather than shipped as a no-op
//! husk.

use crate::basics::{Color, CounterKind};
use crate::cards::{creature, mana_cost_mono_hybrid, CardDb};
use crate::effects::ability::{Ability, ActionPattern, Keyword, Rewrite};
use crate::effects::target::CardFilter;
use crate::effects::value::ValueExpr;
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const WILDGROWTH_ARCHAIC: u32 = 308;

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        WILDGROWTH_ARCHAIC,
        "Wildgrowth Archaic",
        &[CreatureType::Avatar],
        Color::Green,
        mana_cost_mono_hybrid(0, &[], &[(2, Color::Green), (2, Color::Green)]),
        0,
        0,
        vec![
            // Converge — enters with a +1/+1 counter for each colour of mana spent (CR 614.1e / 702.75);
            // without it the 0/0 dies to the toughness-0 SBA when cast with any colour.
            Ability::Replacement {
                pattern: ActionPattern::WouldEnterBattlefield(CardFilter::ItSelf),
                rewrite: Rewrite::EntersWithCountersValue {
                    kind: CounterKind::PlusOnePlusOne,
                    n: ValueExpr::ColorsSpent,
                },
            },
            // deferred: "Whenever you cast a creature spell, that creature enters with X additional
            // +1/+1 counters, where X = colours of mana spent to cast it." Needs a delayed enters-with
            // replacement targeting the cast spell — omitted (see module doc).
        ],
    );
    def.chars.keywords = vec![Keyword::Trample, Keyword::Reach];
    def.text = "Trample, reach\nConverge — This creature enters with a +1/+1 counter on it for each color of mana spent to cast it.\nWhenever you cast a creature spell, that creature enters with X additional +1/+1 counters on it, where X is the number of colors of mana spent to cast it.".to_string();
    db.insert(def.incomplete());
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn wildgrowth_archaic_shape() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(WILDGROWTH_ARCHAIC).unwrap();
        assert_eq!(def.chars.colors, vec![Color::Green]);
        assert_eq!(def.chars.keywords, vec![Keyword::Trample, Keyword::Reach]);
        assert_eq!(
            def.chars.mana_cost.as_ref().unwrap().mono_hybrid,
            vec![(2, Color::Green), (2, Color::Green)]
        );
        assert_eq!(def.chars.mana_value(), 4, "{{2/G}}x2 = MV 4 (CR 202.3g)");
        assert!(!def.fully_implemented, "creature-cast counter-injection trigger deferred");
        expect![[r#"
            [
                Replacement {
                    pattern: WouldEnterBattlefield(
                        ItSelf,
                    ),
                    rewrite: EntersWithCountersValue {
                        kind: PlusOnePlusOne,
                        n: ColorsSpent,
                    },
                },
            ]"#]]
        .assert_eq(&format!("{:#?}", def.abilities));
    }

    /// Behaviour: a real cast of `{2/G}{2/G}` paid with two Forests (green) + two colourless spends
    /// one colour → enters with one +1/+1 counter → a 1/1 (survives the 0/0 toughness SBA).
    #[test]
    fn wildgrowth_real_cast_converge_one_color() {
        use crate::agent::{Agent, CastVariant, DecisionRequest, DecisionResponse, PlayerView};
        use crate::basics::{CounterKind, Zone};
        use crate::cards::{grp, starter_db};
        use crate::ids::PlayerId;
        use crate::priority::Engine;
        use crate::state::GameState;
        use std::sync::Arc;

        #[derive(Clone)]
        struct PassiveAgent;
        impl Agent for PassiveAgent {
            fn decide(&mut self, _v: &PlayerView, _req: &DecisionRequest) -> DecisionResponse {
                DecisionResponse::Pass
            }
        }

        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(starter_db()));
        let wild = {
            let c = state.card_db().get(WILDGROWTH_ARCHAIC).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Hand)
        };
        // Four Forests (G): each {2/G} pip pays with one green → colours spent = {G} = 1.
        for _ in 0..4 {
            let f = state.card_db().get(grp::FOREST).unwrap().chars.clone();
            state.add_card(PlayerId(0), f, Zone::Battlefield);
        }
        let mut e = Engine::new(state, vec![Box::new(PassiveAgent), Box::new(PassiveAgent)]);
        e.cast_spell(PlayerId(0), wild, CastVariant::Normal);
        e.resolve_top();
        e.run_agenda();
        assert_eq!(
            e.state.object(wild).counters.get(&CounterKind::PlusOnePlusOne),
            1,
            "one colour spent (G) → one +1/+1 counter"
        );
        assert_eq!(e.state.computed(wild).power, Some(1), "0/0 base + one +1/+1 = 1/1");
        assert!(e.state.player(PlayerId(0)).battlefield.contains(&wild), "survives the 0/0 SBA");
    }
}
