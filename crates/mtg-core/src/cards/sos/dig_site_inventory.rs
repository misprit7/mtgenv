//! Dig Site Inventory — `{W}` Sorcery (first printed SOS).
//!
//! Oracle: "Put a +1/+1 counter on target creature you control. It gains vigilance until end of turn.
//! / Flashback {W}"
//!
//! **Fully implemented** — a +1/+1 counter + vigilance-until-EOT on a target creature you control,
//! with `Ability::Flashback {W}` (cast from the graveyard for `{W}`, then it's exiled as it resolves).

use crate::basics::{CardType, Color, CounterKind};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::ability::{Ability, Keyword};
use crate::effects::condition::Duration;
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};

/// grp id (per-set ids live near their cards).
pub const DIG_SITE_INVENTORY: u32 = 286;

pub fn register(db: &mut CardDb) {
    let effect = Effect::Sequence(vec![
        Effect::PutCounters {
            what: EffectTarget::Target(TargetSpec {
                kind: TargetKind::Creature(CardFilter::ControlledBy(PlayerRef::Controller)),
                min: 1,
                max: 1,
                distinct: true,
            }),
            kind: CounterKind::PlusOnePlusOne,
            n: ValueExpr::Fixed(1),
        },
        Effect::GrantKeyword {
            what: EffectTarget::ChosenIndex(0),
            keyword: Keyword::Vigilance,
            duration: Duration::UntilEndOfTurn,
        },
    ]);
    let mut def = spell(
        DIG_SITE_INVENTORY,
        "Dig Site Inventory",
        CardType::Sorcery,
        Color::White,
        mana_cost(0, &[(Color::White, 1)]),
        effect,
    )
    .with_text("Put a +1/+1 counter on target creature you control. It gains vigilance until end of turn.\nFlashback {W}");
    // Flashback {W} — the same single white pip.
    def.abilities.push(Ability::Flashback { cost: mana_cost(0, &[(Color::White, 1)]) });
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, CastVariant, DecisionRequest, DecisionResponse, PlayableAction, PlayerView};
    use crate::basics::{Phase, Zone};
    use crate::cards::{build_game, grp};
    use crate::ids::{PlayerId, StackId};
    use crate::priority::Engine;

    #[derive(Clone)]
    struct Passer;
    impl Agent for Passer {
        fn decide(&mut self, _v: &PlayerView, _r: &DecisionRequest) -> DecisionResponse {
            DecisionResponse::Pass
        }
    }

    #[test]
    fn dig_site_inventory_has_flashback() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(DIG_SITE_INVENTORY).unwrap();
        assert!(def.fully_implemented);
        assert!(def.abilities.iter().any(|a| matches!(a, Ability::Flashback { .. })), "declares Flashback");
    }

    /// S10 cap (offer): a card with `Ability::Flashback` in your graveyard, with the flashback cost
    /// affordable, is offered as a `CastVariant::Flashback` from the graveyard.
    #[test]
    fn flashback_offered_from_graveyard() {
        let mut state = build_game(1, &[&[], &[]]);
        let card = state.add_card(PlayerId(0), state.card_db().get(DIG_SITE_INVENTORY).unwrap().chars.clone(), Zone::Graveyard);
        // A creature to target + an untapped Plains for the {W}.
        let _bear = state.add_card(PlayerId(0), state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Battlefield);
        let plains = state.card_db().get(grp::PLAINS).unwrap().chars.clone();
        state.add_card(PlayerId(0), plains, Zone::Battlefield);
        let mut e = Engine::new(state, vec![Box::new(Passer), Box::new(Passer)]);
        e.state.phase = Phase::PrecombatMain;
        let offered = e.legal_actions(PlayerId(0)).into_iter().any(|a| {
            matches!(a, PlayableAction::Cast { spell, variant: CastVariant::Flashback } if spell == card)
        });
        assert!(offered, "flashback cast is offered from the graveyard");
    }

    /// S10 cap (exile-on-resolve): a flashback-cast spell is exiled — not put in the graveyard — as it
    /// leaves the stack (CR 702.34d).
    #[test]
    fn flashback_spell_is_exiled_on_resolution() {
        use crate::stack::{StackObject, StackObjectKind};
        let mut state = build_game(1, &[&[], &[]]);
        let card = state.add_card(PlayerId(0), state.card_db().get(DIG_SITE_INVENTORY).unwrap().chars.clone(), Zone::Stack);
        // Mark it flashback-cast (no legal target → effect is skipped, but it still leaves the stack).
        state.objects.get_mut(&card).unwrap().flashback_cast = true;
        let sid = StackId(700);
        state.stack.push(StackObject {
            id: sid,
            controller: PlayerId(0),
            source: None,
            kind: StackObjectKind::Spell(card),
            targets: Vec::new(),
            x: None,
            modes: Vec::new(),
        });
        let mut e = Engine::new(state, vec![Box::new(Passer), Box::new(Passer)]);
        e.resolve_top();
        assert!(e.state.players[0].exile.contains(&card), "flashback spell exiled");
        assert!(!e.state.players[0].graveyard.contains(&card), "not in the graveyard");
    }
}
