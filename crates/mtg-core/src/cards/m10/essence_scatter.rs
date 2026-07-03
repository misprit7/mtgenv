//! Essence Scatter — `{1}{U}` Instant (first printed M10, Magic 2010; reprinted in SOS).
//!
//! Oracle: "Counter target creature spell."
//!
//! **Fully implemented** — `Effect::Counter` over one declared "target creature spell" (a
//! `TargetKind::StackObject` restricted to creature spells). Exercises the `Counter` effect leaf: a
//! countered spell leaves the stack and goes to its owner's graveyard (CR 701.5a). A creature spell
//! with `CantBeCountered` (Surrak, Elusive Hunter) is left on the stack (CR 701.5f).

use crate::basics::{CardType, Color};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::{Effect, EffectTarget};

/// grp id (per-set ids live near their cards).
pub const ESSENCE_SCATTER: u32 = 217;

pub fn register(db: &mut CardDb) {
    let effect = Effect::Counter {
        what: EffectTarget::Target(TargetSpec {
            kind: TargetKind::StackObject(CardFilter::HasCardType(CardType::Creature)),
            min: 1,
            max: 1,
            distinct: true,
        }),
    };
    db.insert(
        spell(
            ESSENCE_SCATTER,
            "Essence Scatter",
            CardType::Instant,
            Color::Blue,
            mana_cost(1, &[(Color::Blue, 1)]),
            effect,
        )
        .with_text("Counter target creature spell."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn essence_scatter_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(ESSENCE_SCATTER).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Instant]);
        assert!(def.fully_implemented);
        expect![[r#"
            Counter {
                what: Target(
                    TargetSpec {
                        kind: StackObject(
                            HasCardType(
                                Creature,
                            ),
                        ),
                        min: 1,
                        max: 1,
                        distinct: true,
                    },
                ),
            }"#]].assert_eq(&format!("{:#?}", def.spell_effect().unwrap()));
    }

    /// Put `card`'s object on the stack as a spell and return its `StackId`.
    fn put_spell_on_stack(
        state: &mut crate::state::GameState,
        card: crate::ids::ObjId,
        controller: crate::ids::PlayerId,
        id: u64,
    ) -> crate::ids::StackId {
        use crate::stack::{StackObject, StackObjectKind};
        let sid = crate::ids::StackId(id);
        state.stack.push(StackObject {
            id: sid,
            controller,
            source: None,
            kind: StackObjectKind::Spell(card),
            targets: vec![],
            x: None,
            modes: Vec::new(),
        });
        sid
    }

    /// Behaviour: Essence Scatter counters a creature spell — the spell leaves the stack and its
    /// card goes to its owner's graveyard.
    #[test]
    fn essence_scatter_counters_a_creature_spell() {
        use crate::agent::RandomAgent;
        use crate::basics::{Target, Zone};
        use crate::cards::{build_game, grp};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        let mut state = build_game(1, &[&[], &[]]);
        // P1 casts a Grizzly Bears (a creature spell on the stack).
        let bears = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
        let bears_obj = state.add_card(PlayerId(1), bears, Zone::Stack);
        let sid = put_spell_on_stack(&mut state, bears_obj, PlayerId(1), 1);
        let effect = state.card_db().get(ESSENCE_SCATTER).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(
            state,
            vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))],
        );
        e.resolve_effect(
            &effect,
            &ResolutionCtx {
                controller: Some(PlayerId(0)),
                chosen_targets: vec![Target::Stack(sid)],
                ..Default::default()
            },
            WbReason::Resolve(StackId(99)),
        );
        assert!(e.state.stack.items.iter().all(|s| s.id != sid), "the creature spell left the stack");
        assert!(e.state.players[1].graveyard.contains(&bears_obj), "countered → owner's graveyard");
    }

    /// Behaviour (closes the Surrak deferral): Essence Scatter targeting Surrak, Elusive Hunter —
    /// which carries the stack-zone `CantBeCountered` static — leaves it on the stack (CR 701.5f).
    #[test]
    fn essence_scatter_cannot_counter_surrak() {
        use crate::agent::RandomAgent;
        use crate::basics::{Target, Zone};
        use crate::cards::build_game;
        use crate::cards::tdm::surrak_elusive_hunter::SURRAK_ELUSIVE_HUNTER;
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        let mut state = build_game(1, &[&[], &[]]);
        let surrak = state.card_db().get(SURRAK_ELUSIVE_HUNTER).unwrap().chars.clone();
        let surrak_obj = state.add_card(PlayerId(1), surrak, Zone::Stack);
        let sid = put_spell_on_stack(&mut state, surrak_obj, PlayerId(1), 1);
        let effect = state.card_db().get(ESSENCE_SCATTER).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(
            state,
            vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))],
        );
        // Sanity: the stack-zone static is now gathered, so Surrak's spell reads CantBeCountered.
        assert!(
            e.state
                .computed(surrak_obj)
                .has_qualification(crate::effects::ability::Qualification::CantBeCountered),
            "Surrak's spell carries CantBeCountered on the stack"
        );
        e.resolve_effect(
            &effect,
            &ResolutionCtx {
                controller: Some(PlayerId(0)),
                chosen_targets: vec![Target::Stack(sid)],
                ..Default::default()
            },
            WbReason::Resolve(StackId(99)),
        );
        assert!(
            e.state.stack.items.iter().any(|s| s.id == sid),
            "Surrak can't be countered — still on the stack, will resolve"
        );
        assert!(!e.state.players[1].graveyard.contains(&surrak_obj), "not put into the graveyard");
    }
}
