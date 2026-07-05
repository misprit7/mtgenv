//! Fix What's Broken — `{2}{W}{B}` Sorcery (first printed SOS).
//!
//! Oracle: "As an additional cost to cast this spell, pay X life. Return each artifact and creature
//! card with mana value X from your graveyard to the battlefield."
//!
//! **Fully implemented** — the reanimate sibling of Vicious Rivalry on the same **X-in-an-
//! additional-cost** shape (CR 601.2b/f + 107.3): X is announced at cast (the mana cost has no
//! `{X}`), paid as life, and read by the effect. A `ForEach` over the caster's graveyard returns
//! each artifact/creature card whose mana value is **exactly** X (the dynamic
//! `CardFilter::ManaValueExpr { min: X, max: X }`) to the battlefield under their control (owner ==
//! controller, so no control override — mirrors Vastlands Scavenger / Forum Necroscribe).

use crate::basics::{CardType, Color, Zone, ZoneDest, ZonePos};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::ability::{AdditionalCost, Ability, Cost, CostComponent};
use crate::effects::target::{CardFilter, SelectSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};

/// grp id (per-set ids live near their cards).
pub const FIX_WHATS_BROKEN: u32 = 413;

pub fn register(db: &mut CardDb) {
    // Each artifact/creature CARD in your graveyard with mana value exactly X.
    let selector = SelectSpec {
        zone: Zone::Graveyard,
        filter: CardFilter::All(vec![
            CardFilter::AnyOf(vec![
                CardFilter::HasCardType(CardType::Artifact),
                CardFilter::HasCardType(CardType::Creature),
            ]),
            CardFilter::ManaValueExpr {
                min: Some(Box::new(ValueExpr::X)),
                max: Some(Box::new(ValueExpr::X)),
            },
        ]),
        chooser: PlayerRef::Controller,
        min: ValueExpr::Fixed(0),
        max: ValueExpr::Fixed(999),
    };
    let effect = Effect::ForEach {
        selector,
        body: Box::new(Effect::MoveZone {
            what: EffectTarget::Each,
            to: ZoneDest { zone: Zone::Battlefield, pos: ZonePos::Any },
            tapped: false,
        }),
    };
    let mut def = spell(
        FIX_WHATS_BROKEN,
        "Fix What's Broken",
        CardType::Sorcery,
        Color::White,
        mana_cost(2, &[(Color::White, 1), (Color::Black, 1)]),
        effect,
    )
    .with_text(
        "As an additional cost to cast this spell, pay X life.\nReturn each artifact and creature card with mana value X from your graveyard to the battlefield.",
    );
    def.chars.colors = vec![Color::White, Color::Black];
    // "As an additional cost to cast this spell, pay X life." (CR 601.2b / 107.3).
    def.abilities.push(Ability::AdditionalCost(AdditionalCost {
        options: vec![Cost { mana: None, components: vec![CostComponent::PayLife(ValueExpr::X)] }],
    }));
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, CastVariant, DecisionRequest, DecisionResponse, PlayerView, RandomAgent};
    use crate::basics::Phase;
    use crate::cards::{build_game, grp};
    use crate::ids::{ObjId, PlayerId};
    use crate::priority::Engine;

    /// An agent that answers `ChooseNumber` (X) with a fixed value, else passes.
    #[derive(Clone)]
    struct XAgent(i64);
    impl Agent for XAgent {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::ChooseNumber { .. } => DecisionResponse::Number(self.0),
                _ => DecisionResponse::Pass,
            }
        }
    }

    #[test]
    fn fix_whats_broken_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(FIX_WHATS_BROKEN).unwrap();
        assert_eq!(def.chars.colors, vec![Color::White, Color::Black]);
        assert_eq!(def.chars.mana_cost.as_ref().unwrap().x, 0, "no {{X}} in the printed mana cost");
        assert!(def.fully_implemented);
        assert!(matches!(
            def.additional_costs()[0].options[0].components[0],
            CostComponent::PayLife(ValueExpr::X)
        ));
    }

    /// Put Fix What's Broken in P0's hand with {2}{W}{B} of mana, and `gy` MV-2 Grizzly Bears in
    /// P0's graveyard. Returns `(engine, fix, gy_bears)`.
    fn setup(x: i64) -> (Engine, ObjId, Vec<ObjId>) {
        let mut state = build_game(1, &[&[], &[]]);
        let fix = state.add_card(
            PlayerId(0),
            state.card_db().get(FIX_WHATS_BROKEN).unwrap().chars.clone(),
            Zone::Hand,
        );
        // {2}{W}{B}: two Plains + two Swamps pay the white/black pips + 2 generic.
        for grp_id in [grp::PLAINS, grp::PLAINS, grp::SWAMP, grp::SWAMP] {
            let c = state.card_db().get(grp_id).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield);
        }
        // Two MV-2 Grizzly Bears in the graveyard, plus a Mountain (a land, MV 0, never matched).
        let mut gy = Vec::new();
        for _ in 0..2 {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            gy.push(state.add_card(PlayerId(0), c, Zone::Graveyard));
        }
        let m = state.card_db().get(grp::MOUNTAIN).unwrap().chars.clone();
        state.add_card(PlayerId(0), m, Zone::Graveyard);
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let e = Engine::new(state, vec![Box::new(XAgent(x)), Box::new(RandomAgent::new(1))]);
        (e, fix, gy)
    }

    /// Real cast: X=2 pays 2 life and returns each MV-exactly-2 artifact/creature card (both bears)
    /// from the graveyard to the battlefield; the Mountain (MV 0) stays in the graveyard.
    #[test]
    fn pays_x_life_and_reanimates_mv_exactly_x() {
        let (mut e, fix, gy) = setup(2);
        let life0 = e.state.player(PlayerId(0)).life;
        let gy_before = e.state.player(PlayerId(0)).graveyard.len();
        assert_eq!(gy_before, 3, "two bears + a Mountain");

        e.cast_spell(PlayerId(0), fix, CastVariant::Normal);
        assert_eq!(e.state.player(PlayerId(0)).life, life0 - 2, "paid X=2 life at cast");
        e.resolve_top();

        for &b in &gy {
            assert!(e.state.player(PlayerId(0)).battlefield.contains(&b), "MV-2 bear reanimated");
            assert!(!e.state.player(PlayerId(0)).graveyard.contains(&b), "left the graveyard");
        }
        // The Mountain (MV 0 ≠ 2) stayed in the graveyard; Fix What's Broken itself resolves there
        // too — so the graveyard now holds those two.
        assert_eq!(e.state.player(PlayerId(0)).graveyard.len(), 2, "Mountain + the resolved sorcery");
    }

    /// The MV bound is *exact*: with X=3 the MV-2 bears are not returned (nothing MV==3).
    #[test]
    fn x_three_returns_nothing_when_no_mv_three_cards() {
        let (mut e, fix, gy) = setup(3);
        e.cast_spell(PlayerId(0), fix, CastVariant::Normal);
        e.resolve_top();
        for &b in &gy {
            assert!(e.state.player(PlayerId(0)).graveyard.contains(&b), "MV 2 ≠ 3 → stays in graveyard");
        }
    }
}
