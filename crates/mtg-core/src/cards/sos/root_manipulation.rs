//! Root Manipulation — `{3}{B}{G}` Sorcery (first printed SOS).
//!
//! Oracle: "Until end of turn, creatures you control get +2/+2 and gain menace and 'Whenever this
//! creature attacks, you gain 1 life.'"
//!
//! **Fully implemented** — the second consumer of the **grant-a-triggered-ability-until-EOT** cap
//! (CR 613.1f). A `ForEach` over the creatures you control gives each a `+2/+2` `PumpPT`, `menace`
//! (`GrantKeyword`), and a granted attack-trigger via `Effect::GrantAbility` pointing at the reserved
//! template [`grp::GRANT_ATTACKS_GAIN_LIFE`] ("Whenever this creature attacks, you gain 1 life"). All
//! three are "until end of turn" continuous effects over the fixed set (CR 611.2c), so a creature that
//! attacks after the turn ends gains no life. Multicolored (B/G).

use crate::basics::{CardType, Color, Zone};
use crate::cards::{grp, mana_cost, spell, CardDb};
use crate::effects::ability::Keyword;
use crate::effects::condition::Duration;
use crate::effects::target::{CardFilter, SelectSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};

/// grp id (per-set ids live near their cards).
pub const ROOT_MANIPULATION: u32 = 421;

pub fn register(db: &mut CardDb) {
    let effect = Effect::ForEach {
        // "creatures you control" — the `Controller` chooser scopes the set to your battlefield.
        selector: SelectSpec {
            zone: Zone::Battlefield,
            filter: CardFilter::HasCardType(CardType::Creature),
            chooser: PlayerRef::Controller,
            min: ValueExpr::Fixed(0),
            max: ValueExpr::Fixed(999),
        },
        body: Box::new(Effect::Sequence(vec![
            Effect::PumpPT {
                what: EffectTarget::Each,
                power: ValueExpr::Fixed(2),
                toughness: ValueExpr::Fixed(2),
                duration: Duration::UntilEndOfTurn,
            },
            Effect::GrantKeyword {
                what: EffectTarget::Each,
                keyword: Keyword::Menace,
                duration: Duration::UntilEndOfTurn,
            },
            Effect::GrantAbility {
                what: EffectTarget::Each,
                template_grp: grp::GRANT_ATTACKS_GAIN_LIFE,
                duration: Duration::UntilEndOfTurn,
            },
        ])),
    };
    let mut def = spell(
        ROOT_MANIPULATION,
        "Root Manipulation",
        CardType::Sorcery,
        Color::Black,
        mana_cost(3, &[(Color::Black, 1), (Color::Green, 1)]),
        effect,
    )
    .with_text("Until end of turn, creatures you control get +2/+2 and gain menace and \"Whenever this creature attacks, you gain 1 life.\"");
    def.chars.colors = vec![Color::Black, Color::Green];
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{CastVariant, RandomAgent};
    use crate::basics::Phase;
    use crate::cards::{build_game, grp};
    use crate::ids::{ObjId, PlayerId};
    use crate::priority::Engine;

    /// P0 with Root Manipulation + {3}{B}{G} and a 2/2 Grizzly Bears. Returns `(engine, root, bears)`.
    fn setup() -> (Engine, ObjId, ObjId) {
        let mut state = build_game(1, &[&[], &[]]);
        let root = state.add_card(
            PlayerId(0),
            state.card_db().get(ROOT_MANIPULATION).unwrap().chars.clone(),
            Zone::Hand,
        );
        for grp_id in [grp::SWAMP, grp::SWAMP, grp::SWAMP, grp::FOREST, grp::FOREST] {
            let c = state.card_db().get(grp_id).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield);
        }
        let bears = {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
        (e, root, bears)
    }

    /// Settle: drain any queued triggers onto the stack and resolve them.
    fn settle(e: &mut Engine) {
        e.run_agenda();
        while !e.state.stack.is_empty() {
            e.resolve_top();
            e.run_agenda();
        }
    }

    /// Grant → +2/+2 and menace applied, then attacking fires the granted "gain 1 life".
    #[test]
    fn granted_attack_trigger_gains_life() {
        let (mut e, root, bears) = setup();
        e.cast_spell(PlayerId(0), root, CastVariant::Normal);
        e.resolve_top();
        assert_eq!(e.state.computed(bears).power, Some(4), "+2/+2 → 4 power");
        assert_eq!(e.state.computed(bears).toughness, Some(4), "+2/+2 → 4 toughness");
        assert!(e.state.computed(bears).has_keyword(Keyword::Menace), "gained menace");

        let life_before = e.state.player(PlayerId(0)).life;
        e.declare_attackers_explicit(&[bears]);
        settle(&mut e);
        assert_eq!(
            e.state.player(PlayerId(0)).life,
            life_before + 1,
            "the granted 'whenever this attacks, gain 1 life' fired"
        );
    }

    /// Until end of turn: after the grant expires, attacking gains no life.
    #[test]
    fn granted_attack_trigger_expires_at_end_of_turn() {
        let (mut e, root, bears) = setup();
        e.cast_spell(PlayerId(0), root, CastVariant::Normal);
        e.resolve_top();
        e.state.end_of_turn_continuous_cleanup();
        e.state.mark_chars_dirty();
        assert_eq!(e.state.computed(bears).power, Some(2), "back to 2 power after EOT");
        assert!(!e.state.computed(bears).has_keyword(Keyword::Menace), "menace gone after EOT");

        // Untap so it can attack again next turn.
        e.state.objects.get_mut(&bears).unwrap().status.tapped = false;
        let life_before = e.state.player(PlayerId(0)).life;
        e.declare_attackers_explicit(&[bears]);
        settle(&mut e);
        assert_eq!(e.state.player(PlayerId(0)).life, life_before, "post-EOT attack gains no life");
    }
}
