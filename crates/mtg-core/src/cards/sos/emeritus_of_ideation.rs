//! Emeritus of Ideation // Ancestral Recall — `{3}{U}{U}` Creature — Human Wizard 5/5 // `{U}` Instant
//! (first printed SOS). A **Prepare** DFC — enters prepared, and re-prepares on attack by exiling from
//! the graveyard.
//!
//! Front: "This creature enters prepared. Whenever this creature attacks, you may exile eight cards
//! from your graveyard. If you do, this creature becomes prepared."
//! Back (Ancestral Recall): "Target player draws three cards."
//!
//! **Fully implemented** — enters-prepared plus a `SelfAttacks` trigger whose effect is a
//! `MayPayCost { exile 8 from your graveyard → BecomePrepared }` (the "you may pay …; if you do, …"
//! leaf). The back is Ancestral Recall (a target player draws three).

use crate::basics::{CardType, Color};
use crate::cards::{creature, helpers, mana_cost, spell, CardDb};
use crate::effects::ability::{Ability, Cost, CostComponent, EventPattern};
use crate::effects::target::{CardFilter, PlayerFilter, SelectSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;
use crate::subtypes::CreatureType;

pub const EMERITUS_OF_IDEATION: u32 = 393;
pub const ANCESTRAL_RECALL: u32 = 9720;

pub fn register(db: &mut CardDb) {
    let ancestral_recall = Effect::Sequence(vec![
        Effect::TargetPlayer(PlayerFilter::Any),
        Effect::Draw { who: PlayerRef::ChosenTarget(0), count: ValueExpr::Fixed(3) },
    ]);
    db.insert(
        spell(ANCESTRAL_RECALL, "Ancestral Recall", CardType::Instant, Color::Blue, mana_cost(0, &[(Color::Blue, 1)]), ancestral_recall)
            .with_text("Target player draws three cards."),
    );

    let mut abilities = helpers::enters_prepared(ANCESTRAL_RECALL);
    abilities.push(Ability::Triggered {
        event: EventPattern::SelfAttacks,
        condition: None,
        intervening_if: false,
        effect: Effect::MayPayCost {
            cost: Cost {
                mana: None,
                components: vec![CostComponent::Exile(SelectSpec {
                    zone: crate::basics::Zone::Graveyard,
                    filter: CardFilter::Any,
                    chooser: PlayerRef::Controller,
                    min: ValueExpr::Fixed(8),
                    max: ValueExpr::Fixed(8),
                })],
            },
            then: Box::new(Effect::BecomePrepared),
        },
    });
    let mut front = creature(
        EMERITUS_OF_IDEATION,
        "Emeritus of Ideation",
        &[CreatureType::Human, CreatureType::Wizard],
        Color::Blue,
        mana_cost(3, &[(Color::Blue, 2)]),
        5,
        5,
        abilities,
    );
    front.text = "This creature enters prepared. Whenever this creature attacks, you may exile eight cards from your graveyard. If you do, this creature becomes prepared. (While it's prepared, you may cast a copy of its spell. Doing so unprepares it.)\n// Ancestral Recall {U} Instant — Target player draws three cards.".to_string();
    db.insert(front);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::RandomAgent;
    use crate::cards::{build_game, grp};
    use crate::effects::action::{ResolutionCtx, WbReason};
    use crate::ids::{PlayerId, StackId};
    use crate::priority::Engine;

    #[test]
    fn emeritus_of_ideation_ir_and_ancestral_recall() {
        let mut db = CardDb::default();
        register(&mut db);
        let f = db.get(EMERITUS_OF_IDEATION).unwrap();
        assert!(matches!(f.abilities[0], Ability::Prepare { spell: ANCESTRAL_RECALL }));
        assert!(matches!(f.abilities[1], Ability::Triggered { event: EventPattern::SelfEnters, .. }));
        assert!(matches!(
            f.abilities[2],
            Ability::Triggered { event: EventPattern::SelfAttacks, .. }
        ));
        // Behaviour: Ancestral Recall makes the targeted player draw three.
        let state = build_game(1, &[&[grp::ISLAND, grp::ISLAND, grp::ISLAND], &[]]);
        let effect = state.card_db().get(ANCESTRAL_RECALL).unwrap().spell_effect().unwrap().clone();
        let hand0 = state.player(PlayerId(0)).hand.len();
        let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
        e.resolve_effect(
            &effect,
            &ResolutionCtx {
                controller: Some(PlayerId(0)),
                chosen_targets: vec![crate::basics::Target::Player(PlayerId(0))],
                ..Default::default()
            },
            WbReason::Resolve(StackId(0)),
        );
        assert_eq!(e.state.player(PlayerId(0)).hand.len(), hand0 + 3, "target player drew three");
    }
}
