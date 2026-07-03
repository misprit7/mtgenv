//! Old-Growth Educator — `{2}{B}{G}` Creature — Treefolk Druid 4/4 (first printed SOS).
//!
//! Oracle: "Vigilance, reach / Infusion — When this creature enters, put two +1/+1 counters on it
//! if you gained life this turn."
//!
//! **Fully implemented** — printed Vigilance + Reach, plus an ETB `Conditional` on the Infusion
//! gate: if you gained life this turn, put two +1/+1 counters on itself (a 6/6). Multicolored (B/G).

use crate::basics::{Color, CounterKind};
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern, Keyword};
use crate::effects::condition::Condition;
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const OLD_GROWTH_EDUCATOR: u32 = 237;

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        OLD_GROWTH_EDUCATOR,
        "Old-Growth Educator",
        &[CreatureType::Treefolk, CreatureType::Druid],
        Color::Black,
        mana_cost(2, &[(Color::Black, 1), (Color::Green, 1)]),
        4,
        4,
        vec![Ability::Triggered {
            event: EventPattern::SelfEnters,
            condition: None,
            intervening_if: false,
            effect: Effect::Conditional {
                cond: Condition::GainedLifeThisTurn { who: PlayerRef::Controller },
                then: Box::new(Effect::PutCounters {
                    what: EffectTarget::SourceSelf,
                    kind: CounterKind::PlusOnePlusOne,
                    n: ValueExpr::Fixed(2),
                }),
                otherwise: None,
            },
        }],
    );
    def.chars.colors = vec![Color::Black, Color::Green];
    def.chars.keywords = vec![Keyword::Vigilance, Keyword::Reach];
    def.text = "Vigilance, reach\nInfusion — When this creature enters, put two +1/+1 counters on it if you gained life this turn.".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn old_growth_educator_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(OLD_GROWTH_EDUCATOR).unwrap();
        assert_eq!(def.chars.colors, vec![Color::Black, Color::Green]);
        assert_eq!(def.chars.keywords, vec![Keyword::Vigilance, Keyword::Reach]);
        assert!(def.fully_implemented);
        expect![[r#"
            [
                Triggered {
                    event: SelfEnters,
                    condition: None,
                    intervening_if: false,
                    effect: Conditional {
                        cond: GainedLifeThisTurn {
                            who: Controller,
                        },
                        then: PutCounters {
                            what: SourceSelf,
                            kind: PlusOnePlusOne,
                            n: Fixed(
                                2,
                            ),
                        },
                        otherwise: None,
                    },
                },
            ]"#]].assert_eq(&format!("{:#?}", def.abilities));
    }

    /// Behaviour: the ETB adds two +1/+1 counters (→ 6/6) only if life was gained this turn.
    #[test]
    fn old_growth_educator_infusion_counters() {
        use crate::agent::RandomAgent;
        use crate::basics::Zone;
        use crate::cards::build_game;
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;

        let power_after = |life_gained: u32| {
            let mut state = build_game(1, &[&[], &[]]);
            state.players[0].life_gained_this_turn = life_gained;
            let chars = state.card_db().get(OLD_GROWTH_EDUCATOR).unwrap().chars.clone();
            let src = state.add_card(PlayerId(0), chars, Zone::Battlefield);
            let etb = match &state.card_db().get(OLD_GROWTH_EDUCATOR).unwrap().abilities[0] {
                Ability::Triggered { effect, .. } => effect.clone(),
                o => panic!("expected ETB Triggered, got {o:?}"),
            };
            let mut e = Engine::new(
                state,
                vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))],
            );
            e.resolve_effect(
                &etb,
                &ResolutionCtx { controller: Some(PlayerId(0)), source: Some(src), ..Default::default() },
                WbReason::Resolve(StackId(0)),
            );
            e.state.computed(src).power.unwrap()
        };
        assert_eq!(power_after(3), 6, "gained life → +2/+2 → 6/6");
        assert_eq!(power_after(0), 4, "no life gained → stays 4/4");
    }
}
