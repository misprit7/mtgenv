//! Resonating Lute — `{2}{U}{R}` Artifact.
//!
//! Oracle:
//!   Lands you control have "{T}: Add two mana of any one color. Spend this mana only to cast instant
//!     and sorcery spells."
//!   {T}: Draw a card. Activate only if you have seven or more cards in your hand.
//!
//! **Fully implemented** (both abilities are plain `{T}` — no cost-bearing "option-B" caveat):
//! - The grant is a printed [`Ability::Static`] carrying [`StaticContribution::GrantTapMana`] over
//!   "lands you control". It's a **layer-6 ability grant** with no home in `ComputedChars` (a mana
//!   ability isn't a keyword/type/colour), so the mana-payment enumeration reads it directly via
//!   [`crate::chars::granted_tap_mana`] — making the granted `{T}: Add …` visible to affordability
//!   and auto-pay. The `any_color: 2` count is honoured through the whole payment path (a land taps
//!   for **two** mana of one chosen colour; surplus floats in the restricted bucket), and the
//!   `InstantSorceryOnly` spend-restriction gates it to I/S casts (CR 106.6).
//! - The draw ability is a plain `{T}` gated by a hand-size restriction (`≥ 7`).

use crate::basics::{CardType, Color};
use crate::cards::{mana_cost, CardDb, CardDef};
use crate::effects::ability::{Ability, Cost, CostComponent, Restriction, StaticContribution, Timing};
use crate::effects::condition::{Condition, Duration};
use crate::effects::target::{CardFilter, ManaSpec, SelectSpec, SpendRestriction};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};
use crate::state::Characteristics;

/// grp id (per-set ids live near their cards).
pub const RESONATING_LUTE: u32 = 503;

/// "Lands you control have '{T}: Add two mana of any one color. Spend this mana only to cast instant
/// and sorcery spells.'" — a static that grants a tap-mana ability to the group (CR 613.1f).
fn grant_lands_two_any_is() -> Ability {
    Ability::Static {
        contribution: StaticContribution::GrantTapMana {
            mana: ManaSpec {
                produces: vec![],
                any_color: Some(ValueExpr::Fixed(2)),
                restriction: Some(SpendRestriction::InstantSorceryOnly),
            },
        },
        affects: SelectSpec {
            zone: crate::basics::Zone::Battlefield,
            filter: CardFilter::All(vec![
                CardFilter::HasCardType(CardType::Land),
                CardFilter::ControlledBy(PlayerRef::Controller),
            ]),
            chooser: PlayerRef::Controller,
            min: ValueExpr::Fixed(0),
            max: ValueExpr::Fixed(999),
        },
        duration: Duration::WhileSourcePresent,
    }
}

/// "{T}: Draw a card. Activate only if you have seven or more cards in your hand."
fn draw_if_seven_plus() -> Ability {
    Ability::Activated {
        cost: Cost { mana: None, components: vec![CostComponent::TapSelf] },
        effect: Effect::Draw { who: PlayerRef::Controller, count: ValueExpr::Fixed(1) },
        timing: Timing::Instant,
        restriction: Some(Restriction::OnlyIf(Condition::ValueAtLeast(
            ValueExpr::HandSize { who: PlayerRef::Controller },
            ValueExpr::Fixed(7),
        ))),
        is_mana: false,
    }
}

