//! Crackle with Power — `{X}{X}{X}{R}{R}` Sorcery (first printed STX; reprinted on the SOS Mystical
//! Archive `soa`).
//!
//! Oracle: "Crackle with Power deals five times X damage to each of up to X targets."
//!
//! **Fully implemented** — a `ForEachTarget` over "up to X" targets (the `TARGET_COUNT_X` sentinel,
//! resolved to the chosen X at cast), each taking `XTimes(5)` = five-times-X damage. The three `{X}`
//! pips (`mc.x = 3`) still resolve to one chosen X value (the target count and the 5× multiplier).

use crate::basics::{CardType, Color, DamageKind};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::target::{CardFilter, TargetKind, TargetSpec, TARGET_COUNT_X};
use crate::effects::value::ValueExpr;
use crate::effects::{Effect, EffectTarget};

/// grp id (bonus-sheet `soa` cards use the 600+ block).
pub const CRACKLE_WITH_POWER: u32 = 639;

pub fn register(db: &mut CardDb) {
    let effect = Effect::ForEachTarget {
        slot: TargetSpec {
            kind: TargetKind::Any,
            min: 0,
            max: TARGET_COUNT_X,
            distinct: true,
        },
        body: Box::new(Effect::DealDamage {
            amount: ValueExpr::XTimes(5),
            to: EffectTarget::Each,
            kind: DamageKind::Noncombat,
        }),
    };
    let mut mc = mana_cost(0, &[(Color::Red, 2)]);
    mc.x = 3; // {X}{X}{X}{R}{R} — three {X} pips, one chosen X value.
    db.insert(
        spell(
            CRACKLE_WITH_POWER,
            "Crackle with Power",
            CardType::Sorcery,
            Color::Red,
            mc,
            effect,
        )
        .with_text("Crackle with Power deals five times X damage to each of up to X targets."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView};
    use crate::basics::{Target, Zone};
    use crate::cards::build_game;
    use crate::effects::action::{ResolutionCtx, WbReason};
    use crate::ids::{PlayerId, StackId};
    use crate::priority::Engine;

    #[derive(Clone)]
    struct Passive;
    impl Agent for Passive {
        fn decide(&mut self, _v: &PlayerView, _r: &DecisionRequest) -> DecisionResponse {
            DecisionResponse::Pass
        }
    }

    #[test]
    fn crackle_ir_up_to_x() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(CRACKLE_WITH_POWER).unwrap();
        assert!(def.fully_implemented);
        let Some(Effect::ForEachTarget { slot, .. }) = def.spell_effect() else { panic!("foreach target") };
        assert_eq!(slot.max, TARGET_COUNT_X, "up-to-X target count");
    }

    /// X=2: each of two targets takes 5×2 = 10 damage.
    #[test]
    fn deals_5x_to_each_target() {
        let mut state = build_game(1, &[&[], &[]]);
        use crate::state::Characteristics;
        let big = |st: &mut crate::state::GameState, p: PlayerId| {
            st.add_card(p, Characteristics { name: "Wall".to_string(), card_types: vec![CardType::Creature], power: Some(0), toughness: Some(20), grp_id: 8050, ..Default::default() }, Zone::Battlefield)
        };
        let a = big(&mut state, PlayerId(1));
        let b = big(&mut state, PlayerId(1));
        let effect = state.card_db().get(CRACKLE_WITH_POWER).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(Passive), Box::new(Passive)]);
        e.resolve_effect(
            &effect,
            &ResolutionCtx { controller: Some(PlayerId(0)), chosen_targets: vec![Target::Object(a), Target::Object(b)], x: Some(2), ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        assert_eq!(e.state.object(a).damage_marked, 10, "5×X = 10 to the first target");
        assert_eq!(e.state.object(b).damage_marked, 10, "5×X = 10 to the second target");
    }
}
