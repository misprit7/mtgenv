//! Forum Necroscribe — `{5}{B}` Creature — Troll Warlock 5/4 (first printed SOS).
//!
//! Oracle: "Ward—Discard a card. / Repartee — Whenever you cast an instant or sorcery spell that
//! targets a creature, return target creature card from your graveyard to the battlefield."
//!
//! **Fully implemented** — a second S17 Ward card, this one the **non-mana** Ward path:
//! - **Ward—Discard a card** (CR 702.21): `ward_discard()` — a `CounterUnlessPay{ Triggering, Cost{
//!   Discard(a card from hand) } }`. When an opponent's spell/ability targets this creature, the
//!   targeting player must discard a card (offered only if they hold one) or the spell/ability is
//!   countered. Exercises the discard cost path (`can_pay_cost`/`pay_cost` `Discard` arms).
//! - **Repartee** reanimation: a `SpellCastTargetingCreature(instant|sorcery)` trigger that returns
//!   a targeted creature card from your graveyard to the battlefield (`MoveZone` graveyard →
//!   battlefield, the same reanimation leaf Lorehold Charm mode 2 uses).

use crate::basics::{CardType, Color, Zone, ZoneDest, ZonePos};
use crate::cards::helpers::{instant_or_sorcery, ward_discard};
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern};
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::PlayerRef;
use crate::effects::{Effect, EffectTarget};
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const FORUM_NECROSCRIBE: u32 = 333;

