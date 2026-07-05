//! Killian's Confidence — `{W}{B}` Sorcery (first printed SOS).
//!
//! Oracle: "Target creature gets +1/+1 until end of turn. Draw a card. Whenever one or more
//! creatures you control deal combat damage to a player, you may pay {W/B}. If you do, return this
//! card from your graveyard to your hand."
//!
//! **Fully implemented** — a trivial spell (pump + draw) plus a **graveyard-functioning triggered
//! ability** (the new-class capability): `Ability::FunctionsFrom(vec![Zone::Graveyard])` makes the
//! card's triggered abilities active while it sits in the graveyard (CR 113.6 — battlefield is the
//! implicit default zone-of-function; only deviating cards carry the marker). The trigger watches
//! the batched `YouDealCombatDamageToPlayer` event (once per combat-damage step) and its effect is a
//! `MayPayCost { {W/B} → return self to hand }` ("you may pay …; if you do, …").

use crate::basics::{CardType, Color, Zone, ZoneDest, ZonePos};
use crate::cards::{mana_cost, mana_cost_hybrid, spell, CardDb};
use crate::effects::ability::{Ability, Cost, EventPattern};
use crate::effects::condition::Duration;
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};

/// grp id (per-set ids live near their cards).
pub const KILLIANS_CONFIDENCE: u32 = 362;

