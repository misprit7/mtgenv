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
use crate::effects::ability::Keyword;
use crate::ids::{ObjId, PlayerId};
use crate::state::GameState;

/// The untapped mana sources `p` controls: `(permanent, colours it can tap for)`. A basic
/// land produces exactly one colour.
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
            let def = state.card_db.get(o.chars.grp_id)?;
            if !def.is_mana_source() {
                return None;
            }
            // CR 302.6: a creature's `{T}` mana ability can't be activated while it's summoning
            // sick (unless it has haste). Lands/artifacts are never sick, so this only gates
            // creature mana dorks (Llanowar Elves). The simplified mana model assumes `{T}`.
            if o.summoning_sick && !state.computed(id).has_keyword(Keyword::Haste) {
                return None;
            }
            Some((id, def.mana_colors.clone()))
        })
        .collect()
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
    for i in chosen {
        let id = sources[i].0;
        if let Some(o) = state.objects.get_mut(&id) {
            o.status.tapped = true;
        }
    }
    true
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
            abilities: Vec::new(),
            mana_colors: vec![Color::Green],
            text: String::new(),
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
}
