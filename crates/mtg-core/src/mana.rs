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
use crate::effects::value::ValueExpr;
use crate::effects::Effect;
use crate::ids::{ObjId, PlayerId};
use crate::state::GameState;
use crate::subtypes::{LandType, Subtype};

/// The untapped mana sources `p` controls: `(permanent, colours it can tap for right now, mana per
/// tap)`. Colours come from three places, unioned: each source's `{T}`-cost IR mana abilities
/// (`Ability::Activated{is_mana}`, condition-aware), the **intrinsic** basic-land-type mana derived
/// from the permanent's COMPUTED subtypes (CR 305.6 — see [`basic_land_type_color`]), and any
/// **granted** tap-mana ability (Resonating Lute — [`crate::chars::granted_tap_mana`]). The third
/// element is how much mana one tap yields (CR — "add two mana"; 1 for a normal source).
fn mana_sources(state: &GameState, p: PlayerId) -> Vec<(ObjId, Vec<Color>, u32)> {
    // Auto-pay: only simple `{T}` sources (cost-bearing mana abilities are excluded — the auto-payer
    // can't pay a sacrifice/mana activation cost, so a Treasure never enters the auto-pay pool).
    mana_sources_kind(state, p, false, false)
}

/// The untapped mana sources `p` controls that produce **restricted** mana (CR 106.6, "spend only to
/// cast instant and sorcery spells" — SoS Hydro-Channeler, Resonating Lute). Intrinsic basic-land-type
/// mana is never restricted, so only authored/granted `AddMana` abilities carrying a `SpendRestriction`
/// contribute here.
fn restricted_mana_sources(state: &GameState, p: PlayerId) -> Vec<(ObjId, Vec<Color>, u32)> {
    mana_sources_kind(state, p, true, false)
}

/// Shared source enumeration. `restricted == false` yields each source's *unrestricted* colours
/// (unrestricted `AddMana` abilities + intrinsic basic-land-type mana + unrestricted granted
/// tap-mana); `restricted == true` yields only colours from `AddMana`/granted abilities carrying a
/// `SpendRestriction` (no intrinsic mana). `include_cost_bearing == false` (the auto-pay path) skips
/// printed mana abilities with a non-`{T}` cost (Treasure's `{T},Sacrifice`) — usable only via manual
/// activation (CR 605.3a). Granted tap-mana abilities are plain `{T}`, so they're always included.
/// The `u32` is the representative mana-per-tap for this pass (the max produced count among the
/// abilities that contributed; intrinsic mana is 1) — see [`mana_spec_count`].
fn mana_sources_kind(
    state: &GameState,
    p: PlayerId,
    restricted: bool,
    include_cost_bearing: bool,
) -> Vec<(ObjId, Vec<Color>, u32)> {
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
            let (mut colors, mut count) =
                producible_mana(state, def, p, restricted, include_cost_bearing);
            // Granted tap-mana abilities (CR 613.1f — Resonating Lute "Lands you control have '{T}: Add
            // two mana of any one colour…'"). These have no home in `ComputedChars`, so read them
            // directly; fold in each grant whose spend-restriction matches this pass. A granted ability
            // is a plain `{T}` (no extra cost) so `include_cost_bearing` doesn't gate it.
            for mana in crate::chars::granted_tap_mana(state, id) {
                if mana.restriction.is_some() != restricted {
                    continue;
                }
                for c in mana_spec_colors(&mana) {
                    if !colors.contains(&c) {
                        colors.push(c);
                    }
                }
                count = count.max(mana_spec_count(&mana));
            }
            // CR 305.6: any land with a basic land type has an intrinsic `{T}: Add <colour>`
            // ability per type, NOT authored on the card. We read the COMPUTED subtypes
            // (post-layer-system) so type-changing effects flow through for free — an animated
            // land keeps its mana, Spreading Seas / Urborg-style subtype changes are honoured.
            // Intrinsic mana is always unrestricted, one per tap.
            if !restricted {
                for st in &computed.subtypes {
                    if let Some(c) = basic_land_type_color(st) {
                        if !colors.contains(&c) {
                            colors.push(c);
                        }
                        count = count.max(1);
                    }
                }
            }
            if colors.is_empty() {
                None
            } else {
                Some((id, colors, count.max(1)))
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

/// The colours `def` can currently produce for controller `p` (unioned) and the max mana its
/// abilities yield per tap — from its IR mana abilities whose activation restriction/condition holds.
/// (Intrinsic basic-land-type mana and granted tap-mana are added by the caller; see [`mana_sources`].)
fn producible_mana(
    state: &GameState,
    def: &crate::cards::CardDef,
    p: PlayerId,
    restricted: bool,
    include_cost_bearing: bool,
) -> (Vec<Color>, u32) {
    let mut colors: Vec<Color> = Vec::new();
    let mut count: u32 = 0;
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
                    if !colors.contains(&c) {
                        colors.push(c);
                    }
                }
                count = count.max(mana_spec_count(mana));
            }
        }
    }
    (colors, count)
}