pub fn register(db: &mut CardDb) {
    // "Target creature gets +1/+1 until end of turn. Draw a card."
    let effect = Effect::Sequence(vec![
        Effect::PumpPT {
            what: EffectTarget::Target(TargetSpec {
                kind: TargetKind::Creature(CardFilter::Any),
                min: 1,
                max: 1,
                distinct: true,
            }),
            power: ValueExpr::Fixed(1),
            toughness: ValueExpr::Fixed(1),
            duration: Duration::UntilEndOfTurn,
        },
        Effect::Draw { who: PlayerRef::Controller, count: ValueExpr::Fixed(1) },
    ]);
    let mut def = spell(
        KILLIANS_CONFIDENCE,
        "Killian's Confidence",
        CardType::Sorcery,
        Color::White,
        mana_cost(0, &[(Color::White, 1), (Color::Black, 1)]),
        effect,
    )
    .with_text(
        "Target creature gets +1/+1 until end of turn. Draw a card.\nWhenever one or more creatures you control deal combat damage to a player, you may pay {W/B}. If you do, return this card from your graveyard to your hand.",
    );
    def.chars.colors = vec![Color::White, Color::Black];
    // This card's triggered abilities function from the graveyard (CR 113.6).
    def.abilities.push(Ability::FunctionsFrom(vec![Zone::Graveyard]));
    // "Whenever one or more creatures you control deal combat damage to a player, you may pay {W/B}.
    // If you do, return this card from your graveyard to your hand."
    def.abilities.push(Ability::Triggered {
        event: EventPattern::YouDealCombatDamageToPlayer,
        condition: None,
        intervening_if: false,
        effect: Effect::MayPayCost {
            cost: Cost {
                mana: Some(mana_cost_hybrid(0, &[], &[(Color::White, Color::Black)])),
                components: vec![],
            },
            then: Box::new(Effect::MoveZone {
                what: EffectTarget::SourceSelf,
                to: ZoneDest { zone: Zone::Hand, pos: ZonePos::Any },
                tapped: false,
            }),
        },
    });
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView, RandomAgent};
    use crate::basics::{Target, Zone};
    use crate::cards::{build_game, grp};
    use crate::combat::{Attack, CombatState};
    use crate::ids::{ObjId, PlayerId};
    use crate::priority::Engine;

    #[test]
    fn killians_confidence_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(KILLIANS_CONFIDENCE).unwrap();
        assert!(def.fully_implemented);
        assert_eq!(def.chars.colors, vec![Color::White, Color::Black]);
        assert!(
            def.abilities.iter().any(|a| matches!(a, Ability::FunctionsFrom(z) if z.contains(&Zone::Graveyard))),
            "carries the graveyard-functioning marker"
        );
        let gy_trigger = def.abilities.iter().any(|a| matches!(a, Ability::Triggered {
            event: EventPattern::YouDealCombatDamageToPlayer, effect: Effect::MayPayCost { .. }, .. }));
        assert!(gy_trigger, "combat-damage graveyard trigger with a may-pay return");
    }

    /// An agent that confirms every `Confirm` (pays the optional cost) and passes otherwise.
    struct PayingAgent;
    impl Agent for PayingAgent {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::Confirm { .. } => DecisionResponse::Bool(true),
                _ => DecisionResponse::Pass,
            }
        }
    }

    fn attacker_bear(state: &mut crate::state::GameState, owner: PlayerId) -> ObjId {
        let mut c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
        c.power = Some(2);
        c.toughness = Some(2);
        let id = state.add_card(owner, c, Zone::Battlefield);
        state.objects.get_mut(&id).unwrap().summoning_sick = false;
        id
    }

    /// Real-path: Killian's sits in P0's graveyard. P0's creature deals combat damage to P1, firing
    /// the graveyard trigger; P0 pays {W/B} (from a Plains) and Killian's returns to P0's hand.
    #[test]
    fn returns_from_graveyard_on_combat_damage_when_paid() {
        let mut state = build_game(1, &[&[], &[]]);
        let bear = attacker_bear(&mut state, PlayerId(0));
        // A Plains, untapped, to pay the {W/B}.
        let plains = state.card_db().get(grp::PLAINS).unwrap().chars.clone();
        state.add_card(PlayerId(0), plains, Zone::Battlefield);
        let killians = state.add_card(
            PlayerId(0),
            state.card_db().get(KILLIANS_CONFIDENCE).unwrap().chars.clone(),
            Zone::Graveyard,
        );
        state.active_player = PlayerId(0);
        state.combat = Some(CombatState {
            attackers: vec![Attack { attacker: bear, defender: Target::Player(PlayerId(1)) }],
            blocks: vec![],
        });
        let mut e = Engine::new(state, vec![Box::new(PayingAgent), Box::new(RandomAgent::new(1))]);
        let opp_life = e.state.player(PlayerId(1)).life;

        e.combat_damage(); // P1 takes 2 → CombatDamageToPlayerBy{P0} → queues the gy trigger
        assert_eq!(e.state.player(PlayerId(1)).life, opp_life - 2, "combat damage landed");
        e.run_agenda(); // put the trigger on the stack
        e.resolve_top(); // resolve → MayPayCost: pay {W/B} → return self to hand

        assert!(e.state.player(PlayerId(0)).hand.contains(&killians), "returned to hand");
        assert!(!e.state.player(PlayerId(0)).graveyard.contains(&killians), "left the graveyard");
    }

    /// If the controller declines (or can't pay), Killian's stays in the graveyard.
    #[test]
    fn stays_in_graveyard_when_not_paid() {
        let mut state = build_game(1, &[&[], &[]]);
        let bear = attacker_bear(&mut state, PlayerId(0));
        // No mana source → the {W/B} is unpayable, so the return never happens.
        let killians = state.add_card(
            PlayerId(0),
            state.card_db().get(KILLIANS_CONFIDENCE).unwrap().chars.clone(),
            Zone::Graveyard,
        );
        state.active_player = PlayerId(0);
        state.combat = Some(CombatState {
            attackers: vec![Attack { attacker: bear, defender: Target::Player(PlayerId(1)) }],
            blocks: vec![],
        });
        let mut e = Engine::new(state, vec![Box::new(PayingAgent), Box::new(RandomAgent::new(1))]);
        e.combat_damage();
        e.run_agenda();
        e.resolve_top();
        assert!(e.state.player(PlayerId(0)).graveyard.contains(&killians), "unpaid → stays in graveyard");
    }
}
