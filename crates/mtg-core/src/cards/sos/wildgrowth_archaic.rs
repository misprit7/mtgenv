//! Wildgrowth Archaic — `{2/G}{2/G}` Creature — Avatar 0/0 (first printed SOS).
//!
//! Oracle: "Trample, reach / Converge — This creature enters with a +1/+1 counter on it for each
//! color of mana spent to cast it. / Whenever you cast a creature spell, that creature enters with
//! X additional +1/+1 counters on it, where X is the number of colors of mana spent to cast it."
//!
//! **Fully implemented.** The monocolour-hybrid cost (`{2/G}` pips), Trample/Reach, and both
//! clauses:
//! - Converge enters-with — a `ColorsSpent` self-replacement (CR 614.1e / 702.75), so the 0/0 enters
//!   as an X/X and survives the toughness SBA.
//! - "Whenever you cast a creature spell, that creature enters with X additional +1/+1 counters" — a
//!   [`Effect::EntersWithCountersRider`] arming a one-shot floating enters-with-counters replacement
//!   (CR 614) scoped to the just-cast spell (still on the stack when the trigger resolves), with
//!   `n = ColorsSpentOnTrigger` fixed at that moment. The rider fires when that spell resolves onto
//!   the battlefield — a Stack→Battlefield move doesn't invalidate the scope (only leaving the
//!   battlefield does), so the counters land as it enters.

use crate::basics::{CardType, Color, CounterKind};
use crate::cards::{creature, mana_cost_mono_hybrid, CardDb};
use crate::effects::ability::{Ability, ActionPattern, EventPattern, Keyword, Rewrite};
use crate::effects::target::CardFilter;
use crate::effects::value::ValueExpr;
use crate::effects::{Effect, EffectTarget};
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
            // "Whenever you cast a creature spell, that creature enters with X additional +1/+1
            // counters, where X = colours of mana spent to cast it." The trigger (you cast a creature
            // spell) arms a one-shot floating enters-with-counters rider on the just-cast spell, with
            // X = the colours spent on it, fixed now. Casting Wildgrowth itself doesn't trigger it —
            // it isn't a permanent (its ability isn't active) while on the stack.
            Ability::Triggered {
                event: EventPattern::SpellCast(CardFilter::HasCardType(CardType::Creature)),
                condition: None,
                intervening_if: false,
                effect: Effect::EntersWithCountersRider {
                    what: EffectTarget::Triggering,
                    kind: CounterKind::PlusOnePlusOne,
                    n: ValueExpr::ColorsSpentOnTrigger,
                },
            },
        ],
    );
    def.chars.keywords = vec![Keyword::Trample, Keyword::Reach];
    def.text = "Trample, reach\nConverge — This creature enters with a +1/+1 counter on it for each color of mana spent to cast it.\nWhenever you cast a creature spell, that creature enters with X additional +1/+1 counters on it, where X is the number of colors of mana spent to cast it.".to_string();
    db.insert(def);
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
        assert!(def.fully_implemented, "both clauses implemented");
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
                Triggered {
                    event: SpellCast(
                        HasCardType(
                            Creature,
                        ),
                    ),
                    condition: None,
                    intervening_if: false,
                    effect: EntersWithCountersRider {
                        what: Triggering,
                        kind: PlusOnePlusOne,
                        n: ColorsSpentOnTrigger,
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

    /// Behaviour (clause 2): with Wildgrowth on the battlefield, casting a creature spell paid with two
    /// colours makes THAT creature enter with two additional +1/+1 counters (the arming rider) on top
    /// of any of its own. Grizzly Bears (`{1}{G}`) paid with a Forest (the `{G}` pip) + a Mountain (the
    /// `{1}` generic, `{R}`) spends `{G,R}` = 2 colours → the 2/2 Bears enters as a 4/4.
    #[test]
    fn wildgrowth_cast_creature_enters_with_extra_counters() {
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
        // Wildgrowth on the battlefield, given one +1/+1 counter so the 0/0 survives the SBA in-test.
        let wild = {
            let c = state.card_db().get(WILDGROWTH_ARCHAIC).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        state.objects.get_mut(&wild).unwrap().counters.counts.insert(CounterKind::PlusOnePlusOne, 1);
        // Grizzly Bears ({1}{G}) in hand + a Forest (G) and a Mountain (R) to pay across two colours.
        let bears = {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Hand)
        };
        for grp_id in [grp::FOREST, grp::MOUNTAIN] {
            let land = state.card_db().get(grp_id).unwrap().chars.clone();
            state.add_card(PlayerId(0), land, Zone::Battlefield);
        }
        state.active_player = PlayerId(0);
        state.phase = crate::basics::Phase::PrecombatMain;
        let mut e = Engine::new(state, vec![Box::new(PassiveAgent), Box::new(PassiveAgent)]);
        e.cast_spell(PlayerId(0), bears, CastVariant::Normal);
        // The "you cast a creature spell" trigger goes on the stack above the Bears; resolving it arms
        // the rider, then resolving the Bears enters it and fires the rider.
        e.run_agenda();
        while !e.state.stack.items.is_empty() {
            e.resolve_top();
            e.run_agenda();
        }
        assert_eq!(
            e.state.object(bears).counters.get(&CounterKind::PlusOnePlusOne),
            2,
            "two colours spent (G,R) → two additional +1/+1 counters on the cast creature"
        );
        assert_eq!(e.state.computed(bears).power, Some(4), "2/2 base + two +1/+1 = 4/4");
    }
}
