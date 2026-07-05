//! Mana and the mana pool, mana abilities (which don't use the stack), and paying costs
//! (CR 106, 605, 118).
//!
//! Milestone 3 scope: basic lands tap for one mana of a fixed colour (CR 605, a mana
//! ability — no stack). Paying a [`ManaCost`] = covering each coloured pip with a source
//! producing that colour, then the generic component with any remaining source (CR 118/202).
//! Payment is **auto-tapped** by the engine (the Arena auto-tap profile / decision elision):
//! [`auto_pay`] greedily taps a sufficient set of untapped mana sources. (A `PayCost`
//! agent decision can replace this later without touching callers.)

use crate::basics::{Color, ManaCost};
use crate::conditions;
use crate::effects::ability::{Ability, EventPattern, Keyword, Restriction};
use crate::effects::target::ManaSpec;
use crate::effects::Effect;
use crate::ids::{ObjId, PlayerId};
use crate::state::GameState;
use crate::subtypes::{LandType, Subtype};

/// The untapped mana sources `p` controls: `(permanent, colours it can tap for right now)`.
/// Colours come from two places, unioned: each source's `{T}`-cost IR mana abilities
/// (`Ability::Activated{is_mana}`, condition-aware), and the **intrinsic** basic-land-type mana
/// derived from the permanent's COMPUTED subtypes (CR 305.6 — see [`basic_land_type_color`]).
fn mana_sources(state: &GameState, p: PlayerId) -> Vec<(ObjId, Vec<Color>)> {
    // Auto-pay: only simple `{T}` sources (cost-bearing mana abilities are excluded — the auto-payer
    // can't pay a sacrifice/mana activation cost, so a Treasure never enters the auto-pay pool).
    mana_sources_kind(state, p, false, false)
}

/// The untapped mana sources `p` controls that produce **restricted** mana (CR 106.6, "spend only to
/// cast instant and sorcery spells" — SoS Hydro-Channeler). Intrinsic basic-land-type mana is never
/// restricted, so only authored `AddMana` abilities carrying a `SpendRestriction` contribute here.
fn restricted_mana_sources(state: &GameState, p: PlayerId) -> Vec<(ObjId, Vec<Color>)> {
    mana_sources_kind(state, p, true, false)
}

/// Shared source enumeration. `restricted == false` yields each source's *unrestricted* colours
/// (unrestricted `AddMana` abilities + intrinsic basic-land-type mana); `restricted == true` yields
/// only colours from `AddMana` abilities carrying a `SpendRestriction` (no intrinsic mana).
/// `include_cost_bearing == false` (the auto-pay path) skips mana abilities with a non-`{T}` cost
/// (Treasure's `{T},Sacrifice`) — those are usable only via manual activation (CR 605.3a).
fn mana_sources_kind(
    state: &GameState,
    p: PlayerId,
    restricted: bool,
    include_cost_bearing: bool,
) -> Vec<(ObjId, Vec<Color>)> {
    state
        .player(p)
        .battlefield
        .iter()
        .filter_map(|&id| {
            let o = state.objects.get(&id)?;
            if o.status.tapped {
                return None;
            }
            let computed = state.computed(id);
            // CR 302.6: a summoning-sick creature can't use a `{T}` mana ability (unless haste).
            // Lands/artifacts are never sick, so this only gates creature mana dorks (Llanowar)
            // and animated man-lands that became creatures this turn.
            if o.summoning_sick && !computed.has_keyword(Keyword::Haste) {
                return None;
            }
            let def = state.card_db.get(o.chars.grp_id)?;
            let mut colors = producible_colors(state, def, p, restricted, include_cost_bearing);
            // CR 305.6: any land with a basic land type has an intrinsic `{T}: Add <colour>`
            // ability per type, NOT authored on the card. We read the COMPUTED subtypes
            // (post-layer-system) so type-changing effects flow through for free — an animated
            // land keeps its mana, Spreading Seas / Urborg-style subtype changes are honoured.
            // Intrinsic mana is always unrestricted.
            if !restricted {
                for st in &computed.subtypes {
                    if let Some(c) = basic_land_type_color(st) {
                        if !colors.contains(&c) {
                            colors.push(c);
                        }
                    }
                }
            }
            if colors.is_empty() {
                None
            } else {
                Some((id, colors))
            }
        })
        .collect()
}

/// CR 305.6: the colour a basic land *type* intrinsically taps for. Returns `None` for
/// non-basic-type subtypes (e.g. Vehicle, Aura). This is what lets a `Forest` produce `{G}`
/// with no authored mana ability, so basics and typed duals (e.g. Temple Garden = `Forest Plains`)
/// carry mana purely from their subtype line — and type-changing effects carry it for free.
fn basic_land_type_color(subtype: &Subtype) -> Option<Color> {
    match subtype {
        Subtype::Land(LandType::Plains) => Some(Color::White),
        Subtype::Land(LandType::Island) => Some(Color::Blue),
        Subtype::Land(LandType::Swamp) => Some(Color::Black),
        Subtype::Land(LandType::Mountain) => Some(Color::Red),
        Subtype::Land(LandType::Forest) => Some(Color::Green),
        _ => None,
    }
}

/// The colours `def` can currently produce for controller `p` — from its IR mana abilities whose
/// activation restriction/condition holds. (Intrinsic basic-land-type mana is added by the caller
/// from the permanent's computed subtypes; see [`mana_sources`].)
fn producible_colors(
    state: &GameState,
    def: &crate::cards::CardDef,
    p: PlayerId,
    restricted: bool,
    include_cost_bearing: bool,
) -> Vec<Color> {
    let mut colors: Vec<Color> = Vec::new();
    let push = |c: Color, v: &mut Vec<Color>| {
        if !v.contains(&c) {
            v.push(c);
        }
    };
    for ab in &def.abilities {
        if let Ability::Activated {
            cost,
            effect: Effect::AddMana { mana, .. },
            restriction,
            is_mana: true,
            ..
        } = ab
        {
            // Select abilities by whether their produced mana carries a spend restriction (CR 106.6):
            // the unrestricted pass wants `mana.restriction == None`, the restricted pass the rest.
            if mana.restriction.is_some() != restricted {
                continue;
            }
            // The auto-pay pool excludes cost-bearing mana abilities (a `{T},Sacrifice` Treasure) —
            // auto-pay only taps; their extra cost is payable only via manual activation (CR 605.3a).
            if !include_cost_bearing && !cost.is_simple_tap_mana() {
                continue;
            }
            let legal = restriction
                .as_ref()
                .is_none_or(|r| restriction_holds(state, r, p));
            if legal {
                for c in mana_spec_colors(mana) {
                    push(c, &mut colors);
                }
            }
        }
    }
    colors
}

