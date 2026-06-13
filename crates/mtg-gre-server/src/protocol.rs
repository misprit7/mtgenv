//! The **JSON projection** of the boundary types carried over the WebSocket (CLIENT_PLAN §5/§6,
//! milestone 2). This is *not* protobuf — that's M3. The boundary types already derive `serde`,
//! so the projection is mostly the boundary types verbatim plus a thin envelope:
//!
//! - server→client: a [`ServerMsg`] for each state push ([`ServerMsg::Event`]) and each decision
//!   prompt ([`ServerMsg::Decide`], carrying the flat [`Prompt`]); and
//! - client→server: a [`ClientMsg::Response`] selecting among the enumerated options.
//!
//! One outstanding decision exists at a time (the engine is single-threaded), but each prompt
//! still carries an `id` the client echoes — the JSON sibling of GRE's `msgId`→`respId`
//! correlation (CLIENT_PLAN §4.3).

use crate::options::Prompt;
use mtg_core::agent::{GameEvent, PlayerView};
use mtg_core::ids::PlayerId;
use serde::{Deserialize, Serialize};

/// Server → client. Two channels over one socket: pushes ([`Event`](ServerMsg::Event)) and
/// prompts ([`Decide`](ServerMsg::Decide)), mirroring the engine's `observe()` / `decide()`.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ServerMsg {
    /// A public/seat-visible event, with the refreshed (information-filtered) view.
    Event {
        event: GameEvent,
        view: PlayerView,
    },
    /// A decision point: render `prompt`'s enumerated options (the masking) and reply with a
    /// [`ClientMsg::Response`] carrying the same `id`.
    Decide {
        id: u64,
        prompt: Prompt,
        view: PlayerView,
    },
    /// The game ended.
    GameOver {
        winner: Option<PlayerId>,
    },
    /// A free-text server log line (diagnostics).
    Log {
        text: String,
    },
}

/// Client → server. The only inbound message: a selection answering the current prompt.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ClientMsg {
    Response {
        id: u64,
        #[serde(default)]
        picks: Vec<u32>,
        #[serde(default)]
        number: Option<i64>,
        #[serde(default)]
        pass: bool,
        #[serde(default)]
        order: Vec<u32>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::options::{self, Selection};
    use expect_test::expect;
    use mtg_core::agent::{
        DecisionRequest, DecisionResponse, PlayableAction, PlayerPrivateView, SelectReason,
    };
    use mtg_core::basics::Phase;
    use mtg_core::ids::ObjId;

    fn tiny_view() -> PlayerView {
        PlayerView {
            seat: PlayerId(0),
            turn: 1,
            active_player: PlayerId(0),
            phase: Phase::PrecombatMain,
            priority_player: Some(PlayerId(0)),
            players: vec![],
            me: PlayerPrivateView {
                hand: vec![],
                known_library: vec![],
                revealed_to_me: vec![],
            },
            battlefield: vec![],
            stack: vec![],
            combat: None,
            stops: None,
        }
    }

    /// The exact server→client `decide` wire frame for a priority decision (the JSON projection
    /// of the boundary, CLIENT_PLAN §5). This snapshot is living protocol documentation: it shows
    /// the envelope tag, the flat `prompt` (enumerated legal options = the masking), and the
    /// information-filtered `view`.
    #[test]
    fn decide_frame_wire_shape() {
        let view = tiny_view();
        let req = DecisionRequest::Priority {
            actions: vec![PlayableAction::PlayLand { card: ObjId(1) }],
            can_pass: true,
        };
        let prompt = options::prompt_for(&view, &req);
        let json = serde_json::to_string_pretty(&ServerMsg::Decide { id: 7, prompt, view }).unwrap();
        expect![[r#"
            {
              "type": "decide",
              "id": 7,
              "prompt": {
                "title": "Priority — choose an action",
                "mode": "action",
                "options": [
                  "Play land — #1"
                ],
                "optionObjs": [
                  1
                ],
                "canPass": true,
                "min": 0,
                "max": 1,
                "numMin": 0,
                "numMax": 0
              },
              "view": {
                "seat": 0,
                "turn": 1,
                "active_player": 0,
                "phase": "PrecombatMain",
                "priority_player": 0,
                "players": [],
                "me": {
                  "hand": [],
                  "known_library": [],
                  "revealed_to_me": []
                },
                "battlefield": [],
                "stack": [],
                "combat": null,
                "stops": null
              }
            }"#]].assert_eq(&json);
    }

    /// A representative `selectMany` prompt (discard to hand size) — documents the bounds fields.
    #[test]
    fn select_cards_prompt_wire_shape() {
        let view = tiny_view();
        let req = DecisionRequest::SelectCards {
            reason: SelectReason::DiscardToHandSize,
            from: vec![ObjId(1), ObjId(2), ObjId(3)],
            min: 1,
            max: 1,
            description: "discard down to 7 cards".into(),
        };
        let json = serde_json::to_string_pretty(&options::prompt_for(&view, &req)).unwrap();
        expect![[r##"
            {
              "title": "discard down to 7 cards (DiscardToHandSize)",
              "mode": "selectMany",
              "options": [
                "#1",
                "#2",
                "#3"
              ],
              "optionObjs": [
                1,
                2,
                3
              ],
              "canPass": false,
              "min": 1,
              "max": 1,
              "numMin": 0,
              "numMax": 0
            }"##]].assert_eq(&json);
    }

    /// Round-trip: an inbound client `response` frame parses and maps back to the engine's
    /// `DecisionResponse` for the request it answered. Documents the client→server direction.
    #[test]
    fn client_response_round_trips() {
        let req = DecisionRequest::Priority {
            actions: vec![PlayableAction::PlayLand { card: ObjId(1) }],
            can_pass: true,
        };
        let map = |wire: &str| {
            let ClientMsg::Response {
                picks, number, pass, order, ..
            } = serde_json::from_str(wire).unwrap();
            options::response_from(
                &req,
                &Selection {
                    picks,
                    number,
                    pass,
                    order,
                },
            )
        };
        // Picking option 0 → play the first land.
        assert_eq!(
            map(r#"{"type":"response","id":7,"picks":[0]}"#),
            DecisionResponse::Action(0)
        );
        // Passing priority (omitted fields default).
        assert_eq!(
            map(r#"{"type":"response","id":7,"pass":true}"#),
            DecisionResponse::Pass
        );
    }
}
