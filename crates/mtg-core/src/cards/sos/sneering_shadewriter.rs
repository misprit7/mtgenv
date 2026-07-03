//! Sneering Shadewriter — `{4}{B}` Creature — Vampire Warlock 3/3 (first printed SOS).
//!
//! Oracle: "Flying / When this creature enters, each opponent loses 2 life and you gain 2 life."
//!
//! **Fully implemented** — printed Flying (CR 702.9) plus an ETB triggered ability (CR 603.6a):
//! `LoseLife 2` for each opponent + `GainLife 2` for the controller. (2-player: `EachOpponent`
//! resolves to the single opponent.)

use crate::basics::Color;
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern, Keyword};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const SNEERING_SHADEWRITER: u32 = 206;

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        SNEERING_SHADEWRITER,
        "Sneering Shadewriter",
        &[CreatureType::Vampire, CreatureType::Warlock],
        Color::Black,
        mana_cost(4, &[(Color::Black, 1)]),
        3,
        3,
        vec![Ability::Triggered {
            event: EventPattern::SelfEnters,
            condition: None,
            intervening_if: false,
            effect: Effect::Sequence(vec![
                Effect::LoseLife {
                    who: PlayerRef::EachOpponent,
                    amount: ValueExpr::Fixed(2),
                },
                Effect::GainLife {
                    who: PlayerRef::Controller,
                    amount: ValueExpr::Fixed(2),
                },
            ]),
        }],
    );
    def.chars.keywords = vec![Keyword::Flying];
    def.text = "Flying\nWhen this creature enters, each opponent loses 2 life and you gain 2 life.".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn sneering_shadewriter_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(SNEERING_SHADEWRITER).unwrap();
        assert_eq!(def.chars.power, Some(3));
        assert_eq!(def.chars.keywords, vec![Keyword::Flying]);
        assert!(def.fully_implemented);
        expect![[r#"
            [
                Triggered {
                    event: SelfEnters,
                    condition: None,
                    intervening_if: false,
                    effect: Sequence(
                        [
                            LoseLife {
                                who: EachOpponent,
                                amount: Fixed(
                                    2,
                                ),
                            },
                            GainLife {
                                who: Controller,
                                amount: Fixed(
                                    2,
                                ),
                            },
                        ],
                    ),
                },
            ]"#]].assert_eq(&format!("{:#?}", def.abilities));
    }

    /// Behaviour: resolving the ETB drains the opponent for 2 and gains the controller 2.
    #[test]
    fn sneering_shadewriter_etb_drains_two() {
        use crate::agent::RandomAgent;
        use crate::basics::Zone;
        use crate::cards::build_game;
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        let mut state = build_game(1, &[&[], &[]]);
        let chars = state.card_db().get(SNEERING_SHADEWRITER).unwrap().chars.clone();
        let src = state.add_card(PlayerId(0), chars, Zone::Battlefield);
        let etb = match &state.card_db().get(SNEERING_SHADEWRITER).unwrap().abilities[0] {
            Ability::Triggered { effect, .. } => effect.clone(),
            o => panic!("expected ETB Triggered, got {o:?}"),
        };
        let mut e = Engine::new(
            state,
            vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))],
        );
        let p0 = e.state.player(PlayerId(0)).life;
        let p1 = e.state.player(PlayerId(1)).life;
        e.resolve_effect(
            &etb,
            &ResolutionCtx { controller: Some(PlayerId(0)), source: Some(src), ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        assert_eq!(e.state.player(PlayerId(1)).life, p1 - 2, "opponent loses 2");
        assert_eq!(e.state.player(PlayerId(0)).life, p0 + 2, "you gain 2");
    }
}