/// Which activation restrictions gate a mana ability's availability (CR 605). `OncePerTurn`
/// isn't tracked for mana sources (mana abilities aren't once-per-turn-limited in practice).
fn restriction_holds(state: &GameState, r: &Restriction, controller: PlayerId) -> bool {
    match r {
        Restriction::OnlyIf(cond) => conditions::holds(state, cond, controller),
        Restriction::OnlyYourTurn => state.active_player == controller,
        Restriction::OncePerTurn => true,
    }
}

/// The colours a `ManaSpec` can produce (for the source-selection model). `any_color` offers all
/// five; `produces` lists fixed colours. (`one_of` constrained-choice is wired when design adds it.)
fn mana_spec_colors(mana: &ManaSpec) -> Vec<Color> {
    let mut v: Vec<Color> = mana.produces.iter().map(|(c, _)| *c).collect();
    if mana.any_color.is_some() {
        v.extend([Color::White, Color::Blue, Color::Black, Color::Red, Color::Green]);
    }
    v
}

/// One unit of mana `p` could put toward a cost. `source == None` is mana already FLOATING in the
/// pool (a fixed colour, no tap). `source == Some(id)` is mana produced by tapping `id`: a base unit
/// (`bonus == false`, 1 of the source's colours) or a `TapCreatureForMana` bonus unit (`bonus ==
/// true`, a fixed bonus colour granted per creature tapped — Badgermole, CR 605.1b). Modelling pool
/// + base + bonus uniformly lets affordability and payment count floating mana AND the bonus (#59).
struct ManaUnit {
    source: Option<ObjId>,
    colors: Vec<Color>,
    bonus: bool,
    /// Spend-restricted mana (CR 106.6) — from the pool's `restricted` bucket or a restricted source.
    /// Only offered as a payment unit when the cost being paid allows it (an instant/sorcery cast).
    restricted: bool,
}

/// The colours every `TapCreatureForMana` (Badgermole-type) ability on `p`'s battlefield grants per
/// creature tapped — a multiset (two such abilities each adding `{G}` ⇒ `[G, G]`, i.e. +2 `{G}` per
/// creature tap). Card-agnostic: reads `Triggered{ TapCreatureForMana, AddMana{..} }` abilities.
fn tap_bonus_colors(state: &GameState, p: PlayerId) -> Vec<Color> {
    let db = state.card_db();
    let mut bonus: Vec<Color> = Vec::new();
    for &id in &state.player(p).battlefield {
        let Some(o) = state.objects.get(&id) else { continue };
        let Some(def) = db.get(o.chars.grp_id) else { continue };
        for ab in &def.abilities {
            if let Ability::Triggered {
                event: EventPattern::TapCreatureForMana,
                effect: Effect::AddMana { mana, .. },
                ..
            } = ab
            {
                bonus.extend(mana_spec_colors(mana));
            }
        }
    }
    bonus
}

/// Every unit of mana `p` could spend on a cost right now, in spend-preference order: FLOATING pool
/// mana first (already produced, no tap), then each untapped source's base mana, then per-creature
/// `TapCreatureForMana` bonus units (so a creature is tapped for its bonus only when needed).
/// `excluded` sources are omitted — so a source already committed to a non-mana cost component
/// (e.g. Ba Sing Se tapped for its own `{T}`) can't also produce mana (#57). The mutating `auto_pay`
/// instead relies on those sources already being tapped (so `mana_sources` skips them).
///
/// `allow_restricted` (CR 106.6) folds in restricted mana — the pool's `restricted` bucket and
/// restricted mana sources (Hydro-Channeler) — usable only when the cost being paid is an instant or
/// sorcery cast. When false, restricted mana is invisible, so it can never pay a creature spell or an
/// ability cost.
fn payment_units(
    state: &GameState,
    p: PlayerId,
    excluded: &[ObjId],
    allow_restricted: bool,
) -> Vec<ManaUnit> {
    let mut units: Vec<ManaUnit> = Vec::new();
    // Floating pool mana — spent first (CR 106.4: it stays in the pool until end of step).
    for (color, n) in &state.player(p).mana_pool.amounts {
        for _ in 0..*n {
            units.push(ManaUnit { source: None, colors: vec![*color], bonus: false, restricted: false });
        }
    }
    // Floating RESTRICTED pool mana (only when the cost allows it).
    if allow_restricted {
        for (color, n) in &state.player(p).mana_pool.restricted {
            for _ in 0..*n {
                units.push(ManaUnit { source: None, colors: vec![*color], bonus: false, restricted: true });
            }
        }
    }
    let sources = mana_sources(state, p);
    let bonus_colors = tap_bonus_colors(state, p);
    for (id, colors) in &sources {
        if !excluded.contains(id) {
            units.push(ManaUnit { source: Some(*id), colors: colors.clone(), bonus: false, restricted: false });
        }
    }
    // Restricted mana sources (Hydro-Channeler) — tappable only for an instant/sorcery cast.
    if allow_restricted {
        for (id, colors) in restricted_mana_sources(state, p) {
            if !excluded.contains(&id) {
                units.push(ManaUnit { source: Some(id), colors, bonus: false, restricted: true });
            }
        }
    }
    if !bonus_colors.is_empty() {
        for (id, _) in &sources {
            if !excluded.contains(id) && state.computed(*id).is_creature() {
                for c in &bonus_colors {
                    units.push(ManaUnit { source: Some(*id), colors: vec![*c], bonus: true, restricted: false });
                }
            }
        }
    }
    units
}

