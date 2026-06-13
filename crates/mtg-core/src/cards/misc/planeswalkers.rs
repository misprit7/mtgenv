//! Planeswalkers (#14 ckpt 4) — loyalty abilities (sorcery-speed, once-per-turn, ±loyalty cost).

use crate::basics::{CardType, Color, DamageKind};
use crate::cards::{grp, mana_cost, CardDb, CardDef};
use crate::effects::ability::{Ability, Cost, CostComponent, Restriction, Timing};
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};
use crate::state::Characteristics;

pub fn register(db: &mut CardDb) {
    // Chandra, Pyrogenius {4}{R}{R} Planeswalker — loyalty 5. Two loyalty abilities (sorcery-
    // speed, once per turn): +2 deals 2 to each opponent; −3 deals 4 to target creature. The
    // −10 ultimate (multi-target sweep) is deferred.
    db.insert(CardDef {
        chars: Characteristics {
            name: "Chandra, Pyrogenius".to_string(),
            card_types: vec![CardType::Planeswalker],
            supertypes: vec!["Legendary".to_string()],
            subtypes: vec!["Chandra".to_string()],
            colors: vec![Color::Red],
            mana_cost: Some(mana_cost(4, &[(Color::Red, 2)])),
            loyalty: Some(5),
            grp_id: grp::CHANDRA_PYROGENIUS,
            ..Default::default()
        },
        abilities: vec![
            Ability::Activated {
                cost: Cost { mana: None, components: vec![CostComponent::Loyalty(2)] },
                effect: Effect::DealDamage {
                    amount: ValueExpr::Fixed(2),
                    to: EffectTarget::Player(PlayerRef::EachOpponent),
                    kind: DamageKind::Noncombat,
                },
                timing: Timing::Sorcery,
                restriction: Some(Restriction::OncePerTurn),
                is_mana: false,
            },
            Ability::Activated {
                cost: Cost { mana: None, components: vec![CostComponent::Loyalty(-3)] },
                effect: Effect::DealDamage {
                    amount: ValueExpr::Fixed(4),
                    to: EffectTarget::Target(TargetSpec {
                        kind: TargetKind::Creature(CardFilter::Any),
                        min: 1,
                        max: 1,
                        distinct: true,
                    }),
                    kind: DamageKind::Noncombat,
                },
                timing: Timing::Sorcery,
                restriction: Some(Restriction::OncePerTurn),
                is_mana: false,
            },
        ],
        mana_colors: Vec::new(),
        text: String::new(),
    }.with_text("+2: Chandra deals 2 damage to each opponent. −3: Chandra deals 4 damage to target creature. (−10 ultimate not yet modeled.)"));
}
