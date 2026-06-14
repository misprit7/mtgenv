//! Erode — `{W}` Instant (first printed SOS).
//!
//! Oracle: "Destroy target creature or planeswalker. Its controller may search their library for a
//! basic land card, put it onto the battlefield tapped, then shuffle."
//!
//! Fully implemented (no deferrals):
//! - Destroy a targeted creature/planeswalker (`Effect::Destroy` over a `Target` permanent — the
//!   single declared target, so `collect_target_specs` prompts for it at cast).
//! - The rider, searched by the *destroyed permanent's controller* (`ControllerOfTarget(0)` — its
//!   controller snapshotted at resolution start, before the Destroy graveyards it). "may search"
//!   is the `min: 0` of the fetch: that player picks 0 (decline) or 1 basic. The engine asks the
//!   `who` player to `SelectCards`, so the opponent — not the caster — makes the choice.

use crate::basics::{CardType, Color};
use crate::cards::helpers::fetch_basic_tapped_by;
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::PlayerRef;
use crate::effects::{Effect, EffectTarget};

/// grp id (per-set ids live near their cards).
pub const ERODE: u32 = 108;

pub fn register(db: &mut CardDb) {
    let effect = Effect::Sequence(vec![
        Effect::Destroy {
            what: EffectTarget::Target(TargetSpec {
                kind: TargetKind::Permanent(CardFilter::AnyOf(vec![
                    CardFilter::HasCardType(CardType::Creature),
                    CardFilter::HasCardType(CardType::Planeswalker),
                ])),
                min: 1,
                max: 1,
                distinct: true,
            }),
        },
        // "Its controller may search …" — the destroyed permanent's controller (target 0).
        fetch_basic_tapped_by(PlayerRef::ControllerOfTarget(0)),
    ]);
    db.insert(
        spell(
            ERODE,
            "Erode",
            CardType::Instant,
            Color::White,
            mana_cost(0, &[(Color::White, 1)]),
            effect,
        )
        .with_text("Destroy target creature or planeswalker. Its controller may search their library for a basic land card, put it onto the battlefield tapped, then shuffle."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn erode_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(ERODE).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Instant]);
        assert!(def.fully_implemented); // no deferred clauses
        expect![[r#"
            Sequence(
                [
                    Destroy {
                        what: Target(
                            TargetSpec {
                                kind: Permanent(
                                    AnyOf(
                                        [
                                            HasCardType(
                                                Creature,
                                            ),
                                            HasCardType(
                                                Planeswalker,
                                            ),
                                        ],
                                    ),
                                ),
                                min: 1,
                                max: 1,
                                distinct: true,
                            },
                        ),
                    },
                    Search {
                        who: ControllerOfTarget(
                            0,
                        ),
                        zone: Library,
                        filter: All(
                            [
                                HasCardType(
                                    Land,
                                ),
                                Supertype(
                                    Basic,
                                ),
                            ],
                        ),
                        min: 0,
                        max: 1,
                        to: ZoneDest {
                            zone: Battlefield,
                            pos: Any,
                        },
                        tapped: true,
                    },
                ],
            )"#]].assert_eq(&format!("{:#?}", def.spell_effect().unwrap()));
    }
}
