//! Restoration Seminar — `{5}{W}{W}` Sorcery — Lesson (first printed SOS).
//!
//! Oracle: "Return target nonland permanent card from your graveyard to the battlefield. Paradigm
//! (Then exile this spell. After you first resolve a spell with this name, you may cast a copy of it
//! from exile without paying its mana cost at the beginning of each of your first main phases.)"
//!
//! **Fully implemented.** Reanimation: `MoveZone` a targeted nonland permanent card (a graveyard card
//! with a permanent type other than land) from your graveyard to the battlefield. **Paradigm** is the
//! shared bundle from [`crate::cards::helpers::paradigm_abilities`].

use crate::basics::{CardType, Color, Zone, ZoneDest, ZonePos};
use crate::cards::{helpers, mana_cost, spell, CardDb};
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::PlayerRef;
use crate::effects::{Effect, EffectTarget};
use crate::subtypes::{SpellType, Subtype};

/// grp id (per-set ids live near their cards).
pub const RESTORATION_SEMINAR: u32 = 370;

/// "a nonland permanent card" — a card with a permanent type other than land (CR 110.4a). Excludes
/// instants/sorceries (nonpermanent) and lands.
fn nonland_permanent() -> CardFilter {
    CardFilter::AnyOf(vec![
        CardFilter::HasCardType(CardType::Artifact),
        CardFilter::HasCardType(CardType::Battle),
        CardFilter::HasCardType(CardType::Creature),
        CardFilter::HasCardType(CardType::Enchantment),
        CardFilter::HasCardType(CardType::Planeswalker),
    ])
}

pub fn register(db: &mut CardDb) {
    let effect = Effect::MoveZone {
        what: EffectTarget::Target(TargetSpec {
            kind: TargetKind::CardInZone {
                zone: Zone::Graveyard,
                filter: CardFilter::All(vec![
                    CardFilter::ControlledBy(PlayerRef::Controller),
                    nonland_permanent(),
                ]),
            },
            min: 1,
            max: 1,
            distinct: true,
        }),
        to: ZoneDest { zone: Zone::Battlefield, pos: ZonePos::Any },
        tapped: false,
    };
    let mut def = spell(
        RESTORATION_SEMINAR,
        "Restoration Seminar",
        CardType::Sorcery,
        Color::White,
        mana_cost(5, &[(Color::White, 2)]),
        effect,
    )
    .with_text(
        "Return target nonland permanent card from your graveyard to the battlefield. Paradigm (Then exile this spell. After you first resolve a spell with this name, you may cast a copy of it from exile without paying its mana cost at the beginning of each of your first main phases.)",
    );
    def.chars.subtypes = vec![Subtype::Spell(SpellType::Lesson)];
    def.abilities.extend(helpers::paradigm_abilities());
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, CastVariant, DecisionRequest, DecisionResponse, PlayerView};
    use crate::basics::Phase;
    use crate::cards::grp;
    use crate::ids::PlayerId;
    use crate::priority::Engine;
    use crate::state::GameState;

    fn db_with_card() -> CardDb {
        let mut db = crate::cards::starter_db();
        register(&mut db);
        db
    }

    #[test]
    fn restoration_seminar_shape() {
        let db = db_with_card();
        let def = db.get(RESTORATION_SEMINAR).unwrap();
        assert_eq!(def.chars.subtypes, vec![Subtype::Spell(SpellType::Lesson)]);
        assert!(def.fully_implemented);
        assert!(matches!(def.abilities.get(1), Some(crate::effects::ability::Ability::Paradigm)));
    }

    /// Picks target-slot-0 candidate 0 for every ChooseTargets; passes otherwise.
    struct PickFirst;
    impl Agent for PickFirst {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::ChooseTargets { slots, .. } => DecisionResponse::Pairs(
                    slots.iter().enumerate().map(|(si, _)| (si as u32, 0u32)).collect(),
                ),
                _ => DecisionResponse::Pass,
            }
        }
    }

    /// Cast it with a creature card in your graveyard: the creature returns to the battlefield and the
    /// Lesson exiles itself (Paradigm).
    #[test]
    fn reanimates_a_graveyard_creature_then_self_exiles() {
        let mut state = GameState::new(2, 1);
        state.set_card_db(std::sync::Arc::new(db_with_card()));
        let card = {
            let c = state.card_db().get(RESTORATION_SEMINAR).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Hand)
        };
        let bears = {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Graveyard)
        };
        for _ in 0..7 {
            let p = state.card_db().get(grp::PLAINS).unwrap().chars.clone();
            state.add_card(PlayerId(0), p, Zone::Battlefield);
        }
        let mut e = Engine::new(state, vec![Box::new(PickFirst), Box::new(PickFirst)]);
        e.state.phase = Phase::PrecombatMain;

        e.cast_spell(PlayerId(0), card, CastVariant::Normal);
        e.resolve_top();

        assert_eq!(
            e.state.object(bears).zone,
            Zone::Battlefield,
            "the graveyard creature was reanimated"
        );
        assert!(
            e.state.player(PlayerId(0)).battlefield.contains(&bears),
            "it is on your battlefield"
        );
        assert!(
            e.state.player(PlayerId(0)).exile.contains(&card),
            "Paradigm exiled the Lesson instead of the graveyard"
        );
    }
}
