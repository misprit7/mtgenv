//! Zaffai and the Tempests — `{5}{U}{R}` Legendary Creature — Human Bard Sorcerer (5/7).
//!
//! Oracle: "Once during each of your turns, you may cast an instant or sorcery spell from your hand
//! without paying its mana cost."
//!
//! **Fully implemented** — a static permission, [`StaticContribution::FreeCastFromHandOncePerTurn`], the
//! player-level analogue of `ExtraLandPlays`/`PlayLandsFrom`. The priority-action builder offers a
//! [`crate::agent::PlayableAction::CastFreeFromHand`] for each matching hand card on Zaffai's controller's
//! turn while Zaffai is unused (`used_once_per_turn`); casting it uses [`crate::agent::CastVariant::
//! WithoutPayingManaCost`] and spends the once-per-turn permission (reset at the controller's next turn).

use crate::basics::{CardType, Color, Zone};
use crate::cards::helpers::instant_or_sorcery;
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, StaticContribution};
use crate::effects::condition::Duration;
use crate::effects::target::{CardFilter, SelectSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::subtypes::{CreatureType, Supertype};

/// grp id (per-set ids live near their cards).
pub const ZAFFAI_AND_THE_TEMPESTS: u32 = 456;

/// "Once during each of your turns, you may cast an instant or sorcery spell from your hand without
/// paying its mana cost." A static permission the priority-action builder reads (not painted on objects).
fn free_cast_is() -> Ability {
    Ability::Static {
        contribution: StaticContribution::FreeCastFromHandOncePerTurn { filter: instant_or_sorcery() },
        affects: SelectSpec {
            zone: Zone::Battlefield,
            filter: CardFilter::ItSelf,
            chooser: PlayerRef::Controller,
            min: ValueExpr::Fixed(0),
            max: ValueExpr::Fixed(0),
        },
        duration: Duration::WhileSourcePresent,
    }
}

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        ZAFFAI_AND_THE_TEMPESTS,
        "Zaffai and the Tempests",
        &[CreatureType::Human, CreatureType::Bard, CreatureType::Sorcerer],
        Color::Blue,
        mana_cost(5, &[(Color::Blue, 1), (Color::Red, 1)]),
        5,
        7,
        vec![free_cast_is()],
    );
    def.chars.supertypes = vec![Supertype::Legendary];
    def.chars.colors = vec![Color::Blue, Color::Red];
    def.text = "Once during each of your turns, you may cast an instant or sorcery spell from your hand without paying its mana cost.".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayableAction, PlayerView};
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
    fn zaffai_shape() {
        let db = db_with_card();
        let def = db.get(ZAFFAI_AND_THE_TEMPESTS).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Creature]);
        assert_eq!(def.chars.colors, vec![Color::Blue, Color::Red]);
        assert_eq!((def.chars.power, def.chars.toughness), (Some(5), Some(7)));
        assert_eq!(def.chars.supertypes, vec![Supertype::Legendary]);
        assert!(matches!(
            def.abilities[0],
            Ability::Static { contribution: StaticContribution::FreeCastFromHandOncePerTurn { .. }, .. }
        ));
    }

    /// Aims any target at P1 (for the free-cast bolt); passes otherwise.
    struct TargetP1;
    impl Agent for TargetP1 {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::ChooseTargets { slots, .. } => {
                    let idx = slots[0]
                        .legal
                        .iter()
                        .position(|t| *t == Target::Player(PlayerId(1)))
                        .unwrap_or(0);
                    DecisionResponse::Pairs(vec![(0, idx as u32)])
                }
                _ => DecisionResponse::Pass,
            }
        }
    }

    fn setup() -> (Engine, ObjId, ObjId) {
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(db_with_card()));
        // Zaffai on P0's battlefield, unused; a Lightning Bolt in P0's hand, NO lands (proving free-cast).
        let zaffai = {
            let c = state.card_db().get(ZAFFAI_AND_THE_TEMPESTS).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        state.objects.get_mut(&zaffai).unwrap().summoning_sick = false;
        let bolt = {
            let c = state.card_db().get(grp::LIGHTNING_BOLT).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Hand)
        };
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let e = Engine::new(state, vec![Box::new(TargetP1), Box::new(TargetP1)]);
        (e, zaffai, bolt)
    }

    /// The free-cast offer appears (with zero mana available), casting for free hits the opponent, and the
    /// permission is spent (offer gone) until Zaffai's next turn.
    #[test]
    fn free_casts_an_instant_once_per_turn() {
        let (mut e, zaffai, bolt) = setup();
        // Offer present despite no mana (it's free).
        assert!(
            e.legal_actions(PlayerId(0))
                .iter()
                .any(|a| matches!(a, PlayableAction::CastFreeFromHand { source, spell } if *source == zaffai && *spell == bolt)),
            "free-cast offer present with zero mana"
        );

        // Take it: bolt at P1, free; Zaffai is then spent for the turn.
        let p1_start = e.state.player(PlayerId(1)).life;
        e.cast_free_from_hand(PlayerId(0), zaffai, bolt);
        e.resolve_top();
        assert_eq!(e.state.player(PlayerId(1)).life, p1_start - 3, "free bolt dealt 3");
        assert!(e.state.object(zaffai).used_once_per_turn, "permission marked used");
        // Offer gone this turn.
        assert!(
            !e.legal_actions(PlayerId(0))
                .iter()
                .any(|a| matches!(a, PlayableAction::CastFreeFromHand { .. })),
            "permission spent for the turn"
        );
    }
}