/// "Repartee — Whenever you cast an instant or sorcery spell that targets a creature, return target
/// creature card from your graveyard to the battlefield."
fn repartee_reanimate() -> Ability {
    Ability::Triggered {
        event: EventPattern::SpellCastTargetingCreature(instant_or_sorcery()),
        condition: None,
        intervening_if: false,
        effect: Effect::MoveZone {
            what: EffectTarget::Target(TargetSpec {
                kind: TargetKind::CardInZone {
                    zone: Zone::Graveyard,
                    filter: CardFilter::All(vec![
                        CardFilter::ControlledBy(PlayerRef::Controller),
                        CardFilter::HasCardType(CardType::Creature),
                    ]),
                },
                min: 1,
                max: 1,
                distinct: true,
            }),
            to: ZoneDest { zone: Zone::Battlefield, pos: ZonePos::Any },
        },
    }
}

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        FORUM_NECROSCRIBE,
        "Forum Necroscribe",
        &[CreatureType::Troll, CreatureType::Warlock],
        Color::Black,
        mana_cost(5, &[(Color::Black, 1)]),
        5,
        4,
        vec![ward_discard(), repartee_reanimate()],
    );
    def.text = "Ward—Discard a card. (Whenever this creature becomes the target of a spell or ability an opponent controls, counter it unless that player discards a card.)\nRepartee — Whenever you cast an instant or sorcery spell that targets a creature, return target creature card from your graveyard to the battlefield.".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, CastVariant, ConfirmKind, DecisionRequest, DecisionResponse, GameEvent, PlayerView};
    use crate::basics::{Target, Zone};
    use crate::cards::sos::erode::ERODE;
    use crate::cards::{grp, starter_db};
    use crate::ids::{ObjId, PlayerId, StackId};
    use crate::priority::Engine;
    use crate::stack::{StackObject, StackObjectKind};
    use crate::state::GameState;
    use expect_test::expect;
    use std::sync::Arc;

    #[test]
    fn forum_necroscribe_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(FORUM_NECROSCRIBE).unwrap();
        assert_eq!(def.chars.power, Some(5));
        assert_eq!(def.chars.toughness, Some(4));
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
                            mana: None,
                            components: [
                                Discard(
                                    SelectSpec {
                                        zone: Hand,
                                        filter: Any,
                                        chooser: Controller,
                                        min: Fixed(
                                            1,
                                        ),
                                        max: Fixed(
                                            1,
                                        ),
                                    },
                                ),
                            ],
                        },
                    },
                },
                Triggered {
                    event: SpellCastTargetingCreature(
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
                    effect: MoveZone {
                        what: Target(
                            TargetSpec {
                                kind: CardInZone {
                                    zone: Graveyard,
                                    filter: All(
                                        [
                                            ControlledBy(
                                                Controller,
                                            ),
                                            HasCardType(
                                                Creature,
                                            ),
                                        ],
                                    ),
                                },
                                min: 1,
                                max: 1,
                                distinct: true,
                            },
                        ),
                        to: ZoneDest {
                            zone: Battlefield,
                            pos: Any,
                        },
                    },
                },
            ]"#]]
        .assert_eq(&format!("{:#?}", def.abilities));
    }

    /// Targets `want`, answers the Ward pay-or-not `Confirm` with `pay`, picks the first `SelectCards`
    /// option (for the discard cost / any reanimation), and declines Erode's optional "may search".
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
                DecisionRequest::Confirm { kind: ConfirmKind::PayToPrevent } => {
                    DecisionResponse::Bool(self.pay)
                }
                DecisionRequest::Confirm { .. } => DecisionResponse::Bool(true),
                // Erode's "may search" is a min:0 select — decline it; a discard-cost select has min:1.
                DecisionRequest::SelectCards { min, from, .. } => {
                    DecisionResponse::Indices((0..(*min).min(from.len() as u32)).collect())
                }
                _ => DecisionResponse::Pass,
            }
        }
    }

    /// Build a game: P1 (opponent) casts Erode at P0's Forum Necroscribe. `p1_hand_cards` extra cards
    /// in P1's hand (a card to discard for Ward). Returns the engine after the cast + agenda (Ward
    /// queued), the Forum object, and the Erode object.
    fn setup(p1_pay_ward: bool, p1_hand_cards: u32) -> (Engine, ObjId, ObjId) {
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(starter_db()));
        let forum = {
            let c = state.card_db().get(FORUM_NECROSCRIBE).unwrap().chars.clone();
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
        for _ in 0..p1_hand_cards {
            let c = state.card_db().get(grp::FOREST).unwrap().chars.clone();
            state.add_card(PlayerId(1), c, Zone::Hand); // a card P1 can discard for Ward
        }
        let mut e = Engine::new(
            state,
            vec![
                Box::new(WardAgent { want: forum, pay: false }),
                Box::new(WardAgent { want: forum, pay: p1_pay_ward }),
            ],
        );
        e.cast_spell(PlayerId(1), erode, CastVariant::Normal); // targets Forum → Ward triggers
        e.run_agenda();
        (e, forum, erode)
    }

    /// Ward—Discard: the opponent holds no card → can't pay → their Erode is countered, Forum survives.
    #[test]
    fn ward_discard_counters_when_opponent_has_no_card() {
        let (mut e, forum, erode) = setup(false, 0);
        while !e.state.stack.items.is_empty() {
            e.resolve_top();
            e.run_agenda();
        }
        assert!(
            e.state.player(PlayerId(0)).battlefield.contains(&forum),
            "Ward—Discard countered Erode (no card to discard) — Forum survives"
        );
        assert!(
            e.state.player(PlayerId(1)).graveyard.contains(&erode),
            "the countered Erode is in its owner's graveyard"
        );
    }

    /// Ward—Discard: the opponent holds a card and pays by discarding → Erode resolves, Forum dies,
    /// and the opponent's hand shrank by the discarded card.
    #[test]
    fn ward_discard_lets_spell_through_when_opponent_discards() {
        let (mut e, forum, _erode) = setup(true, 1);
        let hand_before = e.state.player(PlayerId(1)).hand.len();
        while !e.state.stack.items.is_empty() {
            e.resolve_top();
            e.run_agenda();
        }
        assert!(
            !e.state.player(PlayerId(0)).battlefield.contains(&forum),
            "the discarded-through Erode resolved — Forum destroyed"
        );
        assert_eq!(
            e.state.player(PlayerId(1)).hand.len(),
            hand_before - 1,
            "paying Ward—Discard cost one card from the opponent's hand"
        );
        // The opponent's graveyard now holds the discarded card AND the resolved Erode (an instant
        // goes to its owner's graveyard as it leaves the stack, CR 608.2n).
        assert_eq!(
            e.state.player(PlayerId(1)).graveyard.len(),
            2,
            "the discarded card + the resolved Erode are in the opponent's graveyard"
        );
    }

    /// Repartee reanimation through the real trigger: P0 casts an instant targeting a creature; Forum's
    /// Repartee fires and returns a creature card from P0's graveyard to the battlefield.
    #[test]
    fn repartee_reanimates_a_creature_from_graveyard() {
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(starter_db()));
        let forum = {
            let c = state.card_db().get(FORUM_NECROSCRIBE).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        // A creature card in P0's graveyard (the reanimation target) and a creature to Bolt.
        let bears = {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Graveyard)
        };
        let victim = {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            state.add_card(PlayerId(1), c, Zone::Battlefield)
        };
        let bolt = {
            let c = state.card_db().get(grp::LIGHTNING_BOLT).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Stack)
        };
        let sid = StackId(500);
        state.stack.push(StackObject {
            id: sid,
            controller: PlayerId(0),
            source: None,
            kind: StackObjectKind::Spell(bolt),
            targets: vec![Target::Object(victim)],
            x: None,
            modes: Vec::new(),
        });
        let mut e = Engine::new(
            state,
            vec![Box::new(WardAgent { want: bears, pay: false }), Box::new(WardAgent { want: bears, pay: false })],
        );
        // Casting the creature-targeting instant fires Repartee (CR 603.2); the trigger reanimates.
        e.broadcast(GameEvent::SpellCast { spell: sid, controller: PlayerId(0) });
        e.run_agenda(); // put the Repartee trigger on the stack (chooses the graveyard bears)
        while !e.state.stack.items.is_empty() {
            e.resolve_top();
            e.run_agenda();
        }
        assert!(
            e.state.player(PlayerId(0)).battlefield.contains(&bears),
            "Repartee returned the creature card from the graveyard to the battlefield"
        );
        assert!(
            !e.state.player(PlayerId(0)).graveyard.contains(&bears),
            "it left the graveyard"
        );
    }
}
