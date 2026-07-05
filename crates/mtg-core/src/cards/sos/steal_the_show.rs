//! Steal the Show — `{2}{R}` Sorcery.
//!
//! Oracle: "Choose one or both —
//! • Target player discards any number of cards, then draws that many cards.
//! • Steal the Show deals damage equal to the number of instant and sorcery cards in your graveyard to
//!   target creature or planeswalker."
//!
//! **Fully implemented** — a `Modal{ choose one or both }`, ZERO new machinery (the ledger's "Native —
//! control-theft + wheel" tag was a stale misread; there is no control theft). Both modes compose over
//! existing caps:
//! - **Mode 1** (a targeted wheel) = [`Effect::TargetPlayer`] + [`Effect::DiscardChosen`] over that
//!   target + [`Effect::Draw`] of [`ValueExpr::DiscardedThisResolution`] — the Colossus / Borrowed
//!   Knowledge "discard any number, then draw that many" pattern, here pointed at a chosen player.
//! - **Mode 2** = [`Effect::DealDamage`] to a target creature-or-planeswalker, amount =
//!   [`ValueExpr::Count`] of instant/sorcery cards in YOUR graveyard.

use crate::basics::{CardType, Color, DamageKind, Zone};
use crate::cards::helpers::instant_or_sorcery;
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::target::{CardFilter, PlayerFilter, TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget, Mode};

/// grp id (per-set ids live near their cards).
pub const STEAL_THE_SHOW: u32 = 454;

