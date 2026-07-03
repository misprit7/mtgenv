//! Rancorous Archaic — `{5}` Creature — Avatar 2/2 (first printed SOS).
//!
//! Oracle: "Trample, reach / Converge — This creature enters with a +1/+1 counter on it for each
//! color of mana spent to cast it."
//!
//! **Fully implemented** — printed Trample + Reach, plus a Converge enters-with self-replacement
//! (CR 614.1e / 702.75): `Rewrite::EntersWithCountersValue{ PlusOnePlusOne, ColorsSpent }`, the same
//! leaf the authored Magmablood/Transcendent Archaics use. `{5}` is generic, so the colours spent
//! depend on what mana the caster taps.

use crate::basics::{CardType, Color, CounterKind};
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, ActionPattern, Keyword, Rewrite};
use crate::effects::target::CardFilter;
use crate::effects::value::ValueExpr;
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const RANCOROUS_ARCHAIC: u32 = 336;

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        RANCOROUS_ARCHAIC,
        "Rancorous Archaic",
        &[CreatureType::Avatar],
        Color::Colorless,
        mana_cost(5, &[]),
        2,
        2,
        vec![Ability::Replacement {
            pattern: ActionPattern::WouldEnterBattlefield(CardFilter::ItSelf),
            rewrite: Rewrite::EntersWithCountersValue {
                kind: CounterKind::PlusOnePlusOne,
                n: ValueExpr::ColorsSpent,
            },
        }],
    );
    def.chars.keywords = vec![Keyword::Trample, Keyword::Reach];
    def.text = "Trample, reach\nConverge — This creature enters with a +1/+1 counter on it for each color of mana spent to cast it.".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::basics::CardType;
    use expect_test::expect;

    #[test]
    fn rancorous_archaic_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(RANCOROUS_ARCHAIC).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Creature]);
        assert_eq!(def.chars.keywords, vec![Keyword::Trample, Keyword::Reach]);
        assert_eq!((def.chars.power, def.chars.toughness), (Some(2), Some(2)));
        assert!(def.fully_implemented);
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

    /// Converge enters-with, driven through the real cast path: `{5}` auto-paid by the four coloured
    /// basics (+ a second Forest for the fifth mana) → four DISTINCT colours spent → the 2/2 enters
    /// with four +1/+1 counters (computed 6/6).
    #[test]
    fn converge_counters_from_colors_spent() {
        use crate::agent::{Agent, CastVariant, DecisionRequest, DecisionResponse, PlayerView};
        use crate::basics::Zone;
        use crate::cards::{grp, starter_db};
        use crate::ids::PlayerId;
        use crate::priority::Engine;
        use crate::state::GameState;
        use std::sync::Arc;

        #[derive(Clone)]
        struct Passer;
        impl Agent for Passer {
            fn decide(&mut self, _v: &PlayerView, _r: &DecisionRequest) -> DecisionResponse {
                DecisionResponse::Pass
            }
        }

        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(starter_db()));
        let card = {
            let c = state.card_db().get(RANCOROUS_ARCHAIC).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Hand)
        };
        // {5} generic: four coloured basics (W,U,R,G) + a second Forest for the fifth mana → four
        // DISTINCT colours spent.
        for land in [grp::PLAINS, grp::ISLAND, grp::MOUNTAIN, grp::FOREST, grp::FOREST] {
            let c = state.card_db().get(land).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield);
        }
        let mut e = Engine::new(state, vec![Box::new(Passer), Box::new(Passer)]);
        e.cast_spell(PlayerId(0), card, CastVariant::Normal);
        e.run_agenda();
        while !e.state.stack.items.is_empty() {
            e.resolve_top();
            e.run_agenda();
        }
        let on_bf = *e.state.player(PlayerId(0)).battlefield.last().unwrap();
        assert_eq!(
            e.state.object(on_bf).counters.get(&CounterKind::PlusOnePlusOne),
            4,
            "Converge: one +1/+1 counter per DISTINCT colour of mana spent (four colours)"
        );
        assert_eq!(e.state.computed(on_bf).power, Some(6), "2/2 base + four +1/+1 → 6/6");
    }
}
