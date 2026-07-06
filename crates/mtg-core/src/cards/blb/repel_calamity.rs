//! Repel Calamity — `{1}{W}` Instant (first printed BLB; reprinted on the SOS Mystical Archive `soa`).
//!
//! Oracle: "Destroy target creature with power or toughness 4 or greater."
//!
//! **Fully implemented** — a single-target `Destroy` restricted to a creature whose power OR toughness
//! is 4 or greater (`AnyOf[PowerAtLeast(4), ToughnessAtLeast(4)]`, the new `*AtLeast` filter cap).

use crate::basics::{CardType, Color};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::{Effect, EffectTarget};

/// grp id (bonus-sheet `soa` cards use the 600+ block).
pub const REPEL_CALAMITY: u32 = 625;

pub fn register(db: &mut CardDb) {
    let effect = Effect::Destroy {
        what: EffectTarget::Target(TargetSpec {
            kind: TargetKind::Creature(CardFilter::AnyOf(vec![
                CardFilter::PowerAtLeast(4),
                CardFilter::ToughnessAtLeast(4),
            ])),
            min: 1,
            max: 1,
            distinct: true,
        }),
    };
    db.insert(
        spell(
            REPEL_CALAMITY,
            "Repel Calamity",
            CardType::Instant,
            Color::White,
            mana_cost(1, &[(Color::White, 1)]),
            effect,
        )
        .with_text("Destroy target creature with power or toughness 4 or greater."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView};
    use crate::basics::{Target, Zone};
    use crate::cards::build_game;
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

    /// A 3/3 Hill Giant is NOT a legal target (power 3, toughness 3), but a 4/5 would be. Here we
    /// destroy a bespoke 4/4 to prove the *AtLeast filter matches.
    #[test]
    fn destroys_a_big_creature() {
        use crate::state::Characteristics;
        let mut state = build_game(1, &[&[], &[]]);
        let big = state.add_card(
            PlayerId(1),
            Characteristics {
                name: "Big Beast".to_string(),
                card_types: vec![CardType::Creature],
                power: Some(4),
                toughness: Some(4),
                grp_id: 8020,
                ..Default::default()
            },
            Zone::Battlefield,
        );
        let effect = state.card_db().get(REPEL_CALAMITY).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(Passive), Box::new(Passive)]);
        e.resolve_effect(
            &effect,
            &ResolutionCtx { controller: Some(PlayerId(0)), chosen_targets: vec![Target::Object(big)], ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        assert!(!e.state.player(PlayerId(1)).battlefield.contains(&big), "the 4/4 was destroyed");
    }
}
