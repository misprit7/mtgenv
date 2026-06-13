//! Characteristic computation: `base (copiable, CR 707.2) ⊕ layered continuous effects →
//! ComputedChars`. The continuous-effects layer system (CR 613) lives here; the engine
//! recomputes on a dirty signal (zone change, step/phase boundary, ability add/remove,
//! counter/timestamp change) and queries via [`GameState::computed`].
//!
//! Milestone-5 prototype scope (per the M5 task): the full 7-layer framework + timestamps
//! (613.7), populated/validated on the COMMON layers first — layer 6 (keyword grants) and
//! layer 7 (P/T: 7b set-base, 7c modify + counters) — over design's `StaticContribution` IR.
//! Layers 1–5 (copy/control/text/type/color) are framework-present and lightly exercised
//! (type/color handled; copy/control/text deferred). The dependency system (613.8) is
//! present as timestamp ordering within a sublayer; a genuine dependency card pair (Humility
//! + type-changers) is deferred.
//!
//! `affects`/filters are evaluated against BASE characteristics (a creature is a creature by
//! its printed type) — full layer-aware dependency evaluation is the deferred 613.8 case.

use std::collections::BTreeSet;

use crate::basics::{CardType, Color};
use crate::effects::ability::{Ability, Keyword, StaticContribution};
use crate::effects::target::{CardFilter, SelectSpec};
use crate::effects::value::PlayerRef;
use crate::ids::{ObjId, PlayerId, Timestamp};
use crate::state::GameState;

/// The post-layer computed characteristics of an object (CR 613 output). Milestone 5 fills
/// P/T, keywords, types and colors; the rest of the closed characteristic list (CR 109.3)
/// joins as layers 1–3 land.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ComputedChars {
    pub power: Option<i32>,
    pub toughness: Option<i32>,
    pub card_types: Vec<CardType>,
    pub subtypes: Vec<String>,
    pub colors: Vec<Color>,
    pub keywords: BTreeSet<Keyword>,
}

impl ComputedChars {
    pub fn is_creature(&self) -> bool {
        self.card_types.contains(&CardType::Creature)
    }
    pub fn has_keyword(&self, k: Keyword) -> bool {
        self.keywords.contains(&k)
    }
}

