//! Culling Ritual — `{2}{B}{G}` Sorcery (first printed STX; reprinted on the SOS Mystical Archive `soa`).
//!
//! Oracle: "Destroy each nonland permanent with mana value 2 or less. Add {B} or {G} for each
//! permanent destroyed this way."
//!
//! **Fully implemented** — a `Sequence`:
//! 1. a `ForEach` mass-destroy over every nonland permanent (both players') of mana value 2 or less
//!    (the `chooser: EachPlayer, min:0, max:999` "all matching" idiom — Brotherhood's End's board wipe);
//! 2. an `AddMana` producing one mana **per permanent actually destroyed** — the count read from the
//!    per-resolution destroy scratch ([`ValueExpr::DestroyedThisResolution`], which only counts real
//!    destructions, not indestructible / replaced-away ones) — each mana independently `{B}` or `{G}`
//!    via the [`ManaSpec::one_of`] subset-choice.
//!
//! The `AddMana` step flushes the staged destroy actions before it runs (imperative-effect flush), so
//! the scratch is populated by the time the count is evaluated.

use crate::basics::{CardType, Color, Zone};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::target::{CardFilter, ManaSpec, SelectSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};

/// grp id (bonus-sheet `soa` cards use the 600+ block).
pub const CULLING_RITUAL: u32 = 658;

pub fn register(db: &mut CardDb) {
    let destroy_each = Effect::ForEach {
        selector: SelectSpec {
            zone: Zone::Battlefield,
            filter: CardFilter::All(vec![
                CardFilter::Not(Box::new(CardFilter::HasCardType(CardType::Land))),
                CardFilter::ManaValue { min: None, max: Some(2) },
            ]),
            chooser: PlayerRef::EachPlayer,
            min: ValueExpr::Fixed(0),
            max: ValueExpr::Fixed(999),
        },
        body: Box::new(Effect::Destroy { what: EffectTarget::Each }),
    };
    let add_mana = Effect::AddMana {
        who: PlayerRef::Controller,
        mana: ManaSpec {
            produces: vec![],
            any_color: None,
            // One mana per permanent destroyed, each independently {B} or {G} (CR 106.1c).
            one_of: Some((vec![Color::Black, Color::Green], ValueExpr::DestroyedThisResolution)),
            restriction: None,
        },
    };
    let effect = Effect::Sequence(vec![destroy_each, add_mana]);
    let mut def = spell(
        CULLING_RITUAL,
        "Culling Ritual",
        CardType::Sorcery,
        Color::Black,
        mana_cost(2, &[(Color::Black, 1), (Color::Green, 1)]),
        effect,
    )
    .with_text(
        "Destroy each nonland permanent with mana value 2 or less. Add {B} or {G} for each permanent destroyed this way.",
    );
    def.chars.colors = vec![Color::Black, Color::Green];
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
    use crate::state::Characteristics;

    /// Picks Black for every `ChooseColor` (the `one_of` mana) and passes everything else.
    #[derive(Clone)]
    struct PickBlack;
    impl Agent for PickBlack {
        fn decide(&mut self, _v: &PlayerView, r: &DecisionRequest) -> DecisionResponse {
            match r {
                DecisionRequest::ChooseColor { allowed, .. } => {
                    // Index of Black within the offered subset ({B}, {G}); default 0.
                    let i = allowed.iter().position(|&c| c == Color::Black).unwrap_or(0);
                    DecisionResponse::Indices(vec![i as u32])
                }
                _ => DecisionResponse::Pass,
            }
        }
    }

    fn small(name: &str, mv_pips: u32) -> Characteristics {
        // A generic nonland permanent (artifact) whose mana value = `mv_pips` generic.
        Characteristics {
            name: name.to_string(),
            card_types: vec![CardType::Artifact],
            grp_id: 8100 + mv_pips,
            mana_cost: Some(mana_cost(mv_pips, &[])),
            ..Default::default()
        }
    }

    #[test]
    fn culling_ritual_shape() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(CULLING_RITUAL).unwrap();
        assert!(def.fully_implemented);
        assert_eq!(def.chars.colors, vec![Color::Black, Color::Green]);
        match def.spell_effect().unwrap() {
            Effect::Sequence(v) => assert_eq!(v.len(), 2),
            o => panic!("expected Sequence, got {o:?}"),
        }
    }

    /// Real path: three MV≤2 nonland permanents (across both players) + one MV3 permanent + a land.
    /// Resolving destroys exactly the three MV≤2 nonlands (land + MV3 survive) and adds THREE mana
    /// ({B}, since the agent picks black) — one per permanent destroyed this way.
    #[test]
    fn destroys_small_nonlands_and_adds_a_mana_each() {
        let mut state = build_game(1, &[&[], &[]]);
        let a = state.add_card(PlayerId(0), small("Trinket A", 1), Zone::Battlefield); // MV1
        let b = state.add_card(PlayerId(0), small("Trinket B", 2), Zone::Battlefield); // MV2
        let c = state.add_card(PlayerId(1), small("Trinket C", 0), Zone::Battlefield); // MV0
        let big = state.add_card(PlayerId(1), small("Colossus", 3), Zone::Battlefield); // MV3 — survives
        let land = state.add_card(
            PlayerId(0),
            state.card_db().get(grp::FOREST).unwrap().chars.clone(),
            Zone::Battlefield,
        ); // land — survives
        let effect = state.card_db().get(CULLING_RITUAL).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(PickBlack), Box::new(PickBlack)]);
        e.resolve_effect(
            &effect,
            &ResolutionCtx { controller: Some(PlayerId(0)), ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        // The three small nonland permanents are gone; the MV3 permanent and the land remain.
        for id in [a, b, c] {
            assert!(e.state.objects.get(&id).map(|o| o.zone) != Some(Zone::Battlefield), "small destroyed");
        }
        assert_eq!(e.state.object(big).zone, Zone::Battlefield, "MV3 permanent survives");
        assert_eq!(e.state.object(land).zone, Zone::Battlefield, "land survives");
        // Three permanents destroyed ⇒ three mana added to the controller's pool ({B} × 3).
        assert_eq!(*e.state.player(PlayerId(0)).mana_pool.amounts.get(&Color::Black).unwrap_or(&0), 3, "one black mana per destroyed");
    }
}