/// How many mana a `ManaSpec` yields per tap (CR — Resonating Lute "add **two** mana of any one
/// colour"). `any_color` carries the count directly; otherwise it's the sum of the per-colour
/// produced amounts. Amounts are `Fixed` for every tap mana ability in the pool; a non-constant
/// amount reads as 1 (conservative — no tap mana ability produces a dynamic count today).
fn mana_spec_count(mana: &ManaSpec) -> u32 {
    let fixed = |v: &ValueExpr| match v {
        ValueExpr::Fixed(n) => (*n).max(0) as u32,
        _ => 1,
    };
    if let Some(n) = &mana.any_color {
        return fixed(n).max(1);
    }
    if let Some((_, n)) = &mana.one_of {
        return fixed(n).max(1);
    }
    mana.produces.iter().map(|(_, v)| fixed(v)).sum::<u32>().max(1)
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
/// five; `produces` lists fixed colours; `one_of` offers its constrained subset (empty ⇒ all five).
fn mana_spec_colors(mana: &ManaSpec) -> Vec<Color> {
    let mut v: Vec<Color> = mana.produces.iter().map(|(c, _)| *c).collect();
    if mana.any_color.is_some() {
        v.extend([Color::White, Color::Blue, Color::Black, Color::Red, Color::Green]);
    }
    if let Some((colors, _)) = &mana.one_of {
        if colors.is_empty() {
            v.extend([Color::White, Color::Blue, Color::Black, Color::Red, Color::Green]);
        } else {
            v.extend(colors.iter().copied());
        }
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
    /// How many mana this unit yields — i.e. how many pips it may cover, **all of one committed
    /// colour** (CR — Resonating Lute's "two mana of any one colour"; 1 for a normal source). See
    /// [`select_payment`], which uses a unit up to `count` times and locks it to a single colour.
    count: u32,
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
            units.push(ManaUnit { source: None, colors: vec![*color], bonus: false, restricted: false, count: 1 });
        }
    }
    // Floating RESTRICTED pool mana (only when the cost allows it).
    if allow_restricted {
        for (color, n) in &state.player(p).mana_pool.restricted {
            for _ in 0..*n {
                units.push(ManaUnit { source: None, colors: vec![*color], bonus: false, restricted: true, count: 1 });
            }
        }
    }
    let sources = mana_sources(state, p);
    let bonus_colors = tap_bonus_colors(state, p);
    for (id, colors, count) in &sources {
        if !excluded.contains(id) {
            units.push(ManaUnit { source: Some(*id), colors: colors.clone(), bonus: false, restricted: false, count: *count });
        }
    }
    // Restricted mana sources (Hydro-Channeler, Resonating Lute) — tappable only for an I/S cast.
    if allow_restricted {
        for (id, colors, count) in restricted_mana_sources(state, p) {
            if !excluded.contains(&id) {
                units.push(ManaUnit { source: Some(id), colors, bonus: false, restricted: true, count });
            }
        }
    }
    if !bonus_colors.is_empty() {
        for (id, _, _) in &sources {
            if !excluded.contains(id) && state.computed(*id).is_creature() {
                for c in &bonus_colors {
                    units.push(ManaUnit { source: Some(*id), colors: vec![*c], bonus: true, restricted: false, count: 1 });
                }
            }
        }
    }
    units
}

