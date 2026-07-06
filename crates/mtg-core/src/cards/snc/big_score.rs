//! Big Score — `{3}{R}` Instant (first printed SNC; reprinted on the SOS Mystical Archive `soa`).
//!
//! Oracle: "As an additional cost to cast this spell, discard a card. Draw two cards and create two
//! Treasure tokens."
//!
//! **Fully implemented** — the spell-level additional "discard a card" cost (`Ability::AdditionalCost`,
//! paid at cast) + a spell effect of `Draw 2` and two Treasure tokens (the shared `helpers::treasure_token`).
//! Same shape as the shipped `seize_the_spoils`, with two Treasures instead of one.

use crate::basics::{CardType, Color, Zone};
use crate::cards::{helpers, mana_cost, spell, CardDb};
use crate::effects::ability::{AdditionalCost, Ability, Cost, CostComponent};
use crate::effects::target::{CardFilter, SelectSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;

/// grp id (bonus-sheet `soa` cards use the 600+ block).
pub const BIG_SCORE: u32 = 613;

pub fn register(db: &mut CardDb) {
    let effect = Effect::Sequence(vec![
        Effect::Draw { who: PlayerRef::Controller, count: ValueExpr::Fixed(2) },
        Effect::CreateToken {
            spec: helpers::treasure_token(),
            count: ValueExpr::Fixed(2),
            controller: PlayerRef::Controller,
            dynamic_counters: vec![],
        },
    ]);
    let mut def = spell(
        BIG_SCORE,
        "Big Score",
        CardType::Instant,
        Color::Red,
        mana_cost(3, &[(Color::Red, 1)]),
        effect,
    )
    .with_text("As an additional cost to cast this spell, discard a card.\nDraw two cards and create two Treasure tokens. (They're artifacts with \"{T}, Sacrifice this token: Add one mana of any color.\")");
    def.abilities.push(Ability::AdditionalCost(AdditionalCost {
        options: vec![Cost {
            mana: None,
            components: vec![CostComponent::Discard(SelectSpec {
                zone: Zone::Hand,
                filter: CardFilter::Any,
                chooser: PlayerRef::Controller,
                min: ValueExpr::Fixed(1),
                max: ValueExpr::Fixed(1),
            })],
        }],
    }));
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::grp;

    #[test]
    fn big_score_shape() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(BIG_SCORE).unwrap();
        assert!(def.fully_implemented);
        let ac = def.additional_costs();
        assert_eq!(ac.len(), 1, "one additional-cost clause (discard a card)");
        assert!(matches!(ac[0].options[0].components[0], CostComponent::Discard(_)));
        // Two Treasures.
        let Some(Effect::Sequence(steps)) = def.spell_effect() else { panic!("sequence") };
        match &steps[1] {
            Effect::CreateToken { spec, count, .. } => {
                assert_eq!(spec.grp_id, grp::TREASURE_TOKEN);
                assert_eq!(*count, ValueExpr::Fixed(2));
            }
            o => panic!("expected CreateToken, got {o:?}"),
        }
    }
}
