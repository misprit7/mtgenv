//! Nita, Forum Conciliator — `{1}{W}{B}` Legendary Creature — Human Advisor (2/3).
//!
//! Oracle:
//!   Whenever you cast a spell you don't own, put a +1/+1 counter on each creature you control.
//!   {2}, Sacrifice another creature: Exile target instant or sorcery card from an opponent's graveyard.
//!   You may cast it this turn, and mana of any type can be spent to cast that spell. If that spell would
//!   be put into a graveyard, exile it instead. Activate only as a sorcery.
//!
//! **Fully implemented.**
//! - **Ability 1.** "Whenever you cast a spell you don't own" is a `SpellCast` trigger filtered by
//!   [`CardFilter::OwnedBy`] (`Not(OwnedBy(Controller))` = a spell whose owner isn't you; the trigger
//!   already gates on you being the caster). Effect = `ForEach` creature you control →
//!   `PutCounters(+1/+1)`.
//! - **Ability 2.** Cost ({2} + sacrifice another creature) + exile a target instant/sorcery from an
//!   opponent's graveyard, then the full rider via [`Effect::ExileTargetThenMayCast`]: (a) a
//!   **cross-player** exile-cast permission (`Object.castable_by = the controller` + the offer-loop scan
//!   of other players' exile), (b) **spend-any-type-of-mana** (`Object.spend_any_mana` collapses the
//!   cast's coloured pips to generic at both the offer gate and `cast_spell`'s payment), and (c)
//!   **exile-on-leave-stack** (`Object.exile_on_leave` → `flashback_cast` on the resulting spell, so it's
//!   exiled instead of graveyard'd when it leaves the stack — on resolve OR counter, CR 702.34d).

use crate::basics::{CardType, Color, CounterKind, Zone};
use crate::cards::{creature, mana_cost, CardDb};
use crate::cards::helpers::instant_or_sorcery;
use crate::effects::ability::{Ability, Cost, CostComponent, EventPattern, Timing};
use crate::effects::target::{CardFilter, SelectSpec, TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};
use crate::subtypes::{CreatureType, Supertype};

/// grp id (per-set ids live near their cards).
pub const NITA_FORUM_CONCILIATOR: u32 = 461;

/// "Whenever you cast a spell you don't own, put a +1/+1 counter on each creature you control."
fn cast_a_spell_you_dont_own() -> Ability {
    Ability::Triggered {
        event: EventPattern::SpellCast(CardFilter::Not(Box::new(CardFilter::OwnedBy(
            PlayerRef::Controller,
        )))),
        condition: None,
        intervening_if: false,
        effect: Effect::ForEach {
            selector: SelectSpec {
                zone: Zone::Battlefield,
                filter: CardFilter::All(vec![
                    CardFilter::HasCardType(CardType::Creature),
                    CardFilter::ControlledBy(PlayerRef::Controller),
                ]),
                chooser: PlayerRef::Controller,
                min: ValueExpr::Fixed(0),
                max: ValueExpr::Fixed(999),
            },
            body: Box::new(Effect::PutCounters {
                what: EffectTarget::Each,
                kind: CounterKind::PlusOnePlusOne,
                n: ValueExpr::Fixed(1),
            }),
        },
    }
}