/// The mutable state of a payment plan-in-progress (see [`select_payment`]): each unit's remaining
/// capacity (its `count`), its committed colour (a multi-mana unit locks to ONE colour, CR — "two
/// mana of any one colour"), the set of sources already tapped (the one-tap-one-ability guard), and
/// the pip→(unit, colour) assignments accumulated so far.
struct Claim {
    remaining: Vec<u32>,
    committed: Vec<Option<Color>>,
    used_source: std::collections::BTreeSet<ObjId>,
    plan: Vec<(usize, Color)>,
}

impl Claim {
    fn new(units: &[ManaUnit]) -> Self {
        Claim {
            remaining: units.iter().map(|u| u.count).collect(),
            committed: vec![None; units.len()],
            used_source: std::collections::BTreeSet::new(),
            plan: Vec::new(),
        }
    }

    /// Whether unit `i` can supply one more mana of `color` right now: it has capacity, produces the
    /// colour, isn't already committed to a different colour, and — for a base (non-bonus) unit that
    /// taps a source — that source isn't already tapped for a *different* mana ability (CR 605.3a).
    fn can_claim(&self, units: &[ManaUnit], i: usize, color: Color) -> bool {
        let u = &units[i];
        self.remaining[i] > 0
            && u.colors.contains(&color)
            && self.committed[i].is_none_or(|c| c == color)
            && match u.source {
                Some(src) if !u.bonus => self.committed[i].is_some() || !self.used_source.contains(&src),
                _ => true,
            }
    }

    fn claim(&mut self, units: &[ManaUnit], i: usize, color: Color) {
        self.remaining[i] -= 1;
        self.committed[i] = Some(color);
        if let (Some(src), false) = (units[i].source, units[i].bonus) {
            self.used_source.insert(src);
        }
        self.plan.push((i, color));
    }

    /// Claim one mana of any colour for a generic pip (CR 202.1): the first usable unit, contributing
    /// its committed colour if set, else its first producible colour. Returns false if none is usable.
    fn claim_generic(&mut self, units: &[ManaUnit]) -> bool {
        for i in 0..units.len() {
            let color = self.committed[i].or_else(|| units[i].colors.first().copied());
            if let Some(color) = color {
                if self.can_claim(units, i, color) {
                    self.claim(units, i, color);
                    return true;
                }
            }
        }
        false
    }
}

/// Greedily assign mana units to `cost`: coloured pips first (each from a unit producing that
/// colour), then hybrid/mono-hybrid, then generic from any remaining unit. A unit may cover up to
/// `count` pips, all of one committed colour (Resonating Lute's "two mana of any one colour"), and a
/// source is tapped for at most one mana ability (the one-tap guard in [`Claim`]). Returns the pip
/// assignments as `(unit, colour)` pairs (a unit index may repeat, once per pip it covers), or `None`
/// if unpayable.
fn select_payment(units: &[ManaUnit], cost: &ManaCost) -> Option<Vec<(usize, Color)>> {
    let mut c = Claim::new(units);
    // Coloured requirements (CR 202.1): each pip needs a matching-colour unit.
    for (color, need) in &cost.colored {
        for _ in 0..*need {
            let i = (0..units.len()).find(|&i| c.can_claim(units, i, *color))?;
            c.claim(units, i, *color);
        }
    }
    // Hybrid pips (CR 107.4e): each `{c1/c2}` is paid by a unit producing *either* colour (prefer
    // `c1`). Done after fixed colour pips so those aren't starved.
    for &(c1, c2) in &cost.hybrid {
        let (i, color) = (0..units.len())
            .find_map(|i| {
                if c.can_claim(units, i, c1) {
                    Some((i, c1))
                } else if c.can_claim(units, i, c2) {
                    Some((i, c2))
                } else {
                    None
                }
            })?;
        c.claim(units, i, color);
    }
    // Monocolour hybrid pips (CR 107.4f): each `{n/c}` is paid by ONE unit producing `c`, else by `n`
    // generic units. Prefer the colour side (uses fewer units, so it never starves a later
    // requirement). Done after the fixed/two-colour pips so those come first.
    for &(n, col) in &cost.mono_hybrid {
        if let Some(i) = (0..units.len()).find(|&i| c.can_claim(units, i, col)) {
            c.claim(units, i, col);
            continue;
        }
        for _ in 0..n {
            if !c.claim_generic(units) {
                return None;
            }
        }
    }
    // Generic: any remaining unit, contributing its committed/first colour (CR 202.1, generic = any).
    for _ in 0..cost.generic {
        if !c.claim_generic(units) {
            return None;
        }
    }
    Some(c.plan)
}

