//! End of the Hunt — `{1}{B}` Sorcery (first printed SOS).
//!
//! Oracle: "Target opponent exiles a creature or planeswalker they control with the greatest mana
//! value among creatures and planeswalkers they control."
//!
//! **Fully implemented** — an edict keyed to the **greatest mana value**: `TargetPlayer(Opponent)`
//! then that player exiles one of their own creatures/planeswalkers whose mana value equals the max
//! among their creatures/planeswalkers. Built from the new **`ValueExpr::GreatestManaValue`** (max
//! MV over a controller-scoped filtered set) feeding a dynamic **`CardFilter::ManaValueExpr`**
//! (`{min: greatest, max: greatest}`) — so the selection is exactly the max-MV objects, and the
//! target player chooses among ties. Reuses the resolution-time `Exile{Select}` path.

use crate::basics::{CardType, Color, Zone};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::target::{CardFilter, PlayerFilter, SelectSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};

/// grp id (per-set ids live near their cards).
pub const END_OF_THE_HUNT: u32 = 416;

/// "creatures and planeswalkers" — the filter shared by the count and the selection.
fn creatures_and_planeswalkers() -> CardFilter {
    CardFilter::AnyOf(vec![
        CardFilter::HasCardType(CardType::Creature),
        CardFilter::HasCardType(CardType::Planeswalker),
    ])
}

pub fn register(db: &mut CardDb) {
    // The greatest mana value among the target opponent's creatures/planeswalkers.
    let greatest = ValueExpr::GreatestManaValue {
        filter: creatures_and_planeswalkers(),
        controller: Some(PlayerRef::ChosenTarget(0)),
    };
    let effect = Effect::Sequence(vec![
        Effect::TargetPlayer(PlayerFilter::Opponent),
        Effect::Exile {
            what: EffectTarget::Select(SelectSpec {
                zone: Zone::Battlefield,
                filter: CardFilter::All(vec![
                    creatures_and_planeswalkers(),
                    // Mana value == the greatest among their creatures/planeswalkers.
                    CardFilter::ManaValueExpr {
                        min: Some(Box::new(greatest.clone())),
                        max: Some(Box::new(greatest)),
                    },
                ]),
                // The target opponent chooses (among ties) and it's their own permanent.
                chooser: PlayerRef::ChosenTarget(0),
                min: ValueExpr::Fixed(1),
                max: ValueExpr::Fixed(1),
            }),
        },
    ]);
    let def = spell(
        END_OF_THE_HUNT,
        "End of the Hunt",
        CardType::Sorcery,
        Color::Black,
        mana_cost(1, &[(Color::Black, 1)]),
        effect,
    )
    .with_text("Target opponent exiles a creature or planeswalker they control with the greatest mana value among creatures and planeswalkers they control.");
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, CastVariant, DecisionRequest, DecisionResponse, PlayerView, RandomAgent};
    use crate::basics::{Phase, Target};
    use crate::cards::{build_game, grp};
    use crate::cards::sos::soaring_stoneglider::SOARING_STONEGLIDER;
    use crate::ids::{ObjId, PlayerId};
    use crate::priority::Engine;

    /// An agent that targets opponent `opp` at ChooseTargets (a player slot), and takes the first
    /// offered at SelectCards, else passes.
    struct HuntAgent(PlayerId);
    impl Agent for HuntAgent {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::ChooseTargets { slots, .. } => {
                    let legal = &slots[0].legal;
                    let idx = legal
                        .iter()
                        .position(|t| *t == Target::Player(self.0))
                        .unwrap_or(0);
                    DecisionResponse::Pairs(vec![(0, idx as u32)])
                }
                DecisionRequest::SelectCards { min, max, from, .. } => {
                    let n = (*min).max(1).min(*max).min(from.len() as u32);
                    DecisionResponse::Indices((0..n).collect())
                }
                _ => DecisionResponse::Pass,
            }
        }
    }

    #[test]
    fn end_of_the_hunt_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(END_OF_THE_HUNT).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Sorcery]);
        assert!(def.fully_implemented);
        let Some(Effect::Sequence(seq)) = def.spell_effect() else { panic!("sequence") };
        assert!(matches!(seq[0], Effect::TargetPlayer(PlayerFilter::Opponent)));
        assert!(matches!(seq[1], Effect::Exile { .. }));
    }

    /// Real cast: the target opponent (P1) has a MV-2 Grizzly Bears and a MV-3 Soaring Stoneglider;
    /// End of the Hunt makes them exile the greatest-MV one (the Stoneglider), sparing the Bears.
    #[test]
    fn exiles_the_greatest_mana_value_creature() {
        let mut state = build_game(1, &[&[], &[]]);
        let hunt = state.add_card(
            PlayerId(0),
            state.card_db().get(END_OF_THE_HUNT).unwrap().chars.clone(),
            Zone::Hand,
        );
        // {1}{B}: two Swamps.
        for _ in 0..2 {
            let c = state.card_db().get(grp::SWAMP).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield);
        }
        // P1: a MV-2 Grizzly Bears and a MV-3 Soaring Stoneglider.
        let bears = {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            state.add_card(PlayerId(1), c, Zone::Battlefield)
        };
        let big: ObjId = {
            let c = state.card_db().get(SOARING_STONEGLIDER).unwrap().chars.clone();
            state.add_card(PlayerId(1), c, Zone::Battlefield)
        };
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;

        let mut e = Engine::new(
            state,
            vec![Box::new(HuntAgent(PlayerId(1))), Box::new(RandomAgent::new(1))],
        );
        e.cast_spell(PlayerId(0), hunt, CastVariant::Normal);
        e.resolve_top();

        assert!(!e.state.player(PlayerId(1)).battlefield.contains(&big), "greatest-MV (3) creature exiled");
        assert!(e.state.player(PlayerId(1)).exile.contains(&big), "…to exile");
        assert!(e.state.player(PlayerId(1)).battlefield.contains(&bears), "the MV-2 Bears survives");
    }
}
