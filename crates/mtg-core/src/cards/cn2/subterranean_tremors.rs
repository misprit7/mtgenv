//! Subterranean Tremors — `{X}{R}` Sorcery (first printed CN2; reprinted on the SOS Mystical Archive `soa`).
//!
//! Oracle: "Subterranean Tremors deals X damage to each creature without flying. If X is 4 or more,
//! destroy all artifacts. If X is 8 or more, create an 8/8 red Lizard creature token."
//!
//! **Fully implemented** — `ForEach` mass X-damage over every creature without flying, then two
//! `Conditional`s keyed on the X chosen at cast (`ValueAtLeast(X, 4)` → destroy all artifacts;
//! `ValueAtLeast(X, 8)` → make an 8/8 red Lizard).

use crate::basics::{CardType, Color, DamageKind, Zone};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::ability::Keyword;
use crate::effects::condition::Condition;
use crate::effects::target::{CardFilter, SelectSpec, TokenSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};
use crate::subtypes::CreatureType;

/// grp id (bonus-sheet `soa` cards use the 600+ block).
pub const SUBTERRANEAN_TREMORS: u32 = 632;

fn mass_over(filter: CardFilter, body: Effect) -> Effect {
    Effect::ForEach {
        selector: SelectSpec {
            zone: Zone::Battlefield,
            filter,
            chooser: PlayerRef::EachPlayer,
            min: ValueExpr::Fixed(0),
            max: ValueExpr::Fixed(999),
        },
        body: Box::new(body),
    }
}

pub fn register(db: &mut CardDb) {
    let effect = Effect::Sequence(vec![
        // X damage to each creature without flying.
        mass_over(
            CardFilter::All(vec![
                CardFilter::HasCardType(CardType::Creature),
                CardFilter::Not(Box::new(CardFilter::HasKeyword(Keyword::Flying))),
            ]),
            Effect::DealDamage { amount: ValueExpr::X, to: EffectTarget::Each, kind: DamageKind::Noncombat },
        ),
        // If X ≥ 4, destroy all artifacts.
        Effect::Conditional {
            cond: Condition::ValueAtLeast(ValueExpr::X, ValueExpr::Fixed(4)),
            then: Box::new(mass_over(
                CardFilter::HasCardType(CardType::Artifact),
                Effect::Destroy { what: EffectTarget::Each },
            )),
            otherwise: None,
        },
        // If X ≥ 8, create an 8/8 red Lizard.
        Effect::Conditional {
            cond: Condition::ValueAtLeast(ValueExpr::X, ValueExpr::Fixed(8)),
            then: Box::new(Effect::CreateToken {
                spec: TokenSpec {
                    name: "Lizard".to_string(),
                    card_types: vec![CardType::Creature],
                    subtypes: vec![CreatureType::Lizard.into()],
                    colors: vec![Color::Red],
                    power: 8,
                    toughness: 8,
                    keywords: vec![],
                    counters: vec![],
                    grp_id: 0,
                },
                count: ValueExpr::Fixed(1),
                controller: PlayerRef::Controller,
                dynamic_counters: vec![],
            }),
            otherwise: None,
        },
    ]);
    db.insert(
        spell(
            SUBTERRANEAN_TREMORS,
            "Subterranean Tremors",
            CardType::Sorcery,
            Color::Red,
            mana_cost(0, &[(Color::Red, 1)]),
            effect,
        )
        .with_text("Subterranean Tremors deals X damage to each creature without flying. If X is 4 or more, destroy all artifacts. If X is 8 or more, create an 8/8 red Lizard creature token."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView};
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

    /// X=4: 4 damage marked on a ground creature, and all artifacts destroyed; no 8/8 (X<8).
    #[test]
    fn x4_damages_ground_and_wipes_artifacts() {
        use crate::state::Characteristics;
        let mut state = build_game(1, &[&[], &[]]);
        let bear = state.add_card(PlayerId(1), state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Battlefield);
        let art = state.add_card(
            PlayerId(1),
            Characteristics { name: "Widget".to_string(), card_types: vec![CardType::Artifact], grp_id: 8040, ..Default::default() },
            Zone::Battlefield,
        );
        let effect = state.card_db().get(SUBTERRANEAN_TREMORS).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(Passive), Box::new(Passive)]);
        e.resolve_effect(
            &effect,
            &ResolutionCtx { controller: Some(PlayerId(0)), x: Some(4), ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        assert_eq!(e.state.object(bear).damage_marked, 4, "4 damage to the ground creature");
        assert!(!e.state.player(PlayerId(1)).battlefield.contains(&art), "artifacts destroyed (X≥4)");
        assert!(!e.state.player(PlayerId(0)).battlefield.iter().any(|&id| e.state.object(id).chars.name == "Lizard"), "no Lizard (X<8)");
    }
}
