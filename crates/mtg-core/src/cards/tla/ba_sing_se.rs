//! Ba Sing Se — Land (first printed TLA, Avatar: The Last Airbender).
//!
//! Oracle:
//!   This land enters tapped unless you control a basic land.
//!   {T}: Add {G}.
//!   {2}{G}, {T}: Earthbend 2. Activate only as a sorcery.
//!
//! IMPLEMENTED:
//! - "enters tapped unless you control a basic land" — a `WouldEnterBattlefield(ItSelf)`
//!   replacement → `EntersTappedUnless(CountAtLeast{basic land ≥ 1})` (C11).
//! - `{T}: Add {G}` — a real IR mana ability (C19; it has no basic land type, so the mana is NOT
//!   intrinsic and needs the explicit ability).
//! - "{2}{G}, {T}: Earthbend 2. Activate only as a sorcery." — an `Ability::Activated`
//!   ({2}{G} mana + `TapSelf`, `Timing::Sorcery`) over `Effect::Earthbend{target: land you control,
//!   n: 2}` (C12, fully landed). The land becomes a 0/0 haste land-creature with two +1/+1 counters,
//!   and the engine's earthbend interpreter registers the "when it dies or is exiled, return it
//!   tapped" delayed trigger (CR 603.7) — so this card is **fully implemented**.

use crate::basics::{CardType, Color, Zone};
use crate::cards::helpers::{basic_land_filter, earthbend};
use crate::cards::{mana_ability, mana_cost, CardDb, CardDef};
use crate::effects::ability::{Ability, ActionPattern, Cost, CostComponent, Rewrite, Timing};
use crate::effects::condition::Condition;
use crate::effects::target::CardFilter;
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::state::Characteristics;

/// grp id (per-set ids live near their cards).
pub const BA_SING_SE: u32 = 110;

