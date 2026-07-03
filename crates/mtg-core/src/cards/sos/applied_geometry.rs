//! Applied Geometry — `{2}{G}{U}` Sorcery (first printed SOS).
//!
//! Oracle: "Create a token that's a copy of target non-Aura permanent you control, except it's a
//! 0/0 Fractal creature in addition to its other types. Put six +1/+1 counters on it."
//!
//! **Fully implemented** — a single `Effect::CreateTokenCopy` (S14 token-copy cap). The copy
//! snapshots the target permanent's copiable characteristics (CR 707.2), then the `mods` apply the
//! "except" overrides: add Creature + Fractal, set base P/T to 0/0, and enter with six +1/+1
//! counters (a 6/6, plus whatever else the source contributed by type/abilities). Multicolored
//! (G/U): built via the `spell` helper (single colour) then the colour vector is corrected.

use crate::basics::{CardType, Color, CounterKind};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::target::{CardFilter, TargetKind, TargetSpec, TokenCopyMods};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};
use crate::subtypes::{CreatureType, EnchantmentType};

/// grp id (per-set ids live near their cards).
pub const APPLIED_GEOMETRY: u32 = 306;

pub fn register(db: &mut CardDb) {
    let effect = Effect::CreateTokenCopy {
        source: EffectTarget::Target(TargetSpec {
            kind: TargetKind::Permanent(CardFilter::All(vec![
                CardFilter::ControlledBy(PlayerRef::Controller),
                CardFilter::Not(Box::new(CardFilter::HasSubtype(EnchantmentType::Aura.into()))),
            ])),
            min: 1,
            max: 1,
            distinct: true,
        }),
        controller: PlayerRef::Controller,
        mods: TokenCopyMods {
            add_card_types: vec![CardType::Creature],
            add_subtypes: vec![CreatureType::Fractal.into()],
            set_power_toughness: Some((0, 0)),
            counters: vec![(CounterKind::PlusOnePlusOne, ValueExpr::Fixed(6))],
        },
    };
    let mut def = spell(
        APPLIED_GEOMETRY,
        "Applied Geometry",
        CardType::Sorcery,
        Color::Green,
        mana_cost(2, &[(Color::Green, 1), (Color::Blue, 1)]),
        effect,
    )
    .with_text(
        "Create a token that's a copy of target non-Aura permanent you control, except it's a 0/0 \
         Fractal creature in addition to its other types. Put six +1/+1 counters on it.",
    );
    def.chars.colors = vec![Color::Green, Color::Blue];
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn applied_geometry_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(APPLIED_GEOMETRY).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Sorcery]);
        assert_eq!(def.chars.colors, vec![Color::Green, Color::Blue]);
        assert!(def.fully_implemented);
        expect![[r#"
            CreateTokenCopy {
                source: Target(
                    TargetSpec {
                        kind: Permanent(
                            All(
                                [
                                    ControlledBy(
                                        Controller,
                                    ),
                                    Not(
                                        HasSubtype(
                                            Enchantment(
                                                Aura,
                                            ),
                                        ),
                                    ),
                                ],
                            ),
                        ),
                        min: 1,
                        max: 1,
                        distinct: true,
                    },
                ),
                controller: Controller,
                mods: TokenCopyMods {
                    add_card_types: [
                        Creature,
                    ],
                    add_subtypes: [
                        Creature(
                            Fractal,
                        ),
                    ],
                    set_power_toughness: Some(
                        (
                            0,
                            0,
                        ),
                    ),
                    counters: [
                        (
                            PlusOnePlusOne,
                            Fixed(
                                6,
                            ),
                        ),
                    ],
                },
            }"#]]
        .assert_eq(&format!("{:#?}", def.spell_effect().unwrap()));
    }

    /// Behaviour: copying a 2/2 creature yields a 0/0 Fractal (plus the copied types) entering with
    /// six +1/+1 counters — a 6/6 — under the caster's control.
    #[test]
    fn applied_geometry_copies_as_6_6_fractal() {
        use crate::agent::RandomAgent;
        use crate::basics::{Target, Zone};
        use crate::cards::{build_game, grp};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        let mut state = build_game(1, &[&[], &[]]);
        let bears = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
        let source = state.add_card(PlayerId(0), bears, Zone::Battlefield);
        let effect = state.card_db().get(APPLIED_GEOMETRY).unwrap().spell_effect().unwrap().clone();
        let before = state.players[0].battlefield.len();
        let mut e = Engine::new(
            state,
            vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))],
        );
        e.resolve_effect(
            &effect,
            &ResolutionCtx {
                controller: Some(PlayerId(0)),
                chosen_targets: vec![Target::Object(source)],
                ..Default::default()
            },
            WbReason::Resolve(StackId(0)),
        );
        assert_eq!(e.state.players[0].battlefield.len(), before + 1, "one token created");
        let token = *e.state.players[0].battlefield.last().unwrap();
        let chars = &e.state.object(token).chars;
        assert_eq!(chars.name, "Grizzly Bears", "copies the source's name");
        assert!(chars.subtypes.contains(&CreatureType::Fractal.into()), "gains Fractal");
        assert_eq!(e.state.computed(token).power, Some(6), "0/0 + six +1/+1 = 6/6");
        assert_eq!(e.state.computed(token).toughness, Some(6));
    }
}
