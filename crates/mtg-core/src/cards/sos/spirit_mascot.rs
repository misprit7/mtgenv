//! Spirit Mascot — `{R}{W}` Creature — Spirit Ox 2/2 (first printed SOS).
//!
//! Oracle: "Whenever one or more cards leave your graveyard, put a +1/+1 counter on this creature."
//!
//! **Fully implemented** — a `CardsLeaveYourGraveyard` trigger growing itself. Multicolored (R/W).

use crate::basics::{Color, CounterKind};
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern};
use crate::effects::value::ValueExpr;
use crate::effects::{Effect, EffectTarget};
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const SPIRIT_MASCOT: u32 = 302;

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        SPIRIT_MASCOT,
        "Spirit Mascot",
        &[CreatureType::Spirit, CreatureType::Ox],
        Color::Red,
        mana_cost(0, &[(Color::Red, 1), (Color::White, 1)]),
        2,
        2,
        vec![Ability::Triggered {
            event: EventPattern::CardsLeaveYourGraveyard,
            condition: None,
            intervening_if: false,
            effect: Effect::PutCounters {
                what: EffectTarget::SourceSelf,
                kind: CounterKind::PlusOnePlusOne,
                n: ValueExpr::Fixed(1),
            },
        }],
    );
    def.chars.colors = vec![Color::Red, Color::White];
    def.text = "Whenever one or more cards leave your graveyard, put a +1/+1 counter on this creature.".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn spirit_mascot_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(SPIRIT_MASCOT).unwrap();
        assert_eq!(def.chars.colors, vec![Color::Red, Color::White]);
        assert!(def.fully_implemented);
        expect![[r#"
            [
                Triggered {
                    event: CardsLeaveYourGraveyard,
                    condition: None,
                    intervening_if: false,
                    effect: PutCounters {
                        what: SourceSelf,
                        kind: PlusOnePlusOne,
                        n: Fixed(
                            1,
                        ),
                    },
                },
            ]"#]].assert_eq(&format!("{:#?}", def.abilities));
    }

    /// S9 cap end-to-end: an effect that returns a card from your graveyard makes the graveyard shrink,
    /// firing `LeftGraveyard` → Spirit Mascot's trigger → +1/+1 (2/2 → 3/3).
    #[test]
    fn spirit_mascot_grows_when_a_card_leaves_graveyard() {
        use crate::agent::RandomAgent;
        use crate::basics::{Target, Zone, ZoneDest, ZonePos};
        use crate::cards::{build_game, grp};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        let mut state = build_game(1, &[&[], &[]]);
        let mascot = state.add_card(PlayerId(0), state.card_db().get(SPIRIT_MASCOT).unwrap().chars.clone(), Zone::Battlefield);
        let gycard = state.add_card(PlayerId(0), state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Graveyard);
        // Return the graveyard card to hand → the graveyard shrinks.
        let effect = Effect::MoveZone {
            what: EffectTarget::Target(TargetSpec { kind: TargetKind::CardInZone { zone: Zone::Graveyard, filter: CardFilter::Any }, min: 1, max: 1, distinct: true }),
            to: ZoneDest { zone: Zone::Hand, pos: ZonePos::Any },
        };
        let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
        e.resolve_effect(&effect, &ResolutionCtx { controller: Some(PlayerId(0)), chosen_targets: vec![Target::Object(gycard)], ..Default::default() }, WbReason::Resolve(StackId(0)));
        e.run_agenda();
        e.resolve_top();
        assert_eq!(e.state.computed(mascot).power, Some(3), "Spirit Mascot grew when a card left the graveyard");
    }
}
