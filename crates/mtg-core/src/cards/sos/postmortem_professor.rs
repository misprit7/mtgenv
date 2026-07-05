//! Postmortem Professor — `{1}{B}` Creature — Zombie Warlock 2/2 (first printed SOS).
//!
//! Oracle: "This creature can't block. Whenever this creature attacks, each opponent loses 1 life
//! and you gain 1 life. {1}{B}, Exile an instant or sorcery card from your graveyard: Return this
//! card from your graveyard to the battlefield."
//!
//! **Fully implemented** — a self `CantBlock` qualification (Layer-6 static painted on itself) + a
//! `SelfAttacks` drain trigger + a **graveyard-recursion** activated ability whose cost includes
//! exiling *another* instant/sorcery card from the graveyard (the newly-wired `CostComponent::Exile`
//! fuel cost) alongside the `ActivateFromGraveyard` marker; the effect returns the source to the
//! battlefield. Activatable any time (no sorcery restriction).

use crate::basics::{CardType, Color, Zone, ZoneDest, ZonePos};
use crate::cards::helpers::itself;
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{
    Ability, Cost, CostComponent, EventPattern, Qualification, StaticContribution, Timing,
};
use crate::effects::condition::Duration;
use crate::effects::target::{CardFilter, SelectSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const POSTMORTEM_PROFESSOR: u32 = 361;

/// "an instant or sorcery card in your graveyard" — the exile-cost fuel selector.
fn instant_or_sorcery_in_graveyard() -> SelectSpec {
    SelectSpec {
        zone: Zone::Graveyard,
        filter: CardFilter::AnyOf(vec![
            CardFilter::HasCardType(CardType::Instant),
            CardFilter::HasCardType(CardType::Sorcery),
        ]),
        chooser: PlayerRef::Controller,
        min: ValueExpr::Fixed(1),
        max: ValueExpr::Fixed(1),
    }
}

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        POSTMORTEM_PROFESSOR,
        "Postmortem Professor",
        &[CreatureType::Zombie, CreatureType::Warlock],
        Color::Black,
        mana_cost(1, &[(Color::Black, 1)]),
        2,
        2,
        vec![
            // "This creature can't block." (CR 509.1b — Layer-6 qualification on itself.)
            Ability::Static {
                contribution: StaticContribution::Qualification(Qualification::CantBlock),
                affects: itself(),
                duration: Duration::WhileSourcePresent,
            },
            // "Whenever this creature attacks, each opponent loses 1 life and you gain 1 life."
            Ability::Triggered {
                event: EventPattern::SelfAttacks,
                condition: None,
                intervening_if: false,
                effect: Effect::Sequence(vec![
                    Effect::LoseLife { who: PlayerRef::EachOpponent, amount: ValueExpr::Fixed(1) },
                    Effect::GainLife { who: PlayerRef::Controller, amount: ValueExpr::Fixed(1) },
                ]),
            },
            // "{1}{B}, Exile an I/S card from your graveyard: Return this card from your gy to the bf."
            Ability::Activated {
                cost: Cost {
                    mana: Some(mana_cost(1, &[(Color::Black, 1)])),
                    components: vec![
                        CostComponent::Exile(instant_or_sorcery_in_graveyard()),
                        CostComponent::ActivateFromGraveyard,
                    ],
                },
                effect: Effect::MoveZone {
                    what: EffectTarget::SourceSelf,
                    to: ZoneDest { zone: Zone::Battlefield, pos: ZonePos::Any },
                    tapped: false,
                },
                timing: Timing::Instant,
                restriction: None,
                is_mana: false,
            },
        ],
    )
    .with_text(
        "This creature can't block.\nWhenever this creature attacks, each opponent loses 1 life and you gain 1 life.\n{1}{B}, Exile an instant or sorcery card from your graveyard: Return this card from your graveyard to the battlefield.",
    );
    def.chars.colors = vec![Color::Black];
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
    fn postmortem_professor_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(POSTMORTEM_PROFESSOR).unwrap();
        assert!(def.fully_implemented);
        let cant_block = matches!(&def.abilities[0], Ability::Static { contribution, .. }
            if matches!(contribution, StaticContribution::Qualification(Qualification::CantBlock)));
        assert!(cant_block, "self CantBlock static");
        let gy_reanimate = matches!(&def.abilities[2], Ability::Activated { cost, .. }
            if cost.components.iter().any(|c| matches!(c, CostComponent::Exile(_)))
                && cost.components.iter().any(|c| matches!(c, CostComponent::ActivateFromGraveyard)));
        assert!(gy_reanimate, "exile-a-gy-card + graveyard-activated reanimation");
    }

    /// Real-path: Postmortem sits in the graveyard alongside an instant. With {1}{B} available, the
    /// engine offers its graveyard ability; paying it exiles the instant and returns Postmortem to
    /// the battlefield.
    #[test]
    fn reanimates_by_exiling_an_instant_from_graveyard() {
        let mut state = build_game(1, &[&[], &[]]);
        // {1}{B}: a Swamp + an Island.
        for grp_id in [grp::SWAMP, grp::ISLAND] {
            let c = state.card_db().get(grp_id).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield);
        }
        let pm = state.add_card(
            PlayerId(0),
            state.card_db().get(POSTMORTEM_PROFESSOR).unwrap().chars.clone(),
            Zone::Graveyard,
        );
        // An instant in the graveyard to exile as the fuel.
        let bolt = state.add_card(
            PlayerId(0),
            state.card_db().get(grp::LIGHTNING_BOLT).unwrap().chars.clone(),
            Zone::Graveyard,
        );
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);

        let actions = e.legal_actions(PlayerId(0));
        let act = actions.iter().find_map(|a| match a {
            PlayableAction::Activate { source, ability } if *source == pm => Some(*ability),
            _ => None,
        });
        assert!(act.is_some(), "graveyard reanimation ability offered (I/S fuel present + {{1}}{{B}})");
        e.activate_ability(PlayerId(0), pm, act.unwrap());
        e.resolve_top();
        assert!(e.state.player(PlayerId(0)).battlefield.contains(&pm), "Postmortem returned to the battlefield");
        assert!(e.state.player(PlayerId(0)).exile.contains(&bolt), "the instant was exiled as the cost");
    }

    /// Without an instant/sorcery in the graveyard the reanimation cost can't be paid → not offered.
    #[test]
    fn not_offered_without_instant_or_sorcery_fuel() {
        let mut state = build_game(1, &[&[], &[]]);
        for grp_id in [grp::SWAMP, grp::ISLAND] {
            let c = state.card_db().get(grp_id).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield);
        }
        let pm = state.add_card(
            PlayerId(0),
            state.card_db().get(POSTMORTEM_PROFESSOR).unwrap().chars.clone(),
            Zone::Graveyard,
        );
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
        let offered = e.legal_actions(PlayerId(0)).iter().any(|a| matches!(
            a, PlayableAction::Activate { source, .. } if *source == pm));
        assert!(!offered, "no I/S card to exile → reanimation not offered");
    }
}
