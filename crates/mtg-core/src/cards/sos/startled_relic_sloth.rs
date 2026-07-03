//! Startled Relic Sloth — `{2}{R}{W}` Creature — Sloth Beast 4/4 (first printed SOS).
//!
//! Oracle: "Trample, lifelink / At the beginning of combat on your turn, exile up to one target
//! card from a graveyard."
//!
//! **Fully implemented** — printed Trample + Lifelink, plus a begin-combat (your turn) trigger that
//! exiles "up to one" (`min: 0`) target card from any graveyard (graveyard-hate). Multicolored (R/W).

use crate::basics::{Color, Phase, Zone};
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern, Keyword};
use crate::effects::condition::Condition;
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::{Effect, EffectTarget};
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const STARTLED_RELIC_SLOTH: u32 = 221;

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        STARTLED_RELIC_SLOTH,
        "Startled Relic Sloth",
        &[CreatureType::Sloth, CreatureType::Beast],
        Color::Red,
        mana_cost(2, &[(Color::Red, 1), (Color::White, 1)]),
        4,
        4,
        vec![Ability::Triggered {
            event: EventPattern::BeginningOfStep(Phase::BeginCombat),
            condition: Some(Condition::YourTurn),
            intervening_if: false,
            effect: Effect::Exile {
                what: EffectTarget::Target(TargetSpec {
                    kind: TargetKind::CardInZone { zone: Zone::Graveyard, filter: CardFilter::Any },
                    min: 0,
                    max: 1,
                    distinct: true,
                }),
            },
        }],
    );
    def.chars.colors = vec![Color::Red, Color::White];
    def.chars.keywords = vec![Keyword::Trample, Keyword::Lifelink];
    def.text = "Trample, lifelink\nAt the beginning of combat on your turn, exile up to one target card from a graveyard.".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn startled_relic_sloth_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(STARTLED_RELIC_SLOTH).unwrap();
        assert_eq!(def.chars.colors, vec![Color::Red, Color::White]);
        assert_eq!(def.chars.keywords, vec![Keyword::Trample, Keyword::Lifelink]);
        assert!(def.fully_implemented);
        expect![[r#"
            [
                Triggered {
                    event: BeginningOfStep(
                        BeginCombat,
                    ),
                    condition: Some(
                        YourTurn,
                    ),
                    intervening_if: false,
                    effect: Exile {
                        what: Target(
                            TargetSpec {
                                kind: CardInZone {
                                    zone: Graveyard,
                                    filter: Any,
                                },
                                min: 0,
                                max: 1,
                                distinct: true,
                            },
                        ),
                    },
                },
            ]"#]].assert_eq(&format!("{:#?}", def.abilities));
    }

    /// Behaviour: resolving the trigger exiles the targeted card from the graveyard.
    #[test]
    fn startled_relic_sloth_exiles_a_graveyard_card() {
        use crate::agent::RandomAgent;
        use crate::basics::{Target, Zone};
        use crate::cards::{build_game, grp};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        let mut state = build_game(1, &[&[], &[]]);
        let bolt = state.card_db().get(grp::LIGHTNING_BOLT).unwrap().chars.clone();
        let card = state.add_card(PlayerId(1), bolt, Zone::Graveyard);
        let trig = match &state.card_db().get(STARTLED_RELIC_SLOTH).unwrap().abilities[0] {
            Ability::Triggered { effect, .. } => effect.clone(),
            o => panic!("expected begin-combat Triggered, got {o:?}"),
        };
        let mut e = Engine::new(
            state,
            vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))],
        );
        e.resolve_effect(
            &trig,
            &ResolutionCtx {
                controller: Some(PlayerId(0)),
                chosen_targets: vec![Target::Object(card)],
                ..Default::default()
            },
            WbReason::Resolve(StackId(0)),
        );
        assert!(!e.state.players[1].graveyard.contains(&card), "left the graveyard");
        assert!(e.state.players[1].exile.contains(&card), "exiled");
    }
}
