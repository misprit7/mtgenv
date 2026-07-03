//! Surrak, Elusive Hunter — `{2}{G}` Legendary Creature — Human Warrior 4/3 (first printed TDM,
//! Tarkir: Dragonstorm; `tdm` is the expansion, `ptdm` is the promo printing).
//!
//! Oracle:
//!   This spell can't be countered.
//!   Trample
//!   Whenever a creature you control or a creature spell you control becomes the target of a spell
//!   or ability an opponent controls, draw a card.
//!
//! IMPLEMENTED:
//! - **Trample** (CR 702.19) — a printed `Keyword`, read by combat-damage assignment today.
//! - 4/3 P/T, Legendary supertype, Human Warrior subtypes (printed characteristics).
//! - **"Whenever a creature you control or a creature spell you control becomes the target of a spell
//!   or ability an opponent controls, draw a card"** — a `Triggered{ BecomesTargeted{ filter: creature
//!   you control, by_opponent: true } }` → `Draw 1`. Covers **both halves**: the battlefield-creature
//!   half (C16, `8d006fd`) and the **creature-spell-on-stack half** (cap `d3ee9e9`) — the engine now
//!   fires `BecomesTargeted` for stack objects too, and the same filter (`HasCardType(Creature) +
//!   ControlledBy(Controller)`) matches a creature *spell* on the stack, so no IR change was needed.
//!   Fires once per matching object that becomes a target (CR 603.2e).
//!
//! **FULLY IMPLEMENTED** (`fully_implemented: true`) — every clause now lands:
//!   - **"This spell can't be countered."** The CR-correct static ability that functions while the
//!     spell is on the stack (CR 113.6f / 604.5): a `Qualification(CantBeCountered)` painted on
//!     `ItSelf` in `Zone::Stack`. Previously inert (the standing "G" deferral), now live on two new
//!     caps built for the SOS push: (a) `chars::gather_statics` also gathers stack-zone statics from
//!     spells on the stack, so Surrak's spell reads `CantBeCountered`; (b) `Effect::Counter`
//!     (Essence Scatter) checks that qualification (CR 701.5f) and leaves the spell on the stack.
//!     Regression-tested by `m10::essence_scatter::essence_scatter_cannot_counter_surrak`.