/// Greedily assign mana units to `cost`: coloured pips first (each from a unit producing that
/// colour), then generic from any remaining unit. Returns the chosen units paired with the colour
/// each contributes (so payment knows what to add to / spend from the pool), or `None` if unpayable.
fn select_payment(units: &[ManaUnit], cost: &ManaCost) -> Option<Vec<(usize, Color)>> {
    let mut assigned: Vec<Option<Color>> = vec![None; units.len()];
    // Coloured requirements (CR 202.1): each pip needs a matching-colour unit.
    for (color, need) in &cost.colored {
        let mut got = 0;
        for (i, u) in units.iter().enumerate() {
            if got == *need {
                break;
            }
            if assigned[i].is_none() && u.colors.contains(color) {
                assigned[i] = Some(*color);
                got += 1;
            }
        }
        if got < *need {
            return None;
        }
    }
    // Hybrid pips (CR 107.4e): each `{c1/c2}` is paid by a remaining unit that produces *either*
    // colour (prefer `c1`). Done after fixed colour pips so those aren't starved.
    for &(c1, c2) in &cost.hybrid {
        let mut done = false;
        for (i, u) in units.iter().enumerate() {
            if assigned[i].is_none() && (u.colors.contains(&c1) || u.colors.contains(&c2)) {
                assigned[i] = Some(if u.colors.contains(&c1) { c1 } else { c2 });
                done = true;
                break;
            }
        }
        if !done {
            return None;
        }
    }
    // Monocolour hybrid pips (CR 107.4f): each `{n/c}` is paid by ONE remaining unit producing `c`,
    // else by `n` remaining units of any colour. Prefer the colour side (uses fewer units, so it
    // never starves a later requirement). Done after the fixed/two-colour pips so those come first.
    for &(n, c) in &cost.mono_hybrid {
        if let Some(i) = units
            .iter()
            .enumerate()
            .find(|&(i, u)| assigned[i].is_none() && u.colors.contains(&c))
            .map(|(i, _)| i)
        {
            assigned[i] = Some(c);
            continue;
        }
        // Fall back to `n` generic units (each contributes its own colour, for Converge).
        let mut got = 0;
        for (i, u) in units.iter().enumerate() {
            if got == n {
                break;
            }
            if assigned[i].is_none() {
                assigned[i] = Some(u.colors[0]);
                got += 1;
            }
        }
        if got < n {
            return None;
        }
    }
    // Generic: any remaining unit, contributing its first colour (CR 202.1, generic = any mana).
    let mut generic_left = cost.generic;
    for (i, u) in units.iter().enumerate() {
        if generic_left == 0 {
            break;
        }
        if assigned[i].is_none() {
            assigned[i] = Some(u.colors[0]);
            generic_left -= 1;
        }
    }
    if generic_left > 0 {
        return None;
    }
    Some(assigned.iter().enumerate().filter_map(|(i, c)| c.map(|col| (i, col))).collect())
}

/// Whether `p` can pay `cost` from floating mana + untapped sources (CR 118.3), counting the
/// TapCreatureForMana bonus. Restricted mana is **excluded** (CR 106.6) — use [`can_pay_ex`] with
/// `allow_restricted = true` for an instant/sorcery cast. See [`can_pay_excluding`] to omit a
/// cost-committed source.
pub fn can_pay(state: &GameState, p: PlayerId, cost: &ManaCost) -> bool {
    can_pay_excluding(state, p, cost, &[], false)
}

/// As [`can_pay`], but `allow_restricted` folds in restricted mana (CR 106.6) — pass the "is this an
/// instant/sorcery cast?" answer at spell-offer sites so I/S-only mana counts toward affordability.
pub fn can_pay_ex(state: &GameState, p: PlayerId, cost: &ManaCost, allow_restricted: bool) -> bool {
    can_pay_excluding(state, p, cost, &[], allow_restricted)
}

/// As [`can_pay`], but with `excluded` sources removed from the mana set — for affordability of a
/// cost whose non-mana components (e.g. a `{T}` TapSelf) commit those permanents before the mana is
/// paid, so they can't double as mana sources (#57). `allow_restricted` (CR 106.6) counts I/S-only
/// mana (false for ability costs — restricted mana can't pay them).
pub fn can_pay_excluding(
    state: &GameState,
    p: PlayerId,
    cost: &ManaCost,
    excluded: &[ObjId],
    allow_restricted: bool,
) -> bool {
    select_payment(&payment_units(state, p, excluded, allow_restricted), cost).is_some()
}

/// The total mana `p` could put toward a cost right now — floating pool + every untapped source's
/// base + each creature's TapCreatureForMana bonus. A loose upper bound for the `{X}` choice.
/// (Excludes restricted mana — a conservative under-count; no `{X}` spell in the pool spends it.)
pub fn available_mana(state: &GameState, p: PlayerId) -> u32 {
    payment_units(state, p, &[], false).len() as u32
}

/// The untapped mana sources `p` controls right now, each paired with the colours it can tap for —
/// the set a UI session enumerates as manual `ActivateMana` actions at priority (CR 605.3a). Same
/// sources the auto-payer draws from (tapped + summoning-sick sources already filtered out); pure
/// floating pool mana is NOT included (it isn't produced by a tap).
pub fn usable_mana_sources(state: &GameState, p: PlayerId) -> Vec<(ObjId, Vec<Color>)> {
    // The manual (UI) path INCLUDES cost-bearing mana abilities (a Treasure's `{T},Sacrifice`), which
    // the auto-pay pool excludes — a human can choose to activate them (paying the extra cost).
    mana_sources_kind(state, p, false, true)
}

