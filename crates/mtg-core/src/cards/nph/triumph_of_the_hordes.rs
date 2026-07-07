//! Triumph of the Hordes — `{2}{G}{G}` Sorcery (first printed NPH; reprinted on the SOS Mystical Archive `soa`).
//!
//! Oracle: "Until end of turn, creatures you control get +1/+1 and gain trample and infect. (Creatures
//! with infect deal damage to creatures in the form of -1/-1 counters and to players in the form of
//! poison counters.)"
//!
//! **Fully implemented** — the infect subsystem (`Keyword::Infect`, read in `apply_damage`): a `ForEach`
//! over your creatures grants each a `Becomes{ +1/+1, Trample, Infect }` until end of turn.

use crate::basics::{CardType, Color};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::ability::{Keyword, StaticContribution};
use crate::effects::condition::Duration;
use crate::effects::target::{CardFilter, SelectSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};

/// grp id (bonus-sheet `soa` cards use the 600+ block).
pub const TRIUMPH_OF_THE_HORDES: u32 = 654;

pub fn register(db: &mut CardDb) {
    // "Until end of turn, creatures you control get +1/+1 and gain trample and infect."
    let effect = Effect::ForEach {
        selector: SelectSpec {
            zone: crate::basics::Zone::Battlefield,
            filter: CardFilter::HasCardType(CardType::Creature),
            chooser: PlayerRef::Controller,
            min: ValueExpr::Fixed(0),
            max: ValueExpr::Fixed(999),
        },
        body: Box::new(Effect::Becomes {
            what: EffectTarget::Each,
            contributions: vec![
                StaticContribution::ModifyPT { power: 1, toughness: 1 },
                StaticContribution::GrantKeyword(Keyword::Trample),
                StaticContribution::GrantKeyword(Keyword::Infect),
            ],
            base_pt: None,
            duration: Duration::UntilEndOfTurn,
        }),
    };
    let def = spell(
        TRIUMPH_OF_THE_HORDES,
        "Triumph of the Hordes",
        CardType::Sorcery,
        Color::Green,
        mana_cost(2, &[(Color::Green, 2)]),
        effect,
    )
    .with_text(
        "Until end of turn, creatures you control get +1/+1 and gain trample and infect. (Creatures with infect deal damage to creatures in the form of -1/-1 counters and to players in the form of poison counters.)",
    );
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, CastVariant, DecisionRequest, DecisionResponse, PlayerView};
    use crate::basics::{CounterKind, DamageKind, Phase, Target, Zone};
    use crate::cards::{build_game, grp};
    use crate::ids::{ObjId, PlayerId};
    use crate::priority::Engine;

    #[derive(Clone)]
    struct Passive;
    impl Agent for Passive {
        fn decide(&mut self, _v: &PlayerView, _req: &DecisionRequest) -> DecisionResponse {
            DecisionResponse::Pass
        }
    }

    fn setup() -> (Engine, ObjId, ObjId) {
        let mut state = build_game(1, &[&[], &[]]);
        let triumph = state.add_card(PlayerId(0), state.card_db().get(TRIUMPH_OF_THE_HORDES).unwrap().chars.clone(), Zone::Hand);
        for _ in 0..4 {
            state.add_card(PlayerId(0), state.card_db().get(grp::FOREST).unwrap().chars.clone(), Zone::Battlefield);
        }
        let bear = state.add_card(PlayerId(0), state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Battlefield);
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let mut e = Engine::new(state, vec![Box::new(Passive), Box::new(Passive)]);
        e.cast_spell(PlayerId(0), triumph, CastVariant::Normal);
        e.run_agenda();
        while !e.state.stack.items.is_empty() {
            e.resolve_top();
            e.run_agenda();
        }
        (e, triumph, bear)
    }

    #[test]
    fn grants_plus_one_trample_and_infect() {
        let (e, _t, bear) = setup();
        assert_eq!(e.state.computed(bear).power, Some(3), "+1/+1");
        assert_eq!(e.state.computed(bear).toughness, Some(3));
        assert!(e.state.computed(bear).has_keyword(Keyword::Trample), "gains trample");
        assert!(e.state.computed(bear).has_keyword(Keyword::Infect), "gains infect");
    }

    /// Infect damage to a player is poison counters, not life loss (CR 702.90c).
    #[test]
    fn infect_damage_to_player_is_poison() {
        let (mut e, _t, bear) = setup();
        let life = e.state.player(PlayerId(1)).life;
        e.apply_damage(Target::Player(PlayerId(1)), 3, bear, DamageKind::Combat);
        assert_eq!(e.state.player(PlayerId(1)).poison, 3, "3 poison counters");
        assert_eq!(e.state.player(PlayerId(1)).life, life, "life unchanged (damage was poison)");
    }

    /// Infect damage to a creature is -1/-1 counters, not marked damage (CR 702.90b) — a 2/2 blocker
    /// takes 3 and dies to the toughness-0 SBA.
    #[test]
    fn infect_damage_to_creature_is_minus_counters() {
        let (mut e, _t, _bear) = setup();
        let victim = e.state.add_card(PlayerId(1), e.state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Battlefield);
        let attacker = e.state.player(PlayerId(0)).battlefield.iter().copied().find(|&id| e.state.computed(id).has_keyword(Keyword::Infect)).unwrap();
        e.apply_damage(Target::Object(victim), 3, attacker, DamageKind::Combat);
        assert_eq!(e.state.object(victim).counters.get(&CounterKind::MinusOneMinusOne), 3, "3 -1/-1 counters");
        assert_eq!(e.state.object(victim).damage_marked, 0, "no marked damage");
        e.run_agenda();
        assert!(e.state.player(PlayerId(1)).graveyard.contains(&victim), "-1/-1 counters killed it (toughness 0)");
    }
}
