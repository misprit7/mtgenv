//! The shared **option projection**: turn a [`DecisionRequest`] into a flat, presentable
//! [`Prompt`] (title + enumerated option labels + an input `mode`), and turn the player's
//! [`Selection`] back into a [`DecisionResponse`].
//!
//! This is where the engine's masking surfaces for a *human*: the option list IS the legal set
//! the engine enumerated (CLIENT_PLAN §3/§7), so the CLI and the web client render exactly the
//! same affordances and an illegal move is unrepresentable. Keeping this in one place means the
//! terminal client (`human`) and the WebSocket client (`session`/web) never disagree.

use mtg_core::agent::{
    CastVariant, DecisionRequest, DecisionResponse, ObjView, PlayableAction, PlayerView,
};
use mtg_core::basics::{ManaCost, Target};
use mtg_core::ids::ObjId;
use serde::Serialize;

/// How the client should let the player answer a [`Prompt`]. Presentation hint only — the
/// authoritative mapping back to a [`DecisionResponse`] is [`response_from`], keyed off the
/// original request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Mode {
    /// Priority: pick one action, or pass.
    Action,
    /// Pick exactly one option.
    SelectOne,
    /// Pick between `min` and `max` options.
    SelectMany,
    /// Enter a number in `[num_min, num_max]`.
    Number,
    /// Submit an ordering (permutation) of the options.
    Order,
}

/// A flat, client-facing projection of one [`DecisionRequest`]: everything a thin UI needs to
/// render the choice without knowing any rules.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Prompt {
    pub title: String,
    pub mode: Mode,
    /// The enumerated legal options, already filtered by the engine — the masking, rendered.
    pub options: Vec<String>,
    /// Parallel to `options`: the board/hand object each option refers to (`None` for options
    /// with no on-board object, e.g. a player target or a number). Lets the UI highlight the
    /// legal cards and submit by clicking them (MTGO-style), not just via the button list.
    pub option_objs: Vec<Option<u64>>,
    pub can_pass: bool,
    /// For [`Mode::SelectMany`]: how many options must be chosen.
    pub min: u32,
    pub max: u32,
    /// For [`Mode::Number`]: inclusive numeric bounds.
    pub num_min: i64,
    pub num_max: i64,
    /// For a multi-slot target choice (`ChooseTargets`): one entry per target "slot", each with its
    /// own description + min/max + the contiguous range of `options` it owns. The UI groups options
    /// by slot and enforces each slot's count independently. Empty for every other prompt.
    pub target_slots: Vec<PromptSlot>,
    /// Parallel to `options` (Priority only): `true` for a mana-ability option (`ActivateMana`, the
    /// #36 manual-mana taps). Lets the client treat mana taps as *available but not a reason to stop*
    /// — the auto-pass rule counts only non-mana actions. Empty for every other prompt.
    pub is_mana: Vec<bool>,
}

/// One target slot in a multi-slot [`Prompt`] (e.g. Bushwhack-fight: slot 0 = a creature you
/// control, slot 1 = a creature you don't). `[start, start+len)` indexes into `Prompt::options`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptSlot {
    pub description: String,
    pub min: u32,
    pub max: u32,
    pub start: u32,
    pub len: u32,
}

impl Prompt {
    fn new(title: impl Into<String>, mode: Mode, options: Vec<String>) -> Self {
        let max = options.len() as u32;
        Prompt {
            title: title.into(),
            mode,
            options,
            option_objs: Vec::new(),
            can_pass: false,
            min: 0,
            max,
            num_min: 0,
            num_max: 0,
            target_slots: Vec::new(),
            is_mana: Vec::new(),
        }
    }

    /// Attach the per-option board objects (same length as `options`).
    fn with_objs(mut self, objs: Vec<Option<u64>>) -> Self {
        self.option_objs = objs;
        self
    }
}

/// The board object an action refers to (for UI highlighting / click-to-act).
fn action_obj(a: &PlayableAction) -> Option<u64> {
    match a {
        PlayableAction::PlayLand { card } => Some(card.0),
        PlayableAction::Cast { spell, .. } => Some(spell.0),
        PlayableAction::Activate { source, .. } | PlayableAction::ActivateMana { source, .. } => {
            Some(source.0)
        }
        PlayableAction::Special { .. } => None,
    }
}

fn target_obj(t: &Target) -> Option<u64> {
    match t {
        Target::Object(id) => Some(id.0),
        _ => None,
    }
}

