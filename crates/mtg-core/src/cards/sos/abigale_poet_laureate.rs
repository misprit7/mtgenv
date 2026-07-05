//! Abigale, Poet Laureate // Heroic Stanza — `{1}{W}{B}` Legendary Creature — Bird Bard 2/3 //
//! `{1}{W/B}` Sorcery (first printed SOS). A **Prepare** DFC — the **cast-a-creature** variant.
//!
//! Front: "Whenever you cast a creature spell, Abigale becomes prepared."
//! Back (Heroic Stanza): "Put a +1/+1 counter on target creature."
//!
//! **Fully implemented** — the prepare trigger is a `SpellCast(creature)` ability whose effect is
//! [`Effect::BecomePrepared`]. The back puts a +1/+1 counter on a target creature.

use crate::basics::{CardType, Color, CounterKind};
use crate::cards::{creature, helpers, mana_cost, mana_cost_hybrid, spell, CardDb};
use crate::effects::ability::EventPattern;
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::ValueExpr;
use crate::effects::{Effect, EffectTarget};
use crate::subtypes::{CreatureType, Supertype};

pub const ABIGALE_POET_LAUREATE: u32 = 388;
pub const HEROIC_STANZA: u32 = 9715;

pub fn register(db: &mut CardDb) {
    let mut back = spell(
        HEROIC_STANZA,
        "Heroic Stanza",
        CardType::Sorcery,
        Color::White,
        mana_cost_hybrid(1, &[], &[(Color::White, Color::Black)]),
        Effect::PutCounters {
            what: EffectTarget::Target(TargetSpec {
                kind: TargetKind::Creature(CardFilter::Any),
                min: 1,
                max: 1,
                distinct: true,
            }),
            kind: CounterKind::PlusOnePlusOne,
            n: ValueExpr::Fixed(1),
        },
    )
    .with_text("Put a +1/+1 counter on target creature.");
    back.chars.colors = vec![Color::White, Color::Black];
    db.insert(back);

    let mut front = creature(
        ABIGALE_POET_LAUREATE,
        "Abigale, Poet Laureate",
        &[CreatureType::Bird, CreatureType::Bard],
        Color::White,
        mana_cost(1, &[(Color::White, 1), (Color::Black, 1)]),
        2,
        3,
        helpers::prepared_abilities(
            HEROIC_STANZA,
            EventPattern::SpellCast(CardFilter::HasCardType(CardType::Creature)),
            None,
            false,
        ),
    );
    front.chars.colors = vec![Color::White, Color::Black];
    front.chars.supertypes = vec![Supertype::Legendary];
    front.text = "Whenever you cast a creature spell, Abigale becomes prepared. (While it's prepared, you may cast a copy of its spell. Doing so unprepares it.)\n// Heroic Stanza {1}{W/B} Sorcery — Put a +1/+1 counter on target creature.".to_string();
    db.insert(front);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::RandomAgent;
    use crate::basics::{Target, Zone};
    use crate::cards::{build_game, grp};
    use crate::effects::ability::Ability;
    use crate::effects::action::{ResolutionCtx, WbReason};
    use crate::ids::{PlayerId, StackId};
    use crate::priority::Engine;

    #[test]
    fn abigale_cast_creature_prepare_and_heroic_stanza() {
        let mut db = CardDb::default();
        register(&mut db);
        let f = db.get(ABIGALE_POET_LAUREATE).unwrap();
        assert!(matches!(f.abilities[0], Ability::Prepare { spell: HEROIC_STANZA }));
        assert!(matches!(f.abilities[1], Ability::Triggered { event: EventPattern::SpellCast(_), .. }));
        // Behaviour: the back puts a +1/+1 counter on the target.
        let mut state = build_game(1, &[&[], &[]]);
        let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
        let bears = state.add_card(PlayerId(0), c, Zone::Battlefield);
        let effect = state.card_db().get(HEROIC_STANZA).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
        e.resolve_effect(
            &effect,
            &ResolutionCtx {
                controller: Some(PlayerId(0)),
                chosen_targets: vec![Target::Object(bears)],
                ..Default::default()
            },
            WbReason::Resolve(StackId(0)),
        );
        assert_eq!(e.state.object(bears).counters.get(&CounterKind::PlusOnePlusOne), 1, "one +1/+1 counter");
    }
}
