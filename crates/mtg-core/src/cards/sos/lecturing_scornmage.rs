//! Lecturing Scornmage — `{B}` Creature — Human Warlock 1/1 (first printed SOS).
//!
//! Oracle: "Repartee — Whenever you cast an instant or sorcery spell that targets a creature, put a
//! +1/+1 counter on this creature."
//!
//! **Fully implemented** — a Repartee cast-trigger (`SpellCastTargetingCreature(instant|sorcery)`):
//! it fires only when the cast spell targets a creature, putting a +1/+1 counter on itself.

use crate::basics::{Color, CounterKind};
use crate::cards::helpers::instant_or_sorcery;
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern};
use crate::effects::value::ValueExpr;
use crate::effects::{Effect, EffectTarget};
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const LECTURING_SCORNMAGE: u32 = 259;

pub fn register(db: &mut CardDb) {
    db.insert(
        creature(
            LECTURING_SCORNMAGE,
            "Lecturing Scornmage",
            &[CreatureType::Human, CreatureType::Warlock],
            Color::Black,
            mana_cost(0, &[(Color::Black, 1)]),
            1,
            1,
            vec![Ability::Triggered {
                event: EventPattern::SpellCastTargetingCreature(instant_or_sorcery()),
                condition: None,
                intervening_if: false,
                effect: Effect::PutCounters {
                    what: EffectTarget::SourceSelf,
                    kind: CounterKind::PlusOnePlusOne,
                    n: ValueExpr::Fixed(1),
                },
            }],
        )
        .with_text("Repartee — Whenever you cast an instant or sorcery spell that targets a creature, put a +1/+1 counter on this creature."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn lecturing_scornmage_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(LECTURING_SCORNMAGE).unwrap();
        assert!(def.fully_implemented);
        expect![[r#"
            [
                Triggered {
                    event: SpellCastTargetingCreature(
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

    /// End-to-end: Repartee fires only when the cast spell targets a creature. Casting an instant
    /// targeting a creature grows the Scornmage (1/1 → 2/2); casting one with no creature target does not.
    #[test]
    fn lecturing_scornmage_repartee_gated_on_creature_target() {
        use crate::agent::{Agent, DecisionRequest, DecisionResponse, GameEvent, PlayerView};
        use crate::basics::{Target, Zone};
        use crate::cards::{build_game, grp};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        use crate::stack::{StackObject, StackObjectKind};

        #[derive(Clone)]
        struct PassiveAgent;
        impl Agent for PassiveAgent {
            fn decide(&mut self, _v: &PlayerView, _req: &DecisionRequest) -> DecisionResponse {
                DecisionResponse::Pass
            }
        }

        // Cast an instant that targets `targets`, fire SpellCast, resolve the (maybe) Repartee trigger.
        let power_after = |targets: Vec<Target>| {
            let mut state = build_game(1, &[&[], &[]]);
            let scorn = {
                let c = state.card_db().get(LECTURING_SCORNMAGE).unwrap().chars.clone();
                state.add_card(PlayerId(0), c, Zone::Battlefield)
            };
            let bolt = {
                let c = state.card_db().get(grp::LIGHTNING_BOLT).unwrap().chars.clone();
                state.add_card(PlayerId(0), c, Zone::Stack)
            };
            let sid = StackId(500);
            state.stack.push(StackObject {
                id: sid,
                controller: PlayerId(0),
                source: None,
                kind: StackObjectKind::Spell(bolt),
                targets,
                x: None,
                modes: Vec::new(),
            });
            let mut e = Engine::new(state, vec![Box::new(PassiveAgent), Box::new(PassiveAgent)]);
            e.broadcast(GameEvent::SpellCast { spell: sid, controller: PlayerId(0) });
            e.run_agenda();
            e.resolve_top();
            e.state.computed(scorn).power.unwrap()
        };
        // Targeting the opponent's face → no creature target → Repartee doesn't fire (stays 1/1).
        assert_eq!(power_after(vec![Target::Player(PlayerId(1))]), 1, "no creature target → no Repartee");
        // Targeting a creature → Repartee fires (→ 2/2). Build a fresh game with a creature to target.
        let grew = {
            let mut state = build_game(1, &[&[], &[]]);
            let scorn = {
                let c = state.card_db().get(LECTURING_SCORNMAGE).unwrap().chars.clone();
                state.add_card(PlayerId(0), c, Zone::Battlefield)
            };
            let bears = {
                let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
                state.add_card(PlayerId(1), c, Zone::Battlefield)
            };
            let bolt = {
                let c = state.card_db().get(grp::LIGHTNING_BOLT).unwrap().chars.clone();
                state.add_card(PlayerId(0), c, Zone::Stack)
            };
            let sid = StackId(500);
            state.stack.push(StackObject {
                id: sid,
                controller: PlayerId(0),
                source: None,
                kind: StackObjectKind::Spell(bolt),
                targets: vec![Target::Object(bears)],
                x: None,
                modes: Vec::new(),
            });
            let mut e = Engine::new(state, vec![Box::new(PassiveAgent), Box::new(PassiveAgent)]);
            let _ = bears;
            e.broadcast(GameEvent::SpellCast { spell: sid, controller: PlayerId(0) });
            e.run_agenda();
            e.resolve_top();
            e.state.computed(scorn).power.unwrap()
        };
        assert_eq!(grew, 2, "creature target → Repartee → +1/+1 → 2/2");
    }
}
