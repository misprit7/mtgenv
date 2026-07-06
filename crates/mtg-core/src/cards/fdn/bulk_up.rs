//! Bulk Up — `{1}{R}` Instant (first printed FDN; reprinted on the SOS Mystical Archive `soa`).
//!
//! Oracle: "Double target creature's power until end of turn.
//! Flashback {4}{R}{R}."
//!
//! **Fully implemented** — "double power" is `PumpPT { power: PowerOfTarget(0), toughness: 0 }` until
//! end of turn (add the creature's current power to itself). Plus **Flashback** `{4}{R}{R}` (the
//! shipped mana-cost flashback cap): cast from the graveyard for the flashback cost, then exile it.

use crate::basics::{CardType, Color};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::ability::{Ability, Cost};
use crate::effects::condition::Duration;
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::ValueExpr;
use crate::effects::{Effect, EffectTarget};

/// grp id (bonus-sheet `soa` cards use the 600+ block).
pub const BULK_UP: u32 = 620;

pub fn register(db: &mut CardDb) {
    let effect = Effect::PumpPT {
        what: EffectTarget::Target(TargetSpec {
            kind: TargetKind::Creature(CardFilter::Any),
            min: 1,
            max: 1,
            distinct: true,
        }),
        power: ValueExpr::PowerOfTarget(0),
        toughness: ValueExpr::Fixed(0),
        duration: Duration::UntilEndOfTurn,
    };
    let mut def = spell(
        BULK_UP,
        "Bulk Up",
        CardType::Instant,
        Color::Red,
        mana_cost(1, &[(Color::Red, 1)]),
        effect,
    )
    .with_text("Double target creature's power until end of turn.\nFlashback {4}{R}{R} (You may cast this card from your graveyard for its flashback cost. Then exile it.)");
    def.abilities.push(Ability::Flashback {
        cost: Cost { mana: Some(mana_cost(4, &[(Color::Red, 2)])), components: vec![] },
    });
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView};
    use crate::basics::{Target, Zone};
    use crate::cards::{build_game, grp};
    use crate::effects::action::{ResolutionCtx, WbReason};
    use crate::ids::{PlayerId, StackId};
    use crate::priority::Engine;

    #[derive(Clone)]
    struct Passive;
    impl Agent for Passive {
        fn decide(&mut self, _v: &PlayerView, _r: &DecisionRequest) -> DecisionResponse {
            DecisionResponse::Pass
        }
    }

    #[test]
    fn bulk_up_has_flashback() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(BULK_UP).unwrap();
        assert!(def.fully_implemented);
        assert!(def.abilities.iter().any(|a| matches!(a, Ability::Flashback { .. })), "flashback present");
    }

    /// Behaviour: doubling a 3/3's power makes it 6/3 until end of turn.
    #[test]
    fn bulk_up_doubles_power() {
        let mut state = build_game(1, &[&[], &[]]);
        let giant = state.add_card(PlayerId(0), state.card_db().get(grp::HILL_GIANT).unwrap().chars.clone(), Zone::Battlefield); // 3/3
        let effect = state.card_db().get(BULK_UP).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(Passive), Box::new(Passive)]);
        e.resolve_effect(
            &effect,
            &ResolutionCtx { controller: Some(PlayerId(0)), chosen_targets: vec![Target::Object(giant)], ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        let c = e.state.computed(giant);
        assert_eq!((c.power, c.toughness), (Some(6), Some(3)), "power doubled 3 → 6, toughness unchanged");
    }
}
