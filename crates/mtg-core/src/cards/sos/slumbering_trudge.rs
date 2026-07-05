//! Slumbering Trudge — `{X}{G}` Creature — Plant Beast 6/6 (first printed SOS).
//!
//! Oracle: "This creature enters with a number of stun counters on it equal to three minus X. If X is
//! 2 or less, it enters tapped. (If a permanent with a stun counter would become untapped, remove one
//! from it instead.)"
//!
//! **Fully implemented** — a 6/6 whose entry is throttled by how cheaply you cast it, via two ETB
//! self-replacements (CR 614.1e / 614.12, scoped to `ItSelf`):
//!  - `EntersWithCountersValue{ Stun, 3 - X }` — `Sum(3, XTimes(-1))`, clamped at 0 (X ≥ 3 → none);
//!  - `EntersTappedUnless(ValueAtLeast(X, 3))` — enters tapped iff X ≤ 2 (the X-threading fix to the
//!    enters-tapped-unless rewrite reads the creature's own cast X).
//! The stun counters then hold it tapped for that many untap steps (the existing CR 702.171 untap-skip
//! replacement removes one instead of untapping).

use crate::basics::{Color, CounterKind};
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, ActionPattern, Rewrite};
use crate::effects::condition::Condition;
use crate::effects::target::CardFilter;
use crate::effects::value::ValueExpr;
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const SLUMBERING_TRUDGE: u32 = 447;

pub fn register(db: &mut CardDb) {
    let mut mc = mana_cost(0, &[(Color::Green, 1)]);
    mc.x = 1; // `{X}{G}` — one `{X}` pip (CR 107.3), announced at cast.
    let mut def = creature(
        SLUMBERING_TRUDGE,
        "Slumbering Trudge",
        &[CreatureType::Plant, CreatureType::Beast],
        Color::Green,
        mc,
        6,
        6,
        vec![
            // "enters with a number of stun counters on it equal to three minus X" (0 if X ≥ 3).
            Ability::Replacement {
                pattern: ActionPattern::WouldEnterBattlefield(CardFilter::ItSelf),
                rewrite: Rewrite::EntersWithCountersValue {
                    kind: CounterKind::Stun,
                    n: ValueExpr::Sum(
                        Box::new(ValueExpr::Fixed(3)),
                        Box::new(ValueExpr::XTimes(-1)),
                    ),
                },
            },
            // "If X is 2 or less, it enters tapped." = enters tapped UNLESS X ≥ 3.
            Ability::Replacement {
                pattern: ActionPattern::WouldEnterBattlefield(CardFilter::ItSelf),
                rewrite: Rewrite::EntersTappedUnless(Condition::ValueAtLeast(
                    ValueExpr::X,
                    ValueExpr::Fixed(3),
                )),
            },
        ],
    );
    def.fully_implemented = true;
    def.text = "This creature enters with a number of stun counters on it equal to three minus X. If X is 2 or less, it enters tapped. (If a permanent with a stun counter would become untapped, remove one from it instead.)".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, CastVariant, DecisionRequest, DecisionResponse, PlayerView};
    use crate::basics::{Phase, Zone};
    use crate::cards::{build_game, grp};
    use crate::ids::{ObjId, PlayerId};
    use crate::priority::Engine;

    #[test]
    fn slumbering_trudge_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(SLUMBERING_TRUDGE).unwrap();
        assert_eq!((def.chars.power, def.chars.toughness), (Some(6), Some(6)));
        assert_eq!(def.chars.mana_cost.as_ref().unwrap().x, 1, "one {{X}} pip");
        assert!(matches!(
            &def.abilities[0],
            Ability::Replacement { rewrite: Rewrite::EntersWithCountersValue { kind: CounterKind::Stun, .. }, .. }
        ));
        assert!(matches!(
            &def.abilities[1],
            Ability::Replacement { rewrite: Rewrite::EntersTappedUnless(_), .. }
        ));
        assert!(def.fully_implemented);
    }

    /// Answers ChooseX with a fixed value; passes otherwise.
    struct XAgent(i64);
    impl Agent for XAgent {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::ChooseNumber { .. } => DecisionResponse::Number(self.0),
                _ => DecisionResponse::Pass,
            }
        }
    }

    /// Cast for X and read the enter: `3 - X` stun counters (clamped at 0), tapped iff X ≤ 2.
    fn cast_with_x(x: i64) -> (Engine, ObjId) {
        let mut state = build_game(1, &[&[], &[]]);
        let trudge = state.add_card(PlayerId(0), state.card_db().get(SLUMBERING_TRUDGE).unwrap().chars.clone(), Zone::Hand);
        // {X}{G}: X + 1 Forests, untapped.
        for _ in 0..(x + 1) {
            state.add_card(PlayerId(0), state.card_db().get(grp::FOREST).unwrap().chars.clone(), Zone::Battlefield);
        }
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let mut e = Engine::new(state, vec![Box::new(XAgent(x)), Box::new(XAgent(x))]);
        e.cast_spell(PlayerId(0), trudge, CastVariant::Normal);
        e.resolve_top();
        e.run_agenda();
        (e, trudge)
    }

    #[test]
    fn enters_stunned_and_tapped_when_cast_cheaply() {
        // X = 1 → 3 - 1 = 2 stun counters, and X ≤ 2 → enters tapped.
        let (e, trudge) = cast_with_x(1);
        assert_eq!(e.state.object(trudge).zone, Zone::Battlefield);
        assert_eq!(e.state.object(trudge).counters.get(&CounterKind::Stun), 2, "3 - 1 = 2 stun counters");
        assert!(e.state.object(trudge).status.tapped, "X ≤ 2 → enters tapped");
    }

    #[test]
    fn enters_free_when_cast_big() {
        // X = 3 → 3 - 3 = 0 stun counters, and X ≥ 3 → enters untapped, ready to go.
        let (e, trudge) = cast_with_x(3);
        assert_eq!(e.state.object(trudge).counters.get(&CounterKind::Stun), 0, "no stun counters");
        assert!(!e.state.object(trudge).status.tapped, "X ≥ 3 → enters untapped");
    }

    /// The stun counters hold it tapped: an untap step removes one instead of untapping (CR 702.171).
    #[test]
    fn stun_counters_delay_untapping() {
        // X = 2 → 1 stun counter, tapped.
        let (mut e, trudge) = cast_with_x(2);
        assert_eq!(e.state.object(trudge).counters.get(&CounterKind::Stun), 1);
        assert!(e.state.object(trudge).status.tapped);
        // Untap the controller's permanents (as at the untap step): the stun counter is removed and it
        // stays tapped this time.
        e.run_step(Phase::Untap);
        assert_eq!(e.state.object(trudge).counters.get(&CounterKind::Stun), 0, "one stun counter removed");
        assert!(e.state.object(trudge).status.tapped, "still tapped this untap step");
        // Next untap step: no stun counters left, so it untaps normally.
        e.run_step(Phase::Untap);
        assert!(!e.state.object(trudge).status.tapped, "now it untaps");
    }
}
