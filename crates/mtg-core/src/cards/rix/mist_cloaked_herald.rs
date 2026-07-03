//! Mist-Cloaked Herald — `{U}` Creature — Merfolk Warrior 1/1 (first printed RIX, Rivals of
//! Ixalan).
//!
//! Oracle:
//!   This creature can't be blocked.
//!
//! **Fully implemented** — a printed static ability (CR 604/611.2) that paints the
//! `CantBeBlocked` evasion qualification (CR 509.1b) on the creature itself: `Ability::Static{
//! Qualification(CantBeBlocked), affects: ItSelf-in-Battlefield }`. `chars::gather_statics` walks
//! the battlefield and paints the marker onto the source (`ItSelf` matches src == target); combat's
//! `can_block` reads `computed(attacker).has_qualification(CantBeBlocked)` and excludes every
//! blocker (CR 509.1b). Unlike Escape Tunnel — which *grants* `CantBeBlocked` as a one-shot
//! until-EOT effect to a target — the Herald carries it as a printed static on itself.

use crate::basics::Color;
use crate::cards::helpers::itself;
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, Qualification, StaticContribution};
use crate::effects::condition::Duration;
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const MIST_CLOAKED_HERALD: u32 = 118;

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        MIST_CLOAKED_HERALD,
        "Mist-Cloaked Herald",
        &[CreatureType::Merfolk, CreatureType::Warrior],
        Color::Blue,
        mana_cost(0, &[(Color::Blue, 1)]),
        1,
        1,
        vec![Ability::Static {
            contribution: StaticContribution::Qualification(Qualification::CantBeBlocked),
            affects: itself(),
            duration: Duration::WhileSourcePresent,
        }],
    );
    def.text = "This creature can't be blocked.".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::basics::CardType;
    use expect_test::expect;

    #[test]
    fn mist_cloaked_herald_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(MIST_CLOAKED_HERALD).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Creature]);
        assert_eq!(def.chars.power, Some(1));
        assert_eq!(def.chars.toughness, Some(1));
        assert!(def.fully_implemented);
        assert!(!def.is_mana_source());
        // A single printed static painting CantBeBlocked on itself (ItSelf, on the battlefield).
        expect![[r#"
            [
                Static {
                    contribution: Qualification(
                        CantBeBlocked,
                    ),
                    affects: SelectSpec {
                        zone: Battlefield,
                        filter: ItSelf,
                        chooser: Controller,
                        min: Fixed(
                            0,
                        ),
                        max: Fixed(
                            0,
                        ),
                    },
                    duration: WhileSourcePresent,
                },
            ]"#]]
        .assert_eq(&format!("{:#?}", def.abilities));
    }

    /// Behaviour: the Herald's printed static paints `CantBeBlocked` on itself, so the block
    /// declaration excludes every would-be blocker (CR 509.1b) and its damage goes through
    /// unblocked. Drives the real `declare_blockers` path with a maximally-aggressive P1 (blocks
    /// anything it legally can) — unlike Escape Tunnel, the qualification is the card's own printed
    /// static, painted with no resolution step.
    #[test]
    fn mist_cloaked_herald_cant_be_blocked() {
        use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView};
        use crate::basics::{Target, Zone};
        use crate::cards::{grp, starter_db};
        use crate::combat::{Attack, CombatState};
        use crate::ids::PlayerId;
        use crate::priority::Engine;
        use crate::state::GameState;
        use std::sync::Arc;

        // Blocks every attacker it legally can (empty `may_block` ⇒ declares nothing).
        #[derive(Clone)]
        struct BlockAll;
        impl Agent for BlockAll {
            fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
                match req {
                    DecisionRequest::DeclareBlockers { eligible, .. } => DecisionResponse::Pairs(
                        eligible
                            .iter()
                            .enumerate()
                            .filter(|(_, o)| !o.may_block.is_empty())
                            .map(|(i, _)| (i as u32, 0))
                            .collect(),
                    ),
                    _ => DecisionResponse::Pass,
                }
            }
        }

        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(starter_db()));
        // P0's Herald attacks; P1's Grizzly Bears (a vanilla 2/2) would like to block.
        let herald = {
            let c = state.card_db().get(MIST_CLOAKED_HERALD).unwrap().chars.clone();
            let id = state.add_card(PlayerId(0), c, Zone::Battlefield);
            state.objects.get_mut(&id).unwrap().summoning_sick = false;
            id
        };
        {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            let id = state.add_card(PlayerId(1), c, Zone::Battlefield);
            state.objects.get_mut(&id).unwrap().summoning_sick = false;
        }

        let mut e = Engine::new(state, vec![Box::new(BlockAll), Box::new(BlockAll)]);

        // The printed static paints the qualification with no resolution step.
        assert!(
            e.state.computed(herald).has_qualification(Qualification::CantBeBlocked),
            "the Herald's printed static makes it unblockable on the battlefield"
        );

        // Set up combat with the Herald attacking, then let P1 try to declare blocks.
        e.state.active_player = PlayerId(0);
        e.state.combat = Some(CombatState {
            attackers: vec![Attack { attacker: herald, defender: Target::Player(PlayerId(1)) }],
            blocks: vec![],
        });
        e.declare_blockers();
        assert!(
            e.state.combat.as_ref().unwrap().blocks.is_empty(),
            "P1 wanted to block but the Herald has no legal blocker (CR 509.1b)"
        );

        // …and the unblocked 1/1 Herald's 1 damage reaches P1's face.
        let life_before = e.state.player(PlayerId(1)).life;
        e.combat_damage();
        assert_eq!(
            e.state.player(PlayerId(1)).life,
            life_before - 1,
            "the unblocked 1/1 Herald deals 1 to P1's face"
        );
    }
}
