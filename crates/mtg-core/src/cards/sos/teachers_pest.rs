//! Teacher's Pest — `{B}{G}` Creature — Skeleton Pest 1/1 (first printed SOS).
//!
//! Oracle: "Menace. Whenever this creature attacks, you gain 1 life. {B}{G}: Return this card from
//! your graveyard to the battlefield tapped."
//!
//! **Fully implemented** — Menace body + a `SelfAttacks` gain-life trigger + a **graveyard-recursion**
//! activated ability that returns the source to the battlefield **tapped**. Completes the
//! graveyard-recursion trio: `CostComponent::ActivateFromGraveyard` (the pure gy-usability marker,
//! agent-8) + `Effect::MoveZone { tapped: true }` (enters-tapped, new — the `Search { tapped }`
//! analogue for reanimation). Activatable any time (no "as a sorcery" restriction), so `Timing::Instant`.

use crate::basics::{Color, Zone, ZoneDest, ZonePos};
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, Cost, CostComponent, EventPattern, Keyword, Timing};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const TEACHERS_PEST: u32 = 360;

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        TEACHERS_PEST,
        "Teacher's Pest",
        &[CreatureType::Skeleton, CreatureType::Pest],
        Color::Black,
        mana_cost(0, &[(Color::Black, 1), (Color::Green, 1)]),
        1,
        1,
        vec![
            // "Whenever this creature attacks, you gain 1 life."
            Ability::Triggered {
                event: EventPattern::SelfAttacks,
                condition: None,
                intervening_if: false,
                effect: Effect::GainLife { who: PlayerRef::Controller, amount: ValueExpr::Fixed(1) },
            },
            // "{B}{G}: Return this card from your graveyard to the battlefield tapped."
            Ability::Activated {
                cost: Cost {
                    mana: Some(mana_cost(0, &[(Color::Black, 1), (Color::Green, 1)])),
                    components: vec![CostComponent::ActivateFromGraveyard],
                },
                effect: Effect::MoveZone {
                    what: EffectTarget::SourceSelf,
                    to: ZoneDest { zone: Zone::Battlefield, pos: ZonePos::Any },
                    tapped: true,
                },
                timing: Timing::Instant,
                restriction: None,
                is_mana: false,
            },
        ],
    )
    .with_text(
        "Menace\nWhenever this creature attacks, you gain 1 life.\n{B}{G}: Return this card from your graveyard to the battlefield tapped.",
    );
    def.chars.colors = vec![Color::Black, Color::Green];
    def.chars.keywords = vec![Keyword::Menace];
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{PlayableAction, RandomAgent};
    use crate::basics::Phase;
    use crate::cards::{build_game, grp};
    use crate::ids::PlayerId;
    use crate::priority::Engine;

    #[test]
    fn teachers_pest_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(TEACHERS_PEST).unwrap();
        assert!(def.fully_implemented);
        assert!(def.chars.keywords.contains(&Keyword::Menace));
        assert_eq!(def.chars.colors, vec![Color::Black, Color::Green]);
        let gy_marker = matches!(&def.abilities[1], Ability::Activated { cost, effect, .. }
            if cost.components.iter().any(|c| matches!(c, CostComponent::ActivateFromGraveyard))
                && matches!(effect, Effect::MoveZone { tapped: true, .. }));
        assert!(gy_marker, "graveyard→battlefield-tapped recursion ability");
    }

    /// Real-path: the card sits in the graveyard; with `{B}{G}` available, the engine offers the
    /// graveyard ability. Activating and resolving returns it to the battlefield **tapped**.
    #[test]
    fn returns_self_from_graveyard_to_battlefield_tapped() {
        let mut state = build_game(1, &[&[], &[]]);
        // {B}{G}: a Swamp + a Forest.
        let swamp = state.card_db().get(grp::SWAMP).unwrap().chars.clone();
        let forest = state.card_db().get(grp::FOREST).unwrap().chars.clone();
        state.add_card(PlayerId(0), swamp, Zone::Battlefield);
        state.add_card(PlayerId(0), forest, Zone::Battlefield);
        let card = state.add_card(
            PlayerId(0),
            state.card_db().get(TEACHERS_PEST).unwrap().chars.clone(),
            Zone::Graveyard,
        );
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);

        let actions = e.legal_actions(PlayerId(0));
        let act = actions.iter().find_map(|a| match a {
            PlayableAction::Activate { source, ability } if *source == card => Some(*ability),
            _ => None,
        });
        assert!(act.is_some(), "graveyard recursion ability is offered");
        e.activate_ability(PlayerId(0), card, act.unwrap());
        e.resolve_top();
        assert!(e.state.player(PlayerId(0)).battlefield.contains(&card), "returned to the battlefield");
        assert!(!e.state.player(PlayerId(0)).graveyard.contains(&card), "no longer in graveyard");
        assert!(e.state.object(card).status.tapped, "entered the battlefield tapped");
    }
}
