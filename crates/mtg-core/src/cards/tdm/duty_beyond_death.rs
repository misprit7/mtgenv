//! Duty Beyond Death — `{1}{W}` Instant (first printed TDM; reprinted on the SOS Mystical Archive `soa`).
//!
//! Oracle: "As an additional cost to cast this spell, sacrifice a creature. Creatures you control
//! gain indestructible until end of turn. Put a +1/+1 counter on each creature you control."
//!
//! **Fully implemented** — a spell-level additional "sacrifice a creature" cost (paid at cast) + a
//! `ForEach` over your creatures granting indestructible until end of turn and adding a +1/+1 counter.

use crate::basics::{CardType, Color, CounterKind, Zone};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::ability::{AdditionalCost, Ability, Cost, CostComponent, Keyword};
use crate::effects::condition::Duration;
use crate::effects::target::{CardFilter, SelectSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};

/// grp id (bonus-sheet `soa` cards use the 600+ block).
pub const DUTY_BEYOND_DEATH: u32 = 621;

pub fn register(db: &mut CardDb) {
    let effect = Effect::ForEach {
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
        body: Box::new(Effect::Sequence(vec![
            Effect::GrantKeyword {
                what: EffectTarget::Each,
                keyword: Keyword::Indestructible,
                duration: Duration::UntilEndOfTurn,
            },
            Effect::PutCounters {
                what: EffectTarget::Each,
                kind: CounterKind::PlusOnePlusOne,
                n: ValueExpr::Fixed(1),
            },
        ])),
    };
    let mut def = spell(
        DUTY_BEYOND_DEATH,
        "Duty Beyond Death",
        CardType::Instant,
        Color::White,
        mana_cost(1, &[(Color::White, 1)]),
        effect,
    )
    .with_text("As an additional cost to cast this spell, sacrifice a creature.\nCreatures you control gain indestructible until end of turn. Put a +1/+1 counter on each creature you control.");
    def.abilities.push(Ability::AdditionalCost(AdditionalCost {
        options: vec![Cost {
            mana: None,
            components: vec![CostComponent::Sacrifice(SelectSpec {
                zone: Zone::Battlefield,
                filter: CardFilter::All(vec![
                    CardFilter::HasCardType(CardType::Creature),
                    CardFilter::ControlledBy(PlayerRef::Controller),
                ]),
                chooser: PlayerRef::Controller,
                min: ValueExpr::Fixed(1),
                max: ValueExpr::Fixed(1),
            })],
        }],
    }));
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView};
    use crate::basics::Zone;
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
    fn duty_beyond_death_shape() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(DUTY_BEYOND_DEATH).unwrap();
        assert!(def.fully_implemented);
        assert_eq!(def.additional_costs().len(), 1, "sacrifice-a-creature clause");
    }

    /// Behaviour: your creatures get a +1/+1 counter and indestructible (the resolution effect only —
    /// the additional cost is paid at cast, tested via the real cast path elsewhere).
    #[test]
    fn buffs_your_creatures() {
        let mut state = build_game(1, &[&[], &[]]);
        let bear = state.add_card(PlayerId(0), state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Battlefield);
        let theirs = state.add_card(PlayerId(1), state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Battlefield);
        let effect = state.card_db().get(DUTY_BEYOND_DEATH).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(Passive), Box::new(Passive)]);
        e.resolve_effect(
            &effect,
            &ResolutionCtx { controller: Some(PlayerId(0)), ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        let c = e.state.computed(bear);
        assert_eq!((c.power, c.toughness), (Some(3), Some(3)), "my creature got a +1/+1 counter → 3/3");
        let t = e.state.computed(theirs);
        assert_eq!((t.power, t.toughness), (Some(2), Some(2)), "opponent's creature untouched");
    }
}
