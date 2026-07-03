//! Fractal Mascot — `{4}{G}{U}` Creature — Fractal Elk 6/6 (first printed SOS).
//!
//! Oracle: "Trample / When this creature enters, tap target creature an opponent controls. Put a stun
//! counter on it."
//!
//! **Fully implemented** — printed Trample + an ETB that taps a target opponent creature and stuns it.
//! Multicolored (G/U).

use crate::basics::{Color, CounterKind};
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern, Keyword};
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const FRACTAL_MASCOT: u32 = 284;

/// "Tap target creature an opponent controls. Put a stun counter on it." (ETB body).
fn tap_and_stun_opponent_creature() -> Effect {
    Effect::Sequence(vec![
        Effect::Tap {
            what: EffectTarget::Target(TargetSpec {
                kind: TargetKind::Creature(CardFilter::ControlledBy(PlayerRef::Opponent)),
                min: 1,
                max: 1,
                distinct: true,
            }),
            tap: true,
        },
        Effect::PutCounters {
            what: EffectTarget::ChosenIndex(0),
            kind: CounterKind::Stun,
            n: ValueExpr::Fixed(1),
        },
    ])
}

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        FRACTAL_MASCOT,
        "Fractal Mascot",
        &[CreatureType::Fractal, CreatureType::Elk],
        Color::Green,
        mana_cost(4, &[(Color::Green, 1), (Color::Blue, 1)]),
        6,
        6,
        vec![Ability::Triggered {
            event: EventPattern::SelfEnters,
            condition: None,
            intervening_if: false,
            effect: tap_and_stun_opponent_creature(),
        }],
    );
    def.chars.colors = vec![Color::Green, Color::Blue];
    def.chars.keywords = vec![Keyword::Trample];
    def.text = "Trample\nWhen this creature enters, tap target creature an opponent controls. Put a stun counter on it.".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn fractal_mascot_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(FRACTAL_MASCOT).unwrap();
        assert_eq!(def.chars.keywords, vec![Keyword::Trample]);
        assert_eq!(def.chars.colors, vec![Color::Green, Color::Blue]);
        assert!(def.fully_implemented);
        expect![[r#"
            [
                Triggered {
                    event: SelfEnters,
                    condition: None,
                    intervening_if: false,
                    effect: Sequence(
                        [
                            Tap {
                                what: Target(
                                    TargetSpec {
                                        kind: Creature(
                                            ControlledBy(
                                                Opponent,
                                            ),
                                        ),
                                        min: 1,
                                        max: 1,
                                        distinct: true,
                                    },
                                ),
                                tap: true,
                            },
                            PutCounters {
                                what: ChosenIndex(
                                    0,
                                ),
                                kind: Stun,
                                n: Fixed(
                                    1,
                                ),
                            },
                        ],
                    ),
                },
            ]"#]].assert_eq(&format!("{:#?}", def.abilities));
    }

    #[test]
    fn fractal_mascot_etb_taps_and_stuns() {
        use crate::agent::RandomAgent;
        use crate::basics::{Target, Zone};
        use crate::cards::{build_game, grp};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        let mut state = build_game(1, &[&[], &[]]);
        let bear = state.add_card(PlayerId(1), state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Battlefield);
        let eff = match &state.card_db().get(FRACTAL_MASCOT).unwrap().abilities[0] {
            Ability::Triggered { effect, .. } => effect.clone(), o => panic!("{o:?}") };
        let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
        e.resolve_effect(&eff, &ResolutionCtx { controller: Some(PlayerId(0)), chosen_targets: vec![Target::Object(bear)], ..Default::default() }, WbReason::Resolve(StackId(0)));
        assert!(e.state.objects.get(&bear).unwrap().status.tapped);
        assert_eq!(e.state.objects.get(&bear).unwrap().counters.get(&CounterKind::Stun), 1);
    }
}