pub fn register(db: &mut CardDb) {
    let chars = Characteristics {
        name: "Ba Sing Se".to_string(),
        card_types: vec![CardType::Land],
        grp_id: BA_SING_SE,
        ..Default::default()
    };
    db.insert(CardDef {
        chars,
        abilities: vec![
            // "{T}: Add {G}." (no basic land type → explicit IR mana ability, not intrinsic).
            mana_ability(Color::Green),
            // "enters tapped unless you control a basic land."
            Ability::Replacement {
                pattern: ActionPattern::WouldEnterBattlefield(CardFilter::ItSelf),
                rewrite: Rewrite::EntersTappedUnless(Condition::CountAtLeast {
                    zone: Zone::Battlefield,
                    filter: basic_land_filter(),
                    controller: Some(PlayerRef::Controller),
                    n: ValueExpr::Fixed(1),
                }),
            },
            // "{2}{G}, {T}: Earthbend 2. Activate only as a sorcery."
            Ability::Activated {
                cost: Cost {
                    mana: Some(mana_cost(2, &[(Color::Green, 1)])),
                    components: vec![CostComponent::TapSelf],
                },
                effect: earthbend(2),
                timing: Timing::Sorcery,
                restriction: None,
                is_mana: false,
            },
        ],
        text: "This land enters tapped unless you control a basic land.\n{T}: Add {G}.\n{2}{G}, {T}: Earthbend 2. Activate only as a sorcery.".to_string(),
        // Fully implemented: all three clauses faithful, and C12 earthbend (incl. return-tapped) landed.
        fully_implemented: true,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn ba_sing_se_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(BA_SING_SE).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Land]);
        assert!(def.is_mana_source()); // explicit {T}: Add {G} IR ability
        // Fully implemented: enters-tapped-unless (C11) + {T}:Add{G} (C19) + earthbend 2 (C12, incl.
        // return-tapped) are all faithful.
        assert!(def.fully_implemented);
        expect![[r#"
            [
                Activated {
                    cost: Cost {
                        mana: None,
                        components: [
                            TapSelf,
                        ],
                    },
                    effect: AddMana {
                        who: Controller,
                        mana: ManaSpec {
                            produces: [
                                (
                                    Green,
                                    Fixed(
                                        1,
                                    ),
                                ),
                            ],
                            any_color: None,
                        },
                    },
                    timing: Instant,
                    restriction: None,
                    is_mana: true,
                },
                Replacement {
                    pattern: WouldEnterBattlefield(
                        ItSelf,
                    ),
                    rewrite: EntersTappedUnless(
                        CountAtLeast {
                            zone: Battlefield,
                            filter: All(
                                [
                                    HasCardType(
                                        Land,
                                    ),
                                    Supertype(
                                        Basic,
                                    ),
                                ],
                            ),
                            controller: Some(
                                Controller,
                            ),
                            n: Fixed(
                                1,
                            ),
                        },
                    ),
                },
                Activated {
                    cost: Cost {
                        mana: Some(
                            ManaCost {
                                generic: 2,
                                colored: {
                                    Green: 1,
                                },
                                x: 0,
                            },
                        ),
                        components: [
                            TapSelf,
                        ],
                    },
                    effect: Earthbend {
                        target: Target(
                            TargetSpec {
                                kind: Permanent(
                                    All(
                                        [
                                            HasCardType(
                                                Land,
                                            ),
                                            ControlledBy(
                                                Controller,
                                            ),
                                        ],
                                    ),
                                ),
                                min: 1,
                                max: 1,
                                distinct: true,
                            },
                        ),
                        n: Fixed(
                            2,
                        ),
                    },
                    timing: Sorcery,
                    restriction: None,
                    is_mana: false,
                },
            ]"#]].assert_eq(&format!("{:#?}", def.abilities));
    }

    /// Behaviour: resolving Ba Sing Se's "Earthbend 2" ability on a Forest you control animates it —
    /// a 0/0 haste creature that's still a land, with two +1/+1 counters → a 2/2 land-creature.
    #[test]
    fn ba_sing_se_earthbend_animates_a_land() {
        use crate::agent::RandomAgent;
        use crate::basics::{CardType, Target, Zone};
        use crate::effects::ability::Keyword;
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        let mut state = crate::cards::build_game(1, &[&[], &[]]);
        let forest_chars = state.card_db().get(crate::cards::grp::FOREST).unwrap().chars.clone();
        let forest = state.add_card(PlayerId(0), forest_chars, Zone::Battlefield);
        // Ba Sing Se's earthbend ability (the activated one).
        let earthbend = match &state.card_db().get(BA_SING_SE).unwrap().abilities[2] {
            Ability::Activated { effect, .. } => effect.clone(),
            other => panic!("expected earthbend Activated, got {other:?}"),
        };
        let mut e = Engine::new(
            state,
            vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))],
        );
        e.resolve_effect(
            &earthbend,
            &ResolutionCtx {
                controller: Some(PlayerId(0)),
                source: Some(forest),
                chosen_targets: vec![Target::Object(forest)],
                ..Default::default()
            },
            WbReason::Resolve(StackId(0)),
        );
        let cc = e.state.computed(forest);
        assert!(cc.is_creature(), "the land became a creature");
        assert!(cc.card_types.contains(&CardType::Land), "and is still a land");
        assert!(cc.has_keyword(Keyword::Haste), "with haste");
        assert_eq!(cc.power, Some(2)); // 0/0 base + two +1/+1 counters
        assert_eq!(cc.toughness, Some(2));
    }

    /// Behaviour: resolving Ba Sing Se's `{T}: Add {G}` mana ability (`abilities[0]`) adds exactly one
    /// green to the controller's pool. Ba Sing Se has no basic land type, so this mana is the explicit
    /// IR ability — not intrinsic — and must be exercised directly. (The `{T}` cost double-count bug
    /// #57 is a *payment-side* issue exercised only when this {T} source pays the earthbend `{2}{G}`.)
    #[test]
    fn ba_sing_se_taps_for_green() {
        use crate::agent::RandomAgent;
        use crate::basics::{Color, Zone};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        let mut state = crate::cards::build_game(1, &[&[], &[]]);
        let chars = state.card_db().get(BA_SING_SE).unwrap().chars.clone();
        let land = state.add_card(PlayerId(0), chars, Zone::Battlefield);
        let mana = match &state.card_db().get(BA_SING_SE).unwrap().abilities[0] {
            Ability::Activated { effect, is_mana: true, .. } => effect.clone(),
            other => panic!("expected the {{T}}: Add {{G}} mana Activated, got {other:?}"),
        };
        let mut e = Engine::new(
            state,
            vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))],
        );
        e.resolve_effect(
            &mana,
            &ResolutionCtx { controller: Some(PlayerId(0)), source: Some(land), ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        assert_eq!(e.state.players[0].mana_pool.amounts.get(&Color::Green), Some(&1));
    }
}
