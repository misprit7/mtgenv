//! Molten Note — `{X}{R}{W}` Sorcery (first printed SOS).
//!
//! Oracle: "Molten Note deals damage to target creature equal to the amount of mana spent to cast
//! this spell. Untap all creatures you control. / Flashback {6}{R}{W}"
//!
//! **Fully implemented** — `ManaSpent` (incl. X) damage to a target creature, then untap every
//! creature you control (`ForEach` + `Tap{ tap: false }`), with `Ability::Flashback {6}{R}{W}`.
//! Multicolored (R/W).

use crate::basics::{CardType, Color, DamageKind, Zone};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::ability::Ability;
use crate::effects::target::{CardFilter, SelectSpec, TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};

/// grp id (per-set ids live near their cards).
pub const MOLTEN_NOTE: u32 = 299;

pub fn register(db: &mut CardDb) {
    let effect = Effect::Sequence(vec![
        Effect::DealDamage {
            amount: ValueExpr::ManaSpent,
            to: EffectTarget::Target(TargetSpec {
                kind: TargetKind::Creature(CardFilter::Any),
                min: 1,
                max: 1,
                distinct: true,
            }),
            kind: DamageKind::Noncombat,
        },
        Effect::ForEach {
            // "each creature you control" — all of them (max large so `select_for_each` takes them
            // all without asking; the helper's max=0 is for Static `affects`, not `ForEach`).
            selector: SelectSpec {
                zone: Zone::Battlefield,
                filter: CardFilter::All(vec![
                    CardFilter::HasCardType(CardType::Creature),
                    CardFilter::ControlledBy(PlayerRef::Controller),
                ]),
                chooser: PlayerRef::Controller,
                min: ValueExpr::Fixed(0),
                max: ValueExpr::Fixed(999),
            },
            body: Box::new(Effect::Tap { what: EffectTarget::Each, tap: false }),
        },
    ]);
    let mut cost = mana_cost(0, &[(Color::Red, 1), (Color::White, 1)]);
    cost.x = 1;
    let mut def = spell(MOLTEN_NOTE, "Molten Note", CardType::Sorcery, Color::Red, cost, effect)
        .with_text("Molten Note deals damage to target creature equal to the amount of mana spent to cast this spell. Untap all creatures you control.\nFlashback {6}{R}{W}");
    def.chars.colors = vec![Color::Red, Color::White];
    def.abilities.push(Ability::Flashback { cost: mana_cost(6, &[(Color::Red, 1), (Color::White, 1)]) });
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effects::ability::Ability;

    #[test]
    fn molten_note_shape() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(MOLTEN_NOTE).unwrap();
        assert_eq!(def.chars.colors, vec![Color::Red, Color::White]);
        assert_eq!(def.chars.mana_cost.as_ref().unwrap().x, 1, "an {{X}} spell");
        assert!(def.abilities.iter().any(|a| matches!(a, Ability::Flashback { .. })));
        assert!(def.fully_implemented);
    }

    #[test]
    fn molten_note_damages_by_mana_spent_and_untaps() {
        use crate::agent::RandomAgent;
        use crate::basics::{Target, Zone};
        use crate::cards::{build_game, grp};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        let mut state = build_game(1, &[&[], &[]]);
        // A tapped creature you control (to be untapped) + an enemy creature to damage.
        let mine = state.add_card(PlayerId(0), state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Battlefield);
        state.objects.get_mut(&mine).unwrap().status.tapped = true;
        let enemy = state.add_card(PlayerId(1), state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Battlefield);
        // The spell object with mana_spent = 3 (e.g. X=1 → {1}{R}{W}).
        let src = state.add_card(PlayerId(0), state.card_db().get(MOLTEN_NOTE).unwrap().chars.clone(), Zone::Stack);
        state.objects.get_mut(&src).unwrap().mana_spent = 3;
        let effect = state.card_db().get(MOLTEN_NOTE).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
        e.resolve_effect(&effect, &ResolutionCtx { controller: Some(PlayerId(0)), source: Some(src), chosen_targets: vec![Target::Object(enemy)], ..Default::default() }, WbReason::Resolve(StackId(0)));
        assert_eq!(e.state.objects.get(&enemy).unwrap().damage_marked, 3, "damage = mana spent (3)");
        assert!(!e.state.objects.get(&mine).unwrap().status.tapped, "your creature untapped");
    }
}
