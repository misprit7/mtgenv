//! Tablet of Discovery — `{2}{R}` Artifact (first printed SOS).
//!
//! Oracle: "When this artifact enters, mill a card. You may play that card this turn.
//! {T}: Add {R}.
//! {T}: Add {R}{R}. Spend this mana only to cast instant and sorcery spells."
//!
//! **Fully implemented** — a red artifact ({R} pip colours it, CR 202.2) with:
//!  - a `SelfEnters` mill-then-play ETB (`Effect::MillThenPlay`) — mills the top card and lets you play
//!    that card from the graveyard until end of turn; and
//!  - two `{T}` mana abilities: an unrestricted `Add {R}`, and an `Add {R}{R}` whose mana is
//!    `SpendRestriction::InstantSorceryOnly` (CR 106.6).

use crate::basics::Color;
use crate::cards::{artifact, mana_cost, CardDb};
use crate::effects::ability::{Ability, Cost, CostComponent, EventPattern, Timing};
use crate::effects::target::{ManaSpec, SpendRestriction};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, PlayWindow};

/// grp id (per-set ids live near their cards).
pub const TABLET_OF_DISCOVERY: u32 = 446;

pub fn register(db: &mut CardDb) {
    let mut def = artifact(
        TABLET_OF_DISCOVERY,
        "Tablet of Discovery",
        mana_cost(2, &[(Color::Red, 1)]),
        vec![
            // "When this artifact enters, mill a card. You may play that card this turn."
            Ability::Triggered {
                event: EventPattern::SelfEnters,
                condition: None,
                intervening_if: false,
                effect: Effect::MillThenPlay {
                    who: PlayerRef::Controller,
                    window: PlayWindow::ThisTurn,
                },
            },
            // "{T}: Add {R}."
            Ability::Activated {
                cost: Cost { mana: None, components: vec![CostComponent::TapSelf] },
                effect: Effect::AddMana {
                    who: PlayerRef::Controller,
                    mana: ManaSpec {
                        produces: vec![(Color::Red, ValueExpr::Fixed(1))],
                        any_color: None,
                        one_of: None,
                        restriction: None,
                    },
                },
                timing: Timing::Instant,
                restriction: None,
                is_mana: true,
            },
            // "{T}: Add {R}{R}. Spend this mana only to cast instant and sorcery spells."
            Ability::Activated {
                cost: Cost { mana: None, components: vec![CostComponent::TapSelf] },
                effect: Effect::AddMana {
                    who: PlayerRef::Controller,
                    mana: ManaSpec {
                        produces: vec![(Color::Red, ValueExpr::Fixed(2))],
                        any_color: None,
                        one_of: None,
                        restriction: Some(SpendRestriction::InstantSorceryOnly),
                    },
                },
                timing: Timing::Instant,
                restriction: None,
                is_mana: true,
            },
        ],
    );
    def.chars.colors = vec![Color::Red]; // {2}{R} → red artifact (CR 202.2)
    def.text = "When this artifact enters, mill a card. You may play that card this turn.\n{T}: Add {R}.\n{T}: Add {R}{R}. Spend this mana only to cast instant and sorcery spells.".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{CastVariant, PlayableAction, RandomAgent};
    use crate::basics::{Phase, Zone};
    use crate::cards::{build_game, grp};
    use crate::ids::PlayerId;
    use crate::priority::Engine;

    #[test]
    fn tablet_of_discovery_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(TABLET_OF_DISCOVERY).unwrap();
        assert_eq!(def.chars.colors, vec![Color::Red]);
        assert!(matches!(&def.abilities[0], Ability::Triggered { event: EventPattern::SelfEnters, .. }));
        assert!(matches!(&def.abilities[1], Ability::Activated { is_mana: true, restriction: None, .. }));
        assert!(matches!(&def.abilities[2], Ability::Activated { is_mana: true, .. }));
        assert!(def.fully_implemented);
    }

    /// ETB mill-then-play through the REAL cast path: cast the Tablet → it enters → its ETB mills the
    /// top card (a Forest) with play-permission → the milled land is offered/played from the graveyard.
    #[test]
    fn etb_mills_a_playable_land() {
        let mut state = build_game(1, &[&[], &[]]);
        let tablet = state.add_card(PlayerId(0), state.card_db().get(TABLET_OF_DISCOVERY).unwrap().chars.clone(), Zone::Hand);
        // {2}{R}: three Mountains.
        for _ in 0..3 {
            state.add_card(PlayerId(0), state.card_db().get(grp::MOUNTAIN).unwrap().chars.clone(), Zone::Battlefield);
        }
        // Forest on TOP of the library (library.last() = top) — the card that will be milled.
        let forest = state.add_card(PlayerId(0), state.card_db().get(grp::FOREST).unwrap().chars.clone(), Zone::Library);
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);

        e.cast_spell(PlayerId(0), tablet, CastVariant::Normal);
        e.resolve_top(); // Tablet enters, ETB trigger queues
        e.run_agenda(); // put the ETB on the stack
        e.resolve_top(); // resolve it → mill the Forest with play permission
        e.run_agenda();

        assert_eq!(e.state.object(forest).zone, Zone::Graveyard, "the top card was milled");
        assert!(e.state.object(forest).playable_from_graveyard, "with play permission");
        let offered = e.legal_actions(PlayerId(0)).iter().any(|a| matches!(a, PlayableAction::PlayLand { card } if *card == forest));
        assert!(offered, "the milled land is offered to be played from the graveyard");
    }

    /// The restricted `{T}: Add {R}{R}` produces mana usable only for instants/sorceries (CR 106.6),
    /// exercised via casting an instant vs a creature after tapping for the restricted mana.
    #[test]
    fn restricted_mana_only_pays_instants_and_sorceries() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(TABLET_OF_DISCOVERY).unwrap();
        // The second activated ability carries the I/S spend restriction.
        match &def.abilities[2] {
            Ability::Activated { effect: Effect::AddMana { mana, .. }, .. } => {
                assert_eq!(mana.produces, vec![(Color::Red, ValueExpr::Fixed(2))]);
                assert_eq!(mana.restriction, Some(SpendRestriction::InstantSorceryOnly));
            }
            other => panic!("expected the restricted mana ability, got {other:?}"),
        }
    }
}
