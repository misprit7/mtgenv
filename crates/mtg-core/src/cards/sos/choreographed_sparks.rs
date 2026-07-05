//! Choreographed Sparks — `{R}{R}` Instant.
//!
//! Oracle: "This spell can't be copied.
//! Choose one or both —
//! • Copy target instant or sorcery spell you control. You may choose new targets for the copy.
//! • Copy target creature spell you control. The copy gains haste and 'At the beginning of the end
//!   step, sacrifice this token.'"
//!
//! **Fully implemented** — a `Modal{ choose one or both }` over the CR 707.10 spell-copy engine, plus
//! a self-static painting [`Qualification::CantBeCopied`] on the stack.
//! - **Mode 1** = [`Effect::CopySpellOnStack`]'s `Target` arm (the storm/casualty engine's copy of a
//!   *chosen* stack spell), targeting an instant or sorcery spell you control, with the 707.10c "you may
//!   choose new targets" reselection. This is the first card to exercise the Target arm (it was wired for
//!   the storm cycle but previously only reached via `Triggering`); making it castable needed the
//!   `CopySpellOnStack` target-spec added to `collect_specs_into`.
//! - **Mode 2** = [`Effect::CopySpellAsToken`] over the same engine: copies a creature spell you control;
//!   the copy resolves into a **token** (CR 707.10f — a copy of a permanent spell isn't a card), is granted
//!   haste, and is armed to sacrifice itself at the next end step (the warp-style delayed trigger rail).
//! - **"This spell can't be copied"** = a self-static (`Zone::Stack` / `ItSelf`) painting `CantBeCopied`,
//!   read at the single `copy_spell_on_stack` choke point — the same shape as Surrak's "can't be countered".

use crate::basics::{CardType, Color, Zone};
use crate::cards::helpers::instant_or_sorcery;
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::ability::{Ability, Qualification, StaticContribution};
use crate::effects::condition::Duration;
use crate::effects::target::{CardFilter, SelectSpec, TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget, Mode};

/// grp id (per-set ids live near their cards).
pub const CHOREOGRAPHED_SPARKS: u32 = 453;

fn one_target(kind: TargetKind) -> EffectTarget {
    EffectTarget::Target(TargetSpec { kind, min: 1, max: 1, distinct: true })
}

/// "This spell can't be copied." — a self-static (CR 604.5 / 113.6f) painting `CantBeCopied` on the
/// spell while it's on the stack; `copy_spell_on_stack` skips any spell carrying it.
fn cant_be_copied() -> Ability {
    Ability::Static {
        contribution: StaticContribution::Qualification(Qualification::CantBeCopied),
        affects: SelectSpec {
            zone: Zone::Stack,
            filter: CardFilter::ItSelf,
            chooser: PlayerRef::Controller,
            min: ValueExpr::Fixed(0),
            max: ValueExpr::Fixed(0),
        },
        duration: Duration::WhileSourcePresent,
    }
}