/// Resolve a cost's **phyrexian** pips (CR 107.4c — `{C/P}` = one mana of `C` OR 2 life) into a
/// phyrexian-free cost + a life total to pay, given the available mana `units` and `life`. Each pip is
/// paid by mana when a spare unit can still cover it after the rest of the cost (mana is **preferred**,
/// CR-agnostic here but the auto-pay convention), added as a coloured requirement; otherwise it's paid
/// by 2 life. The `auto` (auto-pay seat) flag applies the **no-suicide gate**: a life-payment is only
/// allowed if the resulting life stays `> 0` (a manual/UI seat may pay to `0`). Returns `None` if a pip
/// can be paid neither way, or if the phyrexian-free base still isn't mana-payable. The returned base
/// cost carries the mana-paid pips as ordinary coloured pips, so `select_payment`/`spend_from_pool`
/// need no phyrexian awareness — the whole feature funnels through this one resolver.
fn resolve_phyrexian(units: &[ManaUnit], cost: &ManaCost, life: i32, auto: bool) -> Option<(ManaCost, u32)> {
    if cost.phyrexian.is_empty() {
        return Some((cost.clone(), 0));
    }
    let mut base = cost.clone();
    let phy = std::mem::take(&mut base.phyrexian);
    let mut life_to_pay: u32 = 0;
    for &c in &phy {
        // Prefer mana: does adding this pip as a coloured requirement still select_payment-pay?
        let mut trial = base.clone();
        *trial.colored.entry(c).or_insert(0) += 1;
        if select_payment(units, &trial).is_some() {
            base = trial;
        } else {
            // Else pay 2 life, gated so an auto-pay seat never pays itself to ≤ 0.
            let after = life - (life_to_pay as i32 + 2);
            let ok = if auto { after > 0 } else { after >= 0 };
            if ok {
                life_to_pay += 2;
            } else {
                return None;
            }
        }
    }
    // The base (non-phyrexian requirements + mana-paid pips) must be fully mana-payable.
    select_payment(units, &base).map(|_| (base, life_to_pay))
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
    let units = payment_units(state, p, excluded, allow_restricted);
    if cost.phyrexian.is_empty() {
        return select_payment(&units, cost).is_some();
    }
    // Phyrexian costs are affordable if every pip can be covered by mana or a no-suicide life payment
    // (the offer gate uses the auto-pay/no-suicide rule so an RL seat is never offered a lethal cast).
    resolve_phyrexian(&units, cost, state.player(p).life, true).is_some()
}

/// The total mana `p` could put toward a cost right now — floating pool + every untapped source's
/// base output (respecting a multi-mana source's per-tap count) + each creature's TapCreatureForMana
/// bonus. A loose upper bound for the `{X}` choice. (Excludes restricted mana — a conservative
/// under-count; no `{X}` spell in the pool spends it.)
pub fn available_mana(state: &GameState, p: PlayerId) -> u32 {
    payment_units(state, p, &[], false).iter().map(|u| u.count).sum()
}

