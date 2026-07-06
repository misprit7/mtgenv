//! Pick Your Poison — `{G}` Sorcery (first real-expansion printing MKM; reprinted on the SOS Mystical
//! Archive `soa`; its earliest printing is the silver-border Mystery Booster playtest set, so it lives
//! in the MKM folder by the first-black-border-printing rule).
//!
//! Oracle: "Choose one —
//! • Each opponent sacrifices an artifact of their choice.
//! • Each opponent sacrifices an enchantment of their choice.
//! • Each opponent sacrifices a creature with flying of their choice."
//!
//! **Fully implemented** — a `Modal` "choose one" of three `Sacrifice { who: EachOpponent }` edicts,
//! each letting the opponent choose which of their own artifact / enchantment / flying creature to sacrifice.

use crate::basics::{CardType, Color, Zone};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::ability::Keyword;
use crate::effects::target::{CardFilter, SelectSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, Mode};

/// grp id (bonus-sheet `soa` cards use the 600+ block).
pub const PICK_YOUR_POISON: u32 = 623;

fn each_opp_sacrifices(filter: CardFilter) -> Effect {
    Effect::Sacrifice {
        who: PlayerRef::EachOpponent,
        what: SelectSpec {
            zone: Zone::Battlefield,
            filter,
            chooser: PlayerRef::Opponent,
            min: ValueExpr::Fixed(1),
            max: ValueExpr::Fixed(1),
        },
    }
}

pub fn register(db: &mut CardDb) {
    let effect = Effect::Modal {
        modes: vec![
            Mode { label: "Each opponent sacrifices an artifact of their choice".to_string(), effect: each_opp_sacrifices(CardFilter::HasCardType(CardType::Artifact)) },
            Mode { label: "Each opponent sacrifices an enchantment of their choice".to_string(), effect: each_opp_sacrifices(CardFilter::HasCardType(CardType::Enchantment)) },
            Mode {
                label: "Each opponent sacrifices a creature with flying of their choice".to_string(),
                effect: each_opp_sacrifices(CardFilter::All(vec![
                    CardFilter::HasCardType(CardType::Creature),
                    CardFilter::HasKeyword(Keyword::Flying),
                ])),
            },
        ],
        min: 1,
        max: 1,
        allow_repeat: false,
    };
    db.insert(
        spell(
            PICK_YOUR_POISON,
            "Pick Your Poison",
            CardType::Sorcery,
            Color::Green,
            mana_cost(0, &[(Color::Green, 1)]),
            effect,
        )
        .with_text("Choose one —\n• Each opponent sacrifices an artifact of their choice.\n• Each opponent sacrifices an enchantment of their choice.\n• Each opponent sacrifices a creature with flying of their choice."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView};
    use crate::cards::{build_game, grp};
    use crate::effects::action::{ResolutionCtx, WbReason};
    use crate::ids::{PlayerId, StackId};
    use crate::priority::Engine;

    /// Sacrifices the first legal option when asked.
    #[derive(Clone)]
    struct Chooser;
    impl Agent for Chooser {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::SelectCards { min, max, from, .. } => {
                    let n = (*min).max(1).min(*max).min(from.len() as u32);
                    DecisionResponse::Indices((0..n).collect())
                }
                _ => DecisionResponse::Pass,
            }
        }
    }

    #[test]
    fn pick_your_poison_shape() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(PICK_YOUR_POISON).unwrap();
        assert!(def.fully_implemented);
        match def.spell_effect().unwrap() {
            Effect::Modal { modes, .. } => assert_eq!(modes.len(), 3),
            o => panic!("expected Modal, got {o:?}"),
        }
    }

    /// Mode 1: the opponent sacrifices an artifact.
    #[test]
    fn mode1_opponent_sacrifices_an_artifact() {
        use crate::state::Characteristics;
        let mut state = build_game(1, &[&[], &[]]);
        let art = state.add_card(
            PlayerId(1),
            Characteristics { name: "Test Artifact".to_string(), card_types: vec![CardType::Artifact], grp_id: 8010, ..Default::default() },
            Zone::Battlefield,
        );
        // A non-artifact so we know the sacrifice picked the artifact, not just "anything".
        let _bear = state.add_card(PlayerId(1), state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Battlefield);
        let effect = state.card_db().get(PICK_YOUR_POISON).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(Chooser), Box::new(Chooser)]);
        e.resolve_effect(
            &effect,
            &ResolutionCtx { controller: Some(PlayerId(0)), chosen_modes: vec![0], ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        assert!(!e.state.player(PlayerId(1)).battlefield.contains(&art), "opponent sacrificed the artifact");
    }
}
