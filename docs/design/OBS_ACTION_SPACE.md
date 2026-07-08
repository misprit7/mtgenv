# Observation & Action Space — current state and honest gaps

*Written 2026-07-08. **Contract v2** (obs `F_PERM = 48`, `MAX_PERM = 256`, action space
`Discrete(322)`). Source of truth: `crates/mtg-py/src/{obs.rs, codec.rs, layout.rs}`. This doc
exists to answer one question precisely: **what can the policy actually know, especially about
relationships between entities in combat — and what can it not.***

> **v2 change (2026-07-08): `MAX_PERM` 32 → 256.** Late-game SOS boards were observed at ~39
> permanents — past the old 32-row cap, so any object beyond row 32 was silently truncated:
> invisible to the policy *and* unmappable to a `PERM` action slot (≈18% of a 39-object board
> unseen). The cap is raised to 256 to never truncate in practice (well past any realistic board,
> degenerate grinds included); a **deterministic truncation priority** (§2a) remains as the safety
> net if that bound is ever exceeded. The action space grew `Discrete(98) → Discrete(322)` (the
> `PERM` bucket widened by 224; all later bucket bases shifted +224). This is a deliberate contract
> break: the byte-equivalence snapshot + ratings fingerprint were re-pinned to v2 on purpose.

## 1. What the policy sees

The encoder reads only the engine's info-filtered `PlayerView` (hidden information is masked
structurally — the encoder never touches `GameState`, so a leak is impossible by construction).
Output is a fixed-shape bundle:

