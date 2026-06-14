//! Root Maze — `{G}` Enchantment. "Artifacts and lands enter tapped." (GLOBAL replacement,
//! affects all players' artifacts/lands.) First printed TMP (Tempest).

use crate::basics::{CardType, Color};
use crate::cards::{grp, enchantment, mana_cost, CardDb};
use crate::effects::ability::{Ability, ActionPattern, Rewrite};
use crate::effects::target::CardFilter;


pub fn register(db: &mut CardDb) {
    db.insert(enchantment(
        grp::ROOT_MAZE,
        "Root Maze",
        Color::Green,
        mana_cost(1, &[]),
        vec![Ability::Replacement {
            pattern: ActionPattern::WouldEnterBattlefield(CardFilter::AnyOf(vec![
                CardFilter::HasCardType(CardType::Artifact),
                CardFilter::HasCardType(CardType::Land),
            ])),
            rewrite: Rewrite::EntersTapped,
        }],
    ).with_text("Artifacts and lands enter tapped."));
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn root_maze_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(grp::ROOT_MAZE).unwrap();
        expect![[r#"
            [
                Replacement {
                    pattern: WouldEnterBattlefield(
                        AnyOf(
                            [
                                HasCardType(
                                    Artifact,
                                ),
                                HasCardType(
                                    Land,
                                ),
                            ],
                        ),
                    ),
                    rewrite: EntersTapped,
                },
            ]"#]].assert_eq(&format!("{:#?}", def.abilities));
    }
}
