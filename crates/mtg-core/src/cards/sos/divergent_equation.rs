//! Divergent Equation — `{X}{X}{U}` Instant (first printed SOS).
//!
//! Oracle: "Return up to X target instant and/or sorcery cards from your graveyard to your hand.
//! Exile Divergent Equation."
//!
//! **Fully implemented** — the lander for a **dynamic {X} target COUNT**: the return slot's `max` is
//! the [`TARGET_COUNT_X`] sentinel, resolved to the chosen `{X}` when the cast builds its target slots
//! (CR 601.2b/c — X is announced before targets). Otherwise a plain multi-target `Effect::MoveZone`
//! (like Pull from the Grave), scoped to instant/sorcery cards in the caster's graveyard, followed by
//! the `Ability::ExileOnResolve` marker (Divergent Equation exiles itself instead of going to the
//! graveyard, via `resolve_top`).

use crate::basics::{CardType, Color, Zone, ZoneDest, ZonePos};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::ability::Ability;
use crate::effects::target::{CardFilter, TargetKind, TargetSpec, TARGET_COUNT_X};
use crate::effects::value::PlayerRef;
use crate::effects::{Effect, EffectTarget};

/// grp id (per-set ids live near their cards).
pub const DIVERGENT_EQUATION: u32 = 448;

