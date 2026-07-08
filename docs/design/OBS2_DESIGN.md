# OBS2 — the from-scratch obs/action redesign

*Written 2026-07-08. Status: **design proposal, not implemented.** Companion to
`OBS_ACTION_SPACE.md` (which documents the current v2 contract and its honest gaps). This doc
answers: "if we started over, what would the observation and action space look like?" — and
lays out the staged path from v2 to there. Every design choice below is annotated with the
concrete failure from the 2.x–4.x campaigns that motivates it.*

## 0. Principles

1. **Entities are a set, not a table.** Position in a fixed matrix is not identity; row caps
   are not physics.
2. **Identity is what a card does, not its catalog number.** Encode characteristics (and our
   Effect IR), not one-hots or hashes-as-features.
3. **Relations are engine facts, exported explicitly.** The engine knows who blocks whom; the
   network should never have to rediscover it by matching id columns.
4. **Actions point at entities.** The action interface names *things*, not *slots*, so it
   never changes shape when tables grow.
5. **Hidden information stays structurally impossible to leak** (encoder reads `PlayerView`
   only) and **legality stays the engine's job** (masks, never penalties). These are the v1/v2
   decisions that proved right; OBS2 keeps them.

## 1. Observation: one variable-length entity list

Replace the three fixed matrices (`bf_feat` 256×48, `hand_feat` 16×18, `stack_feat` 8×18) with
a single **list of entity tokens**, each `{zone, characteristics, state}`, plus a count:

- **`zone`** — small one-hot (battlefield / my-hand / stack / graveyard / exile / …). One
  schema for all zones; a hand card and a permanent are the same kind of token with different
  zone tags and different populated-state blocks.
- **`characteristics`** — the card-identity block (§2), identical for every zone.
- **`state`** — zone-specific dynamics: P/T-as-computed, damage, tapped, sick, counters,
  controller; `castable` for hand rows; controller for stack rows. Sparse columns are fine —
  the schema is unified, the values are zero where inapplicable.

**Padding becomes a batching concern, not a contract concern.** The engine emits exactly the
entities that exist; the Python batcher pads to the *batch max* dynamically. This kills both
failure modes we actually hit:

- the **MAX_PERM truncation bug** (39 permanents silently cut at 32 rows — user-found; class
  eliminated: there is no cap to outgrow), and
- the **dead-row decode cost** (v2 pays to encode/ship/decode 240 empty rows every step;
  measured ~2.3× total cost, 648→281 sps). With variable length you pay for what exists —
  a heralds board is ~10 tokens, not 280.

Most future changes become **additive** (a new state column, a new zone tag) instead of
contract-breaking resizes.

> **Revision (2026-07-08, design review with the user): contract-level variable length is
> demoted — the fixed-shape contract stays.** The 2.3× cost decomposes into (a) transport
> (solved by sparse wire format alone: ship only occupied rows, pad into a preallocated zero
> buffer Python-side — observed tensors byte-identical, no fingerprint bump), (b) forward-pass
> compute (present-masking hides padding's *contribution*, not its O(N²) attention *cost* —
> solved by **gathering present rows inside the extractor**: index-select real entities, run
> segment attention with a block-diagonal batch mask, scatter pointer logits back to fixed
> slots — mathematically identical to ragged batching but ~40 lines in the network instead of
> replacing SB3's rollout buffer), and (c) rollout-buffer memory + cap elimination — the only
> things a truly variable-length contract uniquely buys, and neither currently binds. Verdict:
> (a)+(b) are pure perf patches with no contract break; the PyG-style custom-buffer rebuild is
> off the roadmap unless buffer memory someday binds. The rest of this section describes the
> original (superseded) contract-level design for the record.

Hidden zones get **explicit unknown tokens**: opponent's hand as k face-down tokens (k = the
public hand count), library as a count in globals. This changes nothing about information
content today, but gives a future belief/inference head something to attach predictions to —
and keeps "number of unknown things" in the same representational currency as known things.

Optionally (cheap, deferred-friendly): a short **recent-action history** — the last N
(actor, verb, target-entity) triples as tokens. This is the one information channel genuinely
absent from v2; play patterns carry inference in hidden-information games.

## 2. Card identity: characteristics + Effect IR, not catalog numbers

v2 has three id notions (hashed `grp_id` → embedding; deck-local one-hot; float instance-id
match keys). OBS2 replaces the first two with **content**:

- **Characteristics block**: mana cost as colored pip counts, types, subtypes (hashed bag),
  printed P/T, keyword flags — straight from the engine's `Characteristics`.
