//! Petrified Hamlet — Land.
//!
//! Oracle:
//!   When this land enters, choose a land card name.
//!   Activated abilities of sources with the chosen name can't be activated unless they're mana
//!     abilities.
//!   Lands with the chosen name have "{T}: Add {C}."
//!   {T}: Add {C}.
//!
//! **Fully implemented.** The ETB choice is a [`crate::effects::Effect::ChooseLandName`] over the
//! engine-enumerated distinct land-card names (a `DecisionRequest::ChooseOption` /
//! `OptionReason::NameCard`), stored in the Hamlet's [`crate::state::Object::chosen_name`]. Two
//! name-keyed statics read that choice:
//! - the **ability-legality gate** — the priority-action builder ([`crate::priority`]) refuses to
//!   offer a *non-mana* activated ability of any permanent whose name is a noted `chosen_name` (mana
//!   abilities are exempt, matching the oracle);
//! - the **`{T}: Add {C}` grant** — an [`Ability::Static`] carrying [`StaticContribution::GrantTapMana`]
//!   over lands filtered by [`CardFilter::NamedAsChooser`] (name == the Hamlet's chosen name), read by
//!   the mana-payment enumeration (the same B3a path Resonating Lute uses).
//! Plus its own intrinsic `{T}: Add {C}`.

use crate::basics::{CardType, Color, Zone};
use crate::cards::{mana_ability, CardDb, CardDef};
use crate::effects::ability::{Ability, EventPattern, StaticContribution};
use crate::effects::condition::Duration;
use crate::effects::target::{CardFilter, ManaSpec, SelectSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};
use crate::state::Characteristics;

/// grp id (per-set ids live near their cards).
pub const PETRIFIED_HAMLET: u32 = 504;

/// "When this land enters, choose a land card name."
fn etb_choose_land_name() -> Ability {
    Ability::Triggered {
        event: EventPattern::SelfEnters,
        condition: None,
        intervening_if: false,
        effect: Effect::ChooseLandName { what: EffectTarget::SourceSelf },
    }
}

/// "Lands with the chosen name have '{T}: Add {C}.'" — a grant-tap-mana static keyed to the Hamlet's
/// chosen name (CR 613.1f). `NamedAsChooser` matches any land whose name == this Hamlet's `chosen_name`.
fn grant_named_lands_add_c() -> Ability {
    Ability::Static {
        contribution: StaticContribution::GrantTapMana {
            mana: ManaSpec {
                produces: vec![(Color::Colorless, ValueExpr::Fixed(1))],
                any_color: None,
                restriction: None,
            },
        },
        affects: SelectSpec {
            zone: Zone::Battlefield,
            filter: CardFilter::All(vec![
                CardFilter::HasCardType(CardType::Land),
                CardFilter::NamedAsChooser,
            ]),
            chooser: PlayerRef::Controller,
            min: ValueExpr::Fixed(0),
            max: ValueExpr::Fixed(999),
        },
        duration: Duration::WhileSourcePresent,
    }
}