pub fn register(db: &mut CardDb) {
    // "up to X target instant and/or sorcery cards from your graveyard" — max = the chosen {X}.
    let effect = Effect::MoveZone {
        what: EffectTarget::Target(TargetSpec {
            kind: TargetKind::CardInZone {
                zone: Zone::Graveyard,
                filter: CardFilter::All(vec![
                    CardFilter::AnyOf(vec![
                        CardFilter::HasCardType(CardType::Instant),
                        CardFilter::HasCardType(CardType::Sorcery),
                    ]),
                    CardFilter::ControlledBy(PlayerRef::Controller),
                ]),
            },
            min: 0,
            max: TARGET_COUNT_X,
            distinct: true,
        }),
        to: ZoneDest { zone: Zone::Hand, pos: ZonePos::Any },
        tapped: false,
    };
    let mut mc = mana_cost(0, &[(Color::Blue, 1)]);
    mc.x = 2; // `{X}{X}{U}` — two `{X}` pips (the target count is the single chosen X value).
    let mut def = spell(
        DIVERGENT_EQUATION,
        "Divergent Equation",
        CardType::Instant,
        Color::Blue,
        mc,
        effect,
    )
    .with_text("Return up to X target instant and/or sorcery cards from your graveyard to your hand.\nExile Divergent Equation.");
    // "Exile Divergent Equation." — leaves the stack to exile, not the graveyard.
    def.abilities.push(Ability::ExileOnResolve);
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, CastVariant, DecisionRequest, DecisionResponse, PlayerView};
    use crate::basics::{Phase, Target, Zone};
    use crate::cards::{build_game, grp};
    use crate::ids::{ObjId, PlayerId};
    use crate::priority::Engine;

    #[test]
    fn divergent_equation_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(DIVERGENT_EQUATION).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Instant]);
        assert_eq!(def.chars.mana_cost.as_ref().unwrap().x, 2, "two {{X}} pips");
        assert!(def.abilities.iter().any(|a| matches!(a, Ability::ExileOnResolve)));
        // The return slot's max is the dynamic-X sentinel.
        match def.spell_effect().unwrap() {
            Effect::MoveZone { what: EffectTarget::Target(spec), .. } => {
                assert_eq!(spec.max, TARGET_COUNT_X, "up-to-X target count");
                assert_eq!(spec.min, 0, "up to X (optional)");
            }
            other => panic!("expected MoveZone, got {other:?}"),
        }
    }

    /// Chooses `pick` cards (in order) for the return slot; answers X with `x`. Passes otherwise.
    struct EqAgent {
        x: i64,
        pick: Vec<ObjId>,
    }
    impl Agent for EqAgent {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::ChooseNumber { .. } => DecisionResponse::Number(self.x),
                DecisionRequest::ChooseTargets { slots, .. } => {
                    let mut pairs = Vec::new();
                    for want in &self.pick {
                        if let Some(j) = slots[0].legal.iter().position(|t| matches!(t, Target::Object(o) if o == want)) {
                            pairs.push((0u32, j as u32));
                        }
                    }
                    DecisionResponse::Pairs(pairs)
                }
                _ => DecisionResponse::Pass,
            }
        }
    }

    fn add(state: &mut crate::state::GameState, who: PlayerId, grp_id: u32, zone: Zone) -> ObjId {
        let c = state.card_db().get(grp_id).unwrap().chars.clone();
        state.add_card(who, c, zone)
    }

    /// Cast for X = 2 returning two of the three I/S cards in the graveyard; Divergent Equation exiles
    /// itself. The dynamic max = X gates the slot to two picks.
    #[test]
    fn returns_up_to_x_instant_sorcery_cards_and_self_exiles() {
        let mut state = build_game(1, &[&[], &[]]);
        // Three I/S cards in your graveyard, plus a creature card that must NOT be a legal target.
        let bolt_a = add(&mut state, PlayerId(0), grp::LIGHTNING_BOLT, Zone::Graveyard);
        let bolt_b = add(&mut state, PlayerId(0), grp::LIGHTNING_BOLT, Zone::Graveyard);
        let bolt_c = add(&mut state, PlayerId(0), grp::LIGHTNING_BOLT, Zone::Graveyard);
        let creature_card = add(&mut state, PlayerId(0), grp::GRIZZLY_BEARS, Zone::Graveyard);
        let eq = add(&mut state, PlayerId(0), DIVERGENT_EQUATION, Zone::Hand);
        // {X}{X}{U} with X=2 → {2}{2}{U} = 5 mana: four Islands (pay 4 generic from 5 → need 5 total).
        for _ in 0..5 {
            add(&mut state, PlayerId(0), grp::ISLAND, Zone::Battlefield);
        }
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let agent = EqAgent { x: 2, pick: vec![bolt_a, bolt_b] };
        let mut e = Engine::new(state, vec![Box::new(EqAgent { x: 2, pick: vec![bolt_a, bolt_b] }), Box::new(agent)]);

        e.cast_spell(PlayerId(0), eq, CastVariant::Normal);
        e.resolve_top();
        e.run_agenda();

        assert_eq!(e.state.object(bolt_a).zone, Zone::Hand, "returned to hand");
        assert_eq!(e.state.object(bolt_b).zone, Zone::Hand, "returned to hand");
        assert_eq!(e.state.object(bolt_c).zone, Zone::Graveyard, "the third stayed (only X=2 returned)");
        assert_eq!(e.state.object(creature_card).zone, Zone::Graveyard, "a creature card was never a legal target");
        assert_eq!(e.state.object(eq).zone, Zone::Exile, "Divergent Equation exiled itself");
    }

    /// X = 0 is legal — no targets, and it still exiles itself (cycle-it-away line).
    #[test]
    fn x_zero_returns_nothing_and_self_exiles() {
        let mut state = build_game(1, &[&[], &[]]);
        add(&mut state, PlayerId(0), grp::LIGHTNING_BOLT, Zone::Graveyard);
        let eq = add(&mut state, PlayerId(0), DIVERGENT_EQUATION, Zone::Hand);
        add(&mut state, PlayerId(0), grp::ISLAND, Zone::Battlefield); // {0}{0}{U} = {U}
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let mut e = Engine::new(state, vec![Box::new(EqAgent { x: 0, pick: vec![] }), Box::new(EqAgent { x: 0, pick: vec![] })]);

        e.cast_spell(PlayerId(0), eq, CastVariant::Normal);
        e.resolve_top();
        e.run_agenda();

        assert_eq!(e.state.object(eq).zone, Zone::Exile, "self-exiled even with X=0");
    }
}
