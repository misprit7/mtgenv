//! Matterbending Mage — `{2}{U}` Creature — Human Wizard 2/2 (first printed SOS).
//!
//! Oracle: "When this creature enters, return up to one other target creature to its owner's hand. /
//! Whenever you cast a spell with {X} in its mana cost, this creature can't be blocked this turn."
//!
//! **Fully implemented** — two triggers:
//! 1. an ETB (`SelfEnters`) single-target bounce of "up to one **other**" (`min: 0`, `Not(ItSelf)`)
//!    creature to its owner's hand (`MoveZone` to Hand) — the "other" is the source-threaded
//!    `Not(ItSelf)` targeting cap.
//! 2. a **cast-with-{X}** trigger (`SpellCast(you control + `HasXInCost`)` — the S21 cap, an
//!    `HasXInCost` arm added to `enter_filter_matches`) that grants this creature `CantBeBlocked`
//!    until end of turn (an evasion `Qualification`, honoured by combat's `can_block`).

use crate::basics::{Color, Zone, ZoneDest, ZonePos};
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern, Qualification};
use crate::effects::condition::Duration;
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::PlayerRef;
use crate::effects::{Effect, EffectTarget};
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const MATTERBENDING_MAGE: u32 = 329;

/// "up to one other target creature" — any creature that is not this source (CR 601.2c "another/other").
fn other_target_creature() -> TargetSpec {
    TargetSpec {
        kind: TargetKind::Creature(CardFilter::Not(Box::new(CardFilter::ItSelf))),
        min: 0,
        max: 1,
        distinct: true,
    }
}

