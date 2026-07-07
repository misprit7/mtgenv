//! Akroma's Will — `{3}{W}` Instant (first printed CMR; reprinted on the SOS Mystical Archive `soa`).
//!
//! Oracle: "Choose one. If you control a commander as you cast this spell, you may choose both instead.
//! • Creatures you control gain flying, vigilance, and double strike until end of turn.
//! • Creatures you control gain lifelink, indestructible, and protection from each color until end of turn."
//!
//! **Fully implemented** — a `Modal` "choose one" (the "choose both if you control a commander" clause is
//! a pool-scoped omission: there are no commanders in a limited environment). Each mode is a `ForEach`
//! over the creatures you control granting a `Becomes { … }` until end of turn (the Triumph idiom):
//! - mode 1 grants Flying / Vigilance / Double Strike (pure keyword grants);
//! - mode 2 grants Lifelink / Indestructible + **protection from each colour** (five
//!   `StaticContribution::ProtectionFromColor` grants — the protection-from-colour subsystem: no
//!   targeting by / damage from / blocking by a source of any colour).

use crate::basics::{CardType, Color};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::ability::{Keyword, StaticContribution};
use crate::effects::condition::Duration;
use crate::effects::target::{CardFilter, SelectSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget, Mode};

/// grp id (bonus-sheet `soa` cards use the 600+ block).
pub const AKROMAS_WILL: u32 = 659;

/// "Creatures you control gain `contributions` until end of turn" — a `ForEach` over your creatures
/// granting each a `Becomes`.
fn creatures_you_control_gain(contributions: Vec<StaticContribution>) -> Effect {
    Effect::ForEach {
        selector: SelectSpec {
            zone: crate::basics::Zone::Battlefield,
            filter: CardFilter::HasCardType(CardType::Creature),
            chooser: PlayerRef::Controller,
            min: ValueExpr::Fixed(0),
            max: ValueExpr::Fixed(999),
        },
        body: Box::new(Effect::Becomes {
            what: EffectTarget::Each,
            contributions,
            base_pt: None,
            duration: Duration::UntilEndOfTurn,
        }),
    }
}

pub fn register(db: &mut CardDb) {
    let mode_evasion = creatures_you_control_gain(vec![
        StaticContribution::GrantKeyword(Keyword::Flying),
        StaticContribution::GrantKeyword(Keyword::Vigilance),
        StaticContribution::GrantKeyword(Keyword::DoubleStrike),
    ]);
    let mode_protection = creatures_you_control_gain(vec![
        StaticContribution::GrantKeyword(Keyword::Lifelink),
        StaticContribution::GrantKeyword(Keyword::Indestructible),
        StaticContribution::ProtectionFromColor(Color::White),
        StaticContribution::ProtectionFromColor(Color::Blue),
        StaticContribution::ProtectionFromColor(Color::Black),
        StaticContribution::ProtectionFromColor(Color::Red),
        StaticContribution::ProtectionFromColor(Color::Green),
    ]);
    let effect = Effect::Modal {
        modes: vec![
            Mode {
                label: "Creatures you control gain flying, vigilance, and double strike until end of turn".to_string(),
                effect: mode_evasion,
            },
            Mode {
                label: "Creatures you control gain lifelink, indestructible, and protection from each color until end of turn".to_string(),
                effect: mode_protection,
            },
        ],
        min: 1,
        max: 1,
        allow_repeat: false,
    };
    let def = spell(
        AKROMAS_WILL,
        "Akroma's Will",
        CardType::Instant,
        Color::White,
        mana_cost(3, &[(Color::White, 1)]),
        effect,
    )
    .with_text(
        "Choose one. If you control a commander as you cast this spell, you may choose both instead.\n• Creatures you control gain flying, vigilance, and double strike until end of turn.\n• Creatures you control gain lifelink, indestructible, and protection from each color until end of turn.",
    );
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView};
    use crate::basics::{DamageKind, Target, Zone};
    use crate::cards::{build_game, grp};
    use crate::effects::action::{ResolutionCtx, WbReason};
    use crate::ids::{PlayerId, StackId};
    use crate::priority::Engine;
    use crate::state::Characteristics;

    #[derive(Clone)]
    struct Passive;
    impl Agent for Passive {
        fn decide(&mut self, _v: &PlayerView, _r: &DecisionRequest) -> DecisionResponse {
            DecisionResponse::Pass
        }
    }

    fn red_creature() -> Characteristics {
        Characteristics {
            name: "Red Ogre".to_string(),
            card_types: vec![CardType::Creature],
            colors: vec![Color::Red],
            power: Some(3),
            toughness: Some(3),
            grp_id: 8300,
            ..Default::default()
        }
    }

    fn resolve_mode(mode: u32) -> (Engine, crate::ids::ObjId, crate::ids::ObjId) {
        let mut state = build_game(1, &[&[], &[]]);
        let bear = state.add_card(PlayerId(0), state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Battlefield);
        let ogre = state.add_card(PlayerId(1), red_creature(), Zone::Battlefield); // opponent's red source
        let effect = state.card_db().get(AKROMAS_WILL).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(Passive), Box::new(Passive)]);
        e.resolve_effect(
            &effect,
            &ResolutionCtx { controller: Some(PlayerId(0)), chosen_modes: vec![mode], ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        (e, bear, ogre)
    }

    #[test]
    fn shape_is_modal_choose_one() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(AKROMAS_WILL).unwrap();
        assert!(def.fully_implemented);
        match def.spell_effect().unwrap() {
            Effect::Modal { modes, min, max, .. } => assert_eq!((modes.len(), *min, *max), (2, 1, 1)),
            o => panic!("expected Modal, got {o:?}"),
        }
    }

    #[test]
    fn mode1_grants_flying_vigilance_double_strike() {
        let (e, bear, _ogre) = resolve_mode(0);
        let cc = e.state.computed(bear);
        assert!(cc.has_keyword(Keyword::Flying), "flying");
        assert!(cc.has_keyword(Keyword::Vigilance), "vigilance");
        assert!(cc.has_keyword(Keyword::DoubleStrike), "double strike");
        assert!(cc.protection_from.is_empty(), "mode 1 grants no protection");
    }

    #[test]
    fn mode2_grants_protection_from_each_color_lifelink_indestructible() {
        let (e, bear, _ogre) = resolve_mode(1);
        let cc = e.state.computed(bear);
        assert!(cc.has_keyword(Keyword::Lifelink), "lifelink");
        assert!(cc.has_keyword(Keyword::Indestructible), "indestructible");
        for color in [Color::White, Color::Blue, Color::Black, Color::Red, Color::Green] {
            assert!(cc.protection_from.contains(&color), "protection from {color:?}");
        }
    }

    /// The protection-from-colour damage seam: a red source's damage to a protected creature is
    /// prevented (CR 702.16d) — no marked damage.
    #[test]
    fn mode2_prevents_damage_from_a_coloured_source() {
        let (mut e, bear, ogre) = resolve_mode(1);
        e.apply_damage(Target::Object(bear), 3, ogre, DamageKind::Combat);
        assert_eq!(e.state.object(bear).damage_marked, 0, "damage from red source prevented");
    }
}
