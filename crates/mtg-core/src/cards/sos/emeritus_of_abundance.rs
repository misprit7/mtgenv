//! Emeritus of Abundance // Regrowth — `{2}{G}` Creature — Elf Druid 3/4 // `{1}{G}` Sorcery
//! (first printed SOS). A **Prepare** DFC — enters prepared, and re-prepares on attack when flooded.
//!
//! Front: "This creature enters prepared. Whenever this creature attacks, if you control eight or more
//! lands, this creature becomes prepared."
//! Back (Regrowth): "Return target card from your graveyard to your hand."
//!
//! **Fully implemented** — enters-prepared plus a `SelfAttacks` trigger with an intervening-if
//! `CountAtLeast(lands you control ≥ 8)`; both effects are [`Effect::BecomePrepared`]. The back returns
//! any target card from your graveyard to hand.

use crate::basics::{CardType, Color, Zone, ZoneDest, ZonePos};
use crate::cards::{creature, helpers, mana_cost, spell, CardDb};
use crate::effects::ability::{Ability, EventPattern};
use crate::effects::condition::Condition;
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};
use crate::subtypes::CreatureType;

pub const EMERITUS_OF_ABUNDANCE: u32 = 392;
pub const REGROWTH: u32 = 9719;

pub fn register(db: &mut CardDb) {
    let regrowth = Effect::MoveZone {
        what: EffectTarget::Target(TargetSpec {
            kind: TargetKind::CardInZone {
                zone: Zone::Graveyard,
                filter: CardFilter::ControlledBy(PlayerRef::Controller),
            },
            min: 1,
            max: 1,
            distinct: true,
        }),
        to: ZoneDest { zone: Zone::Hand, pos: ZonePos::Any },
        tapped: false,
    };
    db.insert(
        spell(REGROWTH, "Regrowth", CardType::Sorcery, Color::Green, mana_cost(1, &[(Color::Green, 1)]), regrowth)
            .with_text("Return target card from your graveyard to your hand."),
    );

    let mut abilities = helpers::enters_prepared(REGROWTH);
    abilities.push(Ability::Triggered {
        event: EventPattern::SelfAttacks,
        condition: Some(Condition::CountAtLeast {
            zone: Zone::Battlefield,
            filter: CardFilter::HasCardType(CardType::Land),
            controller: Some(PlayerRef::Controller),
            n: ValueExpr::Fixed(8),
        }),
        intervening_if: true,
        effect: Effect::BecomePrepared,
    });
    let mut front = creature(
        EMERITUS_OF_ABUNDANCE,
        "Emeritus of Abundance",
        &[CreatureType::Elf, CreatureType::Druid],
        Color::Green,
        mana_cost(2, &[(Color::Green, 1)]),
        3,
        4,
        abilities,
    );
    front.text = "This creature enters prepared. Whenever this creature attacks, if you control eight or more lands, this creature becomes prepared. (While it's prepared, you may cast a copy of its spell. Doing so unprepares it.)\n// Regrowth {1}{G} Sorcery — Return target card from your graveyard to your hand.".to_string();
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
    fn emeritus_of_abundance_ir_and_regrowth() {
        let mut db = CardDb::default();
        register(&mut db);
        let f = db.get(EMERITUS_OF_ABUNDANCE).unwrap();
        assert!(matches!(f.abilities[0], Ability::Prepare { spell: REGROWTH }));
        assert!(matches!(f.abilities[1], Ability::Triggered { event: EventPattern::SelfEnters, .. }));
        assert!(matches!(
            f.abilities[2],
            Ability::Triggered { event: EventPattern::SelfAttacks, intervening_if: true, .. }
        ));
        // Behaviour: Regrowth returns a card from the graveyard to hand.
        let mut state = build_game(1, &[&[], &[]]);
        let card = state.add_card(PlayerId(0), state.card_db().get(grp::SHOCK).unwrap().chars.clone(), Zone::Graveyard);
        let effect = state.card_db().get(REGROWTH).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
        e.resolve_effect(
            &effect,
            &ResolutionCtx {
                controller: Some(PlayerId(0)),
                chosen_targets: vec![Target::Object(card)],
                ..Default::default()
            },
            WbReason::Resolve(StackId(0)),
        );
        assert!(e.state.player(PlayerId(0)).hand.contains(&card), "Regrowth returned the card to hand");
    }
}
