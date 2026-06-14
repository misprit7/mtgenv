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
            let mut colors = producible_colors(state, def, p);
            // CR 305.6: any land with a basic land type has an intrinsic `{T}: Add <colour>`
            // ability per type, NOT authored on the card. We read the COMPUTED subtypes
            // (post-layer-system) so type-changing effects flow through for free — an animated
            // land keeps its mana, Spreading Seas / Urborg-style subtype changes are honoured.
            for st in &computed.subtypes {
                if let Some(c) = basic_land_type_color(st) {
                    if !colors.contains(&c) {
                        colors.push(c);
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
fn producible_colors(state: &GameState, def: &crate::cards::CardDef, p: PlayerId) -> Vec<Color> {
    let mut colors: Vec<Color> = Vec::new();
    let push = |c: Color, v: &mut Vec<Color>| {
        if !v.contains(&c) {
            v.push(c);
        }
    };
    for ab in &def.abilities {
        if let Ability::Activated {
            effect: Effect::AddMana { mana, .. },
            restriction,
            is_mana: true,
            ..
        } = ab
        {
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
fn payment_units(state: &GameState, p: PlayerId, excluded: &[ObjId]) -> Vec<ManaUnit> {
    let mut units: Vec<ManaUnit> = Vec::new();
    // Floating pool mana — spent first (CR 106.4: it stays in the pool until end of step).
    for (color, n) in &state.player(p).mana_pool.amounts {
        for _ in 0..*n {
            units.push(ManaUnit { source: None, colors: vec![*color], bonus: false });
        }
    }
    let sources = mana_sources(state, p);
    let bonus_colors = tap_bonus_colors(state, p);
    for (id, colors) in &sources {
        if !excluded.contains(id) {
            units.push(ManaUnit { source: Some(*id), colors: colors.clone(), bonus: false });
        }
    }
    if !bonus_colors.is_empty() {
        for (id, _) in &sources {
            if !excluded.contains(id) && state.computed(*id).is_creature() {
                for c in &bonus_colors {
                    units.push(ManaUnit { source: Some(*id), colors: vec![*c], bonus: true });
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
/// TapCreatureForMana bonus. See [`can_pay_excluding`] to omit a cost-committed source.
pub fn can_pay(state: &GameState, p: PlayerId, cost: &ManaCost) -> bool {
    can_pay_excluding(state, p, cost, &[])
}

/// As [`can_pay`], but with `excluded` sources removed from the mana set — for affordability of a
/// cost whose non-mana components (e.g. a `{T}` TapSelf) commit those permanents before the mana is
/// paid, so they can't double as mana sources (#57).
pub fn can_pay_excluding(
    state: &GameState,
    p: PlayerId,
    cost: &ManaCost,
    excluded: &[ObjId],
) -> bool {
    select_payment(&payment_units(state, p, excluded), cost).is_some()
}

/// The total mana `p` could put toward a cost right now — floating pool + every untapped source's
/// base + each creature's TapCreatureForMana bonus. A loose upper bound for the `{X}` choice.
pub fn available_mana(state: &GameState, p: PlayerId) -> u32 {
    payment_units(state, p, &[]).len() as u32
}

/// The untapped mana sources `p` controls right now, each paired with the colours it can tap for —
/// the set a UI session enumerates as manual `ActivateMana` actions at priority (CR 605.3a). Same
/// sources the auto-payer draws from (tapped + summoning-sick sources already filtered out); pure
/// floating pool mana is NOT included (it isn't produced by a tap).
pub fn usable_mana_sources(state: &GameState, p: PlayerId) -> Vec<(ObjId, Vec<Color>)> {
    mana_sources(state, p)
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
pub fn auto_pay(state: &mut GameState, p: PlayerId, cost: &ManaCost) -> bool {
    use std::collections::{BTreeMap, BTreeSet};
    let units = payment_units(state, p, &[]);
    let chosen = match select_payment(&units, cost) {
        Some(c) => c,
        None => return false,
    };
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
    // Tap every distinct source backing a chosen produced unit.
    let taps: BTreeSet<ObjId> = chosen.iter().filter_map(|&(i, _)| units[i].source).collect();
    for &id in &taps {
        if let Some(o) = state.objects.get_mut(&id) {
            o.status.tapped = true;
        }
    }
    // Add each tapped source's FULL real output to the pool: its base mana (in the colour the plan
    // assigned it, else its first colour for a bonus-only tap), plus the bonus per creature tapped.
    let bonus_colors = tap_bonus_colors(state, p);
    let mut additions: BTreeMap<Color, u32> = BTreeMap::new();
    for &id in &taps {
        let base = base_color_of
            .get(&id)
            .copied()
            .or_else(|| source_base_colors.get(&id).and_then(|cs| cs.first().copied()));
        if let Some(c) = base {
            *additions.entry(c).or_insert(0) += 1;
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
    spend_from_pool(state, p, cost);
    true
}

/// Deduct `cost` from `p`'s mana pool: each coloured pip from its colour, then the generic component
/// from any remaining mana (CR 202.1). Assumes affordability was checked (the pool covers it). Drops
/// emptied colour entries so the pool stays canonical for the view.
fn spend_from_pool(state: &mut GameState, p: PlayerId, cost: &ManaCost) {
    let pool = &mut state.player_mut(p).mana_pool.amounts;
    for (color, need) in &cost.colored {
        let avail = pool.get(color).copied().unwrap_or(0);
        let spent = (*need).min(avail);
        if let Some(v) = pool.get_mut(color) {
            *v -= spent;
        }
    }
    let mut generic_left = cost.generic;
    for v in pool.values_mut() {
        if generic_left == 0 {
            break;
        }
        let spent = (*v).min(generic_left);
        *v -= spent;
        generic_left -= spent;
    }
    pool.retain(|_, v| *v > 0);
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
        ManaCost { generic, colored, x: 0 }
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

        let paid = auto_pay(&mut state, PlayerId(0), &cost(0, &[(Color::Green, 1)]));
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
        assert!(auto_pay(&mut state, PlayerId(0), &cost(2, &[(Color::Green, 1)])));
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
        assert!(auto_pay(&mut s2, PlayerId(0), &cost(2, &[(Color::Green, 2)])));
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
        assert!(auto_pay(&mut state, PlayerId(0), &cost(0, &[(Color::Green, 1)])));
        assert_eq!(green(&state), 1, "the Badgermole bonus {{G}} floats after paying {{G}}");
        assert!(state.object(dork).status.tapped, "the dork is now tapped");
        // Second {G} in the same step: paid from the FLOATING mana — the dork is already tapped, so
        // there is no other source; this only succeeds because floating mana persisted.
        assert!(auto_pay(&mut state, PlayerId(0), &cost(0, &[(Color::Green, 1)])));
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
        assert!(auto_pay(&mut state, PlayerId(0), &cost(1, &[(Color::Green, 1)])));
        let untapped_after = state.player(PlayerId(0)).battlefield.iter().filter(|&&id| !state.objects[&id].status.tapped).count();
        assert_eq!(untapped_after, 2, "two lands tapped to pay {{1}}{{G}}");
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
