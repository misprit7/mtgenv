//! Bushwhack — `{G}` Sorcery (first printed BRO, The Brothers' War).
//!
//! Oracle:
//!   Choose one —
//!   • Search your library for a basic land card, reveal it, put it into your hand, then shuffle.
//!   • Target creature you control fights target creature you don't control.
//!
//! Fully implemented (no deferrals): a modal "choose one" (`Effect::Modal`, C7) over the two modes
//! — mode 1 is the C5 search to **hand** (declares no targets); mode 2 is `Effect::Fight` (C8) with
//! its two targets declared via `Target(TargetSpec)` (a creature you control vs one you don't), so
//! the engine's cast-time modal flow collects *only the chosen mode's* targets at 601.2c.

use crate::basics::{CardType, Color, Zone, ZoneDest, ZonePos};
use crate::cards::helpers::basic_land_filter;
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::PlayerRef;
use crate::effects::{Effect, EffectTarget, Mode};

/// grp id (per-set ids live near their cards).
pub const BUSHWHACK: u32 = 111;

/// "target creature you {control / don't control}" — a single creature target.
fn creature_target(filter: CardFilter) -> EffectTarget {
    EffectTarget::Target(TargetSpec {
        kind: TargetKind::Creature(filter),
        min: 1,
        max: 1,
        distinct: true,
    })
}

pub fn register(db: &mut CardDb) {
    let effect = Effect::Modal {
        modes: vec![
            // "Search your library for a basic land card … put it into your hand …"
            Mode {
                label: "Search for a basic land card (to your hand)".to_string(),
                effect: Effect::Search {
                    who: PlayerRef::Controller,
                    zone: Zone::Library,
                    filter: basic_land_filter(),
                    min: 0,
                    max: 1,
                    to: ZoneDest { zone: Zone::Hand, pos: ZonePos::Any },
                    tapped: false,
                },
            },
            // "Target creature you control fights target creature you don't control."
            Mode {
                label: "Fight (your creature vs theirs)".to_string(),
                effect: Effect::Fight {
                    a: creature_target(CardFilter::ControlledBy(PlayerRef::Controller)),
                    b: creature_target(CardFilter::Not(Box::new(CardFilter::ControlledBy(
                        PlayerRef::Controller,
                    )))),
                },
            },
        ],
        min: 1,
        max: 1,
        allow_repeat: false,
    };
    db.insert(
        spell(
            BUSHWHACK,
            "Bushwhack",
            CardType::Sorcery,
            Color::Green,
            mana_cost(0, &[(Color::Green, 1)]),
            effect,
        )
        .with_text("Choose one —\n• Search your library for a basic land card, reveal it, put it into your hand, then shuffle.\n• Target creature you control fights target creature you don't control."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn bushwhack_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(BUSHWHACK).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Sorcery]);
        assert!(def.fully_implemented); // both modes faithful (search→hand + fight)
        expect![[r#"
            Modal {
                modes: [
                    Mode {
                        label: "Search for a basic land card (to your hand)",
                        effect: Search {
                            who: Controller,
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
                                zone: Hand,
                                pos: Any,
                            },
                            tapped: false,
                        },
                    },
                    Mode {
                        label: "Fight (your creature vs theirs)",
                        effect: Fight {
                            a: Target(
                                TargetSpec {
                                    kind: Creature(
                                        ControlledBy(
                                            Controller,
                                        ),
                                    ),
                                    min: 1,
                                    max: 1,
                                    distinct: true,
                                },
                            ),
                            b: Target(
                                TargetSpec {
                                    kind: Creature(
                                        Not(
                                            ControlledBy(
                                                Controller,
                                            ),
                                        ),
                                    ),
                                    min: 1,
                                    max: 1,
                                    distinct: true,
                                },
                            ),
                        },
                    },
                ],
                min: 1,
                max: 1,
                allow_repeat: false,
            }"#]].assert_eq(&format!("{:#?}", def.spell_effect().unwrap()));
    }

    /// Behaviour: choosing the fight mode (mode 1) makes your creature and an opponent's creature deal
    /// damage equal to their power to each other — two 2/2s each end with 2 marked damage (mutually lethal).
    #[test]
    fn bushwhack_fight_mode_deals_mutual_damage() {
        use crate::agent::RandomAgent;
        use crate::basics::{Target, Zone};
        use crate::cards::{build_game, grp};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        let mut state = build_game(1, &[&[], &[]]);
        let bears_chars = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
        let mine = state.add_card(PlayerId(0), bears_chars.clone(), Zone::Battlefield); // my 2/2
        let theirs = state.add_card(PlayerId(1), bears_chars, Zone::Battlefield); // their 2/2
        let bushwhack = state.card_db().get(BUSHWHACK).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(
            state,
            vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))],
        );
        // Mode 1 = "Target creature you control fights target creature you don't control."
        e.resolve_effect(
            &bushwhack,
            &ResolutionCtx {
                controller: Some(PlayerId(0)),
                chosen_modes: vec![1],
                chosen_targets: vec![Target::Object(mine), Target::Object(theirs)],
                ..Default::default()
            },
            WbReason::Resolve(StackId(0)),
        );
        assert_eq!(e.state.objects.get(&mine).unwrap().damage_marked, 2, "mine took 2 from theirs");
        assert_eq!(e.state.objects.get(&theirs).unwrap().damage_marked, 2, "theirs took 2 from mine");
    }
}
