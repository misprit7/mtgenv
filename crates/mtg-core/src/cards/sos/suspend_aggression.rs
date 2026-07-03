//! Suspend Aggression — `{1}{R}{W}` Instant (first printed SOS).
//!
//! Oracle: "Exile target nonland permanent and the top card of your library. For each of those
//! cards, its owner may play it until the end of their next turn."
//!
//! **Fully implemented** — a `Sequence` of two S15 impulse-exiles: the targeted nonland permanent
//! (`ExileForPlay { Target, YourNextTurn }`) and the top card of your library
//! (`ExileForPlay { TopOfLibrary(Controller), YourNextTurn }`). The `ExileForPlay` arm keys the play
//! window off **each exiled card's owner**, so "until the end of their next turn" falls out for free:
//! an opponent-owned permanent becomes playable through the opponent's next turn, your top card
//! through yours. (A removal-and-tempo swing: the exiled permanent can be recast by its owner.)

use crate::basics::{CardType, Color};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::PlayerRef;
use crate::effects::{Effect, EffectTarget, PlayWindow};

/// grp id (per-set ids live near their cards).
pub const SUSPEND_AGGRESSION: u32 = 320;

pub fn register(db: &mut CardDb) {
    let effect = Effect::Sequence(vec![
        // "Exile target nonland permanent … its owner may play it until the end of their next turn."
        Effect::ExileForPlay {
            what: EffectTarget::Target(TargetSpec {
                kind: TargetKind::Permanent(CardFilter::Not(Box::new(CardFilter::HasCardType(
                    CardType::Land,
                )))),
                min: 1,
                max: 1,
                distinct: true,
            }),
            window: PlayWindow::YourNextTurn,
        },
        // "…and the top card of your library …" (its owner = you → through your next turn).
        Effect::ExileForPlay {
            what: EffectTarget::TopOfLibrary(PlayerRef::Controller),
            window: PlayWindow::YourNextTurn,
        },
    ]);
    let mut def = spell(
        SUSPEND_AGGRESSION,
        "Suspend Aggression",
        CardType::Instant,
        Color::Red,
        mana_cost(1, &[(Color::Red, 1), (Color::White, 1)]),
        effect,
    )
    .with_text("Exile target nonland permanent and the top card of your library. For each of those cards, its owner may play it until the end of their next turn.");
    def.chars.colors = vec![Color::Red, Color::White];
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn suspend_aggression_shape() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(SUSPEND_AGGRESSION).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Instant]);
        assert_eq!(def.chars.colors, vec![Color::Red, Color::White]);
        assert_eq!(def.chars.mana_value(), 3);
        assert!(def.fully_implemented);
    }

    #[test]
    fn suspend_aggression_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(SUSPEND_AGGRESSION).unwrap();
        expect![[r#"
            Sequence(
                [
                    ExileForPlay {
                        what: Target(
                            TargetSpec {
                                kind: Permanent(
                                    Not(
                                        HasCardType(
                                            Land,
                                        ),
                                    ),
                                ),
                                min: 1,
                                max: 1,
                                distinct: true,
                            },
                        ),
                        window: YourNextTurn,
                    },
                    ExileForPlay {
                        what: TopOfLibrary(
                            Controller,
                        ),
                        window: YourNextTurn,
                    },
                ],
            )"#]].assert_eq(&format!("{:#?}", def.spell_effect().unwrap()));
    }

    /// Behaviour: resolving Suspend Aggression exiles both the targeted opponent permanent and your
    /// own top card, each impulse-granted through its **owner's** next turn (opponent's = turn+1,
    /// yours = turn+2 on your own turn).
    #[test]
    fn suspend_aggression_exiles_both_with_per_owner_windows() {
        use crate::agent::RandomAgent;
        use crate::basics::{Target, Zone};
        use crate::cards::{build_game, grp};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        // P0 casts on their own turn (turn 1). P0's library has a top card; P1 has a creature.
        let mut state = build_game(1, &[&[grp::FOREST, grp::LIGHTNING_BOLT], &[]]);
        let top_before = *state.player(PlayerId(0)).library.last().unwrap();
        let bears = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
        let victim = state.add_card(PlayerId(1), bears, Zone::Battlefield);
        let effect = state.card_db().get(SUSPEND_AGGRESSION).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(
            state,
            vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))],
        );
        e.resolve_effect(
            &effect,
            &ResolutionCtx {
                controller: Some(PlayerId(0)),
                chosen_targets: vec![Target::Object(victim)],
                ..Default::default()
            },
            WbReason::Resolve(StackId(0)),
        );
        // The opponent's permanent: exiled, playable through the opponent's next turn (turn 2).
        assert!(!e.state.players[1].battlefield.contains(&victim), "permanent left the battlefield");
        assert!(e.state.players[1].exile.contains(&victim), "in its owner's (P1) exile");
        let vo = e.state.object(victim);
        assert!(vo.castable_from_exile);
        assert_eq!(vo.play_until_turn, Some(2), "opponent's next turn");
        // Your top card: exiled, playable through your next turn (turn 3).
        assert!(e.state.players[0].exile.contains(&top_before), "top card in your exile");
        let to = e.state.object(top_before);
        assert!(to.castable_from_exile);
        assert_eq!(to.play_until_turn, Some(3), "your next turn");
    }
}
