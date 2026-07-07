//! Return to the Ranks — `{X}{W}{W}` Sorcery (first printed M15; reprinted on the SOS Mystical
//! Archive `soa`).
//!
//! Oracle: "Convoke (Your creatures can help cast this spell. Each creature you tap while casting this
//! spell pays for {1} or one mana of that creature's color.)
//! Return X target creature cards with mana value 2 or less from your graveyard to the battlefield."
//!
//! **Fully implemented** — the Convoke subsystem (`Ability::Convoke`: the offer gate and payment share
//! the `convoke_reduce` planner — creatures tapped while casting reduce the mana cost by {1} or one
//! matching coloured pip, before `auto_pay`). The body is a `ForEachTarget` over the X (`TARGET_COUNT_X`)
//! chosen graveyard creature cards you own with mana value ≤ 2, each `ReanimateUnderControl`led back to
//! the battlefield. (Sizing the chosen X against convoke — vs mana alone — is the deferred §2.6 pending-
//! cast milestone; convoke here reduces the fixed pips.)

use crate::basics::{CardType, Color, Zone};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::ability::Ability;
use crate::effects::target::{CardFilter, TargetKind, TargetSpec, TARGET_COUNT_X};
use crate::effects::value::PlayerRef;
use crate::effects::{Effect, EffectTarget};

/// grp id (bonus-sheet `soa` cards use the 600+ block).
pub const RETURN_TO_THE_RANKS: u32 = 663;

