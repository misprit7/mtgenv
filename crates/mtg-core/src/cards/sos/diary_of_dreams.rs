//! Diary of Dreams — `{2}` Artifact — Book (first printed SOS).
//!
//! Oracle: "Whenever you cast an instant or sorcery spell, put a page counter on this artifact.
//! {5}, {T}: Draw a card. This ability costs {1} less to activate for each page counter on this
//! artifact."
//!
//! **Fully implemented** — a `SpellCast(instant/sorcery)` → page-counter trigger plus a `{5},{T}:
//! Draw` activated ability whose cost is reduced by an **activated-ability cost reduction** (the S12
//! cost-reduction mechanism extended to CR 602): `Ability::CostReduction { scope: ActivatedAbilities,
//! amount: GenericValue(CountersOnSelf(page)) }`, applied by `effective_activation_cost` at BOTH the
//! offer gate and payment. The page counter is `CounterKind::Named("page")` (no enum churn).

use crate::basics::CounterKind;
use crate::cards::helpers::instant_or_sorcery;
use crate::cards::{artifact, mana_cost, CardDb};
use crate::effects::ability::{
    Ability, Cost, CostComponent, CostReductionAmount, CostReductionCondition, CostReductionScope,
    EventPattern, Timing,
};
use crate::effects::condition::Condition;
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};

/// grp id (per-set ids live near their cards).
pub const DIARY_OF_DREAMS: u32 = 363;

/// The card-specific "page" counter (CR 122 — a named counter; no `CounterKind` enum variant).
fn page() -> CounterKind {
    CounterKind::Named("page".to_string())
}