/// Compute an object's characteristics by applying every applicable continuous effect in
/// layer/sublayer/timestamp order (CR 613). For battlefield objects; off-battlefield objects
/// just get their base characteristics (no permanents' statics apply to them here).
pub fn compute(state: &GameState, id: ObjId) -> ComputedChars {
    let obj = match state.objects.get(&id) {
        Some(o) => o,
        None => return ComputedChars::default(),
    };
    let base = &obj.chars;
    let mut c = ComputedChars {
        power: base.power,
        toughness: base.toughness,
        card_types: base.card_types.clone(),
        subtypes: base.subtypes.clone(),
        colors: base.colors.clone(),
        // Printed keywords aren't carried on `Characteristics` yet; granted ones come from
        // layer 6. (When a printed-keyword card lands, seed from base here.)
        keywords: BTreeSet::new(),
    };

    // Gather the static continuous effects that affect this object, tagged with the source
    // permanent's timestamp (CR 613.7).
    let mut effects: Vec<(Timestamp, StaticContribution)> = Vec::new();
    for p in &state.players {
        for &src_id in &p.battlefield {
            let src = match state.objects.get(&src_id) {
                Some(s) => s,
                None => continue,
            };
            let def = match state.card_db.get(src.chars.grp_id) {
                Some(d) => d,
                None => continue,
            };
            for ab in &def.abilities {
                if let Ability::Static { contribution, affects, .. } = ab {
                    if static_affects(state, affects, src_id, src.controller, id) {
                        effects.push((src.timestamp, contribution.clone()));
                    }
                }
            }
        }
    }
    // Within each (sub)layer, apply in timestamp order (613.7). Stable sort preserves a
    // deterministic order for equal timestamps.
    effects.sort_by_key(|(ts, _)| *ts);

    // Layer 4 — type (add card types).
    for (_, contrib) in &effects {
        if let StaticContribution::AddType(t) = contrib {
            if !c.card_types.contains(t) {
                c.card_types.push(*t);
            }
        }
    }
    // Layer 5 — color.
    for (_, contrib) in &effects {
        match contrib {
            StaticContribution::AddColor(col) => {
                if !c.colors.contains(col) {
                    c.colors.push(*col);
                }
            }
            StaticContribution::SetColor(v) => c.colors = v.clone(),
            _ => {}
        }
    }
    // Layer 6 — abilities (grant/remove keywords), in timestamp order.
    for (_, contrib) in &effects {
        match contrib {
            StaticContribution::GrantKeyword(k) => {
                c.keywords.insert(*k);
            }
            StaticContribution::RemoveKeyword(k) => {
                c.keywords.remove(k);
            }
            _ => {}
        }
    }
    // Layer 7b — set base P/T (and "base P/T" references). Later timestamp wins.
    for (_, contrib) in &effects {
        if let StaticContribution::SetBasePT { power, toughness } = contrib {
            c.power = Some(*power);
            c.toughness = Some(*toughness);
        }
    }
    // Layer 7c — modify P/T: +N/+N effects (timestamp order) then counters. Both add, so the
    // order among them doesn't change the result for the modeled cards.
    for (_, contrib) in &effects {
        if let StaticContribution::ModifyPT { power, toughness } = contrib {
            if let Some(p) = c.power.as_mut() {
                *p += power;
            }
            if let Some(t) = c.toughness.as_mut() {
                *t += toughness;
            }
        }
    }
    let counter_delta = obj.counter_pt_delta();
    if counter_delta != 0 {
        if let Some(p) = c.power.as_mut() {
            *p += counter_delta;
        }
        if let Some(t) = c.toughness.as_mut() {
            *t += counter_delta;
        }
    }
    // Layer 7d — switch P/T: no card uses it in the prototype.
    c
}

/// Whether a static on `src` (controlled by `src_controller`) affects `target` — i.e. the
/// target is in the static's `affects.zone` and matches its filter (evaluated against base
/// characteristics, with the source as the `ItSelf`/`ControlledBy(Controller)` context).
fn static_affects(
    state: &GameState,
    affects: &SelectSpec,
    src_id: ObjId,
    src_controller: PlayerId,
    target: ObjId,
) -> bool {
    match state.objects.get(&target) {
        Some(o) if o.zone == affects.zone => {}
        _ => return false,
    }
    matches_filter(state, &affects.filter, target, src_id, src_controller)
}

