//! Moment of Reckoning — `{3}{W}{W}{B}{B}` Sorcery (first printed SOS).
//!
//! Oracle: "Choose up to four. You may choose the same mode more than once.
//!   • Destroy target nonland permanent.
//!   • Return target nonland permanent card from your graveyard to the battlefield."
//!
//! **Fully implemented** — pure existing machinery: a **repeatable** `Modal` (`min: 0, max: 4,
//! allow_repeat: true`) over two modes that each already exist (`Destroy` a targeted nonland
//! permanent; `MoveZone` a targeted nonland permanent card from your graveyard to the battlefield).
//! Each chosen mode instance consumes its own target via the modal cursor, so choosing a mode
//! several times targets several objects. Multicolored (W/B).
//!
//! (First-pass caveat: cross-instance target *distinctness* for repeated modes isn't enforced by the
//! modal targeting — choosing the same mode twice on the same object simply wastes the second
//! instance, which then fizzles. No functional loss; noted for the future modal-distinctness pass.)

use crate::basics::{CardType, Color, Zone, ZoneDest, ZonePos};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::PlayerRef;
use crate::effects::{Effect, EffectTarget, Mode};

/// grp id (per-set ids live near their cards).
pub const MOMENT_OF_RECKONING: u32 = 418;

/// A "nonland permanent card" (CR 110.1 / 205) — a permanent-type card that isn't a land.
fn nonland_permanent_card() -> CardFilter {
    CardFilter::AnyOf(vec![
        CardFilter::HasCardType(CardType::Artifact),
        CardFilter::HasCardType(CardType::Creature),
        CardFilter::HasCardType(CardType::Enchantment),
        CardFilter::HasCardType(CardType::Planeswalker),
    ])
}

pub fn register(db: &mut CardDb) {
    let effect = Effect::Modal {
        modes: vec![
            Mode {
                label: "Destroy target nonland permanent".to_string(),
                effect: Effect::Destroy {
                    what: EffectTarget::Target(TargetSpec {
                        kind: TargetKind::Permanent(CardFilter::Not(Box::new(
                            CardFilter::HasCardType(CardType::Land),
                        ))),
                        min: 1,
                        max: 1,
                        distinct: true,
                    }),
                },
            },
            Mode {
                label: "Return target nonland permanent card from your graveyard to the battlefield"
                    .to_string(),
                effect: Effect::MoveZone {
                    what: EffectTarget::Target(TargetSpec {
                        kind: TargetKind::CardInZone {
                            zone: Zone::Graveyard,
                            filter: CardFilter::All(vec![
                                CardFilter::ControlledBy(PlayerRef::Controller),
                                nonland_permanent_card(),
                            ]),
                        },
                        min: 1,
                        max: 1,
                        distinct: true,
                    }),
                    to: ZoneDest { zone: Zone::Battlefield, pos: ZonePos::Any },
                    tapped: false,
                },
            },
        ],
        min: 0,
        max: 4,
        allow_repeat: true,
    };
    let mut def = spell(
        MOMENT_OF_RECKONING,
        "Moment of Reckoning",
        CardType::Sorcery,
        Color::White,
        mana_cost(3, &[(Color::White, 2), (Color::Black, 2)]),
        effect,
    )
    .with_text("Choose up to four. You may choose the same mode more than once.\n• Destroy target nonland permanent.\n• Return target nonland permanent card from your graveyard to the battlefield.");
    def.chars.colors = vec![Color::White, Color::Black];
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, CastVariant, DecisionRequest, DecisionResponse, PlayerView, RandomAgent};
    use crate::basics::{Phase, Target};
    use crate::cards::{build_game, grp};
    use crate::ids::{ObjId, PlayerId};
    use crate::priority::Engine;

    #[test]
    fn moment_of_reckoning_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(MOMENT_OF_RECKONING).unwrap();
        assert_eq!(def.chars.colors, vec![Color::White, Color::Black]);
        assert!(def.fully_implemented);
        let Some(Effect::Modal { modes, min, max, allow_repeat }) = def.spell_effect() else {
            panic!("modal");
        };
        assert_eq!((modes.len(), *min, *max, *allow_repeat), (2, 0, 4, true));
    }

    /// An agent that chooses modes `modes` and answers ChooseTargets slot i with `picks[i]`.
    struct ReckonAgent {
        modes: Vec<u32>,
        picks: Vec<ObjId>,
    }
    impl Agent for ReckonAgent {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::ChooseModes { .. } => DecisionResponse::Indices(self.modes.clone()),
                DecisionRequest::ChooseTargets { slots, .. } => {
                    let pairs = slots
                        .iter()
                        .enumerate()
                        .map(|(si, slot)| {
                            let want = self.picks.get(si).copied();
                            let idx = want
                                .and_then(|w| slot.legal.iter().position(|t| *t == Target::Object(w)))
                                .unwrap_or(0);
                            (si as u32, idx as u32)
                        })
                        .collect();
                    DecisionResponse::Pairs(pairs)
                }
                _ => DecisionResponse::Pass,
            }
        }
    }

    /// Real cast choosing both modes once: destroy an opponent's creature AND return one of your own
    /// nonland permanent cards from the graveyard to the battlefield.
    #[test]
    fn destroys_and_reanimates_via_two_modes() {
        let mut state = build_game(1, &[&[], &[]]);
        let mor = state.add_card(
            PlayerId(0),
            state.card_db().get(MOMENT_OF_RECKONING).unwrap().chars.clone(),
            Zone::Hand,
        );
        // {3}{W}{W}{B}{B} = 7 mana: 2 Plains + 2 Swamps + 3 more (Plains).
        for grp_id in [grp::PLAINS, grp::PLAINS, grp::PLAINS, grp::SWAMP, grp::SWAMP, grp::PLAINS, grp::SWAMP] {
            let c = state.card_db().get(grp_id).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield);
        }
        // P1 has a creature to destroy; P0 has a creature card in the graveyard to return.
        let victim = {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            state.add_card(PlayerId(1), c, Zone::Battlefield)
        };
        let gy_creature = {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Graveyard)
        };
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;

        let mut e = Engine::new(
            state,
            vec![
                Box::new(ReckonAgent { modes: vec![0, 1], picks: vec![victim, gy_creature] }),
                Box::new(RandomAgent::new(1)),
            ],
        );
        e.cast_spell(PlayerId(0), mor, CastVariant::Normal);
        e.resolve_top();

        assert!(!e.state.player(PlayerId(1)).battlefield.contains(&victim), "mode 0: opponent's creature destroyed");
        assert!(e.state.player(PlayerId(0)).battlefield.contains(&gy_creature), "mode 1: graveyard creature reanimated");
        assert!(!e.state.player(PlayerId(0)).graveyard.contains(&gy_creature), "…left the graveyard");
    }
}