/// Manually activate `source`'s mana ability for one mana of `color` (CR 605.3 — a mana ability, no
/// stack): tap it, add `color` to `p`'s pool, plus any `TapCreatureForMana` bonus when `source` is a
/// creature (Badgermole, CR 605.1b). Returns false (changing nothing) if `source` isn't a current
/// untapped usable mana source of `p` able to produce `color`. The caller floats this in the pool
/// (CR 106.4) and spends it on the next cost — letting a human pick which sources fund a spell (#36).
pub fn produce_mana(state: &mut GameState, p: PlayerId, source: ObjId, color: Color) -> bool {
    use std::collections::BTreeMap;
    // Validate against the live source set so a stale/illegal request is a no-op (the engine only
    // ever offers legal sources, but this keeps the primitive safe in isolation).
    match usable_mana_sources(state, p).into_iter().find(|(id, _)| *id == source) {
        Some((_, cs)) if cs.contains(&color) => {}
        _ => return false,
    }
    if let Some(o) = state.objects.get_mut(&source) {
        o.status.tapped = true;
    }
    let mut additions: BTreeMap<Color, u32> = BTreeMap::new();
    *additions.entry(color).or_insert(0) += 1;
    // Tapping a creature for mana fires every TapCreatureForMana bonus (CR 605.1b), exactly as the
    // auto-payer does — so manual taps and auto-pay agree on the Badgermole-style bonus.
    let bonus_colors = tap_bonus_colors(state, p);
    if !bonus_colors.is_empty() && state.computed(source).is_creature() {
        for c in &bonus_colors {
            *additions.entry(*c).or_insert(0) += 1;
        }
    }
    for (c, n) in additions {
        *state.player_mut(p).mana_pool.amounts.entry(c).or_insert(0) += n;
    }
    true
}

/// Pay `cost` through the real mana pool (CR 605/106/118): tap a sufficient set of sources, produce
/// each tapped source's FULL output into `p`'s pool (base + TapCreatureForMana bonus), then deduct
/// the cost from the pool — spending floating mana first. Surplus stays FLOATING (CR 106.4, emptied
/// at end of step). Returns false (changing nothing) if unpayable. `{0}` is always payable. Callers
/// pay any non-mana cost components (TapSelf/Sacrifice) FIRST so those permanents are excluded here.
/// Auto-tap and pay `cost` (CR 601.2f–h). Returns `Some(colors)` with the **distinct colours of mana
/// spent** to pay it (for Converge, CR 702.75 — "the number of colors of mana spent to cast this
/// spell"), or `None` if the cost can't be paid (nothing is tapped in that case). The colours are the
/// payment plan's assigned colours, so `{3}` paid with three green is one colour, `{W}{U}` is two.
pub fn auto_pay(state: &mut GameState, p: PlayerId, cost: &ManaCost) -> Option<Vec<Color>> {
    auto_pay_ex(state, p, cost, false)
}

/// As [`auto_pay`], but `allow_restricted` (CR 106.6) lets restricted (I/S-only) mana pay `cost` —
/// pass the "is this an instant/sorcery cast?" answer. A tapped restricted source's output is routed
/// to the pool's `restricted` bucket, and [`spend_from_pool`] spends restricted mana first.
pub fn auto_pay_ex(
    state: &mut GameState,
    p: PlayerId,
    cost: &ManaCost,
    allow_restricted: bool,
) -> Option<Vec<Color>> {
    use std::collections::{BTreeMap, BTreeSet};
    let units = payment_units(state, p, &[], allow_restricted);
    let chosen = match select_payment(&units, cost) {
        Some(c) => c,
        None => return None,
    };
    // The distinct colours of the mana this plan spends (CR 702.75 Converge).
    let colors_spent: Vec<Color> =
        chosen.iter().map(|&(_, c)| c).collect::<BTreeSet<Color>>().into_iter().collect();
    // The colour the plan assigned each chosen BASE source-unit (so we add the right colour), and
    // every source's base colours (to colour a creature tapped only for its bonus). Captured BEFORE
    // tapping, since tapping removes the source from `mana_sources`.
    let base_color_of: BTreeMap<ObjId, Color> = chosen
        .iter()
        .filter_map(|&(i, c)| {
            let u = &units[i];
            u.source.filter(|_| !u.bonus).map(|id| (id, c))
        })
        .collect();
    let source_base_colors: BTreeMap<ObjId, Vec<Color>> = units
        .iter()
        .filter(|u| u.source.is_some() && !u.bonus)
        .map(|u| (u.source.unwrap(), u.colors.clone()))
        .collect();
    // Sources whose chosen unit is restricted (their output funds the `restricted` bucket).
    let restricted_taps: BTreeSet<ObjId> = chosen
        .iter()
        .filter_map(|&(i, _)| units[i].source.filter(|_| units[i].restricted))
        .collect();
    // Tap every distinct source backing a chosen produced unit.
    let taps: BTreeSet<ObjId> = chosen.iter().filter_map(|&(i, _)| units[i].source).collect();
    for &id in &taps {
        if let Some(o) = state.objects.get_mut(&id) {
            o.status.tapped = true;
        }
    }
    // Add each tapped source's FULL real output to the pool: its base mana (in the colour the plan
    // assigned it, else its first colour for a bonus-only tap), plus the bonus per creature tapped.
    // A restricted source's base mana goes to the `restricted` bucket; bonus mana is unrestricted.
    let bonus_colors = tap_bonus_colors(state, p);
    let mut additions: BTreeMap<Color, u32> = BTreeMap::new();
    let mut restricted_additions: BTreeMap<Color, u32> = BTreeMap::new();
    for &id in &taps {
        let base = base_color_of
            .get(&id)
            .copied()
            .or_else(|| source_base_colors.get(&id).and_then(|cs| cs.first().copied()));
        if let Some(c) = base {
            if restricted_taps.contains(&id) {
                *restricted_additions.entry(c).or_insert(0) += 1;
            } else {
                *additions.entry(c).or_insert(0) += 1;
            }
        }
        if !bonus_colors.is_empty() && state.computed(id).is_creature() {
            for c in &bonus_colors {
                *additions.entry(*c).or_insert(0) += 1;
            }
        }
    }
    for (c, n) in additions {
        *state.player_mut(p).mana_pool.amounts.entry(c).or_insert(0) += n;
    }
    for (c, n) in restricted_additions {
        *state.player_mut(p).mana_pool.restricted.entry(c).or_insert(0) += n;
    }
    spend_from_pool(state, p, cost, allow_restricted);
    Some(colors_spent)
}

