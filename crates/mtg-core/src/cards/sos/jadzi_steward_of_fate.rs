//! Jadzi, Steward of Fate // Oracle's Gift — `{2}{U}` Legendary Creature — Human Wizard 2/4 //
//! `{X}{X}{U}` Sorcery (first printed SOS). A **Prepare** DFC whose back is a dynamic-X Fractal build.
//!
//! Front oracle: "Jadzi enters prepared. (While it's prepared, you may cast a copy of its spell. Doing
//! so unprepares it.)  When Jadzi enters, draw two cards, then discard two cards."
//! Back oracle (Oracle's Gift): "Create X 0/0 green and blue Fractal creature tokens, then put X +1/+1
//! counters on each Fractal you control."
//!
//! **Fully implemented — no new cap.** The front is enters-prepared plus a second `SelfEnters` trigger
//! (draw two, then discard two). The back is `{X}{X}` (two `{X}` pips → `ManaCost.x = 2`, so paying
//! charges `2X`; the chosen X threads onto the stack object as `cast_x`, read by `ValueExpr::X`): a
//! `Sequence` that first creates X Fractal tokens ([`Effect::CreateToken`] with `count: X`), then puts X
//! +1/+1 counters on **each Fractal you control** via `ForEach{ Fractals you control → PutCounters{Each} }`
//! (the shipped Blech pattern — a resolution-time select of *all* matching permanents, so the freshly
//! made tokens and any pre-existing Fractals alike get the counters).

use crate::basics::{CardType, Color, CounterKind, Zone};
use crate::cards::helpers::fractal_token;
use crate::cards::{creature, helpers, spell, CardDb};
use crate::effects::ability::{Ability, EventPattern};
use crate::effects::target::{CardFilter, SelectSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};
use crate::subtypes::{CreatureType, Supertype};

/// grp id (per-set ids live near their cards).
pub const JADZI: u32 = 406;
/// The copy-only back-face spell (reserved 9700+ Prepare block).
pub const ORACLES_GIFT: u32 = 9731;

/// "Fractal creatures you control" — the resolution-time select the counter pass iterates.
fn fractals_you_control() -> SelectSpec {
    SelectSpec {
        zone: Zone::Battlefield,
        filter: CardFilter::All(vec![
            CardFilter::ControlledBy(PlayerRef::Controller),
            CardFilter::HasSubtype(CreatureType::Fractal.into()),
        ]),
        chooser: PlayerRef::Controller,
        min: ValueExpr::Fixed(0),
        max: ValueExpr::Fixed(999),
    }
}

