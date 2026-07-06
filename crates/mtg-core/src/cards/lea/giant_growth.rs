//! Giant Growth — `{G}` Instant (first printed LEA; reprinted on the SOS Mystical Archive `soa`).
//!
//! Oracle: "Target creature gets +3/+3 until end of turn."
//!
//! **Fully implemented** — a single-target `PumpPT` of `+3/+3` until end of turn.

use crate::basics::{CardType, Color};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::condition::Duration;
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::ValueExpr;
use crate::effects::{Effect, EffectTarget};

/// grp id (per-set ids live near their cards; bonus-sheet `soa` cards use the 600+ block).
pub const GIANT_GROWTH: u32 = 600;

pub fn register(db: &mut CardDb) {
    let effect = Effect::PumpPT {
        what: EffectTarget::Target(TargetSpec {
            kind: TargetKind::Creature(CardFilter::Any),
            min: 1,
            max: 1,
            distinct: true,
        }),
        power: ValueExpr::Fixed(3),
        toughness: ValueExpr::Fixed(3),
        duration: Duration::UntilEndOfTurn,
    };
    db.insert(
        spell(
            GIANT_GROWTH,
            "Giant Growth",
            CardType::Instant,
            Color::Green,
            mana_cost(0, &[(Color::Green, 1)]),
            effect,
        )
        .with_text("Target creature gets +3/+3 until end of turn."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn giant_growth_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(GIANT_GROWTH).unwrap();
        assert!(def.fully_implemented);
        assert_eq!(def.chars.card_types, vec![CardType::Instant]);
        expect![[r#"
            PumpPT {
                what: Target(
                    TargetSpec {
                        kind: Creature(
                            Any,
                        ),
                        min: 1,
                        max: 1,
                        distinct: true,
                    },
                ),
                power: Fixed(
                    3,
                ),
                toughness: Fixed(
                    3,
                ),
                duration: UntilEndOfTurn,
            }"#]]
        .assert_eq(&format!("{:#?}", def.spell_effect().unwrap()));
    }

    /// Behaviour: +3/+3 lands on a 2/2, making it 5/5 until end of turn.
    #[test]
    fn giant_growth_pumps() {
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
        let mut state = build_game(1, &[&[], &[]]);
        let bear = state.add_card(
            PlayerId(0),
            state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(),
            Zone::Battlefield,
        );
        let effect = state.card_db().get(GIANT_GROWTH).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(Passive), Box::new(Passive)]);
        e.resolve_effect(
            &effect,
            &ResolutionCtx {
                controller: Some(PlayerId(0)),
                chosen_targets: vec![Target::Object(bear)],
                ..Default::default()
            },
            WbReason::Resolve(StackId(0)),
        );
        let chars = e.state.computed(bear);
        assert_eq!((chars.power, chars.toughness), (Some(5), Some(5)));
    }
}
