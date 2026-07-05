//! Run Behind — `{3}{U}` Instant (first printed SOS).
//!
//! Oracle: "This spell costs {1} less to cast if it targets an attacking creature. Target creature's
//! owner puts it on their choice of the top or bottom of their library."
//!
//! **Fully implemented.** The S12 target-dependent cost reduction (`CostReduction{ Generic(1),
//! TargetMatches(Attacking), Cast }`) plus [`Effect::PutOnTopOrBottom`] — the targeted creature's
//! **owner** chooses top vs bottom of their library.

use crate::basics::{CardType, Color};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::ability::{
    Ability, CostReductionAmount, CostReductionCondition, CostReductionScope,
};
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::{Effect, EffectTarget};

/// grp id (per-set ids live near their cards).
pub const RUN_BEHIND: u32 = 372;

pub fn register(db: &mut CardDb) {
    let effect = Effect::PutOnTopOrBottom {
        what: EffectTarget::Target(TargetSpec {
            kind: TargetKind::Creature(CardFilter::Any),
            min: 1,
            max: 1,
            distinct: true,
        }),
    };
    let mut def = spell(
        RUN_BEHIND,
        "Run Behind",
        CardType::Instant,
        Color::Blue,
        mana_cost(3, &[(Color::Blue, 1)]),
        effect,
    )
    .with_text(
        "This spell costs {1} less to cast if it targets an attacking creature. Target creature's owner puts it on their choice of the top or bottom of their library.",
    );
    // {1} less if it targets an attacking creature (CR 601.2f target-dependent reduction).
    def.abilities.push(Ability::CostReduction {
        amount: CostReductionAmount::Generic(1),
        condition: CostReductionCondition::TargetMatches(CardFilter::Attacking),
        scope: CostReductionScope::Cast,
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

    #[test]
    fn run_behind_shape() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(RUN_BEHIND).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Instant]);
        assert!(def.fully_implemented);
        assert!(matches!(
            def.abilities.last(),
            Some(Ability::CostReduction {
                condition: CostReductionCondition::TargetMatches(CardFilter::Attacking),
                ..
            })
        ));
    }

    /// The targeted creature's OWNER chooses top or bottom; here P1's creature is put on top of P1's
    /// library (P1 answers the Confirm with "top").
    #[test]
    fn owner_puts_targeted_creature_on_top_of_library() {
        let mut state = build_game(1, &[&[], &[]]);
        let bears = {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            state.add_card(PlayerId(1), c, Zone::Battlefield)
        };
        // Run Behind is registered via `starter_db` (sos::register).
        let effect = state.card_db().get(RUN_BEHIND).unwrap().spell_effect().unwrap().clone();

        // P1 (the owner) says "top"; P0 (caster) passes.
        struct TopChoice;
        impl Agent for TopChoice {
            fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
                match req {
                    DecisionRequest::Confirm { .. } => DecisionResponse::Bool(true),
                    _ => DecisionResponse::Pass,
                }
            }
        }
        let mut e = Engine::new(state, vec![Box::new(TopChoice), Box::new(TopChoice)]);
        e.resolve_effect(
            &effect,
            &ResolutionCtx {
                controller: Some(PlayerId(0)),
                chosen_targets: vec![Target::Object(bears)],
                ..Default::default()
            },
            WbReason::Resolve(StackId(0)),
        );

        assert_eq!(e.state.object(bears).zone, Zone::Library, "the creature was put into its library");
        assert_eq!(
            e.state.player(PlayerId(1)).library.last().copied(),
            Some(bears),
            "on TOP (the library's top is the last element)"
        );
    }
}