- **Effect IR features**: this is our unfair advantage. Because the engine is card-agnostic,
  every card's rules text already exists as *structured data* (the Effect IR). Encode it as a
  bag of effect-opcodes with parameters — "deal N damage to target X", "draw N", "destroy
  target Y" become feature vectors, not memorized identities. No text embeddings, no
  memorization, zero-shot transfer to unseen cards. No other MTG-AI substrate on this machine
  (Forge included) can do this cleanly; ours can because cards-as-data is architecture law.
- **Residual id embedding**: a small learned embedding on `grp_id` survives only as a
  tie-breaker for cards whose IR features coincide. It is explicitly *not* load-bearing.

This eliminates: the hash-collision risk (4096-bucket birthday problem at SOS scale), the
deck-local one-hot wart (obs width depending on deck composition — the v3 bundle already
proposes dropping it), and the "new set = relearn every identity" tax. It is the only identity
scheme that survives the full-pool / random-deck endgame the user has planned.

## 3. Relations: an explicit edge list

v2 encodes relations as float match-key columns (`blocking_id`, `attached_to_id`,
`instance_id`) that the network must learn to *compare across rows* — and stack targeting
doesn't exist at all (gap G1). OBS2 exports relations the way the engine already knows them:

```
edges: [(src_entity_idx, dst_entity_idx, edge_type)]
edge_type ∈ { BLOCKS, ATTACHED_TO, TARGETS, CONTROLS, STACK_SOURCE, DECISION_SOURCE, DECISION_CANDIDATE, … }
```

Consumed as an attention bias / graph structure by the network (the 4.7+ attention arm already
builds exactly this adjacency — from id-equality; OBS2 hands it the adjacency directly and
deletes the discovery problem).

**The unification that makes this elegant:** the *current decision* becomes a token too. The
`is_decision_source` / `is_decision_candidate` flag columns become `DECISION_SOURCE` /
`DECISION_CANDIDATE` edges from the decision token to entities. One mechanism carries all
relational context — combat pairings, aura hosts, spell targets (G1 closed by construction),
and decision framing. `decision_ids` stops being a special bypass channel; the decision token
carries the request kind and its scalars, and relates to its candidates like everything else.

## 4. Actions: (verb, pointer-arguments), not positional slots

Replace `Discrete(322)` positional buckets with a structured, autoregressive action:

```
verb ∈ { PLAY, ACTIVATE, TARGET, DECLARE_ATTACKER, ASSIGN_BLOCKER, PASS/COMMIT,
         YES, NO, CHOOSE_MODE, CHOOSE_COLOR, CHOOSE_NUMBER }
args = zero or more entity pointers (selected by pointing at entity tokens)
     | a scalar head (numbers/X) | color one-hot
```

Pick the verb, then pick each argument by a pointer head over entity tokens — the mechanism
the 4.9 result already validated on the output side (content-based pointer beat indexed slots
*in interaction with* the punisher scriptpool). Modes become tokens with IR features, so
modal choices are also pointer selections. The engine's legality enumeration masks
(verb, argument) pairs exactly as it masks slots today.

What this buys, in order of importance:

1. **The action interface never changes shape again.** No bucket bases shifting +224 when a
   table grows (v1→v2's 98→322 head invalidation), no remap tables, no `SchemaAdapter`
   temptations. Ladder resets stop being forced by *size* changes.
2. **Scales to real MTG.** Multi-target spells, X costs, damage-assignment order (gap G2),
   sacrifice/discard choices — all are "point at k entities, maybe emit a number", which is
   this interface's native shape. The v2 slot enum would need a new bucket per request kind.
3. **Binding is direct.** The user's core objection to mean-pool — "impossible to see the
   relationship between action choice and card ids" — is resolved at the interface: the
   action *is* a reference to the entity's content.

Combat stays autoregressive exactly as today (declare one attacker per step with pending-plan
visibility, two-level blocker assignment) — that decomposition worked and survives unchanged;
only the *addressing* changes from `PERM[i]` to a pointer at the entity.

### 4a. The partial-decision observability invariant (2026-07-08 user review)

Multi-step decisions (autoregressive combat, multi-target selection, damage division) mean the
policy is called repeatedly *inside* one resolution. The rule: **at every sub-step, the
observation alone must carry the full commitment prefix — the action mask must never be the
only carrier of decision context** (the mask feeds neither the value net nor the features).
We were burned by exactly this once: `is_pending_combat` (col 44) had to be retrofitted in the
4.x audit because a policy mid-DeclareAttackers could not see its own partial attacker set.

Mechanism in OBS2 terms:

- the **decision token** carries the request kind plus sub-step scalars ("target 2 of 3",
  damage remaining to assign);
- **`PENDING_PLAN` edges** (decision → entity) for every pick already made in the in-flight
  decision;
- **relation edges appear immediately as picks are made mid-decision** — the `BLOCKS` edge
  from a blocker assigned at sub-step 1 is visible in sub-step 2's observation; pendingness is
  marked by the accompanying `PENDING_PLAN` edge on the same entity rather than by doubling
  the type vocabulary (`PENDING_BLOCKS` etc.).

Enforcement: for each multi-step `DecisionRequest` kind, an expect-test walks a scripted
mid-decision sequence and snapshots the observation at each sub-step, asserting the pending
picks are visible (edges present, scalars updated). General audit rule: any state the legality
enumerator reads to build the mask that is not derivable from the observation is a bug — that
check applied from the start would have caught the col-44 gap before it cost a training run.

## 5. What OBS2 keeps from v1/v2

- **Structural hidden-info safety**: encoder reads only `PlayerView`. A leak stays impossible,
  not just unlikely.
- **Engine-side legality masking**: no invalid-action penalties; masks derived from the
  engine's own enumeration (the `_codec_layout()` discipline of deriving layout from the live
  contract, never hard-coding).
