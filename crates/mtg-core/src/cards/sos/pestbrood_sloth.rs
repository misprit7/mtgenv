//! Pestbrood Sloth — `{3}{G}` Creature — Plant Sloth 4/4 (first printed SOS).
//!
//! Oracle: "Reach / When this creature dies, create two 1/1 black and green Pest creature tokens with
//! \"Whenever this token attacks, you gain 1 life.\""
//!
//! **Fully implemented** — printed Reach + a dies-trigger creating two ability-bearing Pest tokens
//! (S11 token-with-ability).

use crate::basics::Color;
use crate::cards::helpers::pest_token;
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern, Keyword};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const PESTBROOD_SLOTH: u32 = 291;

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        PESTBROOD_SLOTH,
        "Pestbrood Sloth",
        &[CreatureType::Plant, CreatureType::Sloth],
        Color::Green,
        mana_cost(3, &[(Color::Green, 1)]),
        4,
        4,
        vec![Ability::Triggered {
            event: EventPattern::SelfDies,
            condition: None,
            intervening_if: false,
            effect: Effect::CreateToken { spec: pest_token(), count: ValueExpr::Fixed(2), controller: PlayerRef::Controller },
        }],
    );
    def.chars.keywords = vec![Keyword::Reach];
    def.text = "Reach\nWhen this creature dies, create two 1/1 black and green Pest creature tokens with \"Whenever this token attacks, you gain 1 life.\"".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn pestbrood_sloth_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(PESTBROOD_SLOTH).unwrap();
        assert_eq!(def.chars.keywords, vec![Keyword::Reach]);
        assert!(def.fully_implemented);
        expect![[r#"
            [
                Triggered {
                    event: SelfDies,
                    condition: None,
                    intervening_if: false,
                    effect: CreateToken {
                        spec: TokenSpec {
                            name: "Pest",
                            card_types: [
                                Creature,
                            ],
                            subtypes: [
                                Creature(
                                    Pest,
                                ),
                            ],
                            colors: [
                                Black,
                                Green,
                            ],
                            power: 1,
                            toughness: 1,
                            keywords: [],
                            counters: [],
                            grp_id: 9001,
                        },
                        count: Fixed(
                            2,
                        ),
                        controller: Controller,
                    },
                },
            ]"#]].assert_eq(&format!("{:#?}", def.abilities));
    }

    #[test]
    fn pestbrood_sloth_dies_makes_two_pests() {
        use crate::agent::RandomAgent;
        use crate::basics::Zone;
        use crate::cards::build_game;
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        let mut state = build_game(1, &[&[], &[]]);
        let src = state.add_card(PlayerId(0), state.card_db().get(PESTBROOD_SLOTH).unwrap().chars.clone(), Zone::Battlefield);
        let eff = match &state.card_db().get(PESTBROOD_SLOTH).unwrap().abilities[0] {
            Ability::Triggered { effect, .. } => effect.clone(), o => panic!("{o:?}") };
        let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
        e.resolve_effect(&eff, &ResolutionCtx { controller: Some(PlayerId(0)), source: Some(src), ..Default::default() }, WbReason::Resolve(StackId(0)));
        let pests = e.state.players[0].battlefield.iter().filter(|&&o| e.state.object(o).chars.name == "Pest").count();
        assert_eq!(pests, 2, "two Pest tokens created");
    }
}
