//! `view_for(state, seat)` — the single, correct place to enforce hidden information
//! (WHITEBOARD_MODEL §2.6, AGENT_INTERFACE §2). Computes design's information-filtered
//! [`PlayerView`](crate::agent::PlayerView) from the full [`GameState`]: public zones are
//! shown, the seat's own hand is shown, opponents' hands/libraries collapse to counts.

use crate::agent::{
    CharacteristicsView, CombatView, ObjView, PlayerPrivateView, PlayerPublicView, PlayerView,
    StackObjView,
};
use crate::cards::CardDb;
use crate::ids::ObjId;
use crate::ids::PlayerId;
use crate::stack::{StackObject, StackObjectKind};

use super::{Characteristics, GameState, Object};

fn chars_view(c: &Characteristics, db: &CardDb) -> CharacteristicsView {
    CharacteristicsView {
        name: c.name.clone(),
        card_types: c.card_types.iter().map(|t| t.as_str().to_string()).collect(),
        // The view keeps the type line as plain strings (the wire format); Display renders each
        // enum to its canonical MTG token so the client/webui sees an unchanged JSON shape.
        subtypes: c.subtypes.iter().map(|s| s.to_string()).collect(),
        supertypes: c.supertypes.iter().map(|s| s.to_string()).collect(),
        colors: c.colors.clone(),
        mana_value: c.mana_value(),
        mana_cost: c.mana_cost.clone(),
        power: c.power,
        toughness: c.toughness,
        keywords: Vec::new(),
        // Oracle text from the card-data layer (CardDef.text), keyed by grp_id — kept out of
        // per-object state (it's static card data).
        rules_text: db.get(c.grp_id).map(|d| d.text.clone()).unwrap_or_default(),
        grp_id: c.grp_id,
        // Engine-fidelity flag (CardDef.fully_implemented), keyed by grp_id. `None` for objects
        // with no card data (engine-generated abilities/tokens) → the client shows no ⚠ marker.
        fully_implemented: db.get(c.grp_id).map(|d| d.fully_implemented),
    }
}

/// A fully-perceived object (public zones + the viewer's own hand). On the battlefield, the
/// shown power/toughness/keywords are the COMPUTED (layered, CR 613) values so the UI sees
/// anthems/counters/keyword grants; elsewhere the base characteristics are shown.
fn visible(state: &GameState, o: &Object) -> ObjView {
    let mut chars = chars_view(&o.chars, &state.card_db);
    if o.zone == crate::basics::Zone::Battlefield {
        let computed = state.computed(o.id);
        chars.power = computed.power;
        chars.toughness = computed.toughness;
        chars.card_types = computed.card_types.iter().map(|t| t.as_str().to_string()).collect();
        chars.colors = computed.colors.clone();
        chars.keywords = computed.keywords.iter().map(|k| format!("{k:?}")).collect();
    }
    // The objects attached to this one (auras/equipment on a host, CR 303/301.5) — so the UI can
    // render them behind their host. Battlefield only; iterated in battlefield order (stable).
    let attachments: Vec<ObjId> = state
        .players
        .iter()
        .flat_map(|p| &p.battlefield)
        .copied()
        .filter(|&id| state.objects.get(&id).and_then(|x| x.attached_to) == Some(o.id))
        .collect();
    ObjView::Visible {
        id: o.id,
        chars,
        controller: o.controller,
        owner: o.owner,
        zone: o.zone,
        status: o.status,
        counters: o.counters.clone(),
        damage_marked: o.damage_marked,
        attachments,
        summoning_sick: o.summoning_sick,
    }
}

fn obj_views<'a>(state: &'a GameState, ids: impl IntoIterator<Item = &'a ObjId>) -> Vec<ObjView> {
    ids.into_iter()
        .filter_map(|id| state.objects.get(id))
        .map(|o| visible(state, o))
        .collect()
}

/// Build `Visible` ObjViews for `ids` (skipping unknown ids). Used to surface the specific
/// candidate cards a `DecisionRequest` references — chiefly a Search/`SelectCards` drawing from the
/// hidden library — to the seat making the choice, which is entitled to see exactly those cards
/// (the rest of the hidden zone stays masked). See `Engine::reveal_request_objects`.
pub(crate) fn reveal_objects(state: &GameState, ids: &[ObjId]) -> Vec<ObjView> {
    obj_views(state, ids.iter())
}

fn stack_view(state: &GameState, s: &StackObject) -> StackObjView {
    let chars = match s.kind {
        StackObjectKind::Spell(id) => state
            .objects
            .get(&id)
            .map(|o| chars_view(&o.chars, &state.card_db))
            .unwrap_or_default(),
        StackObjectKind::Ability { .. } => CharacteristicsView {
            name: "Ability".to_string(),
            ..Default::default()
        },
        StackObjectKind::DelayedAbility { .. } => CharacteristicsView {
            name: "Delayed ability".to_string(),
            ..Default::default()
        },
    };
    StackObjView {
        id: s.id,
        controller: s.controller,
        source: s.source,
        chars,
        targets: s.targets.clone(),
    }
}

