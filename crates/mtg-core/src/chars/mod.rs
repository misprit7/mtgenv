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
use crate::effects::ability::{Ability, Keyword, Qualification, StaticContribution};
use crate::effects::target::{CardFilter, SelectSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::ids::{ObjId, PlayerId, Timestamp};
use crate::state::{Characteristics, GameState};

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
    /// Qualification markers painted by static abilities (CR 613 layer 6 / §2.4) — the
    /// "can't"/status flags the structural machinery reads (e.g. Pacifism's CantAttack/CantBlock).
    pub qualifications: BTreeSet<Qualification>,
}

impl ComputedChars {
    pub fn is_creature(&self) -> bool {
        self.card_types.contains(&CardType::Creature)
    }
    pub fn has_keyword(&self, k: Keyword) -> bool {
        self.keywords.contains(&k)
    }
    pub fn has_qualification(&self, q: Qualification) -> bool {
        self.qualifications.contains(&q)
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
        // Seed from printed keywords (CR 702); layer 6 grants/removes on top.
        keywords: base.keywords.iter().copied().collect(),
        qualifications: BTreeSet::new(),
    };

    // Every static continuous effect on the battlefield, in timestamp order (CR 613.7). We do
    // NOT pre-filter by applicability here: whether an effect applies to `id` is re-checked at
    // each layer against the characteristics computed THROUGH PRIOR LAYERS (CR 613.8 — e.g. an
    // anthem's "creatures you control" must see a land that became a creature in layer 4).
    let effects = gather_statics(state);

