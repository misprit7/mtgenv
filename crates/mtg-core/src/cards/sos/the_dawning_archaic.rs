//! The Dawning Archaic — `{10}` Legendary Creature — Avatar 7/7, Reach (first printed SOS).
//!
//! Oracle: "This spell costs {1} less to cast for each instant and sorcery card in your graveyard.
//! Reach. Whenever The Dawning Archaic attacks, you may cast target instant or sorcery card from your
//! graveyard without paying its mana cost. If that spell would be put into your graveyard, exile it
//! instead."
//!
//! **Fully implemented.** Three pieces:
//! - **Cost reduction** — `CostReduction{ GenericValue(Count{ I/S in your graveyard }), Always, Cast }`.
//! - **Reach** — a keyword.
//! - **Attack trigger** — `SelfAttacks` → [`Effect::CastForFree`] on an up-to-one target (CR "you may
//!   cast target …"), casting the actual graveyard card for {0} and flagging it to be **exiled as it
//!   leaves the stack** (`exile_on_leave`, reusing the flashback exile path) — the "exile it instead"
//!   rider.

use crate::basics::{CardType, Color, Zone};
use crate::cards::{creature, helpers, mana_cost, CardDb};
use crate::effects::ability::{
    Ability, CostReductionAmount, CostReductionCondition, CostReductionScope, EventPattern, Keyword,
};
use crate::effects::condition::Condition;
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};
use crate::subtypes::{CreatureType, Supertype};

/// grp id (per-set ids live near their cards).
pub const THE_DAWNING_ARCHAIC: u32 = 369;

pub fn register(db: &mut CardDb) {
    // "{1} less for each instant and sorcery card in your graveyard" — an unconditional reduction
    // (Always) whose amount is that count (0 when the graveyard has none).
    let is_in_gy = ValueExpr::Count {
        zone: Zone::Graveyard,
        filter: helpers::instant_or_sorcery(),
        controller: Some(PlayerRef::Controller),
    };
    let cost_reduction = Ability::CostReduction {
        amount: CostReductionAmount::GenericValue(is_in_gy),
        condition: CostReductionCondition::State(Condition::Always),
        scope: CostReductionScope::Cast,
    };
    // "Whenever this attacks, you may cast target I/S card from your graveyard without paying its mana
    // cost. If that spell would be put into your graveyard, exile it instead." Modeled as an up-to-one
    // target (declining = choosing no target); `exile_on_leave` grants the flashback-style exile rider.
    let attack_trigger = Ability::Triggered {
        event: EventPattern::SelfAttacks,
        condition: None,
        intervening_if: false,
        effect: Effect::CastForFree {
            what: EffectTarget::Target(TargetSpec {
                kind: TargetKind::CardInZone {
                    zone: Zone::Graveyard,
                    filter: CardFilter::All(vec![
                        helpers::instant_or_sorcery(),
                        CardFilter::ControlledBy(PlayerRef::Controller),
                    ]),
                },
                min: 0,
                max: 1,
                distinct: true,
            }),
            exile_on_leave: true,
        },
    };
    let mut def = creature(
        THE_DAWNING_ARCHAIC,
        "The Dawning Archaic",
        &[CreatureType::Avatar],
        Color::Colorless,
        mana_cost(10, &[]),
        7,
        7,
        vec![cost_reduction, attack_trigger],
    )
    .with_text(
        "This spell costs {1} less to cast for each instant and sorcery card in your graveyard.\nReach\nWhenever The Dawning Archaic attacks, you may cast target instant or sorcery card from your graveyard without paying its mana cost. If that spell would be put into your graveyard, exile it instead.",
    );
    def.chars.supertypes = vec![Supertype::Legendary];
    def.chars.keywords = vec![Keyword::Reach];
    def.chars.colors = vec![]; // colorless (CR 105.2c)
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, DecisionRequest, DecisionResponse, GameEvent, PlayerView};
    use crate::basics::ManaCost;
    use crate::cards::{grp, spell};
    use crate::ids::PlayerId;
    use crate::priority::Engine;
    use crate::state::GameState;

    fn db_with_card() -> CardDb {
        let mut db = crate::cards::starter_db();
        register(&mut db);
        // A simple untargeted I/S in the graveyard to free-cast: "Test Recall" {3} sorcery, draw 1.
        db.insert(spell(
            9970,
            "Test Recall",
            CardType::Sorcery,
            Color::Blue,
            ManaCost { generic: 3, ..Default::default() },
            Effect::Draw { who: PlayerRef::Controller, count: ValueExpr::Fixed(1) },
        ));
        db
    }

    #[test]
    fn dawning_archaic_shape() {
        let db = db_with_card();
        let def = db.get(THE_DAWNING_ARCHAIC).unwrap();
        assert_eq!(def.chars.power, Some(7));
        assert!(def.chars.colors.is_empty(), "colorless");
        assert!(def.chars.supertypes.contains(&Supertype::Legendary));
        assert!(def.chars.keywords.contains(&Keyword::Reach));
        assert!(def.fully_implemented);
    }

    /// Picks target-slot-0 candidate 0 (the sole gy sorcery) for every ChooseTargets; passes otherwise.
    struct PickFirst;
    impl Agent for PickFirst {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::ChooseTargets { slots, .. } => DecisionResponse::Pairs(
                    slots.iter().enumerate().map(|(si, _)| (si as u32, 0u32)).collect(),
                ),
                _ => DecisionResponse::Pass,
            }
        }
    }

    /// Attack → the trigger casts the graveyard sorcery for free, it resolves (draw 1), then the
    /// "exile it instead" rider sends it to **exile** rather than the graveyard.
    #[test]
    fn attack_free_casts_a_graveyard_spell_that_then_exiles() {
        let mut state = GameState::new(2, 1);
        state.set_card_db(std::sync::Arc::new(db_with_card()));
        let archaic = {
            let c = state.card_db().get(THE_DAWNING_ARCHAIC).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        let recall = {
            let c = state.card_db().get(9970).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Graveyard)
        };
        // A card in the library for the free-cast Recall to draw.
        let lib = state.card_db().get(grp::FOREST).unwrap().chars.clone();
        state.add_card(PlayerId(0), lib, Zone::Library);
        let mut e = Engine::new(state, vec![Box::new(PickFirst), Box::new(PickFirst)]);
        let hand_before = e.state.player(PlayerId(0)).hand.len();

        // The Archaic attacks → its SelfAttacks trigger fires.
        e.broadcast(GameEvent::AttackersDeclared { attackers: vec![archaic], by: PlayerId(0) });
        e.run_agenda(); // put the trigger on the stack, targeting the gy sorcery
        e.resolve_top(); // resolve the trigger → free-cast the gy sorcery
        e.run_agenda();
        e.resolve_top(); // resolve the free-cast sorcery → draw 1, then it leaves the stack

        assert_eq!(
            e.state.player(PlayerId(0)).hand.len(),
            hand_before + 1,
            "the free-cast graveyard sorcery resolved (drew a card)"
        );
        assert_eq!(
            e.state.object(recall).zone,
            Zone::Exile,
            "the 'exile it instead' rider sent it to exile, not the graveyard"
        );
        assert!(
            !e.state.player(PlayerId(0)).graveyard.contains(&recall),
            "it is not in the graveyard"
        );
    }
}
