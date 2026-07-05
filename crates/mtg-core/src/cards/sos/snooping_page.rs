//! Snooping Page — `{1}{W}{B}` Creature — Human Cleric 2/3.
//!
//! Oracle: "Repartee — Whenever you cast an instant or sorcery spell that targets a creature, this
//! creature can't be blocked this turn.
//! Whenever this creature deals combat damage to a player, you draw a card and lose 1 life."
//!
//! **Fully implemented** — a Repartee cast-trigger granting itself `CantBeBlocked` until end of turn,
//! plus a per-creature combat-damage trigger. The latter uses the new
//! `EventPattern::SelfDealsCombatDamageToPlayer` (queued from the damage source in `combat_damage`,
//! once per creature that dealt combat damage to a player) — the per-creature analogue of the batched
//! `YouDealCombatDamageToPlayer`.

use crate::basics::Color;
use crate::cards::helpers::instant_or_sorcery;
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern, Qualification};
use crate::effects::condition::Duration;
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const SNOOPING_PAGE: u32 = 443;

pub fn register(db: &mut CardDb) {
    // "Repartee — Whenever you cast an I/S spell that targets a creature, this can't be blocked this turn."
    let repartee = Ability::Triggered {
        event: EventPattern::SpellCastTargetingCreature(instant_or_sorcery()),
        condition: None,
        intervening_if: false,
        effect: Effect::GrantQualification {
            what: EffectTarget::SourceSelf,
            qualification: Qualification::CantBeBlocked,
            duration: Duration::UntilEndOfTurn,
        },
    };
    // "Whenever this creature deals combat damage to a player, you draw a card and lose 1 life."
    let combat = Ability::Triggered {
        event: EventPattern::SelfDealsCombatDamageToPlayer,
        condition: None,
        intervening_if: false,
        effect: Effect::Sequence(vec![
            Effect::Draw { who: PlayerRef::Controller, count: ValueExpr::Fixed(1) },
            Effect::LoseLife { who: PlayerRef::Controller, amount: ValueExpr::Fixed(1) },
        ]),
    };
    let mut def = creature(
        SNOOPING_PAGE,
        "Snooping Page",
        &[CreatureType::Human, CreatureType::Cleric],
        Color::White,
        mana_cost(1, &[(Color::White, 1), (Color::Black, 1)]),
        2,
        3,
        vec![repartee, combat],
    );
    def.chars.colors = vec![Color::White, Color::Black];
    def.text = "Repartee — Whenever you cast an instant or sorcery spell that targets a creature, this creature can't be blocked this turn.\nWhenever this creature deals combat damage to a player, you draw a card and lose 1 life.".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{CastVariant, RandomAgent};
    use crate::basics::{Target, Zone};
    use crate::cards::{build_game, grp, starter_db};
    use crate::combat::{Attack, CombatState};
    use crate::ids::PlayerId;
    use crate::priority::Engine;
    use std::sync::Arc;

    #[test]
    fn snooping_page_shape() {
        let mut db = starter_db();
        register(&mut db);
        let def = db.get(SNOOPING_PAGE).unwrap();
        assert_eq!((def.chars.power, def.chars.toughness), (Some(2), Some(3)));
        assert_eq!(def.chars.colors, vec![Color::White, Color::Black]);
        assert!(def.fully_implemented);
        assert!(matches!(
            def.abilities[1],
            Ability::Triggered { event: EventPattern::SelfDealsCombatDamageToPlayer, .. }
        ));
    }

    fn drive(e: &mut Engine) {
        loop {
            e.run_agenda();
            if e.state.stack.items.is_empty() {
                break;
            }
            e.resolve_top();
        }
    }

    /// Snooping Page attacks P1 and connects: its combat-damage trigger draws P0 a card and costs 1 life.
    #[test]
    fn combat_damage_to_player_draws_and_loses_life() {
        let mut db = starter_db();
        register(&mut db);
        let mut state = build_game(1, &[&[], &[]]);
        state.set_card_db(Arc::new(db));
        let page = state.add_card(
            PlayerId(0),
            state.card_db().get(SNOOPING_PAGE).unwrap().chars.clone(),
            Zone::Battlefield,
        );
        // A card in P0's library to draw.
        {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Library);
        }
        state.active_player = PlayerId(0);
        state.combat = Some(CombatState {
            attackers: vec![Attack { attacker: page, defender: Target::Player(PlayerId(1)) }],
            blocks: vec![],
        });
        let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
        let (p1_life, p0_life) = (e.state.player(PlayerId(1)).life, e.state.player(PlayerId(0)).life);
        let hand_before = e.state.player(PlayerId(0)).hand.len();

        e.combat_damage(); // P1 takes 2 → Snooping Page's SelfDealsCombatDamageToPlayer queues
        assert_eq!(e.state.player(PlayerId(1)).life, p1_life - 2, "combat damage landed");
        drive(&mut e); // resolve the trigger

        assert_eq!(e.state.player(PlayerId(0)).hand.len(), hand_before + 1, "drew a card");
        assert_eq!(e.state.player(PlayerId(0)).life, p0_life - 1, "lost 1 life");
    }
}
