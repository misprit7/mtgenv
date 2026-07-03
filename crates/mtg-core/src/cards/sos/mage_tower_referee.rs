//! Mage Tower Referee — `{2}` Artifact Creature — Construct 2/1 (first printed SOS).
//!
//! Oracle: "Whenever you cast a multicolored spell, put a +1/+1 counter on this creature."
//!
//! **Fully implemented** — a colorless artifact creature with a `SpellCast(Multicolored)` cast-trigger
//! (the new `CardFilter::Multicolored` cap, ≥2 colors, landed alongside this card) that puts a +1/+1
//! counter on itself.

use crate::basics::{CardType, Color, CounterKind};
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern};
use crate::effects::target::CardFilter;
use crate::effects::value::ValueExpr;
use crate::effects::{Effect, EffectTarget};
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const MAGE_TOWER_REFEREE: u32 = 325;

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        MAGE_TOWER_REFEREE,
        "Mage Tower Referee",
        &[CreatureType::Construct],
        Color::White,
        mana_cost(2, &[]),
        2,
        1,
        vec![Ability::Triggered {
            event: EventPattern::SpellCast(CardFilter::Multicolored),
            condition: None,
            intervening_if: false,
            effect: Effect::PutCounters {
                what: EffectTarget::SourceSelf,
                kind: CounterKind::PlusOnePlusOne,
                n: ValueExpr::Fixed(1),
            },
        }],
    );
    def.chars.card_types = vec![CardType::Artifact, CardType::Creature];
    def.chars.colors = Vec::new(); // colorless artifact
    def.text = "Whenever you cast a multicolored spell, put a +1/+1 counter on this creature.".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn mage_tower_referee_shape() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(MAGE_TOWER_REFEREE).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Artifact, CardType::Creature]);
        assert!(def.chars.colors.is_empty());
        assert_eq!((def.chars.power, def.chars.toughness), (Some(2), Some(1)));
        assert_eq!(def.chars.mana_value(), 2);
        assert!(def.fully_implemented);
    }

    #[test]
    fn mage_tower_referee_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(MAGE_TOWER_REFEREE).unwrap();
        expect![[r#"
            [
                Triggered {
                    event: SpellCast(
                        Multicolored,
                    ),
                    condition: None,
                    intervening_if: false,
                    effect: PutCounters {
                        what: SourceSelf,
                        kind: PlusOnePlusOne,
                        n: Fixed(
                            1,
                        ),
                    },
                },
            ]"#]].assert_eq(&format!("{:#?}", def.abilities));
    }

    /// Integration (real trigger engine): casting a MULTICOLORED spell puts a +1/+1 counter on the
    /// Referee; a monocolored spell does not — exercising the `Multicolored` filter in the spell-cast
    /// trigger matcher.
    #[test]
    fn mage_tower_referee_counts_only_multicolored_spells() {
        use crate::agent::{Agent, DecisionRequest, DecisionResponse, GameEvent, PlayerView};
        use crate::basics::Zone;
        use crate::cards::{build_game, grp};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        use crate::stack::{StackObject, StackObjectKind};

        #[derive(Clone)]
        struct PassAgent;
        impl Agent for PassAgent {
            fn decide(&mut self, _v: &PlayerView, _r: &DecisionRequest) -> DecisionResponse {
                DecisionResponse::Pass
            }
        }

        // `colors` = the triggering spell's colors. Returns the +1/+1 counters on the Referee after.
        let run = |colors: Vec<Color>| -> u32 {
            let mut state = build_game(1, &[&[], &[]]);
            let referee = {
                let c = state.card_db().get(MAGE_TOWER_REFEREE).unwrap().chars.clone();
                state.add_card(PlayerId(0), c, Zone::Battlefield)
            };
            let spell = {
                let mut c = state.card_db().get(grp::LIGHTNING_BOLT).unwrap().chars.clone();
                c.colors = colors;
                state.add_card(PlayerId(0), c, Zone::Stack)
            };
            let sid = StackId(1);
            state.stack.push(StackObject {
                id: sid,
                controller: PlayerId(0),
                source: None,
                kind: StackObjectKind::Spell(spell),
                targets: vec![],
                x: None,
                modes: Vec::new(),
            });
            let mut e = Engine::new(state, vec![Box::new(PassAgent), Box::new(PassAgent)]);
            e.broadcast(GameEvent::SpellCast { spell: sid, controller: PlayerId(0) });
            e.run_agenda();
            // Resolve the Referee's trigger if it was queued (multicolored); leave the Bolt otherwise.
            let has_trigger =
                e.state.stack.items.iter().any(|s| matches!(s.kind, StackObjectKind::Ability { .. }));
            if has_trigger {
                e.resolve_top();
            }
            e.state.object(referee).counters.get(&CounterKind::PlusOnePlusOne)
        };

        assert_eq!(run(vec![Color::Blue, Color::Red]), 1, "multicolored spell → +1/+1 counter");
        assert_eq!(run(vec![Color::Red]), 0, "monocolored spell → no counter");
    }
}
