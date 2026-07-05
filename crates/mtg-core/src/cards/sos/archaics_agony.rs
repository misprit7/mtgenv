//! Archaic's Agony — `{4}{R}` Sorcery (first printed SOS).
//!
//! Oracle: "Converge — Archaic's Agony deals X damage to target creature, where X is the number of
//! colors of mana spent to cast this spell. Exile cards from the top of your library equal to the
//! excess damage dealt to that creature this way. You may play those cards until the end of your next
//! turn."
//!
//! **Fully implemented** — one `Effect::DealDamageExcessImpulse{ amount: ColorsSpent, YourNextTurn }`:
//! Converge damage (`ValueExpr::ColorsSpent`, recorded at cast) to a target creature, then exile from
//! the top of your library a number of cards equal to the **excess** damage (CR 120.7 — amount beyond
//! what was lethal), each castable/playable until the end of your next turn (the impulse machinery).

use crate::basics::{CardType, Color};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::ValueExpr;
use crate::effects::{Effect, EffectTarget, PlayWindow};

/// grp id (per-set ids live near their cards).
pub const ARCHAICS_AGONY: u32 = 452;

pub fn register(db: &mut CardDb) {
    let effect = Effect::DealDamageExcessImpulse {
        amount: ValueExpr::ColorsSpent,
        to: EffectTarget::Target(TargetSpec {
            kind: TargetKind::Creature(CardFilter::Any),
            min: 1,
            max: 1,
            distinct: true,
        }),
        window: PlayWindow::YourNextTurn,
    };
    db.insert(
        spell(
            ARCHAICS_AGONY,
            "Archaic's Agony",
            CardType::Sorcery,
            Color::Red,
            mana_cost(4, &[(Color::Red, 1)]),
            effect,
        )
        .with_text("Converge — Archaic's Agony deals X damage to target creature, where X is the number of colors of mana spent to cast this spell. Exile cards from the top of your library equal to the excess damage dealt to that creature this way. You may play those cards until the end of your next turn."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::RandomAgent;
    use crate::basics::{Target, Zone};
    use crate::cards::{build_game, grp};
    use crate::effects::action::{ResolutionCtx, WbReason};
    use crate::ids::{ObjId, PlayerId, StackId};
    use crate::priority::Engine;

    #[test]
    fn archaics_agony_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(ARCHAICS_AGONY).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Sorcery]);
        assert!(matches!(
            def.spell_effect().unwrap(),
            Effect::DealDamageExcessImpulse { amount: ValueExpr::ColorsSpent, window: PlayWindow::YourNextTurn, .. }
        ));
    }

    /// Resolve with 3 colors spent at a 1/1: 3 damage (2 excess over the 1 needed) → the top 2 library
    /// cards are exiled with impulse play-permission, and the creature dies.
    #[test]
    fn excess_damage_impulses_that_many_cards() {
        let mut state = build_game(1, &[&[], &[]]);
        // The Agony source, with 3 colors of mana recorded as spent.
        let agony = state.add_card(PlayerId(0), state.card_db().get(ARCHAICS_AGONY).unwrap().chars.clone(), Zone::Stack);
        state.objects.get_mut(&agony).unwrap().colors_spent = 3;
        // A 1/1 target (remaining toughness 1 → 3 damage = 2 excess).
        let bird = state.add_card(PlayerId(1), state.card_db().get(grp::ELVISH_VISIONARY).unwrap().chars.clone(), Zone::Battlefield);
        // Library: `library.last()` is the TOP, so add the bottom card first. The top 2 (l1, l2) are
        // the ones the 2 excess exiles take.
        state.add_card(PlayerId(0), state.card_db().get(grp::ISLAND).unwrap().chars.clone(), Zone::Library); // bottom, stays
        let l2 = state.add_card(PlayerId(0), state.card_db().get(grp::MOUNTAIN).unwrap().chars.clone(), Zone::Library);
        let l1 = state.add_card(PlayerId(0), state.card_db().get(grp::FOREST).unwrap().chars.clone(), Zone::Library); // top
        state.active_player = PlayerId(0);
        let effect = state.card_db().get(ARCHAICS_AGONY).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);

        e.resolve_effect(
            &effect,
            &ResolutionCtx { controller: Some(PlayerId(0)), source: Some(agony), chosen_targets: vec![Target::Object(bird)], ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        e.run_agenda(); // collect the lethal-damage SBA

        // The two top cards (l2 was on top, then l1) are exiled with play permission.
        for card in [l1, l2] {
            assert_eq!(e.state.object(card).zone, Zone::Exile, "excess card impulse-exiled");
            assert!(e.state.object(card).castable_from_exile, "with play permission");
            assert!(e.state.object(card).play_until_turn.is_some(), "windowed");
        }
        assert_eq!(e.state.player(PlayerId(0)).library.len(), 1, "only the excess (2) were exiled");
        assert_eq!(e.state.object(bird).zone, Zone::Graveyard, "the 1/1 took 3 and died");
    }

    /// No excess when the damage isn't more than lethal: 2 colors at a 4/4 → 2 damage, 0 excess → no
    /// cards exiled.
    #[test]
    fn no_excess_no_exile() {
        let mut state = build_game(1, &[&[], &[]]);
        let agony = state.add_card(PlayerId(0), state.card_db().get(ARCHAICS_AGONY).unwrap().chars.clone(), Zone::Stack);
        state.objects.get_mut(&agony).unwrap().colors_spent = 2;
        let giant = state.add_card(PlayerId(1), state.card_db().get(grp::HILL_GIANT).unwrap().chars.clone(), Zone::Battlefield); // 3/3
        for grp_id in [grp::FOREST, grp::MOUNTAIN] {
            state.add_card(PlayerId(0), state.card_db().get(grp_id).unwrap().chars.clone(), Zone::Library);
        }
        state.active_player = PlayerId(0);
        let effect = state.card_db().get(ARCHAICS_AGONY).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
        e.resolve_effect(
            &effect,
            &ResolutionCtx { controller: Some(PlayerId(0)), source: Some(agony), chosen_targets: vec![Target::Object(giant)], ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        e.run_agenda();
        assert_eq!(e.state.player(PlayerId(0)).library.len(), 2, "2 damage to a 3/3 = no excess → nothing exiled");
        assert_eq!(e.state.object(giant).zone, Zone::Battlefield, "the 3/3 survived");
        let _ = (giant, ObjId(0));
    }
}
