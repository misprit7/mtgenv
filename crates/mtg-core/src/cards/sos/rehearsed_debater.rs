//! Rehearsed Debater — `{2}{W}` Creature — Djinn Bard 3/3 (first printed SOS).
//!
//! Oracle: "Vigilance / Repartee — Whenever you cast an instant or sorcery spell that targets a
//! creature, this creature gets +1/+1 until end of turn."
//!
//! **Fully implemented** — printed Vigilance + a Repartee cast-trigger pumping itself +1/+1 EOT.

use crate::basics::Color;
use crate::cards::helpers::instant_or_sorcery;
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern, Keyword};
use crate::effects::condition::Duration;
use crate::effects::value::ValueExpr;
use crate::effects::{Effect, EffectTarget};
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const REHEARSED_DEBATER: u32 = 261;

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        REHEARSED_DEBATER,
        "Rehearsed Debater",
        &[CreatureType::Djinn, CreatureType::Bard],
        Color::White,
        mana_cost(2, &[(Color::White, 1)]),
        3,
        3,
        vec![Ability::Triggered {
            event: EventPattern::SpellCastTargetingCreature(instant_or_sorcery()),
            condition: None,
            intervening_if: false,
            effect: Effect::PumpPT {
                what: EffectTarget::SourceSelf,
                power: ValueExpr::Fixed(1),
                toughness: ValueExpr::Fixed(1),
                duration: Duration::UntilEndOfTurn,
            },
        }],
    );
    def.chars.keywords = vec![Keyword::Vigilance];
    def.text = "Vigilance\nRepartee — Whenever you cast an instant or sorcery spell that targets a creature, this creature gets +1/+1 until end of turn.".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn rehearsed_debater_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(REHEARSED_DEBATER).unwrap();
        assert_eq!(def.chars.keywords, vec![Keyword::Vigilance]);
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
                    effect: PumpPT {
                        what: SourceSelf,
                        power: Fixed(
                            1,
                        ),
                        toughness: Fixed(
                            1,
                        ),
                        duration: UntilEndOfTurn,
                    },
                },
            ]"#]].assert_eq(&format!("{:#?}", def.abilities));
    }

    #[test]
    fn rehearsed_debater_repartee_pumps() {
        use crate::agent::RandomAgent;
        use crate::basics::Zone;
        use crate::cards::build_game;
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        let mut state = build_game(1, &[&[], &[]]);
        let src = state.add_card(PlayerId(0), state.card_db().get(REHEARSED_DEBATER).unwrap().chars.clone(), Zone::Battlefield);
        let eff = match &state.card_db().get(REHEARSED_DEBATER).unwrap().abilities[0] {
            Ability::Triggered { effect, .. } => effect.clone(), o => panic!("{o:?}") };
        let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
        e.resolve_effect(&eff, &ResolutionCtx { controller: Some(PlayerId(0)), source: Some(src), ..Default::default() }, WbReason::Resolve(StackId(0)));
        assert_eq!(e.state.computed(src).power, Some(4));
    }
}
