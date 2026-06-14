//! Mossborn Hydra — `{2}{G}` Creature — Elemental Hydra 0/0 (first printed FDN, Foundations).
//!
//! "Trample. This creature enters with a +1/+1 counter on it. Landfall — Whenever a land you
//! control enters, double the number of +1/+1 counters on this creature."
//!
//! Fully implemented:
//! - Trample (keyword).
//! - Enters with a +1/+1 counter: a self-replacement (CR 614.12 via `ItSelf`) — without it the
//!   0/0 would die to the toughness-0 SBA. Its P/T comes from counters via the normal layer 7c
//!   (no CDA needed).
//! - Landfall double: a triggered ability (C4) that puts `CountersOnSelf(+1/+1)` *more* counters
//!   on itself (C9b) — adding the current count doubles it.

use crate::basics::{Color, CounterKind};
use crate::cards::helpers::land_you_control;
use crate::cards::{creature, mana_cost, CardDb};
use crate::subtypes::CreatureType;
use crate::effects::ability::{Ability, ActionPattern, EventPattern, Keyword, Rewrite};
use crate::effects::target::CardFilter;
use crate::effects::value::ValueExpr;
use crate::effects::{Effect, EffectTarget};

/// grp id (per-set ids live near their cards).
pub const MOSSBORN_HYDRA: u32 = 103;