/// Evaluate a `CardFilter` against object `obj` (base characteristics), where the filter
/// belongs to a static on `src` controlled by `src_controller`.
fn matches_filter(
    state: &GameState,
    filter: &CardFilter,
    obj: ObjId,
    src: ObjId,
    src_controller: PlayerId,
) -> bool {
    let o = match state.objects.get(&obj) {
        Some(o) => o,
        None => return false,
    };
    match filter {
        CardFilter::Any => true,
        CardFilter::ItSelf => obj == src,
        CardFilter::All(fs) => fs.iter().all(|f| matches_filter(state, f, obj, src, src_controller)),
        CardFilter::AnyOf(fs) => fs.iter().any(|f| matches_filter(state, f, obj, src, src_controller)),
        CardFilter::Not(f) => !matches_filter(state, f, obj, src, src_controller),
        CardFilter::HasCardType(t) => o.chars.card_types.contains(t),
        CardFilter::HasSubtype(s) => o.chars.subtypes.contains(s),
        CardFilter::HasColor(col) => o.chars.colors.contains(col),
        CardFilter::ControlledBy(pref) => {
            let want = match pref {
                PlayerRef::Controller | PlayerRef::Owner => src_controller,
                PlayerRef::Opponent | PlayerRef::EachOpponent => state
                    .players
                    .iter()
                    .map(|p| p.id)
                    .find(|&q| q != src_controller)
                    .unwrap_or(src_controller),
                _ => src_controller,
            };
            o.controller == want
        }
        // Tapped/Untapped/HasCounter/ManaValue/Named/Colorless: not needed by the M5 prototype.
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::basics::{CounterKind, Zone};
    use crate::cards::{self, grp};

    fn put(state: &mut GameState, owner: PlayerId, grp_id: u32, zone: Zone) -> ObjId {
        let chars = state.card_db().get(grp_id).unwrap().chars.clone();
        state.add_card(owner, chars, zone)
    }

    #[test]
    fn anthem_buffs_only_your_creatures_layer_7c() {
        let mut s = cards::build_game(1, &[&[], &[]]);
        let bears = put(&mut s, PlayerId(0), grp::GRIZZLY_BEARS, Zone::Battlefield); // 2/2
        let giant = put(&mut s, PlayerId(0), grp::HILL_GIANT, Zone::Battlefield); // 3/3
        let foe = put(&mut s, PlayerId(1), grp::GRIZZLY_BEARS, Zone::Battlefield); // 2/2 opponent
        put(&mut s, PlayerId(0), grp::GLORIOUS_ANTHEM, Zone::Battlefield);

        assert_eq!(compute(&s, bears).power, Some(3));
        assert_eq!(compute(&s, bears).toughness, Some(3));
        assert_eq!(compute(&s, giant).power, Some(4));
        assert_eq!(compute(&s, giant).toughness, Some(4));
        assert_eq!(compute(&s, foe).power, Some(2), "opponent's creature is unaffected");
    }

    #[test]
    fn two_anthems_stack() {
        let mut s = cards::build_game(1, &[&[], &[]]);
        let bears = put(&mut s, PlayerId(0), grp::GRIZZLY_BEARS, Zone::Battlefield); // 2/2
        put(&mut s, PlayerId(0), grp::GLORIOUS_ANTHEM, Zone::Battlefield);
        put(&mut s, PlayerId(0), grp::GLORIOUS_ANTHEM, Zone::Battlefield);
        assert_eq!(compute(&s, bears).power, Some(4), "+1/+1 twice");
        assert_eq!(compute(&s, bears).toughness, Some(4));
    }

    #[test]
    fn levitation_grants_flying_layer_6() {
        let mut s = cards::build_game(1, &[&[], &[]]);
        let bears = put(&mut s, PlayerId(0), grp::GRIZZLY_BEARS, Zone::Battlefield);
        let foe = put(&mut s, PlayerId(1), grp::GRIZZLY_BEARS, Zone::Battlefield);
        put(&mut s, PlayerId(0), grp::LEVITATION, Zone::Battlefield);
        assert!(compute(&s, bears).has_keyword(Keyword::Flying), "your creature gains flying");
        assert!(!compute(&s, foe).has_keyword(Keyword::Flying), "opponent's does not");
    }

    #[test]
    fn humility_sets_base_then_anthem_then_counter_in_sublayer_order() {
        // 7b (set base 1/1) → 7c (+1/+1 anthem) → 7c counters: 2/2 base → 1/1 → 2/2 → 3/3.
        let mut s = cards::build_game(1, &[&[], &[]]);
        let bears = put(&mut s, PlayerId(0), grp::GRIZZLY_BEARS, Zone::Battlefield); // base 2/2
        put(&mut s, PlayerId(0), grp::HUMILITY, Zone::Battlefield);
        assert_eq!(compute(&s, bears).power, Some(1), "Humility sets base 1/1 (over the 2/2)");
        assert_eq!(compute(&s, bears).toughness, Some(1));

        put(&mut s, PlayerId(0), grp::GLORIOUS_ANTHEM, Zone::Battlefield);
        assert_eq!(compute(&s, bears).power, Some(2), "7c anthem applies after 7b set");

        s.objects
            .get_mut(&bears)
            .unwrap()
            .counters
            .counts
            .insert(CounterKind::PlusOnePlusOne, 1);
        assert_eq!(compute(&s, bears).power, Some(3), "+1/+1 counter (7c) stacks on top");
        assert_eq!(compute(&s, bears).toughness, Some(3));
    }
}
