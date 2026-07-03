//! Burrog Banemaker — `{B}` Creature — Frog Warlock 1/1 (first printed SOS).
//!
//! Oracle: "Deathtouch / {1}{B}: This creature gets +1/+1 until end of turn."
//!
//! **Fully implemented** — printed Deathtouch plus a `{1}{B}` activated pump (+1/+1 until end of
//! turn) on itself (`SourceSelf`), any number of times.

use crate::basics::Color;
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, Cost, Keyword, Timing};
use crate::effects::condition::Duration;
use crate::effects::value::ValueExpr;
use crate::effects::{Effect, EffectTarget};
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const BURROG_BANEMAKER: u32 = 226;

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        BURROG_BANEMAKER,
        "Burrog Banemaker",
        &[CreatureType::Frog, CreatureType::Warlock],
        Color::Black,
        mana_cost(0, &[(Color::Black, 1)]),
        1,
        1,
        vec![Ability::Activated {
            cost: Cost {
                mana: Some(mana_cost(1, &[(Color::Black, 1)])),
                components: vec![],
            },
            effect: Effect::PumpPT {
                what: EffectTarget::SourceSelf,
                power: ValueExpr::Fixed(1),
                toughness: ValueExpr::Fixed(1),
                duration: Duration::UntilEndOfTurn,
            },
            timing: Timing::Instant,
            restriction: None,
            is_mana: false,
        }],
    );
    def.chars.keywords = vec![Keyword::Deathtouch];
    def.text = "Deathtouch\n{1}{B}: This creature gets +1/+1 until end of turn.".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn burrog_banemaker_shape() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(BURROG_BANEMAKER).unwrap();
        assert_eq!(def.chars.power, Some(1));
        assert_eq!(def.chars.keywords, vec![Keyword::Deathtouch]);
        assert!(!def.is_mana_source());
        assert!(def.fully_implemented);
    }

    /// Behaviour: activating the pump effect makes the 1/1 a 2/2 (until EOT).
    #[test]
    fn burrog_banemaker_pumps_itself() {
        use crate::agent::RandomAgent;
        use crate::basics::Zone;
        use crate::cards::build_game;
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        let mut state = build_game(1, &[&[], &[]]);
        let chars = state.card_db().get(BURROG_BANEMAKER).unwrap().chars.clone();
        let src = state.add_card(PlayerId(0), chars, Zone::Battlefield);
        let pump = match &state.card_db().get(BURROG_BANEMAKER).unwrap().abilities[0] {
            Ability::Activated { effect, .. } => effect.clone(),
            o => panic!("expected Activated pump, got {o:?}"),
        };
        let mut e = Engine::new(
            state,
            vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))],
        );
        assert_eq!(e.state.computed(src).power, Some(1));
        e.resolve_effect(
            &pump,
            &ResolutionCtx { controller: Some(PlayerId(0)), source: Some(src), ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        assert_eq!(e.state.computed(src).power, Some(2), "+1/+1 → 2/2");
    }
}
