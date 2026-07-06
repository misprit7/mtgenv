//! Shamanic Revelation — `{3}{G}{G}` Sorcery (first printed FRF; reprinted on the SOS Mystical Archive `soa`).
//!
//! Oracle: "Draw a card for each creature you control.
//! Ferocious — You gain 4 life for each creature you control with power 4 or greater."
//!
//! **Fully implemented** — `Draw` equal to `Count` of the creatures you control, then a `ForEach` over
//! your creatures with power 4 or greater (`PowerAtLeast(4)`) whose body gains 4 life per creature (so
//! total = 4 × the Ferocious count; 0 if you control none).

use crate::basics::{CardType, Color, Zone};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::target::{CardFilter, SelectSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;

/// grp id (bonus-sheet `soa` cards use the 600+ block).
pub const SHAMANIC_REVELATION: u32 = 626;

fn your_creatures(extra: Option<CardFilter>) -> CardFilter {
    let mut parts = vec![
        CardFilter::HasCardType(CardType::Creature),
        CardFilter::ControlledBy(PlayerRef::Controller),
    ];
    if let Some(f) = extra {
        parts.push(f);
    }
    CardFilter::All(parts)
}

pub fn register(db: &mut CardDb) {
    let effect = Effect::Sequence(vec![
        Effect::Draw {
            who: PlayerRef::Controller,
            count: ValueExpr::Count {
                zone: Zone::Battlefield,
                filter: your_creatures(None),
                controller: Some(PlayerRef::Controller),
            },
        },
        // Ferocious — gain 4 life for each creature you control with power 4 or greater.
        Effect::ForEach {
            selector: SelectSpec {
                zone: Zone::Battlefield,
                filter: your_creatures(Some(CardFilter::PowerAtLeast(4))),
                chooser: PlayerRef::Controller,
                min: ValueExpr::Fixed(0),
                max: ValueExpr::Fixed(999),
            },
            body: Box::new(Effect::GainLife { who: PlayerRef::Controller, amount: ValueExpr::Fixed(4) }),
        },
    ]);
    let mut def = spell(
        SHAMANIC_REVELATION,
        "Shamanic Revelation",
        CardType::Sorcery,
        Color::Green,
        mana_cost(3, &[(Color::Green, 2)]),
        effect,
    )
    .with_text("Draw a card for each creature you control.\nFerocious — You gain 4 life for each creature you control with power 4 or greater.");
    def.chars.colors = vec![Color::Green];
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView};
    use crate::cards::{build_game, grp};
    use crate::effects::action::{ResolutionCtx, WbReason};
    use crate::ids::{PlayerId, StackId};
    use crate::priority::Engine;
    use crate::state::Characteristics;

    #[derive(Clone)]
    struct Passive;
    impl Agent for Passive {
        fn decide(&mut self, _v: &PlayerView, _r: &DecisionRequest) -> DecisionResponse {
            DecisionResponse::Pass
        }
    }

    /// P0 controls three creatures (two 2/2 Bears + one 5/5 big). Draw 3; Ferocious gains 4 (one
    /// creature with power ≥ 4).
    #[test]
    fn draws_per_creature_and_ferocious_life() {
        let mut state = build_game(1, &[&[], &[]]);
        state.add_card(PlayerId(0), state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Battlefield);
        state.add_card(PlayerId(0), state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Battlefield);
        state.add_card(
            PlayerId(0),
            Characteristics { name: "Big One".to_string(), card_types: vec![CardType::Creature], power: Some(5), toughness: Some(5), grp_id: 8030, ..Default::default() },
            Zone::Battlefield,
        );
        for _ in 0..5 {
            state.add_card(PlayerId(0), state.card_db().get(grp::FOREST).unwrap().chars.clone(), Zone::Library);
        }
        let life_before = state.player(PlayerId(0)).life;
        let effect = state.card_db().get(SHAMANIC_REVELATION).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(Passive), Box::new(Passive)]);
        e.resolve_effect(
            &effect,
            &ResolutionCtx { controller: Some(PlayerId(0)), ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        assert_eq!(e.state.player(PlayerId(0)).hand.len(), 3, "drew a card per creature (3)");
        assert_eq!(e.state.player(PlayerId(0)).life, life_before + 4, "Ferocious: gained 4 for the one power-≥4 creature");
    }
}