pub fn register(db: &mut CardDb) {
    db.insert(CardDef {
        chars: Characteristics {
            name: "Resonating Lute".to_string(),
            card_types: vec![CardType::Artifact],
            colors: vec![Color::Blue, Color::Red],
            mana_cost: Some(mana_cost(2, &[(Color::Blue, 1), (Color::Red, 1)])),
            grp_id: RESONATING_LUTE,
            ..Default::default()
        },
        abilities: vec![grant_lands_two_any_is(), draw_if_seven_plus()],
        text: "Lands you control have \"{T}: Add two mana of any one color. Spend this mana only to cast instant and sorcery spells.\"\n{T}: Draw a card. Activate only if you have seven or more cards in your hand.".to_string(),
        fully_implemented: true,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, CastVariant, DecisionRequest, DecisionResponse, PlayableAction, PlayerView};
    use crate::basics::{Phase, Zone};
    use crate::cards::{build_game, CardDef};
    use crate::ids::PlayerId;
    use crate::priority::Engine;
    use crate::state::Characteristics;

    #[test]
    fn lute_shape() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(RESONATING_LUTE).unwrap();
        assert!(def.fully_implemented);
        assert_eq!(def.chars.card_types, vec![CardType::Artifact]);
        assert!(matches!(
            &def.abilities[0],
            Ability::Static { contribution: StaticContribution::GrantTapMana { mana }, .. }
                if mana.any_color == Some(ValueExpr::Fixed(2))
                    && mana.restriction == Some(SpendRestriction::InstantSorceryOnly)
        ));
    }

    #[derive(Clone)]
    struct PassiveAgent;
    impl Agent for PassiveAgent {
        fn decide(&mut self, _v: &PlayerView, _req: &DecisionRequest) -> DecisionResponse {
            DecisionResponse::Pass
        }
    }

    /// A blank Land with NO intrinsic mana (no basic land type, no printed mana ability) — so the only
    /// mana it can make is what Resonating Lute grants. grp in the test-only 96xx range.
    const BLANK_LAND: u32 = 9640;
    /// A `{U}{U}` instant that gains 3 life — payable ONLY via a Lute-granted land (2 of one colour,
    /// I/S-only). grp in the test-only 96xx range.
    const UU_INSTANT: u32 = 9641;

    fn test_db() -> CardDb {
        let mut db = CardDb::default();
        register(&mut db);
        db.insert(CardDef {
            chars: Characteristics {
                name: "Blank Land".to_string(),
                card_types: vec![CardType::Land],
                grp_id: BLANK_LAND,
                ..Default::default()
            },
            abilities: vec![],
            text: String::new(),
            fully_implemented: true,
        });
        db.insert(CardDef {
            chars: Characteristics {
                name: "UU Lifegain".to_string(),
                card_types: vec![CardType::Instant],
                colors: vec![Color::Blue],
                mana_cost: Some(mana_cost(0, &[(Color::Blue, 2)])),
                grp_id: UU_INSTANT,
                ..Default::default()
            },
            abilities: vec![Ability::Spell {
                effect: Effect::GainLife { who: PlayerRef::Controller, amount: ValueExpr::Fixed(3) },
            }],
            text: String::new(),
            fully_implemented: true,
        });
        db
    }

    /// The REQUIRED test: a `{U}{U}` instant payable ONLY via a Lute-granted land (a land with no
    /// intrinsic mana) is OFFERED to an agent seat and paid by auto-pay — proving (a) a granted
    /// tap-mana ability is visible to affordability/enumeration, (b) the `add two of one colour` count
    /// is honoured (one land tap funds both blue pips), and (c) the I/S-only mana can pay an instant.
    #[test]
    fn granted_two_any_lets_an_agent_cast_a_uu_instant() {
        let mut state = build_game(1, &[&[], &[]]);
        state.set_card_db(std::sync::Arc::new(test_db()));
        // Resonating Lute + ONE blank land (no intrinsic mana). With the Lute out, the land gains
        // "{T}: Add two of any one colour, I/S-only" — the only mana available.
        for grp in [RESONATING_LUTE, BLANK_LAND] {
            let c = state.card_db().get(grp).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield);
        }
        let spell = {
            let c = state.card_db().get(UU_INSTANT).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Hand)
        };
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let life_before = state.player(PlayerId(0)).life;
        let mut e = Engine::new(state, vec![Box::new(PassiveAgent), Box::new(PassiveAgent)]);

        // (a) The spell is OFFERED at priority — the granted mana counts toward affordability.
        let offered = e
            .legal_actions(PlayerId(0))
            .iter()
            .any(|a| matches!(a, PlayableAction::Cast { spell: s, .. } if *s == spell));
        assert!(offered, "the {{U}}{{U}} instant is offered — the Lute-granted land funds it");

        // (b)+(c) Auto-pay funds it from the single Lute-granted land (2 blue from one tap, I/S-only).
        e.cast_spell(PlayerId(0), spell, CastVariant::Normal);
        e.run_agenda();
        while !e.state.stack.items.is_empty() {
            e.resolve_top();
            e.run_agenda();
        }
        assert_eq!(
            e.state.player(PlayerId(0)).life,
            life_before + 3,
            "the instant resolved — it was paid by the Lute-granted land"
        );
        // The blank land tapped for the two blue mana; both were spent (no phantom float).
        let land = e.state.player(PlayerId(0)).battlefield.iter().copied().find(|&id| {
            e.state.object(id).chars.grp_id == BLANK_LAND
        });
        assert!(e.state.object(land.unwrap()).status.tapped, "the granted land tapped for the {{U}}{{U}}");
    }

    /// No-regression: the granted mana is I/S-**only** (CR 106.6). A creature spell can't be paid by it,
    /// so a `{U}{U}` creature with only the Lute-granted land available is NOT offered.
    #[test]
    fn granted_mana_cannot_pay_a_creature_spell() {
        const UU_CREATURE: u32 = 9642;
        let mut db = test_db();
        db.insert(CardDef {
            chars: Characteristics {
                name: "UU Bear".to_string(),
                card_types: vec![CardType::Creature],
                colors: vec![Color::Blue],
                mana_cost: Some(mana_cost(0, &[(Color::Blue, 2)])),
                power: Some(2),
                toughness: Some(2),
                grp_id: UU_CREATURE,
                ..Default::default()
            },
            abilities: vec![],
            text: String::new(),
            fully_implemented: true,
        });
        let mut state = build_game(1, &[&[], &[]]);
        state.set_card_db(std::sync::Arc::new(db));
        for grp in [RESONATING_LUTE, BLANK_LAND] {
            let c = state.card_db().get(grp).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield);
        }
        let creature = {
            let c = state.card_db().get(UU_CREATURE).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Hand)
        };
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let e = Engine::new(state, vec![Box::new(PassiveAgent), Box::new(PassiveAgent)]);
        let offered = e
            .legal_actions(PlayerId(0))
            .iter()
            .any(|a| matches!(a, PlayableAction::Cast { spell: s, .. } if *s == creature));
        assert!(!offered, "I/S-only Lute mana can't pay a creature spell (CR 106.6)");
    }
}
