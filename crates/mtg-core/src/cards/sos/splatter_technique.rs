//! Splatter Technique — `{1}{U}{U}{R}{R}` Sorcery (first printed SOS).
//!
//! Oracle: "Choose one —
//!   • Draw four cards.
//!   • Splatter Technique deals 4 damage to each creature and planeswalker."
//!
//! **Fully implemented** — a `Modal` "choose one": mode 1 draws four; mode 2 is a **multi-player**
//! `ForEach` over every creature and planeswalker (both players — the `PlayerRef::EachPlayer` area
//! selector, landed alongside this card) dealing 4 to each.

use crate::basics::{CardType, Color, DamageKind, Zone};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::target::{CardFilter, SelectSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget, Mode};

/// grp id (per-set ids live near their cards).
pub const SPLATTER_TECHNIQUE: u32 = 326;

pub fn register(db: &mut CardDb) {
    let effect = Effect::Modal {
        modes: vec![
            Mode {
                label: "Draw four cards".to_string(),
                effect: Effect::Draw { who: PlayerRef::Controller, count: ValueExpr::Fixed(4) },
            },
            Mode {
                label: "Deal 4 damage to each creature and planeswalker".to_string(),
                effect: Effect::ForEach {
                    // Every creature/planeswalker in play — both players (CR area effect).
                    selector: SelectSpec {
                        zone: Zone::Battlefield,
                        filter: CardFilter::AnyOf(vec![
                            CardFilter::HasCardType(CardType::Creature),
                            CardFilter::HasCardType(CardType::Planeswalker),
                        ]),
                        chooser: PlayerRef::EachPlayer,
                        min: ValueExpr::Fixed(0),
                        max: ValueExpr::Fixed(999),
                    },
                    body: Box::new(Effect::DealDamage {
                        amount: ValueExpr::Fixed(4),
                        to: EffectTarget::Each,
                        kind: DamageKind::Noncombat,
                    }),
                },
            },
        ],
        min: 1,
        max: 1,
        allow_repeat: false,
    };
    let mut def = spell(
        SPLATTER_TECHNIQUE,
        "Splatter Technique",
        CardType::Sorcery,
        Color::Blue,
        mana_cost(1, &[(Color::Blue, 2), (Color::Red, 2)]),
        effect,
    )
    .with_text("Choose one —\n• Draw four cards.\n• Splatter Technique deals 4 damage to each creature and planeswalker.");
    def.chars.colors = vec![Color::Blue, Color::Red];
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn splatter_technique_shape() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(SPLATTER_TECHNIQUE).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Sorcery]);
        assert_eq!(def.chars.colors, vec![Color::Blue, Color::Red]);
        assert_eq!(def.chars.mana_value(), 5);
        assert!(def.fully_implemented);
    }

    #[test]
    fn splatter_technique_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(SPLATTER_TECHNIQUE).unwrap();
        expect![[r#"
            Modal {
                modes: [
                    Mode {
                        label: "Draw four cards",
                        effect: Draw {
                            who: Controller,
                            count: Fixed(
                                4,
                            ),
                        },
                    },
                    Mode {
                        label: "Deal 4 damage to each creature and planeswalker",
                        effect: ForEach {
                            selector: SelectSpec {
                                zone: Battlefield,
                                filter: AnyOf(
                                    [
                                        HasCardType(
                                            Creature,
                                        ),
                                        HasCardType(
                                            Planeswalker,
                                        ),
                                    ],
                                ),
                                chooser: EachPlayer,
                                min: Fixed(
                                    0,
                                ),
                                max: Fixed(
                                    999,
                                ),
                            },
                            body: DealDamage {
                                amount: Fixed(
                                    4,
                                ),
                                to: Each,
                                kind: Noncombat,
                            },
                        },
                    },
                ],
                min: 1,
                max: 1,
                allow_repeat: false,
            }"#]].assert_eq(&format!("{:#?}", def.spell_effect().unwrap()));
    }

    /// Behaviour: mode 2 deals 4 to every creature on BOTH battlefields (multi-player ForEach).
    #[test]
    fn splatter_technique_damages_all_creatures_both_sides() {
        use crate::agent::RandomAgent;
        use crate::basics::Zone;
        use crate::cards::{build_game, grp};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;

        let mut state = build_game(1, &[&[], &[]]);
        let mine = {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        let theirs = {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            state.add_card(PlayerId(1), c, Zone::Battlefield)
        };
        let mode = match state.card_db().get(SPLATTER_TECHNIQUE).unwrap().spell_effect() {
            Some(Effect::Modal { modes, .. }) => modes[1].effect.clone(),
            _ => panic!("expected Modal"),
        };
        let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
        e.resolve_effect(
            &mode,
            &ResolutionCtx { controller: Some(PlayerId(0)), ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        assert_eq!(e.state.object(mine).damage_marked, 4, "your creature took 4");
        assert_eq!(e.state.object(theirs).damage_marked, 4, "the opponent's creature took 4");
    }
}
