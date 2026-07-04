//! Render Speechless — `{2}{W}{B}` Sorcery (first printed SOS).
//!
//! Oracle: "Target opponent reveals their hand. You choose a nonland card from it. That player
//! discards that card. Put two +1/+1 counters on up to one target creature."
//!
//! **Fully implemented** — two targeting slots:
//! - slot 0 = `TargetPlayer(Opponent)` → a `DirectedDiscard` where the CASTER (`chooser`) picks a
//!   nonland card (`Not(HasCardType(Land))`) from that opponent's hand and the opponent discards it.
//!   Unlike a plain `Discard` (the discarding player chooses), directed discard is the "you choose"
//!   variant (CR 701.8) — a new general leaf.
//! - slot 1 = `PutCounters` on an "up to one target creature" (`min: 0, max: 1`), two +1/+1 counters.

use crate::basics::{CardType, Color, CounterKind};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::target::{CardFilter, PlayerFilter, TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};

/// grp id (per-set ids live near their cards).
pub const RENDER_SPEECHLESS: u32 = 354;

pub fn register(db: &mut CardDb) {
    let effect = Effect::Sequence(vec![
        // slot 0 — "target opponent" (reveals + discards).
        Effect::TargetPlayer(PlayerFilter::Opponent),
        // "…reveals their hand. You choose a nonland card from it. That player discards that card."
        Effect::DirectedDiscard {
            who: PlayerRef::ChosenTarget(0),
            chooser: PlayerRef::Controller,
            count: ValueExpr::Fixed(1),
            filter: CardFilter::Not(Box::new(CardFilter::HasCardType(CardType::Land))),
        },
        // slot 1 — "put two +1/+1 counters on up to one target creature."
        Effect::PutCounters {
            what: EffectTarget::Target(TargetSpec {
                kind: TargetKind::Creature(CardFilter::Any),
                min: 0,
                max: 1,
                distinct: true,
            }),
            kind: CounterKind::PlusOnePlusOne,
            n: ValueExpr::Fixed(2),
        },
    ]);
    let mut def = spell(
        RENDER_SPEECHLESS,
        "Render Speechless",
        CardType::Sorcery,
        Color::White,
        mana_cost(2, &[(Color::White, 1), (Color::Black, 1)]),
        effect,
    )
    .with_text(
        "Target opponent reveals their hand. You choose a nonland card from it. That player discards that card. Put two +1/+1 counters on up to one target creature.",
    );
    def.chars.colors = vec![Color::White, Color::Black];
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView, RandomAgent};
    use crate::basics::{Target, Zone};
    use crate::cards::{build_game, grp};
    use crate::effects::action::{ResolutionCtx, WbReason};
    use crate::ids::{PlayerId, StackId};
    use crate::priority::Engine;
    use expect_test::expect;

    #[test]
    fn render_speechless_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(RENDER_SPEECHLESS).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Sorcery]);
        assert_eq!(def.chars.colors, vec![Color::White, Color::Black]);
        assert!(def.fully_implemented);
        expect![[r#"
            Sequence(
                [
                    TargetPlayer(
                        Opponent,
                    ),
                    DirectedDiscard {
                        who: ChosenTarget(
                            0,
                        ),
                        chooser: Controller,
                        count: Fixed(
                            1,
                        ),
                        filter: Not(
                            HasCardType(
                                Land,
                            ),
                        ),
                    },
                    PutCounters {
                        what: Target(
                            TargetSpec {
                                kind: Creature(
                                    Any,
                                ),
                                min: 0,
                                max: 1,
                                distinct: true,
                            },
                        ),
                        kind: PlusOnePlusOne,
                        n: Fixed(
                            2,
                        ),
                    },
                ],
            )"#]]
        .assert_eq(&format!("{:#?}", def.spell_effect().unwrap()));
    }

    /// Behaviour (direct resolve): opponent (P1) holds a land + two nonland cards; the caster (P0)
    /// chooses which nonland the opponent discards. The scripted P0 picks index 1 of the eligible
    /// (nonland) set; that card ends in P1's graveyard and the land is untouched.
    #[test]
    fn caster_chooses_opponents_discard() {
        let mut state = build_game(1, &[&[], &[]]);
        // P1's hand: a Forest (land, ineligible) + two Grizzly Bears (nonland, eligible).
        let forest = state.card_db().get(grp::FOREST).unwrap().chars.clone();
        let bears = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
        let land = state.add_card(PlayerId(1), forest, Zone::Hand);
        let bear_a = state.add_card(PlayerId(1), bears.clone(), Zone::Hand);
        let bear_b = state.add_card(PlayerId(1), bears, Zone::Hand);
        let effect = state.card_db().get(RENDER_SPEECHLESS).unwrap().spell_effect().unwrap().clone();

        // Caster (P0) is asked to Reveal-choose from the eligible [bear_a, bear_b]; pick index 1.
        struct PickSecond;
        impl Agent for PickSecond {
            fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
                match req {
                    DecisionRequest::SelectCards { .. } => DecisionResponse::Indices(vec![1]),
                    _ => DecisionResponse::Pass,
                }
            }
        }
        let mut e = Engine::new(state, vec![Box::new(PickSecond), Box::new(RandomAgent::new(1))]);
        e.resolve_effect(
            &effect,
            &ResolutionCtx {
                controller: Some(PlayerId(0)),
                // slot 0 = the target opponent (P1); slot 1 = no creature chosen (up-to-one, empty).
                chosen_targets: vec![Target::Player(PlayerId(1))],
                ..Default::default()
            },
            WbReason::Resolve(StackId(0)),
        );
        assert!(e.state.player(PlayerId(1)).graveyard.contains(&bear_b), "chosen nonland discarded");
        assert!(e.state.player(PlayerId(1)).hand.contains(&bear_a), "other nonland kept");
        assert!(e.state.player(PlayerId(1)).hand.contains(&land), "land can't be chosen");
    }
}