/// What the player picked, in option-index terms. Mirrors the wire `response` message; the few
/// numeric/order requests use the extra fields.
#[derive(Debug, Clone, Default)]
pub struct Selection {
    pub picks: Vec<u32>,
    pub number: Option<i64>,
    pub pass: bool,
    pub order: Vec<u32>,
}

// ── Labeling helpers (look names up out of the information-filtered view) ────────────────────

fn all_objs(view: &PlayerView) -> impl Iterator<Item = &ObjView> {
    // Includes the seat's revealed/known-library cards so a Search's candidates resolve to real
    // names (Erode/Bushwhack/Worldwagon fetch a basic). Those fields are empty until the engine
    // populates them when a SelectCards-from-a-hidden-zone decision is pending (forward-compat).
    view.me
        .hand
        .iter()
        .chain(view.me.revealed_to_me.iter())
        .chain(view.me.known_library.iter())
        .chain(view.battlefield.iter())
        .chain(
            view.players
                .iter()
                .flat_map(|p| p.graveyard.iter().chain(p.exile_public.iter())),
        )
}

/// A human-readable name for an object id, as far as this seat may perceive it.
pub fn name_of(view: &PlayerView, id: ObjId) -> String {
    for o in all_objs(view) {
        if let ObjView::Visible { id: oid, chars, .. } = o {
            if *oid == id {
                return chars.name.clone();
            }
        }
    }
    format!("#{}", id.0)
}

/// The mana cost a given cast `variant` of `id` would pay — the warp cost for `Warp`, else the
/// printed mana cost — for labelling the cast option (`None` if the object isn't in the view).
fn cast_cost_of(view: &PlayerView, id: ObjId, variant: CastVariant) -> Option<ManaCost> {
    for o in all_objs(view) {
        if let ObjView::Visible { id: oid, chars, .. } = o {
            if *oid == id {
                return if variant == CastVariant::Warp {
                    chars.warp_cost.clone()
                } else {
                    chars.mana_cost.clone()
                };
            }
        }
    }
    None
}

fn describe_target(view: &PlayerView, t: &Target) -> String {
    match t {
        Target::Player(p) => format!("Player {}", p.0),
        Target::Object(id) => name_of(view, *id),
        Target::Stack(s) => format!("stack object #{}", s.0),
    }
}

fn describe_action(view: &PlayerView, a: &PlayableAction) -> String {
    match a {
        PlayableAction::PlayLand { card } => format!("Play land — {}", name_of(view, *card)),
        PlayableAction::Cast { spell, variant } => {
            // Label with the variant's actual cost: "Cast {2}{G}{G}" / "Warp {2}{G}".
            let verb = if *variant == CastVariant::Warp { "Warp" } else { "Cast" };
            let cost = cast_cost_of(view, *spell, *variant)
                .map(|c| format!(" {c}"))
                .unwrap_or_default();
            format!("{verb} {}{cost}", name_of(view, *spell))
        }
        PlayableAction::Activate { source, ability } => {
            format!("Activate {} ability #{}", name_of(view, *source), ability.0)
        }
        PlayableAction::ActivateMana { source, .. } => {
            format!("Tap {} for mana", name_of(view, *source))
        }
        PlayableAction::Special { kind } => format!("Special action: {kind:?}"),
    }
}

// ── DecisionRequest → Prompt (the masking, flattened for the client) ─────────────────────────

