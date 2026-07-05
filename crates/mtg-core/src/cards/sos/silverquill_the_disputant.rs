//! Silverquill, the Disputant — `{2}{W}{B}` Legendary Creature — Elder Dragon 4/4.
//!
//! Oracle: "Flying, vigilance. Each instant and sorcery spell you cast has casualty 1. (As you cast
//! that spell, you may sacrifice a creature with power 1 or greater. When you do, copy the spell and
//! you may choose new targets for the copy.)"
//!
//! **Fully implemented** — the Silverquill (Casualty) Elder Dragon, a composition over the CR 707.10
//! copy-a-spell-on-the-stack cap (shares [`Effect::CopySpellOnStack`] with Prismari's storm):
//! - **4/4 flying, vigilance** body.
//! - **Casualty 1**, modeled as a `Triggered{ SpellCast(instant|sorcery) }` whose effect is
//!   `Optional{ IfYouDo{ Sacrifice(a creature power ≥ 1), CopySpellOnStack{ Triggering, count: 1,
//!   new targets } } }` — you may sacrifice; if you do, copy the triggering spell once (offering the
//!   707.10c reselection). The trigger resolves above the still-on-stack spell, so the copy resolves
//!   before the original (correct order).
//!
//! ⚠️ Timing caveat: real casualty is a **cast-time** additional cost (CR 601.2b) with the copy as a
//! reflexive "when you do" trigger. This cast-*trigger* model instead sacrifices + copies a beat later
//! (when the trigger resolves, above the spell). The observable result matches — the copy still
//! resolves before the original spell — but the sacrifice happens after the spell is fully on the
//! stack rather than during its cast. Acceptable for the current pool; noted for a future 601.2b cap.

use crate::basics::{CardType, Color};
use crate::cards::helpers::instant_or_sorcery;
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern, Keyword};
use crate::effects::target::{CardFilter, SelectSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};
use crate::basics::Zone;
use crate::subtypes::{CreatureType, Supertype};

/// grp id (per-set ids live near their cards).
pub const SILVERQUILL_THE_DISPUTANT: u32 = 424;

/// "a creature with power 1 or greater" — a creature whose computed power is NOT ≤ 0.
fn creature_power_1_or_more() -> CardFilter {
    CardFilter::All(vec![
        CardFilter::HasCardType(CardType::Creature),
        CardFilter::Not(Box::new(CardFilter::PowerAtMost(0))),
    ])
}