pub fn register(db: &mut CardDb) {
    let effect = Effect::ForEachTarget {
        slot: TargetSpec {
            kind: TargetKind::CardInZone {
                zone: Zone::Graveyard,
                filter: CardFilter::All(vec![
                    CardFilter::HasCardType(CardType::Creature),
                    CardFilter::ManaValue { min: None, max: Some(2) },
                    CardFilter::OwnedBy(PlayerRef::Controller),
                ]),
            },
            min: 0,
            max: TARGET_COUNT_X,
            distinct: true,
        },
        body: Box::new(Effect::ReanimateUnderControl { what: EffectTarget::Each }),
    };
    let mut mc = mana_cost(0, &[(Color::White, 2)]);
    mc.x = 1; // {X}{W}{W}
    let mut def = spell(RETURN_TO_THE_RANKS, "Return to the Ranks", CardType::Sorcery, Color::White, mc, effect)
        .with_text(
            "Convoke (Your creatures can help cast this spell. Each creature you tap while casting this spell pays for {1} or one mana of that creature's color.)\nReturn X target creature cards with mana value 2 or less from your graveyard to the battlefield.",
        );
    def.abilities.push(Ability::Convoke);
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, CastVariant, DecisionRequest, DecisionResponse, PlayableAction, PlayerView};
    use crate::basics::Target;
    use crate::cards::{build_game, grp};
    use crate::effects::action::{ResolutionCtx, WbReason};
    use crate::ids::{PlayerId, StackId};
    use crate::priority::Engine;

    #[derive(Clone)]
    struct Passive;
    impl Agent for Passive {
        fn decide(&mut self, _v: &PlayerView, _r: &DecisionRequest) -> DecisionResponse {
            DecisionResponse::Pass
        }
    }

    /// Convokes all offered creatures, chooses X = 0 (no reanimation), targets nothing.
    #[derive(Clone)]
    struct ConvokeAll;
    impl Agent for ConvokeAll {
        fn decide(&mut self, _v: &PlayerView, r: &DecisionRequest) -> DecisionResponse {
            match r {
                DecisionRequest::SelectCards { from, .. } => {
                    DecisionResponse::Indices((0..from.len() as u32).collect())
                }
                DecisionRequest::ChooseNumber { .. } => DecisionResponse::Number(0),
                _ => DecisionResponse::Pass,
            }
        }
    }

    fn white_bear(state: &mut crate::state::GameState, p: PlayerId, zone: Zone) -> crate::ids::ObjId {
        // Grizzly Bears are green; give a plain white 2/2 for convoke-colour matching.
        let mut ch = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
        ch.colors = vec![Color::White];
        state.add_card(p, ch, zone)
    }

    fn set_sorcery_timing(state: &mut crate::state::GameState) {
        state.active_player = PlayerId(0);
        state.phase = crate::basics::Phase::PrecombatMain;
    }

    #[test]
    fn shape_has_convoke() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(RETURN_TO_THE_RANKS).unwrap();
        assert!(def.fully_implemented);
        assert!(def.abilities.iter().any(|a| matches!(a, Ability::Convoke)), "has Convoke");
    }

    /// Offer gate: with no mana but two white creatures, convoke makes the {W}{W} base cost payable —
    /// the Normal cast is offered. With neither mana nor creatures it is not.
    #[test]
    fn convoke_gates_the_offer() {
        let mut state = build_game(1, &[&[], &[]]);
        let rtr = state.add_card(PlayerId(0), state.card_db().get(RETURN_TO_THE_RANKS).unwrap().chars.clone(), Zone::Hand);
        // A legal reanimation target (an owned MV-2 creature card in the graveyard) so the target gate
        // passes — this test isolates the *convoke* affordability half of the offer.
        state.add_card(PlayerId(0), state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Graveyard);
        set_sorcery_timing(&mut state);
        let mut e = Engine::new(state, vec![Box::new(Passive), Box::new(Passive)]);
        let offered = |e: &Engine| e.legal_actions(PlayerId(0)).iter().any(|a| matches!(a, PlayableAction::Cast { spell, variant: CastVariant::Normal } if *spell == rtr));
        assert!(!offered(&e), "no mana, no creatures to convoke → not castable");
        white_bear(&mut e.state, PlayerId(0), Zone::Battlefield);
        white_bear(&mut e.state, PlayerId(0), Zone::Battlefield);
        assert!(offered(&e), "two white creatures convoke the {{W}}{{W}} → castable");
    }

    /// Payment: casting with two white creatures and no mana taps both for convoke (paying {W}{W})
    /// and the spell lands on the stack.
    #[test]
    fn convoke_taps_creatures_to_pay() {
        let mut state = build_game(1, &[&[], &[]]);
        let rtr = state.add_card(PlayerId(0), state.card_db().get(RETURN_TO_THE_RANKS).unwrap().chars.clone(), Zone::Hand);
        let c1 = white_bear(&mut state, PlayerId(0), Zone::Battlefield);
        let c2 = white_bear(&mut state, PlayerId(0), Zone::Battlefield);
        set_sorcery_timing(&mut state);
        let mut e = Engine::new(state, vec![Box::new(ConvokeAll), Box::new(ConvokeAll)]);
        e.cast_spell(PlayerId(0), rtr, CastVariant::Normal);
        assert!(!e.state.stack.is_empty(), "cast onto the stack via convoke");
        assert!(e.state.object(c1).status.tapped && e.state.object(c2).status.tapped, "both creatures tapped for convoke");
    }

    /// Body: with X = 2 and two owned creature cards (MV ≤ 2) targeted, both return to the battlefield.
    #[test]
    fn body_returns_x_creature_cards() {
        let mut state = build_game(1, &[&[], &[]]);
        let g1 = state.add_card(PlayerId(0), state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Graveyard);
        let g2 = state.add_card(PlayerId(0), state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Graveyard);
        let effect = state.card_db().get(RETURN_TO_THE_RANKS).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(Passive), Box::new(Passive)]);
        e.resolve_effect(
            &effect,
            &ResolutionCtx {
                controller: Some(PlayerId(0)),
                x: Some(2),
                chosen_targets: vec![Target::Object(g1), Target::Object(g2)],
                ..Default::default()
            },
            WbReason::Resolve(StackId(0)),
        );
        assert_eq!(e.state.object(g1).zone, Zone::Battlefield, "first creature returned");
        assert_eq!(e.state.object(g2).zone, Zone::Battlefield, "second creature returned");
    }
}
