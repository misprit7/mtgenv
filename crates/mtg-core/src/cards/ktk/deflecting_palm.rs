//! Deflecting Palm — `{R}{W}` Instant (first printed KTK; reprinted on the SOS Mystical Archive `soa`).
//!
//! Oracle: "The next time a source of your choice would deal damage to you this turn, prevent that
//! damage. If damage is prevented this way, Deflecting Palm deals that much damage to that source's
//! controller."
//!
//! **Fully implemented** — the damage-redirect subsystem: `Effect::DeflectDamage` asks the caster to
//! choose a source (a battlefield permanent) and arms a one-shot floating replacement scoped to it
//! (`FloatingRewrite::PreventAndRedirectToSourceController`). The next time that source would deal
//! damage to the caster this turn, the rewrite pass prevents it and stages equal damage to the
//! source's controller (CR 615). Sources on the stack / in other zones are a pool-scoped omission
//! (the common case — deflecting an attacker or a permanent's ability — is a battlefield permanent).

use crate::basics::{CardType, Color};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::value::PlayerRef;
use crate::effects::Effect;

/// grp id (bonus-sheet `soa` cards use the 600+ block).
pub const DEFLECTING_PALM: u32 = 660;

pub fn register(db: &mut CardDb) {
    let effect = Effect::DeflectDamage { who: PlayerRef::Controller };
    let mut def = spell(
        DEFLECTING_PALM,
        "Deflecting Palm",
        CardType::Instant,
        Color::Red,
        mana_cost(0, &[(Color::Red, 1), (Color::White, 1)]),
        effect,
    )
    .with_text(
        "The next time a source of your choice would deal damage to you this turn, prevent that damage. If damage is prevented this way, Deflecting Palm deals that much damage to that source's controller.",
    );
    def.chars.colors = vec![Color::Red, Color::White];
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView};
    use crate::basics::{DamageKind, Target, Zone};
    use crate::cards::build_game;
    use crate::effects::action::{ResolutionCtx, WbReason};
    use crate::effects::EffectTarget;
    use crate::ids::{ObjId, PlayerId, StackId};
    use crate::priority::Engine;
    use crate::state::Characteristics;

    /// Chooses index 0 for any `SelectCards` (the single candidate source), passes otherwise.
    #[derive(Clone)]
    struct PickFirst;
    impl Agent for PickFirst {
        fn decide(&mut self, _v: &PlayerView, r: &DecisionRequest) -> DecisionResponse {
            match r {
                DecisionRequest::SelectCards { .. } => DecisionResponse::Indices(vec![0]),
                _ => DecisionResponse::Pass,
            }
        }
    }

    fn red_creature() -> Characteristics {
        Characteristics {
            name: "Red Ogre".to_string(),
            card_types: vec![CardType::Creature],
            colors: vec![Color::Red],
            power: Some(3),
            toughness: Some(3),
            grp_id: 8400,
            ..Default::default()
        }
    }

    #[test]
    fn shape_is_deflect() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(DEFLECTING_PALM).unwrap();
        assert!(def.fully_implemented);
        assert!(matches!(def.spell_effect().unwrap(), Effect::DeflectDamage { .. }));
    }

    /// Real path: P0 casts Deflecting Palm naming P1's ogre. The ogre then "deals" 3 to P0 — the
    /// damage is prevented (P0's life unchanged) and redirected to P1, the ogre's controller.
    #[test]
    fn prevents_then_redirects_to_sources_controller() {
        let mut state = build_game(1, &[&[], &[]]);
        // The resolved Deflecting Palm object (its graveyard home) — the redirected damage's source.
        let dp = state.add_card(PlayerId(0), state.card_db().get(DEFLECTING_PALM).unwrap().chars.clone(), Zone::Graveyard);
        let ogre = state.add_card(PlayerId(1), red_creature(), Zone::Battlefield);
        let mut e = Engine::new(state, vec![Box::new(PickFirst), Box::new(PickFirst)]);
        let p0_life = e.state.player(PlayerId(0)).life;
        let p1_life = e.state.player(PlayerId(1)).life;

        // Arm: "the next time [ogre] would deal damage to you (P0) this turn, prevent + redirect".
        e.resolve_effect(
            &Effect::DeflectDamage { who: PlayerRef::Controller },
            &ResolutionCtx { controller: Some(PlayerId(0)), source: Some(dp), ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );

        // The ogre deals 3 damage to P0 — routed through the staged-action rewrite pass.
        deal_to_player(&mut e, ogre, PlayerId(0), 3);

        assert_eq!(e.state.player(PlayerId(0)).life, p0_life, "P0's damage was prevented");
        assert_eq!(e.state.player(PlayerId(1)).life, p1_life - 3, "3 redirected to the ogre's controller (P1)");
        // One-shot: a second hit from the ogre this turn is NOT deflected.
        deal_to_player(&mut e, ogre, PlayerId(0), 2);
        assert_eq!(e.state.player(PlayerId(0)).life, p0_life - 2, "rider was one-shot; second hit lands on P0");
    }

    /// Deal `amount` damage from `src` to `player` via the staged whiteboard → `commit` (so the
    /// replacement/rewrite pass runs, unlike a direct `apply_damage`).
    fn deal_to_player(e: &mut Engine, src: ObjId, player: PlayerId, amount: i32) {
        let ctx = ResolutionCtx {
            controller: Some(PlayerId(0)),
            source: Some(src),
            chosen_targets: vec![Target::Player(player)],
            ..Default::default()
        };
        e.resolve_effect(
            &Effect::DealDamage {
                amount: crate::effects::value::ValueExpr::Fixed(amount as i64),
                to: EffectTarget::ChosenIndex(0),
                kind: DamageKind::Noncombat,
            },
            &ctx,
            WbReason::Resolve(StackId(0)),
        );
    }
}
