//! Emil, Vastlands Roamer — `{2}{G}` Legendary Creature — Elf Druid 3/3 (first printed SOS).
//!
//! Oracle:
//! - "Creatures you control with +1/+1 counters on them have trample."
//! - "{4}{G}, {T}: Create a 0/0 green and blue Fractal creature token. Put X +1/+1 counters on it,
//!   where X is the number of differently named lands you control."
//!
//! **Fully implemented.** The anthem is a `Static` `GrantKeyword(Trample)` scoped to creatures you
//! control that have a +1/+1 counter (`CardFilter::HasCounter`, now wired into the static-scope
//! matcher). The activated ability creates the shared 0/0 Fractal (`fractal_token(0)`) entering with
//! `DistinctNames` +1/+1 counters — the new `ValueExpr::DistinctNames` counting distinct card names
//! among the lands you control — via `CreateToken.dynamic_counters` (so it enters as an X/X). The
//! ability's X is NOT a paid `{X}`; its cost is a plain `{4}{G}, {T}`.

use crate::basics::{CardType, Color, CounterKind, Zone};
use crate::cards::helpers::fractal_token;
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, Cost, CostComponent, Keyword, StaticContribution, Timing};
use crate::effects::condition::Duration;
use crate::effects::target::{CardFilter, SelectSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;
use crate::subtypes::{CreatureType, Supertype};

/// grp id (per-set ids live near their cards).
pub const EMIL_VASTLANDS_ROAMER: u32 = 353;

/// "Creatures you control with +1/+1 counters on them have trample."
fn trample_anthem() -> Ability {
    Ability::Static {
        contribution: StaticContribution::GrantKeyword(Keyword::Trample),
        affects: SelectSpec {
            zone: Zone::Battlefield,
            filter: CardFilter::All(vec![
                CardFilter::HasCardType(CardType::Creature),
                CardFilter::ControlledBy(PlayerRef::Controller),
                CardFilter::HasCounter(CounterKind::PlusOnePlusOne),
            ]),
            chooser: PlayerRef::Controller,
            min: ValueExpr::Fixed(0),
            max: ValueExpr::Fixed(0),
        },
        duration: Duration::WhileSourcePresent,
    }
}

/// "{4}{G}, {T}: Create a 0/0 green and blue Fractal, X +1/+1 counters, X = differently named lands
/// you control."
fn make_fractal() -> Ability {
    Ability::Activated {
        cost: Cost {
            mana: Some(mana_cost(4, &[(Color::Green, 1)])),
            components: vec![CostComponent::TapSelf],
        },
        effect: Effect::CreateToken {
            spec: fractal_token(0),
            count: ValueExpr::Fixed(1),
            controller: PlayerRef::Controller,
            dynamic_counters: vec![(
                CounterKind::PlusOnePlusOne,
                ValueExpr::DistinctNames {
                    zone: Zone::Battlefield,
                    filter: CardFilter::HasCardType(CardType::Land),
                    controller: Some(PlayerRef::Controller),
                },
            )],
        },
        timing: Timing::Instant,
        restriction: None,
        is_mana: false,
    }
}

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        EMIL_VASTLANDS_ROAMER,
        "Emil, Vastlands Roamer",
        &[CreatureType::Elf, CreatureType::Druid],
        Color::Green,
        mana_cost(2, &[(Color::Green, 1)]),
        3,
        3,
        vec![trample_anthem(), make_fractal()],
    );
    def.chars.supertypes = vec![Supertype::Legendary];
    def.text = "Creatures you control with +1/+1 counters on them have trample.\n{4}{G}, {T}: Create a 0/0 green and blue Fractal creature token. Put X +1/+1 counters on it, where X is the number of differently named lands you control.".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{AbilityRef, RandomAgent};
    use crate::basics::Zone;
    use crate::cards::{grp, starter_db};
    use crate::ids::PlayerId;
    use crate::priority::Engine;
    use crate::state::GameState;
    use expect_test::expect;
    use std::sync::Arc;

    #[test]
    fn emil_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(EMIL_VASTLANDS_ROAMER).unwrap();
        assert_eq!(def.chars.supertypes, vec![Supertype::Legendary]);
        assert!(def.fully_implemented);
        expect![[r#"
            [
                Static {
                    contribution: GrantKeyword(
                        Trample,
                    ),
                    affects: SelectSpec {
                        zone: Battlefield,
                        filter: All(
                            [
                                HasCardType(
                                    Creature,
                                ),
                                ControlledBy(
                                    Controller,
                                ),
                                HasCounter(
                                    PlusOnePlusOne,
                                ),
                            ],
                        ),
                        chooser: Controller,
                        min: Fixed(
                            0,
                        ),
                        max: Fixed(
                            0,
                        ),
                    },
                    duration: WhileSourcePresent,
                },
                Activated {
                    cost: Cost {
                        mana: Some(
                            ManaCost {
                                generic: 4,
                                colored: {
                                    Green: 1,
                                },
                                x: 0,
                                hybrid: [],
                                mono_hybrid: [],
                                phyrexian: [],
                            },
                        ),
                        components: [
                            TapSelf,
                        ],
                    },
                    effect: CreateToken {
                        spec: TokenSpec {
                            name: "Fractal",
                            card_types: [
                                Creature,
                            ],
                            subtypes: [
                                Creature(
                                    Fractal,
                                ),
                            ],
                            colors: [
                                Green,
                                Blue,
                            ],
                            power: 0,
                            toughness: 0,
                            keywords: [],
                            counters: [],
                            grp_id: 0,
                        },
                        count: Fixed(
                            1,
                        ),
                        controller: Controller,
                        dynamic_counters: [
                            (
                                PlusOnePlusOne,
                                DistinctNames {
                                    zone: Battlefield,
                                    filter: HasCardType(
                                        Land,
                                    ),
                                    controller: Some(
                                        Controller,
                                    ),
                                },
                            ),
                        ],
                    },
                    timing: Instant,
                    restriction: None,
                    is_mana: false,
                },
            ]"#]]
        .assert_eq(&format!("{:#?}", def.abilities));
    }

    /// The anthem grants trample only to your creatures that have a +1/+1 counter.
    #[test]
    fn anthem_grants_trample_only_with_a_counter() {
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(starter_db()));
        let emil = {
            let c = state.card_db().get(EMIL_VASTLANDS_ROAMER).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        let with_counter = {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        let without = {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        state.objects.get_mut(&with_counter).unwrap().counters.counts.insert(CounterKind::PlusOnePlusOne, 1);
        state.mark_chars_dirty();
        let e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
        assert!(
            e.state.computed(with_counter).has_keyword(Keyword::Trample),
            "a counter-bearing creature you control has trample"
        );
        assert!(
            !e.state.computed(without).has_keyword(Keyword::Trample),
            "a creature with no counter does not"
        );
        // Emil itself has no +1/+1 counter, so no trample.
        assert!(!e.state.computed(emil).has_keyword(Keyword::Trample), "Emil (no counter) — no trample");
    }

    /// Real-path: with two differently-named lands (Forest, Island) plus a duplicate Forest,
    /// activating `{4}{G}, {T}` makes a Fractal entering with X=2 +1/+1 counters (distinct names,
    /// so the second Forest doesn't count).
    #[test]
    fn activated_makes_fractal_with_distinct_named_lands() {
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(starter_db()));
        let emil = {
            let c = state.card_db().get(EMIL_VASTLANDS_ROAMER).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        // Lands: two Forests (same name) + one Island = 2 distinct names. Plus 5 more Forests to fund
        // the {4}{G} cost (all Forests, but distinct-names still counts "Forest" once).
        for _ in 0..7 {
            let c = state.card_db().get(grp::FOREST).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield);
        }
        let island = state.card_db().get(grp::ISLAND).unwrap().chars.clone();
        state.add_card(PlayerId(0), island, Zone::Battlefield);
        state.active_player = PlayerId(0);
        let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
        let bf_before = e.state.player(PlayerId(0)).battlefield.len();
        // Ability index 1 = the {4}{G},{T} activated ability.
        e.activate_ability(PlayerId(0), emil, AbilityRef(1));
        assert!(e.state.object(emil).status.tapped, "Emil tapped for {{T}}");
        assert_eq!(e.state.stack.items.len(), 1, "ability on the stack");
        e.resolve_top();
        e.run_agenda();
        let bf = &e.state.player(PlayerId(0)).battlefield;
        assert_eq!(bf.len(), bf_before + 1, "one Fractal token created");
        let token = *bf.last().unwrap();
        assert_eq!(
            e.state.object(token).counters.get(&CounterKind::PlusOnePlusOne),
            2,
            "X = 2 differently named lands (Forest, Island)"
        );
    }
}
