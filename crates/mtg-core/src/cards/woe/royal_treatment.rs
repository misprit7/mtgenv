//! Royal Treatment — `{G}` Instant (first printed WOE; reprinted on the SOS Mystical Archive `soa`).
//!
//! Oracle: "Target creature you control gains hexproof until end of turn. Create a Royal Role token
//! attached to that creature. (If you control another Role on it, put that one into the graveyard.
//! Enchanted creature gets +1/+1 and has ward {1}.)"
//!
//! **Fully implemented** — the Role subsystem (`Effect::CreateRoleToken`): grant the target hexproof,
//! then mint a Royal Role Aura token attached to it (which grants +1/+1 & ward {1} via the token def's
//! host-scoped static + a printed `BecomesTargeted{AttachedHost}` ward trigger).

use crate::basics::{CardType, Color};
use crate::cards::grp;
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::ability::Keyword;
use crate::effects::condition::Duration;
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::PlayerRef;
use crate::effects::{Effect, EffectTarget};

/// grp id (bonus-sheet `soa` cards use the 600+ block).
pub const ROYAL_TREATMENT: u32 = 651;

pub fn register(db: &mut CardDb) {
    let effect = Effect::Sequence(vec![
        // "Target creature you control gains hexproof until end of turn." — slot 0 (the Role's host).
        Effect::GrantKeyword {
            what: EffectTarget::Target(TargetSpec {
                kind: TargetKind::Creature(CardFilter::ControlledBy(PlayerRef::Controller)),
                min: 1,
                max: 1,
                distinct: true,
            }),
            keyword: Keyword::Hexproof,
            duration: Duration::UntilEndOfTurn,
        },
        // "Create a Royal Role token attached to that creature." — the just-targeted creature (slot 0).
        Effect::CreateRoleToken { role: grp::ROYAL_ROLE_TOKEN, attach_to: EffectTarget::ChosenIndex(0) },
    ]);
    let def = spell(
        ROYAL_TREATMENT,
        "Royal Treatment",
        CardType::Instant,
        Color::Green,
        mana_cost(0, &[(Color::Green, 1)]),
        effect,
    )
    .with_text(
        "Target creature you control gains hexproof until end of turn. Create a Royal Role token attached to that creature. (If you control another Role on it, put that one into the graveyard. Enchanted creature gets +1/+1 and has ward {1}.)",
    );
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, CastVariant, DecisionRequest, DecisionResponse, PlayerView};
    use crate::basics::{Phase, Target, Zone};
    use crate::ids::{ObjId, PlayerId};
    use crate::priority::Engine;
    use crate::subtypes::{EnchantmentType, Subtype};

    #[derive(Clone)]
    struct TargetFirst;
    impl Agent for TargetFirst {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::ChooseTargets { slots, .. } if !slots.is_empty() && !slots[0].legal.is_empty() => {
                    DecisionResponse::Pairs(vec![(0, 0)])
                }
                _ => DecisionResponse::Pass,
            }
        }
    }

    fn resolve_all(e: &mut Engine) {
        e.run_agenda();
        while !e.state.stack.items.is_empty() {
            e.resolve_top();
            e.run_agenda();
        }
    }

    #[test]
    fn royal_role_grants_hexproof_plus_one_and_ward() {
        let mut state = crate::cards::build_game(1, &[&[], &[]]);
        let treat = state.add_card(PlayerId(0), state.card_db().get(ROYAL_TREATMENT).unwrap().chars.clone(), Zone::Hand);
        state.add_card(PlayerId(0), state.card_db().get(grp::FOREST).unwrap().chars.clone(), Zone::Battlefield);
        let bear = state.add_card(PlayerId(0), state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Battlefield);
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let mut e = Engine::new(state, vec![Box::new(TargetFirst), Box::new(TargetFirst)]);
        e.cast_spell(PlayerId(0), treat, CastVariant::Normal);
        resolve_all(&mut e);
        // 2/2 base + Role's +1/+1 = 3/3, hexproof this turn.
        assert_eq!(e.state.computed(bear).power, Some(3));
        assert_eq!(e.state.computed(bear).toughness, Some(3));
        assert!(e.state.computed(bear).has_keyword(Keyword::Hexproof), "hexproof this turn");
        // One Royal Role attached, carrying the Ward trigger.
        let role = e.state.objects.values().find(|o| o.chars.subtypes.contains(&Subtype::Enchantment(EnchantmentType::Role)) && o.attached_to == Some(bear)).map(|o| o.id).unwrap();
        let has_ward = e.state.def_of(role).is_some_and(|d| {
            d.abilities.iter().any(|a| matches!(a, crate::effects::ability::Ability::Triggered { event: crate::effects::ability::EventPattern::BecomesTargeted { .. }, .. }))
        });
        assert!(has_ward, "Royal Role carries a ward (BecomesTargeted) trigger");
    }

    /// Ward {1}: on a later turn (hexproof expired), an opponent's spell targeting the enchanted
    /// creature is countered unless they pay {1}. The opponent has exactly one Mountain — enough to
    /// cast Shock but not the extra {1} the ward demands — so the Shock is countered.
    #[test]
    fn ward_counters_unpaid_opponent_spell() {
        let mut state = crate::cards::build_game(1, &[&[], &[]]);
        let treat = state.add_card(PlayerId(0), state.card_db().get(ROYAL_TREATMENT).unwrap().chars.clone(), Zone::Hand);
        state.add_card(PlayerId(0), state.card_db().get(grp::FOREST).unwrap().chars.clone(), Zone::Battlefield);
        let bear = state.add_card(PlayerId(0), state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Battlefield);
        // Opponent: a Shock + exactly one Mountain (can pay {R} for Shock but not the ward's extra {1}).
        let shock = state.add_card(PlayerId(1), state.card_db().get(grp::SHOCK).unwrap().chars.clone(), Zone::Hand);
        state.add_card(PlayerId(1), state.card_db().get(grp::MOUNTAIN).unwrap().chars.clone(), Zone::Battlefield);
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let mut e = Engine::new(state, vec![Box::new(TargetFirst), Box::new(TargetFirst)]);
        e.cast_spell(PlayerId(0), treat, CastVariant::Normal);
        resolve_all(&mut e);
        // Expire the until-end-of-turn hexproof (the Role's +1/+1 & ward are permanent, WhileSourcePresent).
        e.state.end_of_turn_continuous_cleanup();
        assert!(!e.state.computed(bear).has_keyword(Keyword::Hexproof), "hexproof expired");
        // Opponent Shocks the bear; the Role's ward soft-counters it (no spare {1}).
        e.cast_spell(PlayerId(1), shock, CastVariant::Normal);
        while !e.state.stack.items.is_empty() {
            e.resolve_top();
            e.run_agenda();
        }
        assert!(e.state.player(PlayerId(0)).battlefield.contains(&bear), "warded creature survives the countered Shock");
        assert!(e.state.player(PlayerId(1)).graveyard.contains(&shock), "Shock was countered (in its owner's graveyard)");
        let _ = Target::Object(bear);
    }
}
