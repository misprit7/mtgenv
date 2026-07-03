//! Stirring Hopesinger — `{2}{W}` Creature — Bird Bard 1/3 (first printed SOS).
//!
//! Oracle: "Flying, lifelink / Repartee — Whenever you cast an instant or sorcery spell that targets
//! a creature, put a +1/+1 counter on each creature you control."
//!
//! **Fully implemented** — printed Flying + Lifelink + a Repartee cast-trigger putting a +1/+1
//! counter on each creature you control (`ForEach`).

use crate::basics::{Color, CounterKind, Zone};
use crate::cards::helpers::instant_or_sorcery;
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern, Keyword};
use crate::effects::target::{CardFilter, SelectSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};
use crate::basics::CardType;
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const STIRRING_HOPESINGER: u32 = 263;

pub fn register(db: &mut CardDb) {
    let creatures_you_control = SelectSpec {
        zone: Zone::Battlefield,
        filter: CardFilter::All(vec![
            CardFilter::HasCardType(CardType::Creature),
            CardFilter::ControlledBy(PlayerRef::Controller),
        ]),
        chooser: PlayerRef::Controller,
        min: ValueExpr::Fixed(0),
        max: ValueExpr::Fixed(999),
    };
    let mut def = creature(
        STIRRING_HOPESINGER,
        "Stirring Hopesinger",
        &[CreatureType::Bird, CreatureType::Bard],
        Color::White,
        mana_cost(2, &[(Color::White, 1)]),
        1,
        3,
        vec![Ability::Triggered {
            event: EventPattern::SpellCastTargetingCreature(instant_or_sorcery()),
            condition: None,
            intervening_if: false,
            effect: Effect::ForEach {
                selector: creatures_you_control,
                body: Box::new(Effect::PutCounters {
                    what: EffectTarget::Each,
                    kind: CounterKind::PlusOnePlusOne,
                    n: ValueExpr::Fixed(1),
                }),
            },
        }],
    );
    def.chars.keywords = vec![Keyword::Flying, Keyword::Lifelink];
    def.text = "Flying, lifelink\nRepartee — Whenever you cast an instant or sorcery spell that targets a creature, put a +1/+1 counter on each creature you control.".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn stirring_hopesinger_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(STIRRING_HOPESINGER).unwrap();
        assert_eq!(def.chars.keywords, vec![Keyword::Flying, Keyword::Lifelink]);
        assert!(def.fully_implemented);
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
                    effect: ForEach {
                        selector: SelectSpec {
                            zone: Battlefield,
                            filter: All(
                                [
                                    HasCardType(
                                        Creature,
                                    ),
                                    ControlledBy(
                                        Controller,
                                    ),
                                ],
                            ),
                            chooser: Controller,
                            min: Fixed(
                                0,
                            ),
                            max: Fixed(
                                999,
                            ),
                        },
                        body: PutCounters {
                            what: Each,
                            kind: PlusOnePlusOne,
                            n: Fixed(
                                1,
                            ),
                        },
                    },
                },
            ]"#]].assert_eq(&format!("{:#?}", def.abilities));
    }

    #[test]
    fn stirring_hopesinger_repartee_counters_team() {
        use crate::agent::RandomAgent;
        use crate::cards::build_game;
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        let mut state = build_game(1, &[&[], &[]]);
        let src = state.add_card(PlayerId(0), state.card_db().get(STIRRING_HOPESINGER).unwrap().chars.clone(), Zone::Battlefield);
        let eff = match &state.card_db().get(STIRRING_HOPESINGER).unwrap().abilities[0] {
            Ability::Triggered { effect, .. } => effect.clone(), o => panic!("{o:?}") };
        let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
        e.resolve_effect(&eff, &ResolutionCtx { controller: Some(PlayerId(0)), source: Some(src), ..Default::default() }, WbReason::Resolve(StackId(0)));
        assert_eq!(e.state.computed(src).power, Some(2), "Hopesinger itself got a +1/+1 counter");
    }
}
