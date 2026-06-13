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
        subtypes: c.subtypes.clone(),
        supertypes: c.supertypes.clone(),
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
    }
}

/// A fully-perceived object (public zones + the viewer's own hand).
fn visible(o: &Object, db: &CardDb) -> ObjView {
    ObjView::Visible {
        id: o.id,
        chars: chars_view(&o.chars, db),
        controller: o.controller,
        owner: o.owner,
        zone: o.zone,
        status: o.status,
        counters: o.counters.clone(),
        damage_marked: o.damage_marked,
        attachments: Vec::new(),
        summoning_sick: o.summoning_sick,
    }
}

fn obj_views<'a>(state: &'a GameState, ids: impl IntoIterator<Item = &'a ObjId>) -> Vec<ObjView> {
    ids.into_iter()
        .filter_map(|id| state.objects.get(id))
        .map(|o| visible(o, &state.card_db))
        .collect()
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
