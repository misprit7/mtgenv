//! Colorstorm Stallion — `{1}{U}{R}` Creature — Elemental Horse 3/3 (first printed SOS).
//!
//! Oracle: "Ward {1} (Whenever this creature becomes the target of a spell or ability an opponent
//! controls, counter it unless that player pays {1}.) / Haste / Opus — Whenever you cast an instant
//! or sorcery spell, this creature gets +1/+1 until end of turn. If five or more mana was spent to
//! cast that spell, create a token that's a copy of this creature."
//!
//! **Fully implemented** — the first card of the S17 Ward cap:
//! - **Ward {1}** (CR 702.21): a `BecomesTargeted{ ItSelf, by_opponent }` trigger whose effect is
//!   `CounterUnlessPay{ Triggering, {1} }` — the soft-counter leaf. When an opponent's spell/ability
//!   targets this creature, the Ward trigger goes on the stack above it; on resolution the *targeting
//!   player* may pay {1} (they're offered the choice only if they can afford it) or the spell/ability
//!   is countered (CR 701.5).
//! - **Haste** — a printed keyword (read by summoning-sickness checks).
//! - **Opus** cast-trigger: always pumps itself +1/+1 until end of turn, and — when `ManaSpentOnTrigger
//!   ≥ 5` — also creates a token that's a copy of itself (`CreateTokenCopy` over `SourceSelf`).

use crate::basics::Color;
use crate::cards::helpers::{instant_or_sorcery, ward_mana};
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern, Keyword};
use crate::effects::condition::{Condition, Duration};
use crate::effects::target::TokenCopyMods;
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const COLORSTORM_STALLION: u32 = 332;

