//! Goblin Glasswright // Craft with Pride — `{1}{R}` Creature — Goblin Sorcerer 2/2 // `{R}` Sorcery
//! (first printed SOS). A **Prepare** DFC whose back makes a Treasure token.
//!
//! Front oracle: "This creature enters prepared. (While it's prepared, you may cast a copy of its spell.
//! Doing so unprepares it.)"
//! Back oracle (Craft with Pride): "Create a Treasure token. (It's an artifact with '{T}, Sacrifice
//! this token: Add one mana of any color.')"
//!
//! **Implemented via the (B) exclude-from-autopay Treasure model** — the front is the usual enters-
//! prepared rails; the back is [`Effect::CreateToken`] of the shared [`helpers::treasure_token`] (a
//! colourless artifact whose ability comes from the registered [`grp::TREASURE_TOKEN`] def). The Treasure
//! is a real, sacrificeable artifact; its `{T}, Sacrifice:` mana ability is **cost-bearing**, so it is
//! usable only via **manual mana activation** (which pays the sacrifice through `pay_cost` and floats
//! the mana) and is kept out of the auto-pay pool.
//!
//! ⚠️ **GYM/AGENT-SEAT FLAG (option B):** agent/replay seats run with `manual_mana = false`, so they are
//! never offered `ActivateMana` for a Treasure — under (B) a Treasure is **inert in training** (it exists
//! as a sacrificeable artifact but can never be spent for mana by an auto-pay seat), and a spell
//! affordable ONLY via a Treasure is uncastable for the RL agent. This is an accepted first-pass limit:
//! the proper home for auto-spending non-tap mana sources (sac-for-mana, convoke-class, Phyrexian) is the
//! future **transactional-pending-cast** re-architecture (WHITEBOARD_MODEL §2.6), where all non-tap
//! payment choices become decisions in one payment flow — recorded there, not as a standalone TODO.

use crate::basics::{CardType, Color};
use crate::cards::{creature, helpers, mana_cost, spell, CardDb};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const GOBLIN_GLASSWRIGHT: u32 = 410;
/// The copy-only back-face spell (reserved 9700+ Prepare block).
pub const CRAFT_WITH_PRIDE: u32 = 9735;

