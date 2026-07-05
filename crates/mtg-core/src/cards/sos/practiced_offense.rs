//! Practiced Offense — `{2}{W}` Sorcery.
//!
//! Oracle: "Put a +1/+1 counter on each creature target player controls. Target creature gains your
//! choice of double strike or lifelink until end of turn.
//! Flashback {1}{W}"
//!
//! **Fully implemented** — `Sequence[ TargetPlayer, ForEach(counters), GrantChosenKeyword ]` + flashback:
//! - **"a +1/+1 counter on each creature target player controls"** — `TargetPlayer(Any)` (slot 0) then a
//!   `ForEach` whose selector is scoped to that player (`chooser: ChosenTarget(0)` → their battlefield),
//!   putting one +1/+1 counter on each of their creatures.
//! - **"target creature gains your choice of double strike or lifelink until end of turn"** — the new
//!   `Effect::GrantChosenKeyword{ options: [DoubleStrike, Lifelink] }` on a second target creature
//!   (slot 1); the controller picks one keyword at resolution.
//! - **Flashback {1}{W}** via the shared `cards::flashback` helper.

use crate::basics::{CardType, Color, CounterKind, Zone};
use crate::cards::{flashback, mana_cost, spell, CardDb};
use crate::effects::ability::Keyword;
use crate::effects::condition::Duration;
use crate::effects::target::{CardFilter, PlayerFilter, SelectSpec, TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};

/// grp id (per-set ids live near their cards).
pub const PRACTICED_OFFENSE: u32 = 435;

pub fn register(db: &mut CardDb) {
    let effect = Effect::Sequence(vec![
        // "target player" — slot 0; referenced by the ForEach's player scope.
        Effect::TargetPlayer(PlayerFilter::Any),
        // "Put a +1/+1 counter on each creature target player controls."
        Effect::ForEach {
            selector: SelectSpec {
                zone: Zone::Battlefield,
                filter: CardFilter::HasCardType(CardType::Creature),
                chooser: PlayerRef::ChosenTarget(0),
                min: ValueExpr::Fixed(0),
                max: ValueExpr::Fixed(999),
            },
            body: Box::new(Effect::PutCounters {
                what: EffectTarget::Each,
                kind: CounterKind::PlusOnePlusOne,
                n: ValueExpr::Fixed(1),
            }),
        },
        // "Target creature gains your choice of double strike or lifelink until end of turn." — slot 1.
        Effect::GrantChosenKeyword {
            what: EffectTarget::Target(TargetSpec {
                kind: TargetKind::Creature(CardFilter::Any),
                min: 1,
                max: 1,
                distinct: true,
            }),
            options: vec![Keyword::DoubleStrike, Keyword::Lifelink],
            duration: Duration::UntilEndOfTurn,
        },
    ]);
    let mut def = spell(
        PRACTICED_OFFENSE,
        "Practiced Offense",
        CardType::Sorcery,
        Color::White,
        mana_cost(2, &[(Color::White, 1)]),
        effect,
    )
    .with_text("Put a +1/+1 counter on each creature target player controls. Target creature gains your choice of double strike or lifelink until end of turn.\nFlashback {1}{W}");
    def.abilities.push(flashback(mana_cost(1, &[(Color::White, 1)])));
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, CastVariant, DecisionRequest, DecisionResponse, PlayerView};
    use crate::basics::{Phase, Target, Zone};
    use crate::cards::{grp, starter_db};
    use crate::ids::{ObjId, PlayerId};
    use crate::priority::Engine;
    use crate::state::GameState;
    use std::sync::Arc;

    fn db_with_card() -> CardDb {
        let mut db = starter_db();
        register(&mut db);
        db
    }

    #[test]
    fn practiced_offense_shape() {
        let db = db_with_card();
        let def = db.get(PRACTICED_OFFENSE).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Sorcery]);
        assert_eq!(def.chars.colors, vec![Color::White]);
        assert!(def.fully_implemented);
        assert!(def.abilities.iter().any(|a| matches!(a, crate::effects::ability::Ability::Flashback { .. })));
    }

    /// Targets P0 (self) for the counters and the named creature for the keyword; picks keyword `kw_idx`.
    #[derive(Clone)]
    struct PoAgent {
        keyword_creature: ObjId,
        kw_idx: u32,
    }
    impl Agent for PoAgent {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::ChooseModes { .. } => DecisionResponse::Indices(vec![self.kw_idx]),
                DecisionRequest::ChooseTargets { slots, .. } => {
                    // Slot 0 = target player (pick P0); slot 1 = target creature (the named one).
                    let pairs: Vec<(u32, u32)> = slots
                        .iter()
                        .enumerate()
                        .map(|(i, slot)| {
                            let want = if slot.legal.iter().any(|t| matches!(t, Target::Player(_))) {
                                slot.legal.iter().position(|t| *t == Target::Player(PlayerId(0)))
                            } else {
                                slot.legal.iter().position(|t| *t == Target::Object(self.keyword_creature))
                            };
                            (i as u32, want.unwrap_or(0) as u32)
                        })
                        .collect();
                    DecisionResponse::Pairs(pairs)
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

    /// P0 controls two creatures; casts Practiced Offense targeting itself (both get a +1/+1 counter)
    /// and grants one of them double strike (kw_idx 0).
    #[test]
    fn counters_each_creature_and_grants_chosen_keyword() {
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(db_with_card()));
        let po = {
            let c = state.card_db().get(PRACTICED_OFFENSE).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Hand)
        };
        let a = {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        let b = {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        for g in [grp::PLAINS, grp::PLAINS, grp::PLAINS] {
            let c = state.card_db().get(g).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield); // {2}{W}
        }
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let mut e = Engine::new(
            state,
            vec![
                Box::new(PoAgent { keyword_creature: a, kw_idx: 0 }),
                Box::new(PoAgent { keyword_creature: a, kw_idx: 0 }),
            ],
        );
        e.cast_spell(PlayerId(0), po, CastVariant::Normal);
        drive(&mut e);

        assert_eq!(e.state.object(a).counters.get(&CounterKind::PlusOnePlusOne), 1, "creature a got a counter");
        assert_eq!(e.state.object(b).counters.get(&CounterKind::PlusOnePlusOne), 1, "creature b got a counter");
        assert!(
            e.state.computed(a).keywords.contains(&Keyword::DoubleStrike),
            "creature a gained double strike (kw_idx 0)"
        );
        assert!(
            !e.state.computed(b).keywords.contains(&Keyword::DoubleStrike),
            "creature b did NOT get the keyword"
        );
    }

    /// Picking kw_idx 1 grants lifelink instead.
    #[test]
    fn grants_lifelink_when_chosen() {
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(db_with_card()));
        let po = {
            let c = state.card_db().get(PRACTICED_OFFENSE).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Hand)
        };
        let a = {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        for g in [grp::PLAINS, grp::PLAINS, grp::PLAINS] {
            let c = state.card_db().get(g).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield);
        }
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let mut e = Engine::new(
            state,
            vec![
                Box::new(PoAgent { keyword_creature: a, kw_idx: 1 }),
                Box::new(PoAgent { keyword_creature: a, kw_idx: 1 }),
            ],
        );
        e.cast_spell(PlayerId(0), po, CastVariant::Normal);
        drive(&mut e);
        assert!(e.state.computed(a).keywords.contains(&Keyword::Lifelink), "gained lifelink (kw_idx 1)");
        assert!(!e.state.computed(a).keywords.contains(&Keyword::DoubleStrike), "not double strike");
    }
}