- **The single decision seam** (`Agent` trait / one `DecisionRequest` funnel) — untouched.
- **Contract fingerprinting + clean-slate rating ladders per version** (user directive: no
  cross-version adapters, ever). OBS2 would be a fingerprint bump like any other — but its
  point is that it should be close to the *last* breaking bump, because after it most changes
  are additive.
- **Globals** largely as-is (turn/phase/life/counts/floating mana), minus the decision one-hot
  (moves onto the decision token).

## 6. Cost, risks, and the staged path

This is a v4-scale rebuild: engine export format, the pump's decode path (flat buffer of
entities + edges with offsets — cheap to decode, and *smaller* than v2's padded tensors on
every realistic board), dynamic batching in Python, and new network heads. Risks worth naming:
variable-length batching complicates SB3 integration (custom collation); pointer-only actions
need careful mask plumbing for multi-step arguments; Effect-IR featurization needs a stable
opcode vocabulary (version it with the IR).

**Status after user review (2026-07-08):** the in-extractor present-row gather (§1 revision,
item b) is **greenlit and in implementation** — pure perf, no contract break. The verb+pointer
action change (§4) is **greenlit in principle**; it requires content for the abstract slots
(choice/decision tokens in the obs), so it rides the same v3 contract break as the edge export
and the one-hot removal — one ladder reset, not several. Effect-IR identity (§2) is parked
behind the user's gate: not until simple envs are played near-optimally. Sparse transport
(§1 revision, item a) is approved-adjacent but unscheduled — it can land any time the
opponent-servicing-loop cost is worth attacking.

**Staging — each step is independently useful:**

1. **v3 bundle (already proposed, awaiting go):** drop deck-local one-hots; add stack
   instance/target/source ids (closes G1 inside the current tensor format); optionally
   engine-exported edges consumed as attention bias while keeping fixed tables. This is
   §2-lite + §3 without the variable-length rebuild.
2. **Pointer-argument actions on fixed tables:** swap `Discrete(322)` for verb+pointer while
   obs stays v3. Kills the size-coupled action breaks before SOS forces them.
3. **Full OBS2 (unified entity schema + IR characteristics):** do this at the swine→SOS
   transition — the moment the current design's remaining assumptions (small decks, no
   targeting, homogeneous entities, identity-memorization-is-fine) all break simultaneously
   anyway. Training on real cards then starts on the right substrate instead of retrofitting
   under a live ladder. *(Per the §1 revision, this stage no longer includes a variable-length
   contract or buffer rebuild — the contract stays fixed-shape; sparse transport + in-extractor
   gather deliver the perf independently of any contract bump.)*

Nothing here launches or changes contracts without explicit go (campaign hold in force).

## 7. The concrete v3 contract — full shapes and the migration plan

*Added 2026-07-08 on user request. This is the implementable spec for the one approved contract
break, designed to extend to the full SOS pool (targeted spells, auras/equipment, modal cards,
suspend, convoke) with named simplifications — not a toy-pool special case.*

### 7.1 Constants

`MAX_PERM 256 · MAX_HAND 16 · MAX_STACK 8` (unchanged) · **`MAX_CHOICE 16`** (new) ·
**`MAX_EDGES 128`** (new). Fixed shapes stay permanently (§1 revision); these are caps with
deterministic truncation priorities, not guesses to outgrow.