/// Project a request into the flat [`Prompt`] a thin client renders.
pub fn prompt_for(view: &PlayerView, req: &DecisionRequest) -> Prompt {
    use DecisionRequest as R;
    match req {
        R::Priority { actions, can_pass } => {
            let opts = actions.iter().map(|a| describe_action(view, a)).collect();
            let mut p = Prompt::new("Priority — choose an action", Mode::Action, opts);
            p.can_pass = *can_pass;
            p.option_objs = actions.iter().map(action_obj).collect();
            // Mark mana-ability options (the #36 manual taps) so the client's auto-pass rule can
            // ignore them — having mana available is never itself a reason to stop.
            p.is_mana = actions
                .iter()
                .map(|a| matches!(a, PlayableAction::ActivateMana { .. }))
                .collect();
            p
        }
        R::ChooseStartingPlayer { candidates } => Prompt::new(
            "Choose who takes the first turn",
            Mode::SelectOne,
            candidates.iter().map(|p| format!("Player {}", p.0)).collect(),
        ),
        R::Mulligan {
            mulligans_taken,
            will_bottom_if_kept,
            hand,
        } => Prompt::new(
            format!(
                "Mulligan? (hand of {}, {mulligans_taken} taken, would bottom {will_bottom_if_kept})",
                hand.len()
            ),
            Mode::SelectOne,
            vec!["Keep this hand".into(), "Mulligan".into()],
        ),
        R::ChooseNumber {
            reason, min, max, ..
        } => {
            let mut p = Prompt::new(format!("Choose a number ({reason:?})"), Mode::Number, vec![]);
            p.num_min = *min;
            p.num_max = *max;
            p
        }
        R::SelectCards {
            reason,
            from,
            min,
            max,
            description,
        } => {
            let opts = from.iter().map(|id| name_of(view, *id)).collect();
            let mut p = Prompt::new(format!("{description} ({reason:?})"), Mode::SelectMany, opts);
            p.min = *min;
            p.max = *max;
            p.option_objs = from.iter().map(|id| Some(id.0)).collect();
            p
        }
        R::ChooseModes {
            modes, min, max, ..
        } => {
            let opts = modes.iter().map(|m| m.label.clone()).collect();
            let mut p = Prompt::new("Choose mode(s)", Mode::SelectMany, opts);
            p.min = *min;
            p.max = *max;
            p
        }
        R::CastingTimeOptions { options, .. } => {
            let opts = options.iter().map(|o| o.label.clone()).collect::<Vec<_>>();
            let mut p = Prompt::new("Cast-time options", Mode::SelectMany, opts);
            p.min = 0;
            p
        }
        R::ChooseTargets { slots, .. } => {
            // Flatten EVERY slot's legal candidates into one option list (slot-by-slot, in order),
            // recording each slot's contiguous option range so the UI groups + enforces per slot.
            // (Bushwhack-fight is the first multi-slot spell: slot 0 = creature you control, slot 1 =
            // creature you don't control.) The response maps each pick back to its (slot, target).
            let mut opts = Vec::new();
            let mut objs = Vec::new();
            let mut target_slots = Vec::new();
            for s in slots {
                let start = opts.len() as u32;
                for t in &s.legal {
                    opts.push(describe_target(view, t));
                    objs.push(target_obj(t));
                }
                target_slots.push(PromptSlot {
                    description: s.description.clone(),
                    min: s.min,
                    max: s.max,
                    start,
                    len: s.legal.len() as u32,
                });
            }
            let mut p = Prompt::new("Choose target(s)", Mode::SelectMany, opts).with_objs(objs);
            p.min = slots.iter().map(|s| s.min).sum();
            p.max = slots.iter().map(|s| s.max).sum();
            p.target_slots = target_slots;
            p
        }
        R::DeclareAttackers { eligible } => Prompt::new(
            "Declare attackers",
            Mode::SelectMany,
            eligible.iter().map(|e| name_of(view, e.creature)).collect(),
        )
        .with_objs(eligible.iter().map(|e| Some(e.creature.0)).collect()),
        R::DeclareBlockers { eligible, .. } => Prompt::new(
            "Declare blockers",
            Mode::SelectMany,
            eligible.iter().map(|e| name_of(view, e.creature)).collect(),
        )
        .with_objs(eligible.iter().map(|e| Some(e.creature.0)).collect()),
        R::AssignCombatDamage {
            recipients, total, ..
        } => Prompt::new(
            format!("Assign {total} combat damage"),
            Mode::SelectOne,
            recipients
                .iter()
                .map(|d| format!("{} (lethal {})", describe_target(view, &d.recipient), d.lethal))
                .collect(),
        )
        .with_objs(recipients.iter().map(|d| target_obj(&d.recipient)).collect()),
        R::OrderObjects { items, .. } => Prompt::new(
            "Order these objects (first = resolves first)",
            Mode::Order,
            items.iter().map(|id| name_of(view, *id)).collect(),
        )
        .with_objs(items.iter().map(|id| Some(id.0)).collect()),
        R::ChooseOption {
            reason,
            options,
            min,
            max,
        } => {
            let opts = options.iter().map(|o| o.label.clone()).collect();
            let mut p = Prompt::new(format!("Choose ({reason:?})"), Mode::SelectMany, opts);
            p.min = *min;
            p.max = *max;
            p
        }
        R::ChooseColor { allowed, min, max } => {
            let opts = allowed.iter().map(|c| format!("{c:?}")).collect();
            let mut p = Prompt::new("Choose color(s)", Mode::SelectMany, opts);
            p.min = *min;
            p.max = *max;
            p
        }
        R::ChooseCounterType { options } => Prompt::new(
            "Choose a counter type",
            Mode::SelectOne,
            options.iter().map(|c| format!("{c:?}")).collect(),
        ),
        R::ChooseReplacement { applicable, .. } => Prompt::new(
            "Choose which replacement effect applies",
            Mode::SelectOne,
            applicable.iter().map(|r| r.description.clone()).collect(),
        ),
        R::SelectFromGroups { groups, .. } => {
            // Flatten group 0 (scaffold). Real grouped selection lands with the effect runtime.
            let (opts, min, max) = match groups.first() {
                Some(g) => (
                    g.options.iter().map(|id| name_of(view, *id)).collect(),
                    g.min,
                    g.max,
                ),
                None => (vec![], 0, 0),
            };
            let mut p = Prompt::new("Select from group", Mode::SelectMany, opts);
            p.min = min;
            p.max = max;
            p
        }
        R::Distribute { among, total, .. } => {
            let mut p = Prompt::new(
                format!("Distribute {total} among recipients (auto-spread)"),
                Mode::SelectOne,
                among.iter().map(|t| describe_target(view, t)).collect(),
            );
            p.min = 0;
            p.max = 0;
            p
        }
        R::ArrangeCards { cards, .. } => Prompt::new(
            "Arrange cards (first = top)",
            Mode::Order,
            cards.iter().map(|id| name_of(view, *id)).collect(),
        ),
        R::PayCost { non_mana, .. } => Prompt::new(
            "Pay cost — choose non-mana payments",
            Mode::SelectMany,
            non_mana.iter().map(|o| format!("{o:?}")).collect(),
        ),
        R::Confirm { kind } => Prompt::new(
            format!("Confirm: {kind:?}"),
            Mode::SelectOne,
            vec!["No".into(), "Yes".into()],
        ),
    }
}

