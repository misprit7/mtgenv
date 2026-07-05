//! Vastlands Scavenger // Bind to Life — `{1}{G}{G}` Creature — Bear Druid 4/4 (Deathtouch) // `{4}{G}`
//! Instant (first printed SOS). A **Prepare** DFC whose back face is a self-mill reanimation.
//!
//! Front oracle: "Deathtouch. This creature enters prepared. (While it's prepared, you may cast a copy
//! of its spell. Doing so unprepares it.)"
//! Back oracle (Bind to Life): "Mill seven cards. Then put a creature card from among them onto the
//! battlefield."
//!
//! **Fully implemented** — the front is the usual Prepare rails on a 4/4 with Deathtouch. The back is
//! [`Effect::MillThenPutCreatureOntoBattlefield`]: you mill seven from your OWN library and then put a
//! creature card from among those seven onto the battlefield (it's yours — owner == controller, so no
//! control override is needed). Choosing which creature is a mandatory pick when any was milled.

use crate::basics::{CardType, Color};
use crate::cards::{creature, helpers, mana_cost, spell, CardDb};
use crate::effects::ability::Keyword;
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const VASTLANDS_SCAVENGER: u32 = 405;
/// The copy-only back-face spell (reserved 9700+ Prepare block).
pub const BIND_TO_LIFE: u32 = 9730;

