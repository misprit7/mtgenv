//! Tome Blast — `{1}{R}` Sorcery (first printed SOS).
//!
//! Oracle: "Tome Blast deals 2 damage to any target. / Flashback {4}{R}"
//!
//! **Fully implemented** — 2 damage to any target, with `Ability::Flashback {4}{R}` (recast from the
//! graveyard, then exiled).

use crate::basics::{CardType, Color, DamageKind};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::ability::Ability;
use crate::effects::target::{TargetKind, TargetSpec};
use crate::effects::value::ValueExpr;
use crate::effects::{Effect, EffectTarget};

/// grp id (per-set ids live near their cards).
pub const TOME_BLAST: u32 = 287;

pub fn register(db: &mut CardDb) {
    let effect = Effect::DealDamage {
        amount: ValueExpr::Fixed(2),
        to: EffectTarget::Target(TargetSpec {
            kind: TargetKind::Any,
            min: 1,
            max: 1,
            distinct: true,
        }),
        kind: DamageKind::Noncombat,
    };
    let mut def = spell(TOME_BLAST, "Tome Blast", CardType::Sorcery, Color::Red, mana_cost(1, &[(Color::Red, 1)]), effect)
        .with_text("Tome Blast deals 2 damage to any target.\nFlashback {4}{R}");
    def.abilities.push(Ability::Flashback { cost: mana_cost(4, &[(Color::Red, 1)]) });
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effects::ability::Ability;

    #[test]
    fn tome_blast_shape() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(TOME_BLAST).unwrap();
        assert!(def.fully_implemented);
        assert!(def.abilities.iter().any(|a| matches!(a, Ability::Flashback { .. })), "declares Flashback");
    }

    #[test]
    fn tome_blast_deals_2_to_a_player() {
        use crate::agent::RandomAgent;
        use crate::basics::Target;
        use crate::cards::build_game;
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        let state = build_game(1, &[&[], &[]]);
        let effect = state.card_db().get(TOME_BLAST).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
        let before = e.state.player(PlayerId(1)).life;
        e.resolve_effect(&effect, &ResolutionCtx { controller: Some(PlayerId(0)), chosen_targets: vec![Target::Player(PlayerId(1))], ..Default::default() }, WbReason::Resolve(StackId(0)));
        assert_eq!(e.state.player(PlayerId(1)).life, before - 2);
    }
}