// ── Selection → DecisionResponse (resolve the picks against the original request) ────────────

fn first(picks: &[u32]) -> u32 {
    picks.first().copied().unwrap_or(0)
}

/// Resolve a player's [`Selection`] back into the [`DecisionResponse`] the engine expects. The
/// original `req` is the source of truth for *shape*; `sel` only carries the chosen indices.
pub fn response_from(req: &DecisionRequest, sel: &Selection) -> DecisionResponse {
    use DecisionRequest as R;
    use DecisionResponse as Resp;
    match req {
        R::Priority { .. } => {
            if sel.pass || sel.picks.is_empty() {
                Resp::Pass
            } else {
                Resp::Action(sel.picks[0])
            }
        }
        R::ChooseStartingPlayer { .. } => Resp::Index(first(&sel.picks)),
        // Option 0 = keep, 1 = mulligan.
        R::Mulligan { .. } => Resp::Bool(sel.picks.first() == Some(&1)),
        R::ChooseModes { .. } => Resp::Indices(sel.picks.clone()),
        R::ChooseNumber { min, .. } => Resp::Number(sel.number.unwrap_or(*min)),
        R::CastingTimeOptions { .. } => Resp::Indices(sel.picks.clone()),
        R::ChooseTargets { slots, .. } => {
            // Rebuild the SAME flat option order as the projection (slot-by-slot, target-by-target)
            // and map each picked global option index back to its (slot_idx, target_idx_in_slot).
            let mut flat: Vec<(u32, u32)> = Vec::new();
            for (si, slot) in slots.iter().enumerate() {
                for ti in 0..slot.legal.len() {
                    flat.push((si as u32, ti as u32));
                }
            }
            Resp::Pairs(
                sel.picks
                    .iter()
                    .filter_map(|&k| flat.get(k as usize).copied())
                    .collect(),
            )
        }
        R::Distribute {
            among,
            total,
            min_each,
            ..
        } => {
            let mut amounts: Vec<(u32, u32)> =
                (0..among.len() as u32).map(|i| (i, *min_each)).collect();
            let assigned = *min_each * among.len() as u32;
            let remainder = total.saturating_sub(assigned);
            if let Some(first) = amounts.first_mut() {
                first.1 += remainder;
            }
            Resp::Amounts(amounts)
        }
        R::PayCost { mana_sources, .. } => Resp::Payment {
            mana: (0..mana_sources.len() as u32).collect(),
            non_mana: sel.picks.clone(),
        },
        R::DeclareAttackers { eligible } => Resp::Pairs(
            sel.picks
                .iter()
                .filter(|&&i| (i as usize) < eligible.len())
                .map(|&i| (i, 0))
                .collect(),
        ),
        R::DeclareBlockers { .. } => Resp::Pairs(sel.picks.iter().map(|&c| (c, 0)).collect()),
        R::AssignCombatDamage { total, .. } => Resp::Amounts(vec![(0, *total)]),
        R::OrderObjects { items, .. } => {
            if sel.order.len() == items.len() {
                Resp::Order(sel.order.clone())
            } else {
                Resp::Order((0..items.len() as u32).collect())
            }
        }
        R::SelectCards { .. } => Resp::Indices(sel.picks.clone()),
        R::SelectFromGroups { .. } => {
            Resp::Pairs(sel.picks.iter().map(|&c| (0, c)).collect())
        }
        R::ArrangeCards { cards, .. } => {
            if sel.order.len() == cards.len() {
                Resp::Arrangement(sel.order.iter().map(|&c| (c, 0, c)).collect())
            } else {
                Resp::Arrangement((0..cards.len() as u32).map(|i| (i, 0, i)).collect())
            }
        }
        R::ChooseReplacement { .. } => Resp::Index(first(&sel.picks)),
        R::ChooseCounterType { .. } => Resp::Index(first(&sel.picks)),
        R::ChooseOption { .. } => Resp::Indices(sel.picks.clone()),
        R::ChooseColor { .. } => Resp::Indices(sel.picks.clone()),
        // Option 0 = No, 1 = Yes.
        R::Confirm { .. } => Resp::Bool(sel.picks.first() == Some(&1)),
    }
}