pub fn register(db: &mut CardDb) {
    // Back face — "Bind to Life" ({4}{G} Instant): mill 7, then put a creature card among them onto bf.
    db.insert(
        spell(
            BIND_TO_LIFE,
            "Bind to Life",
            CardType::Instant,
            Color::Green,
            mana_cost(4, &[(Color::Green, 1)]),
            Effect::MillThenPutCreatureOntoBattlefield {
                who: PlayerRef::Controller,
                count: ValueExpr::Fixed(7),
            },
        )
        .with_text("Mill seven cards. Then put a creature card from among them onto the battlefield."),
    );

    // Front face — the 4/4; Deathtouch + enters-prepared via a `SelfEnters → BecomePrepared` trigger.
    let mut front = creature(
        VASTLANDS_SCAVENGER,
        "Vastlands Scavenger",
        &[CreatureType::Bear, CreatureType::Druid],
        Color::Green,
        mana_cost(1, &[(Color::Green, 2)]),
        4,
        4,
        helpers::enters_prepared(BIND_TO_LIFE),
    );
    front.chars.keywords = vec![Keyword::Deathtouch];
    front.text = "Deathtouch\nThis creature enters prepared. (While it's prepared, you may cast a copy of its spell. Doing so unprepares it.)\n// Bind to Life {4}{G} Instant — Mill seven cards. Then put a creature card from among them onto the battlefield.".to_string();
    db.insert(front);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, CastVariant, DecisionRequest, DecisionResponse, PlayerView};
    use crate::basics::{Phase, Zone};
    use crate::cards::grp;
    use crate::effects::ability::{Ability, EventPattern};
    use crate::ids::{ObjId, PlayerId};
    use crate::priority::Engine;
    use crate::state::GameState;

    fn db_with_card() -> CardDb {
        let mut db = crate::cards::starter_db();
        register(&mut db);
        db
    }

    #[test]
    fn vastlands_scavenger_ir() {
        let db = db_with_card();
        let front = db.get(VASTLANDS_SCAVENGER).unwrap();
        assert_eq!(front.chars.keywords, vec![Keyword::Deathtouch]);
        assert!(matches!(front.abilities[0], Ability::Prepare { spell: BIND_TO_LIFE }));
        assert!(matches!(
            front.abilities[1],
            Ability::Triggered { event: EventPattern::SelfEnters, .. }
        ));
        assert!(matches!(
            db.get(BIND_TO_LIFE).unwrap().spell_effect(),
            Some(Effect::MillThenPutCreatureOntoBattlefield { .. })
        ));
    }

    /// For SelectCards picks the first candidate; passes otherwise.
    struct PickFirst;
    impl Agent for PickFirst {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::SelectCards { from, .. } if !from.is_empty() => {
                    DecisionResponse::Indices(vec![0])
                }
                _ => DecisionResponse::Pass,
            }
        }
    }

    fn add(state: &mut GameState, who: PlayerId, grp_id: u32, zone: Zone) -> ObjId {
        let c = state.card_db().get(grp_id).unwrap().chars.clone();
        state.add_card(who, c, zone)
    }

    /// Headline: a prepared Vastlands Scavenger casts Bind to Life → mill 7 (a Grizzly Bears among 6
    /// Forests) → the Bears is put onto the battlefield, the six Forests stay in the graveyard.
    #[test]
    fn mills_seven_and_reanimates_a_creature_from_among_them() {
        let mut state = GameState::new(2, 1);
        state.set_card_db(std::sync::Arc::new(db_with_card()));
        let scavenger = add(&mut state, PlayerId(0), VASTLANDS_SCAVENGER, Zone::Battlefield);
        state.objects.get_mut(&scavenger).unwrap().prepared = true;
        // {4}{G} for the Bind to Life copy.
        for _ in 0..5 {
            add(&mut state, PlayerId(0), grp::FOREST, Zone::Battlefield);
        }
        // Library top 7: one Grizzly Bears + six Forests.
        let bears = add(&mut state, PlayerId(0), grp::GRIZZLY_BEARS, Zone::Library);
        for _ in 0..6 {
            add(&mut state, PlayerId(0), grp::FOREST, Zone::Library);
        }
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let bf_before = state.player(PlayerId(0)).battlefield.len();
        let mut e = Engine::new(state, vec![Box::new(PickFirst), Box::new(PickFirst)]);

        e.cast_prepared(PlayerId(0), scavenger);
        e.resolve_top(); // Bind to Life resolves: mill 7, put the Bears onto the battlefield
        e.run_agenda();

        assert!(e.state.player(PlayerId(0)).battlefield.contains(&bears), "the milled Bears was reanimated");
        assert!(!e.state.player(PlayerId(0)).graveyard.contains(&bears), "it left the graveyard");
        assert_eq!(
            e.state.player(PlayerId(0)).graveyard.len(),
            6,
            "the six milled Forests remain in the graveyard"
        );
        assert!(e.state.object(bears).summoning_sick, "the reanimated creature is summoning sick");
        assert_eq!(
            e.state.player(PlayerId(0)).battlefield.len(),
            bf_before + 1,
            "one creature (the Bears) joined the battlefield"
        );
    }

    /// With no creature among the milled cards, nothing is put onto the battlefield (all seven stay
    /// milled).
    #[test]
    fn mills_seven_but_reanimates_nothing_without_a_creature() {
        let mut state = GameState::new(2, 1);
        state.set_card_db(std::sync::Arc::new(db_with_card()));
        let effect = state.card_db().get(BIND_TO_LIFE).unwrap().spell_effect().unwrap().clone();
        for _ in 0..7 {
            add(&mut state, PlayerId(0), grp::FOREST, Zone::Library);
        }
        let bf_before = state.player(PlayerId(0)).battlefield.len();
        let mut e = Engine::new(state, vec![Box::new(PickFirst), Box::new(PickFirst)]);
        e.resolve_effect(
            &effect,
            &crate::effects::action::ResolutionCtx {
                controller: Some(PlayerId(0)),
                ..Default::default()
            },
            crate::effects::action::WbReason::Resolve(crate::ids::StackId(99)),
        );
        assert_eq!(e.state.player(PlayerId(0)).graveyard.len(), 7, "all seven Forests were milled");
        assert_eq!(
            e.state.player(PlayerId(0)).battlefield.len(),
            bf_before,
            "no creature among them → nothing put onto the battlefield"
        );
    }
}
