//! Prismari, the Inspiration — `{5}{U}{R}` Legendary Creature — Elder Dragon 7/7.
//!
//! Oracle: "Flying. Ward—Pay 5 life. Instant and sorcery spells you cast have storm. (Whenever you
//! cast an instant or sorcery spell, copy it for each spell cast before it this turn. You may choose
//! new targets for the copies.)"
//!
//! **Fully implemented** — the Prismari (Storm) Elder Dragon, a composition over the CR 707.10
//! copy-a-spell-on-the-stack cap:
//! - **7/7 flying** vanilla body.
//! - **Ward—Pay 5 life** via [`crate::cards::helpers::ward_pay_life`] — a `BecomesTargeted`
//!   soft-counter whose cost is `CostComponent::PayLife(5)` (paid by `pay_cost`).
//! - **Storm**, modeled as a `Triggered{ SpellCast(instant|sorcery) }` on the dragon itself (rather
//!   than a static that grants the storm keyword to each spell — observably identical: the trigger
//!   resolves above the just-cast spell and copies it). Its effect is
//!   [`Effect::CopySpellOnStack`]`{ what: Triggering, count: SpellsCastThisTurn − 1,
//!   choose_new_targets: true }`. The engine bumps `spells_cast_this_turn` **before** the `SpellCast`
//!   broadcast, so at trigger resolution `SpellsCastThisTurn` already counts the triggering spell —
//!   `count = n − 1` = the spells cast *before* it this turn (exact storm count). Each copy offers the
//!   707.10c "you may choose new targets" reselection.

use crate::basics::Color;
use crate::cards::helpers::{instant_or_sorcery, ward_pay_life};
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern, Keyword};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};
use crate::subtypes::{CreatureType, Supertype};

/// grp id (per-set ids live near their cards).
pub const PRISMARI_THE_INSPIRATION: u32 = 423;

