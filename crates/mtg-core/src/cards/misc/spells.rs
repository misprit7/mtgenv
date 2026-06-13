//! Instant / sorcery spells from the starter + #14 pool: burn, draw, gain-life, removal.

use crate::basics::{CardType, Color};
use crate::cards::{deal_to_any, grp, mana_cost, spell, CardDb};
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};

pub fn register(db: &mut CardDb) {
    db.insert(
        spell(
            grp::SHOCK,
            "Shock",
            CardType::Instant,
            Color::Red,
            mana_cost(0, &[(Color::Red, 1)]),
            deal_to_any(2),
        )
        .with_text("Shock deals 2 damage to any target."),
    );
    db.insert(
        spell(
            grp::LIGHTNING_BOLT,
            "Lightning Bolt",
            CardType::Instant,
            Color::Red,
            mana_cost(0, &[(Color::Red, 1)]),
            deal_to_any(3),
        )
        .with_text("Lightning Bolt deals 3 damage to any target."),
    );
    db.insert(
        spell(
            grp::DIVINATION,
            "Divination",
            CardType::Sorcery,
            Color::Blue,
            mana_cost(2, &[(Color::Blue, 1)]),
            Effect::Draw {
                who: PlayerRef::Controller,
                count: ValueExpr::Fixed(2),
            },
        )
        .with_text("Draw two cards."),
    );
    // Murder {1}{B}{B} — "Destroy target creature." (Effect::Destroy.)
    db.insert(
        spell(
            grp::MURDER,
            "Murder",
            CardType::Instant,
            Color::Black,
            mana_cost(1, &[(Color::Black, 2)]),
            Effect::Destroy {
                what: EffectTarget::Target(TargetSpec {
                    kind: TargetKind::Creature(CardFilter::Any),
                    min: 1,
                    max: 1,
                    distinct: true,
                }),
            },
        )
        .with_text("Destroy target creature."),
    );
}