/// "Opus — … this creature gets +1/+1 until end of turn. If five or more mana was spent to cast that
/// spell, create a token that's a copy of this creature."
fn opus() -> Ability {
    let pump = Effect::PumpPT {
        what: EffectTarget::SourceSelf,
        power: ValueExpr::Fixed(1),
        toughness: ValueExpr::Fixed(1),
        duration: Duration::UntilEndOfTurn,
    };
    let copy_if_five = Effect::Conditional {
        cond: Condition::ValueAtLeast(ValueExpr::ManaSpentOnTrigger, ValueExpr::Fixed(5)),
        then: Box::new(Effect::CreateTokenCopy {
            source: EffectTarget::SourceSelf,
            controller: PlayerRef::Controller,
            mods: TokenCopyMods::default(),
        }),
        otherwise: None,
    };
    Ability::Triggered {
        event: EventPattern::SpellCast(instant_or_sorcery()),
        condition: None,
        intervening_if: false,
        effect: Effect::Sequence(vec![pump, copy_if_five]),
    }
}

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        COLORSTORM_STALLION,
        "Colorstorm Stallion",
        &[CreatureType::Elemental, CreatureType::Horse],
        Color::Blue,
        mana_cost(1, &[(Color::Blue, 1), (Color::Red, 1)]),
        3,
        3,
        vec![ward_mana(1), opus()],
    );
    def.chars.colors = vec![Color::Blue, Color::Red];
    def.chars.keywords = vec![Keyword::Haste];
    def.text = "Ward {1} (Whenever this creature becomes the target of a spell or ability an opponent controls, counter it unless that player pays {1}.)\nHaste\nOpus — Whenever you cast an instant or sorcery spell, this creature gets +1/+1 until end of turn. If five or more mana was spent to cast that spell, create a token that's a copy of this creature.".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, CastVariant, ConfirmKind, DecisionRequest, DecisionResponse, PlayerView};
    use crate::basics::{Target, Zone};
    use crate::cards::sos::erode::ERODE;
    use crate::cards::{grp, starter_db};
    use crate::ids::{ObjId, PlayerId};
    use crate::priority::Engine;
    use crate::state::GameState;
    use expect_test::expect;
    use std::sync::Arc;

    #[test]
    fn colorstorm_stallion_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(COLORSTORM_STALLION).unwrap();
        assert_eq!(def.chars.power, Some(3));
        assert_eq!(def.chars.toughness, Some(3));
        assert_eq!(def.chars.keywords, vec![Keyword::Haste]);
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
                    effect: CounterUnlessPay {
                        what: Triggering,
                        cost: Cost {
                            mana: Some(
                                ManaCost {
                                    generic: 1,
                                    colored: {},
                                    x: 0,
                                    hybrid: [],
                                    mono_hybrid: [],
                                    phyrexian: [],
                                },
                            ),
                            components: [],
                        },
                    },
                },
                Triggered {
                    event: SpellCast(
                        AnyOf(
                            [
                                HasCardType(
                                    Instant,
                                ),
                                HasCardType(
                                    Sorcery,
                                ),
                            ],
                        ),
                    ),
                    condition: None,
                    intervening_if: false,
                    effect: Sequence(
                        [
                            PumpPT {
                                what: SourceSelf,
                                power: Fixed(
                                    1,
                                ),
                                toughness: Fixed(
                                    1,
                                ),
                                duration: UntilEndOfTurn,
                            },
                            Conditional {
                                cond: ValueAtLeast(
                                    ManaSpentOnTrigger,
                                    Fixed(
                                        5,
                                    ),
                                ),
                                then: CreateTokenCopy {
                                    source: SourceSelf,
                                    controller: Controller,
                                    mods: TokenCopyMods {
                                        add_card_types: [],
                                        add_subtypes: [],
                                        set_power_toughness: None,
                                        counters: [],
                                    },
                                },
                                otherwise: None,
                            },
                        ],
                    ),
                },
            ]"#]]
        .assert_eq(&format!("{:#?}", def.abilities));
    }

    /// An agent that targets `want` with a spell, answers the Ward pay-or-not `Confirm` with `pay`,
    /// and declines every optional `SelectCards` (Erode's "may search").
    #[derive(Clone)]
    struct WardAgent {
        want: ObjId,
        pay: bool,
    }
    impl Agent for WardAgent {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::ChooseTargets { slots, .. } => {
                    let i = slots[0]
                        .legal
                        .iter()
                        .position(|t| matches!(t, Target::Object(o) if *o == self.want))
                        .unwrap_or(0);
                    DecisionResponse::Pairs(vec![(0, i as u32)])
                }
                // The Ward soft-counter's pay-or-be-countered choice (CR 702.21).
                DecisionRequest::Confirm { kind: ConfirmKind::PayToPrevent } => {
                    DecisionResponse::Bool(self.pay)
                }
                DecisionRequest::Confirm { .. } => DecisionResponse::Bool(true),
                // Decline Erode's optional "may search".
                DecisionRequest::SelectCards { .. } => DecisionResponse::Indices(vec![]),
                _ => DecisionResponse::Pass,
            }
        }
    }

    /// Build a game where P1 (opponent) casts Erode ({W}, "Destroy target creature") at P0's
    /// Colorstorm Stallion. `p1_lands` untapped Plains for P1 (one pays Erode's {W}); Islands give
    /// P1 the extra mana to pay Ward's {1}. Returns the engine mid-flow *after* the cast and running
    /// the agenda (Ward trigger queued), plus the Colorstorm object id and the Erode object id.
    fn setup(p1_pay_ward: bool, p1_islands: u32) -> (Engine, ObjId, ObjId) {
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(starter_db()));
        let stallion = {
            let c = state.card_db().get(COLORSTORM_STALLION).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        let erode = {
            let c = state.card_db().get(ERODE).unwrap().chars.clone();
            state.add_card(PlayerId(1), c, Zone::Hand)
        };
        {
            let c = state.card_db().get(grp::PLAINS).unwrap().chars.clone();
            state.add_card(PlayerId(1), c, Zone::Battlefield); // pays Erode's {W}
        }
        for _ in 0..p1_islands {
            let c = state.card_db().get(grp::ISLAND).unwrap().chars.clone();
            state.add_card(PlayerId(1), c, Zone::Battlefield); // pays Ward's {1}
        }
        let mut e = Engine::new(
            state,
            vec![
                Box::new(WardAgent { want: stallion, pay: false }),
                Box::new(WardAgent { want: stallion, pay: p1_pay_ward }),
            ],
        );
        e.cast_spell(PlayerId(1), erode, CastVariant::Normal); // targets Colorstorm → Ward triggers
        e.run_agenda();
        (e, stallion, erode)
    }

    /// Ward {1}: the opponent has no spare mana → can't pay → their Erode is countered, and
    /// Colorstorm survives (never destroyed).
    #[test]
    fn ward_counters_when_targeting_player_cant_pay() {
        let (mut e, stallion, erode) = setup(false, 0);
        while !e.state.stack.items.is_empty() {
            e.resolve_top();
            e.run_agenda();
        }
        assert!(
            e.state.player(PlayerId(0)).battlefield.contains(&stallion),
            "Ward countered Erode — Colorstorm was never destroyed"
        );
        assert!(
            e.state.player(PlayerId(1)).graveyard.contains(&erode),
            "the countered Erode is in its owner's graveyard"
        );
    }

    /// Ward {1}: the opponent has the mana and chooses to pay {1} → Erode is NOT countered and
    /// resolves, destroying Colorstorm.
    #[test]
    fn ward_lets_spell_through_when_targeting_player_pays() {
        let (mut e, stallion, _erode) = setup(true, 1);
        while !e.state.stack.items.is_empty() {
            e.resolve_top();
            e.run_agenda();
        }
        assert!(
            !e.state.player(PlayerId(0)).battlefield.contains(&stallion),
            "Erode was paid-through and resolved — Colorstorm destroyed"
        );
        assert!(
            e.state.player(PlayerId(0)).graveyard.contains(&stallion),
            "the destroyed Colorstorm is in its owner's graveyard"
        );
    }

    /// Ward {1}: the opponent CAN pay but declines → Erode is countered (Colorstorm survives).
    #[test]
    fn ward_counters_when_targeting_player_declines() {
        let (mut e, stallion, erode) = setup(false, 1);
        while !e.state.stack.items.is_empty() {
            e.resolve_top();
            e.run_agenda();
        }
        assert!(
            e.state.player(PlayerId(0)).battlefield.contains(&stallion),
            "declining Ward's {{1}} countered Erode — Colorstorm survives"
        );
        assert!(
            e.state.player(PlayerId(1)).graveyard.contains(&erode),
            "the declined-through Erode is countered into its owner's graveyard"
        );
    }
}
