//! Germination Practicum — `{3}{G}{G}` Sorcery — Lesson (first printed SOS).
//!
//! Oracle: "Put two +1/+1 counters on each creature you control. Paradigm (Then exile this spell.
//! After you first resolve a spell with this name, you may cast a copy of it from exile without
//! paying its mana cost at the beginning of each of your first main phases.)"
//!
//! **Fully implemented.** The underlying effect is a `ForEach` over "creatures you control" putting
//! two +1/+1 counters on each (`EffectTarget::Each`). **Paradigm** is the shared bundle from
//! [`crate::cards::helpers::paradigm_abilities`] (the spell-copy subsystem, CR 707.12).

use crate::basics::{CardType, Color, CounterKind, Zone};
use crate::cards::{helpers, mana_cost, spell, CardDb};
use crate::effects::target::{CardFilter, SelectSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};
use crate::subtypes::{SpellType, Subtype};

/// grp id (per-set ids live near their cards).
pub const GERMINATION_PRACTICUM: u32 = 368;

pub fn register(db: &mut CardDb) {
    // "each creature you control" — a ForEach selector that iterates ALL of them (max = all; the
    // `helpers::creatures_you_control()` helper's max=0 is for static `affects` scopes, not ForEach).
    let creatures_you_control = SelectSpec {
        zone: Zone::Battlefield,
        filter: CardFilter::All(vec![
            CardFilter::HasCardType(CardType::Creature),
            CardFilter::ControlledBy(PlayerRef::Controller),
        ]),
        chooser: PlayerRef::Controller,
        min: ValueExpr::Fixed(0),
        max: ValueExpr::Fixed(999),
    };
    let effect = Effect::ForEach {
        selector: creatures_you_control,
        body: Box::new(Effect::PutCounters {
            what: EffectTarget::Each,
            kind: CounterKind::PlusOnePlusOne,
            n: ValueExpr::Fixed(2),
        }),
    };
    let mut def = spell(
        GERMINATION_PRACTICUM,
        "Germination Practicum",
        CardType::Sorcery,
        Color::Green,
        mana_cost(3, &[(Color::Green, 2)]),
        effect,
    )
    .with_text(
        "Put two +1/+1 counters on each creature you control. Paradigm (Then exile this spell. After you first resolve a spell with this name, you may cast a copy of it from exile without paying its mana cost at the beginning of each of your first main phases.)",
    );
    def.chars.subtypes = vec![Subtype::Spell(SpellType::Lesson)];
    def.abilities.extend(helpers::paradigm_abilities());
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{CastVariant, RandomAgent};
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
    fn germination_practicum_shape() {
        let db = db_with_card();
        let def = db.get(GERMINATION_PRACTICUM).unwrap();
        assert_eq!(def.chars.subtypes, vec![Subtype::Spell(SpellType::Lesson)]);
        assert!(def.fully_implemented);
        // Spell ability + the 3-part Paradigm bundle.
        assert!(matches!(def.abilities.get(1), Some(crate::effects::ability::Ability::Paradigm)));
        assert!(matches!(
            def.abilities.get(2),
            Some(crate::effects::ability::Ability::FunctionsFrom(z)) if z == &vec![Zone::Exile]
        ));
    }

    /// Cast it with two creatures out: each gets two +1/+1 counters, then Paradigm exiles the Lesson.
    #[test]
    fn puts_two_counters_on_each_creature_then_self_exiles() {
        let mut state = GameState::new(2, 1);
        state.set_card_db(std::sync::Arc::new(db_with_card()));
        let card = {
            let c = state.card_db().get(GERMINATION_PRACTICUM).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Hand)
        };
        let bears = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
        let a = state.add_card(PlayerId(0), bears.clone(), Zone::Battlefield);
        let b = state.add_card(PlayerId(0), bears.clone(), Zone::Battlefield);
        let theirs = state.add_card(PlayerId(1), bears, Zone::Battlefield); // opponent's — untouched
        for _ in 0..5 {
            let f = state.card_db().get(grp::FOREST).unwrap().chars.clone();
            state.add_card(PlayerId(0), f, Zone::Battlefield);
        }
        let mut e =
            Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
        e.state.phase = Phase::PrecombatMain;

        e.cast_spell(PlayerId(0), card, CastVariant::Normal);
        e.resolve_top();

        let plus = |id| e.state.object(id).counters.get(&CounterKind::PlusOnePlusOne);
        assert_eq!(plus(a), 2, "your first creature got two +1/+1 counters");
        assert_eq!(plus(b), 2, "your second creature got two +1/+1 counters");
        assert_eq!(plus(theirs), 0, "an opponent's creature is untouched");
        assert!(
            e.state.player(PlayerId(0)).exile.contains(&card),
            "Paradigm exiled the Lesson instead of the graveyard"
        );
    }
}