pub fn register(db: &mut CardDb) {
    // "instant or sorcery spell you control" / "creature spell you control" — stack targets filtered by
    // type + control (a spell on the stack is a card object controlled by its caster).
    let is_you_control = CardFilter::All(vec![
        instant_or_sorcery(),
        CardFilter::ControlledBy(PlayerRef::Controller),
    ]);
    let creature_you_control = CardFilter::All(vec![
        CardFilter::HasCardType(CardType::Creature),
        CardFilter::ControlledBy(PlayerRef::Controller),
    ]);
    let effect = Effect::Modal {
        modes: vec![
            Mode {
                label: "Copy target instant or sorcery spell you control. You may choose new targets for the copy".to_string(),
                effect: Effect::CopySpellOnStack {
                    what: one_target(TargetKind::StackObject(is_you_control)),
                    count: ValueExpr::Fixed(1),
                    choose_new_targets: true,
                },
            },
            Mode {
                label: "Copy target creature spell you control. The copy gains haste and \"At the beginning of the end step, sacrifice this token.\"".to_string(),
                effect: Effect::CopySpellAsToken {
                    what: one_target(TargetKind::StackObject(creature_you_control)),
                    haste: true,
                    sacrifice_at_next_end_step: true,
                },
            },
        ],
        min: 1,
        max: 2,
        allow_repeat: false,
    };
    let mut def = spell(
        CHOREOGRAPHED_SPARKS,
        "Choreographed Sparks",
        CardType::Instant,
        Color::Red,
        mana_cost(0, &[(Color::Red, 2)]),
        effect,
    )
    .with_text(
        "This spell can't be copied.\nChoose one or both —\n• Copy target instant or sorcery spell you control. You may choose new targets for the copy.\n• Copy target creature spell you control. The copy gains haste and \"At the beginning of the end step, sacrifice this token.\"",
    );
    def.abilities.push(cant_be_copied());
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, CastVariant, DecisionRequest, DecisionResponse, PlayerView};
    use crate::basics::{Phase, Target};
    use crate::cards::{grp, starter_db};
    use crate::effects::ability::Keyword;
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
    fn choreographed_sparks_shape() {
        let db = db_with_card();
        let def = db.get(CHOREOGRAPHED_SPARKS).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Instant]);
        assert_eq!(def.chars.colors, vec![Color::Red]);
        assert_eq!(def.chars.mana_value(), 2);
        assert!(def.fully_implemented);
        // ability 0 = the spell effect (modal), ability 1 = the can't-be-copied self-static.
        let Some(Effect::Modal { modes, min, max, .. }) = def.spell_effect() else { panic!("modal") };
        assert_eq!((modes.len(), *min, *max), (2, 1, 2), "choose one or both");
        assert!(matches!(modes[0].effect, Effect::CopySpellOnStack { choose_new_targets: true, .. }));
        assert!(matches!(
            modes[1].effect,
            Effect::CopySpellAsToken { haste: true, sacrifice_at_next_end_step: true, .. }
        ));
        assert!(matches!(
            def.abilities[1],
            Ability::Static {
                contribution: StaticContribution::Qualification(Qualification::CantBeCopied),
                ..
            }
        ));
    }

    /// Chooses `modes`, and at every `ChooseTargets` prefers P1 (for "any target" bolt/copy slots),
    /// else the first stack spell (for the CS copy target), else index 0.
    struct SparkAgent {
        modes: Vec<u32>,
    }
    impl Agent for SparkAgent {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::ChooseModes { .. } => DecisionResponse::Indices(self.modes.clone()),
                DecisionRequest::ChooseTargets { slots, .. } => {
                    let legal = &slots[0].legal;
                    let idx = legal
                        .iter()
                        .position(|t| *t == Target::Player(PlayerId(1)))
                        .or_else(|| legal.iter().position(|t| matches!(t, Target::Stack(_))))
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

    /// A game where P0 has Choreographed Sparks in hand + plenty of untapped lands.
    fn setup(modes: Vec<u32>) -> Engine {
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(db_with_card()));
        for _ in 0..4 {
            let m = state.card_db().get(grp::MOUNTAIN).unwrap().chars.clone();
            state.add_card(PlayerId(0), m, Zone::Battlefield);
        }
        let f = state.card_db().get(grp::FOREST).unwrap().chars.clone();
        state.add_card(PlayerId(0), f, Zone::Battlefield);
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        Engine::new(
            state,
            vec![Box::new(SparkAgent { modes: modes.clone() }), Box::new(SparkAgent { modes })],
        )
    }

    /// Mode 1: copy a Lightning Bolt you control (the first card to exercise the `CopySpellOnStack`
    /// `Target` arm). The copy resolves first and is re-aimed at P1, then the real bolt resolves → 6.
    #[test]
    fn mode1_copies_your_instant() {
        let mut e = setup(vec![0]);
        let bolt = {
            let c = e.state.card_db().get(grp::LIGHTNING_BOLT).unwrap().chars.clone();
            e.state.add_card(PlayerId(0), c, Zone::Hand)
        };
        let cs = {
            let c = e.state.card_db().get(CHOREOGRAPHED_SPARKS).unwrap().chars.clone();
            e.state.add_card(PlayerId(0), c, Zone::Hand)
        };
        let p1_start = e.state.player(PlayerId(1)).life;
        // Cast the bolt at P1, then Choreographed Sparks in response, copying the bolt.
        e.cast_spell(PlayerId(0), bolt, CastVariant::Normal);
        e.cast_spell(PlayerId(0), cs, CastVariant::Normal);
        drive(&mut e);
        assert_eq!(e.state.player(PlayerId(1)).life, p1_start - 6, "bolt (3) + one copy (3) = 6");
        assert!(e.state.player(PlayerId(0)).graveyard.contains(&bolt), "the real bolt is in gy");
        // The copy ceased to exist (never reached a graveyard); only bolt + CS are there.
        assert!(e.state.player(PlayerId(0)).graveyard.contains(&cs), "CS in gy");
        assert_eq!(e.state.player(PlayerId(0)).graveyard.len(), 2, "bolt + CS only — the copy vanished");
    }

    /// Mode 2: copy a creature spell you control → a token that enters with haste and is sacrificed at
    /// the next end step; the real creature stays.
    #[test]
    fn mode2_copies_your_creature_as_hasty_token_sacrificed_at_end_step() {
        let mut e = setup(vec![1]);
        let bears = {
            let c = e.state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            e.state.add_card(PlayerId(0), c, Zone::Hand)
        };
        let cs = {
            let c = e.state.card_db().get(CHOREOGRAPHED_SPARKS).unwrap().chars.clone();
            e.state.add_card(PlayerId(0), c, Zone::Hand)
        };
        e.cast_spell(PlayerId(0), bears, CastVariant::Normal);
        e.cast_spell(PlayerId(0), cs, CastVariant::Normal);
        drive(&mut e);
        // Two Grizzly Bears on the battlefield: the real one + the token copy.
        let bears_bf: Vec<_> = e
            .state
            .player(PlayerId(0))
            .battlefield
            .iter()
            .copied()
            .filter(|&o| e.state.object(o).chars.name == "Grizzly Bears")
            .collect();
        assert_eq!(bears_bf.len(), 2, "real Bears + token copy on the battlefield");
        // Exactly one of them (the copy) has haste.
        let hasty: Vec<_> = bears_bf
            .iter()
            .copied()
            .filter(|&o| e.state.computed(o).has_keyword(Keyword::Haste))
            .collect();
        assert_eq!(hasty.len(), 1, "the token copy gained haste");
        let token = hasty[0];
        assert_ne!(token, bears, "the hasty one is the copy, not the real Bears");

        // At the next end step, the armed delayed trigger sacrifices the token; the real Bears stays.
        e.fire_end_step_delayed_triggers();
        drive(&mut e);
        assert!(!e.state.player(PlayerId(0)).battlefield.contains(&token), "token sacrificed at end step");
        assert!(e.state.player(PlayerId(0)).battlefield.contains(&bears), "the real Bears survives");
    }

    /// "This spell can't be copied": the `CantBeCopied` qualification is painted on the stack, and the
    /// copy engine refuses to mint a copy of it (a storm/Mica-style copy attempt is a no-op).
    #[test]
    fn cant_be_copied() {
        let mut e = setup(vec![0]);
        // A bolt on the stack makes mode 1 legal so CS can be cast; CS then sits on the stack above it.
        let bolt = {
            let c = e.state.card_db().get(grp::LIGHTNING_BOLT).unwrap().chars.clone();
            e.state.add_card(PlayerId(0), c, Zone::Hand)
        };
        let cs = {
            let c = e.state.card_db().get(CHOREOGRAPHED_SPARKS).unwrap().chars.clone();
            e.state.add_card(PlayerId(0), c, Zone::Hand)
        };
        e.cast_spell(PlayerId(0), bolt, CastVariant::Normal);
        e.cast_spell(PlayerId(0), cs, CastVariant::Normal);
        assert!(
            e.state.computed(cs).has_qualification(Qualification::CantBeCopied),
            "the self-static paints CantBeCopied while it's on the stack"
        );
        let before = e.state.stack.items.len();
        let made = e.copy_spell_on_stack(cs, PlayerId(0), false);
        assert!(made.is_none(), "copy_spell_on_stack refuses a can't-be-copied spell");
        assert_eq!(e.state.stack.items.len(), before, "no copy was minted onto the stack");
    }
}
