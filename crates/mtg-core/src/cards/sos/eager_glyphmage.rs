//! Eager Glyphmage — `{3}{W}` Creature — Cat Cleric 3/3 (first printed SOS).
//!
//! Oracle: "When this creature enters, create a 1/1 white and black Inkling creature token with
//! flying."
//!
//! **Fully implemented** — an ETB triggered `CreateToken` of the shared Inkling token (a 1/1 W/B
//! flyer). The token's Flying now lands (token keywords are applied on creation).

use crate::basics::Color;
use crate::cards::helpers::inkling_token;
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const EAGER_GLYPHMAGE: u32 = 218;

pub fn register(db: &mut CardDb) {
    db.insert(
        creature(
            EAGER_GLYPHMAGE,
            "Eager Glyphmage",
            &[CreatureType::Cat, CreatureType::Cleric],
            Color::White,
            mana_cost(3, &[(Color::White, 1)]),
            3,
            3,
            vec![Ability::Triggered {
                event: EventPattern::SelfEnters,
                condition: None,
                intervening_if: false,
                effect: Effect::CreateToken {
                    spec: inkling_token(),
                    count: ValueExpr::Fixed(1),
                    controller: PlayerRef::Controller,
                },
            }],
        )
        .with_text("When this creature enters, create a 1/1 white and black Inkling creature token with flying."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn eager_glyphmage_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(EAGER_GLYPHMAGE).unwrap();
        assert_eq!(def.chars.power, Some(3));
        assert!(def.fully_implemented);
        expect![[r#"
            [
                Triggered {
                    event: SelfEnters,
                    condition: None,
                    intervening_if: false,
                    effect: CreateToken {
                        spec: TokenSpec {
                            name: "Inkling",
                            card_types: [
                                Creature,
                            ],
                            subtypes: [
                                Creature(
                                    Inkling,
                                ),
                            ],
                            colors: [
                                White,
                                Black,
                            ],
                            power: 1,
                            toughness: 1,
                            keywords: [
                                Flying,
                            ],
                            counters: [],
                            grp_id: 0,
                        },
                        count: Fixed(
                            1,
                        ),
                        controller: Controller,
                    },
                },
            ]"#]].assert_eq(&format!("{:#?}", def.abilities));
    }

    /// Behaviour: resolving the ETB puts a 1/1 white-and-black Inkling with flying onto the battlefield.
    #[test]
    fn eager_glyphmage_makes_a_flying_inkling() {
        use crate::agent::RandomAgent;
        use crate::basics::Zone;
        use crate::cards::build_game;
        use crate::effects::ability::Keyword;
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        let mut state = build_game(1, &[&[], &[]]);
        let chars = state.card_db().get(EAGER_GLYPHMAGE).unwrap().chars.clone();
        let src = state.add_card(PlayerId(0), chars, Zone::Battlefield);
        let etb = match &state.card_db().get(EAGER_GLYPHMAGE).unwrap().abilities[0] {
            Ability::Triggered { effect, .. } => effect.clone(),
            o => panic!("expected ETB Triggered, got {o:?}"),
        };
        let mut e = Engine::new(
            state,
            vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))],
        );
        let before = e.state.players[0].battlefield.len();
        e.resolve_effect(
            &etb,
            &ResolutionCtx { controller: Some(PlayerId(0)), source: Some(src), ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        let tokens: Vec<_> = e.state.players[0]
            .battlefield
            .iter()
            .copied()
            .filter(|&o| o != src)
            .collect();
        assert_eq!(e.state.players[0].battlefield.len(), before + 1, "one token created");
        let tok = tokens[0];
        assert_eq!(e.state.computed(tok).power, Some(1));
        assert!(e.state.computed(tok).has_keyword(Keyword::Flying), "the Inkling has flying");
    }
}
