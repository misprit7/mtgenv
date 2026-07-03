//! Bogwater Lumaret — `{B}{G}` Creature — Spirit Frog 2/2 (first printed SOS).
//!
//! Oracle: "Whenever this creature or another creature you control enters, you gain 1 life."
//!
//! **Fully implemented** — a `PermanentEnters(creature you control)` triggered ability (CR 603.2)
//! that gains the controller 1 life. The filter matches the source itself entering too ("this
//! creature … enters"). Multicolored (B/G).

use crate::basics::{CardType, Color};
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern};
use crate::effects::target::CardFilter;
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const BOGWATER_LUMARET: u32 = 214;

pub fn register(db: &mut CardDb) {
    let creature_you_control = CardFilter::All(vec![
        CardFilter::HasCardType(CardType::Creature),
        CardFilter::ControlledBy(PlayerRef::Controller),
    ]);
    let mut def = creature(
        BOGWATER_LUMARET,
        "Bogwater Lumaret",
        &[CreatureType::Spirit, CreatureType::Frog],
        Color::Black,
        mana_cost(0, &[(Color::Black, 1), (Color::Green, 1)]),
        2,
        2,
        vec![Ability::Triggered {
            event: EventPattern::PermanentEnters(creature_you_control),
            condition: None,
            intervening_if: false,
            effect: Effect::GainLife {
                who: PlayerRef::Controller,
                amount: ValueExpr::Fixed(1),
            },
        }],
    );
    def.chars.colors = vec![Color::Black, Color::Green];
    def.text = "Whenever this creature or another creature you control enters, you gain 1 life.".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn bogwater_lumaret_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(BOGWATER_LUMARET).unwrap();
        assert_eq!(def.chars.colors, vec![Color::Black, Color::Green]);
        assert!(def.fully_implemented);
        expect![[r#"
            [
                Triggered {
                    event: PermanentEnters(
                        All(
                            [
                                HasCardType(
                                    Creature,
                                ),
                                ControlledBy(
                                    Controller,
                                ),
                            ],
                        ),
                    ),
                    condition: None,
                    intervening_if: false,
                    effect: GainLife {
                        who: Controller,
                        amount: Fixed(
                            1,
                        ),
                    },
                },
            ]"#]].assert_eq(&format!("{:#?}", def.abilities));
    }

    /// Behaviour: resolving the enters-trigger gains the controller 1 life.
    #[test]
    fn bogwater_lumaret_gains_life_on_enter() {
        use crate::agent::RandomAgent;
        use crate::basics::Zone;
        use crate::cards::build_game;
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        let mut state = build_game(1, &[&[], &[]]);
        let chars = state.card_db().get(BOGWATER_LUMARET).unwrap().chars.clone();
        let src = state.add_card(PlayerId(0), chars, Zone::Battlefield);
        let trig = match &state.card_db().get(BOGWATER_LUMARET).unwrap().abilities[0] {
            Ability::Triggered { effect, .. } => effect.clone(),
            o => panic!("expected enters Triggered, got {o:?}"),
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
        assert_eq!(e.state.player(PlayerId(0)).life, p0 + 1, "gain 1 life per creature entering");
    }
}