/// The untapped mana sources `p` controls right now, each paired with the colours it can tap for —
/// the set a UI session enumerates as manual `ActivateMana` actions at priority (CR 605.3a). Same
/// sources the auto-payer draws from (tapped + summoning-sick sources already filtered out); pure
/// floating pool mana is NOT included (it isn't produced by a tap).
pub fn usable_mana_sources(state: &GameState, p: PlayerId) -> Vec<(ObjId, Vec<Color>)> {
    // The manual (UI) path INCLUDES cost-bearing mana abilities (a Treasure's `{T},Sacrifice`), which
    // the auto-pay pool excludes — a human can choose to activate them (paying the extra cost). The
    // per-tap count is dropped here (the UI offers colours; `produce_mana` re-derives the count).
    mana_sources_kind(state, p, false, true).into_iter().map(|(id, cs, _)| (id, cs)).collect()
}

/// Manually activate `source`'s mana ability for `color` (CR 605.3 — a mana ability, no stack): tap
/// it, add its full per-tap output of `color` to `p`'s pool (respecting a multi-mana source's count —
/// Resonating Lute's "add two"), plus any `TapCreatureForMana` bonus when `source` is a creature
/// (Badgermole, CR 605.1b). Returns false (changing nothing) if `source` isn't a current untapped
/// usable *unrestricted* mana source of `p` able to produce `color`. The caller floats this in the
/// pool (CR 106.4) and spends it on the next cost — letting a human pick which sources fund a spell
/// (#36). (Restricted granted mana is auto-pay-only for now — see the module note on `produce_mana`.)
pub fn produce_mana(state: &mut GameState, p: PlayerId, source: ObjId, color: Color) -> bool {
    use std::collections::BTreeMap;
    // Validate against the live source set so a stale/illegal request is a no-op (the engine only
    // ever offers legal sources, but this keeps the primitive safe in isolation). The count is this
    // source's per-tap output for the requested colour (1 for a normal source).
    let count = match mana_sources_kind(state, p, false, true).into_iter().find(|(id, _, _)| *id == source) {
        Some((_, cs, n)) if cs.contains(&color) => n,
        _ => return false,
    };
    if let Some(o) = state.objects.get_mut(&source) {
        o.status.tapped = true;
    }
    let mut additions: BTreeMap<Color, u32> = BTreeMap::new();
    *additions.entry(color).or_insert(0) += count;
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
    // Resolve phyrexian pips (CR 107.4c) into a phyrexian-free base cost + life to pay: mana-paid pips
    // become coloured requirements, life-paid pips are deducted below (no-suicide gate for this seat).
    let (cost, life_to_pay) = resolve_phyrexian(&units, cost, state.player(p).life, true)?;
    let cost = &cost;
    let chosen = match select_payment(&units, cost) {
        Some(c) => c,
        None => return None,
    };
    // The distinct colours of the mana this plan spends (CR 702.75 Converge).
    let colors_spent: Vec<Color> =
        chosen.iter().map(|&(_, c)| c).collect::<BTreeSet<Color>>().into_iter().collect();
    // Per tapped source, the chosen BASE unit's committed colour, FULL per-tap output count, and
    // restricted flag (the one-tap-one-ability guard ⇒ at most one base unit per source is chosen).
    // Captured BEFORE tapping, since tapping removes the source from `mana_sources`.
    let mut base_plan: BTreeMap<ObjId, (Color, u32, bool)> = BTreeMap::new();
    for &(i, c) in &chosen {
        let u = &units[i];
        if let Some(src) = u.source.filter(|_| !u.bonus) {
            base_plan.entry(src).or_insert((c, u.count, u.restricted));
        }
    }
    // Fallback for a creature tapped ONLY for its TapCreatureForMana bonus (its base mana is still
    // produced): the source's first base unit's colour/count/restriction.
    let mut base_fallback: BTreeMap<ObjId, (Color, u32, bool)> = BTreeMap::new();
    for u in &units {
        if let Some(src) = u.source.filter(|_| !u.bonus) {
            base_fallback.entry(src).or_insert((
                u.colors.first().copied().unwrap_or(Color::Colorless),
                u.count,
                u.restricted,
            ));
        }
    }
    // Tap every distinct source backing a chosen produced unit.
    let taps: BTreeSet<ObjId> = chosen.iter().filter_map(|&(i, _)| units[i].source).collect();
    for &id in &taps {
        if let Some(o) = state.objects.get_mut(&id) {
            o.status.tapped = true;
        }
    }
    // Add each tapped source's FULL real output to the pool: its base mana (`count` × the committed
    // colour, into the `restricted` bucket for a restricted ability, so surplus floats there — CR
    // 106.4), plus the TapCreatureForMana bonus per creature tapped (always unrestricted).
    let bonus_colors = tap_bonus_colors(state, p);
    let mut additions: BTreeMap<Color, u32> = BTreeMap::new();
    let mut restricted_additions: BTreeMap<Color, u32> = BTreeMap::new();
    for &id in &taps {
        if let Some((color, count, restricted)) =
            base_plan.get(&id).or_else(|| base_fallback.get(&id)).copied()
        {
            let bucket = if restricted { &mut restricted_additions } else { &mut additions };
            *bucket.entry(color).or_insert(0) += count;
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
    // Phyrexian pips paid with life (CR 107.4c): deduct 2 life each. The mana-paid pips were folded
    // into `cost`'s coloured requirements above and spent from the pool.
    if life_to_pay > 0 {
        state.player_mut(p).life -= life_to_pay as i32;
    }
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
    // Hybrid pips (CR 107.4e): each `{c1/c2}` cost one mana of either colour — deduct it, c1
    // preferred to mirror `select_payment`. (Omitting these leaked one floating mana per hybrid
    // pip: the "cast 6 mana off 5 lands" bug.)
    for &(c1, c2) in &cost.hybrid {
        let mut left = 1u32;
        for color in [c1, c2] {
            if left == 0 {
                break;
            }
            if allow_restricted {
                left = take_color(&mut state.player_mut(p).mana_pool.restricted, color, left);
            }
            if left > 0 {
                left = take_color(&mut state.player_mut(p).mana_pool.amounts, color, left);
            }
        }
    }
    // Monocolour hybrid pips (CR 107.4f): `{n/c}` cost one `c`, else `n` generic — colour-first,
    // mirroring `select_payment`.
    for &(n, col) in &cost.mono_hybrid {
        let mut left = 1u32;
        if allow_restricted {
            left = take_color(&mut state.player_mut(p).mana_pool.restricted, col, left);
        }
        if left > 0 {
            left = take_color(&mut state.player_mut(p).mana_pool.amounts, col, left);
        }
        if left > 0 {
            let mut gen_left = n;
            if allow_restricted {
                gen_left = take_any(&mut state.player_mut(p).mana_pool.restricted, gen_left);
            }
            take_any(&mut state.player_mut(p).mana_pool.amounts, gen_left);
        }
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
                        one_of: None,
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
                            one_of: None,
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
                        one_of: None,
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

    /// Add `n` Swamps (black sources) to P0's battlefield.
    fn add_swamps(state: &mut GameState, n: usize) {
        let db = state.card_db();
        for _ in 0..n {
            let c = db.get(grp::SWAMP).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield);
        }
    }

    fn pool_is_empty(state: &GameState, p: PlayerId) -> bool {
        let pool = &state.player(p).mana_pool;
        pool.amounts.values().all(|&v| v == 0) && pool.restricted.values().all(|&v| v == 0)
    }

    #[test]
    fn phyrexian_affordable_by_color_or_life() {
        // CR 107.4c: `{B/P}` is payable by one black mana OR 2 life.
        let bp = ManaCost { phyrexian: vec![Color::Black], ..Default::default() };
        // A Swamp covers the colour side.
        let mut with_swamp = game_with_lands(0, 0);
        add_swamps(&mut with_swamp, 1);
        assert!(can_pay(&with_swamp, PlayerId(0), &bp));
        // No black source but plenty of life → payable via 2 life.
        assert!(can_pay(&game_with_lands(0, 0), PlayerId(0), &bp));
        // No black source and only 1 life → the no-suicide gate blocks the life payment.
        let mut broke = game_with_lands(0, 0);
        broke.players[0].life = 1;
        assert!(!can_pay(&broke, PlayerId(0), &bp), "can't pay 2 life from 1 (would go ≤ 0)");
    }

    #[test]
    fn phyrexian_mana_paid_empties_pool_and_keeps_life() {
        // Dismember {1}{B/P}{B/P} paid entirely with mana: 1 Forest ({1}) + 2 Swamps (the two {B/P}).
        let mut state = game_with_lands(1, 0);
        add_swamps(&mut state, 2);
        let life_before = state.players[0].life;
        let cost = crate::cards::mana_cost_phyrexian(1, &[], &[Color::Black, Color::Black]);
        assert!(can_pay(&state, PlayerId(0), &cost));
        assert!(auto_pay(&mut state, PlayerId(0), &cost).is_some());
        assert!(pool_is_empty(&state, PlayerId(0)), "no floating mana leaks after an exact-cost phyrexian cast");
        assert_eq!(state.players[0].life, life_before, "mana-paid phyrexian costs no life");
    }

    #[test]
    fn phyrexian_life_paid_empties_pool_and_deducts_life() {
        // Same cost, but no black source: 1 Forest pays {1}; the two {B/P} are paid with 2 life each.
        let mut state = game_with_lands(1, 0);
        let life_before = state.players[0].life;
        let cost = crate::cards::mana_cost_phyrexian(1, &[], &[Color::Black, Color::Black]);
        assert!(can_pay(&state, PlayerId(0), &cost));
        assert!(auto_pay(&mut state, PlayerId(0), &cost).is_some());
        assert!(pool_is_empty(&state, PlayerId(0)), "no floating mana leaks when phyrexian pips are life-paid");
        assert_eq!(state.players[0].life, life_before - 4, "two {{B/P}} paid with 4 life total");
    }

    #[test]
    fn phyrexian_prefers_mana_then_life_for_the_shortfall() {
        // {B/P}{B/P} with a single Swamp: one pip is mana-paid, the other life-paid (2 life).
        let mut state = game_with_lands(0, 0);
        add_swamps(&mut state, 1);
        let life_before = state.players[0].life;
        let cost = ManaCost { phyrexian: vec![Color::Black, Color::Black], ..Default::default() };
        assert!(auto_pay(&mut state, PlayerId(0), &cost).is_some());
        assert!(pool_is_empty(&state, PlayerId(0)), "the mana-paid pip's Swamp mana is fully spent");
        assert_eq!(state.players[0].life, life_before - 2, "only the uncovered pip cost life");
    }

    #[test]
    fn phyrexian_no_suicide_gate_at_exactly_lethal_life() {
        // {B/P}{B/P} with no black source costs 4 life. At 4 life that's lethal → not offered/payable;
        // at 5 life it's fine (5 − 4 = 1 > 0).
        let cost = ManaCost { phyrexian: vec![Color::Black, Color::Black], ..Default::default() };
        let mut lethal = game_with_lands(0, 0);
        lethal.players[0].life = 4;
        assert!(!can_pay(&lethal, PlayerId(0), &cost), "paying yourself to exactly 0 is not auto-offered");
        assert!(auto_pay(&mut lethal, PlayerId(0), &cost).is_none());
        let mut ok = game_with_lands(0, 0);
        ok.players[0].life = 5;
        assert!(can_pay(&ok, PlayerId(0), &cost));
        assert!(auto_pay(&mut ok, PlayerId(0), &cost).is_some());
        assert_eq!(ok.players[0].life, 1, "5 − 4 life = 1");
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
                        one_of: None,
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