    // Layer 4 — type. A type-changer's own `affects` is evaluated against BASE types (intra-
    // layer-4 dependency is the deferred hard case); the result is the computed type set.
    let base_types = base.card_types.clone();
    for e in &effects {
        if let StaticContribution::AddType(t) = e.contribution {
            if affects_matches(state, e, id, &base_types) && !c.card_types.contains(t) {
                c.card_types.push(*t);
            }
        }
    }
    // From here on, `c.card_types` is the post-layer-4 type set: subsequent layers read it
    // (this is the "affects reads computed, not base" fix).
    // Layer 5 — color.
    for e in &effects {
        match e.contribution {
            StaticContribution::AddColor(col) => {
                if affects_matches(state, e, id, &c.card_types) && !c.colors.contains(col) {
                    c.colors.push(*col);
                }
            }
            StaticContribution::SetColor(v) => {
                if affects_matches(state, e, id, &c.card_types) {
                    c.colors = v.clone();
                }
            }
            _ => {}
        }
    }
    // Layer 6 — abilities (grant/remove keywords), in timestamp order.
    for e in &effects {
        match e.contribution {
            StaticContribution::GrantKeyword(k) => {
                if affects_matches(state, e, id, &c.card_types) {
                    c.keywords.insert(*k);
                }
            }
            StaticContribution::RemoveKeyword(k) => {
                if affects_matches(state, e, id, &c.card_types) {
                    c.keywords.remove(k);
                }
            }
            // Qualification markers (CR §2.4) are painted in the abilities layer too.
            StaticContribution::Qualification(q) => {
                if affects_matches(state, e, id, &c.card_types) {
                    c.qualifications.insert(*q);
                }
            }
            _ => {}
        }
    }
    // Layer 7a — characteristic-defining P/T (CDA): set base P/T from dynamic values, e.g. a
    // Vehicle whose power equals the number of lands you control (CR 604.3 / 613.4b). Applies
    // before the fixed set-base (7b) so a later "becomes 2/2" overrides a CDA.
    for e in &effects {
        if let StaticContribution::SetBasePTValue { power, toughness } = e.contribution {
            if affects_matches(state, e, id, &c.card_types) {
                c.power = Some(eval_pt_value(state, power, id));
                c.toughness = Some(eval_pt_value(state, toughness, id));
            }
        }
    }
    // Layer 7b — set base P/T. Later timestamp wins.
    for e in &effects {
        if let StaticContribution::SetBasePT { power, toughness } = e.contribution {
            if affects_matches(state, e, id, &c.card_types) {
                c.power = Some(*power);
                c.toughness = Some(*toughness);
            }
        }
    }
    // Layer 7c — modify P/T: +N/+N effects (timestamp order) then counters. Both add, so the
    // order among them doesn't change the result for the modeled cards.
    for e in &effects {
        if let StaticContribution::ModifyPT { power, toughness } = e.contribution {
            if affects_matches(state, e, id, &c.card_types) {
                if let Some(p) = c.power.as_mut() {
                    *p += power;
                }
                if let Some(t) = c.toughness.as_mut() {
                    *t += toughness;
                }
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

/// Evaluate a `ValueExpr` for a characteristic-defining P/T (CR 604.3) — the "source" is the
/// object being computed (`id`). Supports the subset CDAs use (fixed, counters-on-self, count
/// of objects, sums); X / NumTargets aren't meaningful for a static and read as 0.
fn eval_pt_value(state: &GameState, v: &ValueExpr, source: ObjId) -> i32 {
    match v {
        ValueExpr::Fixed(n) => *n as i32,
        ValueExpr::CountersOnSelf(kind) => state
            .objects
            .get(&source)
            .map(|o| o.counters.get(kind) as i32)
            .unwrap_or(0),
        ValueExpr::Sum(a, b) => eval_pt_value(state, a, source) + eval_pt_value(state, b, source),
        ValueExpr::Count { zone, filter, controller } => {
            let want = controller.map(|r| pt_controller(state, r, source));
            state
                .objects
                .values()
                .filter(|o| o.zone == *zone)
                .filter(|o| want.is_none_or(|p| o.controller == p))
                .filter(|o| pt_base_filter(&o.chars, filter))
                .count() as i32
        }
        _ => 0,
    }
}

/// Resolve a `PlayerRef` in a CDA's `Count` relative to the computed object's controller.
fn pt_controller(state: &GameState, r: PlayerRef, source: ObjId) -> PlayerId {
    let me = state.objects.get(&source).map(|o| o.controller).unwrap_or(PlayerId(0));
    match r {
        PlayerRef::Opponent | PlayerRef::EachOpponent => {
            state.players.iter().map(|p| p.id).find(|&q| q != me).unwrap_or(me)
        }
        _ => me, // Controller / Owner / others
    }
}

/// Evaluate a `CardFilter` against an object's BASE characteristics (avoids recursion into the
/// layer system while computing P/T). `ControlledBy` is handled by `Count`'s controller filter.
fn pt_base_filter(chars: &Characteristics, filter: &CardFilter) -> bool {
    match filter {
        CardFilter::Any | CardFilter::ControlledBy(_) => true,
        CardFilter::HasCardType(t) => chars.card_types.contains(t),
        CardFilter::Supertype(s) => chars.supertypes.contains(s),
        CardFilter::HasSubtype(s) => chars.subtypes.contains(s),
        CardFilter::HasColor(c) => chars.colors.contains(c),
        CardFilter::All(fs) => fs.iter().all(|f| pt_base_filter(chars, f)),
        CardFilter::AnyOf(fs) => fs.iter().any(|f| pt_base_filter(chars, f)),
        CardFilter::Not(f) => !pt_base_filter(chars, f),
        _ => false,
    }
}

/// A static continuous effect on the battlefield, tagged with its source + timestamp.
struct StaticEffect<'a> {
    timestamp: Timestamp,
    src_id: ObjId,
    src_controller: PlayerId,
    contribution: &'a StaticContribution,
    affects: &'a SelectSpec,
}

/// Collect every static continuous effect on the battlefield, timestamp-ordered (CR 613.7).
fn gather_statics(state: &GameState) -> Vec<StaticEffect<'_>> {
    let mut v = Vec::new();
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
                    v.push(StaticEffect {
                        timestamp: src.timestamp,
                        src_id,
                        src_controller: src.controller,
                        contribution,
                        affects,
                    });
                }
            }
        }
    }
    v.sort_by_key(|e| e.timestamp);
    v
}

/// Whether effect `e` applies to `target` — target is in `e.affects.zone` and matches the
/// filter. `target_types` is the target's card types as computed through the layers applied so
/// far (so a layer-6/7 effect's "creature" filter sees a land that became a creature in layer 4).
fn affects_matches(
    state: &GameState,
    e: &StaticEffect,
    target: ObjId,
    target_types: &[CardType],
) -> bool {
    let o = match state.objects.get(&target) {
        Some(o) if o.zone == e.affects.zone => o,
        _ => return false,
    };
    matches_filter(state, &e.affects.filter, o, target, target_types, e.src_id, e.src_controller)
}

