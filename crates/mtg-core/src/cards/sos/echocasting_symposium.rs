//! Echocasting Symposium — `{4}{U}{U}` Sorcery — Lesson (first printed SOS).
//!
//! Oracle: "Target player creates a token that's a copy of target creature you control. Paradigm
//! (Then exile this spell. After you first resolve a spell with this name, you may cast a copy of it
//! from exile without paying its mana cost at the beginning of each of your first main phases.)"
//!
//! **Fully implemented.** Two targets: slot 0 = a target player (who gets the token), slot 1 = a
//! creature you control (the copy source). `CreateTokenCopy` with `controller = ChosenTarget(0)`
//! (the target player) and `source = ` the target creature. **Paradigm** is the shared bundle from
//! [`crate::cards::helpers::paradigm_abilities`].

use crate::basics::{CardType, Color};
use crate::cards::{helpers, mana_cost, spell, CardDb};
use crate::effects::target::{CardFilter, PlayerFilter, TargetKind, TargetSpec, TokenCopyMods};
use crate::effects::value::PlayerRef;
use crate::effects::{Effect, EffectTarget};
use crate::subtypes::{SpellType, Subtype};

/// grp id (per-set ids live near their cards).
pub const ECHOCASTING_SYMPOSIUM: u32 = 371;

pub fn register(db: &mut CardDb) {
    let effect = Effect::Sequence(vec![
        // slot 0 — the target player who creates the token.
        Effect::TargetPlayer(PlayerFilter::Any),
        // slot 1 — a creature you control; that player makes a token copy of it (CR 707.9e).
        Effect::CreateTokenCopy {
            source: EffectTarget::Target(TargetSpec {
                kind: TargetKind::Creature(CardFilter::ControlledBy(PlayerRef::Controller)),
                min: 1,
                max: 1,
                distinct: true,
            }),
            controller: PlayerRef::ChosenTarget(0),
            mods: TokenCopyMods::default(),
        },
    ]);
    let mut def = spell(
        ECHOCASTING_SYMPOSIUM,
        "Echocasting Symposium",
        CardType::Sorcery,
        Color::Blue,
        mana_cost(4, &[(Color::Blue, 2)]),
        effect,
    )
    .with_text(
        "Target player creates a token that's a copy of target creature you control. Paradigm (Then exile this spell. After you first resolve a spell with this name, you may cast a copy of it from exile without paying its mana cost at the beginning of each of your first main phases.)",
    );
    def.chars.subtypes = vec![Subtype::Spell(SpellType::Lesson)];
    def.abilities.extend(helpers::paradigm_abilities());
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, CastVariant, DecisionRequest, DecisionResponse, PlayerView};
    use crate::basics::{Phase, Zone};
    use crate::cards::grp;
    use crate::ids::PlayerId;
    use crate::priority::Engine;
    use crate::state::GameState;

    fn db_with_card() -> CardDb {
        let mut db = crate::cards::starter_db();
        register(&mut db);
        db
    }

    #[test]
    fn echocasting_symposium_shape() {
        let db = db_with_card();
        let def = db.get(ECHOCASTING_SYMPOSIUM).unwrap();
        assert_eq!(def.chars.subtypes, vec![Subtype::Spell(SpellType::Lesson)]);
        assert!(def.fully_implemented);
        assert!(matches!(def.abilities.get(1), Some(crate::effects::ability::Ability::Paradigm)));
    }

    /// Picks target-slot candidate 0 for every ChooseTargets (self as the player, your creature as the
    /// copy source); passes otherwise.
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

    /// Cast it targeting yourself + your Grizzly Bears: you get a token copy of the Bears, and the
    /// Lesson exiles itself (Paradigm).
    #[test]
    fn makes_a_token_copy_then_self_exiles() {
        let mut state = GameState::new(2, 1);
        state.set_card_db(std::sync::Arc::new(db_with_card()));
        let card = {
            let c = state.card_db().get(ECHOCASTING_SYMPOSIUM).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Hand)
        };
        let bears = {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        for _ in 0..6 {
            let i = state.card_db().get(grp::ISLAND).unwrap().chars.clone();
            state.add_card(PlayerId(0), i, Zone::Battlefield);
        }
        let mut e = Engine::new(state, vec![Box::new(PickFirst), Box::new(PickFirst)]);
        e.state.phase = Phase::PrecombatMain;
        let bf_before = e.state.player(PlayerId(0)).battlefield.len();

        e.cast_spell(PlayerId(0), card, CastVariant::Normal);
        e.resolve_top();

        assert_eq!(
            e.state.player(PlayerId(0)).battlefield.len(),
            bf_before + 1,
            "a token copy entered your battlefield"
        );
        // The new token is a copy of the Bears (same name, 2/2), and is distinct from the original.
        let token = e
            .state
            .player(PlayerId(0))
            .battlefield
            .iter()
            .copied()
            .find(|&id| id != bears && e.state.object(id).chars.name == "Grizzly Bears")
            .expect("a Grizzly Bears token copy");
        assert_eq!(e.state.object(token).chars.power, Some(2), "copy is a 2/2");
        assert!(
            e.state.player(PlayerId(0)).exile.contains(&card),
            "Paradigm exiled the Lesson instead of the graveyard"
        );
    }
}
