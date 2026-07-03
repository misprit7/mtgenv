//! Inkshape Demonstrator — `{3}{W}` Creature — Elephant Cleric 3/4 (first printed SOS).
//!
//! Oracle: "Ward {2} / Repartee — Whenever you cast an instant or sorcery spell that targets a
//! creature, this creature gets +1/+0 and gains lifelink until end of turn."
//!
//! **Fully implemented** — the fifth S17 Ward card. Ward {2} (mana) via `ward_mana(2)`, plus a
//! Repartee cast-trigger (`SpellCastTargetingCreature(instant|sorcery)`) that pumps itself +1/+0 and
//! grants itself **Lifelink** until end of turn (`GrantKeyword`). Lifelink is live in combat/damage
//! (`apply_damage` already gains the source's controller life equal to the damage dealt, CR 702.15),
//! and it reads the COMPUTED keyword set, so the granted lifelink counts.

use crate::basics::Color;
use crate::cards::helpers::{instant_or_sorcery, ward_mana};
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern, Keyword};
use crate::effects::condition::Duration;
use crate::effects::value::ValueExpr;
use crate::effects::{Effect, EffectTarget};
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const INKSHAPE_DEMONSTRATOR: u32 = 340;

/// "Repartee — Whenever you cast an instant or sorcery spell that targets a creature, this creature
/// gets +1/+0 and gains lifelink until end of turn."
fn repartee_pump_lifelink() -> Ability {
    Ability::Triggered {
        event: EventPattern::SpellCastTargetingCreature(instant_or_sorcery()),
        condition: None,
        intervening_if: false,
        effect: Effect::Sequence(vec![
            Effect::PumpPT {
                what: EffectTarget::SourceSelf,
                power: ValueExpr::Fixed(1),
                toughness: ValueExpr::Fixed(0),
                duration: Duration::UntilEndOfTurn,
            },
            Effect::GrantKeyword {
                what: EffectTarget::SourceSelf,
                keyword: Keyword::Lifelink,
                duration: Duration::UntilEndOfTurn,
            },
        ]),
    }
}

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        INKSHAPE_DEMONSTRATOR,
        "Inkshape Demonstrator",
        &[CreatureType::Elephant, CreatureType::Cleric],
        Color::White,
        mana_cost(3, &[(Color::White, 1)]),
        3,
        4,
        vec![ward_mana(2), repartee_pump_lifelink()],
    );
    def.text = "Ward {2} (Whenever this creature becomes the target of a spell or ability an opponent controls, counter it unless that player pays {2}.)\nRepartee — Whenever you cast an instant or sorcery spell that targets a creature, this creature gets +1/+0 and gains lifelink until end of turn.".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, DecisionRequest, DecisionResponse, GameEvent, PlayerView};
    use crate::basics::{DamageKind, Target, Zone};
    use crate::cards::{grp, starter_db};
    use crate::ids::{PlayerId, StackId};
    use crate::priority::Engine;
    use crate::stack::{StackObject, StackObjectKind};
    use crate::state::GameState;
    use std::sync::Arc;

    #[derive(Clone)]
    struct Passer;
    impl Agent for Passer {
        fn decide(&mut self, _v: &PlayerView, _r: &DecisionRequest) -> DecisionResponse {
            DecisionResponse::Pass
        }
    }

    #[test]
    fn inkshape_demonstrator_shape() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(INKSHAPE_DEMONSTRATOR).unwrap();
        assert_eq!((def.chars.power, def.chars.toughness), (Some(3), Some(4)));
        assert!(def.fully_implemented);
        assert!(matches!(&def.abilities[0], Ability::Triggered { .. }), "ability 0 is Ward");
    }

    /// Repartee through the real trigger: casting an instant that targets a creature pumps Inkshape
    /// +1/+0 (3/4 → 4/4) and grants it lifelink; the granted lifelink is then live — dealing 4 combat
    /// damage gains its controller 4 life.
    #[test]
    fn repartee_grants_pump_and_working_lifelink() {
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(starter_db()));
        let ink = {
            let c = state.card_db().get(INKSHAPE_DEMONSTRATOR).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        // A creature for the instant to target (so Repartee fires).
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
        let mut e = Engine::new(state, vec![Box::new(Passer), Box::new(Passer)]);
        e.broadcast(GameEvent::SpellCast { spell: sid, controller: PlayerId(0) });
        e.run_agenda(); // put the Repartee trigger on the stack
        while !e.state.stack.items.is_empty() {
            e.resolve_top();
            e.run_agenda();
        }
        assert_eq!(e.state.computed(ink).power, Some(4), "Repartee: +1/+0 → 4/4");
        assert!(
            e.state.computed(ink).has_keyword(Keyword::Lifelink),
            "Repartee granted lifelink until end of turn"
        );
        // The granted lifelink is live: 4 damage from Inkshape gains its controller 4 life.
        let life_before = e.state.player(PlayerId(0)).life;
        e.apply_damage(Target::Object(bears), 4, ink, DamageKind::Combat);
        assert_eq!(
            e.state.player(PlayerId(0)).life,
            life_before + 4,
            "lifelink gained 4 life for the 4 damage dealt"
        );
    }
}
