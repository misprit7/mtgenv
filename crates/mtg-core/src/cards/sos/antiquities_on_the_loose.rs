//! Antiquities on the Loose — `{1}{W}{W}` Sorcery (first printed SOS).
//!
//! Oracle: "Create two 2/2 red and white Spirit creature tokens. Then if this spell was cast from
//! anywhere other than your hand, put a +1/+1 counter on each Spirit you control. / Flashback {4}{W}{W}"
//!
//! **Fully implemented** — the first S10 flashback FRONT-side cap consumer: a `Sequence` of a
//! `CreateToken` (two `spirit_token()`s) then a `Conditional` on the new `Condition::CastFromNotHand`
//! (reads the spell's `flashback_cast` flag) whose `then` is a `ForEach` over "each Spirit you
//! control" putting a +1/+1 counter on each. `Ability::Flashback {4}{W}{W}` casts it from the
//! graveyard (a non-hand zone → the counter rider fires), then exiles it.

use crate::basics::{CardType, Color, CounterKind, Zone};
use crate::cards::helpers::spirit_token;
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::ability::Ability;
use crate::effects::condition::Condition;
use crate::effects::target::{CardFilter, SelectSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const ANTIQUITIES_ON_THE_LOOSE: u32 = 335;

pub fn register(db: &mut CardDb) {
    let effect = Effect::Sequence(vec![
        Effect::CreateToken {
            spec: spirit_token(),
            count: ValueExpr::Fixed(2),
            controller: PlayerRef::Controller,
            dynamic_counters: Vec::new(),
        },
        Effect::Conditional {
            cond: Condition::CastFromNotHand,
            then: Box::new(Effect::ForEach {
                // "each Spirit you control" — all of them (max large so it takes them all silently).
                selector: SelectSpec {
                    zone: Zone::Battlefield,
                    filter: CardFilter::All(vec![
                        CardFilter::HasSubtype(CreatureType::Spirit.into()),
                        CardFilter::ControlledBy(PlayerRef::Controller),
                    ]),
                    chooser: PlayerRef::Controller,
                    min: ValueExpr::Fixed(0),
                    max: ValueExpr::Fixed(999),
                },
                body: Box::new(Effect::PutCounters {
                    what: EffectTarget::Each,
                    kind: CounterKind::PlusOnePlusOne,
                    n: ValueExpr::Fixed(1),
                }),
            }),
            otherwise: None,
        },
    ]);
    let mut def = spell(
        ANTIQUITIES_ON_THE_LOOSE,
        "Antiquities on the Loose",
        CardType::Sorcery,
        Color::White,
        mana_cost(1, &[(Color::White, 2)]),
        effect,
    )
    .with_text("Create two 2/2 red and white Spirit creature tokens. Then if this spell was cast from anywhere other than your hand, put a +1/+1 counter on each Spirit you control.\nFlashback {4}{W}{W}");
    def.abilities.push(Ability::Flashback { cost: mana_cost(4, &[(Color::White, 2)]) });
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView};
    use crate::basics::Zone;
    use crate::cards::{build_game, grp};
    use crate::ids::{PlayerId, StackId};
    use crate::priority::Engine;
    use crate::stack::{StackObject, StackObjectKind};

    #[derive(Clone)]
    struct Passer;
    impl Agent for Passer {
        fn decide(&mut self, _v: &PlayerView, _r: &DecisionRequest) -> DecisionResponse {
            DecisionResponse::Pass
        }
    }

    #[test]
    fn antiquities_shape() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(ANTIQUITIES_ON_THE_LOOSE).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Sorcery]);
        assert!(def.fully_implemented);
        assert!(
            def.abilities.iter().any(|a| matches!(a, Ability::Flashback { .. })),
            "declares Flashback"
        );
    }

    /// Resolve the spell with `flashback_cast` set, and return (number of Spirit tokens P0 controls,
    /// the +1/+1 counters on the FIRST such Spirit). Drives the real spell resolution (`resolve_top`).
    fn resolve(flashback_cast: bool) -> (usize, u32) {
        let mut state = build_game(1, &[&[], &[]]);
        let card = state.add_card(
            PlayerId(0),
            state.card_db().get(ANTIQUITIES_ON_THE_LOOSE).unwrap().chars.clone(),
            Zone::Stack,
        );
        state.objects.get_mut(&card).unwrap().flashback_cast = flashback_cast;
        let sid = StackId(700);
        state.stack.push(StackObject {
            id: sid,
            controller: PlayerId(0),
            source: None,
            kind: StackObjectKind::Spell(card),
            targets: Vec::new(),
            x: None,
            modes: Vec::new(),
        });
        let mut e = Engine::new(state, vec![Box::new(Passer), Box::new(Passer)]);
        let _ = grp::PLAINS;
        e.resolve_top();
        let spirits: Vec<_> = e
            .state
            .player(PlayerId(0))
            .battlefield
            .iter()
            .copied()
            .filter(|&o| e.state.object(o).chars.subtypes.contains(&CreatureType::Spirit.into()))
            .collect();
        let counters = spirits
            .first()
            .map(|&o| e.state.object(o).counters.get(&CounterKind::PlusOnePlusOne))
            .unwrap_or(0);
        (spirits.len(), counters)
    }

    /// Cast from hand (`flashback_cast == false`): two Spirits, NO counters.
    #[test]
    fn from_hand_makes_two_bare_spirits() {
        let (n, counters) = resolve(false);
        assert_eq!(n, 2, "two Spirit tokens");
        assert_eq!(counters, 0, "cast from hand → no +1/+1 counter rider");
    }

    /// Flashback-cast (from the graveyard, a non-hand zone): two Spirits, each with a +1/+1 counter.
    #[test]
    fn flashback_puts_a_counter_on_each_spirit() {
        let (n, counters) = resolve(true);
        assert_eq!(n, 2, "two Spirit tokens");
        assert_eq!(counters, 1, "cast from a non-hand zone → +1/+1 counter on each Spirit you control");
    }
}