pub fn register(db: &mut CardDb) {
    let mut hydra = creature(
        MOSSBORN_HYDRA,
        "Mossborn Hydra",
        &[CreatureType::Elemental, CreatureType::Hydra],
        Color::Green,
        mana_cost(2, &[(Color::Green, 1)]),
        0,
        0,
        vec![
            // Enters with a +1/+1 counter (self-replacement, CR 614.12 scoped to `ItSelf`).
            Ability::Replacement {
                pattern: ActionPattern::WouldEnterBattlefield(CardFilter::ItSelf),
                rewrite: Rewrite::EntersWithCounters {
                    kind: CounterKind::PlusOnePlusOne,
                    n: 1,
                },
            },
            // Landfall — "double the +1/+1 counters" = add (current count) more of them.
            Ability::Triggered {
                event: EventPattern::PermanentEnters(land_you_control()),
                condition: None,
                intervening_if: false,
                effect: Effect::PutCounters {
                    what: EffectTarget::SourceSelf,
                    kind: CounterKind::PlusOnePlusOne,
                    n: ValueExpr::CountersOnSelf(CounterKind::PlusOnePlusOne),
                },
            },
        ],
    );
    hydra.chars.keywords = vec![Keyword::Trample];
    db.insert(hydra.with_text(
        "Trample\nThis creature enters with a +1/+1 counter on it.\nLandfall — Whenever a land you control enters, double the number of +1/+1 counters on this creature.",
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn mossborn_hydra_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(MOSSBORN_HYDRA).unwrap();
        assert_eq!(def.chars.power, Some(0));
        assert_eq!(def.chars.toughness, Some(0));
        assert_eq!(def.chars.subtypes, vec![CreatureType::Elemental.into(), CreatureType::Hydra.into()]);
        assert_eq!(def.chars.keywords, vec![Keyword::Trample]);
        assert!(!def.is_mana_source());
        expect![[r#"
            [
                Replacement {
                    pattern: WouldEnterBattlefield(
                        ItSelf,
                    ),
                    rewrite: EntersWithCounters {
                        kind: PlusOnePlusOne,
                        n: 1,
                    },
                },
                Triggered {
                    event: PermanentEnters(
                        All(
                            [
                                HasCardType(
                                    Land,
                                ),
                                ControlledBy(
                                    Controller,
                                ),
                            ],
                        ),
                    ),
                    condition: None,
                    intervening_if: false,
                    effect: PutCounters {
                        what: SourceSelf,
                        kind: PlusOnePlusOne,
                        n: CountersOnSelf(
                            PlusOnePlusOne,
                        ),
                    },
                },
            ]"#]].assert_eq(&format!("{:#?}", def.abilities));
    }

    /// Behaviour: the landfall trigger "doubles the +1/+1 counters" (a `CountersOnSelf` value) — seed
    /// a 0/0 Hydra with two counters (a 2/2), resolve landfall → four counters (a 4/4).
    #[test]
    fn mossborn_hydra_landfall_doubles_its_counters() {
        use crate::agent::RandomAgent;
        use crate::basics::{CounterKind, Zone};
        use crate::cards::build_game;
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::effects::value::ValueExpr;
        use crate::effects::{Effect, EffectTarget};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        let mut state = build_game(1, &[&[], &[]]);
        let chars = state.card_db().get(MOSSBORN_HYDRA).unwrap().chars.clone();
        let hydra = state.add_card(PlayerId(0), chars, Zone::Battlefield);
        let double = match &state.card_db().get(MOSSBORN_HYDRA).unwrap().abilities[1] {
            Ability::Triggered { effect, .. } => effect.clone(),
            o => panic!("expected landfall Triggered, got {o:?}"),
        };
        let mut e = Engine::new(
            state,
            vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))],
        );
        let ctx = ResolutionCtx { controller: Some(PlayerId(0)), source: Some(hydra), ..Default::default() };
        // Seed two +1/+1 counters → a 2/2.
        e.resolve_effect(
            &Effect::PutCounters {
                what: EffectTarget::SourceSelf,
                kind: CounterKind::PlusOnePlusOne,
                n: ValueExpr::Fixed(2),
            },
            &ctx,
            WbReason::Resolve(StackId(0)),
        );
        assert_eq!(e.state.computed(hydra).power, Some(2));
        // Landfall: double the +1/+1 counters → 4.
        e.resolve_effect(&double, &ctx, WbReason::Resolve(StackId(0)));
        assert_eq!(e.state.computed(hydra).power, Some(4));
    }

    /// #60 end-to-end (REAL cast + land drops): cast Mossborn `{2}{G}` (enters with one +1/+1 counter
    /// via its ETB replacement → 1/1), then each land you play fires landfall — "double the number of
    /// +1/+1 counters on this creature" — 1 → 2 (2/2) → 4 (4/4). Drives the whole loop: `cast_spell`
    /// (real mana) → `resolve_top` (ETB counter) → `play_land` + `run_agenda`/`resolve_top` (landfall).
    #[test]
    fn mossborn_landfall_doubles_via_real_play() {
        use crate::agent::{Agent, CastVariant, DecisionRequest, DecisionResponse, PlayerView};
        use crate::basics::{CounterKind, Zone};
        use crate::cards::{grp, starter_db};
        use crate::ids::PlayerId;
        use crate::priority::Engine;
        use crate::state::GameState;
        use std::sync::Arc;

        #[derive(Clone)]
        struct PassiveAgent;
        impl Agent for PassiveAgent {
            fn decide(&mut self, _v: &PlayerView, _req: &DecisionRequest) -> DecisionResponse {
                DecisionResponse::Pass
            }
        }

        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(starter_db()));
        let hydra = {
            let c = state.card_db().get(MOSSBORN_HYDRA).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Hand)
        };
        for _ in 0..3 {
            let c = state.card_db().get(grp::FOREST).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield); // pays {2}{G}
        }
        let hand_lands: Vec<_> = (0..2)
            .map(|_| {
                let c = state.card_db().get(grp::FOREST).unwrap().chars.clone();
                state.add_card(PlayerId(0), c, Zone::Hand)
            })
            .collect();
        let mut e = Engine::new(state, vec![Box::new(PassiveAgent), Box::new(PassiveAgent)]);

        let settle = |e: &mut Engine| {
            e.run_agenda();
            while !e.state.stack.items.is_empty() {
                e.resolve_top();
                e.run_agenda();
            }
        };
        let counters = |e: &Engine| e.state.object(hydra).counters.get(&CounterKind::PlusOnePlusOne);

        e.cast_spell(PlayerId(0), hydra, CastVariant::Normal);
        e.resolve_top(); // enters → ETB replacement gives one +1/+1 counter
        settle(&mut e);
        assert_eq!(counters(&e), 1, "enters with one +1/+1 counter (→ 1/1)");
        assert_eq!(e.state.computed(hydra).power, Some(1));

        e.play_land(PlayerId(0), hand_lands[0]);
        settle(&mut e);
        assert_eq!(counters(&e), 2, "first landfall doubles 1 → 2 (→ 2/2)");

        e.play_land(PlayerId(0), hand_lands[1]);
        settle(&mut e);
        assert_eq!(counters(&e), 4, "second landfall doubles 2 → 4 (→ 4/4)");
        assert_eq!(e.state.computed(hydra).power, Some(4));
    }
}