/// "{2}, Sacrifice another creature: Exile target I/S card from an opponent's graveyard. You may cast
/// it this turn, and mana of any type can be spent to cast that spell. If that spell would be put into a
/// graveyard, exile it instead. Activate only as a sorcery." (See the module docs.)
fn exile_opp_gy_is() -> Ability {
    Ability::Activated {
        cost: Cost {
            mana: Some(mana_cost(2, &[])),
            components: vec![CostComponent::Sacrifice(SelectSpec {
                zone: Zone::Battlefield,
                filter: CardFilter::All(vec![
                    CardFilter::HasCardType(CardType::Creature),
                    CardFilter::Not(Box::new(CardFilter::ItSelf)),
                ]),
                chooser: PlayerRef::Controller,
                min: ValueExpr::Fixed(1),
                max: ValueExpr::Fixed(1),
            })],
        },
        effect: Effect::ExileTargetThenMayCast {
            what: EffectTarget::Target(TargetSpec {
                kind: TargetKind::CardInZone {
                    zone: Zone::Graveyard,
                    filter: CardFilter::All(vec![
                        instant_or_sorcery(),
                        CardFilter::ControlledBy(PlayerRef::Opponent),
                    ]),
                },
                min: 1,
                max: 1,
                distinct: true,
            }),
            any_mana: true,       // "mana of any type can be spent to cast that spell"
            exile_on_leave: true, // "if that spell would be put into a graveyard, exile it instead"
        },
        timing: Timing::Sorcery,
        restriction: None,
        is_mana: false,
    }
}

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        NITA_FORUM_CONCILIATOR,
        "Nita, Forum Conciliator",
        &[CreatureType::Human, CreatureType::Advisor],
        Color::White,
        mana_cost(1, &[(Color::White, 1), (Color::Black, 1)]),
        2,
        3,
        vec![cast_a_spell_you_dont_own(), exile_opp_gy_is()],
    );
    def.chars.supertypes = vec![Supertype::Legendary];
    def.chars.colors = vec![Color::White, Color::Black];
    def.text = "Whenever you cast a spell you don't own, put a +1/+1 counter on each creature you control.\n{2}, Sacrifice another creature: Exile target instant or sorcery card from an opponent's graveyard. You may cast it this turn, and mana of any type can be spent to cast that spell. If that spell would be put into a graveyard, exile it instead. Activate only as a sorcery.".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, CastVariant, DecisionRequest, DecisionResponse, PlayerView, RandomAgent};
    use crate::cards::{grp, starter_db};
    use crate::ids::PlayerId;
    use crate::priority::Engine;
    use crate::state::GameState;
    use std::sync::Arc;

    fn db_with_card() -> CardDb {
        let mut db = starter_db();
        register(&mut db);
        db
    }

    #[test]
    fn nita_shape() {
        let db = db_with_card();
        let def = db.get(NITA_FORUM_CONCILIATOR).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Creature]);
        assert_eq!(def.chars.colors, vec![Color::White, Color::Black]);
        assert_eq!((def.chars.power, def.chars.toughness), (Some(2), Some(3)));
        assert_eq!(def.chars.supertypes, vec![Supertype::Legendary]);
        assert!(def.fully_implemented, "ability 2's full rider is now implemented");
        assert!(matches!(
            def.abilities[0],
            Ability::Triggered { event: EventPattern::SpellCast(_), .. }
        ));
        assert!(matches!(
            def.abilities[1],
            Ability::Activated { effect: Effect::ExileTargetThenMayCast { any_mana: true, exile_on_leave: true, .. }, .. }
        ));
    }

    /// Targets P1 with any target; passes otherwise.
    struct P1Agent;
    impl Agent for P1Agent {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::ChooseTargets { slots, .. } => {
                    let idx = slots[0]
                        .legal
                        .iter()
                        .position(|t| matches!(t, crate::basics::Target::Player(PlayerId(1))))
                        .unwrap_or(0);
                    DecisionResponse::Pairs(vec![(0, idx as u32)])
                }
                _ => DecisionResponse::Pass,
            }
        }
    }

    /// Ability 1 (fully faithful): casting a spell you DON'T own puts a +1/+1 counter on each creature you
    /// control. Model "a spell you don't own" by a Lightning Bolt whose owner is P1 but cast by P0.
    #[test]
    fn casting_a_spell_you_dont_own_pumps_your_team() {
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(db_with_card()));
        let nita = {
            let c = state.card_db().get(NITA_FORUM_CONCILIATOR).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        // Another creature P0 controls (a Grizzly Bears), to see the counter land on it too.
        let bears = {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        // A Lightning Bolt OWNED by P1 but placed in P0's hand, and a Mountain to pay for it. P0 casting
        // it = "casting a spell you don't own."
        let bolt = {
            let c = state.card_db().get(grp::LIGHTNING_BOLT).unwrap().chars.clone();
            let id = state.add_card(PlayerId(1), c, Zone::Hand); // owner P1
            state.objects.get_mut(&id).unwrap().controller = PlayerId(0);
            // Move it into P0's hand list so P0 can cast it.
            state.players[1].hand.retain(|&o| o != id);
            state.players[0].hand.push(id);
            state.objects.get_mut(&id).unwrap().zone = Zone::Hand;
            id
        };
        {
            let m = state.card_db().get(grp::MOUNTAIN).unwrap().chars.clone();
            state.add_card(PlayerId(0), m, Zone::Battlefield);
        }
        state.active_player = PlayerId(0);
        state.phase = crate::basics::Phase::PrecombatMain;
        let mut e = Engine::new(state, vec![Box::new(P1Agent), Box::new(RandomAgent::new(1))]);

        assert_eq!(e.state.computed(bears).power, Some(2), "2/2 before");
        e.cast_spell(PlayerId(0), bolt, CastVariant::Normal);
        // Run the agenda so the "cast a spell you don't own" trigger goes on the stack, then resolve it.
        e.run_agenda();
        while !e.state.stack.items.is_empty() {
            e.resolve_top();
            e.run_agenda();
        }
        // Nita's trigger put a +1/+1 counter on each creature P0 controls (Nita + the Bears).
        assert_eq!(e.state.computed(bears).power, Some(3), "Bears got a +1/+1 counter");
        assert_eq!(e.state.computed(nita).power, Some(3), "Nita got one too (2/3 → 3/4)");
    }

    /// Picks the bolt in the opponent's graveyard for Nita's target, and player P1 for anything cast at
    /// "any target" (the bolt). Passes otherwise.
    struct RiderAgent;
    impl Agent for RiderAgent {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::ChooseTargets { slots, .. } => {
                    // Prefer targeting P1 (for the bolt); else the first legal candidate (Nita's
                    // graveyard-card target has just one — the bolt).
                    let idx = slots[0]
                        .legal
                        .iter()
                        .position(|t| matches!(t, crate::basics::Target::Player(PlayerId(1))))
                        .unwrap_or(0);
                    DecisionResponse::Pairs(vec![(0, idx as u32)])
                }
                _ => DecisionResponse::Pass,
            }
        }
    }

    /// The full ability-2 rider (clears the tracked-partial): `{2}, Sac another creature` exiles a
    /// target instant from the OPPONENT's graveyard, then P0 casts it THIS TURN from P1's exile, paying
    /// its `{R}` with GREEN mana (mana of any type), and it's EXILED (not put in a graveyard) as it
    /// leaves the stack.
    #[test]
    fn ability2_cross_player_any_mana_exile_on_leave() {
        use crate::agent::{AbilityRef, PlayableAction};
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(db_with_card()));
        // P0: Nita + one other creature (auto-sacrificed) + three Forests (green only).
        let nita = {
            let c = state.card_db().get(NITA_FORUM_CONCILIATOR).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield);
        }
        for _ in 0..3 {
            let f = state.card_db().get(grp::FOREST).unwrap().chars.clone();
            state.add_card(PlayerId(0), f, Zone::Battlefield);
        }
        // P1 owns a Lightning Bolt ({R}) in their graveyard — the cross-player target.
        let bolt = {
            let c = state.card_db().get(grp::LIGHTNING_BOLT).unwrap().chars.clone();
            state.add_card(PlayerId(1), c, Zone::Graveyard)
        };
        state.active_player = PlayerId(0);
        state.phase = crate::basics::Phase::PrecombatMain;
        let p1_life = state.player(PlayerId(1)).life;
        let mut e = Engine::new(state, vec![Box::new(RiderAgent), Box::new(RiderAgent)]);

        // Activate ability 2 (index 1): pays {2} (two Forests) + auto-sacs the Bears, targeting the bolt.
        e.activate_ability(PlayerId(0), nita, AbilityRef(1));
        e.resolve_top();
        e.run_agenda();

        // The bolt is now in P1's EXILE, castable by P0 this turn, any-mana + exile-on-leave.
        let bo = e.state.object(bolt);
        assert_eq!(bo.zone, Zone::Exile, "the bolt was exiled from P1's graveyard");
        assert!(bo.castable_from_exile && bo.castable_by == Some(PlayerId(0)), "P0 may cast it");
        assert!(bo.spend_any_mana && bo.exile_on_leave, "any-mana + exile-on-leave riders set");
        assert!(
            e.state.player(PlayerId(1)).exile.contains(&bolt),
            "it sits in the OWNER's (P1's) exile"
        );

        // P0 is offered the cast (cross-player exile scan + any-mana affordability with only green mana).
        let offered = e
            .legal_actions(PlayerId(0))
            .iter()
            .any(|a| matches!(a, PlayableAction::Cast { spell, .. } if *spell == bolt));
        assert!(offered, "P0 is offered the cast — {{R}} payable by green via 'any type of mana'");

        // Cast it (paid {R} with the last Forest via the any-mana collapse), targeting P1.
        e.cast_spell(PlayerId(0), bolt, CastVariant::Normal);
        e.run_agenda();
        while !e.state.stack.items.is_empty() {
            e.resolve_top();
            e.run_agenda();
        }
        assert_eq!(
            e.state.player(PlayerId(1)).life,
            p1_life - 3,
            "the borrowed bolt dealt 3 to P1 — it was cast and paid with green mana"
        );
        // Exile-on-leave: the spell was exiled (not put into a graveyard) as it left the stack.
        assert_eq!(e.state.object(bolt).zone, Zone::Exile, "exiled instead of graveyard'd (CR 702.34d)");
        assert!(!e.state.player(PlayerId(1)).graveyard.contains(&bolt), "not in the graveyard");
    }
}