### 7.1a Naming: exactly two id spaces (user decision, 2026-07-08)

- **`grpid`** — *which Magic card this is* (oracle/printing identity; Arena GRE's GrpId).
  The only id that appears in observation tensors: the `*_ids` channels are renamed
  **`bf_grpid` / `hand_grpid` / `stack_grpid` / `decision_grpid`** at the v3 break (they're
  obs-spec keys, so the rename rides the same fingerprint bump). Hashed → shared embedding.
- **`entityid`** — *a particular object instance in a game* (a permanent, a card in a zone, a
  spell on the stack — and abilities on the stack, which today live in a **separate stack-id
  space**: that space is abolished and folded into the one `entityid` allocator; CR-wise
  they're all objects, and Arena's GRE likewise gives stack abilities plain instanceIds).
  Replaces every `instance_id`/object-id/stack-id notion in the engine, protocol, client and
  replays. **Never appears in a tensor** — `obs.rs` resolves entityid → row at encode time.
- **Nothing else is an id.** Players are seat numbers (0/1), not ids. Edge endpoints are
  **row positions** in the current observation — the fields are named `src_row`/`dst_row`
  precisely so "id" never labels a positional value. Deck-local one-hots are deleted (§7.3).

Implementation timing: the obs-key renames land inside the v3 break; the engine-side
`instance_id` → `entityid` rename + stack-id unification is a pure refactor that can land as
its own commit any time before it (protocol note: the target enum's `Object`/`Stack` variants
collapse into one `Entity` variant when the spaces merge).

### 7.2 Row space (edge addressing)

Edges name entities by **row position in this observation** (`src_row`/`dst_row`), one flat
space shared with the attention layout — entityids never enter the network:

```
0–255    battlefield rows (perm_order)     272–279  stack rows
256–271  hand rows                         280 you · 281 opponent (player tokens)
                                           282 the current decision (pseudo-entity)
```

### 7.3 Observation tensors

| Tensor | dtype | v2 | v3 | Change |
|---|---|---|---|---|
| `globals` | f32 | 69 | **69** | unchanged (decision one-hot stays; token unification deferred to OBS2-full) |
| `bf_feat` | f32 | 256×48 | **256×45** | drop cols 45–47 (`instance_id`/`blocking_id`/`attached_to_id`) — superseded by `edges` |
| `bf_ids` | i64 | 256 | 256 | kept — hashed `grp_id` → embedding |
| `bf_cardid` | f32 | 256×D | **—** | **deleted** (deck-local one-hot; D was deck-dependent) |
| `hand_feat` | f32 | 16×18 | 16×18 | unchanged |
| `hand_ids` / `hand_cardid` | | 16 / 16×D | 16 / **—** | ids kept / one-hot deleted |
| `stack_feat` | f32 | 8×18 | 8×18 | unchanged — targeting arrives via `edges` (**G1 closed**) |
| `stack_ids` / `stack_cardid` | | 8 / 8×D | 8 / **—** | ids kept / one-hot deleted |
| `decision_ids` / `decision_cardid` | | 1 / 1×D | 1 / **—** | ids kept / one-hot deleted |
| `edges` | i32 | — | **128×4** | NEW: `(src, dst, type, k)`, pad rows −1 |
| `choice_feat` | f32 | — | **16×12** | NEW: content tokens for the current decision's abstract options |

Principle for `bf_feat`: **scalar features stay, float match-keys die.** The redundant unary
flags (attacking 39, blocking 40, decision src/cand 41–42, `blocked_by` count 43, pending 44)
are kept — they're correct encodings of per-row properties and cost nothing; only the three
raw-id columns (which required cross-row equality matching to mean anything) are deleted.

### 7.4 Edge types

| `type` | src → dst | Notes |
|---|---|---|
| 0 `BLOCKS` | blocker → attacker | replaces `blocking_id` matching |
| 1 `ATTACKS` | attacker → defending player | says *whom* (planeswalkers/multiplayer later) |
| 2 `ATTACHED_TO` | aura/equipment → host | replaces `attached_to_id` |
| 3 `TARGETS` | stack object → entity or player | **G1**; `k` = target slot order |
| 4 `STACK_SOURCE` | ability on stack → its source permanent | |
| 5 `PENDING_PICK` | decision (282) → already-picked entity | the §4a commitment prefix; `k` = pick/slot index |

Unary decision context (source/candidate flags) stays in the feature columns — a flag is the
correct encoding for a per-row property; edges are reserved for *pairings* that need cross-row
identity. `k` (4th column) carries ordering where it matters (multi-target slots, pick order);
damage-division *amounts* stay out (G2, still behind engine-default COMMIT). If 128 edges ever
overflow (never observed; bound analysis: blocks+attachments+targets+pending ≲ 100 on
degenerate boards), truncation priority is `TARGETS > PENDING_PICK > BLOCKS > ATTACHED_TO >
ATTACKS > STACK_SOURCE` with a logged counter.

Consumption: per-(type, direction) learned bias on attention logits between src/dst rows — the
adjacency the 4.7+ arm currently reconstructs from id-equality, handed over directly.

### 7.5 Choice tokens (`choice_feat` 16×12)

Rows = the current decision's abstract options (only one family is live per decision, so 16
rows cover all buckets): col 0 `present` · 1–4 kind one-hot (mode/color/number/bool) ·
5 value scalar (number value, mode index) · 6–10 color one-hot · 11 reserved. The action codec
maps the live `MODE`/`COLOR`/`NUMBER`/`YES`/`NO` bucket onto choice rows positionally. This
gives every abstract action slot *content* for the pointer head — deleting the
unnormalized-abstract-embedding family that caused the 4.7 logit-scale bug. (Richer mode
content — the mode's Effect-IR features — plugs into these rows later under §2.)

### 7.6 Action space

**`Discrete(322)`, layout unchanged.** The verb+pointer change is realized *head-side*: every
slot's logit is computed from a content token (entity rows for HAND/PERM/PLAYER/STACK — as the
attn arm already does — plus choice rows for the abstract buckets). No bucket bases move, so
the contract break comes from the observation side only.

