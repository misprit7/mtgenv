//! Mindful Biomancer — `{1}{G}` Creature — Dryad Druid 2/2 (first printed SOS).
//!
//! Oracle: "When this creature enters, you gain 1 life. / {2}{G}: This creature gets +2/+2 until
//! end of turn. Activate only once each turn."
//!
//! **Fully implemented** — an ETB `GainLife 1` plus a `{2}{G}` activated pump (+2/+2 until end of
//! turn) restricted to once each turn (`Restriction::OncePerTurn`, engine-enforced via the source's
//! `used_once_per_turn`).

use crate::basics::Color;
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, Cost, EventPattern, Restriction, Timing};
use crate::effects::condition::Duration;
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const MINDFUL_BIOMANCER: u32 = 215;

pub fn register(db: &mut CardDb) {
    db.insert(
        creature(
            MINDFUL_BIOMANCER,
            "Mindful Biomancer",
            &[CreatureType::Dryad, CreatureType::Druid],
            Color::Green,
            mana_cost(1, &[(Color::Green, 1)]),
            2,
            2,
            vec![
                Ability::Triggered {
                    event: EventPattern::SelfEnters,
                    condition: None,
                    intervening_if: false,
                    effect: Effect::GainLife {
                        who: PlayerRef::Controller,
                        amount: ValueExpr::Fixed(1),
                    },
                },
                Ability::Activated {
                    cost: Cost {
                        mana: Some(mana_cost(2, &[(Color::Green, 1)])),
                        components: vec![],
                    },
                    effect: Effect::PumpPT {
                        what: EffectTarget::SourceSelf,
                        power: ValueExpr::Fixed(2),
                        toughness: ValueExpr::Fixed(2),
                        duration: Duration::UntilEndOfTurn,
                    },
                    timing: Timing::Instant,
                    restriction: Some(Restriction::OncePerTurn),
                    is_mana: false,
                },
            ],
        )
        .with_text("When this creature enters, you gain 1 life.\n{2}{G}: This creature gets +2/+2 until end of turn. Activate only once each turn."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn mindful_biomancer_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(MINDFUL_BIOMANCER).unwrap();
        assert_eq!(def.chars.power, Some(2));
        assert!(def.fully_implemented);
        expect![[r#"
            [
                Triggered {
                    event: SelfEnters,
                    condition: None,
                    intervening_if: false,
                    effect: GainLife {
                        who: Controller,
                        amount: Fixed(
                            1,
                        ),
                    },
                },
                Activated {
                    cost: Cost {
                        mana: Some(
                            ManaCost {
                                generic: 2,
                                colored: {
                                    Green: 1,
                                },
                                x: 0,
                                hybrid: [],
                                mono_hybrid: [],
                            },
                        ),
                        components: [],
                    },
                    effect: PumpPT {
                        what: SourceSelf,
                        power: Fixed(
                            2,
                        ),
                        toughness: Fixed(
                            2,
                        ),
                        duration: UntilEndOfTurn,
                    },
                    timing: Instant,
                    restriction: Some(
                        OncePerTurn,
                    ),
                    is_mana: false,
                },
            ]"#]].assert_eq(&format!("{:#?}", def.abilities));
    }

    /// Behaviour: the ETB gains 1 life; the activated ability's effect pumps it +2/+2.
    #[test]
    fn mindful_biomancer_etb_and_pump() {
        use crate::agent::RandomAgent;
        use crate::basics::Zone;
        use crate::cards::build_game;
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        let mut state = build_game(1, &[&[], &[]]);
        let chars = state.card_db().get(MINDFUL_BIOMANCER).unwrap().chars.clone();
        let src = state.add_card(PlayerId(0), chars, Zone::Battlefield);
        let etb = match &state.card_db().get(MINDFUL_BIOMANCER).unwrap().abilities[0] {
            Ability::Triggered { effect, .. } => effect.clone(),
            o => panic!("expected ETB Triggered, got {o:?}"),
        };
        let pump = match &state.card_db().get(MINDFUL_BIOMANCER).unwrap().abilities[1] {
            Ability::Activated { effect, .. } => effect.clone(),
            o => panic!("expected Activated pump, got {o:?}"),
        };
        let mut e = Engine::new(
            state,
            vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))],
        );
        let ctx = ResolutionCtx { controller: Some(PlayerId(0)), source: Some(src), ..Default::default() };
        let p0 = e.state.player(PlayerId(0)).life;
        e.resolve_effect(&etb, &ctx, WbReason::Resolve(StackId(0)));
        assert_eq!(e.state.player(PlayerId(0)).life, p0 + 1, "ETB gains 1 life");
        assert_eq!(e.state.computed(src).power, Some(2));
        e.resolve_effect(&pump, &ctx, WbReason::Resolve(StackId(0)));
        assert_eq!(e.state.computed(src).power, Some(4), "+2 power from the pump");
        assert_eq!(e.state.computed(src).toughness, Some(4), "+2 toughness from the pump");
    }
}
