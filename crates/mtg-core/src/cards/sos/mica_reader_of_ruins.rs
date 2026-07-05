//! Mica, Reader of Ruins — `{3}{R}` Legendary Creature — Human Artificer 4/4.
//!
//! Oracle: "Ward—Pay 3 life. (Whenever this creature becomes the target of a spell or ability an
//! opponent controls, counter it unless that player pays 3 life.)
//! Whenever you cast an instant or sorcery spell, you may sacrifice an artifact. If you do, copy that
//! spell and you may choose new targets for the copy."
//!
//! **Fully implemented** — a spell-copy consumer built entirely from existing caps (mirrors
//! [`super::silverquill_the_disputant`], the Casualty dragon): the copy clause is the very same
//! `Triggered{ SpellCast(instant|sorcery) } → Optional{ IfYouDo{ Sacrifice(an artifact),
//! CopySpellOnStack{ Triggering, count: 1, new targets } } }`, only the cost is "sacrifice an artifact"
//! (any artifact, no power gate) instead of Casualty's power-≥1 creature. Ward—Pay 3 life is the shared
//! [`crate::cards::helpers::ward_pay_life`] `BecomesTargeted` trigger (the same one Prismari uses at 5).
//!
//! ⚠️ Same timing caveat as Silverquill: real "as you cast" copy triggers would sacrifice during the
//! cast; this cast-*trigger* model sacrifices a beat later, above the still-on-stack spell — observably
//! identical (the copy resolves before the original).

use crate::basics::{CardType, Color, Zone};
use crate::cards::helpers::{instant_or_sorcery, ward_pay_life};
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern};
use crate::effects::target::{CardFilter, SelectSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};
use crate::subtypes::{CreatureType, Supertype};

/// grp id (per-set ids live near their cards).
pub const MICA_READER_OF_RUINS: u32 = 430;

