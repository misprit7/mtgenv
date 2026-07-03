//! Practiced Scrollsmith — `{R}{R/W}{W}` Creature — Dwarf Cleric 3/2 (first printed SOS).
//!
//! Oracle: "First strike / When this creature enters, exile target noncreature, nonland card from
//! your graveyard. Until the end of your next turn, you may cast that card."
//!
//! **Fully implemented** — printed First strike + a two-colour hybrid cost (`{R/W}`), plus an ETB
//! trigger that **impulse-exiles** (S15) a target noncreature/nonland card from *your* graveyard and
//! grants you permission to cast it until the end of your next turn
//! (`Effect::ExileForPlay { window: YourNextTurn }`). First consumer of the S15 impulse-play cap
//! (the engine base for which was adopted from an orphaned predecessor WIP).

use crate::basics::{CardType, Color, Zone};
use crate::cards::{creature, mana_cost_hybrid, CardDb};
use crate::effects::ability::{Ability, EventPattern, Keyword};
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::PlayerRef;
use crate::effects::{Effect, EffectTarget, PlayWindow};
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const PRACTICED_SCROLLSMITH: u32 = 318;

/// "target noncreature, nonland card from your graveyard" — a graveyard card's `controller` is its
/// owner (CR 108.4/400.7), so `ControlledBy(Controller)` scopes it to *your* graveyard.
fn noncreature_nonland_in_your_graveyard() -> TargetSpec {
    TargetSpec {
        kind: TargetKind::CardInZone {
            zone: Zone::Graveyard,
            filter: CardFilter::All(vec![
                CardFilter::ControlledBy(PlayerRef::Controller),
                CardFilter::Not(Box::new(CardFilter::HasCardType(CardType::Creature))),
                CardFilter::Not(Box::new(CardFilter::HasCardType(CardType::Land))),
            ]),
        },
        min: 1,
        max: 1,
        distinct: true,
    }
}

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        PRACTICED_SCROLLSMITH,
        "Practiced Scrollsmith",
        &[CreatureType::Dwarf, CreatureType::Cleric],
        Color::Red,
        mana_cost_hybrid(0, &[(Color::Red, 1), (Color::White, 1)], &[(Color::Red, Color::White)]),
        3,
        2,
        vec![Ability::Triggered {
            event: EventPattern::SelfEnters,
            condition: None,
            intervening_if: false,
            effect: Effect::ExileForPlay {
                what: EffectTarget::Target(noncreature_nonland_in_your_graveyard()),
                window: PlayWindow::YourNextTurn,
            },
        }],
    );
    def.chars.colors = vec![Color::Red, Color::White];
    def.chars.keywords = vec![Keyword::FirstStrike];
    def.text = "First strike\nWhen this creature enters, exile target noncreature, nonland card from your graveyard. Until the end of your next turn, you may cast that card.".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn practiced_scrollsmith_shape() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(PRACTICED_SCROLLSMITH).unwrap();
        assert_eq!(def.chars.colors, vec![Color::Red, Color::White]);
        assert_eq!(def.chars.keywords, vec![Keyword::FirstStrike]);
        assert_eq!(def.chars.mana_cost.as_ref().unwrap().hybrid, vec![(Color::Red, Color::White)]);
        assert_eq!(def.chars.mana_value(), 3, "{{R}}{{R/W}}{{W}} = MV 3");
        assert!(def.fully_implemented);
    }

    #[test]
    fn practiced_scrollsmith_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(PRACTICED_SCROLLSMITH).unwrap();
        expect![[r#"
            [
                Triggered {
                    event: SelfEnters,
                    condition: None,
                    intervening_if: false,
                    effect: ExileForPlay {
                        what: Target(
                            TargetSpec {
                                kind: CardInZone {
                                    zone: Graveyard,
                                    filter: All(
                                        [
                                            ControlledBy(
                                                Controller,
                                            ),
                                            Not(
                                                HasCardType(
                                                    Creature,
                                                ),
                                            ),
                                            Not(
                                                HasCardType(
                                                    Land,
                                                ),
                                            ),
                                        ],
                                    ),
                                },
                                min: 1,
                                max: 1,
                                distinct: true,
                            },
                        ),
                        window: YourNextTurn,
                    },
                },
            ]"#]].assert_eq(&format!("{:#?}", def.abilities));
    }

    /// Behaviour: the ETB impulse-exiles the chosen graveyard card and grants play-from-exile
    /// permission through the end of your next turn.
    #[test]
    fn practiced_scrollsmith_etb_impulse_exiles_and_grants_play() {
        use crate::agent::RandomAgent;
        use crate::basics::{Target, Zone};
        use crate::cards::{build_game, grp};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        let mut state = build_game(1, &[&[], &[]]);
        // A noncreature, nonland card (Lightning Bolt — instant) in P0's graveyard.
        let bolt = state.card_db().get(grp::LIGHTNING_BOLT).unwrap().chars.clone();
        let card = state.add_card(PlayerId(0), bolt, Zone::Graveyard);
        let etb = match &state.card_db().get(PRACTICED_SCROLLSMITH).unwrap().abilities[0] {
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
                chosen_targets: vec![Target::Object(card)],
                ..Default::default()
            },
            WbReason::Resolve(StackId(0)),
        );
        assert!(!e.state.players[0].graveyard.contains(&card), "left the graveyard");
        assert!(e.state.players[0].exile.contains(&card), "exiled");
        let o = e.state.object(card);
        assert!(o.castable_from_exile, "granted play-from-exile permission");
        // Resolved on turn 1, P0's own turn (active): "until end of your next turn" spans turn 1
        // (rest), turn 2 (opponent), turn 3 (your next) → the window closes after turn 3.
        assert_eq!(o.play_until_turn, Some(3));
    }

    /// Behaviour: an impulse-exiled instant is offered as a cast within its window and no longer once
    /// the window has passed (timing + expiry).
    #[test]
    fn practiced_scrollsmith_offer_respects_window() {
        use crate::agent::{PlayableAction, RandomAgent};
        use crate::basics::{Color, Zone};
        use crate::cards::{build_game, grp};
        use crate::ids::PlayerId;
        use crate::priority::Engine;
        let mut state = build_game(1, &[&[], &[]]);
        // A Lightning Bolt (instant, {R}) impulse-granted in P0's exile through turn 3.
        let bolt = state.card_db().get(grp::LIGHTNING_BOLT).unwrap().chars.clone();
        let card = state.add_card(PlayerId(0), bolt, Zone::Exile);
        {
            let o = state.objects.get_mut(&card).unwrap();
            o.castable_from_exile = true;
            o.play_until_turn = Some(3);
        }
        // Floating red mana so the {R} cost is affordable.
        *state.player_mut(PlayerId(0)).mana_pool.amounts.entry(Color::Red).or_insert(0) += 1;
        let e = Engine::new(
            state,
            vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))],
        );
        let offered = |e: &Engine| {
            e.legal_actions(PlayerId(0))
                .iter()
                .any(|a| matches!(a, PlayableAction::Cast { spell, .. } if *spell == card))
        };
        // Within the window (turn 1 <= 3): offered even though it's an instant on exile.
        assert!(offered(&e), "impulse-exiled instant is offered within its window");
        // Past the window (turn 4 > 3): no longer offered.
        let mut e = e;
        e.state.turn_number = 4;
        assert!(!offered(&e), "no longer offered after the window expires");
    }
}
