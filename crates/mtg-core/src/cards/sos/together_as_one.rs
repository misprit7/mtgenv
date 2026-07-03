//! Together as One — `{6}` Sorcery (first printed SOS).
//!
//! Oracle: "Converge — Target player draws X cards, Together as One deals X damage to any target, and
//! you gain X life, where X is the number of colors of mana spent to cast this spell."
//!
//! **Fully implemented** — a `TargetPlayer` (slot 0) + an "any target" (slot 1), with each effect's
//! count `ValueExpr::ColorsSpent` (Converge): the targeted player draws X, X damage to any target, you
//! gain X.

use crate::basics::{CardType, Color, DamageKind};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::target::{TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};

/// grp id (per-set ids live near their cards).
pub const TOGETHER_AS_ONE: u32 = 293;

pub fn register(db: &mut CardDb) {
    let effect = Effect::Sequence(vec![
        Effect::TargetPlayer,
        Effect::Draw { who: PlayerRef::ChosenTarget(0), count: ValueExpr::ColorsSpent },
        Effect::DealDamage {
            amount: ValueExpr::ColorsSpent,
            to: EffectTarget::Target(TargetSpec { kind: TargetKind::Any, min: 1, max: 1, distinct: true }),
            kind: DamageKind::Noncombat,
        },
        Effect::GainLife { who: PlayerRef::Controller, amount: ValueExpr::ColorsSpent },
    ]);
    let mut def = spell(TOGETHER_AS_ONE, "Together as One", CardType::Sorcery, Color::Colorless, mana_cost(6, &[]), effect)
        .with_text("Converge — Target player draws X cards, Together as One deals X damage to any target, and you gain X life, where X is the number of colors of mana spent to cast this spell.");
    def.chars.colors = vec![]; // colorless (CR 202.2 — a `{6}` spell has no colours)
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn together_as_one_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        assert!(db.get(TOGETHER_AS_ONE).unwrap().fully_implemented);
        expect![[r#"
            Sequence(
                [
                    TargetPlayer,
                    Draw {
                        who: ChosenTarget(
                            0,
                        ),
                        count: ColorsSpent,
                    },
                    DealDamage {
                        amount: ColorsSpent,
                        to: Target(
                            TargetSpec {
                                kind: Any,
                                min: 1,
                                max: 1,
                                distinct: true,
                            },
                        ),
                        kind: Noncombat,
                    },
                    GainLife {
                        who: Controller,
                        amount: ColorsSpent,
                    },
                ],
            )"#]].assert_eq(&format!("{:#?}", db.get(TOGETHER_AS_ONE).unwrap().spell_effect().unwrap()));
    }

    /// Behaviour with colors_spent = 3: the targeted player draws 3, 3 damage to the any-target
    /// player, and the caster gains 3.
    #[test]
    fn together_as_one_scales_with_colors_spent() {
        use crate::agent::RandomAgent;
        use crate::basics::{Target, Zone};
        use crate::cards::{build_game, grp};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        let lib: Vec<u32> = std::iter::repeat(grp::FOREST).take(5).collect();
        let mut state = build_game(1, &[&[], &lib]);
        let src = state.add_card(PlayerId(0), state.card_db().get(TOGETHER_AS_ONE).unwrap().chars.clone(), Zone::Stack);
        state.objects.get_mut(&src).unwrap().colors_spent = 3;
        let effect = state.card_db().get(TOGETHER_AS_ONE).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
        let (p0_life, p1_life, p1_hand) = (e.state.player(PlayerId(0)).life, e.state.player(PlayerId(1)).life, e.state.players[1].hand.len());
        e.resolve_effect(
            &effect,
            &ResolutionCtx {
                controller: Some(PlayerId(0)),
                source: Some(src),
                // slot 0 = target player (P1, draws), slot 1 = any target (P1, takes damage).
                chosen_targets: vec![Target::Player(PlayerId(1)), Target::Player(PlayerId(1))],
                ..Default::default()
            },
            WbReason::Resolve(StackId(0)),
        );
        assert_eq!(e.state.players[1].hand.len(), p1_hand + 3, "target player drew X=3");
        assert_eq!(e.state.player(PlayerId(1)).life, p1_life - 3, "3 damage to the any-target");
        assert_eq!(e.state.player(PlayerId(0)).life, p0_life + 3, "caster gained 3");
    }
}
