//! Kirol, History Buff // Pack a Punch — `{R}{W}` Legendary Creature — Vampire Cleric 2/3 //
//! `{1}{R}{W}` Sorcery (first printed SOS). A **Prepare** DFC — the **cards-leave-your-graveyard** variant.
//!
//! Front: "Whenever one or more cards leave your graveyard, Kirol becomes prepared."
//! Back (Pack a Punch): "Mill a card. Put two +1/+1 counters on target creature. It gains trample
//! until end of turn."
//!
//! **Fully implemented** — the prepare trigger is a `CardsLeaveYourGraveyard` ability whose effect is
//! [`Effect::BecomePrepared`]. The back mills one, puts two +1/+1 counters on a target creature, and
//! grants it trample until end of turn.

use crate::basics::{CardType, Color, CounterKind};
use crate::cards::{creature, helpers, mana_cost, spell, CardDb};
use crate::effects::ability::{EventPattern, Keyword};
use crate::effects::condition::Duration;
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};
use crate::subtypes::{CreatureType, Supertype};

pub const KIROL_HISTORY_BUFF: u32 = 389;
pub const PACK_A_PUNCH: u32 = 9716;

pub fn register(db: &mut CardDb) {
    let pack_a_punch = Effect::Sequence(vec![
        Effect::Mill { who: PlayerRef::Controller, count: ValueExpr::Fixed(1) },
        Effect::PutCounters {
            what: EffectTarget::Target(TargetSpec {
                kind: TargetKind::Creature(CardFilter::Any),
                min: 1,
                max: 1,
                distinct: true,
            }),
            kind: CounterKind::PlusOnePlusOne,
            n: ValueExpr::Fixed(2),
        },
        Effect::GrantKeyword {
            what: EffectTarget::ChosenIndex(0),
            keyword: Keyword::Trample,
            duration: Duration::UntilEndOfTurn,
        },
    ]);
    let mut back = spell(
        PACK_A_PUNCH,
        "Pack a Punch",
        CardType::Sorcery,
        Color::Red,
        mana_cost(1, &[(Color::Red, 1), (Color::White, 1)]),
        pack_a_punch,
    )
    .with_text("Mill a card. Put two +1/+1 counters on target creature. It gains trample until end of turn.");
    back.chars.colors = vec![Color::Red, Color::White];
    db.insert(back);

    let mut front = creature(
        KIROL_HISTORY_BUFF,
        "Kirol, History Buff",
        &[CreatureType::Vampire, CreatureType::Cleric],
        Color::Red,
        mana_cost(0, &[(Color::Red, 1), (Color::White, 1)]),
        2,
        3,
        helpers::prepared_abilities(PACK_A_PUNCH, EventPattern::CardsLeaveYourGraveyard, None, false),
    );
    front.chars.colors = vec![Color::Red, Color::White];
    front.chars.supertypes = vec![Supertype::Legendary];
    front.text = "Whenever one or more cards leave your graveyard, Kirol becomes prepared. (While it's prepared, you may cast a copy of its spell. Doing so unprepares it.)\n// Pack a Punch {1}{R}{W} Sorcery — Mill a card. Put two +1/+1 counters on target creature. It gains trample until end of turn.".to_string();
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
    fn kirol_gy_leave_prepare_and_pack_a_punch() {
        let mut db = CardDb::default();
        register(&mut db);
        let f = db.get(KIROL_HISTORY_BUFF).unwrap();
        assert!(matches!(f.abilities[0], Ability::Prepare { spell: PACK_A_PUNCH }));
        assert!(matches!(
            f.abilities[1],
            Ability::Triggered { event: EventPattern::CardsLeaveYourGraveyard, .. }
        ));
        // Behaviour: mill one + two counters + trample on the target.
        let mut state = build_game(1, &[&[grp::ISLAND], &[]]);
        let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
        let bears = state.add_card(PlayerId(0), c, Zone::Battlefield);
        let effect = state.card_db().get(PACK_A_PUNCH).unwrap().spell_effect().unwrap().clone();
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
        assert_eq!(e.state.object(bears).counters.get(&CounterKind::PlusOnePlusOne), 2, "two +1/+1 counters");
        assert!(e.state.computed(bears).has_keyword(Keyword::Trample), "gained trample");
        assert_eq!(e.state.player(PlayerId(0)).graveyard.len(), 1, "milled one card");
    }
}