/// "a spell with {X} in its mana cost that you control" (CR 107.3).
fn your_x_spell() -> CardFilter {
    CardFilter::All(vec![
        CardFilter::ControlledBy(PlayerRef::Controller),
        CardFilter::HasXInCost,
    ])
}

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        MATTERBENDING_MAGE,
        "Matterbending Mage",
        &[CreatureType::Human, CreatureType::Wizard],
        Color::Blue,
        mana_cost(2, &[(Color::Blue, 1)]),
        2,
        2,
        vec![
            Ability::Triggered {
                event: EventPattern::SelfEnters,
                condition: None,
                intervening_if: false,
                effect: Effect::MoveZone {
                    what: EffectTarget::Target(other_target_creature()),
                    to: ZoneDest { zone: Zone::Hand, pos: ZonePos::Any },
                    tapped: false,
                },
            },
            Ability::Triggered {
                event: EventPattern::SpellCast(your_x_spell()),
                condition: None,
                intervening_if: false,
                effect: Effect::GrantQualification {
                    what: EffectTarget::SourceSelf,
                    qualification: Qualification::CantBeBlocked,
                    duration: Duration::UntilEndOfTurn,
                },
            },
        ],
    );
    def.text = "When this creature enters, return up to one other target creature to its owner's hand.\nWhenever you cast a spell with {X} in its mana cost, this creature can't be blocked this turn.".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn matterbending_mage_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(MATTERBENDING_MAGE).unwrap();
        assert_eq!(def.chars.power, Some(2));
        assert!(def.fully_implemented);
        expect![[r#"
            [
                Triggered {
                    event: SelfEnters,
                    condition: None,
                    intervening_if: false,
                    effect: MoveZone {
                        what: Target(
                            TargetSpec {
                                kind: Creature(
                                    Not(
                                        ItSelf,
                                    ),
                                ),
                                min: 0,
                                max: 1,
                                distinct: true,
                            },
                        ),
                        to: ZoneDest {
                            zone: Hand,
                            pos: Any,
                        },
                        tapped: false,
                    },
                },
                Triggered {
                    event: SpellCast(
                        All(
                            [
                                ControlledBy(
                                    Controller,
                                ),
                                HasXInCost,
                            ],
                        ),
                    ),
                    condition: None,
                    intervening_if: false,
                    effect: GrantQualification {
                        what: SourceSelf,
                        qualification: CantBeBlocked,
                        duration: UntilEndOfTurn,
                    },
                },
            ]"#]].assert_eq(&format!("{:#?}", def.abilities));
    }

    /// Behaviour: the ETB bounce returns the (other) targeted creature to its owner's hand.
    #[test]
    fn etb_bounces_another_creature_to_hand() {
        use crate::agent::RandomAgent;
        use crate::basics::{Target, Zone};
        use crate::cards::{build_game, grp};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        let mut state = build_game(1, &[&[], &[]]);
        let bears = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
        let victim = state.add_card(PlayerId(1), bears, Zone::Battlefield);
        let mage_chars = state.card_db().get(MATTERBENDING_MAGE).unwrap().chars.clone();
        let mage = state.add_card(PlayerId(0), mage_chars, Zone::Battlefield);
        let etb = match &state.card_db().get(MATTERBENDING_MAGE).unwrap().abilities[0] {
            Ability::Triggered { effect, .. } => effect.clone(),
            o => panic!("expected ETB Triggered, got {o:?}"),
        };
        let mut e = Engine::new(
            state,
            vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))],
        );
        e.resolve_effect(
            &etb,
            &ResolutionCtx {
                controller: Some(PlayerId(0)),
                source: Some(mage),
                chosen_targets: vec![Target::Object(victim)],
                ..Default::default()
            },
            WbReason::Resolve(StackId(0)),
        );
        assert!(!e.state.players[1].battlefield.contains(&victim), "left the battlefield");
        assert!(e.state.players[1].hand.contains(&victim), "returned to owner's hand");
    }

    /// Integration (real engine): casting a spell with {X} in its cost fires the trigger and grants
    /// the Mage `CantBeBlocked` until end of turn — and a non-{X} spell does NOT (S21 filter).
    #[test]
    fn casting_an_x_spell_makes_the_mage_unblockable() {
        use crate::agent::{Agent, DecisionRequest, DecisionResponse, GameEvent, PlayerView};
        use crate::basics::Zone;
        use crate::cards::{build_game, grp};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        use crate::stack::{StackObject, StackObjectKind};

        #[derive(Clone)]
        struct PassiveAgent;
        impl Agent for PassiveAgent {
            fn decide(&mut self, _v: &PlayerView, _req: &DecisionRequest) -> DecisionResponse {
                DecisionResponse::Pass
            }
        }

        // Put the Mage out, cast a spell whose printed cost has `has_x` {X}, settle the trigger.
        let run = |has_x: bool| -> bool {
            let mut state = build_game(1, &[&[], &[]]);
            let mage = {
                let c = state.card_db().get(MATTERBENDING_MAGE).unwrap().chars.clone();
                state.add_card(PlayerId(0), c, Zone::Battlefield)
            };
            // A Lightning Bolt on the stack, with {X} added to its printed cost when `has_x`.
            let spell = {
                let mut c = state.card_db().get(grp::LIGHTNING_BOLT).unwrap().chars.clone();
                if has_x {
                    c.mana_cost.as_mut().unwrap().x = 1;
                }
                state.add_card(PlayerId(0), c, Zone::Stack)
            };
            let sid = StackId(1);
            state.stack.push(StackObject {
                id: sid,
                controller: PlayerId(0),
                source: None,
                kind: StackObjectKind::Spell(spell),
                targets: vec![],
                x: has_x.then_some(1),
                modes: Vec::new(),
            });
            let mut e = Engine::new(state, vec![Box::new(PassiveAgent), Box::new(PassiveAgent)]);
            e.broadcast(GameEvent::SpellCast { spell: sid, controller: PlayerId(0) });
            e.run_agenda();
            // A fired trigger is an Ability sitting on top of the bolt (a Spell); resolve just it.
            // If none fired, the bolt is still on top — leave it (resolving it would need a target).
            if matches!(e.state.stack.top().map(|s| &s.kind), Some(StackObjectKind::Ability { .. })) {
                e.resolve_top();
            }
            e.state.computed(mage).has_qualification(Qualification::CantBeBlocked)
        };

        assert!(run(true), "an {{X}} spell grants CantBeBlocked");
        assert!(!run(false), "a non-{{X}} spell does not");
    }
}