/// Deduct `cost` from `p`'s mana pool: each coloured pip from its colour, then the generic component
/// from any remaining mana (CR 202.1). Assumes affordability was checked (the pool covers it). Drops
/// emptied colour entries so the pool stays canonical for the view. When `allow_restricted` (an I/S
/// cast, CR 106.6), the pool's `restricted` bucket is spent **first** (so restricted mana isn't wasted
/// while unrestricted mana pays); when false, restricted mana is untouched.
fn spend_from_pool(state: &mut GameState, p: PlayerId, cost: &ManaCost, allow_restricted: bool) {
    // Spend `need` mana of `color` from a bucket, returning how much remains unpaid.
    fn take_color(bucket: &mut std::collections::BTreeMap<Color, u32>, color: Color, need: u32) -> u32 {
        let avail = bucket.get(&color).copied().unwrap_or(0);
        let spent = need.min(avail);
        if let Some(v) = bucket.get_mut(&color) {
            *v -= spent;
        }
        need - spent
    }
    // Spend up to `need` mana of any colour from a bucket, returning how much remains unpaid.
    fn take_any(bucket: &mut std::collections::BTreeMap<Color, u32>, mut need: u32) -> u32 {
        for v in bucket.values_mut() {
            if need == 0 {
                break;
            }
            let spent = (*v).min(need);
            *v -= spent;
            need -= spent;
        }
        need
    }
    for (color, need) in &cost.colored {
        let mut left = *need;
        if allow_restricted {
            left = take_color(&mut state.player_mut(p).mana_pool.restricted, *color, left);
        }
        take_color(&mut state.player_mut(p).mana_pool.amounts, *color, left);
    }
    let mut generic_left = cost.generic;
    if allow_restricted {
        generic_left = take_any(&mut state.player_mut(p).mana_pool.restricted, generic_left);
    }
    take_any(&mut state.player_mut(p).mana_pool.amounts, generic_left);
    state.player_mut(p).mana_pool.amounts.retain(|_, v| *v > 0);
    state.player_mut(p).mana_pool.restricted.retain(|_, v| *v > 0);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::basics::Zone;
    use crate::cards::{self, grp};
    use std::collections::BTreeMap;
    use std::sync::Arc;

    fn cost(generic: u32, pips: &[(Color, u32)]) -> ManaCost {
        let mut colored = BTreeMap::new();
        for &(c, n) in pips {
            colored.insert(c, n);
        }
        ManaCost { generic, colored, ..Default::default() }
    }

    fn game_with_lands(forests: usize, mountains: usize) -> GameState {
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(cards::starter_db()));
        let db = state.card_db();
        for _ in 0..forests {
            let c = db.get(grp::FOREST).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield);
        }
        for _ in 0..mountains {
            let c = db.get(grp::MOUNTAIN).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield);
        }
        state
    }

    #[test]
    fn tap_creature_for_mana_triggers_bonus_mana_badgermole() {
        use crate::effects::ability::{Ability, EventPattern};
        use crate::effects::value::{PlayerRef, ValueExpr};
        use crate::state::Characteristics;
        // Badgermole Cub: "Whenever you tap a creature for mana, add an additional {G}." A no-stack
        // mana trigger (CR 605.1b): tapping a creature mana source adds a bonus {G} to the pool.
        let mut db = cards::starter_db();
        db.insert(cards::CardDef {
            chars: Characteristics {
                name: "Dork (test)".into(),
                card_types: vec![crate::basics::CardType::Creature],
                power: Some(1),
                toughness: Some(1),
                grp_id: 9970,
                ..Default::default()
            },
            abilities: vec![cards::mana_ability(Color::Green)], // {T}: Add {G}
            text: String::new(),
            ..Default::default()
        });
        db.insert(cards::CardDef {
            chars: Characteristics {
                name: "Badgermole (test)".into(),
                card_types: vec![crate::basics::CardType::Creature],
                power: Some(2),
                toughness: Some(2),
                grp_id: 9971,
                ..Default::default()
            },
            abilities: vec![Ability::Triggered {
                event: EventPattern::TapCreatureForMana,
                condition: None,
                intervening_if: false,
                effect: Effect::AddMana {
                    who: PlayerRef::Controller,
                    mana: ManaSpec {
                        produces: vec![(Color::Green, ValueExpr::Fixed(1))],
                        any_color: None,
                        restriction: None,
                    },
                },
            }],
            text: String::new(),
            ..Default::default()
        });
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(db));
        let dork = {
            let c = state.card_db().get(9970).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        {
            let c = state.card_db().get(9971).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield);
        }
        state.objects.get_mut(&dork).unwrap().summoning_sick = false; // may tap for mana

        let paid = auto_pay(&mut state, PlayerId(0), &cost(0, &[(Color::Green, 1)])).is_some();
        assert!(paid, "the {{G}} cost was paid by tapping the creature dork");
        assert!(state.object(dork).status.tapped, "the dork tapped for mana");
        assert_eq!(
            state.player(PlayerId(0)).mana_pool.amounts.get(&Color::Green).copied().unwrap_or(0),
            1,
            "Badgermole added a bonus {{G}} to the pool"
        );
    }

    #[test]
    fn badgermole_bonus_counts_toward_affordability_and_payment() {
        // #56: a creature mana-source yields its base mana PLUS the Badgermole TapCreatureForMana
        // bonus, so affordability + payment must count it. Before the fix, costs needing the bonus
        // were wrongly blocked (each source counted as 1 mana; the bonus was added to the pool only
        // AFTER selection and never used to pay).
        use crate::basics::CardType;
        use crate::effects::ability::{Ability, EventPattern};
        use crate::effects::value::{PlayerRef, ValueExpr};
        use crate::state::Characteristics;

        fn badgermole_db() -> cards::CardDb {
            let mut db = cards::starter_db();
            db.insert(cards::CardDef {
                chars: Characteristics {
                    name: "Mana Dork".into(),
                    card_types: vec![CardType::Creature],
                    power: Some(1),
                    toughness: Some(1),
                    grp_id: 9970,
                    ..Default::default()
                },
                abilities: vec![cards::mana_ability(Color::Green)], // {T}: Add {G}
                text: String::new(),
                ..Default::default()
            });
            db.insert(cards::CardDef {
                chars: Characteristics {
                    name: "Badgermole".into(),
                    card_types: vec![CardType::Creature],
                    power: Some(2),
                    toughness: Some(2),
                    grp_id: 9971,
                    ..Default::default()
                },
                abilities: vec![Ability::Triggered {
                    event: EventPattern::TapCreatureForMana,
                    condition: None,
                    intervening_if: false,
                    effect: Effect::AddMana {
                        who: PlayerRef::Controller,
                        mana: ManaSpec {
                            produces: vec![(Color::Green, ValueExpr::Fixed(1))],
                            any_color: None,
                            restriction: None,
                        },
                    },
                }],
                text: String::new(),
                ..Default::default()
            });
            // A Land Creature — Forest (an "earthbent" land): taps for {G} from its subtype AND is a
            // creature, so tapping it triggers the bonus.
            db.insert(cards::CardDef {
                chars: Characteristics {
                    name: "Earthbent Forest".into(),
                    card_types: vec![CardType::Land, CardType::Creature],
                    subtypes: vec![LandType::Forest.into()],
                    power: Some(0),
                    toughness: Some(3),
                    grp_id: 9972,
                    ..Default::default()
                },
                abilities: Vec::new(),
                text: String::new(),
                ..Default::default()
            });
            db
        }

        let add = |state: &mut GameState, grp: u32| -> ObjId {
            let c = state.card_db().get(grp).unwrap().chars.clone();
            let id = state.add_card(PlayerId(0), c, Zone::Battlefield);
            state.objects.get_mut(&id).unwrap().summoning_sick = false; // may tap for mana
            id
        };

        // Case 1: dork ({G}, creature) + Forest ({G}). Two real sources = 2 mana — NOT enough for
        // {2}{G} (3). Adding Badgermole makes the dork's tap also yield a bonus {G} → 3 → affordable.
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(badgermole_db()));
        let dork = add(&mut state, 9970);
        add(&mut state, grp::FOREST);
        assert!(
            !can_pay(&state, PlayerId(0), &cost(2, &[(Color::Green, 1)])),
            "2 plain sources can't pay {{2}}{{G}}"
        );
        add(&mut state, 9971); // Badgermole
        assert!(
            can_pay(&state, PlayerId(0), &cost(2, &[(Color::Green, 1)])),
            "the Badgermole bonus makes {{2}}{{G}} affordable"
        );
        assert!(auto_pay(&mut state, PlayerId(0), &cost(2, &[(Color::Green, 1)])).is_some());
        assert!(state.object(dork).status.tapped, "the dork tapped (its bonus was needed)");
        assert_eq!(
            state.player(PlayerId(0)).mana_pool.amounts.get(&Color::Green).copied().unwrap_or(0),
            0,
            "the bonus was fully spent on the cost → no phantom float"
        );

        // Case 2: dork + earthbent Forest (BOTH creatures) + Badgermole → 2 base + 2 bonus = 4 mana,
        // enough for {2}{G}{G}; two real sources alone (2 mana) could not.
        let mut s2 = GameState::new(2, 1);
        s2.set_card_db(Arc::new(badgermole_db()));
        add(&mut s2, 9970); // dork
        add(&mut s2, 9972); // earthbent Forest (land creature)
        assert!(
            !can_pay(&s2, PlayerId(0), &cost(2, &[(Color::Green, 2)])),
            "two sources alone can't pay {{2}}{{G}}{{G}}"
        );
        add(&mut s2, 9971); // Badgermole
        assert!(
            can_pay(&s2, PlayerId(0), &cost(2, &[(Color::Green, 2)])),
            "two creature sources each get a bonus {{G}} → {{2}}{{G}}{{G}} affordable"
        );
        assert!(auto_pay(&mut s2, PlayerId(0), &cost(2, &[(Color::Green, 2)])).is_some());
    }

    #[test]
    fn floating_mana_persists_across_payments_within_a_step() {
        // #59: tapping the dork for {G} with Badgermole out produces {G}{G} into the pool; paying
        // {G} leaves 1 FLOATING. A SECOND {G} payment in the same step is covered by that floating
        // mana — no new source is tapped (the dork is already tapped). The pool then has none left.
        // (End-of-step emptying, CR 500.4, is `empty_mana_pools`' job — exercised at the priority
        // level; here we prove the pool both retains and spends floating mana mid-step.)
        use crate::basics::CardType;
        use crate::effects::ability::{Ability, EventPattern};
        use crate::effects::value::{PlayerRef, ValueExpr};
        use crate::state::Characteristics;

        let mut db = cards::starter_db();
        db.insert(cards::CardDef {
            chars: Characteristics {
                name: "Dork".into(),
                card_types: vec![CardType::Creature],
                power: Some(1),
                toughness: Some(1),
                grp_id: 9975,
                ..Default::default()
            },
            abilities: vec![cards::mana_ability(Color::Green)],
            text: String::new(),
            ..Default::default()
        });
        db.insert(cards::CardDef {
            chars: Characteristics {
                name: "Badgermole".into(),
                card_types: vec![CardType::Creature],
                power: Some(2),
                toughness: Some(2),
                grp_id: 9976,
                ..Default::default()
            },
            abilities: vec![Ability::Triggered {
                event: EventPattern::TapCreatureForMana,
                condition: None,
                intervening_if: false,
                effect: Effect::AddMana {
                    who: PlayerRef::Controller,
                    mana: ManaSpec {
                        produces: vec![(Color::Green, ValueExpr::Fixed(1))],
                        any_color: None,
                        restriction: None,
                    },
                },
            }],
            text: String::new(),
            ..Default::default()
        });
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(db));
        let dork = {
            let c = state.card_db().get(9975).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        {
            let c = state.card_db().get(9976).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield);
        }
        state.objects.get_mut(&dork).unwrap().summoning_sick = false;
        let green =
            |s: &GameState| s.player(PlayerId(0)).mana_pool.amounts.get(&Color::Green).copied().unwrap_or(0);

        // First {G}: the dork taps → {G}{G} produced → spend {G} → 1 floats.
        assert!(auto_pay(&mut state, PlayerId(0), &cost(0, &[(Color::Green, 1)])).is_some());
        assert_eq!(green(&state), 1, "the Badgermole bonus {{G}} floats after paying {{G}}");
        assert!(state.object(dork).status.tapped, "the dork is now tapped");
        // Second {G} in the same step: paid from the FLOATING mana — the dork is already tapped, so
        // there is no other source; this only succeeds because floating mana persisted.
        assert!(auto_pay(&mut state, PlayerId(0), &cost(0, &[(Color::Green, 1)])).is_some());
        assert_eq!(green(&state), 0, "the floating {{G}} paid the second cost — no new source tapped");
    }

    #[test]
    fn pays_colored_and_generic() {
        let mut state = game_with_lands(2, 2); // GG, RR available
        // {1}{G} (Grizzly Bears): payable.
        assert!(can_pay(&state, PlayerId(0), &cost(1, &[(Color::Green, 1)])));
        // {3}{R}: needs 4 mana total — exactly available.
        assert!(can_pay(&state, PlayerId(0), &cost(3, &[(Color::Red, 1)])));
        // {W}: no white source.
        assert!(!can_pay(&state, PlayerId(0), &cost(0, &[(Color::White, 1)])));
        // {5}: only 4 sources.
        assert!(!can_pay(&state, PlayerId(0), &cost(5, &[])));

        // Paying {1}{G} taps exactly two lands.
        let untapped_before = state.player(PlayerId(0)).battlefield.iter().filter(|&&id| !state.objects[&id].status.tapped).count();
        assert_eq!(untapped_before, 4);
        assert!(auto_pay(&mut state, PlayerId(0), &cost(1, &[(Color::Green, 1)])).is_some());
        let untapped_after = state.player(PlayerId(0)).battlefield.iter().filter(|&&id| !state.objects[&id].status.tapped).count();
        assert_eq!(untapped_after, 2, "two lands tapped to pay {{1}}{{G}}");
    }

    #[test]
    fn auto_pay_reports_distinct_colors_spent() {
        // Converge (CR 702.75): the count of distinct colours the payment plan spends.
        let mut two = game_with_lands(1, 1); // Forest (G) + Mountain (R)
        let colors = auto_pay(&mut two, PlayerId(0), &cost(0, &[(Color::Green, 1), (Color::Red, 1)])).unwrap();
        assert_eq!(colors.len(), 2, "{{G}}{{R}} spends two colours");

        let mut one = game_with_lands(2, 0); // GG
        let colors = auto_pay(&mut one, PlayerId(0), &cost(1, &[(Color::Green, 1)])).unwrap();
        assert_eq!(colors.len(), 1, "{{1}}{{G}} paid all-green spends one colour");
    }

    #[test]
    fn hybrid_pip_pays_with_either_color() {
        let hybrid = |a: Color, b: Color| ManaCost { hybrid: vec![(a, b)], ..Default::default() };
        let state = game_with_lands(1, 0); // one Forest (G)
        // {G/R} is payable — the Forest covers the G side.
        assert!(can_pay(&state, PlayerId(0), &hybrid(Color::Green, Color::Red)));
        // {R/G} too (order-independent).
        assert!(can_pay(&state, PlayerId(0), &hybrid(Color::Red, Color::Green)));
        // {W/U} is NOT payable — a green source is neither white nor blue.
        assert!(!can_pay(&state, PlayerId(0), &hybrid(Color::White, Color::Blue)));
    }

    #[test]
    fn mono_hybrid_pip_pays_with_color_or_n_generic() {
        // CR 107.4f: `{2/R}` is payable by ONE red mana OR TWO of any mana.
        let mono = |n: u32, c: Color| ManaCost { mono_hybrid: vec![(n, c)], ..Default::default() };
        // One Mountain (R) covers the coloured side.
        assert!(can_pay(&game_with_lands(0, 1), PlayerId(0), &mono(2, Color::Red)));
        // Two Forests (G) cover the 2-generic side.
        assert!(can_pay(&game_with_lands(2, 0), PlayerId(0), &mono(2, Color::Red)));
        // A single Forest is neither red nor two mana → not payable.
        assert!(!can_pay(&game_with_lands(1, 0), PlayerId(0), &mono(2, Color::Red)));
        // Three `{2/R}` pips (Magmablood): three Mountains pay all coloured sides.
        assert!(can_pay(
            &game_with_lands(0, 3),
            PlayerId(0),
            &ManaCost { mono_hybrid: vec![(2, Color::Red), (2, Color::Red), (2, Color::Red)], ..Default::default() }
        ));
    }

    #[test]
    fn summoning_sick_creature_cannot_tap_for_mana() {
        // C1 / CR 302.6: a creature mana dork that entered this turn can't tap for mana yet.
        use crate::basics::CardType;
        use crate::state::Characteristics;
        let mut db = cards::starter_db();
        db.insert(cards::CardDef {
            chars: Characteristics {
                name: "Test Dork".into(),
                card_types: vec![CardType::Creature],
                colors: vec![Color::Green],
                power: Some(1),
                toughness: Some(1),
                grp_id: 9000,
                ..Default::default()
            },
            // C19: mana via a real `{T}: Add {G}` IR ability (the `mana_colors` shortcut is gone).
            abilities: vec![cards::mana_ability(Color::Green)],
            text: String::new(),
            ..Default::default()
        });
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(db));
        let chars = state.card_db().get(9000).unwrap().chars.clone();
        let dork = state.add_card(PlayerId(0), chars, Zone::Battlefield);

        // Entered this turn → summoning sick → not a usable mana source.
        state.objects.get_mut(&dork).unwrap().summoning_sick = true;
        assert!(
            !can_pay(&state, PlayerId(0), &cost(0, &[(Color::Green, 1)])),
            "a summoning-sick dork can't tap for {{G}}"
        );
        // Sickness gone → it can tap.
        state.objects.get_mut(&dork).unwrap().summoning_sick = false;
        assert!(
            can_pay(&state, PlayerId(0), &cost(0, &[(Color::Green, 1)])),
            "an un-sick dork taps for {{G}}"
        );
    }

    #[test]
    fn conditional_mana_ability_is_gated_by_its_condition() {
        // C19: a land with "{T}: Add {W}, only if you control a Forest" (IR mana ability with
        // Restriction::OnlyIf) is only a {W} source while the condition holds.
        use crate::basics::CardType;
        use crate::effects::ability::{Ability, Cost, CostComponent, Restriction, Timing};
        use crate::effects::condition::Condition;
        use crate::effects::target::{CardFilter, ManaSpec};
        use crate::effects::value::{PlayerRef, ValueExpr};
        use crate::effects::Effect;
        use crate::state::Characteristics;
        let mut db = cards::starter_db();
        db.insert(cards::CardDef {
            chars: Characteristics {
                name: "Conditional Land".into(),
                card_types: vec![CardType::Land],
                grp_id: 9400,
                ..Default::default()
            },
            abilities: vec![Ability::Activated {
                cost: Cost { mana: None, components: vec![CostComponent::TapSelf] },
                effect: Effect::AddMana {
                    who: PlayerRef::Controller,
                    mana: ManaSpec {
                        produces: vec![(Color::White, ValueExpr::Fixed(1))],
                        any_color: None,
                        restriction: None,
                    },
                },
                timing: Timing::Instant,
                restriction: Some(Restriction::OnlyIf(Condition::CountAtLeast {
                    zone: Zone::Battlefield,
                    filter: CardFilter::HasSubtype(LandType::Forest.into()),
                    controller: Some(PlayerRef::Controller),
                    n: ValueExpr::Fixed(1),
                })),
                is_mana: true,
            }],
            text: String::new(),
            ..Default::default()
        });
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(db));
        let c = state.card_db().get(9400).unwrap().chars.clone();
        state.add_card(PlayerId(0), c, Zone::Battlefield);

        // No Forest → the conditional {W} ability isn't available.
        assert!(
            !can_pay(&state, PlayerId(0), &cost(0, &[(Color::White, 1)])),
            "conditional {{W}} is unavailable without a Forest"
        );
        // Control a Forest → the condition holds → {W} becomes payable.
        let forest = state.card_db().get(grp::FOREST).unwrap().chars.clone();
        state.add_card(PlayerId(0), forest, Zone::Battlefield);
        assert!(
            can_pay(&state, PlayerId(0), &cost(0, &[(Color::White, 1)])),
            "conditional {{W}} is available once you control a Forest"
        );
    }

    #[test]
    fn basic_land_type_mana_is_intrinsic_from_subtype() {
        // CR 305.6: a land taps for its basic land type's colour with NO authored mana ability
        // and NO `mana_colors` shortcut — purely from its computed subtype line. A typed dual
        // (e.g. Temple Garden = `Forest Plains`) taps for both.
        use crate::basics::CardType;
        use crate::state::Characteristics;
        let mut db = cards::starter_db();
        // A pure-data basic: just `Land` + supertype `Basic` + subtype `Forest`. No ability,
        // no `mana_colors`. This is exactly how design will author basics post-migration.
        db.insert(cards::CardDef {
            chars: Characteristics {
                name: "Plain Forest".into(),
                card_types: vec![CardType::Land],
                supertypes: vec![crate::subtypes::Supertype::Basic],
                subtypes: vec![LandType::Forest.into()],
                grp_id: 9401,
                ..Default::default()
            },
            abilities: Vec::new(),
            text: String::new(),
            ..Default::default()
        });
        // A typed dual: subtypes `Forest Plains`, no ability, no shortcut.
        db.insert(cards::CardDef {
            chars: Characteristics {
                name: "Type Garden".into(),
                card_types: vec![CardType::Land],
                subtypes: vec![LandType::Forest.into(), LandType::Plains.into()],
                grp_id: 9402,
                ..Default::default()
            },
            abilities: Vec::new(),
            text: String::new(),
            ..Default::default()
        });
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(db));
        let forest = state.card_db().get(9401).unwrap().chars.clone();
        state.add_card(PlayerId(0), forest, Zone::Battlefield);

        // The subtype alone makes it a {G} source.
        assert!(
            can_pay(&state, PlayerId(0), &cost(0, &[(Color::Green, 1)])),
            "a `Forest` subtype intrinsically taps for {{G}}"
        );
        assert!(
            !can_pay(&state, PlayerId(0), &cost(0, &[(Color::White, 1)])),
            "a `Forest` doesn't tap for {{W}}"
        );

        let garden = state.card_db().get(9402).unwrap().chars.clone();
        state.add_card(PlayerId(0), garden, Zone::Battlefield);
        // Now 2 sources; the dual covers both G and W simultaneously.
        assert!(
            can_pay(&state, PlayerId(0), &cost(0, &[(Color::Green, 1), (Color::White, 1)])),
            "`Forest Plains` + `Forest` pay {{G}}{{W}}"
        );
        assert_eq!(available_mana(&state, PlayerId(0)), 2);
    }
}
