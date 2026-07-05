//! Quandrix Charm — `{G}{U}` Instant (first printed SOS).
//!
//! Oracle: "Choose one —
//!   • Counter target spell unless its controller pays {2}.
//!   • Destroy target enchantment.
//!   • Target creature has base power and toughness 5/5 until end of turn."
//!
//! **Fully implemented** — a `Modal` "choose one". Modes 1–2 reuse existing machinery
//! (`CounterUnlessPay` over a stack target; `Destroy` a target enchantment). Mode 3 is the lander
//! for **`Effect::SetBasePT` until end of turn** (CR 613 layer 7b) — it lowers to the existing
//! `GrantContinuous{ SetBasePT }` continuous-effect path, so a later +N/+N still stacks on top and
//! the effect expires at cleanup. Multicolored (G/U).

use crate::basics::{CardType, Color};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::ability::Cost;
use crate::effects::condition::Duration;
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::{Effect, EffectTarget, Mode};

/// grp id (per-set ids live near their cards).
pub const QUANDRIX_CHARM: u32 = 415;

fn one_target(kind: TargetKind) -> EffectTarget {
    EffectTarget::Target(TargetSpec { kind, min: 1, max: 1, distinct: true })
}

pub fn register(db: &mut CardDb) {
    let effect = Effect::Modal {
        modes: vec![
            Mode {
                label: "Counter target spell unless its controller pays {2}".to_string(),
                effect: Effect::CounterUnlessPay {
                    what: one_target(TargetKind::StackObject(CardFilter::Any)),
                    cost: Cost { mana: Some(mana_cost(2, &[])), components: vec![] },
                },
            },
            Mode {
                label: "Destroy target enchantment".to_string(),
                effect: Effect::Destroy {
                    what: one_target(TargetKind::Permanent(CardFilter::HasCardType(
                        CardType::Enchantment,
                    ))),
                },
            },
            Mode {
                label: "Target creature has base power and toughness 5/5 until end of turn".to_string(),
                effect: Effect::SetBasePT {
                    what: one_target(TargetKind::Creature(CardFilter::Any)),
                    power: 5,
                    toughness: 5,
                    duration: Duration::UntilEndOfTurn,
                },
            },
        ],
        min: 1,
        max: 1,
        allow_repeat: false,
    };
    let mut def = spell(
        QUANDRIX_CHARM,
        "Quandrix Charm",
        CardType::Instant,
        Color::Green,
        mana_cost(0, &[(Color::Green, 1), (Color::Blue, 1)]),
        effect,
    )
    .with_text("Choose one —\n• Counter target spell unless its controller pays {2}.\n• Destroy target enchantment.\n• Target creature has base power and toughness 5/5 until end of turn.");
    def.chars.colors = vec![Color::Green, Color::Blue];
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, CastVariant, DecisionRequest, DecisionResponse, PlayerView, RandomAgent};
    use crate::basics::{Phase, Target};
    use crate::cards::{build_game, grp};
    use crate::ids::{ObjId, PlayerId};
    use crate::priority::Engine;

    #[test]
    fn quandrix_charm_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(QUANDRIX_CHARM).unwrap();
        assert_eq!(def.chars.colors, vec![Color::Green, Color::Blue]);
        assert!(def.fully_implemented);
        let Some(Effect::Modal { modes, .. }) = def.spell_effect() else { panic!("modal") };
        assert_eq!(modes.len(), 3);
        assert!(matches!(modes[0].effect, Effect::CounterUnlessPay { .. }));
        assert!(matches!(modes[1].effect, Effect::Destroy { .. }));
        assert!(matches!(
            modes[2].effect,
            Effect::SetBasePT { power: 5, toughness: 5, duration: Duration::UntilEndOfTurn, .. }
        ));
    }

    /// An agent that chooses mode `mode` and targets `pick` (a creature), else passes.
    struct CharmAgent {
        mode: u32,
        pick: ObjId,
    }
    impl Agent for CharmAgent {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::ChooseModes { .. } => DecisionResponse::Indices(vec![self.mode]),
                DecisionRequest::ChooseTargets { slots, .. } => {
                    let legal = &slots[0].legal;
                    let idx = legal
                        .iter()
                        .position(|t| *t == Target::Object(self.pick))
                        .unwrap_or(0);
                    DecisionResponse::Pairs(vec![(0, idx as u32)])
                }
                _ => DecisionResponse::Pass,
            }
        }
    }

    /// Mode 3 real cast: a 2/2 Grizzly Bears becomes base 5/5 (computed through the layer system),
    /// and a +1/+1 counter still stacks on top (layer 7b base-set, then layer 7c counter) → 6/6.
    #[test]
    fn mode3_sets_base_pt() {
        use crate::basics::{CounterKind, Zone};
        let mut state = build_game(1, &[&[], &[]]);
        let charm = state.add_card(
            PlayerId(0),
            state.card_db().get(QUANDRIX_CHARM).unwrap().chars.clone(),
            Zone::Hand,
        );
        // {G}{U}: a Forest + an Island.
        for grp_id in [grp::FOREST, grp::ISLAND] {
            let c = state.card_db().get(grp_id).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield);
        }
        // A 2/2 Grizzly Bears with a +1/+1 counter → currently 3/3.
        let bears = {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        state.objects.get_mut(&bears).unwrap().counters.counts.insert(CounterKind::PlusOnePlusOne, 1);
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;

        let mut e = Engine::new(state, vec![Box::new(CharmAgent { mode: 2, pick: bears }), Box::new(RandomAgent::new(1))]);
        assert_eq!(e.state.computed(bears).power, Some(3), "2/2 + a counter = 3/3 before");

        e.cast_spell(PlayerId(0), charm, CastVariant::Normal);
        e.resolve_top();
        // Base set to 5/5 (layer 7b), then the +1/+1 counter (layer 7c) stacks on top → 6/6.
        assert_eq!(e.state.computed(bears).power, Some(6), "base 5 + counter 1 = 6");
        assert_eq!(e.state.computed(bears).toughness, Some(6), "base 5 + counter 1 = 6");
    }
}