pub fn register(db: &mut CardDb) {
    // Back face — "Craft with Pride" ({R} Sorcery): create one Treasure token.
    db.insert(
        spell(
            CRAFT_WITH_PRIDE,
            "Craft with Pride",
            CardType::Sorcery,
            Color::Red,
            mana_cost(0, &[(Color::Red, 1)]),
            Effect::CreateToken {
                spec: helpers::treasure_token(),
                count: ValueExpr::Fixed(1),
                controller: PlayerRef::Controller,
                dynamic_counters: vec![],
            },
        )
        .with_text("Create a Treasure token. (It's an artifact with \"{T}, Sacrifice this token: Add one mana of any color.\")"),
    );

    // Front face — the 2/2; enters prepared.
    let mut front = creature(
        GOBLIN_GLASSWRIGHT,
        "Goblin Glasswright",
        &[CreatureType::Goblin, CreatureType::Sorcerer],
        Color::Red,
        mana_cost(1, &[(Color::Red, 1)]),
        2,
        2,
        helpers::enters_prepared(CRAFT_WITH_PRIDE),
    );
    front.text = "This creature enters prepared. (While it's prepared, you may cast a copy of its spell. Doing so unprepares it.)\n// Craft with Pride {R} Sorcery — Create a Treasure token.".to_string();
    db.insert(front);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{AbilityRef, Agent, DecisionRequest, DecisionResponse, PlayableAction, PlayerView};
    use crate::basics::{Phase, Zone};
    use crate::cards::grp;
    use crate::ids::{ObjId, PlayerId};
    use crate::priority::Engine;
    use crate::state::GameState;

    fn db_with_card() -> CardDb {
        let mut db = crate::cards::starter_db();
        register(&mut db);
        db
    }

    fn add(state: &mut GameState, who: PlayerId, grp_id: u32, zone: Zone) -> ObjId {
        let c = state.card_db().get(grp_id).unwrap().chars.clone();
        state.add_card(who, c, zone)
    }

    #[test]
    fn goblin_glasswright_ir() {
        let db = db_with_card();
        let front = db.get(GOBLIN_GLASSWRIGHT).unwrap();
        assert!(matches!(
            front.abilities[0],
            crate::effects::ability::Ability::Prepare { spell: CRAFT_WITH_PRIDE }
        ));
        let back = db.get(CRAFT_WITH_PRIDE).unwrap();
        assert!(matches!(back.spell_effect(), Some(Effect::CreateToken { .. })));
        // The registered Treasure token def carries the cost-bearing mana ability.
        let treasure = db.get(grp::TREASURE_TOKEN).unwrap();
        assert_eq!(treasure.chars.card_types, vec![CardType::Artifact]);
        assert!(matches!(
            treasure.abilities[0],
            crate::effects::ability::Ability::Activated { is_mana: true, .. }
        ));
    }

    struct PassAgent;
    impl Agent for PassAgent {
        fn decide(&mut self, _v: &PlayerView, _req: &DecisionRequest) -> DecisionResponse {
            DecisionResponse::Pass
        }
    }

    /// Craft with Pride creates a Treasure token — a colourless artifact (no P/T) that is NOT counted by
    /// the auto-payer (it can't sac-for-mana), so `available_mana` is unchanged by its presence.
    #[test]
    fn craft_with_pride_makes_a_treasure_inert_to_autopay() {
        let mut state = GameState::new(2, 1);
        state.set_card_db(std::sync::Arc::new(db_with_card()));
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let effect = state.card_db().get(CRAFT_WITH_PRIDE).unwrap().spell_effect().unwrap().clone();
        let avail_before = crate::mana::available_mana(&state, PlayerId(0));
        let mut e = Engine::new(state, vec![Box::new(PassAgent), Box::new(PassAgent)]);
        e.resolve_effect(
            &effect,
            &crate::effects::action::ResolutionCtx { controller: Some(PlayerId(0)), ..Default::default() },
            crate::effects::action::WbReason::Resolve(crate::ids::StackId(99)),
        );
        let treasure = *e.state.player(PlayerId(0)).battlefield.last().unwrap();
        assert_eq!(e.state.object(treasure).chars.name, "Treasure");
        assert_eq!(e.state.object(treasure).chars.power, None, "a non-creature token has no P/T");
        assert_eq!(
            crate::mana::available_mana(&e.state, PlayerId(0)),
            avail_before,
            "the Treasure is NOT an auto-pay mana source (its sac-cost ability is manual-only)"
        );
    }

    /// Chooses red for any ChooseColor; passes otherwise.
    struct ChooseRed;
    impl Agent for ChooseRed {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::ChooseColor { allowed, .. } => {
                    let i = allowed.iter().position(|c| *c == Color::Red).unwrap_or(0);
                    DecisionResponse::Indices(vec![i as u32])
                }
                _ => DecisionResponse::Pass,
            }
        }
    }

    /// The headline (B) path end-to-end: with manual mana ON, the Treasure is offered as an
    /// `ActivateMana`; activating it sacrifices the token and floats one mana of the chosen colour
    /// (auto-pay never uses it); that floated mana then pays a `{R}` spell. Token gone, spell cast.
    #[test]
    fn manual_activation_sacrifices_treasure_then_pays_a_spell() {
        let mut state = GameState::new(2, 1);
        state.set_card_db(std::sync::Arc::new(db_with_card()));
        let treasure = add(&mut state, PlayerId(0), grp::TREASURE_TOKEN, Zone::Battlefield);
        // A {R} Raging Goblin in hand — castable ONLY via the Treasure (no lands in play).
        let goblin = add(&mut state, PlayerId(0), grp::RAGING_GOBLIN, Zone::Hand);
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let mut e = Engine::new(state, vec![Box::new(ChooseRed), Box::new(ChooseRed)]);
        e.set_manual_mana(PlayerId(0), true);

        // With no mana sources, the Goblin is NOT yet castable (the Treasure is inert to auto-pay).
        assert_eq!(crate::mana::available_mana(&e.state, PlayerId(0)), 0, "Treasure is not an auto-pay source");

        // With manual mana ON, the Treasure IS offered as a manual mana action.
        let ability = e
            .legal_actions(PlayerId(0))
            .into_iter()
            .find_map(|a| match a {
                PlayableAction::ActivateMana { source, ability } if source == treasure => Some(ability),
                _ => None,
            })
            .expect("Treasure offered as a manual ActivateMana");
        assert_ne!(ability, AbilityRef(u32::MAX), "names an authored mana ability, not intrinsic land mana");

        // Activate it: pay {T} + Sacrifice, float one red mana.
        e.activate_mana_ability(PlayerId(0), treasure, ability);
        assert!(e.state.player(PlayerId(0)).graveyard.contains(&treasure), "the Treasure was sacrificed to its graveyard");
        assert!(!e.state.player(PlayerId(0)).battlefield.contains(&treasure), "gone from the battlefield");
        assert_eq!(
            e.state.player(PlayerId(0)).mana_pool.amounts.get(&Color::Red).copied().unwrap_or(0),
            1,
            "one red mana floated from the sacrificed Treasure"
        );

        // Now the {R} Goblin is affordable off the floated mana — cast it and resolve.
        e.cast_spell(PlayerId(0), goblin, crate::agent::CastVariant::Normal);
        e.resolve_top();
        assert!(e.state.player(PlayerId(0)).battlefield.contains(&goblin), "the Goblin was cast and entered — paid by Treasure mana");
        assert_eq!(
            e.state.player(PlayerId(0)).mana_pool.amounts.get(&Color::Red).copied().unwrap_or(0),
            0,
            "the floated red mana was spent on the Goblin"
        );
    }
}
