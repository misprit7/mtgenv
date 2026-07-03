//! Magmablood Archaic — `{2/R}{2/R}{2/R}` Creature — Avatar 2/2 (first printed SOS).
//!
//! Oracle: "Trample, reach / Converge — This creature enters with a +1/+1 counter on it for each
//! color of mana spent to cast it. / Whenever you cast an instant or sorcery spell, creatures you
//! control get +1/+0 until end of turn for each color of mana spent to cast that spell."
//!
//! **Fully implemented** — a monocolour-hybrid cost (`{2/R}` pips, each payable by 2 generic or one
//! red), plus:
//! - Converge enters-with: a self-replacement (CR 614.1e / 702.75) giving `ColorsSpent` +1/+1
//!   counters — the distinct colours of mana spent to cast it, recorded at cast.
//! - The Opus-style I/S cast-trigger mass-pumps every creature you control by `ColorsSpentOnTrigger`
//!   (the colours spent on the *triggering* spell) / +0 until end of turn.

use crate::basics::{CardType, Color, CounterKind, Zone};
use crate::cards::helpers::instant_or_sorcery;
use crate::cards::{creature, mana_cost_mono_hybrid, CardDb};
use crate::effects::ability::{Ability, ActionPattern, EventPattern, Keyword, Rewrite};
use crate::effects::condition::Duration;
use crate::effects::target::{CardFilter, SelectSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const MAGMABLOOD_ARCHAIC: u32 = 307;

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        MAGMABLOOD_ARCHAIC,
        "Magmablood Archaic",
        &[CreatureType::Avatar],
        Color::Red,
        mana_cost_mono_hybrid(0, &[], &[(2, Color::Red), (2, Color::Red), (2, Color::Red)]),
        2,
        2,
        vec![
            // Converge — enters with a +1/+1 counter for each colour of mana spent (CR 614.1e / 702.75).
            Ability::Replacement {
                pattern: ActionPattern::WouldEnterBattlefield(CardFilter::ItSelf),
                rewrite: Rewrite::EntersWithCountersValue {
                    kind: CounterKind::PlusOnePlusOne,
                    n: ValueExpr::ColorsSpent,
                },
            },
            // "Whenever you cast an instant or sorcery spell, creatures you control get +1/+0 UEOT for
            // each colour of mana spent to cast that spell."
            Ability::Triggered {
                event: EventPattern::SpellCast(instant_or_sorcery()),
                condition: None,
                intervening_if: false,
                effect: Effect::ForEach {
                    // "each creature you control" — all of them (max large so `select_for_each` takes
                    // them all without asking; the `creatures_you_control()` helper's max=0 is for
                    // Static `affects`, not `ForEach`).
                    selector: SelectSpec {
                        zone: Zone::Battlefield,
                        filter: CardFilter::All(vec![
                            CardFilter::HasCardType(CardType::Creature),
                            CardFilter::ControlledBy(PlayerRef::Controller),
                        ]),
                        chooser: PlayerRef::Controller,
                        min: ValueExpr::Fixed(0),
                        max: ValueExpr::Fixed(999),
                    },
                    body: Box::new(Effect::PumpPT {
                        what: EffectTarget::Each,
                        power: ValueExpr::ColorsSpentOnTrigger,
                        toughness: ValueExpr::Fixed(0),
                        duration: Duration::UntilEndOfTurn,
                    }),
                },
            },
        ],
    );
    def.chars.keywords = vec![Keyword::Trample, Keyword::Reach];
    def.text = "Trample, reach\nConverge — This creature enters with a +1/+1 counter on it for each color of mana spent to cast it.\nWhenever you cast an instant or sorcery spell, creatures you control get +1/+0 until end of turn for each color of mana spent to cast that spell.".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn magmablood_archaic_shape() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(MAGMABLOOD_ARCHAIC).unwrap();
        assert_eq!(def.chars.colors, vec![Color::Red]);
        assert_eq!(def.chars.keywords, vec![Keyword::Trample, Keyword::Reach]);
        assert_eq!(
            def.chars.mana_cost.as_ref().unwrap().mono_hybrid,
            vec![(2, Color::Red), (2, Color::Red), (2, Color::Red)]
        );
        assert_eq!(def.chars.mana_value(), 6, "{{2/R}}x3 = MV 6 (CR 202.3g)");
        assert!(def.fully_implemented);
        expect![[r#"
            [
                Replacement {
                    pattern: WouldEnterBattlefield(
                        ItSelf,
                    ),
                    rewrite: EntersWithCountersValue {
                        kind: PlusOnePlusOne,
                        n: ColorsSpent,
                    },
                },
                Triggered {
                    event: SpellCast(
                        AnyOf(
                            [
                                HasCardType(
                                    Instant,
                                ),
                                HasCardType(
                                    Sorcery,
                                ),
                            ],
                        ),
                    ),
                    condition: None,
                    intervening_if: false,
                    effect: ForEach {
                        selector: SelectSpec {
                            zone: Battlefield,
                            filter: All(
                                [
                                    HasCardType(
                                        Creature,
                                    ),
                                    ControlledBy(
                                        Controller,
                                    ),
                                ],
                            ),
                            chooser: Controller,
                            min: Fixed(
                                0,
                            ),
                            max: Fixed(
                                999,
                            ),
                        },
                        body: PumpPT {
                            what: Each,
                            power: ColorsSpentOnTrigger,
                            toughness: Fixed(
                                0,
                            ),
                            duration: UntilEndOfTurn,
                        },
                    },
                },
            ]"#]]
        .assert_eq(&format!("{:#?}", def.abilities));
    }

    /// Behaviour: with two colours spent on the triggering spell, the Opus trigger pumps every
    /// creature you control by +2/+0 until end of turn.
    #[test]
    fn magmablood_pumps_your_creatures_by_colors_spent_on_trigger() {
        use crate::agent::RandomAgent;
        use crate::basics::Zone;
        use crate::cards::{build_game, grp};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        let mut state = build_game(1, &[&[], &[]]);
        let self_chars = state.card_db().get(MAGMABLOOD_ARCHAIC).unwrap().chars.clone();
        let src = state.add_card(PlayerId(0), self_chars, Zone::Battlefield);
        let bears = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
        let ally = state.add_card(PlayerId(0), bears, Zone::Battlefield);
        // A stand-in triggering spell on the stack with two distinct colours spent.
        let trig = state.add_card(PlayerId(0), state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Stack);
        state.objects.get_mut(&trig).unwrap().colors_spent = 2;
        let effect = match &state.card_db().get(MAGMABLOOD_ARCHAIC).unwrap().abilities[1] {
            Ability::Triggered { effect, .. } => effect.clone(),
            o => panic!("expected Triggered, got {o:?}"),
        };
        let mut e = Engine::new(
            state,
            vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))],
        );
        e.resolve_effect(
            &effect,
            &ResolutionCtx {
                controller: Some(PlayerId(0)),
                source: Some(src),
                triggering_spell: Some(trig),
                ..Default::default()
            },
            WbReason::Resolve(StackId(0)),
        );
        // Grizzly Bears (2/2) → 4/2; Magmablood (2/2) → 4/2. Both are creatures you control.
        assert_eq!(e.state.computed(ally).power, Some(4), "+2/+0 for two colours spent");
        assert_eq!(e.state.computed(src).power, Some(4), "self also pumped");
        assert_eq!(e.state.computed(ally).toughness, Some(2), "toughness unchanged");
    }

    /// End-to-end: a REAL cast of `{2/R}{2/R}{2/R}` paid with one Mountain (a red side) + four Forests
    /// (two 2-generic sides) spends TWO colours (R, G), so Converge makes it enter with two +1/+1
    /// counters → a 4/4. Exercises the mono-hybrid payment planner + `colors_spent` recording + the
    /// `EntersWithCountersValue{ColorsSpent}` replacement.
    #[test]
    fn magmablood_real_cast_converge_two_colors() {
        use crate::agent::{Agent, CastVariant, DecisionRequest, DecisionResponse, PlayerView};
        use crate::basics::{CounterKind, Zone};
        use crate::cards::{grp, starter_db};
        use crate::ids::PlayerId;
        use crate::priority::Engine;
        use crate::state::GameState;
        use std::sync::Arc;

        #[derive(Clone)]
        struct PassiveAgent;
        impl Agent for PassiveAgent {
            fn decide(&mut self, _v: &PlayerView, _req: &DecisionRequest) -> DecisionResponse {
                DecisionResponse::Pass
            }
        }

        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(starter_db()));
        let magma = {
            let c = state.card_db().get(MAGMABLOOD_ARCHAIC).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Hand)
        };
        // One Mountain (R) + four Forests (G) → {2/R} pays R once, then 2 generic twice = colours {R,G}.
        let mtn = state.card_db().get(grp::MOUNTAIN).unwrap().chars.clone();
        state.add_card(PlayerId(0), mtn, Zone::Battlefield);
        for _ in 0..4 {
            let f = state.card_db().get(grp::FOREST).unwrap().chars.clone();
            state.add_card(PlayerId(0), f, Zone::Battlefield);
        }
        let mut e = Engine::new(state, vec![Box::new(PassiveAgent), Box::new(PassiveAgent)]);
        e.cast_spell(PlayerId(0), magma, CastVariant::Normal);
        e.resolve_top(); // enters → EntersWithCountersValue{ColorsSpent} applies
        e.run_agenda();
        assert_eq!(
            e.state.object(magma).counters.get(&CounterKind::PlusOnePlusOne),
            2,
            "two distinct colours spent (R + G) → two +1/+1 counters"
        );
        assert_eq!(e.state.computed(magma).power, Some(4), "2/2 base + two +1/+1 = 4/4");
    }
}
