//! Improvisation Capstone — `{5}{R}{R}` Sorcery — Lesson (first printed SOS). The 5th and heaviest
//! Lesson.
//!
//! Oracle: "Exile cards from the top of your library until you exile cards with total mana value 4 or
//! greater. You may cast any number of spells from among them without paying their mana costs. Paradigm
//! (Then exile this spell. After you first resolve a spell with this name, you may cast a copy of it
//! from exile without paying its mana cost at the beginning of each of your first main phases.)"
//!
//! **Fully implemented.** The underlying effect is [`Effect::ExileTopUntilManaValueMayCastFree`] with a
//! threshold of 4: exile from the top one card at a time until the exiled cards' total mana value ≥ 4,
//! then (CR 601.3e cast-during-resolution) loop offering the controller to cast any number of the exiled
//! **nonland** cards for free — each a real `cast_spell(WithoutPayingManaCost)` onto the stack. Uncast
//! cards (and the exiled lands) stay in exile. **Paradigm** is the shared bundle from
//! [`crate::cards::helpers::paradigm_abilities`] (the spell-copy subsystem, CR 707.12).

use crate::basics::{CardType, Color};
use crate::cards::{helpers, mana_cost, spell, CardDb};
use crate::effects::value::PlayerRef;
use crate::effects::Effect;
use crate::subtypes::{SpellType, Subtype};

/// grp id (per-set ids live near their cards).
pub const IMPROVISATION_CAPSTONE: u32 = 402;

