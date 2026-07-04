//! Potioner's Trove — `{3}` Artifact (first printed SOS).
//!
//! Oracle: "{T}: Add one mana of any color. / {T}: You gain 2 life. Activate this ability only if
//! you've cast an instant or sorcery spell this turn."
//!
//! **Fully implemented** — a colorless artifact with two `{T}` activated abilities:
//! 1. a mana ability adding one mana of **any color** (`ManaSpec { any_color: Some(1) }`, resolved by
//!    the whiteboard asking the controller which colour).
//! 2. `GainLife 2`, gated by `Restriction::OnlyIf(CastInstantOrSorceryThisTurn)` — the new S22
//!    per-player counter (`Player.instants_sorceries_cast_this_turn`, incremented in `cast_spell`,
//!    reset each turn) read via the activation legality gate (which now honours `OnlyIf` for
//!    non-mana activated abilities, not just mana abilities).

use crate::cards::{artifact, mana_cost, CardDb};
use crate::effects::ability::{Ability, Cost, CostComponent, Restriction, Timing};
use crate::effects::condition::Condition;
use crate::effects::target::ManaSpec;
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;

/// grp id (per-set ids live near their cards).
pub const POTIONERS_TROVE: u32 = 349;

pub fn register(db: &mut CardDb) {
    let mut def = artifact(
        POTIONERS_TROVE,
        "Potioner's Trove",
        mana_cost(3, &[]),
        vec![
            // {T}: Add one mana of any color.
            Ability::Activated {
                cost: Cost { mana: None, components: vec![CostComponent::TapSelf] },
                effect: Effect::AddMana {
                    who: PlayerRef::Controller,
                    mana: ManaSpec {
                        produces: vec![],
                        any_color: Some(ValueExpr::Fixed(1)),
                        restriction: None,
                    },
                },
                timing: Timing::Instant,
                restriction: None,
                is_mana: true,
            },
            // {T}: You gain 2 life. Activate only if you've cast an instant or sorcery this turn.
            Ability::Activated {
                cost: Cost { mana: None, components: vec![CostComponent::TapSelf] },
                effect: Effect::GainLife { who: PlayerRef::Controller, amount: ValueExpr::Fixed(2) },
                timing: Timing::Instant,
                restriction: Some(Restriction::OnlyIf(Condition::CastInstantOrSorceryThisTurn {
                    who: PlayerRef::Controller,
                })),
                is_mana: false,
            },
        ],
    );
    def.text = "{T}: Add one mana of any color.\n{T}: You gain 2 life. Activate this ability only if you've cast an instant or sorcery spell this turn.".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{PlayableAction, RandomAgent};
    use crate::basics::Zone;
    use crate::cards::starter_db;
    use crate::ids::{ObjId, PlayerId};
    use crate::priority::Engine;
    use crate::state::GameState;
    use std::sync::Arc;

    fn engine_with_trove(cast_is: bool) -> (Engine, ObjId) {
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(starter_db()));
        let trove = {
            let c = state.card_db().get(POTIONERS_TROVE).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        if cast_is {
            state.players[0].instants_sorceries_cast_this_turn = 1;
        }
        state.active_player = PlayerId(0);
        (Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]), trove)
    }

    #[test]
    fn potioners_trove_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(POTIONERS_TROVE).unwrap();
        assert!(def.chars.colors.is_empty(), "artifacts are colorless");
        assert!(def.fully_implemented);
        assert!(matches!(&def.abilities[0], Ability::Activated { is_mana: true, .. }));
        assert!(matches!(
            &def.abilities[1],
            Ability::Activated { restriction: Some(Restriction::OnlyIf(_)), is_mana: false, .. }
        ));
    }

    /// The gain-life ability is offered iff the controller has cast an I/S this turn (the OnlyIf gate,
    /// now honoured for non-mana activated abilities). Proves S22 end-to-end through the legality path.
    #[test]
    fn gain_life_gated_on_cast_this_turn() {
        for cast_is in [false, true] {
            let (e, trove) = engine_with_trove(cast_is);
            let actions = e.legal_actions(PlayerId(0));
            let gain_life_offered = actions.iter().any(|a| matches!(
                a,
                PlayableAction::Activate { source, ability } if *source == trove && ability.0 == 1
            ));
            assert_eq!(gain_life_offered, cast_is, "gain-life offered iff an I/S was cast this turn");
        }
    }
}
