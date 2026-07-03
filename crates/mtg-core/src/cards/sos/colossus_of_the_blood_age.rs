//! Colossus of the Blood Age — `{4}{R}{W}` Artifact Creature — Construct 6/6 (first printed SOS).
//!
//! Oracle: "When this creature enters, it deals 3 damage to each opponent and you gain 3 life. / When
//! this creature dies, discard any number of cards, then draw that many cards plus one."
//!
//! **Tracked-partial** (`.incomplete()`): the ETB — 3 damage to each opponent + gain 3 life — is
//! implemented. The dies clause ("discard any number, then draw that many **plus one**") is deferred:
//! the draw count is "cards discarded this resolution + 1", and the engine has no value for "how many
//! were discarded this resolution" yet. Omitted rather than shipped as a no-op husk.

use crate::basics::{CardType, Color, DamageKind};
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const COLOSSUS_OF_THE_BLOOD_AGE: u32 = 314;

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        COLOSSUS_OF_THE_BLOOD_AGE,
        "Colossus of the Blood Age",
        &[CreatureType::Construct],
        Color::Red,
        mana_cost(4, &[(Color::Red, 1), (Color::White, 1)]),
        6,
        6,
        vec![Ability::Triggered {
            event: EventPattern::SelfEnters,
            condition: None,
            intervening_if: false,
            effect: Effect::Sequence(vec![
                Effect::DealDamage {
                    amount: ValueExpr::Fixed(3),
                    to: EffectTarget::Player(PlayerRef::EachOpponent),
                    kind: DamageKind::Noncombat,
                },
                Effect::GainLife { who: PlayerRef::Controller, amount: ValueExpr::Fixed(3) },
            ]),
        }],
    );
    def.chars.card_types = vec![CardType::Artifact, CardType::Creature];
    def.chars.colors = vec![Color::Red, Color::White];
    def.text = "When this creature enters, it deals 3 damage to each opponent and you gain 3 life.\nWhen this creature dies, discard any number of cards, then draw that many cards plus one.".to_string();
    db.insert(def.incomplete());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colossus_shape() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(COLOSSUS_OF_THE_BLOOD_AGE).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Artifact, CardType::Creature]);
        assert_eq!(def.chars.colors, vec![Color::Red, Color::White]);
        assert!(!def.fully_implemented, "dies-clause discard/draw deferred");
    }

    /// Behaviour: the ETB deals 3 to the opponent and gains its controller 3 life.
    #[test]
    fn colossus_etb_drains_opponent_and_gains_life() {
        use crate::agent::RandomAgent;
        use crate::basics::Zone;
        use crate::cards::build_game;
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        let mut state = build_game(1, &[&[], &[]]);
        let src = state.add_card(PlayerId(0), state.card_db().get(COLOSSUS_OF_THE_BLOOD_AGE).unwrap().chars.clone(), Zone::Battlefield);
        let etb = match &state.card_db().get(COLOSSUS_OF_THE_BLOOD_AGE).unwrap().abilities[0] {
            Ability::Triggered { effect, .. } => effect.clone(),
            o => panic!("expected ETB, got {o:?}"),
        };
        let (my_life, opp_life) = (state.player(PlayerId(0)).life, state.player(PlayerId(1)).life);
        let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
        e.resolve_effect(
            &etb,
            &ResolutionCtx { controller: Some(PlayerId(0)), source: Some(src), ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        assert_eq!(e.state.player(PlayerId(1)).life, opp_life - 3, "3 damage to the opponent");
        assert_eq!(e.state.player(PlayerId(0)).life, my_life + 3, "gained 3 life");
    }
}
