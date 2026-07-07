//! Llanowar Elves — `{G}` Creature — Elf Druid 1/1 (first printed LEA). "{T}: Add {G}."
//!
//! A green mana dork. Its mana ability is first-class Effect IR (`{T}: Add {G}` via
//! `mana_ability`, an `Ability::Activated{is_mana:true}` + `Effect::AddMana`) — not the legacy
//! `mana_colors` shortcut. The engine gates activation by summoning sickness (C1, CR 302.6) so a
//! freshly-cast Llanowar can't tap the turn it enters.

use crate::basics::Color;
use crate::cards::{creature, mana_ability, mana_cost, CardDb};
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const LLANOWAR_ELVES: u32 = 100;

pub fn register(db: &mut CardDb) {
    db.insert(
        creature(
            LLANOWAR_ELVES,
            "Llanowar Elves",
            &[CreatureType::Elf, CreatureType::Druid],
            Color::Green,
            mana_cost(0, &[(Color::Green, 1)]),
            1,
            1,
            vec![mana_ability(Color::Green)],
        )
        .with_text("{T}: Add {G}."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn llanowar_elves_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(LLANOWAR_ELVES).unwrap();
        // A 1/1 Elf Druid whose `{T}: Add {G}` is a real mana ability (no `mana_colors` shortcut).
        assert_eq!(def.chars.power, Some(1));
        assert_eq!(def.chars.toughness, Some(1));
        assert_eq!(def.chars.subtypes, vec![CreatureType::Elf.into(), CreatureType::Druid.into()]);
        assert!(def.is_mana_source());
        expect![[r#"
            [
                Activated {
                    cost: Cost {
                        mana: None,
                        components: [
                            TapSelf,
                        ],
                    },
                    effect: AddMana {
                        who: Controller,
                        mana: ManaSpec {
                            produces: [
                                (
                                    Green,
                                    Fixed(
                                        1,
                                    ),
                                ),
                            ],
                            any_color: None,
                            one_of: None,
                            restriction: None,
                        },
                    },
                    timing: Instant,
                    restriction: None,
                    is_mana: true,
                },
            ]"#]].assert_eq(&format!("{:#?}", def.abilities));
    }

    /// Behaviour: resolving the `{T}: Add {G}` mana ability adds one green mana to your pool.
    #[test]
    fn llanowar_taps_for_green() {
        use crate::agent::RandomAgent;
        use crate::basics::{Color, Zone};
        use crate::cards::build_game;
        use crate::effects::ability::Ability;
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        let mut state = build_game(1, &[&[], &[]]);
        let chars = state.card_db().get(LLANOWAR_ELVES).unwrap().chars.clone();
        let elf = state.add_card(PlayerId(0), chars, Zone::Battlefield);
        let mana = match &state.card_db().get(LLANOWAR_ELVES).unwrap().abilities[0] {
            Ability::Activated { effect, .. } => effect.clone(),
            o => panic!("expected mana Activated, got {o:?}"),
        };
        let mut e = Engine::new(
            state,
            vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))],
        );
        e.resolve_effect(
            &mana,
            &ResolutionCtx { controller: Some(PlayerId(0)), source: Some(elf), ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        assert_eq!(e.state.players[0].mana_pool.amounts.get(&Color::Green), Some(&1));
    }

    /// #60 end-to-end (the REAL cast path): cast Llanowar Elves `{G}` from hand via `cast_spell`
    /// (auto-pays `{G}` from a Forest), then `resolve_top` puts the permanent onto the battlefield. It
    /// enters as a 1/1 Elf Druid. (Its `{T}: Add {G}` is covered above; this confirms the cast leg.)
    #[test]
    fn llanowar_cast_enters_as_a_one_one() {
        use crate::agent::{Agent, CastVariant, DecisionRequest, DecisionResponse, PlayerView};
        use crate::basics::{CardType, Zone};
        use crate::cards::{grp, starter_db};
        use crate::ids::PlayerId;
        use crate::priority::Engine;
        use crate::state::GameState;
        use std::sync::Arc;

        #[derive(Clone)]
        struct PassiveAgent;
        impl Agent for PassiveAgent {
            fn decide(&mut self, _v: &PlayerView, _req: &DecisionRequest) -> DecisionResponse {
                DecisionResponse::Pass
            }
        }

        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(starter_db()));
        let elf = {
            let c = state.card_db().get(LLANOWAR_ELVES).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Hand)
        };
        let forest = {
            let c = state.card_db().get(grp::FOREST).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield) // pays {G}
        };
        let _ = CastVariant::Normal;
        let mut e = Engine::new(state, vec![Box::new(PassiveAgent), Box::new(PassiveAgent)]);
        e.cast_spell(PlayerId(0), elf, CastVariant::Normal);
        e.resolve_top();

        assert!(e.state.object(forest).status.tapped, "the Forest was tapped to pay {{G}}");
        assert_eq!(e.state.object(elf).zone, Zone::Battlefield, "Llanowar entered the battlefield");
        let cc = e.state.computed(elf);
        assert!(cc.card_types.contains(&CardType::Creature), "it's a creature");
        assert_eq!((cc.power, cc.toughness), (Some(1), Some(1)), "a 1/1");
        assert_eq!(
            cc.subtypes,
            vec![CreatureType::Elf.into(), CreatureType::Druid.into()],
            "Elf Druid"
        );
    }
}
