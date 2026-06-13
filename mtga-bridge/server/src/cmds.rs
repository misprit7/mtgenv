//! FrontDoor command dispatch — the minimal set of `CmdType` responses that get
//! the real MTGA client from "FrontDoor connected" to an interactive home screen,
//! then into a bot match against our backend.
//!
//! Derived from the recovered protocol (CmdType values verified against the
//! decompiled `Wizards.Arena.Models.Network/CmdType.cs`; response shapes from the
//! `Wizards.Unification.Models.*` POCOs the client deserializes). All payloads are
//! sent uncompressed; collections are emitted as `[]`/`{}` rather than omitted,
//! because several client paths iterate without null-guards.

use crate::envelope::Cmd;

/// Verified `CmdType` enum values (decompiled `CmdType.cs`).
pub mod cmd_type {
    pub const AUTHENTICATE: i32 = 0;
    pub const START_HOOK: i32 = 1;
    pub const ATTACH: i32 = 5;
    pub const GET_FORMATS: i32 = 6;
    pub const DECK_GET_DECK_SUMMARIES_V3: i32 = 411;
    pub const CARD_GET_ALL_CARDS: i32 = 551;
    pub const EVENT_AI_BOT_MATCH: i32 = 612;
    pub const EVENT_GET_ACTIVE_MATCHES: i32 = 613;
    pub const EVENT_GET_COURSES_V2: i32 = 623;
    pub const EVENT_GET_ACTIVE_EVENTS_V2: i32 = 624;
    pub const CAROUSEL_GET_CAROUSEL_ITEMS: i32 = 704;
    pub const QUEST_GET_QUESTS: i32 = 1000;
    pub const RANK_GET_COMBINED_RANK_INFO: i32 = 1100;
    pub const RANK_GET_SEASON_AND_RANK_DETAILS: i32 = 1102;
    pub const PERIODIC_REWARDS_GET_STATUS: i32 = 1200;
    pub const GRAPH_GET_GRAPH_DEFINITIONS: i32 = 1700;
    pub const GRAPH_GET_GRAPH_STATE: i32 = 1701;
    pub const COSMETICS_GET_PLAYER_OWNED_COSMETICS: i32 = 1900;

    /// Best-effort human name for logging.
    pub fn name(t: i32) -> &'static str {
        match t {
            AUTHENTICATE => "Authenticate",
            START_HOOK => "StartHook",
            ATTACH => "Attach",
            GET_FORMATS => "GetFormats",
            DECK_GET_DECK_SUMMARIES_V3 => "DeckGetDeckSummariesV3",
            CARD_GET_ALL_CARDS => "CardGetAllCards",
            EVENT_AI_BOT_MATCH => "EventAiBotMatch",
            EVENT_GET_ACTIVE_MATCHES => "EventGetActiveMatches",
            EVENT_GET_COURSES_V2 => "EventGetCoursesV2",
            EVENT_GET_ACTIVE_EVENTS_V2 => "EventGetActiveEventsV2",
            CAROUSEL_GET_CAROUSEL_ITEMS => "CarouselGetCarouselItems",
            QUEST_GET_QUESTS => "QuestGetQuests",
            RANK_GET_COMBINED_RANK_INFO => "RankGetCombinedRankInfo",
            RANK_GET_SEASON_AND_RANK_DETAILS => "RankGetSeasonAndRankDetails",
            PERIODIC_REWARDS_GET_STATUS => "PeriodicRewardsGetStatus",
            GRAPH_GET_GRAPH_DEFINITIONS => "GraphGetGraphDefinitions",
            GRAPH_GET_GRAPH_STATE => "GraphGetGraphState",
            COSMETICS_GET_PLAYER_OWNED_COSMETICS => "CosmeticsGetPlayerOwnedCosmetics",
            _ => "Unknown",
        }
    }
}

/// A stub deck the client can select and start a bot match with.
pub const STUB_DECK_ID: &str = "11111111-1111-1111-1111-111111111111";