pub fn register(db: &mut CardDb) {
    let wheel = Effect::Sequence(vec![
        Effect::TargetPlayer(PlayerFilter::Any),
        Effect::DiscardChosen { who: PlayerRef::ChosenTarget(0) },
        Effect::Draw {
            who: PlayerRef::ChosenTarget(0),
            count: ValueExpr::DiscardedThisResolution,
        },
    ]);
    let burn = Effect::DealDamage {
        amount: ValueExpr::Count {
            zone: Zone::Graveyard,
            filter: instant_or_sorcery(),
            controller: Some(PlayerRef::Controller),
        },
        to: EffectTarget::Target(TargetSpec {
            kind: TargetKind::Permanent(CardFilter::AnyOf(vec![
                CardFilter::HasCardType(CardType::Creature),
                CardFilter::HasCardType(CardType::Planeswalker),
            ])),
            min: 1,
            max: 1,
            distinct: true,
        }),
        kind: DamageKind::Noncombat,
    };
    let effect = Effect::Modal {
        modes: vec![
            Mode {
                label: "Target player discards any number of cards, then draws that many cards".to_string(),
                effect: wheel,
            },
            Mode {
                label: "Steal the Show deals damage equal to the number of instant and sorcery cards in your graveyard to target creature or planeswalker".to_string(),
                effect: burn,
            },
        ],
        min: 1,
        max: 2,
        allow_repeat: false,
    };
    let def = spell(
        STEAL_THE_SHOW,
        "Steal the Show",
        CardType::Sorcery,
        Color::Red,
        mana_cost(2, &[(Color::Red, 1)]),
        effect,
    )
    .with_text(
        "Choose one or both —\n• Target player discards any number of cards, then draws that many cards.\n• Steal the Show deals damage equal to the number of instant and sorcery cards in your graveyard to target creature or planeswalker.",
    );
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, CastVariant, DecisionRequest, DecisionResponse, PlayerView, RandomAgent, SelectReason};
    use crate::basics::{Phase, Target};
    use crate::cards::{grp, starter_db};
    use crate::ids::{ObjId, PlayerId};
    use crate::priority::Engine;
    use crate::state::GameState;
    use std::sync::Arc;

    fn db_with_card() -> CardDb {
        let mut db = starter_db();
        register(&mut db);
        db
    }

    #[test]
    fn steal_the_show_shape() {
        let db = db_with_card();
        let def = db.get(STEAL_THE_SHOW).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Sorcery]);
        assert_eq!(def.chars.colors, vec![Color::Red]);
        assert_eq!(def.chars.mana_value(), 3);
        assert!(def.fully_implemented);
        let Some(Effect::Modal { modes, min, max, .. }) = def.spell_effect() else { panic!("modal") };
        assert_eq!((modes.len(), *min, *max), (2, 1, 2), "choose one or both");
    }

    /// Drives modes + a fixed set of cards to discard (indices into a `SelectCards` reason=Discard),
    /// aiming any player/creature target at the configured pick.
    struct ShowAgent {
        modes: Vec<u32>,
        discard: Vec<u32>,
        target: Target,
    }
    impl Agent for ShowAgent {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::ChooseModes { .. } => DecisionResponse::Indices(self.modes.clone()),
                DecisionRequest::ChooseTargets { slots, .. } => {
                    let idx = slots[0]
                        .legal
                        .iter()
                        .position(|t| *t == self.target)
                        .unwrap_or(0);
                    DecisionResponse::Pairs(vec![(0, idx as u32)])
                }
                DecisionRequest::SelectCards { reason: SelectReason::Discard, .. } => {
                    DecisionResponse::Indices(self.discard.clone())
                }
                _ => DecisionResponse::Pass,
            }
        }
    }

    /// A state with P0 on turn, 3 untapped Mountains, the card db loaded.
    fn setup() -> GameState {
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(db_with_card()));
        for _ in 0..3 {
            let m = state.card_db().get(grp::MOUNTAIN).unwrap().chars.clone();
            state.add_card(PlayerId(0), m, Zone::Battlefield);
        }
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        state
    }

    /// Mode 1: target P0 (self), who discards 2 of their 3 hand cards then draws 2.
    #[test]
    fn mode1_wheel_discards_then_draws_that_many() {
        let mut state = setup();
        // P0 hand: Steal the Show + 3 fodder Bears; library non-empty so the draw resolves.
        let show = {
            let c = state.card_db().get(STEAL_THE_SHOW).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Hand)
        };
        for _ in 0..3 {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Hand);
        }
        for _ in 0..5 {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Library);
        }
        let agent = ShowAgent { modes: vec![0], discard: vec![0, 1], target: Target::Player(PlayerId(0)) };
        let mut e = Engine::new(state, vec![Box::new(agent), Box::new(RandomAgent::new(1))]);
        let hand_before = e.state.player(PlayerId(0)).hand.len(); // 4 (show + 3 bears)
        e.cast_spell(PlayerId(0), show, CastVariant::Normal);
        e.resolve_top();
        // Cast removed Steal the Show (→3 in hand), discarded 2 (→1), drew 2 (→3).
        assert_eq!(e.state.player(PlayerId(0)).hand.len(), hand_before - 1 - 2 + 2, "discard 2, draw 2");
        assert_eq!(e.state.player(PlayerId(0)).graveyard.iter().filter(|&&o| o != show).count(), 2, "2 discarded");
    }

    /// Mode 2: with 2 instant/sorcery cards in your graveyard, deal 2 to a target creature (a 2/2 dies).
    #[test]
    fn mode2_burns_for_instants_in_graveyard() {
        let mut state = setup();
        let show = {
            let c = state.card_db().get(STEAL_THE_SHOW).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Hand)
        };
        // Two instant/sorcery cards in P0's graveyard (Lightning Bolt = instant).
        for _ in 0..2 {
            let c = state.card_db().get(grp::LIGHTNING_BOLT).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Graveyard);
        }
        // A 2/2 target owned by P1.
        let bears: ObjId = {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            state.add_card(PlayerId(1), c, Zone::Battlefield)
        };
        let agent = ShowAgent { modes: vec![1], discard: vec![], target: Target::Object(bears) };
        let mut e = Engine::new(state, vec![Box::new(agent), Box::new(RandomAgent::new(1))]);
        e.cast_spell(PlayerId(0), show, CastVariant::Normal);
        e.resolve_top();
        e.run_agenda(); // collect the lethal-damage SBA
        assert!(e.state.player(PlayerId(1)).graveyard.contains(&bears), "2 damage killed the 2/2");
    }
}
