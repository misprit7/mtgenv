//! Vicious Rivalry — `{2}{B}{G}` Sorcery (first printed SOS).
//!
//! Oracle: "As an additional cost to cast this spell, pay X life. Destroy all artifacts and
//! creatures with mana value X or less."
//!
//! **Fully implemented** — the lander for the **X-in-an-additional-cost** shape (CR 601.2b/f +
//! 107.3): the spell's mana cost has no `{X}`, yet an additional cost pays X life, so the engine
//! announces X at cast (the shared chosen X, `ValueExpr::X`), pays that much life through the real
//! cost machinery, and the effect reads the same X. The board wipe is a `ForEach` over every
//! artifact/creature both players control (the `PlayerRef::EachPlayer` area selector) whose mana
//! value is `X` or less — a **dynamic** `CardFilter::ManaValueExpr` resolved against the chosen X.

use crate::basics::{CardType, Color, Zone};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::ability::{AdditionalCost, Ability, Cost, CostComponent};
use crate::effects::target::{CardFilter, SelectSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};

/// grp id (per-set ids live near their cards).
pub const VICIOUS_RIVALRY: u32 = 412;

pub fn register(db: &mut CardDb) {
    // Every artifact/creature in play (both players) with mana value ≤ X.
    let selector = SelectSpec {
        zone: Zone::Battlefield,
        filter: CardFilter::All(vec![
            CardFilter::AnyOf(vec![
                CardFilter::HasCardType(CardType::Artifact),
                CardFilter::HasCardType(CardType::Creature),
            ]),
            CardFilter::ManaValueExpr { min: None, max: Some(Box::new(ValueExpr::X)) },
        ]),
        chooser: PlayerRef::EachPlayer,
        min: ValueExpr::Fixed(0),
        max: ValueExpr::Fixed(999),
    };
    let effect = Effect::ForEach {
        selector,
        body: Box::new(Effect::Destroy { what: EffectTarget::Each }),
    };
    let mut def = spell(
        VICIOUS_RIVALRY,
        "Vicious Rivalry",
        CardType::Sorcery,
        Color::Black,
        mana_cost(2, &[(Color::Black, 1), (Color::Green, 1)]),
        effect,
    )
    .with_text(
        "As an additional cost to cast this spell, pay X life.\nDestroy all artifacts and creatures with mana value X or less.",
    );
    def.chars.colors = vec![Color::Black, Color::Green];
    // "As an additional cost to cast this spell, pay X life." (CR 601.2b) — X is the spell's single
    // chosen value (CR 107.3), announced at cast even though the mana cost has no {X}.
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
    use expect_test::expect;

    #[test]
    fn vicious_rivalry_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(VICIOUS_RIVALRY).unwrap();
        assert_eq!(def.chars.colors, vec![Color::Black, Color::Green]);
        assert!(def.chars.mana_cost.as_ref().unwrap().x == 0, "no {{X}} in the printed mana cost");
        assert!(def.fully_implemented);
        let ac = def.additional_costs();
        assert!(matches!(ac[0].options[0].components[0], CostComponent::PayLife(ValueExpr::X)));
        expect![[r#"
            ForEach {
                selector: SelectSpec {
                    zone: Battlefield,
                    filter: All(
                        [
                            AnyOf(
                                [
                                    HasCardType(
                                        Artifact,
                                    ),
                                    HasCardType(
                                        Creature,
                                    ),
                                ],
                            ),
                            ManaValueExpr {
                                min: None,
                                max: Some(
                                    X,
                                ),
                            },
                        ],
                    ),
                    chooser: EachPlayer,
                    min: Fixed(
                        0,
                    ),
                    max: Fixed(
                        999,
                    ),
                },
                body: Destroy {
                    what: Each,
                },
            }"#]]
        .assert_eq(&format!("{:#?}", def.spell_effect().unwrap()));
    }

    /// An agent that answers `ChooseNumber` (the X announcement) with a fixed value, else passes.
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

    /// Put Vicious Rivalry in P0's hand with {2}{B}{G} of mana, and a Grizzly Bears (MV 2) on each
    /// side of the board. Returns `(engine, rivalry, p0_bear, p1_bear)`.
    fn setup(x: i64) -> (Engine, ObjId, ObjId, ObjId) {
        let mut state = build_game(1, &[&[], &[]]);
        let rivalry = state.add_card(
            PlayerId(0),
            state.card_db().get(VICIOUS_RIVALRY).unwrap().chars.clone(),
            Zone::Hand,
        );
        // {2}{B}{G}: two Swamps + two Forests pay the black/green pips + 2 generic.
        for grp_id in [grp::SWAMP, grp::SWAMP, grp::FOREST, grp::FOREST] {
            let c = state.card_db().get(grp_id).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield);
        }
        let bear = |state: &mut crate::state::GameState, owner: PlayerId| {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            state.add_card(owner, c, Zone::Battlefield)
        };
        let p0_bear = bear(&mut state, PlayerId(0));
        let p1_bear = bear(&mut state, PlayerId(1));
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let e = Engine::new(state, vec![Box::new(XAgent(x)), Box::new(RandomAgent::new(1))]);
        (e, rivalry, p0_bear, p1_bear)
    }

    /// Real cast: choosing X=2 pays 2 life at cast and destroys every MV≤2 artifact/creature (both
    /// Grizzly Bears, MV 2), across both players.
    #[test]
    fn pays_x_life_and_wipes_mv_at_most_x() {
        let (mut e, rivalry, p0_bear, p1_bear) = setup(2);
        let life0 = e.state.player(PlayerId(0)).life;
        e.cast_spell(PlayerId(0), rivalry, CastVariant::Normal);
        // X life paid AT CAST (before resolution).
        assert_eq!(e.state.player(PlayerId(0)).life, life0 - 2, "paid X=2 life at cast");
        e.resolve_top();
        assert!(!e.state.player(PlayerId(0)).battlefield.contains(&p0_bear), "own MV-2 bear destroyed");
        assert!(!e.state.player(PlayerId(1)).battlefield.contains(&p1_bear), "opp MV-2 bear destroyed");
    }

    /// The MV bound is dynamic: with X=1 the MV-2 bears survive (nothing MV≤1 to destroy).
    #[test]
    fn x_one_spares_the_mv_two_creatures() {
        let (mut e, rivalry, p0_bear, p1_bear) = setup(1);
        let life0 = e.state.player(PlayerId(0)).life;
        e.cast_spell(PlayerId(0), rivalry, CastVariant::Normal);
        assert_eq!(e.state.player(PlayerId(0)).life, life0 - 1, "paid X=1 life");
        e.resolve_top();
        assert!(e.state.player(PlayerId(0)).battlefield.contains(&p0_bear), "MV 2 > X 1 → survives");
        assert!(e.state.player(PlayerId(1)).battlefield.contains(&p1_bear), "MV 2 > X 1 → survives");
    }
}
