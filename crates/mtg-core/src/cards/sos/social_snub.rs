//! Social Snub — `{1}{W}{B}` Sorcery.
//!
//! Oracle: "When you cast this spell while you control a creature, you may copy this spell. Each
//! player sacrifices a creature of their choice. Each opponent loses 1 life and you gain 1 life."
//!
//! **Fully implemented** — a copy-self edict over the SelfCast + CopySpellOnStack caps. The main
//! effect is an edict + drain (`Sacrifice{EachPlayer, a creature}` · `LoseLife{EachOpponent,1}` ·
//! `GainLife{Controller,1}`); the copy clause is a `Triggered{ SelfCast, if you control a creature,
//! Optional{ CopySpellOnStack{ Triggering, 1 } } }`. The copy (no targets, so no reselection) resolves
//! above the original, so the edict + drain happen twice when you copy.

use crate::basics::{CardType, Color, Zone};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::ability::{Ability, EventPattern};
use crate::effects::condition::Condition;
use crate::effects::target::{CardFilter, SelectSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};

/// grp id (per-set ids live near their cards).
pub const SOCIAL_SNUB: u32 = 428;

/// "a creature of their choice" — each affected player picks one of their own creatures.
fn a_creature() -> SelectSpec {
    SelectSpec {
        zone: Zone::Battlefield,
        filter: CardFilter::HasCardType(CardType::Creature),
        chooser: PlayerRef::EachPlayer,
        min: ValueExpr::Fixed(1),
        max: ValueExpr::Fixed(1),
    }
}

pub fn register(db: &mut CardDb) {
    let mut def = spell(
        SOCIAL_SNUB,
        "Social Snub",
        CardType::Sorcery,
        Color::White,
        mana_cost(1, &[(Color::White, 1), (Color::Black, 1)]),
        Effect::Sequence(vec![
            Effect::Sacrifice { who: PlayerRef::EachPlayer, what: a_creature() },
            Effect::LoseLife { who: PlayerRef::EachOpponent, amount: ValueExpr::Fixed(1) },
            Effect::GainLife { who: PlayerRef::Controller, amount: ValueExpr::Fixed(1) },
        ]),
    );
    def.chars.colors = vec![Color::White, Color::Black];
    // "When you cast this spell while you control a creature, you may copy this spell." A SelfCast
    // trigger gated on controlling a creature; the copy has no targets (no reselection).
    def.abilities.push(Ability::Triggered {
        event: EventPattern::SelfCast,
        condition: Some(Condition::CountAtLeast {
            zone: Zone::Battlefield,
            filter: CardFilter::HasCardType(CardType::Creature),
            controller: Some(PlayerRef::Controller),
            n: ValueExpr::Fixed(1),
        }),
        intervening_if: false,
        effect: Effect::Optional {
            prompt: "Copy Social Snub?".to_string(),
            body: Box::new(Effect::CopySpellOnStack {
                what: EffectTarget::Triggering,
                count: ValueExpr::Fixed(1),
                choose_new_targets: false,
            }),
        },
    });
    def.text = "When you cast this spell while you control a creature, you may copy this spell.\nEach player sacrifices a creature of their choice. Each opponent loses 1 life and you gain 1 life.".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, CastVariant, DecisionRequest, DecisionResponse, PlayerView};
    use crate::basics::Phase;
    use crate::cards::{grp, starter_db};
    use crate::ids::PlayerId;
    use crate::priority::Engine;
    use crate::state::GameState;
    use std::sync::Arc;

    fn db_with_card() -> CardDb {
        let mut db = starter_db();
        register(&mut db);
        db
    }

    #[test]
    fn social_snub_shape() {
        let db = db_with_card();
        let def = db.get(SOCIAL_SNUB).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Sorcery]);
        assert_eq!(def.chars.colors, vec![Color::White, Color::Black]);
        assert!(def.fully_implemented);
        assert!(matches!(def.abilities[0], Ability::Spell { .. }));
        assert!(matches!(
            def.abilities[1],
            Ability::Triggered { event: EventPattern::SelfCast, .. }
        ));
    }

    /// Sacrifices the first creature offered; answers the "copy?" confirm with `copy`.
    #[derive(Clone)]
    struct SnubAgent {
        copy: bool,
    }
    impl Agent for SnubAgent {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::Confirm { .. } => DecisionResponse::Bool(self.copy),
                DecisionRequest::SelectCards { from, .. } => {
                    DecisionResponse::Indices(if from.is_empty() { vec![] } else { vec![0] })
                }
                _ => DecisionResponse::Pass,
            }
        }
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

    /// Creatures in `p`'s graveyard (the sacrificed ones — Social Snub itself is a sorcery, so it's
    /// excluded).
    fn creatures_in_gy(e: &Engine, p: PlayerId) -> usize {
        e.state.player(p).graveyard.iter().filter(|&&o| e.state.object(o).chars.is_creature()).count()
    }

    /// Each player gets `n` Grizzly Bears; P0 also gets Social Snub + {1}{W}{B}. `copy` decides
    /// whether P0 takes the "you may copy" clause.
    fn setup(bears_each: usize, copy: bool) -> Engine {
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(db_with_card()));
        for p in [PlayerId(0), PlayerId(1)] {
            for _ in 0..bears_each {
                let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
                state.add_card(p, c, Zone::Battlefield);
            }
        }
        let snub = {
            let c = state.card_db().get(SOCIAL_SNUB).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Hand)
        };
        for g in [grp::PLAINS, grp::SWAMP, grp::PLAINS] {
            let c = state.card_db().get(g).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield); // {1}{W}{B}
        }
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let mut e = Engine::new(state, vec![Box::new(SnubAgent { copy }), Box::new(SnubAgent { copy })]);
        e.cast_spell(PlayerId(0), snub, CastVariant::Normal);
        e
    }

    /// Copy accepted: the edict + drain happen TWICE — each player sacrifices 2 creatures, P0 gains 2,
    /// each opponent loses 2.
    #[test]
    fn copying_doubles_the_edict_and_drain() {
        let mut e = setup(2, true);
        let p0_start = e.state.player(PlayerId(0)).life;
        let p1_start = e.state.player(PlayerId(1)).life;
        drive(&mut e);
        assert_eq!(creatures_in_gy(&e, PlayerId(0)), 2, "P0 sacrificed 2 creatures");
        assert_eq!(creatures_in_gy(&e, PlayerId(1)), 2, "P1 sacrificed 2 creatures");
        assert_eq!(e.state.player(PlayerId(0)).life, p0_start + 2, "P0 gained 1 twice");
        assert_eq!(e.state.player(PlayerId(1)).life, p1_start - 2, "P1 lost 1 twice");
    }

    /// Copy declined: the edict + drain happen once — each player sacrifices 1, P0 gains 1, P1 loses 1.
    #[test]
    fn declining_copy_runs_once() {
        let mut e = setup(2, false);
        let p0_start = e.state.player(PlayerId(0)).life;
        let p1_start = e.state.player(PlayerId(1)).life;
        drive(&mut e);
        assert_eq!(creatures_in_gy(&e, PlayerId(0)), 1, "P0 sacrificed 1 creature");
        assert_eq!(creatures_in_gy(&e, PlayerId(1)), 1, "P1 sacrificed 1 creature");
        assert_eq!(e.state.player(PlayerId(0)).life, p0_start + 1, "P0 gained 1");
        assert_eq!(e.state.player(PlayerId(1)).life, p1_start - 1, "P1 lost 1");
    }
}
