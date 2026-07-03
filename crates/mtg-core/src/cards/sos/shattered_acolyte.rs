//! Shattered Acolyte — `{1}{W}` Creature — Dwarf Warlock 2/2 (first printed SOS).
//!
//! Oracle: "Lifelink / {1}, Sacrifice this creature: Destroy target artifact or enchantment."
//!
//! **Fully implemented** — printed Lifelink plus an activated ability whose cost is `{1}` +
//! sacrifice this creature (`CostComponent::Sacrifice`), destroying one target artifact or
//! enchantment.

use crate::basics::{CardType, Color};
use crate::cards::helpers::sacrifice_self;
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, Cost, CostComponent, Keyword, Timing};
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::{Effect, EffectTarget};
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const SHATTERED_ACOLYTE: u32 = 225;

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        SHATTERED_ACOLYTE,
        "Shattered Acolyte",
        &[CreatureType::Dwarf, CreatureType::Warlock],
        Color::White,
        mana_cost(1, &[(Color::White, 1)]),
        2,
        2,
        vec![Ability::Activated {
            cost: Cost {
                mana: Some(mana_cost(1, &[])),
                components: vec![CostComponent::Sacrifice(sacrifice_self())],
            },
            effect: Effect::Destroy {
                what: EffectTarget::Target(TargetSpec {
                    kind: TargetKind::Permanent(CardFilter::AnyOf(vec![
                        CardFilter::HasCardType(CardType::Artifact),
                        CardFilter::HasCardType(CardType::Enchantment),
                    ])),
                    min: 1,
                    max: 1,
                    distinct: true,
                }),
            },
            timing: Timing::Instant,
            restriction: None,
            is_mana: false,
        }],
    );
    def.chars.keywords = vec![Keyword::Lifelink];
    def.text = "Lifelink\n{1}, Sacrifice this creature: Destroy target artifact or enchantment.".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shattered_acolyte_shape() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(SHATTERED_ACOLYTE).unwrap();
        assert_eq!(def.chars.keywords, vec![Keyword::Lifelink]);
        assert!(def.fully_implemented);
        match &def.abilities[0] {
            Ability::Activated { cost, .. } => {
                assert!(matches!(cost.components[0], CostComponent::Sacrifice(_)), "sac-self cost");
            }
            o => panic!("expected Activated, got {o:?}"),
        }
    }

    /// Behaviour: the ability's effect destroys a target artifact.
    #[test]
    fn shattered_acolyte_destroys_an_artifact() {
        use crate::agent::RandomAgent;
        use crate::basics::{Target, Zone};
        use crate::cards::{build_game, grp};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        // Use Bonesplitter (an Equipment artifact) as the target.
        let mut state = build_game(1, &[&[], &[]]);
        let art = state.card_db().get(grp::BONESPLITTER).unwrap().chars.clone();
        let target = state.add_card(PlayerId(1), art, Zone::Battlefield);
        let effect = match &state.card_db().get(SHATTERED_ACOLYTE).unwrap().abilities[0] {
            Ability::Activated { effect, .. } => effect.clone(),
            o => panic!("expected Activated, got {o:?}"),
        };
        let mut e = Engine::new(
            state,
            vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))],
        );
        e.resolve_effect(
            &effect,
            &ResolutionCtx {
                controller: Some(PlayerId(0)),
                chosen_targets: vec![Target::Object(target)],
                ..Default::default()
            },
            WbReason::Resolve(StackId(0)),
        );
        assert!(e.state.players[1].graveyard.contains(&target), "the artifact was destroyed");
    }
}
