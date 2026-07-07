//! Rubble Rouser — `{2}{R}` Creature — Dwarf Sorcerer 1/4 (first printed SOS).
//!
//! Oracle: "When this creature enters, you may discard a card. If you do, draw a card. /
//! {T}, Exile a card from your graveyard: Add {R}. When you do, this creature deals 1 damage to
//! each opponent."
//!
//! **Fully implemented.** The `{T},Exile-from-graveyard` ability is authored as an ordinary NON-mana
//! activated ability whose effect is `Sequence[AddMana{R}, DealDamage 1 to each opponent]`. Although
//! by CR 605.1a it would qualify as a mana ability, the engine never offers mana abilities to
//! agent/RL seats (only the UI manual path), and the reflexive "when you do" damage — the ability's
//! strategic point — is unreachable through the no-stack mana path. Modeling it as a stack ability
//! makes the ping agent-selectable and fires the damage; the `{R}` floats on resolution. (Minor,
//! flagged: it loses true-mana-ability timing — can't be tapped mid-cast — and is respondable.)
//! The ETB loot rides the existing `Optional{ IfYouDo{ Discard, Draw } }` machinery.

use crate::basics::{Color, DamageKind, Zone};
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, Cost, CostComponent, EventPattern, Timing};
use crate::effects::target::{CardFilter, ManaSpec, SelectSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const RUBBLE_ROUSER: u32 = 502;

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        RUBBLE_ROUSER,
        "Rubble Rouser",
        &[CreatureType::Dwarf, CreatureType::Sorcerer],
        Color::Red,
        mana_cost(2, &[(Color::Red, 1)]),
        1,
        4,
        vec![
            // "When this creature enters, you may discard a card. If you do, draw a card." (loot)
            Ability::Triggered {
                event: EventPattern::SelfEnters,
                condition: None,
                intervening_if: false,
                effect: Effect::Optional {
                    prompt: "Discard a card to draw a card?".to_string(),
                    body: Box::new(Effect::IfYouDo {
                        cost: Box::new(Effect::Discard {
                            who: PlayerRef::Controller,
                            count: ValueExpr::Fixed(1),
                        }),
                        reward: Box::new(Effect::Draw {
                            who: PlayerRef::Controller,
                            count: ValueExpr::Fixed(1),
                        }),
                    }),
                },
            },
            // "{T}, Exile a card from your graveyard: Add {R}. When you do, this creature deals 1 damage
            // to each opponent." A non-mana activated ability (see module doc): Sequence[AddMana, ping].
            Ability::Activated {
                cost: Cost {
                    mana: None,
                    components: vec![
                        CostComponent::TapSelf,
                        CostComponent::Exile(SelectSpec {
                            zone: Zone::Graveyard,
                            filter: CardFilter::Any,
                            chooser: PlayerRef::Controller,
                            min: ValueExpr::Fixed(1),
                            max: ValueExpr::Fixed(1),
                        }),
                    ],
                },
                effect: Effect::Sequence(vec![
                    Effect::AddMana {
                        who: PlayerRef::Controller,
                        mana: ManaSpec {
                            produces: vec![(Color::Red, ValueExpr::Fixed(1))],
                            any_color: None,
                            one_of: None,
                            restriction: None,
                        },
                    },
                    Effect::DealDamage {
                        amount: ValueExpr::Fixed(1),
                        to: EffectTarget::Player(PlayerRef::EachOpponent),
                        kind: DamageKind::Noncombat,
                    },
                ]),
                timing: Timing::Instant,
                restriction: None,
                is_mana: false,
            },
        ],
    );
    def.text = "When this creature enters, you may discard a card. If you do, draw a card.\n{T}, Exile a card from your graveyard: Add {R}. When you do, this creature deals 1 damage to each opponent.".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayableAction, PlayerView};
    use crate::basics::Phase;
    use crate::cards::{build_game, grp};
    use crate::ids::{ObjId, PlayerId};
    use crate::priority::Engine;

    #[test]
    fn rubble_rouser_shape() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(RUBBLE_ROUSER).unwrap();
        assert!(def.fully_implemented);
        assert_eq!((def.chars.power, def.chars.toughness), (Some(1), Some(4)));
        assert_eq!(def.chars.colors, vec![Color::Red]);
        // The activation is a NON-mana ability (so it's agent-selectable + fires the reflexive ping).
        assert!(matches!(
            &def.abilities[1],
            Ability::Activated { is_mana: false, .. }
        ));
    }

    /// Selects the first `min` candidates for any SelectCards request; passes otherwise.
    struct FirstAgent;
    impl Agent for FirstAgent {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::SelectCards { min, .. } => {
                    DecisionResponse::Indices((0..(*min).max(1)).collect())
                }
                _ => DecisionResponse::Pass,
            }
        }
    }

    fn find_activate(e: &Engine, source: ObjId) -> Option<crate::agent::AbilityRef> {
        e.legal_actions(PlayerId(0)).iter().find_map(|a| match a {
            PlayableAction::Activate { source: s, ability } if *s == source => Some(*ability),
            _ => None,
        })
    }

    /// Real-path: `{T}, Exile a card from your graveyard: Add {R}. When you do, deal 1 to each opp.`
    /// Activating exiles a graveyard card, taps Rubble, floats {R}, and pings the opponent for 1.
    #[test]
    fn activated_ability_pings_and_floats_red() {
        let mut state = build_game(1, &[&[], &[]]);
        let rubble = state.add_card(
            PlayerId(0),
            state.card_db().get(RUBBLE_ROUSER).unwrap().chars.clone(),
            Zone::Battlefield,
        );
        state.objects.get_mut(&rubble).unwrap().summoning_sick = false; // can use {T} (CR 302.6)
        // A card in your graveyard to exile for the cost.
        let fuel = state.add_card(PlayerId(0), state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Graveyard);
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let opp_life = state.player(PlayerId(1)).life;
        let mut e = Engine::new(state, vec![Box::new(FirstAgent), Box::new(FirstAgent)]);

        let ab = find_activate(&e, rubble).expect("the {T},Exile ability is offered");
        e.activate_ability(PlayerId(0), rubble, ab);
        e.resolve_top();
        e.run_agenda();

        assert!(e.state.object(rubble).status.tapped, "Rubble tapped for the cost");
        assert_eq!(e.state.object(fuel).zone, Zone::Exile, "a graveyard card was exiled as the cost");
        assert_eq!(e.state.player(PlayerId(1)).life, opp_life - 1, "each opponent took 1 damage");
        assert_eq!(
            e.state.player(PlayerId(0)).mana_pool.amounts.get(&Color::Red).copied().unwrap_or(0),
            1,
            "{{R}} floated on resolution"
        );
    }
}