pub fn register(db: &mut CardDb) {
    let effect = Effect::ExileTopUntilManaValueMayCastFree {
        who: PlayerRef::Controller,
        total_mana_value: 4,
    };
    let mut def = spell(
        IMPROVISATION_CAPSTONE,
        "Improvisation Capstone",
        CardType::Sorcery,
        Color::Red,
        mana_cost(5, &[(Color::Red, 2)]),
        effect,
    )
    .with_text(
        "Exile cards from the top of your library until you exile cards with total mana value 4 or greater. You may cast any number of spells from among them without paying their mana costs. Paradigm (Then exile this spell. After you first resolve a spell with this name, you may cast a copy of it from exile without paying its mana cost at the beginning of each of your first main phases.)",
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
    fn improvisation_capstone_shape() {
        let db = db_with_card();
        let def = db.get(IMPROVISATION_CAPSTONE).unwrap();
        assert_eq!(def.chars.subtypes, vec![Subtype::Spell(SpellType::Lesson)]);
        assert!(def.fully_implemented);
        assert!(matches!(def.abilities.get(1), Some(crate::effects::ability::Ability::Paradigm)));
        assert!(matches!(
            def.spell_effect(),
            Some(Effect::ExileTopUntilManaValueMayCastFree { total_mana_value: 4, .. })
        ));
    }

    /// Confirms yes; for a SelectCards ("cast for free") always casts the first remaining card; passes
    /// otherwise. Drives casting every exiled nonland card.
    struct CastAllAgent;
    impl Agent for CastAllAgent {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::SelectCards { from, .. } if !from.is_empty() => {
                    DecisionResponse::Indices(vec![0])
                }
                DecisionRequest::Confirm { .. } => DecisionResponse::Bool(true),
                _ => DecisionResponse::Pass,
            }
        }
    }

    /// Declines every "cast for free" offer.
    struct CastNoneAgent;
    impl Agent for CastNoneAgent {
        fn decide(&mut self, _v: &PlayerView, _req: &DecisionRequest) -> DecisionResponse {
            match _req {
                DecisionRequest::SelectCards { .. } => DecisionResponse::Indices(vec![]),
                _ => DecisionResponse::Pass,
            }
        }
    }

    /// Build a game: Improvisation Capstone in P0's hand + {5}{R}{R} of Mountains; the library's top
    /// three (exile order) are Grizzly Bears (MV2), Forest (MV0 land), Grizzly Bears (MV2). Returns the
    /// (state, capstone, forest) so tests can assert on the exiled set.
    fn setup() -> (GameState, crate::ids::ObjId, crate::ids::ObjId) {
        let mut state = GameState::new(2, 1);
        state.set_card_db(std::sync::Arc::new(db_with_card()));
        let capstone = {
            let c = state.card_db().get(IMPROVISATION_CAPSTONE).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Hand)
        };
        for _ in 0..7 {
            let m = state.card_db().get(grp::MOUNTAIN).unwrap().chars.clone();
            state.add_card(PlayerId(0), m, Zone::Battlefield);
        }
        // Library — added bottom→top, so the last added is exiled first. Exile order: Bears, Forest,
        // Bears → totals 2, 2, 4 → stop with 3 cards exiled (one is a land).
        let mk = |state: &mut GameState, grp_id: u32| {
            let c = state.card_db().get(grp_id).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Library)
        };
        mk(&mut state, grp::GRIZZLY_BEARS); // bottom (exiled 3rd)
        let forest = mk(&mut state, grp::FOREST); // exiled 2nd
        mk(&mut state, grp::GRIZZLY_BEARS); // top (exiled 1st)
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        (state, capstone, forest)
    }

    /// Headline: exile from the top until total MV ≥ 4 (2 Bears + a Forest = 3 cards), then cast the two
    /// nonland cards (the Bears) for free — they leave exile onto the stack and, resolved, enter the
    /// battlefield; the Forest stays exiled and the Lesson self-exiles (Paradigm).
    #[test]
    fn exiles_until_mv4_then_casts_the_nonland_spells_free() {
        let (state, capstone, forest) = setup();
        let bf_before = state.player(PlayerId(0)).battlefield.len();
        let mut e = Engine::new(state, vec![Box::new(CastAllAgent), Box::new(CastAllAgent)]);

        e.cast_spell(PlayerId(0), capstone, CastVariant::Normal);
        e.resolve_top(); // the Lesson resolves: exile 3, cast the 2 Bears for free (onto the stack)

        // The two Bears are now on the stack (cast, not yet resolved); resolve them → they enter.
        assert_eq!(e.state.stack.len(), 2, "both Bears were cast for free onto the stack");
        e.resolve_top();
        e.resolve_top();
        e.run_agenda();

        assert_eq!(
            e.state.player(PlayerId(0)).battlefield.len(),
            bf_before + 2,
            "both free-cast Bears entered the battlefield"
        );
        // The exiled land stayed in exile (a land isn't a spell); the Lesson self-exiled (Paradigm).
        assert!(e.state.player(PlayerId(0)).exile.contains(&forest), "the exiled Forest stays exiled");
        assert!(
            e.state.player(PlayerId(0)).exile.contains(&capstone),
            "Paradigm exiled the Lesson instead of the graveyard"
        );
        // The two Bears left exile (they were cast); only the Forest + the Lesson remain there.
        assert_eq!(e.state.player(PlayerId(0)).exile.len(), 2, "exile holds only the Forest and the Lesson");
    }

    /// Declining every "cast for free" offer leaves all three exiled cards in exile.
    #[test]
    fn may_decline_and_leave_them_exiled() {
        let (state, capstone, forest) = setup();
        let mut e = Engine::new(state, vec![Box::new(CastNoneAgent), Box::new(CastNoneAgent)]);
        e.cast_spell(PlayerId(0), capstone, CastVariant::Normal);
        e.resolve_top();
        assert!(e.state.stack.is_empty(), "nothing was cast");
        // 3 exiled cards + the self-exiled Lesson = 4 in exile.
        assert!(e.state.player(PlayerId(0)).exile.contains(&forest), "the Forest is exiled");
        assert!(e.state.player(PlayerId(0)).exile.contains(&capstone), "the Lesson self-exiled");
        assert_eq!(
            e.state.player(PlayerId(0)).exile.len(),
            4,
            "3 milled-to-exile cards + the self-exiled Lesson"
        );
    }
}