pub fn register(db: &mut CardDb) {
    db.insert(CardDef {
        chars: Characteristics {
            name: "Petrified Hamlet".to_string(),
            card_types: vec![CardType::Land],
            grp_id: PETRIFIED_HAMLET,
            ..Default::default()
        },
        // The legality gate is powered by the `chosen_name` marker + the priority-builder scan (no
        // explicit static ability is needed for it); the grant + own mana ability are printed here.
        abilities: vec![
            etb_choose_land_name(),
            grant_named_lands_add_c(),
            mana_ability(Color::Colorless), // "{T}: Add {C}."
        ],
        text: "When this land enters, choose a land card name.\nActivated abilities of sources with the chosen name can't be activated unless they're mana abilities.\nLands with the chosen name have \"{T}: Add {C}.\"\n{T}: Add {C}.".to_string(),
        fully_implemented: true,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayableAction, PlayerView};
    use crate::basics::Phase;
    use crate::cards::build_game;
    use crate::effects::ability::{Cost, CostComponent, Timing};
    use crate::ids::{ObjId, PlayerId};
    use crate::priority::Engine;

    /// A land named "Signal Beacon" with NO intrinsic mana and a NON-mana `{T}: gain 1 life` ability —
    /// so the gate (its ability turned off) and the grant (it gains `{T}: Add {C}`) are both observable.
    const SIGNAL_BEACON: u32 = 9650;

    fn test_db() -> CardDb {
        let mut db = CardDb::default();
        register(&mut db);
        db.insert(CardDef {
            chars: Characteristics {
                name: "Signal Beacon".to_string(),
                card_types: vec![CardType::Land],
                grp_id: SIGNAL_BEACON,
                ..Default::default()
            },
            abilities: vec![Ability::Activated {
                cost: Cost { mana: None, components: vec![CostComponent::TapSelf] },
                effect: Effect::GainLife { who: PlayerRef::Controller, amount: ValueExpr::Fixed(1) },
                timing: Timing::Instant,
                restriction: None,
                is_mana: false,
            }],
            text: String::new(),
            fully_implemented: true,
        });
        db
    }

    #[test]
    fn hamlet_shape() {
        let db = test_db();
        let def = db.get(PETRIFIED_HAMLET).unwrap();
        assert!(def.fully_implemented);
        assert_eq!(def.chars.card_types, vec![CardType::Land]);
        assert!(matches!(&def.abilities[0], Ability::Triggered { effect: Effect::ChooseLandName { .. }, .. }));
    }

    /// The single {C} pip cost `{C}` (a colorless mana requirement).
    fn c_cost(n: u32) -> crate::basics::ManaCost {
        let mut colored = std::collections::BTreeMap::new();
        colored.insert(Color::Colorless, n);
        crate::basics::ManaCost { colored, ..Default::default() }
    }

    /// Names "Signal Beacon" at the ETB `ChooseOption`; passes otherwise.
    struct NamingAgent(&'static str);
    impl Agent for NamingAgent {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::ChooseOption { options, .. } => {
                    let idx = options.iter().position(|o| o.label == self.0).unwrap_or(0);
                    DecisionResponse::Indices(vec![idx as u32])
                }
                _ => DecisionResponse::Pass,
            }
        }
    }

    fn beacon_ability_offered(e: &Engine, beacon: ObjId) -> bool {
        e.legal_actions(PlayerId(0))
            .iter()
            .any(|a| matches!(a, PlayableAction::Activate { source, .. } if *source == beacon))
    }

    /// Real ETB path: playing Petrified Hamlet asks the controller to name a land; naming "Signal
    /// Beacon" (a) notes the name, (b) turns OFF the Beacon's non-mana `{T}` ability (the legality
    /// gate), and (c) grants the Beacon `{T}: Add {C}` (so `{C}{C}` becomes payable — Hamlet + Beacon).
    #[test]
    fn etb_names_a_land_then_gates_and_grants() {
        let mut state = build_game(1, &[&[], &[]]);
        state.set_card_db(std::sync::Arc::new(test_db()));
        let beacon = {
            let c = state.card_db().get(SIGNAL_BEACON).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        let hamlet = {
            let c = state.card_db().get(PETRIFIED_HAMLET).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Hand)
        };
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let mut e = Engine::new(state, vec![Box::new(NamingAgent("Signal Beacon")), Box::new(NamingAgent("Signal Beacon"))]);

        // Baseline (before the Hamlet is out): the Beacon's non-mana {T} ability IS offered, and
        // {C}{C} is NOT payable (the Beacon has no intrinsic mana, nothing else makes {C}).
        assert!(beacon_ability_offered(&e, beacon), "baseline: Beacon's {{T}} ability is offered");
        assert!(!crate::mana::can_pay(&e.state, PlayerId(0), &c_cost(2)), "baseline: {{C}}{{C}} not payable");

        // Play the Hamlet as a land → ETB trigger → the agent names "Signal Beacon".
        e.play_land(PlayerId(0), hamlet);
        e.run_agenda();
        while !e.state.stack.items.is_empty() {
            e.resolve_top();
            e.run_agenda();
        }
        assert_eq!(
            e.state.object(hamlet).chosen_name.as_deref(),
            Some("Signal Beacon"),
            "the ETB choice noted the land name"
        );

        // (b) The gate: the Beacon's NON-mana ability is no longer offered.
        assert!(!beacon_ability_offered(&e, beacon), "the Beacon's non-mana ability is gated off");
        // (c) The grant: the Beacon now taps for {C}, so {C}{C} is payable from Hamlet + Beacon.
        assert!(
            crate::mana::can_pay(&e.state, PlayerId(0), &c_cost(2)),
            "the grant gave the Beacon {{T}}: Add {{C}} — {{C}}{{C}} now payable"
        );
    }

    /// The Hamlet's own `{T}: Add {C}` is a mana ability (exempt from its own name-gate even if it
    /// names itself) and funds a `{C}` cost.
    #[test]
    fn hamlet_taps_for_colorless() {
        let mut state = build_game(1, &[&[], &[]]);
        state.set_card_db(std::sync::Arc::new(test_db()));
        let hamlet = {
            let c = state.card_db().get(PETRIFIED_HAMLET).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        // Its own {T}: Add {C} funds a {C} cost.
        assert!(crate::mana::can_pay(&state, PlayerId(0), &c_cost(1)), "Hamlet taps for {{C}}");
        assert!(state.object(hamlet).chosen_name.is_none(), "no name chosen without the ETB");
    }
}