/// "you may sacrifice an artifact. If you do, copy that spell and you may choose new targets" — a
/// SpellCast(I/S) trigger sharing the CR 707.10 copy cap with the Casualty dragon, only the fodder is
/// any artifact.
fn copy_on_cast() -> Ability {
    Ability::Triggered {
        event: EventPattern::SpellCast(instant_or_sorcery()),
        condition: None,
        intervening_if: false,
        effect: Effect::Optional {
            prompt: "Sacrifice an artifact to copy the spell?".to_string(),
            body: Box::new(Effect::IfYouDo {
                cost: Box::new(Effect::Sacrifice {
                    who: PlayerRef::Controller,
                    what: SelectSpec {
                        zone: Zone::Battlefield,
                        filter: CardFilter::HasCardType(CardType::Artifact),
                        chooser: PlayerRef::Controller,
                        min: ValueExpr::Fixed(1),
                        max: ValueExpr::Fixed(1),
                    },
                }),
                reward: Box::new(Effect::CopySpellOnStack {
                    what: EffectTarget::Triggering,
                    count: ValueExpr::Fixed(1),
                    choose_new_targets: true,
                }),
            }),
        },
    }
}

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        MICA_READER_OF_RUINS,
        "Mica, Reader of Ruins",
        &[CreatureType::Human, CreatureType::Artificer],
        Color::Red,
        mana_cost(3, &[(Color::Red, 1)]),
        4,
        4,
        vec![ward_pay_life(3), copy_on_cast()],
    );
    def.chars.supertypes = vec![Supertype::Legendary];
    def.text = "Ward—Pay 3 life.\nWhenever you cast an instant or sorcery spell, you may sacrifice an artifact. If you do, copy that spell and you may choose new targets for the copy.".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, CastVariant, ConfirmKind, DecisionRequest, DecisionResponse, PlayerView};
    use crate::basics::{Phase, Target};
    use crate::cards::{grp, starter_db};
    use crate::ids::{ObjId, PlayerId};
    use crate::priority::Engine;
    use crate::state::GameState;
    use crate::stack::StackObjectKind;
    use std::sync::Arc;

    fn db_with_card() -> CardDb {
        let mut db = starter_db();
        register(&mut db);
        db
    }

    #[test]
    fn mica_shape() {
        let db = db_with_card();
        let def = db.get(MICA_READER_OF_RUINS).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Creature]);
        assert_eq!(def.chars.supertypes, vec![Supertype::Legendary]);
        assert_eq!(def.chars.colors, vec![Color::Red]);
        assert_eq!((def.chars.power, def.chars.toughness), (Some(4), Some(4)));
        assert_eq!(def.chars.mana_value(), 4);
        assert!(def.fully_implemented);
        // ability 0 = Ward (a BecomesTargeted trigger), ability 1 = copy-on-cast (a SpellCast trigger).
        assert!(matches!(
            def.abilities[1],
            Ability::Triggered { event: EventPattern::SpellCast(_), .. }
        ));
    }

    /// Targets P1 with the bolt, says yes to the copy offer, sacrifices the named artifact fodder, and
    /// re-aims the copy at P1 too.
    #[derive(Clone)]
    struct MicaAgent {
        confirm_copy: bool,
        fodder: ObjId,
    }
    impl Agent for MicaAgent {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::Confirm { kind: ConfirmKind::MayEffect } => {
                    DecisionResponse::Bool(self.confirm_copy)
                }
                DecisionRequest::Confirm { .. } => DecisionResponse::Bool(true),
                DecisionRequest::SelectCards { from, .. } => {
                    let idx = from.iter().position(|o| *o == self.fodder).unwrap_or(0) as u32;
                    DecisionResponse::Indices(vec![idx])
                }
                DecisionRequest::ChooseTargets { slots, .. } => {
                    let idx = slots[0]
                        .legal
                        .iter()
                        .position(|t| *t == Target::Player(PlayerId(1)))
                        .unwrap_or(0);
                    DecisionResponse::Pairs(vec![(0, idx as u32)])
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

    /// P0 with Mica out, an artifact (Treasure-less: a plain artifact token stand-in — a Signet-style
    /// permanent) to sacrifice, a Lightning Bolt + Mountain to cast it. Returns engine, bolt, fodder.
    fn setup(confirm_copy: bool) -> (Engine, ObjId, ObjId) {
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(db_with_card()));
        {
            let c = state.card_db().get(MICA_READER_OF_RUINS).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield);
        }
        // An artifact to sacrifice — reshape a Bears into a plain noncreature artifact so the sac
        // filter (any artifact) matches it (the starter db ships no artifact).
        let fodder = {
            let mut c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            c.card_types = vec![CardType::Artifact];
            c.power = None;
            c.toughness = None;
            c.subtypes.clear();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        let bolt = {
            let c = state.card_db().get(grp::LIGHTNING_BOLT).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Hand)
        };
        {
            let m = state.card_db().get(grp::MOUNTAIN).unwrap().chars.clone();
            state.add_card(PlayerId(0), m, Zone::Battlefield);
        }
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let e = Engine::new(
            state,
            vec![
                Box::new(MicaAgent { confirm_copy, fodder }),
                Box::new(MicaAgent { confirm_copy, fodder }),
            ],
        );
        (e, bolt, fodder)
    }

    /// Copy accepted: cast a bolt at P1, sacrifice the artifact, spell copied exactly ONCE — P1 takes 6.
    #[test]
    fn sacrificing_artifact_copies_once() {
        let (mut e, bolt, fodder) = setup(true);
        let p1_start = e.state.player(PlayerId(1)).life;
        e.cast_spell(PlayerId(0), bolt, CastVariant::Normal);
        drive(&mut e);
        assert_eq!(e.state.player(PlayerId(1)).life, p1_start - 6, "bolt + one copy = 6");
        assert!(e.state.player(PlayerId(0)).graveyard.contains(&fodder), "the artifact was sacrificed");
        assert!(e.state.player(PlayerId(0)).graveyard.contains(&bolt), "the real bolt is in gy");
        assert_eq!(e.state.player(PlayerId(0)).graveyard.len(), 2, "artifact + real bolt only (no copy)");
    }

    /// Copy declined: no sacrifice, no copy — P1 takes just the bolt's 3, the artifact survives.
    #[test]
    fn declining_makes_no_copy() {
        let (mut e, bolt, fodder) = setup(false);
        let p1_start = e.state.player(PlayerId(1)).life;
        e.cast_spell(PlayerId(0), bolt, CastVariant::Normal);
        drive(&mut e);
        assert_eq!(e.state.player(PlayerId(1)).life, p1_start - 3, "declined: just the bolt, no copy");
        assert!(e.state.player(PlayerId(0)).battlefield.contains(&fodder), "the artifact was not sacrificed");
        assert!(
            !e.state.stack.items.iter().any(|s| matches!(s.kind, StackObjectKind::Spell(_))),
            "no copy left on the stack"
        );
    }
}
