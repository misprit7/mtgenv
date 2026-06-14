//! Hardened Scales — `{G}` Enchantment. "If one or more +1/+1 counters would be put on a
//! creature you control, that many plus one are put on it instead." (GLOBAL counter modifier
//! scoped to creatures the controller controls.) First printed KTK (Khans of Tarkir).

use crate::basics::{Color, CounterKind};
use crate::cards::{grp, enchantment, mana_cost, CardDb};
use crate::effects::ability::{Ability, ActionPattern, Rewrite};
use crate::effects::target::CardFilter;
use crate::effects::value::PlayerRef;


pub fn register(db: &mut CardDb) {
    db.insert(enchantment(
        grp::HARDENED_SCALES,
        "Hardened Scales",
        Color::Green,
        mana_cost(0, &[(Color::Green, 1)]),
        vec![Ability::Replacement {
            pattern: ActionPattern::WouldAddCounters {
                kind: CounterKind::PlusOnePlusOne,
                to: CardFilter::ControlledBy(PlayerRef::Controller),
            },
            rewrite: Rewrite::AddAmount(1),
        }],
    ).with_text("If one or more +1/+1 counters would be put on a creature you control, that many plus one +1/+1 counters are put on it instead."));
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn hardened_scales_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(grp::HARDENED_SCALES).unwrap();
        expect![[r#"
            [
                Replacement {
                    pattern: WouldAddCounters {
                        kind: PlusOnePlusOne,
                        to: ControlledBy(
                            Controller,
                        ),
                    },
                    rewrite: AddAmount(
                        1,
                    ),
                },
            ]"#]].assert_eq(&format!("{:#?}", def.abilities));
    }
}
