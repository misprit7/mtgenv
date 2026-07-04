//! Summoned Dromedary — `{3}{W}` Creature — Spirit Camel 4/3 (first printed SOS).
//!
//! Oracle: "Vigilance. {1}{W}: Return this card from your graveyard to your hand. Activate only as
//! a sorcery."
//!
//! **Fully implemented** — a 4/3 vigilance body plus a **graveyard-recursion** activated ability. The
//! ability is marked `CostComponent::ActivateFromGraveyard` (a pure graveyard-usability marker: the
//! effect returns the source, so the cost has nothing to exile — cf. S18's `ExileSelfFromGraveyard`
//! which is both marker AND cost) and returns the source to hand via `MoveZone { SourceSelf → Hand }`.
//! Sorcery-timed (`Restriction`/`Timing::Sorcery`).

use crate::basics::{Color, Zone, ZoneDest, ZonePos};
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, Cost, CostComponent, Keyword, Timing};
use crate::effects::{Effect, EffectTarget};
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const SUMMONED_DROMEDARY: u32 = 355;

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        SUMMONED_DROMEDARY,
        "Summoned Dromedary",
        &[CreatureType::Spirit, CreatureType::Camel],
        Color::White,
        mana_cost(3, &[(Color::White, 1)]),
        4,
        3,
        vec![Ability::Activated {
            cost: Cost {
                mana: Some(mana_cost(1, &[(Color::White, 1)])),
                components: vec![CostComponent::ActivateFromGraveyard],
            },
            // "Return this card from your graveyard to your hand."
            effect: Effect::MoveZone {
                what: EffectTarget::SourceSelf,
                to: ZoneDest { zone: Zone::Hand, pos: ZonePos::Any },
            },
            // "Activate only as a sorcery." (CR 601.3e — main-phase, empty stack, your turn.)
            timing: Timing::Sorcery,
            restriction: None,
            is_mana: false,
        }],
    )
    .with_text(
        "Vigilance\n{1}{W}: Return this card from your graveyard to your hand. Activate only as a sorcery.",
    );
    def.chars.keywords = vec![Keyword::Vigilance];
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{PlayableAction, RandomAgent};
    use crate::basics::Phase;
    use crate::cards::build_game;
    use crate::ids::PlayerId;
    use crate::priority::Engine;

    #[test]
    fn summoned_dromedary_is_graveyard_activated() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(SUMMONED_DROMEDARY).unwrap();
        assert!(def.fully_implemented);
        assert!(def.chars.keywords.contains(&Keyword::Vigilance));
        let gy_marker = matches!(&def.abilities[0], Ability::Activated { cost, .. }
            if cost.components.iter().any(|c| matches!(c, CostComponent::ActivateFromGraveyard)));
        assert!(gy_marker, "carries the graveyard-usability marker (no exile cost)");
    }

    /// Real-path: the card sits in the graveyard; with `{1}{W}` available on P0's turn, the engine
    /// offers the graveyard ability. Activating it and resolving returns the card to hand.
    #[test]
    fn returns_self_from_graveyard_to_hand() {
        // P0 has two Plains in play (for {1}{W}) and Summoned Dromedary in the graveyard.
        let mut state = build_game(1, &[&[], &[]]);
        let plains = state.card_db().get(crate::cards::grp::PLAINS).unwrap().chars.clone();
        let _p1 = state.add_card(PlayerId(0), plains.clone(), Zone::Battlefield);
        let _p2 = state.add_card(PlayerId(0), plains, Zone::Battlefield);
        let card = state.add_card(
            PlayerId(0),
            state.card_db().get(SUMMONED_DROMEDARY).unwrap().chars.clone(),
            Zone::Graveyard,
        );
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain; // sorcery-speed timing
        let mut e = Engine::new(
            state,
            vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))],
        );
        // The graveyard ability is offered at sorcery speed on P0's main phase.
        let actions = e.legal_actions(PlayerId(0));
        let act = actions.iter().find_map(|a| match a {
            PlayableAction::Activate { source, ability } if *source == card => Some(*ability),
            _ => None,
        });
        assert!(act.is_some(), "graveyard recursion ability is offered");
        e.activate_ability(PlayerId(0), card, act.unwrap());
        // Resolve the ability off the stack.
        e.resolve_top();
        assert!(e.state.player(PlayerId(0)).hand.contains(&card), "card returned to hand");
        assert!(!e.state.player(PlayerId(0)).graveyard.contains(&card), "no longer in graveyard");
    }
}