use crate::basics::{CardType, Color, Zone};
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern, Keyword, Qualification, StaticContribution};
use crate::effects::condition::Duration;
use crate::effects::target::{CardFilter, SelectSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;
use crate::subtypes::{CreatureType, Supertype};

/// grp id (per-set ids live near their cards).
pub const SURRAK_ELUSIVE_HUNTER: u32 = 112;

/// "This spell can't be countered." — a static ability that functions while the spell is on the
/// stack (CR 604.5 / 113.6f), painting the `CantBeCountered` qualification on the spell itself.
/// Inert until the engine gathers stack-zone statics and a counter check reads the marker (tracked).
fn cant_be_countered() -> Ability {
    Ability::Static {
        contribution: StaticContribution::Qualification(Qualification::CantBeCountered),
        affects: SelectSpec {
            zone: Zone::Stack,
            filter: CardFilter::ItSelf,
            chooser: PlayerRef::Controller,
            // min/max are unused for statics (the marker applies to every match — here, itself).
            min: ValueExpr::Fixed(0),
            max: ValueExpr::Fixed(0),
        },
        duration: Duration::WhileSourcePresent,
    }
}

/// "Whenever a creature you control or a creature spell you control becomes the target of a spell or
/// ability an opponent controls, draw a card." Fires once per matching object (CR 603.2e) — the same
/// filter covers both the battlefield-creature half (C16) and the creature-spell-on-stack half (d3ee9e9).
fn becomes_targeted_draw() -> Ability {
    Ability::Triggered {
        event: EventPattern::BecomesTargeted {
            filter: CardFilter::All(vec![
                CardFilter::HasCardType(CardType::Creature),
                CardFilter::ControlledBy(PlayerRef::Controller),
            ]),
            by_opponent: true,
        },
        condition: None,
        intervening_if: false,
        effect: Effect::Draw { who: PlayerRef::Controller, count: ValueExpr::Fixed(1) },
    }
}

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        SURRAK_ELUSIVE_HUNTER,
        "Surrak, Elusive Hunter",
        &[CreatureType::Human, CreatureType::Warrior],
        Color::Green,
        mana_cost(2, &[(Color::Green, 1)]),
        4,
        3,
        vec![cant_be_countered(), becomes_targeted_draw()],
    );
    def.chars.supertypes = vec![Supertype::Legendary];
    def.chars.keywords = vec![Keyword::Trample];
    def.text = "This spell can't be countered.\nTrample\nWhenever a creature you control or a creature spell you control becomes the target of a spell or ability an opponent controls, draw a card.".to_string();
    // Fully faithful: can't-be-countered now lands (stack-zone static gathering + `Effect::Counter`
    // reading `CantBeCountered`, both built for the SOS push); the draw trigger covers both halves
    // (battlefield creature + creature spell on the stack, cap d3ee9e9). No remaining deferral.
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::basics::CardType;
    use expect_test::expect;

    #[test]
    fn surrak_elusive_hunter_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(SURRAK_ELUSIVE_HUNTER).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Creature]);
        assert_eq!(def.chars.supertypes, vec![Supertype::Legendary]);
        assert_eq!(def.chars.keywords, vec![Keyword::Trample]); // trample works today
        assert_eq!(def.chars.power, Some(4));
        assert_eq!(def.chars.toughness, Some(3));
        // Fully faithful now: can't-be-countered lands (stack-static gathering + Effect::Counter check),
        // and the draw trigger covers both the battlefield-creature and creature-spell-on-stack halves.
        assert!(def.fully_implemented);
        // The can't-be-countered static + the becomes-targeted draw trigger (battlefield half, C16).
        expect![[r#"
            [
                Static {
                    contribution: Qualification(
                        CantBeCountered,
                    ),
                    affects: SelectSpec {
                        zone: Stack,
                        filter: ItSelf,
                        chooser: Controller,
                        min: Fixed(
                            0,
                        ),
                        max: Fixed(
                            0,
                        ),
                    },
                    duration: WhileSourcePresent,
                },
                Triggered {
                    event: BecomesTargeted {
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
                        by_opponent: true,
                    },
                    condition: None,
                    intervening_if: false,
                    effect: Draw {
                        who: Controller,
                        count: Fixed(
                            1,
                        ),
                    },
                },
            ]"#]]
        .assert_eq(&format!("{:#?}", def.abilities));
    }

    /// #60 end-to-end (REAL opponent spell → becomes-targeted trigger): "Whenever a creature you
    /// control … becomes the target of a spell or ability an opponent controls, draw a card." The
    /// opponent (P1) casts Erode targeting a creature P0 controls; on targeting (CR 603.2e), Surrak's
    /// trigger fires and P0 draws. Driven via P1 `cast_spell` (real `{W}`, ChooseTargets) → `run_agenda`
    /// (stacks the draw trigger) → `resolve_top`. This whole clause had no behaviour test before.
    #[test]
    fn surrak_becomes_targeted_draw_via_opponent_spell() {
        use crate::agent::{Agent, CastVariant, DecisionRequest, DecisionResponse, PlayerView};
        use crate::basics::{Target, Zone};
        use crate::cards::sos::erode::ERODE;
        use crate::cards::{grp, starter_db};
        use crate::ids::{ObjId, PlayerId};
        use crate::priority::Engine;
        use crate::state::GameState;
        use std::sync::Arc;

        // P1 targets the chosen creature with Erode; both seats take any offered "may" fetch.
        #[derive(Clone)]
        struct PlayAgent {
            want: ObjId,
        }
        impl Agent for PlayAgent {
            fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
                match req {
                    DecisionRequest::ChooseTargets { slots, .. } => {
                        let i = slots[0]
                            .legal
                            .iter()
                            .position(|t| matches!(t, Target::Object(o) if *o == self.want))
                            .unwrap_or(0);
                        DecisionResponse::Pairs(vec![(0, i as u32)])
                    }
                    DecisionRequest::Confirm { .. } => DecisionResponse::Bool(true),
                    DecisionRequest::SelectCards { from, min, max, .. } => {
                        let n = (*min).max(1).min(*max).min(from.len() as u32);
                        DecisionResponse::Indices((0..n).collect())
                    }
                    _ => DecisionResponse::Pass,
                }
            }
        }

        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(starter_db()));
        // P0 controls Surrak + a bystander creature, and has a card to draw.
        {
            let c = state.card_db().get(SURRAK_ELUSIVE_HUNTER).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield);
        }
        let victim = {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        {
            let c = state.card_db().get(grp::FOREST).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Library); // the card Surrak's trigger draws
        }
        // P1 (opponent) casts Erode {W} at P0's creature.
        let erode = {
            let c = state.card_db().get(ERODE).unwrap().chars.clone();
            state.add_card(PlayerId(1), c, Zone::Hand)
        };
        {
            let c = state.card_db().get(grp::PLAINS).unwrap().chars.clone();
            state.add_card(PlayerId(1), c, Zone::Battlefield); // pays {W}
        }
        let mut e = Engine::new(
            state,
            vec![Box::new(PlayAgent { want: victim }), Box::new(PlayAgent { want: victim })],
        );
        assert_eq!(e.state.players[0].hand.len(), 0, "P0 starts with an empty hand");

        e.cast_spell(PlayerId(1), erode, CastVariant::Normal); // targets P0's creature → Surrak triggers
        e.run_agenda();
        while !e.state.stack.items.is_empty() {
            e.resolve_top();
            e.run_agenda();
        }

        assert_eq!(
            e.state.players[0].hand.len(),
            1,
            "Surrak's controller drew a card when their creature became the target of an opponent's spell"
        );
        assert!(e.state.players[0].library.is_empty(), "the draw came from P0's library");
    }
}
