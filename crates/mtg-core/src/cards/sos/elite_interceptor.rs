//! Elite Interceptor // Rejoinder — `{W}` Creature — Human Wizard 1/2 // `{1}{W}` Sorcery
//! (first printed SOS). A **Prepare** DFC (enters prepared).
//!
//! Front: "This creature enters prepared."
//! Back (Rejoinder): "You may tap or untap target creature. Draw a card."
//!
//! **Fully implemented** — enters-prepared via [`helpers::enters_prepared`]; the back is
//! [`Effect::MayTapOrUntap`] (opt-in, then a tap-or-untap direction) on a target creature, then draw a card.

use crate::basics::{CardType, Color};
use crate::cards::{creature, helpers, mana_cost, spell, CardDb};
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};
use crate::subtypes::CreatureType;

pub const ELITE_INTERCEPTOR: u32 = 397;
pub const REJOINDER: u32 = 9724;

pub fn register(db: &mut CardDb) {
    let rejoinder = Effect::Sequence(vec![
        Effect::MayTapOrUntap {
            what: EffectTarget::Target(TargetSpec {
                kind: TargetKind::Creature(CardFilter::Any),
                min: 1,
                max: 1,
                distinct: true,
            }),
        },
        Effect::Draw { who: PlayerRef::Controller, count: ValueExpr::Fixed(1) },
    ]);
    db.insert(
        spell(REJOINDER, "Rejoinder", CardType::Sorcery, Color::White, mana_cost(1, &[(Color::White, 1)]), rejoinder)
            .with_text("You may tap or untap target creature. Draw a card."),
    );
    let mut front = creature(
        ELITE_INTERCEPTOR,
        "Elite Interceptor",
        &[CreatureType::Human, CreatureType::Wizard],
        Color::White,
        mana_cost(0, &[(Color::White, 1)]),
        1,
        2,
        helpers::enters_prepared(REJOINDER),
    );
    front.text = "This creature enters prepared. (While it's prepared, you may cast a copy of its spell. Doing so unprepares it.)\n// Rejoinder {1}{W} Sorcery — You may tap or untap target creature. Draw a card.".to_string();
    db.insert(front);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, ConfirmKind, DecisionRequest, DecisionResponse, PlayerView};
    use crate::basics::{Target, Zone};
    use crate::cards::{build_game, grp};
    use crate::effects::action::{ResolutionCtx, WbReason};
    use crate::ids::{PlayerId, StackId};
    use crate::priority::Engine;

    /// Opts in to the "may", then chooses **untap** (direction = false).
    struct UntapIt;
    impl Agent for UntapIt {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::Confirm { kind: ConfirmKind::MayEffect } => DecisionResponse::Bool(true),
                DecisionRequest::Confirm { kind: ConfirmKind::Generic } => DecisionResponse::Bool(false),
                _ => DecisionResponse::Pass,
            }
        }
    }

    #[test]
    fn rejoinder_untaps_a_tapped_creature_and_draws() {
        let mut state = build_game(1, &[&[grp::ISLAND], &[]]);
        let bears = {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            let id = state.add_card(PlayerId(0), c, Zone::Battlefield);
            state.objects.get_mut(&id).unwrap().status.tapped = true;
            id
        };
        let effect = state.card_db().get(REJOINDER).unwrap().spell_effect().unwrap().clone();
        let hand0 = state.player(PlayerId(0)).hand.len();
        let mut e = Engine::new(state, vec![Box::new(UntapIt), Box::new(UntapIt)]);
        e.resolve_effect(
            &effect,
            &ResolutionCtx {
                controller: Some(PlayerId(0)),
                chosen_targets: vec![Target::Object(bears)],
                ..Default::default()
            },
            WbReason::Resolve(StackId(0)),
        );
        assert!(!e.state.object(bears).status.tapped, "the tapped creature was untapped");
        assert_eq!(e.state.player(PlayerId(0)).hand.len(), hand0 + 1, "drew a card");
    }
}