/// What the server should do in response to a FrontDoor `Cmd`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    /// JSON to send back as the `Response` payload (echoing the request transId).
    pub response_json: String,
    /// Whether to follow the response with an unsolicited `MatchCreated` push
    /// (true only for `EventAiBotMatch`).
    pub then_push_match: bool,
    /// True if we recognized the command; false → we sent a generic `{}` fallback.
    pub recognized: bool,
}

/// Build the response for a received FrontDoor command.
pub fn handle(cmd: &Cmd) -> Outcome {
    use cmd_type::*;
    let (json, push, recognized): (String, bool, bool) = match cmd.cmd_type {
        AUTHENTICATE => (auth_response(), false, true),
        START_HOOK => (start_hook_response(), false, true),
        GET_FORMATS => (r#"{"Formats":[],"FormatGroups":[]}"#.into(), false, true),
        RANK_GET_COMBINED_RANK_INFO => (combined_rank_response(), false, true),
        RANK_GET_SEASON_AND_RANK_DETAILS => ("{}".into(), false, true),
        EVENT_GET_ACTIVE_EVENTS_V2 => (
            r#"{"DynamicFilterTags":[],"CacheVersion":0,"Events":[],"Challenges":[],"AiBotMatches":[]}"#.into(),
            false,
            true,
        ),
        EVENT_GET_COURSES_V2 => (r#"{"Courses":[]}"#.into(), false, true),
        EVENT_GET_ACTIVE_MATCHES => (r#"{"MatchesV3":[]}"#.into(), false, true),
        QUEST_GET_QUESTS => (r#"{"canSwap":false,"newQuests":0,"quests":[]}"#.into(), false, true),
        PERIODIC_REWARDS_GET_STATUS => (periodic_rewards_response(), false, true),
        // ClientGraphDefinitionsResponse { List<ClientGraphDefinition> GraphDefinitions }.
        // Must be a non-null list or the Node Graph Manager NREs on init (black screen).
        GRAPH_GET_GRAPH_DEFINITIONS => (r#"{"GraphDefinitions":[]}"#.into(), false, true),
        GRAPH_GET_GRAPH_STATE => (r#"{"NodeStates":{},"MilestoneStates":{}}"#.into(), false, true),
        CARD_GET_ALL_CARDS => (r#"{}"#.into(), false, true),
        COSMETICS_GET_PLAYER_OWNED_COSMETICS => (r#"{}"#.into(), false, true),
        CAROUSEL_GET_CAROUSEL_ITEMS => (r#"{"Items":[]}"#.into(), false, true),
        DECK_GET_DECK_SUMMARIES_V3 => (deck_summaries_response(), false, true),
        EVENT_AI_BOT_MATCH => ("\"stub-match-1\"".into(), true, true),
        _ => ("{}".into(), false, false),
    };
    Outcome { response_json: json, then_push_match: push, recognized }
}

/// `FrontDoorSessionResp` — the auth gate. `Attached:true` + `PlayerConditions:0`
/// is what flips the client out of the queued state and fires `OnConnected`.
fn auth_response() -> String {
    r#"{"SessionId":"stub-session","Attached":true,"PlayerConditions":0,"ExternalChatAppId":""}"#.into()
}

/// `StartHookResponseV2` — the primary boot gate. One selectable deck.
fn start_hook_response() -> String {
    format!(
        r#"{{"DeckSummaries":[{summary}],"Decks":{{"{id}":{deck}}},"UpdatedGraphs":{{}},"HomePageAchievements":{{"Claimable":[],"Favorites":[],"CloseToComplete":[],"RecentlyProgressed":[],"OneShots":[],"SparseStates":{{}}}},"DeckLimit":0,"ServerTime":"2026-01-01T00:00:00+00:00"}}"#,
        summary = deck_summary_json(),
        id = STUB_DECK_ID,
        deck = deck_json(),
    )
}

fn deck_summaries_response() -> String {
    format!(r#"{{"Summaries":[{}]}}"#, deck_summary_json())
}

fn deck_summary_json() -> String {
    format!(
        r#"{{"DeckId":"{id}","Name":"Stub Deck","Mana":"","Attributes":[],"DeckTileId":0,"IsCompanionValid":false,"IsNetDeck":false,"checkInbox":false}}"#,
        id = STUB_DECK_ID,
    )
}

/// A minimal mono-color deck. grpId 70262 is a placeholder; only matters once a
/// real match starts (the GRE channel), not for the home screen.
fn deck_json() -> String {
    r#"{"MainDeck":[{"cardId":70262,"quantity":60}],"Sideboard":[],"CommandZone":[],"Companions":[],"CardSkins":[]}"#.into()
}

fn combined_rank_response() -> String {
    r#"{"playerId":"","constructedClass":0,"constructedLevel":0,"constructedStep":0,"limitedClass":0,"limitedLevel":0,"limitedStep":0,"constructedPercentile":0.0,"limitedPercentile":0.0}"#.into()
}

fn periodic_rewards_response() -> String {
    r#"{"_dailyRewardSequenceId":0,"_dailyRewardResetTimestamp":"2026-01-01T00:00:00","_weeklyRewardSequenceId":0,"_weeklyRewardResetTimestamp":"2026-01-01T00:00:00","_dailyRewardChestDescriptions":{},"_weeklyRewardChestDescriptions":{}}"#.into()
}

/// Build the unsolicited `MatchCreated` PushNotification JSON that hands the
/// client our GRE match endpoint. Sent as a `Response` envelope with an empty
/// transId after answering `EventAiBotMatch`.
pub fn match_created_push(host: &str, port: u16, match_id: &str) -> String {
    format!(
        r#"{{"Type":"MatchCreated","MatchInfoV3":{{"MatchId":"{match_id}","MatchEndpointHost":"{host}","MatchEndpointPort":{port},"EventId":"AIBotMatch","YourSeat":1,"PlayerInfos":[],"ClientMetadata":[]}}}}"#,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::envelope::{encode_cmd, Cmd};

    fn cmd(t: i32) -> Cmd {
        Cmd::decode(&encode_cmd(t, "tx-1", "{}")).unwrap()
    }

    #[test]
    fn auth_is_the_gate() {
        let out = handle(&cmd(cmd_type::AUTHENTICATE));
        assert!(out.recognized);
        assert!(out.response_json.contains(r#""Attached":true"#));
        assert!(out.response_json.contains(r#""PlayerConditions":0"#));
        assert!(!out.then_push_match);
    }

    #[test]
    fn start_hook_has_selectable_deck() {
        let out = handle(&cmd(cmd_type::START_HOOK));
        assert!(out.response_json.contains("DeckSummaries"));
        assert!(out.response_json.contains(STUB_DECK_ID));
    }

    #[test]
    fn bot_match_triggers_push() {
        let out = handle(&cmd(cmd_type::EVENT_AI_BOT_MATCH));
        assert!(out.then_push_match);
        assert_eq!(out.response_json, "\"stub-match-1\"");
    }

    #[test]
    fn unknown_falls_back_to_empty_object() {
        let out = handle(&cmd(99999));
        assert!(!out.recognized);
        assert_eq!(out.response_json, "{}");
    }

    #[test]
    fn match_push_carries_endpoint() {
        let push = match_created_push("127.0.0.1", 27019, "m-1");
        assert!(push.contains(r#""Type":"MatchCreated""#));
        assert!(push.contains(r#""MatchEndpointHost":"127.0.0.1""#));
        assert!(push.contains(r#""MatchEndpointPort":27019"#));
    }

    #[test]
    fn boot_commands_all_recognized() {
        for t in [
            cmd_type::GET_FORMATS,
            cmd_type::RANK_GET_COMBINED_RANK_INFO,
            cmd_type::EVENT_GET_ACTIVE_EVENTS_V2,
            cmd_type::EVENT_GET_ACTIVE_MATCHES,
            cmd_type::QUEST_GET_QUESTS,
            cmd_type::PERIODIC_REWARDS_GET_STATUS,
            cmd_type::GRAPH_GET_GRAPH_STATE,
        ] {
            assert!(handle(&cmd(t)).recognized, "cmd {t} should be recognized");
        }
    }
}
