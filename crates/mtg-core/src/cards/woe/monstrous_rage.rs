//! Monstrous Rage — `{R}` Instant (first printed WOE; reprinted on the SOS Mystical Archive `soa`).
//!
//! Oracle: "Target creature gets +2/+0 until end of turn. Create a Monster Role token attached to it.
//! (If you control another Role on it, put that one into the graveyard. Enchanted creature gets +1/+1
//! and has trample.)"
//!
//! **Fully implemented** — the Role subsystem (`Effect::CreateRoleToken`): pump the target +2/+0, then
//! mint a Monster Role Aura token attached to it (which grants +1/+1 & trample via host-scoped statics).

use crate::basics::{CardType, Color};
use crate::cards::grp;
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::condition::Duration;
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::ValueExpr;
use crate::effects::{Effect, EffectTarget};

/// grp id (bonus-sheet `soa` cards use the 600+ block).
pub const MONSTROUS_RAGE: u32 = 650;

pub fn register(db: &mut CardDb) {
    let effect = Effect::Sequence(vec![
        // "Target creature gets +2/+0 until end of turn." — slot 0 (the creature the Role attaches to).
        Effect::PumpPT {
            what: EffectTarget::Target(TargetSpec {
                kind: TargetKind::Creature(CardFilter::Any),
                min: 1,
                max: 1,
                distinct: true,
            }),
            power: ValueExpr::Fixed(2),
            toughness: ValueExpr::Fixed(0),
            duration: Duration::UntilEndOfTurn,
        },
        // "Create a Monster Role token attached to it." — the just-targeted creature (slot 0).
        Effect::CreateRoleToken { role: grp::MONSTER_ROLE_TOKEN, attach_to: EffectTarget::ChosenIndex(0) },
    ]);
    let def = spell(
        MONSTROUS_RAGE,
        "Monstrous Rage",
        CardType::Instant,
        Color::Red,
        mana_cost(0, &[(Color::Red, 1)]),
        effect,
    )
    .with_text(
        "Target creature gets +2/+0 until end of turn. Create a Monster Role token attached to it. (If you control another Role on it, put that one into the graveyard. Enchanted creature gets +1/+1 and has trample.)",
    );
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, CastVariant, DecisionRequest, DecisionResponse, PlayerView};
    use crate::basics::{Phase, Zone};
    use crate::effects::ability::Keyword;
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

    fn setup() -> (Engine, ObjId, ObjId) {
        let mut state = crate::cards::build_game(1, &[&[], &[]]);
        let rage = state.add_card(PlayerId(0), state.card_db().get(MONSTROUS_RAGE).unwrap().chars.clone(), Zone::Hand);
        state.add_card(PlayerId(0), state.card_db().get(grp::MOUNTAIN).unwrap().chars.clone(), Zone::Battlefield);
        let bear = state.add_card(PlayerId(0), state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Battlefield);
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let e = Engine::new(state, vec![Box::new(TargetFirst), Box::new(TargetFirst)]);
        (e, rage, bear)
    }

    fn is_role(e: &Engine, id: ObjId) -> bool {
        e.state.objects.get(&id).is_some_and(|o| {
            o.chars.subtypes.contains(&Subtype::Enchantment(EnchantmentType::Role))
        })
    }

    fn resolve_all(e: &mut Engine) {
        e.run_agenda();
        while !e.state.stack.items.is_empty() {
            e.resolve_top();
            e.run_agenda();
        }
    }

    #[test]
    fn monster_role_pumps_and_grants_trample() {
        let (mut e, rage, bear) = setup();
        e.cast_spell(PlayerId(0), rage, CastVariant::Normal);
        resolve_all(&mut e);
        // 2/2 base + spell's +2/+0 + Role's +1/+1 = 5/3, with trample.
        assert_eq!(e.state.computed(bear).power, Some(5), "power 2 +2 +1");
        assert_eq!(e.state.computed(bear).toughness, Some(3), "toughness 2 +0 +1");
        assert!(e.state.computed(bear).has_keyword(Keyword::Trample), "Role grants trample");
        // Exactly one Monster Role token attached to the bear.
        let roles: Vec<ObjId> = e.state.objects.values().filter(|o| is_role(&e, o.id) && o.attached_to == Some(bear)).map(|o| o.id).collect();
        assert_eq!(roles.len(), 1, "one Role attached");
    }

    /// One Role per controller (CR 303.4k): a second Monstrous Rage on the same creature replaces the
    /// first Role — the older token goes to the graveyard and then ceases to exist (CR 111.7 token SBA).
    #[test]
    fn second_role_replaces_first_and_prior_ceases_to_exist() {
        let (mut e, rage, bear) = setup();
        let rage2 = e.state.add_card(PlayerId(0), e.state.card_db().get(MONSTROUS_RAGE).unwrap().chars.clone(), Zone::Hand);
        e.state.add_card(PlayerId(0), e.state.card_db().get(grp::MOUNTAIN).unwrap().chars.clone(), Zone::Battlefield);
        e.cast_spell(PlayerId(0), rage, CastVariant::Normal);
        resolve_all(&mut e);
        let first_role = e.state.objects.values().find(|o| is_role(&e, o.id) && o.attached_to == Some(bear)).map(|o| o.id).unwrap();
        e.cast_spell(PlayerId(0), rage2, CastVariant::Normal);
        resolve_all(&mut e);
        // The first Role ceased to exist (removed entirely — not a stray graveyard object).
        assert!(!e.state.objects.contains_key(&first_role), "first Role ceased to exist");
        let roles: Vec<ObjId> = e.state.objects.values().filter(|o| is_role(&e, o.id) && o.attached_to == Some(bear)).map(|o| o.id).collect();
        assert_eq!(roles.len(), 1, "still exactly one Role attached");
        // The spell's +2/+0 stacked twice (both resolved this turn) + one Role's +1/+1: 2 +4 +1 = 7/3.
        assert_eq!(e.state.computed(bear).power, Some(7));
    }

    /// A Role whose host leaves the battlefield falls off (CR 704.5) and, being a token, ceases to
    /// exist (CR 111.7) rather than lingering in the graveyard.
    #[test]
    fn role_ceases_to_exist_when_host_leaves() {
        let (mut e, rage, bear) = setup();
        e.cast_spell(PlayerId(0), rage, CastVariant::Normal);
        resolve_all(&mut e);
        let role = e.state.objects.values().find(|o| is_role(&e, o.id)).map(|o| o.id).unwrap();
        // Host leaves the battlefield → Role falls off → token ceases to exist.
        let owner = e.state.object(bear).owner;
        e.state.move_object(bear, Zone::Graveyard, owner);
        e.run_agenda();
        assert!(!e.state.objects.contains_key(&role), "Role ceased to exist after host left");
    }
}
