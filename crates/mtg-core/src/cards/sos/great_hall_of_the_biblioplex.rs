//! Great Hall of the Biblioplex — Land (first printed SOS).
//!
//! Oracle:
//!   {T}: Add {C}.
//!   {T}, Pay 1 life: Add one mana of any color. Spend this mana only to cast an instant or sorcery
//!     spell.
//!   {5}: If this land isn't a creature, it becomes a 2/4 Wizard creature with "Whenever you cast an
//!     instant or sorcery spell, this creature gets +1/+0 until end of turn." It's still a land.
//!
//! **Fully implemented** (the animation via subsystem-A layer-4 subtype/type changes):
//! - `{T}: Add {C}` — an intrinsic colorless mana ability.
//! - `{T}, Pay 1 life: Add any color (I/S-only)` — a **cost-bearing** mana ability (the pay-life). Per
//!   the engine's established convention it is usable via the manual mana path (which pays the life
//!   through `pay_cost`) and is auto-pay-inert for agent seats — the same "option-B" treatment as
//!   Treasures / Goblin Glasswright's Treasure, pending the transactional-cast payment work
//!   (WHITEBOARD_MODEL §2.6). Faithful as card data.
//! - `{5}` animation — an [`Effect::Becomes`] guarded by `Not(`[`Condition::SelfIsCreature`]`)` (so a
//!   second activation while already a creature does nothing, not re-granting the trigger): adds the
//!   Creature type + a Wizard subtype (layer 4), sets base P/T 2/4 (layer 7b), and grants the I/S-cast
//!   +1/+0 pump ([`grp::GRANT_ISCAST_PUMP`], layer 6) — permanently, staying a land.

use crate::basics::{CardType, Color};
use crate::cards::{grp, mana_ability, mana_cost, CardDb, CardDef};
use crate::effects::ability::{Ability, Cost, CostComponent, StaticContribution, Timing};
use crate::effects::condition::{Condition, Duration};
use crate::effects::target::{ManaSpec, SpendRestriction};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};
use crate::state::Characteristics;
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const GREAT_HALL_OF_THE_BIBLIOPLEX: u32 = 501;

