//! Winds of Abandon — `{1}{W}` Sorcery (first printed MH1; reprinted on the SOS Mystical Archive `soa`).
//!
//! Oracle: "Exile target creature you don't control. For each creature exiled this way, its controller
//! searches their library for a basic land card, put it onto the battlefield tapped, then shuffle.
//! Overload {4}{W}{W}."
//!
//! **Fully implemented** — normal cast exiles one target creature you don't control and its controller
//! (`ControllerOfTarget(0)`) searches for a basic land. **Overload** (`Ability::Overload { {4}{W}{W} }`)
//! casts with no targets and `overload_rewrite` broadens it to "each creature you don't control": the
//! whole per-creature body (exile + that creature's controller's land search) runs inside the derived
//! `ForEach`, with `ControllerOfTarget`→`ControllerOfEach` so "its controller" is the current creature's.

use crate::basics::{CardType, Color, Zone, ZoneDest, ZonePos};
use crate::cards::helpers::basic_land_filter;
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::ability::Ability;
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::PlayerRef;
use crate::effects::{Effect, EffectTarget};

/// grp id (bonus-sheet `soa` cards use the 600+ block).
pub const WINDS_OF_ABANDON: u32 = 647;

pub fn register(db: &mut CardDb) {
    let effect = Effect::Sequence(vec![
        Effect::Exile {
            what: EffectTarget::Target(TargetSpec {
                kind: TargetKind::Creature(CardFilter::ControlledBy(PlayerRef::Opponent)),
                min: 1,
                max: 1,
                distinct: true,
            }),
        },
        // "its controller searches their library for a basic land, onto the battlefield tapped, then shuffle."
        Effect::Search {
            who: PlayerRef::ControllerOfTarget(0),
            zone: Zone::Library,
            filter: basic_land_filter(),
            min: 0,
            max: 1,
            to: ZoneDest { zone: Zone::Battlefield, pos: ZonePos::Any },
            tapped: true,
        },
    ]);
    let mut def = spell(
        WINDS_OF_ABANDON,
        "Winds of Abandon",
        CardType::Sorcery,
        Color::White,
        mana_cost(1, &[(Color::White, 1)]),
        effect,
    )
    .with_text("Exile target creature you don't control. For each creature exiled this way, its controller searches their library for a basic land card, put it onto the battlefield tapped, then shuffle.\nOverload {4}{W}{W}");
    def.abilities.push(Ability::Overload { cost: mana_cost(4, &[(Color::White, 2)]) });
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, CastVariant, DecisionRequest, DecisionResponse, PlayerView};
    use crate::basics::Phase;
    use crate::cards::{build_game, grp};
    use crate::ids::PlayerId;
    use crate::priority::Engine;

    /// Fetches the first offered basic; passes otherwise.
    #[derive(Clone)]
    struct FetchBasic;
    impl Agent for FetchBasic {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::SelectCards { from, .. } if !from.is_empty() => DecisionResponse::Indices(vec![0]),
                _ => DecisionResponse::Pass,
            }
        }
    }

    #[test]
    fn winds_has_overload() {
        let mut db = CardDb::default();
        register(&mut db);
        assert!(db.get(WINDS_OF_ABANDON).unwrap().abilities.iter().any(|a| matches!(a, Ability::Overload { .. })));
    }

    /// Overload: exiles EACH opponent creature; for each, its controller (P1) fetches a basic land
    /// tapped. Two opponent creatures exiled → P1 gets two tapped basics; your creature stays.
    #[test]
    fn overload_exiles_each_opponent_creature_and_they_ramp() {
        let mut state = build_game(1, &[&[], &[]]);
        let winds = state.add_card(PlayerId(0), state.card_db().get(WINDS_OF_ABANDON).unwrap().chars.clone(), Zone::Hand);
        for _ in 0..6 { state.add_card(PlayerId(0), state.card_db().get(grp::PLAINS).unwrap().chars.clone(), Zone::Battlefield); }
        let opp1 = state.add_card(PlayerId(1), state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Battlefield);
        let opp2 = state.add_card(PlayerId(1), state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Battlefield);
        let mine = state.add_card(PlayerId(0), state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Battlefield);
        // P1's library has basics to fetch.
        for _ in 0..3 { state.add_card(PlayerId(1), state.card_db().get(grp::FOREST).unwrap().chars.clone(), Zone::Library); }
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let p1_bf_before = state.player(PlayerId(1)).battlefield.len();
        let mut e = Engine::new(state, vec![Box::new(FetchBasic), Box::new(FetchBasic)]);
        e.cast_spell(PlayerId(0), winds, CastVariant::Overload);
        e.run_agenda();
        while !e.state.stack.items.is_empty() { e.resolve_top(); e.run_agenda(); }
        assert!(e.state.player(PlayerId(1)).exile.contains(&opp1) && e.state.player(PlayerId(1)).exile.contains(&opp2), "each opponent creature exiled");
        assert!(e.state.player(PlayerId(0)).battlefield.contains(&mine), "your own creature stays");
        // P1 had 2 creatures on the battlefield; both left (exiled) and 2 basics entered → net field size
        // = before − 2 creatures + 2 fetched basics = unchanged count, but now two are tapped Forests.
        let p1_forests = e.state.player(PlayerId(1)).battlefield.iter().filter(|&&id| e.state.object(id).chars.grp_id == grp::FOREST).count();
        assert_eq!(p1_forests, 2, "its controller fetched a basic per exiled creature");
        let _ = p1_bf_before;
    }
}