pub fn register(db: &mut CardDb) {
    // Back face — "Oracle's Gift" ({X}{X}{U} Sorcery): create X Fractals, then X counters on each.
    let cost = crate::basics::ManaCost {
        generic: 0,
        colored: std::collections::BTreeMap::from([(Color::Blue, 1)]),
        x: 2,
        ..Default::default()
    };
    db.insert(
        spell(
            ORACLES_GIFT,
            "Oracle's Gift",
            CardType::Sorcery,
            Color::Blue,
            cost,
            Effect::Sequence(vec![
                Effect::CreateToken {
                    spec: fractal_token(0),
                    count: ValueExpr::X,
                    controller: PlayerRef::Controller,
                    dynamic_counters: vec![],
                },
                Effect::ForEach {
                    selector: fractals_you_control(),
                    body: Box::new(Effect::PutCounters {
                        what: EffectTarget::Each,
                        kind: CounterKind::PlusOnePlusOne,
                        n: ValueExpr::X,
                    }),
                },
            ]),
        )
        .with_text("Create X 0/0 green and blue Fractal creature tokens, then put X +1/+1 counters on each Fractal you control."),
    );

    // Front face — the 2/4 legend; enters prepared + a second `SelfEnters` trigger (draw 2, discard 2).
    let mut abilities = helpers::enters_prepared(ORACLES_GIFT);
    abilities.push(Ability::Triggered {
        event: EventPattern::SelfEnters,
        condition: None,
        intervening_if: false,
        effect: Effect::Sequence(vec![
            Effect::Draw { who: PlayerRef::Controller, count: ValueExpr::Fixed(2) },
            Effect::Discard { who: PlayerRef::Controller, count: ValueExpr::Fixed(2) },
        ]),
    });
    let mut front = creature(
        JADZI,
        "Jadzi, Steward of Fate",
        &[CreatureType::Human, CreatureType::Wizard],
        Color::Blue,
        crate::cards::mana_cost(2, &[(Color::Blue, 1)]),
        2,
        4,
        abilities,
    );
    front.chars.supertypes = vec![Supertype::Legendary];
    front.text = "Jadzi enters prepared. (While it's prepared, you may cast a copy of its spell. Doing so unprepares it.)\nWhen Jadzi enters, draw two cards, then discard two cards.\n// Oracle's Gift {X}{X}{U} Sorcery — Create X 0/0 green and blue Fractal creature tokens, then put X +1/+1 counters on each Fractal you control.".to_string();
    db.insert(front);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView};
    use crate::basics::{Phase, Zone};
    use crate::cards::grp;
    use crate::ids::{ObjId, PlayerId};
    use crate::priority::Engine;
    use crate::state::GameState;

    fn db_with_card() -> CardDb {
        let mut db = crate::cards::starter_db();
        register(&mut db);
        db
    }

    fn add(state: &mut GameState, who: PlayerId, grp_id: u32, zone: Zone) -> ObjId {
        let c = state.card_db().get(grp_id).unwrap().chars.clone();
        state.add_card(who, c, zone)
    }

    #[test]
    fn jadzi_ir() {
        let db = db_with_card();
        let front = db.get(JADZI).unwrap();
        assert_eq!(front.chars.supertypes, vec![Supertype::Legendary]);
        assert!(matches!(front.abilities[0], Ability::Prepare { spell: ORACLES_GIFT }));
        assert!(matches!(
            front.abilities[1],
            Ability::Triggered { event: EventPattern::SelfEnters, .. }
        ));
        assert!(matches!(
            front.abilities[2],
            Ability::Triggered { event: EventPattern::SelfEnters, .. }
        ));
        let back = db.get(ORACLES_GIFT).unwrap();
        assert_eq!(back.chars.card_types, vec![CardType::Sorcery]);
        assert_eq!(back.chars.mana_cost.as_ref().unwrap().x, 2);
    }

    /// Choose X for the {X} prompt; select the first candidate otherwise.
    struct ChooseX(i64);
    impl Agent for ChooseX {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::ChooseNumber { .. } => DecisionResponse::Number(self.0),
                DecisionRequest::SelectCards { from, .. } if !from.is_empty() => {
                    DecisionResponse::Indices(vec![0])
                }
                _ => DecisionResponse::Pass,
            }
        }
    }

    /// Headline: a prepared Jadzi casts Oracle's Gift with X=2 → two 0/0 Fractals are created, then a
    /// pre-existing Fractal AND the two new ones each get two +1/+1 counters (all become 2/2s).
    #[test]
    fn oracles_gift_makes_x_fractals_and_pumps_each_fractal() {
        let mut state = GameState::new(2, 1);
        state.set_card_db(std::sync::Arc::new(db_with_card()));
        let jadzi = add(&mut state, PlayerId(0), JADZI, Zone::Battlefield);
        state.objects.get_mut(&jadzi).unwrap().prepared = true;
        // A pre-existing Fractal token on the battlefield (0/0) that should also receive counters.
        let pre = add(&mut state, PlayerId(0), grp::FOREST, Zone::Battlefield); // placeholder id; overwrite chars
        {
            let spec = fractal_token(0);
            let o = state.objects.get_mut(&pre).unwrap();
            o.chars.name = spec.name.clone();
            o.chars.card_types = spec.card_types.clone();
            o.chars.subtypes = spec.subtypes.clone();
            o.chars.power = Some(spec.power);
            o.chars.toughness = Some(spec.toughness);
            o.chars.colors = spec.colors.clone();
            o.chars.mana_cost = None;
        }
        // {X}{X}{U} with X=2 → need 2*2 + 1 = 5 mana (4 Islands + 1 more Island).
        for _ in 0..5 {
            add(&mut state, PlayerId(0), grp::ISLAND, Zone::Battlefield);
        }
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let mut e = Engine::new(state, vec![Box::new(ChooseX(2)), Box::new(ChooseX(2))]);

        e.cast_prepared(PlayerId(0), jadzi);
        e.resolve_top(); // Oracle's Gift resolves: make 2 Fractals, then +2 counters on each Fractal
        e.run_agenda();

        let fractals: Vec<ObjId> = e
            .state
            .player(PlayerId(0))
            .battlefield
            .iter()
            .copied()
            .filter(|&o| e.state.object(o).chars.name == "Fractal")
            .collect();
        assert_eq!(fractals.len(), 3, "two new Fractals + the pre-existing one");
        for f in &fractals {
            assert_eq!(
                e.state.object(*f).counters.get(&CounterKind::PlusOnePlusOne),
                2,
                "each Fractal you control got X=2 +1/+1 counters"
            );
        }
        assert!(!e.state.object(jadzi).prepared, "casting the copy unprepared Jadzi");
    }
}