/// Parse one line of user/script input into a [`Selection`], according to the prompt's input
/// `mode`. Shared by the terminal `HumanAgent` and any other line-driven backend so they all
/// interpret input identically. Tolerant: unparseable input falls back to a safe default (pass /
/// no selection), and `response_from` + the engine clamp anything out of range.
pub fn parse_selection(prompt: &Prompt, input: &str) -> Selection {
    let t = input.trim();
    match prompt.mode {
        Mode::Action => {
            if t.is_empty() || t.eq_ignore_ascii_case("p") || t.eq_ignore_ascii_case("pass") {
                Selection {
                    pass: true,
                    ..Default::default()
                }
            } else if let Ok(i) = t.parse::<u32>() {
                Selection {
                    picks: vec![i],
                    ..Default::default()
                }
            } else {
                Selection {
                    pass: true,
                    ..Default::default()
                }
            }
        }
        Mode::SelectOne => Selection {
            picks: vec![t.parse::<u32>().unwrap_or(0)],
            ..Default::default()
        },
        Mode::SelectMany => Selection {
            picks: t.split_whitespace().filter_map(|s| s.parse().ok()).collect(),
            ..Default::default()
        },
        Mode::Number => {
            let n = t
                .parse::<i64>()
                .unwrap_or(prompt.num_min)
                .clamp(prompt.num_min, prompt.num_max);
            Selection {
                number: Some(n),
                ..Default::default()
            }
        }
        Mode::Order => Selection {
            order: t.split_whitespace().filter_map(|s| s.parse().ok()).collect(),
            ..Default::default()
        },
    }
}