pub fn register(db: &mut CardDb) {
    let def = artifact(
        DIARY_OF_DREAMS,
        "Diary of Dreams",
        mana_cost(2, &[]),
        vec![
            // "Whenever you cast an instant or sorcery spell, put a page counter on this artifact."
            Ability::Triggered {
                event: EventPattern::SpellCast(instant_or_sorcery()),
                condition: None,
                intervening_if: false,
                effect: Effect::PutCounters {
                    what: EffectTarget::SourceSelf,
                    kind: page(),
                    n: ValueExpr::Fixed(1),
                },
            },
            // "{5}, {T}: Draw a card."
            Ability::Activated {
                cost: Cost {
                    mana: Some(mana_cost(5, &[])),
                    components: vec![CostComponent::TapSelf],
                },
                effect: Effect::Draw { who: PlayerRef::Controller, count: ValueExpr::Fixed(1) },
                timing: Timing::Instant,
                restriction: None,
                is_mana: false,
            },
            // "This ability costs {1} less to activate for each page counter on this artifact."
            Ability::CostReduction {
                amount: CostReductionAmount::GenericValue(ValueExpr::CountersOnSelf(page())),
                condition: CostReductionCondition::State(Condition::Always),
                scope: CostReductionScope::ActivatedAbilities,
            },
        ],
    )
    .with_text(
        "Whenever you cast an instant or sorcery spell, put a page counter on this artifact.\n{5}, {T}: Draw a card. This ability costs {1} less to activate for each page counter on this artifact.",
    );
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{PlayableAction, RandomAgent};
    use crate::basics::{Phase, Zone};
    use crate::cards::{build_game, grp};
    use crate::ids::{ObjId, PlayerId};
    use crate::priority::Engine;

    #[test]
    fn diary_of_dreams_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(DIARY_OF_DREAMS).unwrap();
        assert!(def.fully_implemented);
        let reduction = def.abilities.iter().any(|a| matches!(a, Ability::CostReduction {
            scope: CostReductionScope::ActivatedAbilities, .. }));
        assert!(reduction, "carries an activated-ability cost reduction");
    }

    /// Put Diary on the battlefield with `counters` page counters + three lands (`{3}` mana) and
    /// return whether its `{5},{T}: Draw` ability is offered — i.e. whether the reduction brought
    /// the cost within reach.
    fn offered(counters: u32) -> bool {
        let mut state = build_game(1, &[&[grp::FOREST], &[]]);
        let diary = state.add_card(
            PlayerId(0),
            state.card_db().get(DIARY_OF_DREAMS).unwrap().chars.clone(),
            Zone::Battlefield,
        );
        {
            let o = state.objects.get_mut(&diary).unwrap();
            o.summoning_sick = false;
            if counters > 0 {
                o.counters.counts.insert(page(), counters);
            }
        }
        for _ in 0..3 {
            let land = state.card_db().get(grp::ISLAND).unwrap().chars.clone();
            state.add_card(PlayerId(0), land, Zone::Battlefield);
        }
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
        e.legal_actions(PlayerId(0))
            .iter()
            .any(|a| matches!(a, PlayableAction::Activate { source, .. } if *source == diary))
    }

    /// The `{5},{T}` draw ability is unaffordable at {5} with only {3} mana, but two page counters
    /// drop it to {3} — so it's offered only once enough counters have accrued.
    #[test]
    fn activation_cost_drops_per_page_counter() {
        assert!(!offered(0), "{{5}} with only {{3}} mana → not offered");
        assert!(!offered(1), "{{4}} still unaffordable with {{3}}");
        assert!(offered(2), "two page counters → {{5}}−{{2}} = {{3}} → offered");
    }

    fn add_diary_and_mana(state: &mut crate::state::GameState, counters: u32, islands: u32) -> ObjId {
        let diary = state.add_card(
            PlayerId(0),
            state.card_db().get(DIARY_OF_DREAMS).unwrap().chars.clone(),
            Zone::Battlefield,
        );
        {
            let o = state.objects.get_mut(&diary).unwrap();
            o.summoning_sick = false;
            if counters > 0 {
                o.counters.counts.insert(page(), counters);
            }
        }
        for _ in 0..islands {
            let land = state.card_db().get(grp::ISLAND).unwrap().chars.clone();
            state.add_card(PlayerId(0), land, Zone::Battlefield);
        }
        diary
    }

    /// Real-path: activate the reduced ability and resolve it to draw a card.
    #[test]
    fn activating_the_reduced_ability_draws() {
        let mut state = build_game(1, &[&[grp::FOREST, grp::FOREST], &[]]);
        let diary = add_diary_and_mana(&mut state, 2, 3); // {3} after reduction
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
        let hand_before = e.state.player(PlayerId(0)).hand.len();
        let act = e.legal_actions(PlayerId(0)).iter().find_map(|a| match a {
            PlayableAction::Activate { source, ability } if *source == diary => Some(*ability),
            _ => None,
        });
        assert!(act.is_some(), "reduced ability offered");
        e.activate_ability(PlayerId(0), diary, act.unwrap());
        e.resolve_top();
        assert_eq!(e.state.player(PlayerId(0)).hand.len(), hand_before + 1, "drew a card");
        assert!(e.state.object(diary).status.tapped, "the {{T}} cost tapped Diary");
    }

    /// Casting an instant/sorcery puts a page counter on Diary (the trigger, through the real cast).
    #[test]
    fn casting_instant_or_sorcery_adds_a_page_counter() {
        let mut state = build_game(1, &[&[grp::FOREST, grp::FOREST], &[]]); // library for Quick Study
        let diary = add_diary_and_mana(&mut state, 0, 3);
        // Quick Study — a {2}{U} sorcery (draw two).
        let qs = state.add_card(
            PlayerId(0),
            state
                .card_db()
                .get(crate::cards::woe::quick_study::QUICK_STUDY)
                .unwrap()
                .chars
                .clone(),
            Zone::Hand,
        );
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
        e.cast_spell(PlayerId(0), qs, crate::agent::CastVariant::Normal); // SpellCast(sorcery) fires
        e.run_agenda(); // Diary's trigger onto the stack (above the spell)
        e.resolve_top(); // resolve the trigger → page counter
        assert_eq!(e.state.object(diary).counters.get(&page()), 1, "one page counter added");
    }
}
