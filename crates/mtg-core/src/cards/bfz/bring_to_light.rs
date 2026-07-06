//! Bring to Light — `{3}{G}{U}` Sorcery (first printed BFZ; reprinted on the SOS Mystical Archive `soa`).
//!
//! Oracle: "Converge — Search your library for a creature, instant, or sorcery card with mana value
//! less than or equal to the number of colors of mana spent to cast this spell, exile that card, then
//! shuffle. You may cast that card without paying its mana cost."
//!
//! **Fully implemented** — a `Search` (to exile, then shuffle) for a creature/instant/sorcery of mana
//! value ≤ `ColorsSpent` (dynamic `ManaValueExpr` bound), then an `Optional` `CastForFree` of the
//! searched card (`EffectTarget::Searched(0)`) — "you may cast it without paying its mana cost."

use crate::basics::{CardType, Color, Zone, ZoneDest, ZonePos};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::target::CardFilter;
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};

/// grp id (bonus-sheet `soa` cards use the 600+ block).
pub const BRING_TO_LIGHT: u32 = 637;

pub fn register(db: &mut CardDb) {
    let effect = Effect::Sequence(vec![
        Effect::Search {
            who: PlayerRef::Controller,
            zone: Zone::Library,
            filter: CardFilter::All(vec![
                CardFilter::AnyOf(vec![
                    CardFilter::HasCardType(CardType::Creature),
                    CardFilter::HasCardType(CardType::Instant),
                    CardFilter::HasCardType(CardType::Sorcery),
                ]),
                CardFilter::ManaValueExpr { min: None, max: Some(Box::new(ValueExpr::ColorsSpent)) },
            ]),
            min: 0,
            max: 1,
            to: ZoneDest { zone: Zone::Exile, pos: ZonePos::Any },
            tapped: false,
        },
        Effect::Optional {
            prompt: "Cast the exiled card without paying its mana cost?".to_string(),
            body: Box::new(Effect::CastForFree {
                what: EffectTarget::Searched(0),
                exile_on_leave: false,
            }),
        },
    ]);
    let mut def = spell(
        BRING_TO_LIGHT,
        "Bring to Light",
        CardType::Sorcery,
        Color::Green,
        mana_cost(3, &[(Color::Green, 1), (Color::Blue, 1)]),
        effect,
    )
    .with_text("Converge — Search your library for a creature, instant, or sorcery card with mana value less than or equal to the number of colors of mana spent to cast this spell, exile that card, then shuffle. You may cast that card without paying its mana cost.");
    def.chars.colors = vec![Color::Green, Color::Blue];
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView};
    use crate::cards::{build_game, grp};
    use crate::effects::action::{ResolutionCtx, WbReason};
    use crate::ids::{ObjId, PlayerId, StackId};
    use crate::priority::Engine;

    /// Fetches the target creature and confirms the optional free cast.
    #[derive(Clone)]
    struct FetchAndCast(ObjId);
    impl Agent for FetchAndCast {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::SelectCards { from, .. } => {
                    let idx = from.iter().position(|&id| id == self.0).unwrap_or(0);
                    DecisionResponse::Indices(vec![idx as u32])
                }
                DecisionRequest::Confirm { .. } => DecisionResponse::Bool(true),
                _ => DecisionResponse::Pass,
            }
        }
    }

    #[test]
    fn bring_to_light_shape() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(BRING_TO_LIGHT).unwrap();
        assert!(def.fully_implemented);
        let Some(Effect::Sequence(steps)) = def.spell_effect() else { panic!("sequence") };
        assert!(matches!(steps[0], Effect::Search { .. }));
        assert!(matches!(steps[1], Effect::Optional { .. }));
    }

    /// Converge = 2 colors: search out a Grizzly Bears (MV 2) and cast it for free onto the battlefield.
    #[test]
    fn searches_and_free_casts_within_converge() {
        let mut state = build_game(1, &[&[], &[]]);
        // A Bring to Light object as the effect source, marked as if cast with 2 colors of mana.
        let src = state.add_card(PlayerId(0), state.card_db().get(BRING_TO_LIGHT).unwrap().chars.clone(), Zone::Stack);
        if let Some(o) = state.objects.get_mut(&src) {
            o.colors_spent = 2;
        }
        let bear = state.add_card(PlayerId(0), state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Library);
        for _ in 0..3 {
            state.add_card(PlayerId(0), state.card_db().get(grp::FOREST).unwrap().chars.clone(), Zone::Library);
        }
        let effect = state.card_db().get(BRING_TO_LIGHT).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(FetchAndCast(bear)), Box::new(FetchAndCast(bear))]);
        e.resolve_effect(
            &effect,
            &ResolutionCtx { controller: Some(PlayerId(0)), source: Some(src), ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        // The bear left the library (searched to exile) and was cast for free — resolve the stack.
        e.run_agenda();
        while !e.state.stack.items.is_empty() {
            e.resolve_top();
            e.run_agenda();
        }
        assert!(!e.state.player(PlayerId(0)).library.contains(&bear), "bear left the library");
        assert!(e.state.player(PlayerId(0)).battlefield.contains(&bear), "free-cast Grizzly Bears resolved onto the battlefield");
    }
}
