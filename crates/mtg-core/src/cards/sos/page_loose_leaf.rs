//! Page, Loose Leaf — `{2}` Legendary Artifact Creature — Construct (0/2).
//!
//! Oracle:
//!   {T}: Add {C}.
//!   Grandeur — Discard another card named Page, Loose Leaf: Reveal cards from the top of your library
//!   until you reveal an instant or sorcery card. Put that card into your hand and the rest on the bottom
//!   of your library in a random order.
//!
//! **Fully implemented** — a colorless mana dork plus a **Grandeur** ability (CR 702 — an activated
//! ability whose cost is "discard another card with this name," modeled as a `CostComponent::Discard` of a
//! hand card named "Page, Loose Leaf"; since Page itself is on the battlefield, a hand copy is always
//! "another"). Its effect is the new [`Effect::RevealFromTopUntilToHand`] (reveal-until an instant/sorcery
//! → hand, rest random-bottom).

use crate::basics::{CardType, Color};
use crate::cards::helpers::instant_or_sorcery;
use crate::cards::{creature, mana_ability, CardDb};
use crate::effects::ability::{Ability, Cost, CostComponent, Timing};
use crate::effects::target::{CardFilter, SelectSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;
use crate::basics::Zone;
use crate::subtypes::{CreatureType, Supertype};

/// grp id (per-set ids live near their cards).
pub const PAGE_LOOSE_LEAF: u32 = 457;

const NAME: &str = "Page, Loose Leaf";

/// "Grandeur — Discard another card named Page, Loose Leaf: Reveal until an instant/sorcery → hand, rest
/// random-bottom." The cost discards a hand card of this name (Page itself is on the battlefield, so any
/// hand copy is "another").
fn grandeur() -> Ability {
    Ability::Activated {
        cost: Cost {
            mana: None,
            components: vec![CostComponent::Discard(SelectSpec {
                zone: Zone::Hand,
                filter: CardFilter::Named(NAME.to_string()),
                chooser: PlayerRef::Controller,
                min: ValueExpr::Fixed(1),
                max: ValueExpr::Fixed(1),
            })],
        },
        effect: Effect::RevealFromTopUntilToHand { filter: instant_or_sorcery() },
        timing: Timing::Instant,
        restriction: None,
        is_mana: false,
    }
}

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        PAGE_LOOSE_LEAF,
        NAME,
        &[CreatureType::Construct],
        Color::Colorless,
        crate::cards::mana_cost(2, &[]),
        0,
        2,
        vec![mana_ability(Color::Colorless), grandeur()],
    );
    def.chars.card_types = vec![CardType::Artifact, CardType::Creature];
    def.chars.colors = vec![]; // an artifact with no colored mana is colorless (CR 105.2c).
    def.chars.supertypes = vec![Supertype::Legendary];
    def.text = "{T}: Add {C}.\nGrandeur — Discard another card named Page, Loose Leaf: Reveal cards from the top of your library until you reveal an instant or sorcery card. Put that card into your hand and the rest on the bottom of your library in a random order.".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{AbilityRef, Agent, DecisionRequest, DecisionResponse, PlayerView};
    use crate::cards::{grp, starter_db};
    use crate::ids::{ObjId, PlayerId};
    use crate::priority::Engine;
    use crate::state::GameState;
    use std::sync::Arc;

    fn db_with_card() -> CardDb {
        let mut db = starter_db();
        register(&mut db);
        db
    }

    #[test]
    fn page_shape() {
        let db = db_with_card();
        let def = db.get(PAGE_LOOSE_LEAF).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Artifact, CardType::Creature]);
        assert_eq!(def.chars.supertypes, vec![Supertype::Legendary]);
        assert_eq!((def.chars.power, def.chars.toughness), (Some(0), Some(2)));
        assert!(def.chars.colors.is_empty(), "colorless");
        assert!(def.is_mana_source(), "the {{T}}: Add {{C}} ability");
        assert!(matches!(
            def.abilities[1],
            Ability::Activated { effect: Effect::RevealFromTopUntilToHand { .. }, .. }
        ));
    }

    /// Selects the discard fodder (a second Page) for the Grandeur cost; passes otherwise.
    struct GrandeurAgent {
        fodder: ObjId,
    }
    impl Agent for GrandeurAgent {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::SelectCards { from, .. } => {
                    let idx = from.iter().position(|o| *o == self.fodder).unwrap_or(0) as u32;
                    DecisionResponse::Indices(vec![idx])
                }
                _ => DecisionResponse::Pass,
            }
        }
    }

    /// The REAL Grandeur activation: discard a second Page, reveal past a land to the first instant, put it
    /// in hand, bottom the land.
    #[test]
    fn grandeur_reveals_until_instant() {
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(db_with_card()));
        let page = {
            let c = state.card_db().get(PAGE_LOOSE_LEAF).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        // A second Page in hand = the Grandeur discard fodder.
        let fodder = {
            let c = state.card_db().get(PAGE_LOOSE_LEAF).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Hand)
        };
        // Library, top → bottom: [Mountain, Lightning Bolt, Grizzly Bears]. `add_card` appends to the
        // library end (the top), so add bottom-first: Bears, then Bolt, then Mountain on top.
        let _bears = {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Library)
        };
        let bolt = {
            let c = state.card_db().get(grp::LIGHTNING_BOLT).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Library)
        };
        let mountain = {
            let c = state.card_db().get(grp::MOUNTAIN).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Library)
        };
        let mut e = Engine::new(state, vec![Box::new(GrandeurAgent { fodder }), Box::new(GrandeurAgent { fodder })]);

        e.activate_ability(PlayerId(0), page, AbilityRef(1)); // discard the fodder Page (the cost)
        e.resolve_top();
        // The revealed instant (Bolt) went to hand; the Mountain (revealed first) was bottomed.
        assert!(e.state.player(PlayerId(0)).hand.contains(&bolt), "the instant went to hand");
        assert!(e.state.player(PlayerId(0)).graveyard.contains(&fodder), "the second Page was discarded");
        // Mountain is on the bottom (front) of the library; the Bolt is gone from the library.
        let lib = &e.state.player(PlayerId(0)).library;
        assert!(!lib.contains(&bolt), "the instant left the library");
        assert_eq!(lib.first(), Some(&mountain), "the non-matching Mountain was bottomed");
    }
}
