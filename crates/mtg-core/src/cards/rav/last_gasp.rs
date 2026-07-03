//! Last Gasp — `{1}{B}` Instant (first printed RAV, Ravnica: City of Guilds; reprinted in SOS).
//!
//! Oracle: "Target creature gets -3/-3 until end of turn."
//!
//! **Fully implemented** — a single `PumpPT` of -3/-3 until end of turn (CR 611) on the one declared
//! target creature. A creature reduced to 0 toughness dies to the lethal-toughness SBA (CR 704.5f).

use crate::basics::{CardType, Color};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::condition::Duration;
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::ValueExpr;
use crate::effects::{Effect, EffectTarget};

/// grp id (per-set ids live near their cards).
pub const LAST_GASP: u32 = 223;

pub fn register(db: &mut CardDb) {
    let effect = Effect::PumpPT {
        what: EffectTarget::Target(TargetSpec {
            kind: TargetKind::Creature(CardFilter::Any),
            min: 1,
            max: 1,
            distinct: true,
        }),
        power: ValueExpr::Fixed(-3),
        toughness: ValueExpr::Fixed(-3),
        duration: Duration::UntilEndOfTurn,
    };
    db.insert(
        spell(
            LAST_GASP,
            "Last Gasp",
            CardType::Instant,
            Color::Black,
            mana_cost(1, &[(Color::Black, 1)]),
            effect,
        )
        .with_text("Target creature gets -3/-3 until end of turn."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn last_gasp_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(LAST_GASP).unwrap();
        assert!(def.fully_implemented);
        expect![[r#"
            PumpPT {
                what: Target(
                    TargetSpec {
                        kind: Creature(
                            Any,
                        ),
                        min: 1,
                        max: 1,
                        distinct: true,
                    },
                ),
                power: Fixed(
                    -3,
                ),
                toughness: Fixed(
                    -3,
                ),
                duration: UntilEndOfTurn,
            }"#]].assert_eq(&format!("{:#?}", def.spell_effect().unwrap()));
    }

    /// Behaviour: -3/-3 shrinks a 2/2 to a would-be -1/-1; the 0-or-less toughness SBA kills it.
    #[test]
    fn last_gasp_kills_a_small_creature() {
        use crate::agent::RandomAgent;
        use crate::basics::{Target, Zone};
        use crate::cards::{build_game, grp};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        let mut state = build_game(1, &[&[], &[]]);
        let bears = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
        let victim = state.add_card(PlayerId(1), bears, Zone::Battlefield);
        let effect = state.card_db().get(LAST_GASP).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(
            state,
            vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))],
        );
        e.resolve_effect(
            &effect,
            &ResolutionCtx {
                controller: Some(PlayerId(0)),
                chosen_targets: vec![Target::Object(victim)],
                ..Default::default()
            },
            WbReason::Resolve(StackId(0)),
        );
        assert_eq!(e.state.computed(victim).toughness, Some(-1), "2 − 3 = -1 toughness");
        e.run_agenda(); // process the 0-or-less-toughness SBA
        assert!(e.state.players[1].graveyard.contains(&victim), "0-or-less toughness → dies (SBA)");
    }
}
