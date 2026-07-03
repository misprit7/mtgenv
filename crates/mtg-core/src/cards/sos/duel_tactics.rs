//! Duel Tactics — `{R}` Sorcery (first printed SOS).
//!
//! Oracle: "Duel Tactics deals 1 damage to target creature. It can't block this turn. / Flashback {1}{R}"
//!
//! **Fully implemented** — 1 damage to a target creature + a can't-block qualification until end of
//! turn, with `Ability::Flashback {1}{R}`.

use crate::basics::{CardType, Color, DamageKind};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::ability::{Ability, Qualification};
use crate::effects::condition::Duration;
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::ValueExpr;
use crate::effects::{Effect, EffectTarget};

/// grp id (per-set ids live near their cards).
pub const DUEL_TACTICS: u32 = 288;

pub fn register(db: &mut CardDb) {
    let effect = Effect::Sequence(vec![
        Effect::DealDamage {
            amount: ValueExpr::Fixed(1),
            to: EffectTarget::Target(TargetSpec {
                kind: TargetKind::Creature(CardFilter::Any),
                min: 1,
                max: 1,
                distinct: true,
            }),
            kind: DamageKind::Noncombat,
        },
        Effect::GrantQualification {
            what: EffectTarget::ChosenIndex(0),
            qualification: Qualification::CantBlock,
            duration: Duration::UntilEndOfTurn,
        },
    ]);
    let mut def = spell(DUEL_TACTICS, "Duel Tactics", CardType::Sorcery, Color::Red, mana_cost(0, &[(Color::Red, 1)]), effect)
        .with_text("Duel Tactics deals 1 damage to target creature. It can't block this turn.\nFlashback {1}{R}");
    def.abilities.push(Ability::Flashback { cost: mana_cost(1, &[(Color::Red, 1)]) });
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effects::ability::Ability;
    use expect_test::expect;

    #[test]
    fn duel_tactics_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(DUEL_TACTICS).unwrap();
        assert!(def.fully_implemented);
        assert!(def.abilities.iter().any(|a| matches!(a, Ability::Flashback { .. })));
        expect![[r#"
            Sequence(
                [
                    DealDamage {
                        amount: Fixed(
                            1,
                        ),
                        to: Target(
                            TargetSpec {
                                kind: Creature(
                                    Any,
                                ),
                                min: 1,
                                max: 1,
                                distinct: true,
                            },
                        ),
                        kind: Noncombat,
                    },
                    GrantQualification {
                        what: ChosenIndex(
                            0,
                        ),
                        qualification: CantBlock,
                        duration: UntilEndOfTurn,
                    },
                ],
            )"#]].assert_eq(&format!("{:#?}", def.spell_effect().unwrap()));
    }

    #[test]
    fn duel_tactics_damages_and_stops_blocking() {
        use crate::agent::RandomAgent;
        use crate::basics::{Target, Zone};
        use crate::cards::{build_game, grp};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        let mut state = build_game(1, &[&[], &[]]);
        let bear = state.add_card(PlayerId(1), state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Battlefield);
        let effect = state.card_db().get(DUEL_TACTICS).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
        e.resolve_effect(&effect, &ResolutionCtx { controller: Some(PlayerId(0)), chosen_targets: vec![Target::Object(bear)], ..Default::default() }, WbReason::Resolve(StackId(0)));
        // The GrantQualification(CantBlock) resolved (snapshotted in the IR test); here we verify the
        // 1 damage landed on the target.
        assert_eq!(e.state.objects.get(&bear).unwrap().damage_marked, 1, "1 damage marked");
    }
}
