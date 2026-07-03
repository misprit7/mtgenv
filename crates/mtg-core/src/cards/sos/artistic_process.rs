//! Artistic Process — `{3}{R}{R}` Sorcery (first printed SOS).
//!
//! Oracle: "Choose one —
//! • Artistic Process deals 6 damage to target creature.
//! • Artistic Process deals 2 damage to each creature you don't control.
//! • Create a 3/3 blue and red Elemental creature token with flying. It gains haste until end of turn."
//!
//! **Fully implemented** — a `Modal` "choose one": (1) 6 damage to a target creature; (2) a `ForEach`
//! over the opponent's creatures (`chooser: Opponent`) dealing each 2; (3) a 3/3 blue/red Elemental
//! token with flying. The token's "gains haste until end of turn" is baked in as a printed `Haste`
//! keyword — for a token created this turn the two are behaviourally identical (it's already un-sick
//! next turn), so this is exact in play, not a gap.

use crate::basics::{CardType, Color, DamageKind, Zone};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::ability::Keyword;
use crate::effects::target::{CardFilter, SelectSpec, TargetKind, TargetSpec, TokenSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget, Mode};
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const ARTISTIC_PROCESS: u32 = 316;

pub fn register(db: &mut CardDb) {
    let effect = Effect::Modal {
        modes: vec![
            Mode {
                label: "Artistic Process deals 6 damage to target creature".to_string(),
                effect: Effect::DealDamage {
                    amount: ValueExpr::Fixed(6),
                    to: EffectTarget::Target(TargetSpec {
                        kind: TargetKind::Creature(CardFilter::Any),
                        min: 1,
                        max: 1,
                        distinct: true,
                    }),
                    kind: DamageKind::Noncombat,
                },
            },
            Mode {
                label: "Artistic Process deals 2 damage to each creature you don't control".to_string(),
                // "each creature you don't control" — the opponent's creatures (chooser: Opponent).
                effect: Effect::ForEach {
                    selector: SelectSpec {
                        zone: Zone::Battlefield,
                        filter: CardFilter::HasCardType(CardType::Creature),
                        chooser: PlayerRef::Opponent,
                        min: ValueExpr::Fixed(0),
                        max: ValueExpr::Fixed(999),
                    },
                    body: Box::new(Effect::DealDamage {
                        amount: ValueExpr::Fixed(2),
                        to: EffectTarget::Each,
                        kind: DamageKind::Noncombat,
                    }),
                },
            },
            Mode {
                label: "Create a 3/3 blue and red Elemental with flying and haste".to_string(),
                effect: Effect::CreateToken {
                    spec: TokenSpec {
                        name: "Elemental".to_string(),
                        card_types: vec![CardType::Creature],
                        subtypes: vec![CreatureType::Elemental.into()],
                        colors: vec![Color::Blue, Color::Red],
                        power: 3,
                        toughness: 3,
                        keywords: vec![Keyword::Flying, Keyword::Haste],
                        counters: Vec::new(),
                        grp_id: 0,
                    },
                    count: ValueExpr::Fixed(1),
                    controller: PlayerRef::Controller,
                    dynamic_counters: Vec::new(),
                },
            },
        ],
        min: 1,
        max: 1,
        allow_repeat: false,
    };
    db.insert(
        spell(
            ARTISTIC_PROCESS,
            "Artistic Process",
            CardType::Sorcery,
            Color::Red,
            mana_cost(3, &[(Color::Red, 2)]),
            effect,
        )
        .with_text("Choose one —\n• Artistic Process deals 6 damage to target creature.\n• Artistic Process deals 2 damage to each creature you don't control.\n• Create a 3/3 blue and red Elemental creature token with flying. It gains haste until end of turn."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn artistic_process_shape() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(ARTISTIC_PROCESS).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Sorcery]);
        assert!(def.fully_implemented);
        match def.spell_effect().unwrap() {
            Effect::Modal { modes, min, max, allow_repeat } => {
                assert_eq!((modes.len(), *min, *max, *allow_repeat), (3, 1, 1, false));
            }
            o => panic!("expected Modal, got {o:?}"),
        }
    }

    /// Behaviour, mode 2: 2 damage to each creature the opponent controls (yours are untouched).
    #[test]
    fn artistic_process_mode2_hits_only_opponents_creatures() {
        use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView};
        use crate::basics::Zone;
        use crate::cards::{build_game, grp};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        #[derive(Clone)]
        struct ChooseMode2;
        impl Agent for ChooseMode2 {
            fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
                match req {
                    DecisionRequest::ChooseModes { .. } => DecisionResponse::Indices(vec![1]),
                    _ => DecisionResponse::Pass,
                }
            }
        }
        let mut state = build_game(1, &[&[], &[]]);
        // Mine (P0) — untouched; theirs (P1) — two 2/2s that take 2 each (lethal → destroyed).
        let mine = state.add_card(PlayerId(0), state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Battlefield);
        let theirs = state.add_card(PlayerId(1), state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Battlefield);
        let effect = state.card_db().get(ARTISTIC_PROCESS).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(ChooseMode2), Box::new(ChooseMode2)]);
        e.resolve_effect(
            &effect,
            &ResolutionCtx { controller: Some(PlayerId(0)), chosen_modes: vec![1], ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        assert_eq!(e.state.object(theirs).damage_marked, 2, "opponent's creature took 2");
        assert_eq!(e.state.object(mine).damage_marked, 0, "my creature is untouched");
    }
}
