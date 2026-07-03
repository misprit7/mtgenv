//! Shopkeeper's Bane — `{2}{G}` Creature — Badger Pest 4/2 (first printed SOS).
//!
//! Oracle: "Trample / Whenever this creature attacks, you gain 2 life."
//!
//! **Fully implemented** — printed Trample (CR 702.19) plus a `SelfAttacks` triggered ability
//! (CR 508.1m) that gains the controller 2 life on attack.

use crate::basics::Color;
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern, Keyword};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const SHOPKEEPERS_BANE: u32 = 208;

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        SHOPKEEPERS_BANE,
        "Shopkeeper's Bane",
        &[CreatureType::Badger, CreatureType::Pest],
        Color::Green,
        mana_cost(2, &[(Color::Green, 1)]),
        4,
        2,
        vec![Ability::Triggered {
            event: EventPattern::SelfAttacks,
            condition: None,
            intervening_if: false,
            effect: Effect::GainLife {
                who: PlayerRef::Controller,
                amount: ValueExpr::Fixed(2),
            },
        }],
    );
    def.chars.keywords = vec![Keyword::Trample];
    def.text = "Trample\nWhenever this creature attacks, you gain 2 life.".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn shopkeepers_bane_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(SHOPKEEPERS_BANE).unwrap();
        assert_eq!(def.chars.power, Some(4));
        assert_eq!(def.chars.toughness, Some(2));
        assert_eq!(def.chars.keywords, vec![Keyword::Trample]);
        assert!(def.fully_implemented);
        expect![[r#"
            [
                Triggered {
                    event: SelfAttacks,
                    condition: None,
                    intervening_if: false,
                    effect: GainLife {
                        who: Controller,
                        amount: Fixed(
                            2,
                        ),
                    },
                },
            ]"#]].assert_eq(&format!("{:#?}", def.abilities));
    }

    /// Behaviour: resolving the attack trigger gains the controller 2 life.
    #[test]
    fn shopkeepers_bane_attack_gains_life() {
        use crate::agent::RandomAgent;
        use crate::basics::Zone;
        use crate::cards::build_game;
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        let mut state = build_game(1, &[&[], &[]]);
        let chars = state.card_db().get(SHOPKEEPERS_BANE).unwrap().chars.clone();
        let src = state.add_card(PlayerId(0), chars, Zone::Battlefield);
        let trig = match &state.card_db().get(SHOPKEEPERS_BANE).unwrap().abilities[0] {
            Ability::Triggered { effect, .. } => effect.clone(),
            o => panic!("expected SelfAttacks Triggered, got {o:?}"),
        };
        let mut e = Engine::new(
            state,
            vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))],
        );
        let p0 = e.state.player(PlayerId(0)).life;
        e.resolve_effect(
            &trig,
            &ResolutionCtx { controller: Some(PlayerId(0)), source: Some(src), ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        assert_eq!(e.state.player(PlayerId(0)).life, p0 + 2, "you gain 2 life on attack");
    }
}
