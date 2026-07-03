//! Impractical Joke — `{R}` Sorcery (first printed SOS).
//!
//! Oracle: "Damage can't be prevented this turn. Impractical Joke deals 3 damage to up to one target
//! creature or planeswalker."
//!
//! **Fully implemented (in the current pool)** — 3 damage to up to one target creature. The two
//! riders are inert here: there are **no damage-prevention effects** in the pool (so "damage can't be
//! prevented this turn" is a no-op, the sanctioned inert-rider precedent — cf. Surrak's can't-be-
//! countered), and there are **no planeswalkers** (so "creature or planeswalker" is fully covered by
//! the creature target). Revisit if prevention / planeswalkers land.

use crate::basics::{CardType, Color, DamageKind};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::ValueExpr;
use crate::effects::{Effect, EffectTarget};

/// grp id (per-set ids live near their cards).
pub const IMPRACTICAL_JOKE: u32 = 300;

pub fn register(db: &mut CardDb) {
    let effect = Effect::DealDamage {
        amount: ValueExpr::Fixed(3),
        to: EffectTarget::Target(TargetSpec {
            kind: TargetKind::Creature(CardFilter::Any),
            min: 0,
            max: 1,
            distinct: true,
        }),
        kind: DamageKind::Noncombat,
    };
    db.insert(
        spell(IMPRACTICAL_JOKE, "Impractical Joke", CardType::Sorcery, Color::Red, mana_cost(0, &[(Color::Red, 1)]), effect)
            .with_text("Damage can't be prevented this turn. Impractical Joke deals 3 damage to up to one target creature or planeswalker."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn impractical_joke_deals_3() {
        use crate::agent::RandomAgent;
        use crate::basics::{Target, Zone};
        use crate::cards::{build_game, grp};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        let mut state = build_game(1, &[&[], &[]]);
        let bear = state.add_card(PlayerId(1), state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Battlefield);
        let effect = state.card_db().get(IMPRACTICAL_JOKE).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
        e.resolve_effect(&effect, &ResolutionCtx { controller: Some(PlayerId(0)), chosen_targets: vec![Target::Object(bear)], ..Default::default() }, WbReason::Resolve(StackId(0)));
        assert_eq!(e.state.objects.get(&bear).unwrap().damage_marked, 3);
    }
}
