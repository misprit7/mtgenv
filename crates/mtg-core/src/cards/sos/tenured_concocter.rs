//! Tenured Concocter — `{4}{G}` Creature — Troll Druid 4/4 (first printed SOS).
//!
//! Oracle: "Vigilance / Whenever this creature becomes the target of a spell or ability an opponent
//! controls, you may draw a card. / Infusion — This creature gets +2/+0 as long as you gained life
//! this turn."
//!
//! **Fully implemented** — printed Vigilance; a `BecomesTargeted` (self, by an opponent) trigger
//! that optionally draws a card; and a `ConditionalStatic` +2/+0 while the Infusion condition holds.

use crate::basics::Color;
use crate::cards::helpers::itself;
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern, Keyword, StaticContribution};
use crate::effects::condition::{Condition, Duration};
use crate::effects::target::CardFilter;
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const TENURED_CONCOCTER: u32 = 240;

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        TENURED_CONCOCTER,
        "Tenured Concocter",
        &[CreatureType::Troll, CreatureType::Druid],
        Color::Green,
        mana_cost(4, &[(Color::Green, 1)]),
        4,
        4,
        vec![
            Ability::Triggered {
                event: EventPattern::BecomesTargeted { filter: CardFilter::ItSelf, by_opponent: true },
                condition: None,
                intervening_if: false,
                effect: Effect::Optional {
                    prompt: "Draw a card?".to_string(),
                    body: Box::new(Effect::Draw {
                        who: PlayerRef::Controller,
                        count: ValueExpr::Fixed(1),
                    }),
                },
            },
            Ability::ConditionalStatic {
                contribution: StaticContribution::ModifyPT { power: 2, toughness: 0 },
                affects: itself(),
                duration: Duration::WhileSourcePresent,
                condition: Condition::GainedLifeThisTurn { who: PlayerRef::Controller },
            },
        ],
    );
    def.chars.keywords = vec![Keyword::Vigilance];
    def.text = "Vigilance\nWhenever this creature becomes the target of a spell or ability an opponent controls, you may draw a card.\nInfusion — This creature gets +2/+0 as long as you gained life this turn.".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn tenured_concocter_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(TENURED_CONCOCTER).unwrap();
        assert_eq!(def.chars.keywords, vec![Keyword::Vigilance]);
        assert!(def.fully_implemented);
        expect![[r#"
            [
                Triggered {
                    event: BecomesTargeted {
                        filter: ItSelf,
                        by_opponent: true,
                    },
                    condition: None,
                    intervening_if: false,
                    effect: Optional {
                        prompt: "Draw a card?",
                        body: Draw {
                            who: Controller,
                            count: Fixed(
                                1,
                            ),
                        },
                    },
                },
                ConditionalStatic {
                    contribution: ModifyPT {
                        power: 2,
                        toughness: 0,
                    },
                    affects: SelectSpec {
                        zone: Battlefield,
                        filter: ItSelf,
                        chooser: Controller,
                        min: Fixed(
                            0,
                        ),
                        max: Fixed(
                            0,
                        ),
                    },
                    duration: WhileSourcePresent,
                    condition: GainedLifeThisTurn {
                        who: Controller,
                    },
                },
            ]"#]].assert_eq(&format!("{:#?}", def.abilities));
    }

    /// Behaviour: the Infusion ConditionalStatic gives +2/+0 while life was gained this turn.
    #[test]
    fn tenured_concocter_infusion_toggles_power() {
        use crate::agent::RandomAgent;
        use crate::basics::Zone;
        use crate::cards::build_game;
        use crate::ids::PlayerId;
        use crate::priority::Engine;
        let mut state = build_game(1, &[&[], &[]]);
        let chars = state.card_db().get(TENURED_CONCOCTER).unwrap().chars.clone();
        let src = state.add_card(PlayerId(0), chars, Zone::Battlefield);
        let mut e = Engine::new(
            state,
            vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))],
        );
        assert_eq!(e.state.computed(src).power, Some(4), "base 4/4");
        e.state.players[0].life_gained_this_turn = 5;
        e.state.mark_chars_dirty();
        assert_eq!(e.state.computed(src).power, Some(6), "gained life → +2/+0 → 6/4");
    }
}