pub fn register(db: &mut CardDb) {
    let chars = Characteristics {
        name: "Great Hall of the Biblioplex".to_string(),
        card_types: vec![CardType::Land],
        grp_id: GREAT_HALL_OF_THE_BIBLIOPLEX,
        ..Default::default()
    };
    db.insert(CardDef {
        chars,
        abilities: vec![
            // "{T}: Add {C}."
            mana_ability(Color::Colorless),
            // "{T}, Pay 1 life: Add one mana of any color. (I/S-only.)" — a cost-bearing mana ability.
            Ability::Activated {
                cost: Cost {
                    mana: None,
                    components: vec![CostComponent::TapSelf, CostComponent::PayLife(ValueExpr::Fixed(1))],
                },
                effect: Effect::AddMana {
                    who: PlayerRef::Controller,
                    mana: ManaSpec {
                        produces: vec![],
                        any_color: Some(ValueExpr::Fixed(1)),
                        restriction: Some(SpendRestriction::InstantSorceryOnly),
                    },
                },
                timing: Timing::Instant,
                restriction: None,
                is_mana: true,
            },
            // "{5}: If this land isn't a creature, it becomes a 2/4 Wizard creature with [pump]. Still a land."
            Ability::Activated {
                cost: Cost { mana: Some(mana_cost(5, &[])), components: vec![] },
                effect: Effect::Conditional {
                    cond: Condition::Not(Box::new(Condition::SelfIsCreature)),
                    then: Box::new(Effect::Becomes {
                        what: EffectTarget::SourceSelf,
                        contributions: vec![
                            StaticContribution::AddType(CardType::Creature),
                            StaticContribution::AddSubtype(CreatureType::Wizard.into()),
                            StaticContribution::GrantAbility { template_grp: grp::GRANT_ISCAST_PUMP },
                        ],
                        base_pt: Some((ValueExpr::Fixed(2), ValueExpr::Fixed(4))),
                        duration: Duration::Permanent,
                    }),
                    otherwise: None,
                },
                timing: Timing::Instant,
                restriction: None,
                is_mana: false,
            },
        ],
        text: "{T}: Add {C}.\n{T}, Pay 1 life: Add one mana of any color. Spend this mana only to cast an instant or sorcery spell.\n{5}: If this land isn't a creature, it becomes a 2/4 Wizard creature with \"Whenever you cast an instant or sorcery spell, this creature gets +1/+0 until end of turn.\" It's still a land.".to_string(),
        fully_implemented: true,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, CastVariant, DecisionRequest, DecisionResponse, PlayableAction, PlayerView};
    use crate::basics::{Phase, Zone};
    use crate::cards::{build_game, grp, starter_db};
    use crate::ids::{ObjId, PlayerId};
    use crate::priority::Engine;
    use crate::state::GameState;
    use crate::subtypes::Subtype;
    use std::sync::Arc;

    #[test]
    fn great_hall_shape() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(GREAT_HALL_OF_THE_BIBLIOPLEX).unwrap();
        assert!(def.fully_implemented);
        assert_eq!(def.chars.card_types, vec![CardType::Land]);
        // Ability 1 = the cost-bearing pay-life mana ability (I/S-only any-color).
        assert!(matches!(&def.abilities[1],
            Ability::Activated { effect: Effect::AddMana { mana, .. }, is_mana: true, .. }
            if mana.restriction == Some(SpendRestriction::InstantSorceryOnly)));
    }

    fn find_activate(e: &Engine, source: ObjId, want: u32) -> Option<crate::agent::AbilityRef> {
        e.legal_actions(PlayerId(0)).iter().find_map(|a| match a {
            PlayableAction::Activate { source: s, ability } if *s == source && ability.0 == want => Some(*ability),
            _ => None,
        })
    }

    #[derive(Clone)]
    struct PassiveAgent;
    impl Agent for PassiveAgent {
        fn decide(&mut self, _v: &PlayerView, _req: &DecisionRequest) -> DecisionResponse {
            DecisionResponse::Pass
        }
    }

    /// Real-path `{5}` animation: Great Hall becomes a 2/4 Wizard creature that is STILL a land; a second
    /// activation while it's already a creature does nothing (the `Not(SelfIsCreature)` guard).
    #[test]
    fn five_animates_into_a_2_4_wizard_still_a_land() {
        let mut state = build_game(1, &[&[], &[]]);
        let hall = state.add_card(
            PlayerId(0),
            state.card_db().get(GREAT_HALL_OF_THE_BIBLIOPLEX).unwrap().chars.clone(),
            Zone::Battlefield,
        );
        // Five Forests to pay the {5}.
        for _ in 0..5 {
            let f = state.card_db().get(grp::FOREST).unwrap().chars.clone();
            state.add_card(PlayerId(0), f, Zone::Battlefield);
        }
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let mut e = Engine::new(state, vec![Box::new(PassiveAgent), Box::new(PassiveAgent)]);

        let five = find_activate(&e, hall, 2).expect("the {5} ability is offered"); // index 2
        e.activate_ability(PlayerId(0), hall, five);
        e.resolve_top();
        e.run_agenda();

        let cc = e.state.computed(hall);
        assert!(cc.card_types.contains(&CardType::Creature), "became a creature");
        assert!(cc.card_types.contains(&CardType::Land), "and is STILL a land");
        assert!(cc.subtypes.contains(&Subtype::Creature(CreatureType::Wizard)), "a Wizard");
        assert_eq!((cc.power, cc.toughness), (Some(2), Some(4)), "2/4");
        let ce_after_first = e.state.continuous_effects.len();

        // A second {5} while already a creature does nothing (guard): no new continuous effect.
        // (It's now a creature so it's tapped-for-mana capable etc., but re-animating must be inert.)
        e.state.objects.get_mut(&hall).unwrap().summoning_sick = false;
        for _ in 0..5 {
            let f = e.state.card_db().get(grp::FOREST).unwrap().chars.clone();
            e.state.add_card(PlayerId(0), f, Zone::Battlefield);
        }
        if let Some(five2) = find_activate(&e, hall, 2) {
            e.activate_ability(PlayerId(0), hall, five2);
            e.resolve_top();
            e.run_agenda();
        }
        assert_eq!(
            e.state.continuous_effects.len(),
            ce_after_first,
            "re-activating {{5}} while already a creature grants no second animation (guard)"
        );
    }

    /// The granted trigger fires: after animating, casting an instant/sorcery pumps Great Hall +1/+0
    /// until end of turn (exercises the granted-cast-trigger scan). Cast a Lightning Bolt from hand.
    #[test]
    fn granted_trigger_pumps_on_instant_cast() {
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(starter_db()));
        let hall = {
            let c = state.card_db().get(GREAT_HALL_OF_THE_BIBLIOPLEX).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        // Animate directly by granting the same continuous effect the {5} would (isolates the trigger).
        state.add_continuous_effect(
            Some(hall),
            PlayerId(0),
            vec![hall],
            vec![
                StaticContribution::AddType(CardType::Creature),
                StaticContribution::AddSubtype(CreatureType::Wizard.into()),
                StaticContribution::GrantAbility { template_grp: grp::GRANT_ISCAST_PUMP },
                StaticContribution::SetBasePT { power: 2, toughness: 4 },
            ],
            Duration::Permanent,
        );
        // A Lightning Bolt in hand + a Mountain to cast it (the I/S that triggers the pump).
        let bolt = {
            let c = state.card_db().get(grp::LIGHTNING_BOLT).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Hand)
        };
        {
            let m = state.card_db().get(grp::MOUNTAIN).unwrap().chars.clone();
            state.add_card(PlayerId(0), m, Zone::Battlefield);
        }
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let mut e = Engine::new(state, vec![Box::new(PassiveAgent), Box::new(PassiveAgent)]);
        assert_eq!(e.state.computed(hall).power, Some(2), "2/4 before the cast");

        e.cast_spell(PlayerId(0), bolt, CastVariant::Normal); // targets opponent by default via PassiveAgent? bolt needs a target
        e.run_agenda();
        while !e.state.stack.items.is_empty() {
            e.resolve_top();
            e.run_agenda();
        }
        assert_eq!(
            e.state.computed(hall).power,
            Some(3),
            "the granted 'whenever you cast an I/S' trigger pumped Great Hall +1/+0"
        );
    }
}
