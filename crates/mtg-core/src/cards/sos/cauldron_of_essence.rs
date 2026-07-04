//! Cauldron of Essence — `{1}{B}{G}` Artifact (first printed SOS).
//!
//! Oracle: "Whenever a creature you control dies, each opponent loses 1 life and you gain 1 life.
//! {1}{B}{G}, {T}, Sacrifice a creature: Return target creature card from your graveyard to the
//! battlefield. Activate only as a sorcery."
//!
//! **Fully implemented** — a **last-known-information dies-trigger** (CR 603.10a):
//! `EventPattern::CreatureDies(ControlledBy(Controller))` reads the dead creature's LKI controller
//! (it's now in the graveyard, where it has none), drains each opponent 1 and gains you 1. Plus a
//! sacrifice-cost reanimation activated ability (`{1}{B}{G}, {T}, Sacrifice a creature` → return a
//! targeted creature card from your graveyard to the battlefield; sorcery-timed).

use crate::basics::{CardType, Color, Zone, ZoneDest, ZonePos};
use crate::cards::{artifact, mana_cost, CardDb};
use crate::effects::ability::{Ability, Cost, CostComponent, EventPattern, Timing};
use crate::effects::target::{CardFilter, SelectSpec, TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};

/// grp id (per-set ids live near their cards).
pub const CAULDRON_OF_ESSENCE: u32 = 357;

pub fn register(db: &mut CardDb) {
    let dies_drain = Ability::Triggered {
        event: EventPattern::CreatureDies(CardFilter::ControlledBy(PlayerRef::Controller)),
        condition: None,
        intervening_if: false,
        effect: Effect::Sequence(vec![
            Effect::LoseLife { who: PlayerRef::EachOpponent, amount: ValueExpr::Fixed(1) },
            Effect::GainLife { who: PlayerRef::Controller, amount: ValueExpr::Fixed(1) },
        ]),
    };
    // "{1}{B}{G}, {T}, Sacrifice a creature: Return target creature card from your graveyard to the
    // battlefield." Sorcery-timed reanimation.
    let reanimate = Ability::Activated {
        cost: Cost {
            mana: Some(mana_cost(1, &[(Color::Black, 1), (Color::Green, 1)])),
            components: vec![
                CostComponent::TapSelf,
                CostComponent::Sacrifice(SelectSpec {
                    zone: Zone::Battlefield,
                    filter: CardFilter::All(vec![
                        CardFilter::ControlledBy(PlayerRef::Controller),
                        CardFilter::HasCardType(CardType::Creature),
                    ]),
                    chooser: PlayerRef::Controller,
                    min: ValueExpr::Fixed(1),
                    max: ValueExpr::Fixed(1),
                }),
            ],
        },
        effect: Effect::MoveZone {
            what: EffectTarget::Target(TargetSpec {
                kind: TargetKind::CardInZone {
                    zone: Zone::Graveyard,
                    filter: CardFilter::All(vec![
                        CardFilter::ControlledBy(PlayerRef::Controller),
                        CardFilter::HasCardType(CardType::Creature),
                    ]),
                },
                min: 1,
                max: 1,
                distinct: true,
            }),
            to: ZoneDest { zone: Zone::Battlefield, pos: ZonePos::Any },
        },
        timing: Timing::Sorcery,
        restriction: None,
        is_mana: false,
    };
    db.insert(
        artifact(CAULDRON_OF_ESSENCE, "Cauldron of Essence", mana_cost(1, &[(Color::Black, 1), (Color::Green, 1)]), vec![dies_drain, reanimate])
            .with_text("Whenever a creature you control dies, each opponent loses 1 life and you gain 1 life.\n{1}{B}{G}, {T}, Sacrifice a creature: Return target creature card from your graveyard to the battlefield. Activate only as a sorcery."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::RandomAgent;
    use crate::cards::{build_game, grp};
    use crate::ids::PlayerId;
    use crate::priority::Engine;

    #[test]
    fn cauldron_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(CAULDRON_OF_ESSENCE).unwrap();
        assert!(def.fully_implemented);
        assert_eq!(def.chars.card_types, vec![CardType::Artifact]);
        assert!(matches!(
            &def.abilities[0],
            Ability::Triggered { event: EventPattern::CreatureDies(_), .. }
        ));
        assert!(matches!(&def.abilities[1], Ability::Activated { timing: Timing::Sorcery, .. }));
    }

    /// Real-path LKI: a creature you control dies (lethal damage → SBA). Cauldron's dies-trigger reads
    /// the dead creature's LKI controller (= you) and drains each opponent 1 / gains you 1.
    #[test]
    fn dies_trigger_drains_via_lki() {
        let mut state = build_game(1, &[&[], &[]]);
        let _cauldron = state.add_card(
            PlayerId(0),
            state.card_db().get(CAULDRON_OF_ESSENCE).unwrap().chars.clone(),
            Zone::Battlefield,
        );
        let victim = {
            let mut c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            c.toughness = Some(1);
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        state.active_player = PlayerId(0);
        // Mark 1 damage so the SBA (toughness 1) destroys it.
        state.objects.get_mut(&victim).unwrap().damage_marked = 1;
        let mut e = Engine::new(
            state,
            vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))],
        );
        let (opp_life, my_life) = (e.state.player(PlayerId(1)).life, e.state.player(PlayerId(0)).life);
        e.run_agenda(); // SBA kills the creature → dies-trigger queued + stacked (no target)
        e.resolve_top();
        assert_eq!(e.state.player(PlayerId(1)).life, opp_life - 1, "each opponent lost 1");
        assert_eq!(e.state.player(PlayerId(0)).life, my_life + 1, "you gained 1");
    }
}
