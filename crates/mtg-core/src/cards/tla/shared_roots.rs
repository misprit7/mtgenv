//! Shared Roots — `{1}{G}` Sorcery — Lesson (first printed TLA; reprinted on the SOS Mystical Archive `soa`).
//!
//! Oracle: "Search your library for a basic land card, put it onto the battlefield tapped, then shuffle."
//!
//! **Fully implemented** — a Lesson sorcery whose effect is the shared `fetch_basic_tapped` (search a
//! basic land onto the battlefield tapped, then shuffle).

use crate::basics::{CardType, Color};
use crate::cards::helpers::fetch_basic_tapped;
use crate::cards::{mana_cost, spell, CardDb};
use crate::subtypes::{SpellType, Subtype};

/// grp id (bonus-sheet `soa` cards use the 600+ block).
pub const SHARED_ROOTS: u32 = 624;

pub fn register(db: &mut CardDb) {
    let mut def = spell(
        SHARED_ROOTS,
        "Shared Roots",
        CardType::Sorcery,
        Color::Green,
        mana_cost(1, &[(Color::Green, 1)]),
        fetch_basic_tapped(),
    )
    .with_text("Search your library for a basic land card, put it onto the battlefield tapped, then shuffle.");
    def.chars.subtypes = vec![Subtype::Spell(SpellType::Lesson)];
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView};
    use crate::basics::Zone;
    use crate::cards::{build_game, grp};
    use crate::effects::action::{ResolutionCtx, WbReason};
    use crate::ids::{PlayerId, StackId};
    use crate::priority::Engine;

    /// Searches out the first offered basic.
    #[derive(Clone)]
    struct FetchFirst;
    impl Agent for FetchFirst {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::SelectCards { from, .. } if !from.is_empty() => DecisionResponse::Indices(vec![0]),
                _ => DecisionResponse::Pass,
            }
        }
    }

    #[test]
    fn shared_roots_is_a_lesson() {
        let mut db = CardDb::default();
        register(&mut db);
        assert_eq!(db.get(SHARED_ROOTS).unwrap().chars.subtypes, vec![Subtype::Spell(SpellType::Lesson)]);
    }

    #[test]
    fn shared_roots_fetches_a_basic_tapped() {
        let mut state = build_game(1, &[&[], &[]]);
        state.add_card(PlayerId(0), state.card_db().get(grp::FOREST).unwrap().chars.clone(), Zone::Library);
        let effect = state.card_db().get(SHARED_ROOTS).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(FetchFirst), Box::new(FetchFirst)]);
        e.resolve_effect(
            &effect,
            &ResolutionCtx { controller: Some(PlayerId(0)), ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        let land = e
            .state
            .player(PlayerId(0))
            .battlefield
            .iter()
            .find(|&&id| e.state.object(id).chars.grp_id == grp::FOREST)
            .copied();
        assert!(land.is_some(), "fetched a Forest onto the battlefield");
        assert!(e.state.object(land.unwrap()).status.tapped, "enters tapped");
    }
}
