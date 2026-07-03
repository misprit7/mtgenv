//! Elemental Mascot — `{1}{U}{R}` Creature — Elemental Bird 1/4 (first printed SOS).
//!
//! Oracle: "Flying, vigilance / Opus — Whenever you cast an instant or sorcery spell, this creature
//! gets +1/+0 until end of turn. If five or more mana was spent to cast that spell, exile the top
//! card of your library. You may play that card until the end of your next turn."
//!
//! **Fully implemented** — printed Flying + Vigilance, plus an `Opus` cast-trigger (S5:
//! `SpellCast(instant|sorcery)`, own spells only): self `+1/+0` EOT, then a `Conditional` on
//! `ManaSpentOnTrigger ≥ 5` that **impulse-exiles** (S15) the top card of your library via
//! `Effect::ExileForPlay { what: TopOfLibrary(Controller), window: YourNextTurn }` — playable
//! (cast, or played as a land) until the end of your next turn. Second consumer of the S15 cap;
//! first to exercise the top-of-library source + land-play-from-exile.

use crate::basics::Color;
use crate::cards::helpers::instant_or_sorcery;
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern, Keyword};
use crate::effects::condition::{Condition, Duration};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget, PlayWindow};
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const ELEMENTAL_MASCOT: u32 = 319;

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        ELEMENTAL_MASCOT,
        "Elemental Mascot",
        &[CreatureType::Elemental, CreatureType::Bird],
        Color::Blue,
        mana_cost(1, &[(Color::Blue, 1), (Color::Red, 1)]),
        1,
        4,
        vec![Ability::Triggered {
            event: EventPattern::SpellCast(instant_or_sorcery()),
            condition: None,
            intervening_if: false,
            effect: Effect::Sequence(vec![
                Effect::PumpPT {
                    what: EffectTarget::SourceSelf,
                    power: ValueExpr::Fixed(1),
                    toughness: ValueExpr::Fixed(0),
                    duration: Duration::UntilEndOfTurn,
                },
                Effect::Conditional {
                    cond: Condition::ValueAtLeast(ValueExpr::ManaSpentOnTrigger, ValueExpr::Fixed(5)),
                    then: Box::new(Effect::ExileForPlay {
                        what: EffectTarget::TopOfLibrary(PlayerRef::Controller),
                        window: PlayWindow::YourNextTurn,
                    }),
                    otherwise: None,
                },
            ]),
        }],
    );
    def.chars.colors = vec![Color::Blue, Color::Red];
    def.chars.keywords = vec![Keyword::Flying, Keyword::Vigilance];
    def.text = "Flying, vigilance\nOpus — Whenever you cast an instant or sorcery spell, this creature gets +1/+0 until end of turn. If five or more mana was spent to cast that spell, exile the top card of your library. You may play that card until the end of your next turn.".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn elemental_mascot_shape() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(ELEMENTAL_MASCOT).unwrap();
        assert_eq!(def.chars.colors, vec![Color::Blue, Color::Red]);
        assert_eq!(def.chars.keywords, vec![Keyword::Flying, Keyword::Vigilance]);
        assert_eq!(def.chars.mana_value(), 3);
        assert!(def.fully_implemented);
    }

    #[test]
    fn elemental_mascot_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(ELEMENTAL_MASCOT).unwrap();
        expect![[r#"
            [
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
                                    0,
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
                                then: ExileForPlay {
                                    what: TopOfLibrary(
                                        Controller,
                                    ),
                                    window: YourNextTurn,
                                },
                                otherwise: None,
                            },
                        ],
                    ),
                },
            ]"#]].assert_eq(&format!("{:#?}", def.abilities));
    }

    /// End-to-end: the Opus trigger fires on casting an instant. Under 5 mana → only the +1/+0 pump;
    /// 5+ mana → also impulse-exiles the top card of your library (castable until end of next turn).
    #[test]
    fn elemental_mascot_opus_impulses_top_card_at_five_mana() {
        use crate::agent::{Agent, DecisionRequest, DecisionResponse, GameEvent, PlayerView};
        use crate::basics::Zone;
        use crate::cards::{build_game, grp};
        use crate::ids::{ObjId, PlayerId, StackId};
        use crate::priority::Engine;
        use crate::stack::{StackObject, StackObjectKind};

        #[derive(Clone)]
        struct PassiveAgent;
        impl Agent for PassiveAgent {
            fn decide(&mut self, _v: &PlayerView, _req: &DecisionRequest) -> DecisionResponse {
                DecisionResponse::Pass
            }
        }

        let run = |mana_spent: u32| -> (Option<i32>, Option<(ObjId, bool, Option<u32>)>) {
            // P0's library has a top card (last element = Lightning Bolt).
            let mut state = build_game(1, &[&[grp::FOREST, grp::LIGHTNING_BOLT], &[]]);
            let mascot = {
                let c = state.card_db().get(ELEMENTAL_MASCOT).unwrap().chars.clone();
                state.add_card(PlayerId(0), c, Zone::Battlefield)
            };
            let top_before = *state.player(PlayerId(0)).library.last().unwrap();
            // The triggering instant, with its mana-spent recorded.
            let bolt = {
                let c = state.card_db().get(grp::LIGHTNING_BOLT).unwrap().chars.clone();
                state.add_card(PlayerId(0), c, Zone::Stack)
            };
            state.objects.get_mut(&bolt).unwrap().mana_spent = mana_spent;
            let sid = StackId(1);
            state.stack.push(StackObject {
                id: sid,
                controller: PlayerId(0),
                source: None,
                kind: StackObjectKind::Spell(bolt),
                targets: vec![],
                x: None,
                modes: Vec::new(),
            });
            let mut e = Engine::new(state, vec![Box::new(PassiveAgent), Box::new(PassiveAgent)]);
            e.broadcast(GameEvent::SpellCast { spell: sid, controller: PlayerId(0) });
            e.run_agenda();
            e.resolve_top();
            let power = e.state.computed(mascot).power;
            let exiled = if e.state.player(PlayerId(0)).exile.contains(&top_before) {
                let o = e.state.object(top_before);
                Some((top_before, o.castable_from_exile, o.play_until_turn))
            } else {
                None
            };
            (power, exiled)
        };

        // <5 mana: +1/+0 only, no impulse.
        assert_eq!(run(3), (Some(2), None), "cheap spell → +1/+0, top card stays in library");
        // 5+ mana: +1/+0 and the top card is impulse-exiled, castable until end of your next turn
        // (resolved on turn 1, your turn → window closes after turn 3).
        let (power, exiled) = run(5);
        assert_eq!(power, Some(2));
        let (_, castable, until) = exiled.expect("top card exiled at 5 mana");
        assert!(castable, "granted play-from-exile permission");
        assert_eq!(until, Some(3));
    }

    /// Behaviour: an impulse-exiled LAND is offered as a `PlayLand` from exile within its window (on
    /// your main phase, land drop available), and not once the window has passed.
    #[test]
    fn elemental_mascot_impulse_exiled_land_is_playable_within_window() {
        use crate::agent::{PlayableAction, RandomAgent};
        use crate::basics::{Phase, Zone};
        use crate::cards::{build_game, grp};
        use crate::ids::PlayerId;
        use crate::priority::Engine;
        let mut state = build_game(1, &[&[], &[]]);
        state.phase = Phase::PrecombatMain; // sorcery speed, land drop available
        let forest = state.card_db().get(grp::FOREST).unwrap().chars.clone();
        let card = state.add_card(PlayerId(0), forest, Zone::Exile);
        {
            let o = state.objects.get_mut(&card).unwrap();
            o.castable_from_exile = true;
            o.play_until_turn = Some(3);
        }
        let e = Engine::new(
            state,
            vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))],
        );
        let offered = |e: &Engine| {
            e.legal_actions(PlayerId(0))
                .iter()
                .any(|a| matches!(a, PlayableAction::PlayLand { card: c } if *c == card))
        };
        assert!(offered(&e), "impulse-exiled land is offered as PlayLand within its window");
        // Past the window (turn 4 > 3): no longer offered.
        let mut e = e;
        e.state.turn_number = 4;
        assert!(!offered(&e), "no longer offered after the window expires");
    }
}
