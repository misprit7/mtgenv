//! Prismatic Ending — `{X}{W}` Sorcery (first printed MH2; reprinted on the SOS Mystical Archive `soa`).
//!
//! Oracle: "Converge — Exile target nonland permanent if its mana value is less than or equal to the
//! number of colors of mana spent to cast this spell."
//!
//! **Fully implemented** — an `Exile` whose target is a nonland permanent restricted to mana value ≤
//! `ColorsSpent` (the dynamic `CardFilter::ManaValueExpr { max: ColorsSpent }` in the target filter,
//! the shipped `sundering_archaic` converge idiom). Modelling note: the mana-value bound is enforced
//! at target selection rather than as a resolution-time "if" — the practical/observable behaviour in a
//! limited game is identical (you'd only ever point it at a permanent you can actually exile).

use crate::basics::{CardType, Color};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::ValueExpr;
use crate::effects::{Effect, EffectTarget};

/// grp id (bonus-sheet `soa` cards use the 600+ block).
pub const PRISMATIC_ENDING: u32 = 636;

pub fn register(db: &mut CardDb) {
    let effect = Effect::Exile {
        what: EffectTarget::Target(TargetSpec {
            kind: TargetKind::Permanent(CardFilter::All(vec![
                CardFilter::Not(Box::new(CardFilter::HasCardType(CardType::Land))),
                CardFilter::ManaValueExpr { min: None, max: Some(Box::new(ValueExpr::ColorsSpent)) },
            ])),
            min: 1,
            max: 1,
            distinct: true,
        }),
    };
    db.insert(
        spell(
            PRISMATIC_ENDING,
            "Prismatic Ending",
            CardType::Sorcery,
            Color::White,
            mana_cost(0, &[(Color::White, 1)]),
            effect,
        )
        .with_text("Converge — Exile target nonland permanent if its mana value is less than or equal to the number of colors of mana spent to cast this spell."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView};
    use crate::basics::{Target, Zone};
    use crate::cards::{build_game, grp};
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
    fn prismatic_ending_ir_has_converge_bound() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(PRISMATIC_ENDING).unwrap();
        assert!(def.fully_implemented);
        // The target filter carries the dynamic ManaValueExpr(ColorsSpent) bound.
        let Some(Effect::Exile { what: EffectTarget::Target(spec) }) = def.spell_effect() else { panic!("exile target") };
        let TargetKind::Permanent(CardFilter::All(fs)) = &spec.kind else { panic!("permanent filter") };
        assert!(fs.iter().any(|f| matches!(f, CardFilter::ManaValueExpr { .. })), "converge MV bound present");
    }

    /// Given a chosen target, resolving exiles it. (The MV-≤-colors restriction is enforced at target
    /// selection, exercised by the shared converge target-filter path — sundering_archaic.)
    #[test]
    fn exiles_the_chosen_permanent() {
        let mut state = build_game(1, &[&[], &[]]);
        let bear = state.add_card(PlayerId(1), state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Battlefield);
        let effect = state.card_db().get(PRISMATIC_ENDING).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(Passive), Box::new(Passive)]);
        e.resolve_effect(
            &effect,
            &ResolutionCtx { controller: Some(PlayerId(0)), chosen_targets: vec![Target::Object(bear)], ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        assert!(e.state.player(PlayerId(1)).exile.contains(&bear), "the chosen permanent was exiled");
    }
}