/// "Instant and sorcery spells you cast have storm" — copy the just-cast spell once per spell cast
/// before it this turn (`SpellsCastThisTurn − 1`), offering new targets for each copy (CR 702.40).
fn storm() -> Ability {
    Ability::Triggered {
        event: EventPattern::SpellCast(instant_or_sorcery()),
        condition: None,
        intervening_if: false,
        effect: Effect::CopySpellOnStack {
            what: EffectTarget::Triggering,
            // spells cast BEFORE this one = (spells cast this turn, which now includes it) − 1.
            count: ValueExpr::Sum(
                Box::new(ValueExpr::SpellsCastThisTurn { who: PlayerRef::Controller }),
                Box::new(ValueExpr::Fixed(-1)),
            ),
            choose_new_targets: true,
        },
    }
}

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        PRISMARI_THE_INSPIRATION,
        "Prismari, the Inspiration",
        &[CreatureType::Elder, CreatureType::Dragon],
        Color::Blue,
        mana_cost(5, &[(Color::Blue, 1), (Color::Red, 1)]),
        7,
        7,
        vec![ward_pay_life(5), storm()],
    );
    def.chars.supertypes = vec![Supertype::Legendary];
    def.chars.colors = vec![Color::Blue, Color::Red];
    def.chars.keywords = vec![Keyword::Flying];
    def.text = "Flying\nWard—Pay 5 life.\nInstant and sorcery spells you cast have storm. (Whenever you cast an instant or sorcery spell, copy it for each spell cast before it this turn. You may choose new targets for the copies.)".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, CastVariant, ConfirmKind, DecisionRequest, DecisionResponse, PlayerView};
    use crate::basics::{CardType, Phase, Target, Zone};
    use crate::cards::{grp, starter_db};
    use crate::ids::{ObjId, PlayerId};
    use crate::priority::Engine;
    use crate::state::GameState;
    use expect_test::expect;
    use std::sync::Arc;

    fn db_with_card() -> CardDb {
        let mut db = starter_db();
        register(&mut db);
        db
    }

    #[test]
    fn prismari_shape() {
        let db = db_with_card();
        let def = db.get(PRISMARI_THE_INSPIRATION).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Creature]);
        assert_eq!(def.chars.supertypes, vec![Supertype::Legendary]);
        assert_eq!(def.chars.colors, vec![Color::Blue, Color::Red]);
        assert_eq!(def.chars.keywords, vec![Keyword::Flying]);
        assert_eq!((def.chars.power, def.chars.toughness), (Some(7), Some(7)));
        assert_eq!(def.chars.mana_value(), 7);
        assert!(def.fully_implemented);
        // ability 0 = Ward (a BecomesTargeted trigger), ability 1 = Storm (a SpellCast trigger).
        assert!(matches!(
            def.abilities[0],
            Ability::Triggered { event: EventPattern::BecomesTargeted { .. }, .. }
        ));
        assert!(matches!(
            def.abilities[1],
            Ability::Triggered { event: EventPattern::SpellCast(_), .. }
        ));
    }

    #[test]
    fn storm_ir() {
        let db = db_with_card();
        let def = db.get(PRISMARI_THE_INSPIRATION).unwrap();
        let Ability::Triggered { effect, .. } = &def.abilities[1] else { panic!("storm is a trigger") };
        expect![[r#"
            CopySpellOnStack {
                what: Triggering,
                count: Sum(
                    SpellsCastThisTurn {
                        who: Controller,
                    },
                    Fixed(
                        -1,
                    ),
                ),
                choose_new_targets: true,
            }"#]]
        .assert_eq(&format!("{effect:#?}"));
    }

    /// Says yes to every confirm; for `ChooseTargets` always picks `Target::Player(1)` — so the
    /// original bolt and every storm copy hit the opponent.
    #[derive(Clone)]
    struct BoltP1Agent;
    impl Agent for BoltP1Agent {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::Confirm { .. } => DecisionResponse::Bool(true),
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

    /// Drive the stack to empty. `run_agenda` runs FIRST each pass so a just-cast spell's storm
    /// trigger lands on the stack (above the spell) before the spell itself resolves — otherwise the
    /// spell leaves and `copy_spell_on_stack` finds nothing to copy.
    fn drive(e: &mut Engine) {
        loop {
            e.run_agenda();
            if e.state.stack.items.is_empty() {
                break;
            }
            e.resolve_top();
        }
    }

    /// The headline (CR 702.40 storm): storm copies the just-cast spell once per spell cast *before*
    /// it this turn. With Prismari out, the 1st bolt makes 0 copies (3 dmg), the 2nd makes 1 copy
    /// (6 dmg), the 3rd makes 2 copies (9 dmg) — the escalating storm count = `SpellsCastThisTurn − 1`.
    #[test]
    fn storm_copies_scale_with_spells_cast() {
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(db_with_card()));
        // Prismari on P0's battlefield (put there, not cast — so it doesn't add to the storm count).
        {
            let c = state.card_db().get(PRISMARI_THE_INSPIRATION).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield);
        }
        let bolts: Vec<ObjId> = (0..3)
            .map(|_| {
                let c = state.card_db().get(grp::LIGHTNING_BOLT).unwrap().chars.clone();
                state.add_card(PlayerId(0), c, Zone::Hand)
            })
            .collect();
        for _ in 0..3 {
            let m = state.card_db().get(grp::MOUNTAIN).unwrap().chars.clone();
            state.add_card(PlayerId(0), m, Zone::Battlefield);
        }
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let start = state.player(PlayerId(1)).life;
        let mut e = Engine::new(state, vec![Box::new(BoltP1Agent), Box::new(BoltP1Agent)]);

        // 1st spell → storm count 0 → just the bolt (3).
        e.cast_spell(PlayerId(0), bolts[0], CastVariant::Normal);
        drive(&mut e);
        assert_eq!(e.state.player(PlayerId(1)).life, start - 3, "1st cast: 0 copies");

        // 2nd spell → storm count 1 → bolt + 1 copy (6).
        e.cast_spell(PlayerId(0), bolts[1], CastVariant::Normal);
        drive(&mut e);
        assert_eq!(e.state.player(PlayerId(1)).life, start - 9, "2nd cast: +1 copy → 6 more");

        // 3rd spell → storm count 2 → bolt + 2 copies (9).
        e.cast_spell(PlayerId(0), bolts[2], CastVariant::Normal);
        drive(&mut e);
        assert_eq!(e.state.player(PlayerId(1)).life, start - 18, "3rd cast: +2 copies → 9 more");

        // Only the three real bolts reached the graveyard — the copies ceased to exist (707.10a).
        assert_eq!(e.state.player(PlayerId(0)).graveyard.len(), 3, "3 real bolts, 0 copies, in gy");
    }

    /// A storm copy offers the 707.10c "choose new targets" reselection **per copy**: the original
    /// 2nd bolt targets P1, but its storm copy is re-aimed at P0. P0 takes the copy's 3, P1 the bolt's.
    #[derive(Clone)]
    struct RetargetAgent {
        seen_choose: std::cell::Cell<u32>,
    }
    impl Agent for RetargetAgent {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::Confirm { .. } => DecisionResponse::Bool(true),
                DecisionRequest::ChooseTargets { slots, .. } => {
                    // Both real bolt casts → P1; only the storm copy's reselection (3rd ChooseTargets)
                    // → P0, proving `choose_new_targets` re-aims the copy independently of the original.
                    let n = self.seen_choose.get();
                    self.seen_choose.set(n + 1);
                    let want = if n < 2 { PlayerId(1) } else { PlayerId(0) };
                    let idx = slots[0]
                        .legal
                        .iter()
                        .position(|t| *t == Target::Player(want))
                        .unwrap_or(0);
                    DecisionResponse::Pairs(vec![(0, idx as u32)])
                }
                _ => DecisionResponse::Pass,
            }
        }
    }

    #[test]
    fn storm_copy_takes_new_targets() {
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(db_with_card()));
        {
            let c = state.card_db().get(PRISMARI_THE_INSPIRATION).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield);
        }
        // Two bolts so the second cast (storm count 1) makes exactly one copy.
        let b0 = {
            let c = state.card_db().get(grp::LIGHTNING_BOLT).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Hand)
        };
        let b1 = {
            let c = state.card_db().get(grp::LIGHTNING_BOLT).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Hand)
        };
        for _ in 0..2 {
            let m = state.card_db().get(grp::MOUNTAIN).unwrap().chars.clone();
            state.add_card(PlayerId(0), m, Zone::Battlefield);
        }
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let p0_start = state.player(PlayerId(0)).life;
        let p1_start = state.player(PlayerId(1)).life;
        let agent = RetargetAgent { seen_choose: std::cell::Cell::new(0) };
        let mut e = Engine::new(state, vec![Box::new(agent), Box::new(BoltP1Agent)]);

        // 1st bolt at P1 (storm count 0 → no copy).
        e.cast_spell(PlayerId(0), b0, CastVariant::Normal);
        drive(&mut e);
        // 2nd bolt at P1 (storm count 1 → one copy, re-aimed at P0).
        e.cast_spell(PlayerId(0), b1, CastVariant::Normal);
        drive(&mut e);

        assert_eq!(e.state.player(PlayerId(1)).life, p1_start - 6, "P1: bolt0 + bolt1 = 6");
        assert_eq!(e.state.player(PlayerId(0)).life, p0_start - 3, "P0: the retargeted storm copy = 3");
    }

    /// An opponent's spell targeting Prismari triggers Ward—Pay 5 life: on `pay`, the targeter loses
    /// 5 life and the spell resolves; on decline, the spell is countered. `_ = ConfirmKind` documents.
    #[derive(Clone)]
    struct WardAgent {
        want: ObjId,
        pay: bool,
    }
    impl Agent for WardAgent {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::ChooseTargets { slots, .. } => {
                    let i = slots[0]
                        .legal
                        .iter()
                        .position(|t| matches!(t, Target::Object(o) if *o == self.want))
                        .unwrap_or(0);
                    DecisionResponse::Pairs(vec![(0, i as u32)])
                }
                DecisionRequest::Confirm { kind: ConfirmKind::PayToPrevent } => {
                    DecisionResponse::Bool(self.pay)
                }
                DecisionRequest::Confirm { .. } => DecisionResponse::Bool(true),
                _ => DecisionResponse::Pass,
            }
        }
    }

    /// P1 casts a Lightning Bolt at P0's Prismari; `pay` decides whether P1 pays Ward's 5 life.
    fn ward_setup(pay: bool) -> (Engine, ObjId, ObjId) {
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(db_with_card()));
        let prismari = {
            let c = state.card_db().get(PRISMARI_THE_INSPIRATION).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        let bolt = {
            let c = state.card_db().get(grp::LIGHTNING_BOLT).unwrap().chars.clone();
            state.add_card(PlayerId(1), c, Zone::Hand)
        };
        {
            let m = state.card_db().get(grp::MOUNTAIN).unwrap().chars.clone();
            state.add_card(PlayerId(1), m, Zone::Battlefield); // pays the bolt's {R}
        }
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let mut e = Engine::new(
            state,
            vec![
                Box::new(WardAgent { want: prismari, pay: false }),
                Box::new(WardAgent { want: prismari, pay }),
            ],
        );
        e.cast_spell(PlayerId(1), bolt, CastVariant::Normal); // targets Prismari → Ward triggers
        e.run_agenda();
        (e, prismari, bolt)
    }

    #[test]
    fn ward_pay_5_life_lets_spell_through() {
        let (mut e, prismari, _bolt) = ward_setup(true);
        let p1_life = e.state.player(PlayerId(1)).life;
        drive(&mut e);
        assert_eq!(e.state.player(PlayerId(1)).life, p1_life - 5, "paid Ward's 5 life");
        // The bolt resolved: 3 damage marked on the 7/7 (it survives).
        assert!(e.state.player(PlayerId(0)).battlefield.contains(&prismari), "Prismari survived the 3");
        assert_eq!(e.state.object(prismari).damage_marked, 3, "the bolt resolved for 3");
    }

    #[test]
    fn ward_counters_when_not_paid() {
        let (mut e, prismari, bolt) = ward_setup(false);
        let p1_life = e.state.player(PlayerId(1)).life;
        drive(&mut e);
        assert_eq!(e.state.player(PlayerId(1)).life, p1_life, "declined Ward → paid no life");
        assert_eq!(e.state.object(prismari).damage_marked, 0, "the bolt was countered — no damage");
        assert!(e.state.player(PlayerId(1)).graveyard.contains(&bolt), "the countered bolt is in gy");
    }
}