/// Evaluate a `CardFilter`. `HasCardType` reads `target_types` (the computed-so-far type set);
/// other characteristic predicates read base characteristics (subtype/color layer-aware
/// evaluation is deferred). `ItSelf`/`ControlledBy` resolve against the effect's source.
fn matches_filter(
    state: &GameState,
    filter: &CardFilter,
    o: &crate::state::Object,
    target_id: ObjId,
    target_types: &[CardType],
    src_id: ObjId,
    src_controller: PlayerId,
) -> bool {
    match filter {
        CardFilter::Any => true,
        CardFilter::ItSelf => target_id == src_id,
        // "Enchanted/equipped creature …": the source (Aura/Equipment) is attached to the
        // candidate. Source-relative, like `ItSelf`, so the "while attached" static stays in the
        // normal global gather scan with no special-casing (CR 702.3e/702.6e).
        CardFilter::AttachedHost => {
            state.objects.get(&src_id).and_then(|s| s.attached_to) == Some(target_id)
        }
        CardFilter::All(fs) => fs
            .iter()
            .all(|f| matches_filter(state, f, o, target_id, target_types, src_id, src_controller)),
        CardFilter::AnyOf(fs) => fs
            .iter()
            .any(|f| matches_filter(state, f, o, target_id, target_types, src_id, src_controller)),
        CardFilter::Not(f) => {
            !matches_filter(state, f, o, target_id, target_types, src_id, src_controller)
        }
        CardFilter::HasCardType(t) => target_types.contains(t),
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
    fn rancor_aura_buffs_and_grants_trample_only_to_its_host() {
        // An Aura's "enchanted creature gets +2/+0 and has trample" is two AttachedHost statics
        // (layer 7c ModifyPT + layer 6 GrantKeyword) — they reach only the attached permanent.
        let mut s = cards::build_game(1, &[&[], &[]]);
        let bears = put(&mut s, PlayerId(0), grp::GRIZZLY_BEARS, Zone::Battlefield); // 2/2
        let other = put(&mut s, PlayerId(0), grp::GRIZZLY_BEARS, Zone::Battlefield); // 2/2
        let rancor = put(&mut s, PlayerId(0), grp::RANCOR, Zone::Battlefield);
        s.objects.get_mut(&rancor).unwrap().attached_to = Some(bears); // (engine sets this on resolution)
        s.mark_chars_dirty();

        let host = compute(&s, bears);
        assert_eq!(host.power, Some(4), "enchanted creature gets +2/+0");
        assert_eq!(host.toughness, Some(2));
        assert!(host.has_keyword(Keyword::Trample), "and has trample");

        let unenchanted = compute(&s, other);
        assert_eq!(unenchanted.power, Some(2), "another creature is unaffected");
        assert!(!unenchanted.has_keyword(Keyword::Trample));
    }

    #[test]
    fn pacifism_paints_cant_attack_block_qualifications_on_its_host() {
        let mut s = cards::build_game(1, &[&[], &[]]);
        let bears = put(&mut s, PlayerId(0), grp::GRIZZLY_BEARS, Zone::Battlefield);
        let other = put(&mut s, PlayerId(0), grp::GRIZZLY_BEARS, Zone::Battlefield);
        let pac = put(&mut s, PlayerId(0), grp::PACIFISM, Zone::Battlefield);
        s.objects.get_mut(&pac).unwrap().attached_to = Some(bears);
        s.mark_chars_dirty();

        let host = compute(&s, bears);
        assert!(host.has_qualification(Qualification::CantAttack), "enchanted creature can't attack");
        assert!(host.has_qualification(Qualification::CantBlock), "enchanted creature can't block");
        assert!(!compute(&s, other).has_qualification(Qualification::CantAttack), "another creature is unaffected");
    }

    #[test]
    fn cda_sets_base_pt_from_a_dynamic_value_c9b() {
        // C9b layer-7a CDA: a Vehicle whose power equals the number of lands you control
        // (Lumbering Worldwagon */4), via StaticContribution::SetBasePTValue over ItSelf.
        use crate::effects::condition::Duration;
        use std::sync::Arc;
        let lands_you_control = ValueExpr::Count {
            zone: Zone::Battlefield,
            filter: CardFilter::All(vec![
                CardFilter::HasCardType(CardType::Land),
                CardFilter::ControlledBy(PlayerRef::Controller),
            ]),
            controller: Some(PlayerRef::Controller),
        };
        let mut db = cards::starter_db();
        db.insert(crate::cards::CardDef {
            chars: Characteristics {
                name: "Test Wagon".into(),
                card_types: vec![CardType::Artifact],
                subtypes: vec!["Vehicle".into()],
                power: Some(0),
                toughness: Some(4),
                grp_id: 9200,
                ..Default::default()
            },
            abilities: vec![Ability::Static {
                contribution: StaticContribution::SetBasePTValue {
                    power: lands_you_control,
                    toughness: ValueExpr::Fixed(4),
                },
                affects: SelectSpec {
                    zone: Zone::Battlefield,
                    filter: CardFilter::ItSelf,
                    chooser: PlayerRef::Controller,
                    min: ValueExpr::Fixed(0),
                    max: ValueExpr::Fixed(0),
                },
                duration: Duration::WhileSourcePresent,
            }],
            mana_colors: Vec::new(),
            text: String::new(),
        });
        let mut s = GameState::new(2, 1);
        s.set_card_db(Arc::new(db));
        let wagon_chars = s.card_db().get(9200).unwrap().chars.clone();
        let wagon = s.add_card(PlayerId(0), wagon_chars, Zone::Battlefield);
        for _ in 0..3 {
            let f = s.card_db().get(grp::FOREST).unwrap().chars.clone();
            s.add_card(PlayerId(0), f, Zone::Battlefield);
        }

        let cc = compute(&s, wagon);
        assert_eq!(cc.power, Some(3), "power = the 3 lands you control");
        assert_eq!(cc.toughness, Some(4), "toughness fixed at 4");
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

    #[test]
    fn affects_reads_computed_type_not_base_cr_613_8() {
        // Nature's Revolt makes all lands 2/2 creatures (layer 4 AddType + 7b SetBasePT).
        // Glorious Anthem ("creatures you control") must see a land's COMPUTED creature type
        // to buff it — the affects-reads-computed (CR 613.8) case.
        let mut s = cards::build_game(1, &[&[], &[]]);
        let my_forest = put(&mut s, PlayerId(0), grp::FOREST, Zone::Battlefield);
        let foe_forest = put(&mut s, PlayerId(1), grp::FOREST, Zone::Battlefield);
        put(&mut s, PlayerId(0), grp::NATURES_REVOLT, Zone::Battlefield);

        assert!(compute(&s, my_forest).is_creature(), "land became a creature (layer 4)");
        assert_eq!(compute(&s, my_forest).power, Some(2), "and 2/2 (7b)");
        assert_eq!(compute(&s, foe_forest).power, Some(2));

        put(&mut s, PlayerId(0), grp::GLORIOUS_ANTHEM, Zone::Battlefield);
        assert_eq!(
            compute(&s, my_forest).power,
            Some(3),
            "anthem buffs the land because its COMPUTED type is Creature"
        );
        assert_eq!(
            compute(&s, foe_forest).power,
            Some(2),
            "opponent's land-creature is not buffed by your anthem"
        );
    }

    #[test]
    fn dirty_recompute_discipline_fires_at_the_right_beats() {
        // The cache is rebuilt on the dirty signal; queries are correct even between rebuilds.
        let mut s = cards::build_game(1, &[&[], &[]]);
        let bears = put(&mut s, PlayerId(0), grp::GRIZZLY_BEARS, Zone::Battlefield);
        assert!(s.chars_is_dirty(), "a permanent entering marks the cache dirty");

        s.recompute_continuous();
        assert!(!s.chars_is_dirty(), "recompute clears the dirty flag");
        assert_eq!(s.computed(bears).power, Some(2), "cached value");

        // An anthem enters → dirty again; a query is still correct (fresh compute while dirty).
        put(&mut s, PlayerId(0), grp::GLORIOUS_ANTHEM, Zone::Battlefield);
        assert!(s.chars_is_dirty(), "the anthem entering re-marks dirty");
        assert_eq!(s.computed(bears).power, Some(3), "correct even before recompute");

        s.recompute_continuous();
        assert!(!s.chars_is_dirty());
        assert_eq!(s.computed(bears).power, Some(3), "cache reflects the anthem after recompute");
    }
}
