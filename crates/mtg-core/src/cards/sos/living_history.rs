//! Living History — `{1}{R}` Enchantment (first printed SOS).
//!
//! Oracle:
//! - "When this enchantment enters, create a 2/2 red and white Spirit creature token."
//! - "Whenever you attack, if a card left your graveyard this turn, target attacking creature gets
//!   +2/+0 until end of turn."
//!
//! **Fully implemented** — the ETB creates the shared `spirit_token()` (2/2 red-white Spirit); the
//! attack trigger is a `YouAttack` ability gated by an intervening-if `CardLeftGraveyardThisTurn`
//! (S9), whose effect pumps a **target attacking creature** +2/+0 until end of turn. The target uses
//! the new `CardFilter::Attacking` (matches a current declared attacker, CR 508.1).

use crate::basics::{Color, Zone};
use crate::cards::helpers::spirit_token;
use crate::cards::{enchantment, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern};
use crate::effects::condition::{Condition, Duration};
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};

/// grp id (per-set ids live near their cards).
pub const LIVING_HISTORY: u32 = 352;

pub fn register(db: &mut CardDb) {
    db.insert(
        enchantment(
            LIVING_HISTORY,
            "Living History",
            Color::Red,
            mana_cost(1, &[(Color::Red, 1)]),
            vec![
                // "When this enchantment enters, create a 2/2 red and white Spirit creature token."
                Ability::Triggered {
                    event: EventPattern::SelfEnters,
                    condition: None,
                    intervening_if: false,
                    effect: Effect::CreateToken {
                        spec: spirit_token(),
                        count: ValueExpr::Fixed(1),
                        controller: PlayerRef::Controller,
                        dynamic_counters: Vec::new(),
                    },
                },
                // "Whenever you attack, if a card left your graveyard this turn, target attacking
                // creature gets +2/+0 until end of turn." Intervening-if (CR 603.4) on S9.
                Ability::Triggered {
                    event: EventPattern::YouAttack,
                    condition: Some(Condition::CardLeftGraveyardThisTurn {
                        who: PlayerRef::Controller,
                    }),
                    intervening_if: true,
                    effect: Effect::PumpPT {
                        what: EffectTarget::Target(TargetSpec {
                            kind: TargetKind::Creature(CardFilter::Attacking),
                            min: 1,
                            max: 1,
                            distinct: true,
                        }),
                        power: ValueExpr::Fixed(2),
                        toughness: ValueExpr::Fixed(0),
                        duration: Duration::UntilEndOfTurn,
                    },
                },
            ],
        )
        .with_text("When this enchantment enters, create a 2/2 red and white Spirit creature token.\nWhenever you attack, if a card left your graveyard this turn, target attacking creature gets +2/+0 until end of turn."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::RandomAgent;
    use crate::basics::{CardType, Target};
    use crate::cards::{build_game, grp};
    use crate::combat::{Attack, CombatState};
    use crate::effects::action::{ResolutionCtx, WbReason};
    use crate::ids::{PlayerId, StackId};
    use crate::priority::Engine;
    use expect_test::expect;

    #[test]
    fn living_history_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(LIVING_HISTORY).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Enchantment]);
        assert!(def.fully_implemented);
        expect![[r#"
            [
                Triggered {
                    event: SelfEnters,
                    condition: None,
                    intervening_if: false,
                    effect: CreateToken {
                        spec: TokenSpec {
                            name: "Spirit",
                            card_types: [
                                Creature,
                            ],
                            subtypes: [
                                Creature(
                                    Spirit,
                                ),
                            ],
                            colors: [
                                Red,
                                White,
                            ],
                            power: 2,
                            toughness: 2,
                            keywords: [],
                            counters: [],
                            grp_id: 0,
                        },
                        count: Fixed(
                            1,
                        ),
                        controller: Controller,
                        dynamic_counters: [],
                    },
                },
                Triggered {
                    event: YouAttack,
                    condition: Some(
                        CardLeftGraveyardThisTurn {
                            who: Controller,
                        },
                    ),
                    intervening_if: true,
                    effect: PumpPT {
                        what: Target(
                            TargetSpec {
                                kind: Creature(
                                    Attacking,
                                ),
                                min: 1,
                                max: 1,
                                distinct: true,
                            },
                        ),
                        power: Fixed(
                            2,
                        ),
                        toughness: Fixed(
                            0,
                        ),
                        duration: UntilEndOfTurn,
                    },
                },
            ]"#]]
        .assert_eq(&format!("{:#?}", def.abilities));
    }

    /// The ETB creates one 2/2 Spirit token.
    #[test]
    fn etb_makes_a_spirit() {
        let state = build_game(1, &[&[], &[]]);
        let etb = match &state.card_db().get(LIVING_HISTORY).unwrap().abilities[0] {
            Ability::Triggered { effect, .. } => effect.clone(),
            o => panic!("expected ETB Triggered, got {o:?}"),
        };
        let mut e = Engine::new(
            state,
            vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))],
        );
        let before = e.state.player(PlayerId(0)).battlefield.len();
        e.resolve_effect(
            &etb,
            &ResolutionCtx { controller: Some(PlayerId(0)), ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        let bf = &e.state.player(PlayerId(0)).battlefield;
        assert_eq!(bf.len(), before + 1, "one Spirit token created");
        let token = *bf.last().unwrap();
        let cc = e.state.computed(token);
        assert_eq!((cc.power, cc.toughness), (Some(2), Some(2)), "2/2 Spirit");
    }

    /// Real-path: with an attacker declared and a card having left the graveyard, the `YouAttack`
    /// trigger fires and pumps the attacker +2/+0. With nothing gone from the graveyard, the
    /// intervening-if fails and the attacker is unpumped.
    fn run_attack(left_gy: bool) -> (Option<i32>, Option<i32>) {
        use crate::agent::GameEvent;
        let mut state = build_game(1, &[&[], &[]]);
        let hist = state.card_db().get(LIVING_HISTORY).unwrap().chars.clone();
        state.add_card(PlayerId(0), hist, Zone::Battlefield);
        let bears = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
        let attacker = state.add_card(PlayerId(0), bears, Zone::Battlefield);
        // Declare the Bears as an attacker (CR 508.1) so `CardFilter::Attacking` matches it.
        state.combat = Some(CombatState {
            attackers: vec![Attack { attacker, defender: Target::Player(PlayerId(1)) }],
            blocks: vec![],
        });
        if left_gy {
            state.player_mut(PlayerId(0)).cards_left_graveyard_this_turn = 1;
        }
        state.active_player = PlayerId(0);
        let mut e = Engine::new(
            state,
            vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))],
        );
        // Fire "you attack" (CR 508.1 fires after attackers are declared).
        e.broadcast(GameEvent::AttackersDeclared { attackers: vec![attacker], by: PlayerId(0) });
        e.run_agenda();
        while !e.state.stack.items.is_empty() {
            e.resolve_top();
            e.run_agenda();
        }
        let cc = e.state.computed(attacker);
        (cc.power, cc.toughness)
    }

    #[test]
    fn attack_pumps_when_a_card_left_the_graveyard() {
        assert_eq!(run_attack(true), (Some(4), Some(2)), "2/2 → +2/+0 → 4/2");
    }

    #[test]
    fn no_pump_without_a_graveyard_leave() {
        assert_eq!(run_attack(false), (Some(2), Some(2)), "intervening-if fails → unpumped 2/2");
    }
}
