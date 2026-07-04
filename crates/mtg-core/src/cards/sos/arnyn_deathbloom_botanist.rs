//! Arnyn, Deathbloom Botanist — `{2}{B}` Legendary Creature — Vampire Druid 2/2 (first printed SOS).
//!
//! Oracle: "Deathtouch. Whenever a creature you control with power or toughness 1 or less dies,
//! target opponent loses 2 life and you gain 2 life."
//!
//! **Fully implemented** — a 2/2 deathtouch body plus a **last-known-information dies-trigger** (CR
//! 603.10a): `EventPattern::CreatureDies(All([ControlledBy(Controller), AnyOf([PowerAtMost(1),
//! ToughnessAtMost(1)])]))`. The filter is evaluated against the dying creature's LKI (its computed
//! P/T + controller as it last existed on the battlefield), since by the time the death fires the
//! creature is in the graveyard. The effect drains a target opponent for 2 and gains 2.

use crate::basics::Color;
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern, Keyword};
use crate::effects::target::{CardFilter, PlayerFilter};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;
use crate::subtypes::{CreatureType, Supertype};

/// grp id (per-set ids live near their cards).
pub const ARNYN_DEATHBLOOM_BOTANIST: u32 = 356;

/// "a creature you control with power or toughness 1 or less."
fn small_creature_you_control() -> CardFilter {
    CardFilter::All(vec![
        CardFilter::ControlledBy(PlayerRef::Controller),
        CardFilter::AnyOf(vec![CardFilter::PowerAtMost(1), CardFilter::ToughnessAtMost(1)]),
    ])
}

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        ARNYN_DEATHBLOOM_BOTANIST,
        "Arnyn, Deathbloom Botanist",
        &[CreatureType::Vampire, CreatureType::Druid],
        Color::Black,
        mana_cost(2, &[(Color::Black, 1)]),
        2,
        2,
        vec![Ability::Triggered {
            event: EventPattern::CreatureDies(small_creature_you_control()),
            condition: None,
            intervening_if: false,
            effect: Effect::Sequence(vec![
                Effect::TargetPlayer(PlayerFilter::Opponent),
                Effect::LoseLife { who: PlayerRef::ChosenTarget(0), amount: ValueExpr::Fixed(2) },
                Effect::GainLife { who: PlayerRef::Controller, amount: ValueExpr::Fixed(2) },
            ]),
        }],
    )
    .with_text(
        "Deathtouch\nWhenever a creature you control with power or toughness 1 or less dies, target opponent loses 2 life and you gain 2 life.",
    );
    def.chars.keywords = vec![Keyword::Deathtouch];
    def.chars.supertypes = vec![Supertype::Legendary];
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::RandomAgent;
    use crate::basics::{DamageKind, Zone};
    use crate::cards::{build_game, grp};
    use crate::effects::action::{ResolutionCtx, WbReason};
    use crate::ids::{PlayerId, StackId};
    use crate::priority::Engine;

    #[test]
    fn arnyn_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(ARNYN_DEATHBLOOM_BOTANIST).unwrap();
        assert!(def.fully_implemented);
        assert!(def.chars.keywords.contains(&Keyword::Deathtouch));
        assert!(def.chars.supertypes.contains(&Supertype::Legendary));
        assert!(matches!(
            &def.abilities[0],
            Ability::Triggered { event: EventPattern::CreatureDies(_), .. }
        ));
    }

    /// Real-path LKI: a 1/1 token you control dies (lethal damage → SBA). Arnyn's dies-trigger reads
    /// the token's last-known P/T (1/1 ≤ 1) + controller (you), fires, and drains the opponent 2 /
    /// gains you 2. Proves the filter matched against LKI even though the token is in the graveyard.
    #[test]
    fn drains_when_small_creature_you_control_dies() {
        let mut state = build_game(1, &[&[], &[]]);
        let _arnyn = state.add_card(
            PlayerId(0),
            state.card_db().get(ARNYN_DEATHBLOOM_BOTANIST).unwrap().chars.clone(),
            Zone::Battlefield,
        );
        // A 1/1 Grizzly-Bears-like victim: use a real 1/1. Bear is 2/2; instead build a 1/1 via a
        // token-sized creature — the Inkling token def is 1/1.
        let small = {
            let mut c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            c.power = Some(1);
            c.toughness = Some(1);
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        state.active_player = PlayerId(0);
        let mut e = Engine::new(
            state,
            vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))],
        );
        let (opp_life, my_life) = (e.state.player(PlayerId(1)).life, e.state.player(PlayerId(0)).life);
        // Deal 1 lethal damage to the 1/1 through the whiteboard, then run SBAs + the trigger.
        let effect = Effect::DealDamage {
            amount: ValueExpr::Fixed(1),
            to: crate::effects::EffectTarget::ChosenIndex(0),
            kind: DamageKind::Noncombat,
        };
        e.resolve_effect(
            &effect,
            &ResolutionCtx {
                controller: Some(PlayerId(0)),
                chosen_targets: vec![crate::basics::Target::Object(small)],
                ..Default::default()
            },
            WbReason::Resolve(StackId(0)),
        );
        // SBA kills the 1/1 (→ PermanentDied → queues Arnyn's dies-trigger), then run_agenda puts
        // the trigger on the stack (targeting the sole opponent); resolve_top resolves it.
        e.run_agenda();
        assert!(!e.state.player(PlayerId(0)).battlefield.contains(&small), "the 1/1 died");
        e.resolve_top();
        assert_eq!(e.state.player(PlayerId(1)).life, opp_life - 2, "opponent lost 2");
        assert_eq!(e.state.player(PlayerId(0)).life, my_life + 2, "you gained 2");
    }
}