| Tensor | Shape | Contents |
|---|---|---|
| `globals` | 69 | turn, phase one-hot (12), active/priority flags, per-seat block ×2 (life, poison, hand/library/graveyard/exile/battlefield counts, floating mana WUBRGC), stack depth, **decision-kind one-hot (21 `DecisionRequest` variants)**, request scalars, 2 player-candidate flags |
| `bf_feat` | 256 × 48 | one row per battlefield object (both players'), in `perm_order` (§2a), see §2 |
| `bf_ids` / `bf_cardid` | 256 | hashed `grp_id` (embedding lookup) + deck-local exact one-hot |
| `hand_feat` | 16 × 18 | own hand rows, column map in §2b |
| `hand_ids` / `hand_cardid` | 16 | hashed `grp_id` (embedding lookup) + deck-local exact one-hot — same card-identity channels as the battlefield |
| `stack_feat` | 8 × 18 | stack rows, column map in §2b — **no target refs; see gap G1** |
| `stack_ids` / `stack_cardid` | 8 | hashed `grp_id` + deck-local one-hot |
| `decision_ids` / `decision_cardid` | 1 | `grp_id` of the card *raising the current decision* (fed to the head directly, bypassing any pooling) |

## 2b. Hand & stack rows — full column maps

Every zone has the same *card-identity* channels (`*_ids` + `*_cardid`); what hand/stack rows lack
is the battlefield's *instance/relation* block (cols 45–47) — see §4/G1.

**`hand_feat` (18 cols):** 0 `present` · 1 `mana_value` · 2 `castable` (legal to cast **right
now**, from the live decision's enumeration — not a static flag) · 3–10 card types · 11–15 colors
· 16 `is_decision_source` · 17 `is_decision_candidate` (e.g. a discard/reveal choice).

**`stack_feat` (18 cols):** 0 `present` · 1 `controller_is_me` · 2 `mana_value` · 3–10 card types
· 11–15 colors · 16 `is_decision_source` (the spell/ability being decided) · 17
`is_decision_candidate` (this stack object is targetable by the current decision). Note what is
NOT here: *what this spell targets* — gap G1.

## 2a. Battlefield row ordering & truncation priority (`layout::perm_order`)

The `bf_feat`/`bf_ids` rows are **not** in raw `view.battlefield` order. Both the obs encoder and the
action codec route through one shared function, `layout::perm_order(battlefield)`, which returns the
row indices **partitioned nonlands-first (creatures/artifacts/enchantments/…), then lands, stable
within each class** (the engine's order preserved inside a class), **capped at `MAX_PERM = 256`**.

This is load-bearing in two ways:

1. **The obs↔action contract.** Obs row `k` and action slot `PERM[k]` must name the *same* object —
   the whole point of positional slots. A single ordering function used by both sides guarantees it
   by construction (there is no second code path to drift). The overflowing-board agreement test in
   `codec.rs` pins this: obs row `k`'s `instance_id` equals codec `PERM[k]`'s id for all `k`.
2. **Deterministic truncation priority.** When a board exceeds `MAX_PERM` (256) permanents,
   `.truncate(256)` drops the **trailing lands** — the least decision-relevant rows (a wall of tapped
   lands has no legal action; the creatures/artifacts/enchantments carry every real choice). A
   face-down/`Hidden` permanent counts as a nonland (it's a 2/2 creature). At 256 this essentially
   never fires — it's the safety net, not the common path. Before v2 the cap was 32 and truncation was
   "first 32 in engine order" — arbitrary, and it silently hid ~18% of an observed 39-object board.

## 2. The battlefield row — where combat lives

Each of the 48 columns per permanent, by block (exact indices for the combat block):

- **0–8**: computed P/T, damage, tapped, summoning-sick, counters, controller-is-me, …
- **9–36**: card types (8), colors (5), keywords (15)
- **37–38**: status pair
- **39 `attacking`** — declared attacker
- **40 `blocking`** — binary "is blocking" (flattens *whom* and *with how many* — the reason later columns exist)
- **41 `is_decision_source`** — this object raised the current decision
- **42 `is_decision_candidate`** — this object is a *legal choice* for the current decision
- **43 `blocked_by`** — per-attacker count of blockers, **committed + pending mid-decision** (makes gang-vs-single observable; added for the 2.9 era)
- **44 `is_pending_combat`** — *my* creature already assigned in the in-flight combat decision (attacker mid-DeclareAttackers, blocker mid-DeclareBlockers; added in the 4.x audit — before this, the policy could not see its own partial combat plan; only the action mask knew, and the mask never feeds the value/feature nets)
- **45 `instance_id`** — stable per-game object tag
- **46 `blocking_id`** — the `instance_id` of the attacker this creature is blocking (exact pairing)
- **47 `attached_to_id`** — host id for auras/equipment

Columns 45–47 were added for the relational (4.7) arm. They are **match keys, not features**:
raw ids are meaningless to an MLP; they become information only through machinery that *matches*
them across rows (the attention arm builds an adjacency bias from `blocking_id[i] == instance_id[j]`).

## 3. The action space — `Discrete(322)`, positional buckets

```
0         COMMIT            (finish/accept-default for the current decision)
1–16      HAND[i]           (act on hand row i: play/cast)
17–272    PERM[i]           (act on battlefield row i: declare attacker/blocker, target, …)  [256 slots]
273–274   PLAYER[i]         (target a player)
275–282   STACK[i]          (target a stack object)
283–298   MODE[i]           (modal choices; also fallback for unmappable candidates)
299–303   COLOR[i]
304–319   NUMBER[i]
320–321   YES / NO
```

Slot semantics are **positional and contextual**: `PERM[3]` means "battlefield row 3 (in
`perm_order`, §2a) *under the current decision*" — an attacker declaration in one phase, a block
target in another. What the current decision *is* comes from the globals' decision one-hot +
`is_decision_source` + `decision_ids`. Every enumerated slot is legal by construction; the mask
zeroes the rest. Bucket bases are **derived** from the shared table sizes (`codec.rs`), so nothing
downstream hard-codes 322 — Python reads `PyGame.action_dim()` and `obs_spec()` at runtime.

*v1→v2 remap (for reading old 98-slot checkpoints through the ratings `SchemaAdapter`):* `COMMIT`,
`HAND[0..16]`, and `PERM[0..32]` keep their indices (the first 32 perm rows map 1:1 after 256→32 obs
truncation); every bucket from `PLAYER` onward shifts **+224** (the `PERM` bucket widened by 224).
`PERM[32..256]` is new — an old net can't see or act on those rows, so they're masked off for it.

**Combat is autoregressive.** DeclareAttackers: pick `PERM[i]` per attacker (each toggle updates
`is_pending_combat` in the next obs), then `COMMIT`. DeclareBlockers is two-level: pick a blocker
(`PERM[i]` among your untapped creatures), then a sub-decision picks *which attacker* it blocks
(`PERM[j]` among enemy attackers, flagged via `is_decision_candidate`); `blocked_by`/`blocking_id`
update between sub-steps, so a second blocker's decision context includes the first assignment.
Complex requests (ordering, damage assignment, cost payment) currently sit behind a single
`COMMIT` = engine-default (GYM_PLAN §4.2) — they are not yet decision points the policy controls.

## 4. The relationship question — what is knowable, and since when

| Relationship | Knowable? | Via | Since |
|---|---|---|---|
| Who is attacking | ✅ | col 39 | always |
| Is a creature blocking (at all) | ✅ | col 40 | always |
| How many blockers gang each attacker (incl. mid-decision) | ✅ | col 43 | 2.9 era |
| Which of MY creatures are already in my in-flight combat plan | ✅ | col 44 | 4.4+ |
| Exactly *which attacker* each blocker blocks | ✅* | cols 45/46 | **4.7+ only** |
| Aura/equipment → host | ✅* | cols 45/47 | 4.7+ (unused by test decks) |
| Which attacker a pending sub-decision would assign a blocker to | ✅ | col 42 candidates + decision context | always |
| **Which spell on the stack targets which object/player** | ❌ | — | **gap G1** |
| Damage-assignment order / trample split | ❌ (engine default behind COMMIT) | — | gap G2 |
| Opponent hand/library contents | ❌ by design (hidden info) | — | — |

*✅\* = present in the observation, but only exploitable by an architecture with id-matching
machinery. This distinction matters and cuts both ways:*

1. **Information-sufficiency is now mostly solved for the current combat micro-envs.** After the
   4.x audit, everything needed to reason about swine combat (who gangs whom, my partial plan,
   eligibility) is in the obs.
2. **Representation ≠ learning.** The 4.7 result is the proof: the attention arm could *express*
   the pairing relations and still learned worse blocking judgment than the mean-pool baseline.
   Suspicion about the obs was reasonable, but the measured bottleneck has moved to the training
   signal (what mirror self-play rewards), not observability.
3. **The known real gaps are ahead of the current decks, not behind them.** G1 (stack targeting
   ids) is invisible today because heralds/bears/swine cast no targeted spells — but every SOS
   deck does; stack rows need `target_id`-style columns (same append-only pattern) before
   spell-heavy training means anything. G2 (damage ordering) isn't even a decision yet.

## 5. History of the widths (for reading old runs)

`F_PERM` is the per-row width (columns); `MAX_PERM` is the row count / action `PERM`-bucket size.

| Era | F_PERM | MAX_PERM | action_dim | What changed |
|---|---|---|---|---|
| ≤2.8 | 43 | 32 | 98 | gang counts missing (double-block invisible → 2.8's flat gang_rate was predetermined) |
| 2.9–4.3 | 44 | 32 | 98 | + own pending combat plan |
| 4.4–4.6 | 45 | 32 | 98 | + relation ids (pairings/attachments) |
| 4.7–4.9 | 48 | 32 | 98 | + G1/G2 relation cols (contract **v1**) |
| 4.10+ | 48 | **256** | **322** | **v2**: `MAX_PERM` 32→256 (late-game truncation fix, §2a) + `perm_order` truncation priority; columns unchanged |
