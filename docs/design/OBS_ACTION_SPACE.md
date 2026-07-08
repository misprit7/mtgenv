# Observation & Action Space — current state and honest gaps

*Written 2026-07-08. **Contract v3** (obs `F_PERM = 45`, `MAX_PERM = 256`, new `edges` +
`choice_feat` tensors, action space `Discrete(322)` — layout unchanged from v2). Source of
truth: `crates/mtg-py/src/{obs.rs, codec.rs, layout.rs}`. This doc exists to answer one
question precisely: **what can the policy actually know, especially about relationships
between entities in combat — and what can it not.***

> **v3 change (2026-07-08, spec: `OBS2_DESIGN.md` §7): relations became explicit; ids left the
> tensors.** The three float id match-key columns (45–47: instance/blocking/attached ids) are
> **deleted** (`F_PERM` 48 → 45) — pairings now arrive as an explicit **`edges` tensor**
> (128×4 `(src_row, dst_row, type, k)`): blocking, attachment, **stack targeting (gap G1
> closed)**, stack sources, attacks, and pending mid-decision picks, addressed by *row
> position*, never by id. Abstract options (modes/colors/numbers/yes-no) gained content tokens
> (**`choice_feat` 16×12**, authored by the codec so slot↔row alignment holds by construction).
> The deck-local `*_cardid` one-hots are **deleted** and `*_ids` renamed **`*_grpid`** (§7.1a:
> `grpid` = which card, `entityid` = which game object — the only two id spaces; entityids are
> resolved to row positions at encode time and never appear in a tensor). The action-space
> layout is **unchanged** — the v2→v3 break is observation-side only. Ladders re-baselined as
> v3 (spine ratings carried over byte-identically — the spine reads only preserved columns).

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
| `bf_feat` | 256 × 45 | one row per battlefield object (both players'), in `perm_order` (§2a), see §2 |
| `bf_grpid` | 256 | hashed `grp_id` per row (embedding lookup) — the only card-identity channel (v3 deleted the deck-local one-hots) |
| `hand_feat` | 16 × 18 | own hand rows, column map in §2b |
| `hand_grpid` | 16 | hashed `grp_id` — same identity channel as the battlefield (shared embedding table → cross-zone identity binding) |
| `stack_feat` | 8 × 18 | stack rows, column map in §2b — targets ride `edges` (G1 closed) |
| `stack_grpid` | 8 | hashed `grp_id` |
| `decision_grpid` | 1 | `grp_id` of the card *raising the current decision* (fed to the head directly, bypassing any pooling) |
| `edges` | 128 × 4 | **relation edges `(src_row, dst_row, type, k)`**, −1-padded. Types: `BLOCKS`, `ATTACKS`, `ATTACHED_TO`, `TARGETS` (k = target order), `STACK_SOURCE`, `PENDING_PICK` (k = pick order). Row space: bf 0–255, hand 256–271, stack 272–279, you 280, opp 281, the decision itself 282. Consumed as per-type attention bias |
| `choice_feat` | 16 × 12 | **content tokens for the current decision's abstract options** (present, kind one-hot mode/color/number/bool, value scalar, color one-hot). Row `j` ↔ slot `j` of the live `MODE`/`COLOR`/`NUMBER` bucket (YES→0, NO→1), authored by the codec |

## 2b. Hand & stack rows — full column maps

Every zone has the same *card-identity* channel (`*_grpid`); relations for every zone ride the
shared `edges` tensor (stack rows participate via `TARGETS`/`STACK_SOURCE` edges).

**`hand_feat` (18 cols):** 0 `present` · 1 `mana_value` · 2 `castable` (legal to cast **right
now**, from the live decision's enumeration — not a static flag) · 3–10 card types · 11–15 colors
· 16 `is_decision_source` · 17 `is_decision_candidate` (e.g. a discard/reveal choice).

**`stack_feat` (18 cols):** 0 `present` · 1 `controller_is_me` · 2 `mana_value` · 3–10 card types
· 11–15 colors · 16 `is_decision_source` (the spell/ability being decided) · 17
`is_decision_candidate` (this stack object is targetable by the current decision). *What this
spell targets* is no longer missing — it arrives as `TARGETS` edges (v3; gap G1 closed).

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

Each of the 45 columns per permanent, by block (exact indices for the combat block):

- **0–8**: computed P/T, damage, tapped, summoning-sick, counters, controller-is-me, …
- **9–36**: card types (8), colors (5), keywords (15)
- **37–38**: status pair
- **39 `attacking`** — declared attacker
- **40 `blocking`** — binary "is blocking" (flattens *whom* and *with how many* — those pairings ride `edges`)
- **41 `is_decision_source`** — this object raised the current decision
- **42 `is_decision_candidate`** — this object is a *legal choice* for the current decision
- **43 `blocked_by`** — per-attacker count of blockers, **committed + pending mid-decision** (makes gang-vs-single observable; added for the 2.9 era)
- **44 `is_pending_combat`** — *my* creature already assigned in the in-flight combat decision (attacker mid-DeclareAttackers, blocker mid-DeclareBlockers; added in the 4.x audit — before this, the policy could not see its own partial combat plan; only the action mask knew, and the mask never feeds the value/feature nets)

Columns 45–47 (`instance_id`/`blocking_id`/`attached_to_id`) existed in contract v1–v2 as float
**match keys** the relational arm compared across rows (`blocking_id[i] == instance_id[j]`).
**v3 deleted them**: the engine now exports the pairings directly as `edges`, so the network
consumes engine truth instead of learning to rediscover it via id equality — "scalar features
stay, float match-keys die." The unary flags (39–44) remain: a flag is the correct encoding for
a per-row property; edges are reserved for cross-row *pairings*.

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
`is_decision_source` + `decision_grpid`. Every enumerated slot is legal by construction; the mask
zeroes the rest. Bucket bases are **derived** from the shared table sizes (`codec.rs`), so nothing
downstream hard-codes 322 — Python reads `PyGame.action_dim()` and `obs_spec()` at runtime.

**v3: every slot has content.** The layout above is unchanged, but the abstract buckets
(`MODE`/`COLOR`/`NUMBER`/`YES`/`NO`) are no longer content-free positional slots: choice row `j`
of `choice_feat` carries what slot `j` *means* (its kind and, for numbers, the exact value it
submits), authored by the codec's own slot table so the two can never disagree. The v3 policy
computes every action logit from a content token (entity rows, player/decision tokens, choice
rows) — the learned "abstract slot embedding" family, source of the 4.7 logit-scale bug, is gone.

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
| Who is attacking | ✅ | col 39 (+ `ATTACKS` edge says *whom*) | always (edge: v3) |
| Is a creature blocking (at all) | ✅ | col 40 | always |
| How many blockers gang each attacker (incl. mid-decision) | ✅ | col 43 | 2.9 era |
| Which of MY creatures are already in my in-flight combat plan | ✅ | col 44 + `PENDING_PICK` edges | 4.4+ (edges: v3) |
| Exactly *which attacker* each blocker blocks | ✅ | `BLOCKS` edges (engine truth, direct) | 4.7 via id-matching; **v3 direct** |
| Aura/equipment → host | ✅ | `ATTACHED_TO` edges | 4.7 via id-matching; **v3 direct** |
| Which attacker a pending sub-decision would assign a blocker to | ✅ | col 42 candidates + decision context | always |
| **Which spell on the stack targets which object/player** | ✅ | `TARGETS` edges (k = target order) | **v3 — gap G1 CLOSED** |
| Which permanent an ability on the stack came from | ✅ | `STACK_SOURCE` edges | v3 |
| Damage-assignment order / trample split | ❌ (engine default behind COMMIT) | — | gap G2 |
| Opponent hand/library contents | ❌ by design (hidden info) | — | — |

Three notes on how to read this table after v3:

1. **The id-matching caveat is gone.** In v1–v2, pairings were "present but only exploitable by
   an architecture with id-matching machinery" (float match keys). In v3 the engine exports the
   pairing itself; the consuming architecture adds a learned per-type attention bias at the edge's
   `(src_row, dst_row)` — no equality function to learn, no representation gap between "in the
   obs" and "usable."
2. **Representation ≠ learning still holds.** The 4.7/4.9 arc proved the bottleneck can sit in
   the training signal (mirror self-play vs punisher scriptpool), not observability. v3 removes
   the *representation* excuses; behavioral gaps from here point at training.
3. **The remaining gap is ahead of the current decks.** G2 (damage ordering / trample splits)
   isn't a decision point yet — it sits behind the engine-default `COMMIT`. The `edges` `k`
   column is reserved to carry ordering when it becomes one. Graveyard/exile contents are
   count-only in `globals` (a named SOS-era simplification — `OBS2_DESIGN.md` §7.7).

## 5. History of the widths (for reading old runs)

`F_PERM` is the per-row width (columns); `MAX_PERM` is the row count / action `PERM`-bucket size.

| Era | F_PERM | MAX_PERM | action_dim | What changed |
|---|---|---|---|---|
| ≤2.8 | 43 | 32 | 98 | gang counts missing (double-block invisible → 2.8's flat gang_rate was predetermined) |
| 2.9–4.3 | 44 | 32 | 98 | + own pending combat plan |
| 4.4–4.6 | 45 | 32 | 98 | + relation ids (pairings/attachments) |
| 4.7–4.9 | 48 | 32 | 98 | + G1/G2 relation cols (contract **v1**) |
| 4.10 | 48 | **256** | **322** | **v2**: `MAX_PERM` 32→256 (late-game truncation fix, §2a) + `perm_order` truncation priority; columns unchanged |
| v3-era | **45** | 256 | 322 | **v3**: id match-key cols 45–47 deleted; + `edges` (128×4) & `choice_feat` (16×12); `*_ids`→`*_grpid`, `*_cardid` deleted; G1 closed; action layout untouched (spec: `OBS2_DESIGN.md` §7) |