### 7.7 SOS-extendability (and the named simplifications)

- **Graveyard / exile / library are NOT entity tables in v3** — counts in `globals` only. This
  is a deliberate simplification (flagged, not forgotten): suspend-in-exile and any future
  recursion play blind until a `gy_feat`/`exile_feat` table is appended — same row schema,
  same id channels, new zone table, an *additive* change under this design.
- **Keyword block** (`bf_feat` cols 9–36): audit the SOS pool's keyword needs at
  implementation time and size the block once, inside this break — don't pay a second break
  for a keyword column.
- **Small closed enums stay one/multi-hot** (phase 12, decision-kind 21, types 8, colors 5,
  keywords ~15): a one-hot into the first linear layer *is* an embedding, mathematically —
  embeddings only pay for large sparse vocabularies (card identity, thousands of grp_ids). The
  deck-local one-hot's sins were deck-dependent width and non-transferability, not
  one-hot-ness. The one future large-vocab case is **subtypes** (~300 creature types) —
  unencoded today; hash-bucket multi-hot or bag-of-embeddings when tribal matters.
- `MAX_STACK 8` / `MAX_HAND 16` / `MAX_CHOICE 16` hold for SOS (deep stacks and >16 hands are
  degenerate; truncation priorities: top-of-stack first, newest-drawn last).

### 7.8 Migration plan (ordered; each step gated)

1. **Rust `crates/mtg-py`** — `obs.rs`/`layout.rs`: delete `*_cardid` + bf cols 45–47; emit
   `edges` + `choice_feat` (the engine already holds every relation: combat pairs,
   attachments, stack targets). `codec.rs`: abstract buckets ↔ choice rows; bases unchanged.
   Regenerate expect snapshots; add the §4a mid-decision expect-tests (scripted sub-step walk
   asserting `PENDING_PICK` edges + updated scalars at every sub-step).
2. **Fingerprint bump → v3** — obs_spec change trips the ratings guard by design;
   `bump-env v3`, re-seed the script spine, no adapters (standing rule).
3. **Python** — attn extractor: consume `edges` as per-type attention bias (delete the
   id-equality adjacency builder); pointer logits for abstract slots from `choice_feat`;
   remove cardid input paths. Hold param-parity with the 4.9 arch for a clean comparison.
4. **Gates** — obs↔action agreement test extended to edges (edge row indices resolve to the
   same engine objects the codec maps); real-env smoke (the 4.7 lesson: synthetic tests lie);
   then a short PPO sanity run — *launch user-gated*.
5. **v3 reference run** — the 4.9 recipe retrained on v3 becomes the new ladder anchor
   (user-gated; supersedes menu item (a) if v3 lands first).

Orthogonal and already in flight: the in-extractor present-row gather (no contract impact).
Explicitly NOT in v3: variable-length contract (rejected), Effect-IR identity (parked),
graveyard/exile tables, G2 damage-ordering decisions, decision-token/globals unification
(OBS2-full, at the SOS transition).
