//! Informed Inkwright — `{1}{W}` Creature — Human Wizard 2/2 (first printed SOS).
//!
//! Oracle: "Vigilance / Repartee — Whenever you cast an instant or sorcery spell that targets a
//! creature, create a 1/1 white and black Inkling creature token with flying."
//!
//! **Fully implemented** — printed Vigilance + a Repartee cast-trigger creating the shared Inkling token.

use crate::basics::Color;
use crate::cards::helpers::{inkling_token, instant_or_sorcery};
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern, Keyword};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const INFORMED_INKWRIGHT: u32 = 262;

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        INFORMED_INKWRIGHT,
        "Informed Inkwright",
        &[CreatureType::Human, CreatureType::Wizard],
        Color::White,
        mana_cost(1, &[(Color::White, 1)]),
        2,
        2,
        vec![Ability::Triggered {
            event: EventPattern::SpellCastTargetingCreature(instant_or_sorcery()),
            condition: None,
            intervening_if: false,
            effect: Effect::CreateToken {
                spec: inkling_token(),
                count: ValueExpr::Fixed(1),
                controller: PlayerRef::Controller,
                dynamic_counters: Vec::new(),
            },
        }],
    );
    def.chars.keywords = vec![Keyword::Vigilance];
    def.text = "Vigilance\nRepartee — Whenever you cast an instant or sorcery spell that targets a creature, create a 1/1 white and black Inkling creature token with flying.".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn informed_inkwright_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        assert!(db.get(INFORMED_INKWRIGHT).unwrap().fully_implemented);
        expect![[r#"
            [
                Triggered {
                    event: SpellCastTargetingCreature(
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
                        dynamic_counters: [],
                    },
                },
            ]"#]].assert_eq(&format!("{:#?}", db.get(INFORMED_INKWRIGHT).unwrap().abilities));
    }

    #[test]
    fn informed_inkwright_repartee_makes_inkling() {
        use crate::agent::RandomAgent;
        use crate::basics::Zone;
        use crate::cards::build_game;
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        let mut state = build_game(1, &[&[], &[]]);
        let src = state.add_card(PlayerId(0), state.card_db().get(INFORMED_INKWRIGHT).unwrap().chars.clone(), Zone::Battlefield);
        let eff = match &state.card_db().get(INFORMED_INKWRIGHT).unwrap().abilities[0] {
            Ability::Triggered { effect, .. } => effect.clone(), o => panic!("{o:?}") };
        let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
        e.resolve_effect(&eff, &ResolutionCtx { controller: Some(PlayerId(0)), source: Some(src), ..Default::default() }, WbReason::Resolve(StackId(0)));
        assert!(e.state.players[0].battlefield.iter().any(|&o| e.state.object(o).chars.name == "Inkling"));
    }
}
