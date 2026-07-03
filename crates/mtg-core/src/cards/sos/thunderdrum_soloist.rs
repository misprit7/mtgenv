//! Thunderdrum Soloist — `{1}{R}` Creature — Dwarf Bard 1/3 (first printed SOS).
//!
//! Oracle: "Reach / Opus — Whenever you cast an instant or sorcery spell, this creature deals 1
//! damage to each opponent. If five or more mana was spent to cast that spell, this creature deals 3
//! damage to each opponent instead."
//!
//! **Fully implemented** — printed Reach + an Opus cast-trigger: a `Conditional` on
//! `ManaSpentOnTrigger ≥ 5` dealing 3 (else 1) damage to each opponent.

use crate::basics::{Color, DamageKind};
use crate::cards::helpers::instant_or_sorcery;
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern, Keyword};
use crate::effects::condition::Condition;
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const THUNDERDRUM_SOLOIST: u32 = 258;

fn burn_each_opponent(amount: i64) -> Effect {
    Effect::DealDamage {
        amount: ValueExpr::Fixed(amount),
        to: EffectTarget::Player(PlayerRef::EachOpponent),
        kind: DamageKind::Noncombat,
    }
}

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        THUNDERDRUM_SOLOIST,
        "Thunderdrum Soloist",
        &[CreatureType::Dwarf, CreatureType::Bard],
        Color::Red,
        mana_cost(1, &[(Color::Red, 1)]),
        1,
        3,
        vec![Ability::Triggered {
            event: EventPattern::SpellCast(instant_or_sorcery()),
            condition: None,
            intervening_if: false,
            effect: Effect::Conditional {
                cond: Condition::ValueAtLeast(ValueExpr::ManaSpentOnTrigger, ValueExpr::Fixed(5)),
                then: Box::new(burn_each_opponent(3)),
                otherwise: Some(Box::new(burn_each_opponent(1))),
            },
        }],
    );
    def.chars.keywords = vec![Keyword::Reach];
    def.text = "Reach\nOpus — Whenever you cast an instant or sorcery spell, this creature deals 1 damage to each opponent. If five or more mana was spent to cast that spell, this creature deals 3 damage to each opponent instead.".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn thunderdrum_soloist_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(THUNDERDRUM_SOLOIST).unwrap();
        assert_eq!(def.chars.keywords, vec![Keyword::Reach]);
        assert!(def.fully_implemented);
        expect![[r#"
            [
                Triggered {
                    event: SpellCast(
                        AnyOf(
                            [
                                HasCardType(
                                    Instant,
                                ),
                                HasCardType(
                                    Sorcery,
                                ),
                            ],
                        ),
                    ),
                    condition: None,
                    intervening_if: false,
                    effect: Conditional {
                        cond: ValueAtLeast(
                            ManaSpentOnTrigger,
                            Fixed(
                                5,
                            ),
                        ),
                        then: DealDamage {
                            amount: Fixed(
                                3,
                            ),
                            to: Player(
                                EachOpponent,
                            ),
                            kind: Noncombat,
                        },
                        otherwise: Some(
                            DealDamage {
                                amount: Fixed(
                                    1,
                                ),
                                to: Player(
                                    EachOpponent,
                                ),
                                kind: Noncombat,
                            },
                        ),
                    },
                },
            ]"#]].assert_eq(&format!("{:#?}", def.abilities));
    }

    /// Behaviour: a cheap Opus deals 1 to the opponent's face; a 5-mana one deals 3.
    #[test]
    fn thunderdrum_soloist_opus_scales() {
        use crate::agent::RandomAgent;
        use crate::basics::Zone;
        use crate::cards::{build_game, grp};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;

        let dmg = |mana_spent: u32| {
            let mut state = build_game(1, &[&[], &[]]);
            let src = {
                let c = state.card_db().get(THUNDERDRUM_SOLOIST).unwrap().chars.clone();
                state.add_card(PlayerId(0), c, Zone::Battlefield)
            };
            let spell = {
                let c = state.card_db().get(grp::LIGHTNING_BOLT).unwrap().chars.clone();
                state.add_card(PlayerId(0), c, Zone::Stack)
            };
            state.objects.get_mut(&spell).unwrap().mana_spent = mana_spent;
            let etb = match &state.card_db().get(THUNDERDRUM_SOLOIST).unwrap().abilities[0] {
                Ability::Triggered { effect, .. } => effect.clone(),
                o => panic!("expected Opus Triggered, got {o:?}"),
            };
            let mut e = Engine::new(
                state,
                vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))],
            );
            let before = e.state.player(PlayerId(1)).life;
            e.resolve_effect(
                &etb,
                &ResolutionCtx {
                    controller: Some(PlayerId(0)),
                    source: Some(src),
                    triggering_spell: Some(spell),
                    ..Default::default()
                },
                WbReason::Resolve(StackId(0)),
            );
            before - e.state.player(PlayerId(1)).life
        };
        assert_eq!(dmg(3), 1, "cheap → 1 damage to the opponent");
        assert_eq!(dmg(5), 3, "5+ mana → 3 damage to the opponent");
    }
}
