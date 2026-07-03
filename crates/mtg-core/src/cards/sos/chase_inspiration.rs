//! Chase Inspiration — `{U}` Instant (first printed SOS).
//!
//! Oracle: "Target creature you control gets +0/+3 and gains hexproof until end of turn."
//!
//! **Fully implemented** — one declared target ("creature you control") gets a `PumpPT` (+0/+3
//! until end of turn) and `GrantKeyword(Hexproof)` for the same duration, the keyword grant
//! referencing that same chosen target via `ChosenIndex(0)`.

use crate::basics::{CardType, Color};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::ability::Keyword;
use crate::effects::condition::Duration;
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};

/// grp id (per-set ids live near their cards).
pub const CHASE_INSPIRATION: u32 = 204;

pub fn register(db: &mut CardDb) {
    let effect = Effect::Sequence(vec![
        Effect::PumpPT {
            what: EffectTarget::Target(TargetSpec {
                kind: TargetKind::Creature(CardFilter::ControlledBy(PlayerRef::Controller)),
                min: 1,
                max: 1,
                distinct: true,
            }),
            power: ValueExpr::Fixed(0),
            toughness: ValueExpr::Fixed(3),
            duration: Duration::UntilEndOfTurn,
        },
        Effect::GrantKeyword {
            what: EffectTarget::ChosenIndex(0),
            keyword: Keyword::Hexproof,
            duration: Duration::UntilEndOfTurn,
        },
    ]);
    db.insert(
        spell(
            CHASE_INSPIRATION,
            "Chase Inspiration",
            CardType::Instant,
            Color::Blue,
            mana_cost(0, &[(Color::Blue, 1)]),
            effect,
        )
        .with_text("Target creature you control gets +0/+3 and gains hexproof until end of turn."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn chase_inspiration_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(CHASE_INSPIRATION).unwrap();
        assert!(def.fully_implemented);
        expect![[r#"
            Sequence(
                [
                    PumpPT {
                        what: Target(
                            TargetSpec {
                                kind: Creature(
                                    ControlledBy(
                                        Controller,
                                    ),
                                ),
                                min: 1,
                                max: 1,
                                distinct: true,
                            },
                        ),
                        power: Fixed(
                            0,
                        ),
                        toughness: Fixed(
                            3,
                        ),
                        duration: UntilEndOfTurn,
                    },
                    GrantKeyword {
                        what: ChosenIndex(
                            0,
                        ),
                        keyword: Hexproof,
                        duration: UntilEndOfTurn,
                    },
                ],
            )"#]].assert_eq(&format!("{:#?}", def.spell_effect().unwrap()));
    }

    /// Behaviour: resolving Chase Inspiration on a 2/2 makes it a 2/5 with hexproof (until EOT).
    #[test]
    fn chase_inspiration_pumps_toughness_and_grants_hexproof() {
        use crate::agent::RandomAgent;
        use crate::basics::{Target, Zone};
        use crate::cards::{build_game, grp};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        let mut state = build_game(1, &[&[], &[]]);
        let bears = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
        let target = state.add_card(PlayerId(0), bears, Zone::Battlefield);
        let effect = state.card_db().get(CHASE_INSPIRATION).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(
            state,
            vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))],
        );
        e.resolve_effect(
            &effect,
            &ResolutionCtx {
                controller: Some(PlayerId(0)),
                chosen_targets: vec![Target::Object(target)],
                ..Default::default()
            },
            WbReason::Resolve(StackId(0)),
        );
        let cc = e.state.computed(target);
        assert_eq!(cc.power, Some(2), "power unchanged");
        assert_eq!(cc.toughness, Some(5), "+3 toughness");
        assert!(cc.has_keyword(Keyword::Hexproof), "gains hexproof");
    }
}