/// Build the information-filtered view for `seat`.
pub fn view_for(state: &GameState, seat: PlayerId) -> PlayerView {
    let players = state
        .players
        .iter()
        .map(|p| PlayerPublicView {
            player: p.id,
            life: p.life,
            poison: p.poison,
            hand_count: p.hand.len() as u32,
            library_count: p.library.len() as u32,
            graveyard: obj_views(state, &p.graveyard),
            exile_public: obj_views(state, &p.exile),
            mana_pool: p.mana_pool.clone(),
            counters: p.counters.clone(),
        })
        .collect();

    let me_player = state.player(seat);
    let me = PlayerPrivateView {
        hand: obj_views(state, &me_player.hand),
        known_library: Vec::new(),
        revealed_to_me: Vec::new(),
    };

    // The battlefield is public; show every permanent (CR 400.2).
    let battlefield = state
        .players
        .iter()
        .flat_map(|p| obj_views(state, &p.battlefield))
        .collect();

    let stack = state
        .stack
        .items
        .iter()
        .map(|s| stack_view(state, s))
        .collect();

    // Combat is public information (CR 506) when a combat phase is in progress.
    let combat = state.combat.as_ref().map(|c| CombatView {
        attackers: c.attackers.iter().map(|a| (a.attacker, a.defender)).collect(),
        blockers: c.blocks.iter().map(|b| (b.blocker, b.attacker)).collect(),
    });

    PlayerView {
        seat,
        turn: state.turn_number,
        active_player: state.active_player,
        phase: state.phase,
        priority_player: state.priority_player,
        players,
        me,
        battlefield,
        stack,
        combat,
        // Settings-echo, filled per-seat by `Engine::view_for_seat` (which has the StopConfig);
        // the bare masking function leaves it `None`.
        stops: None,
    }
}

/// An omniscient, no-hidden-information view of the whole game (REPLAY_PLAN) — for spectators and
/// replays. **Every zone of every player is fully visible**: hands face-up, libraries IN ORDER
/// (top-first), graveyards/exile, the battlefield and stack — all face-up. Reuses the same
/// `ObjView` machinery as [`view_for`], so the web board code renders a `GodView` directly.
/// Spectators aren't players, so showing them everything can't leak to a competitor.
pub fn god_view(state: &GameState) -> crate::replay::GodView {
    use crate::replay::GodPlayerView;
    let players = state
        .players
        .iter()
        .map(|p| GodPlayerView {
            player: p.id,
            life: p.life,
            poison: p.poison,
            mana_pool: p.mana_pool.clone(),
            counters: p.counters.clone(),
            hand: obj_views(state, &p.hand),
            // The library is stored with the top at the END (a draw is a `pop`); reverse so the
            // contract holds: `library[0]` is the top of the library.
            library: obj_views(state, p.library.iter().rev()),
            graveyard: obj_views(state, &p.graveyard),
            exile: obj_views(state, &p.exile),
        })
        .collect();
    let battlefield = state
        .players
        .iter()
        .flat_map(|p| obj_views(state, &p.battlefield))
        .collect();
    let stack = state.stack.items.iter().map(|s| stack_view(state, s)).collect();
    let combat = state.combat.as_ref().map(|c| CombatView {
        attackers: c.attackers.iter().map(|a| (a.attacker, a.defender)).collect(),
        blockers: c.blocks.iter().map(|b| (b.blocker, b.attacker)).collect(),
    });
    crate::replay::GodView {
        turn: state.turn_number,
        active_player: state.active_player,
        phase: state.phase,
        priority_player: state.priority_player,
        players,
        battlefield,
        stack,
        combat,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::CardDef;

    fn def(grp_id: u32, name: &str, fully_implemented: bool) -> CardDef {
        CardDef {
            chars: Characteristics { name: name.into(), grp_id, ..Default::default() },
            abilities: Vec::new(),
            text: String::new(),
            fully_implemented,
        }
    }

    #[test]
    fn fully_implemented_flag_flows_into_the_view() {
        // The CardDef fidelity flag is surfaced per-object in the view the client reads, keyed by
        // grp_id: `Some(true)`/`Some(false)` for real cards, `None` for engine-generated objects
        // (abilities/tokens) with no card data. The client renders ⚠ iff `Some(false)`.
        let mut db = CardDb::default();
        db.insert(def(1, "Faithful", true));
        db.insert(def(2, "Deferred", false));

        let full = chars_view(&db.get(1).unwrap().chars.clone(), &db);
        let partial = chars_view(&db.get(2).unwrap().chars.clone(), &db);
        // A characteristics whose grp_id isn't in the db (e.g. an engine token) → None.
        let no_card = chars_view(&Characteristics { grp_id: 9999, ..Default::default() }, &db);

        assert_eq!(full.fully_implemented, Some(true), "implemented card → Some(true), no marker");
        assert_eq!(partial.fully_implemented, Some(false), "deferred-clause card → Some(false), ⚠");
        assert_eq!(no_card.fully_implemented, None, "no card data → None, no marker");
    }

    #[test]
    fn attachments_list_objects_attached_to_a_host() {
        // The view exposes, per battlefield object, the ids of objects attached to it (auras /
        // equipment), so the client can render them behind their host.
        use crate::basics::{CardType, Zone};
        let mut state = GameState::new(2, 1);
        let host = state.add_card(
            PlayerId(0),
            Characteristics {
                name: "Bear".into(),
                card_types: vec![CardType::Creature],
                power: Some(2),
                toughness: Some(2),
                ..Default::default()
            },
            Zone::Battlefield,
        );
        let aura = state.add_card(
            PlayerId(0),
            Characteristics {
                name: "Pacifism".into(),
                card_types: vec![CardType::Enchantment],
                ..Default::default()
            },
            Zone::Battlefield,
        );
        state.objects.get_mut(&aura).unwrap().attached_to = Some(host);

        let attachments_of = |id| {
            god_view(&state).battlefield.into_iter().find_map(|o| match o {
                ObjView::Visible { id: oid, attachments, .. } if oid == id => Some(attachments),
                _ => None,
            })
        };
        assert_eq!(attachments_of(host), Some(vec![aura]), "host lists its aura");
        assert_eq!(attachments_of(aura), Some(vec![]), "the aura itself has no attachers");
    }
}