/// A safe default response for a request (used on EOF / closed input): pass priority, keep the
/// hand, decline optional choices, and under-select bounded picks (the engine fills the rest).
pub fn default_response(req: &DecisionRequest) -> DecisionResponse {
    response_from(req, &Selection::default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;
    use mtg_core::agent::{ActionRef, CastVariant, PlayerPrivateView, TargetSlot};
    use mtg_core::basics::Phase;
    use mtg_core::ids::{PlayerId, StackId};

    #[test]
    fn choose_targets_covers_every_slot_and_maps_picks_to_pairs() {
        // Bushwhack-fight shape: slot 0 = a creature you control (2 legal), slot 1 = one you don't
        // (1 legal). The bug was that only slot 0 was projected, so slot 1 never got a target.
        let req = DecisionRequest::ChooseTargets {
            for_action: ActionRef(StackId(0)),
            source: None,
            slots: vec![
                TargetSlot {
                    description: "creature you control".into(),
                    legal: vec![Target::Object(ObjId(10)), Target::Object(ObjId(11))],
                    min: 1,
                    max: 1,
                },
                TargetSlot {
                    description: "creature you don't control".into(),
                    legal: vec![Target::Object(ObjId(20))],
                    min: 1,
                    max: 1,
                },
            ],
        };
        let p = prompt_for(&empty_view(), &req);
        assert_eq!(p.options.len(), 3, "all 3 candidates across both slots projected");
        assert_eq!(p.option_objs, vec![Some(10), Some(11), Some(20)]);
        assert_eq!(p.min, 2);
        assert_eq!(p.max, 2);
        assert_eq!(p.target_slots.len(), 2);
        assert_eq!((p.target_slots[0].start, p.target_slots[0].len), (0, 2));
        assert_eq!((p.target_slots[1].start, p.target_slots[1].len), (2, 1));
        // Pick the 2nd option of slot 0 (global idx 1) and slot 1's only option (global idx 2).
        let sel = Selection { picks: vec![1, 2], ..Default::default() };
        match response_from(&req, &sel) {
            DecisionResponse::Pairs(pairs) => assert_eq!(pairs, vec![(0, 1), (1, 0)]),
            other => panic!("expected Pairs, got {other:?}"),
        }
    }

    fn empty_view() -> PlayerView {
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

    #[test]
    fn priority_prompt_lists_actions_and_pass() {
        let view = empty_view();
        let req = DecisionRequest::Priority {
            actions: vec![
                PlayableAction::PlayLand { card: ObjId(1) },
                PlayableAction::Cast {
                    spell: ObjId(2),
                    variant: CastVariant::Normal,
                },
            ],
            can_pass: true,
        };
        let p = prompt_for(&view, &req);
        expect![[r#"
            Prompt {
                title: "Priority — choose an action",
                mode: Action,
                options: [
                    "Play land — #1",
                    "Cast #2",
                ],
                option_objs: [
                    Some(
                        1,
                    ),
                    Some(
                        2,
                    ),
                ],
                can_pass: true,
                min: 0,
                max: 2,
                num_min: 0,
                num_max: 0,
                target_slots: [],
                is_mana: [
                    false,
                    false,
                ],
            }"#]]
        .assert_eq(&format!("{p:#?}"));
    }

    #[test]
    fn priority_keeps_both_cast_variants_of_one_card_distinct() {
        // Mightform Harmonizer is castable for its normal cost AND its Warp alt-cost — the engine
        // enumerates BOTH as separate `Cast` actions on the SAME spell object. The projection must
        // keep both options (same option_obj, distinct labels + indices) so the web client can let
        // the player choose the variant via the card click (instead of collapsing to the first).
        let req = DecisionRequest::Priority {
            actions: vec![
                PlayableAction::Cast { spell: ObjId(5), variant: CastVariant::Normal },
                PlayableAction::Cast { spell: ObjId(5), variant: CastVariant::Warp },
            ],
            can_pass: true,
        };
        let p = prompt_for(&empty_view(), &req);
        assert_eq!(p.options.len(), 2, "both cast variants surfaced");
        assert_eq!(p.option_objs, vec![Some(5), Some(5)], "both reference the same card");
        assert_ne!(p.options[0], p.options[1], "labels distinguish the variants");
        assert!(p.options[0].starts_with("Cast") && p.options[1].starts_with("Warp"));
        // Each maps back to its own action index (the client picks one variant → that action).
        for (i, want) in [(0u32, 0u32), (1, 1)] {
            let sel = Selection { picks: vec![i], ..Default::default() };
            assert_eq!(response_from(&req, &sel), DecisionResponse::Action(want));
        }
    }

    #[test]
    fn priority_selection_maps_to_action_or_pass() {
        let req = DecisionRequest::Priority {
            actions: vec![PlayableAction::PlayLand { card: ObjId(1) }],
            can_pass: true,
        };
        let passed = response_from(
            &req,
            &Selection {
                pass: true,
                ..Default::default()
            },
        );
        assert_eq!(passed, DecisionResponse::Pass);
        let chose = response_from(
            &req,
            &Selection {
                picks: vec![0],
                ..Default::default()
            },
        );
        assert_eq!(chose, DecisionResponse::Action(0));
    }

    #[test]
    fn mulligan_keep_vs_mull_round_trips() {
        let req = DecisionRequest::Mulligan {
            hand: vec![ObjId(1)],
            mulligans_taken: 0,
            will_bottom_if_kept: 0,
        };
        let keep = response_from(
            &req,
            &Selection {
                picks: vec![0],
                ..Default::default()
            },
        );
        let mull = response_from(
            &req,
            &Selection {
                picks: vec![1],
                ..Default::default()
            },
        );
        assert_eq!(keep, DecisionResponse::Bool(false));
        assert_eq!(mull, DecisionResponse::Bool(true));
    }
}
