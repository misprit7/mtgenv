//! Cheerful Osteomancer // Raise Dead — `{3}{B}` Creature — Orc Warlock 4/2 // `{B}` Sorcery
//! (first printed SOS). A **Prepare** DFC (enters prepared).
//!
//! Front: "This creature enters prepared."  Back (Raise Dead): "Return target creature card from your
//! graveyard to your hand."
//!
//! **Fully implemented** — enters-prepared via [`helpers::enters_prepared`]; the back returns a target
//! creature card from your graveyard to hand (`MoveZone`, mirroring Pull from the Grave).

use crate::basics::{CardType, Color, Zone, ZoneDest, ZonePos};
use crate::cards::{creature, helpers, mana_cost, spell, CardDb};
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::PlayerRef;
use crate::effects::{Effect, EffectTarget};
use crate::subtypes::CreatureType;

pub const CHEERFUL_OSTEOMANCER: u32 = 384;
pub const RAISE_DEAD: u32 = 9711;

pub fn register(db: &mut CardDb) {
    let raise_dead = Effect::MoveZone {
        what: EffectTarget::Target(TargetSpec {
            kind: TargetKind::CardInZone {
                zone: Zone::Graveyard,
                filter: CardFilter::All(vec![
                    CardFilter::HasCardType(CardType::Creature),
                    CardFilter::ControlledBy(PlayerRef::Controller),
                ]),
            },
            min: 1,
            max: 1,
            distinct: true,
        }),
        to: ZoneDest { zone: Zone::Hand, pos: ZonePos::Any },
        tapped: false,
    };
    db.insert(
        spell(RAISE_DEAD, "Raise Dead", CardType::Sorcery, Color::Black, mana_cost(0, &[(Color::Black, 1)]), raise_dead)
            .with_text("Return target creature card from your graveyard to your hand."),
    );
    let mut front = creature(
        CHEERFUL_OSTEOMANCER,
        "Cheerful Osteomancer",
        &[CreatureType::Orc, CreatureType::Warlock],
        Color::Black,
        mana_cost(3, &[(Color::Black, 1)]),
        4,
        2,
        helpers::enters_prepared(RAISE_DEAD),
    );
    front.text = "This creature enters prepared. (While it's prepared, you may cast a copy of its spell. Doing so unprepares it.)\n// Raise Dead {B} Sorcery — Return target creature card from your graveyard to your hand.".to_string();
    db.insert(front);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::RandomAgent;
    use crate::basics::Target;
    use crate::cards::{build_game, grp};
    use crate::effects::action::{ResolutionCtx, WbReason};
    use crate::ids::{PlayerId, StackId};
    use crate::priority::Engine;

    #[test]
    fn raise_dead_returns_a_creature_from_graveyard_to_hand() {
        let mut state = build_game(1, &[&[], &[]]);
        let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
        let bears = state.add_card(PlayerId(0), c, Zone::Graveyard);
        let effect = state.card_db().get(RAISE_DEAD).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
        e.resolve_effect(
            &effect,
            &ResolutionCtx {
                controller: Some(PlayerId(0)),
                chosen_targets: vec![Target::Object(bears)],
                ..Default::default()
            },
            WbReason::Resolve(StackId(0)),
        );
        assert!(e.state.player(PlayerId(0)).hand.contains(&bears), "the creature returned to hand");
        assert!(!e.state.player(PlayerId(0)).graveyard.contains(&bears), "it left the graveyard");
    }
}
