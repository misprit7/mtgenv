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

/// Greedily select which sources to use to pay `cost`: coloured pips first (each from a
/// source that can produce that colour), then the generic component from any remaining
/// source. Returns the chosen sources' indices into `sources`, or `None` if unpayable.
fn select_payment(sources: &[(ObjId, Vec<Color>)], cost: &ManaCost) -> Option<Vec<usize>> {
    let mut used = vec![false; sources.len()];
    // Coloured requirements (CR 202.1): each pip needs a matching-colour source.
    for (color, need) in &cost.colored {
        let mut got = 0;
        for (i, (_, colors)) in sources.iter().enumerate() {
            if got == *need {
                break;
            }
            if !used[i] && colors.contains(color) {
                used[i] = true;
                got += 1;
            }
        }
        if got < *need {
            return None;
        }
    }
    // Generic: any remaining source (CR 202.1, generic can be paid with any mana).
    let mut generic_left = cost.generic;
    for (i, u) in used.iter_mut().enumerate() {
        let _ = i;
        if generic_left == 0 {
            break;
        }
        if !*u {
            *u = true;
            generic_left -= 1;
        }
    }
    if generic_left > 0 {
        return None;
    }
    Some(
        used.iter()
            .enumerate()
            .filter_map(|(i, &u)| if u { Some(i) } else { None })
            .collect(),
    )
}

/// Whether `p` can pay `cost` from currently-untapped mana sources (CR 118.3).
pub fn can_pay(state: &GameState, p: PlayerId, cost: &ManaCost) -> bool {
    let sources = mana_sources(state, p);
    select_payment(&sources, cost).is_some()
}

/// The total mana `p` could produce right now (one per untapped mana source). A loose upper
/// bound used to bound the `{X}` choice (CR 107.3 — colour constraints aren't modeled here).
pub fn available_mana(state: &GameState, p: PlayerId) -> u32 {
    mana_sources(state, p).len() as u32
}

/// Pay `cost` by tapping a sufficient set of `p`'s mana sources (CR 605.3a / 601.2g-h).
/// Returns false (tapping nothing) if the cost can't be paid. `{0}` is always payable
/// (CR 118.3a).
pub fn auto_pay(state: &mut GameState, p: PlayerId, cost: &ManaCost) -> bool {
    let sources = mana_sources(state, p);
    let chosen = match select_payment(&sources, cost) {
        Some(c) => c,
        None => return false,
    };
    let tapped: Vec<ObjId> = chosen.iter().map(|&i| sources[i].0).collect();
    for &id in &tapped {
        if let Some(o) = state.objects.get_mut(&id) {
            o.status.tapped = true;
        }
    }
    fire_tap_creature_for_mana(state, p, &tapped);
    true
}

/// Fire any "whenever you tap a creature for mana, add …" no-stack triggered mana abilities
/// (CR 605.1b) — once per creature among the just-tapped `tapped` sources (an animated land that's
/// a land-creature counts). The produced mana goes straight into `p`'s pool (mana abilities don't
/// use the stack). Card-agnostic: reads `Triggered{ TapCreatureForMana, AddMana{..} }` abilities.
fn fire_tap_creature_for_mana(state: &mut GameState, p: PlayerId, tapped: &[ObjId]) {
    let creature_taps = tapped.iter().filter(|&&id| state.computed(id).is_creature()).count() as u32;
    if creature_taps == 0 {
        return;
    }
    let db = state.card_db();
    let battlefield = state.player(p).battlefield.clone();
    let mut bonus: Vec<Color> = Vec::new();
    for id in battlefield {
        let grp = match state.objects.get(&id) {
            Some(o) => o.chars.grp_id,
            None => continue,
        };
        let Some(def) = db.get(grp) else { continue };
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
    for c in bonus {
        *state.player_mut(p).mana_pool.amounts.entry(c).or_insert(0) += creature_taps;
    }
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