/// "Each instant and sorcery spell you cast has casualty 1" — you may sacrifice a creature with power
/// ≥ 1; when you do, copy the triggering spell once (new targets allowed) (CR 702.152).
fn casualty_1() -> Ability {
    Ability::Triggered {
        event: EventPattern::SpellCast(instant_or_sorcery()),
        condition: None,
        intervening_if: false,
        effect: Effect::Optional {
            prompt: "Sacrifice a creature with power 1 or greater to copy the spell? (Casualty 1)"
                .to_string(),
            body: Box::new(Effect::IfYouDo {
                cost: Box::new(Effect::Sacrifice {
                    who: PlayerRef::Controller,
                    what: SelectSpec {
                        zone: Zone::Battlefield,
                        filter: creature_power_1_or_more(),
                        chooser: PlayerRef::Controller,
                        min: ValueExpr::Fixed(1),
                        max: ValueExpr::Fixed(1),
                    },
                }),
                reward: Box::new(Effect::CopySpellOnStack {
                    what: EffectTarget::Triggering,
                    count: ValueExpr::Fixed(1),
                    choose_new_targets: true,
                }),
            }),
        },
    }
}

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        SILVERQUILL_THE_DISPUTANT,
        "Silverquill, the Disputant",
        &[CreatureType::Elder, CreatureType::Dragon],
        Color::White,
        mana_cost(2, &[(Color::White, 1), (Color::Black, 1)]),
        4,
        4,
        vec![casualty_1()],
    );
    def.chars.supertypes = vec![Supertype::Legendary];
    def.chars.colors = vec![Color::White, Color::Black];
    def.chars.keywords = vec![Keyword::Flying, Keyword::Vigilance];
    def.text = "Flying, vigilance\nEach instant and sorcery spell you cast has casualty 1. (As you cast that spell, you may sacrifice a creature with power 1 or greater. When you do, copy the spell and you may choose new targets for the copy.)".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, CastVariant, ConfirmKind, DecisionRequest, DecisionResponse, PlayerView};
    use crate::basics::{Phase, Target};
    use crate::cards::{grp, starter_db};
    use crate::ids::{ObjId, PlayerId};
    use crate::priority::Engine;
    use crate::state::GameState;
    use crate::stack::StackObjectKind;
    use expect_test::expect;
    use std::sync::Arc;

    fn db_with_card() -> CardDb {
        let mut db = starter_db();
        register(&mut db);
        db
    }

    #[test]
    fn silverquill_shape() {
        let db = db_with_card();
        let def = db.get(SILVERQUILL_THE_DISPUTANT).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Creature]);
        assert_eq!(def.chars.supertypes, vec![Supertype::Legendary]);
        assert_eq!(def.chars.colors, vec![Color::White, Color::Black]);
        assert_eq!(def.chars.keywords, vec![Keyword::Flying, Keyword::Vigilance]);
        assert_eq!((def.chars.power, def.chars.toughness), (Some(4), Some(4)));
        assert_eq!(def.chars.mana_value(), 4);
        assert!(def.fully_implemented);
        assert!(matches!(
            def.abilities[0],
            Ability::Triggered { event: EventPattern::SpellCast(_), .. }
        ));
    }

    #[test]
    fn casualty_ir() {
        let db = db_with_card();
        let def = db.get(SILVERQUILL_THE_DISPUTANT).unwrap();
        let Ability::Triggered { effect, .. } = &def.abilities[0] else { panic!("casualty is a trigger") };
        expect![[r#"
            Optional {
                prompt: "Sacrifice a creature with power 1 or greater to copy the spell? (Casualty 1)",
                body: IfYouDo {
                    cost: Sacrifice {
                        who: Controller,
                        what: SelectSpec {
                            zone: Battlefield,
                            filter: All(
                                [
                                    HasCardType(
                                        Creature,
                                    ),
                                    Not(
                                        PowerAtMost(
                                            0,
                                        ),
                                    ),
                                ],
                            ),
                            chooser: Controller,
                            min: Fixed(
                                1,
                            ),
                            max: Fixed(
                                1,
                            ),
                        },
                    },
                    reward: CopySpellOnStack {
                        what: Triggering,
                        count: Fixed(
                            1,
                        ),
                        choose_new_targets: true,
                    },
                },
            }"#]]
        .assert_eq(&format!("{effect:#?}"));
    }

    /// An agent that: targets P1 with the bolt, says yes to the casualty offer, sacrifices the named
    /// fodder creature, and re-aims the casualty copy at P1 too.
    #[derive(Clone)]
    struct CasualtyAgent {
        confirm_casualty: bool,
        fodder: ObjId,
    }
    impl Agent for CasualtyAgent {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::Confirm { kind: ConfirmKind::MayEffect } => {
                    DecisionResponse::Bool(self.confirm_casualty)
                }
                DecisionRequest::Confirm { .. } => DecisionResponse::Bool(true),
                // Sacrifice picks the fodder (a `SelectCards` from the battlefield).
                DecisionRequest::SelectCards { from, .. } => {
                    let idx = from.iter().position(|o| *o == self.fodder).unwrap_or(0) as u32;
                    DecisionResponse::Indices(vec![idx])
                }
                DecisionRequest::ChooseTargets { slots, .. } => {
                    let idx = slots[0]
                        .legal
                        .iter()
                        .position(|t| *t == Target::Player(PlayerId(1)))
                        .unwrap_or(0);
                    DecisionResponse::Pairs(vec![(0, idx as u32)])
                }
                _ => DecisionResponse::Pass,
            }
        }
    }

    fn drive(e: &mut Engine) {
        loop {
            e.run_agenda();
            if e.state.stack.items.is_empty() {
                break;
            }
            e.resolve_top();
        }
    }

    /// Build P0 with Silverquill out, a fodder Bears (a 2/2 with power ≥ 1) to sacrifice, a Lightning
    /// Bolt in hand, and a Mountain to cast it. Returns the engine, the bolt, and the fodder id.
    fn setup(confirm_casualty: bool) -> (Engine, ObjId, ObjId) {
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(db_with_card()));
        {
            let c = state.card_db().get(SILVERQUILL_THE_DISPUTANT).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield);
        }
        let fodder = {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        let bolt = {
            let c = state.card_db().get(grp::LIGHTNING_BOLT).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Hand)
        };
        {
            let m = state.card_db().get(grp::MOUNTAIN).unwrap().chars.clone();
            state.add_card(PlayerId(0), m, Zone::Battlefield);
        }
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let e = Engine::new(
            state,
            vec![
                Box::new(CasualtyAgent { confirm_casualty, fodder }),
                Box::new(CasualtyAgent { confirm_casualty, fodder }),
            ],
        );
        (e, bolt, fodder)
    }

    /// Casualty 1 accepted: cast a bolt at P1, sacrifice the fodder, and the spell is copied exactly
    /// ONCE — P1 takes 6 (bolt + one copy), the fodder is in the graveyard, only the real bolt too.
    #[test]
    fn casualty_makes_exactly_one_copy() {
        let (mut e, bolt, fodder) = setup(true);
        let p1_start = e.state.player(PlayerId(1)).life;
        e.cast_spell(PlayerId(0), bolt, CastVariant::Normal);
        drive(&mut e);
        assert_eq!(e.state.player(PlayerId(1)).life, p1_start - 6, "bolt + one casualty copy = 6");
        assert!(e.state.player(PlayerId(0)).graveyard.contains(&fodder), "the fodder was sacrificed");
        // Only the fodder + the real bolt hit the graveyard — the copy ceased to exist (707.10a).
        assert!(e.state.player(PlayerId(0)).graveyard.contains(&bolt), "the real bolt is in gy");
        assert_eq!(e.state.player(PlayerId(0)).graveyard.len(), 2, "fodder + real bolt only (no copy)");
    }

    /// Casualty 1 declined: no sacrifice, no copy — P1 takes just the bolt's 3, the fodder survives.
    #[test]
    fn declining_casualty_makes_no_copy() {
        let (mut e, bolt, fodder) = setup(false);
        let p1_start = e.state.player(PlayerId(1)).life;
        e.cast_spell(PlayerId(0), bolt, CastVariant::Normal);
        drive(&mut e);
        assert_eq!(e.state.player(PlayerId(1)).life, p1_start - 3, "declined: just the bolt, no copy");
        assert!(e.state.player(PlayerId(0)).battlefield.contains(&fodder), "the fodder was not sacrificed");
        assert_eq!(e.state.player(PlayerId(0)).graveyard.len(), 1, "only the real bolt in gy");
        // Sanity: never a lingering copy on the stack.
        assert!(
            !e.state.stack.items.iter().any(|s| matches!(s.kind, StackObjectKind::Spell(_))),
            "no copy left on the stack"
        );
    }
}
