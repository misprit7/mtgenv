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

**Staging — each step is independently useful:**

1. **v3 bundle (already proposed, awaiting go):** drop deck-local one-hots; add stack
   instance/target/source ids (closes G1 inside the current tensor format); optionally
   engine-exported edges consumed as attention bias while keeping fixed tables. This is
   §2-lite + §3 without the variable-length rebuild.
2. **Pointer-argument actions on fixed tables:** swap `Discrete(322)` for verb+pointer while
   obs stays v3. Kills the size-coupled action breaks before SOS forces them.
3. **Full OBS2 (variable-length entities + IR characteristics):** do this at the swine→SOS
   transition — the moment the current design's remaining assumptions (small decks, no
   targeting, homogeneous entities, identity-memorization-is-fine) all break simultaneously
   anyway. Training on real cards then starts on the right substrate instead of retrofitting
   under a live ladder.

Nothing here launches or changes contracts without explicit go (campaign hold in force).
