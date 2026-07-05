# Engine Architecture: The Whiteboard Model

> **Status:** Foundational design decision. This is the architectural backbone for the
> `mtgenv` rules engine. Read this before `docs/plans/ENGINE_PLAN.md`.
>
> **Source of the idea:** MTG Arena's own rules engine, as described by its developers:
> - [On Whiteboards, Naps, and Living Breakthrough](https://magic.wizards.com/en/news/mtg-arena/on-whiteboards-naps-and-living-breakthrough)
> - [Dev Diary: Zurgo, Thunder's Decree](https://magic.wizards.com/en/news/mtg-arena/dev-diary-zurgo-thunders-decree)
> - [Dev Diary: Creating Arena-Powered Cube](https://magic.wizards.com/en/news/mtg-arena/dev-diary-creating-arena-powered-cube)
> - [Dev Diary: Sylvan Library](https://magic.wizards.com/en/news/mtg-arena/dev-diary-sylvan-library)

The user has directed that, absent a compelling reason otherwise, we adopt MTGA's
internal model. The cards those diaries discuss (Zurgo, Sylvan Library, Living
Breakthrough, Necromancy, Through the Breach…) are **out of near-term scope**, but the
architecture must be *expressive enough that they become possible later without a
rewrite*. That extensibility is the whole point of this document.

---

## 1. What MTGA actually does

| MTGA concept | What it is |
| --- | --- |
| **GRE** (Game Rules Engine) | Core engine (C++/CLIPS). Knows *only* structural Magic: priority, the stack, turn/phase/step progression, casting steps, combat, damage, SBAs. **Contains zero card-specific knowledge.** |
| **The Whiteboard** | A mutable, ordered to-do list of the concrete game actions the engine is *about to* perform. e.g. `Destroy Colossus / Destroy Leveler / Destroy Bears`. |
| **"Naps"** | The GRE writes intended actions to the whiteboard, then *suspends*. Card logic runs during the nap and rewrites the whiteboard. The GRE resumes and executes whatever survived. |
| **CLIPS rules** | Card abilities expressed as declarative **pattern → action** production rules. They fire opportunistically during naps, match against game state / whiteboard entries, and erase/add/replace entries. Cards never reference each other; correct interactions *emerge*. |
| **GRP** (Game Rules Parser) | A Python compiler turning English oracle text into CLIPS rules. ~80% of cards are generated automatically; genuine oddballs are hand-authored. |
| **Agendas** | Named, **strictly ordered** processing phases ("a carefully choreographed ballet"): triggered-ability collection, replacement-effect application, state-based actions, etc. Ordering bugs here break everything. |
| **Qualifications** | Status markers the engine attaches to objects ("can't be sacrificed", indestructible, "can't be countered"). The whiteboard-rewrite pass consults them rather than each ability blocking actions directly. |
| **Layers** | The CR 613 continuous-effects layer system. Fully **recomputed** whenever an object changes zones, a step/phase boundary occurs, or an ability is granted/removed. (The Zurgo bug was a *missed* recompute at a step boundary.) |

### Why it's powerful
- **Decoupling.** "The GRE has no idea that either Yawgmoth's Will or Meddling Mage
  exists, and neither one knows about the other." Composition is emergent, not coded
  pairwise. N cards → N rules, not N² special cases.
- **Uniformity.** Replacement effects, prevention, redirection, "can't" effects, and
  cost/▢ modifications are *all the same mechanism*: a rule that rewrites pending
  actions before they commit.
- **Data, not code.** Cards are data (rules), so the engine binary is fixed and the card
  pool grows by adding data. This is exactly what an RL pipeline wants (fast core, large
  swappable card set).

### The costs (called out by the devs — we inherit these problems)
- **Agenda ordering** is subtle and load-bearing. We must make it explicit and tested.
- **Object identity across zones / LKI.** A spell that becomes a permanent is a *new*
  object; shuffling makes *new* objects that don't carry continuous effects. Historical
  values (e.g. Living Breakthrough's noted mana value) must be **stored**, not looked up
  live. → first-class object identity + last-known-information snapshots.
- **Recompute timing.** Layers/qualifications must be recomputed at the right beats or
  you get the Zurgo class of bug. → an explicit "dirty → recompute" discipline.
- **Resolution shape.** MTGA "is just not set up well to add dynamic additional steps"
  during resolution (why Splice/Through the Breach were hard). We should consciously
  decide how flexible our resolution loop is.

---

## 2. The Rust translation

We mirror the model with three layers:

```
┌──────────────────────────────────────────────────────────────────┐
│  CORE ENGINE  (the GRE)  — card-agnostic                           │
│  turn/phase/step machine · priority loop · the stack · casting &   │
│  resolution skeleton · combat skeleton · SBA loop · damage · mana  │
│  Drives everything by emitting EVENTS and building WHITEBOARDS.     │
└───────────────┬──────────────────────────────────────────────────┘
                │ events / whiteboards (typed, card-agnostic)
                ▼
┌──────────────────────────────────────────────────────────────────┐
│  EFFECT RUNTIME  (the CLIPS layer)                                  │
│  A registry of RULES (pattern → action) attached to objects.       │
│  - replacement/prevention rules rewrite whiteboards                 │
│  - triggered abilities watch the event bus, push to the stack       │
│  - continuous effects contribute to the layer computation           │
│  - static "qualifications" mark objects with capabilities/bans      │
└───────────────┬──────────────────────────────────────────────────┘
                │ interpreted from
                ▼
┌──────────────────────────────────────────────────────────────────┐
│  CARD DATA  (the GRP output)                                        │
│  Each card = a declarative bundle of abilities in our Effect IR.    │
│  Near-term: hand-authored for a tiny pool. Long-term: compiled      │
│  from oracle text / MTGJSON / Forge card scripts.                   │
└──────────────────────────────────────────────────────────────────┘
```

The cardinal rule, enforced by crate boundaries (see ENGINE_PLAN): **the core engine
must not `match` on card names or card-specific behavior.** Everything card-specific is
data interpreted by the effect runtime.

### 2.1 The Whiteboard

A whiteboard is the staged, not-yet-committed batch of atomic mutations the engine
intends to apply *together*, in one place where rules may rewrite them.

```rust
/// A single intended mutation to game state. Card-agnostic and inspectable.
pub enum Action {
    Destroy   { obj: ObjId, source: Option<ObjId> },
    Sacrifice { obj: ObjId, by: PlayerId },
    Damage    { target: Target, amount: u32, source: ObjId, kind: DamageKind },
    Draw      { player: PlayerId, count: u32 },
    LoseLife  { player: PlayerId, amount: u32 },
    MoveZone  { obj: ObjId, to: Zone, .. },
    TapUntap  { obj: ObjId, tap: bool },
    AddCounters { obj: ObjId, kind: CounterKind, n: i32 },
    CreateToken { spec: TokenSpec, controller: PlayerId },
    // … grows with the IR vocabulary
}

pub struct Whiteboard {
    pub reason: WbReason,        // e.g. ResolveSpell(stack_id), CombatDamage, Cleanup-SBA
    pub actions: Vec<Action>,    // ordered; rules may erase / replace / insert
    pub ctx: ResolutionCtx,      // controller, source, chosen modes/targets, X, etc.
}
```

Lifecycle of every batch of game actions (the "nap"):

1. **Materialize.** The core (or a resolving ability) fills a `Whiteboard` with the
   actions it *intends* to perform.
2. **Rewrite pass (replacements/prevention).** Run applicable replacement & prevention
   rules. Per CR 614.5/616, **at most one replacement modifies a given event**, and each
   replacement applies **once** per event — track which have seen which action. Rules may
   delete actions (prevention/"can't"), mutate them (redirect, "instead"), or expand one
   into several. Loop until no rule wants to act (a fixpoint, with the once-per-event
   guard preventing infinite loops).
3. **Commit.** Execute the surviving actions, emitting an `Event` for each completed one.

> The Zurgo "Destroy all / but not the indestructible one" and "Sacrifice Serra Angel /
> Sacrifice token, but one can't be sacrificed" examples are *exactly* step 2 erasing a
> whiteboard entry because the target carries an indestructible/can't-be-sacrificed
> **qualification**.

### 2.2 Events and the agenda pipeline

Committing actions produces **events** (`PermanentDied`, `DrewCard`, `DealtDamage`,
`SpellCast`, `EnteredBattlefield`, `PhaseBegan`, …). Events drive triggered abilities and
are the substrate for "looking back in time" (CR 603.10) and LKI.

The engine advances through an explicit, ordered **agenda** loop — our version of MTGA's
"choreographed ballet" — run to a fixpoint between priority passes (CR 117.5, 704.3,
603.3):

```
loop {
    recompute_continuous_effects_if_dirty();   // §2.4 layers + qualifications
    let sbas = collect_state_based_actions();   // CR 704
    if !sbas.is_empty() { perform_as_whiteboard(sbas); continue; }   // SBAs repeat
    let triggers = drain_pending_triggers();    // CR 603.3, APNAP-ordered (603.3b)
    if !triggers.is_empty() { put_on_stack(triggers); continue; }
    break;  // game-state stable → a player gets priority
}
```

The order — recompute → SBAs (loop) → put triggers on stack → priority — is law and is
covered by tests. This is where MTGA warns ordering bugs hide.

### 2.3 Abilities as rules (the IR)

Every card ability is one of a small set of rule kinds, all data:

- **Activated / Spell ability** — `cost → effect`, where `effect` is a tree of `Action`
  builders + choices. Resolution materializes a whiteboard.
- **Triggered ability** — `event pattern [+ intervening-if condition] → effect`
  (CR 603). Includes state triggers (603.8) and delayed triggers (603.7).
- **Replacement / prevention rule** — `event/action pattern → rewrite` (CR 614/615).
  Registered so the whiteboard rewrite pass (§2.1 step 2) can find it.
- **Continuous / static effect** — contributes to a **layer** (§2.4) and/or sets a
  **qualification** on objects (e.g. grants flying, "can't be sacrificed", cost
  reduction). CR 611/613.

The IR is an enum-of-effects ("effect vocabulary"): `DealDamage`, `Draw`, `Destroy`,
`Mill`, `Pump{+x/+y}`, `AddMana`, `CreateToken`, `Search`, `Counter`, modal/choice nodes,
target selectors, conditions, durations, etc. Cards compose these. **Escape hatch:** a
`Native(fn)` variant lets a genuinely unique card supply hand-written Rust — the MTGA
equivalent of "I gave up and wrote the CLIPS by hand." We want ≥1 such hatch so no card
is ever *impossible*, only *not-yet-done*.

### 2.4 Continuous effects, layers, qualifications

Characteristics of every object are computed as `base ⊕ layered transformations`
(CR 613). We keep a `CharacteristicsCache` recomputed from scratch whenever a **dirty**
signal fires: zone change, step/phase boundary, ability grant/remove, timestamp change,
counter change. Recompute order = the seven layers (1 copy → 2 control → 3 text → 4 type
→ 5 color → 6 abilities → 7 P/T, with 7a–7d sublayers), respecting timestamps (613.7)
and dependencies (613.8).

**Qualifications** are boolean/typed flags materialized by this same pass (indestructible,
hexproof, shroud, can't-be-sacrificed, can't-attack, must-attack, "can't be countered",
phased-out, etc.). The whiteboard rewrite pass and the legality checks read them. This is
MTGA's exact trick: abilities don't intercept actions directly — they paint markers, and
the structural machinery respects the markers.

### 2.5 Object identity, LKI, copiable values

- Every game object has a stable `ObjId`. Changing zones (CR 400.7) generally yields a
  **new** `ObjId` — continuous effects and counters do not follow unless a rule says so.
- **Last Known Information** (CR 603.10, 608.2g): when an object leaves, snapshot the
  characteristics other effects will need. Store snapshots; never look up a moved object
  live.
- **Copiable values** (CR 707): store the copiable characteristics separately from the
  computed ones so copy effects in layer 1 work and so "historical" values (Living
  Breakthrough's noted mana value of `spell #278`) are recorded at note-time, not
  recomputed.
- **Runtime-minted objects carry their behaviour as a registered def, not a name.** Tokens
  (CR 111) and emblems (CR 114) are created at resolution with no printed card; each points
  at a registered `CardDef` in a reserved `grp_id` block (tokens `9000+`, emblems `9500+`),
  so `def_of` supplies its abilities — behaviour stays *data*, never a name-match in the core.
  An **emblem** is the minimal case: no characteristics (CR 114.2), just a triggered/static
  ability that **functions from `Zone::Command`** (`Ability::FunctionsFrom(vec![Zone::Command])`,
  the same zone-of-function marker graveyard-triggers use). The **command zone** is modeled
  per-player (`Player.command`); it is scanned for functioning triggers exactly like the
  graveyard, and nothing in the SBA/removal path touches it, so emblems are permanent. A
  triggered ability that reads "**that much**" (Dellian's emblem: opponent loses the life you
  gained) carries the triggering amount on the trigger's `x`, read as `ValueExpr::X`.

### 2.6 Decisions carry constraints

When the engine needs a player choice, the request enumerates legal options **and**
constraints — mirroring MTGA extending player-choice with "forbidden values" for X, and
tracking hidden info like "which cards were drawn this turn" (Sylvan Library). This is the
same `DecisionRequest`/`DecisionResponse` boundary the agent/RL/MTGA-client backends plug
into — see `docs/plans/ENGINE_PLAN.md` §decision-interface and `docs/plans/GYM_PLAN.md`.
The whiteboard model and the agent interface meet here: the engine produces fully
specified, legal-option-masked choice points; a backend (scripted AI, Python RL, or the
decompiled MTGA protocol) answers.

**No-rewind is a pragmatic economy, not an architecture law.** Today the cast path pre-masks
so tightly that no decision ever needs undoing — affordability and target legality are
resolved before anything is committed (e.g. target-dependent cost modifiers, CR 601.2f: the
final cost is computed *after* targets are chosen and each target candidate is pre-filtered to
what the caster can pay, so `auto_pay` never underpays). Keep that where the pre-filter stays
cheap — an exact legal mask is valuable to an RL agent. But it is **not** a rule the design must
bend to preserve: when a mechanic makes pre-filtering combinatorial (convoke/improvise-class
alternate payments, stacked cost modifiers × restricted mana, modal + X + affordability
interactions), the sanctioned path is a **transactional pending-cast** — snapshot/hold the cast
context and allow cancel/rollback before commitment. That is exactly MTGA's GRE model (a pending
cast the player can back out of), and mirroring the GRE is a stated project goal — so a future
agent should reach for rollback rather than over-engineer pre-filtering to avoid a rewind the
architecture actually permits.

---

## 3. Worked examples (from the dev diaries)

| Card / situation | How the model handles it |
| --- | --- |
| **Indestructible vs. "destroy all"** | Core materializes `Destroy` actions for each creature. Rewrite pass sees the indestructible *qualification* on one and erases that `Destroy` action. Core commits the rest. No card knows about the others. |
| **Zurgo "can't be sacrificed"** | The ban is a qualification set by a continuous effect. A `Sacrifice` action targeting it is erased in the rewrite pass. *Required fix:* recompute qualifications at the step boundary **before** the sacrifice whiteboard is evaluated (the Zurgo bug). |
| **Sylvan Library "cards drawn this turn"** | Engine tracks `drawn_this_turn` per object as hidden state updated on `DrewCard` events. The triggered/replacement effect's choice request is constrained to those objects. |
| **Sylvan Library pay 8 life = 4 twice** | The effect IR expands one "pay 4 life" into the appropriate number of discrete `LoseLife`/payment actions, so each is a separate event (Font of Agonies triggers twice). Compositional actions, not a single lump. |
| **Living Breakthrough noted mana value** | At note-time, store the *copiable/historical* mana value with the note (`spell #278 → mv 5`). Later checks read the stored value, not the object (which may now be a permanent). |
| **X spells "forbidden values"** | The `DecisionRequest` for choosing X carries a `forbidden: {…}` constraint; the legal-option set excludes them. |

---

## 4. Build-now vs. keep-possible

**Now (small pool, simple cards):**
- Whiteboard + commit + event bus, even if early whiteboards have one action.
- The agenda loop (recompute → SBA → triggers → priority) wired correctly from day one —
  this is structural, not card-specific, and retrofitting it is painful.
- Layer system present but exercising only a few layers (P/T pumps, type/color changes,
  keyword grants).
- Effect IR with a starter vocabulary + the `Native` escape hatch.
- Qualifications for the common evergreen set (flying, indestructible, hexproof, …).

**Keep possible (do not foreclose):**
- Replacement-effect rewrite richness (redirect, "instead", skip, double) — keep the
  rewrite pass a real fixpoint with the once-per-event guard, not a special case.
- Copy layer & CDAs; historical/LKI snapshots — keep copiable values separate now even
  if nothing copies yet.
- Hand-authored `Native` abilities for oddballs.
- A future oracle-text → IR compiler (the GRP analog); near-term we may instead import
  Forge's declarative card scripts (`../forge-ai/forge-gui/res/cardsfolder/`) as a corpus.

## 5. Open questions (resolve as we build)
- Granularity of `Action`: how atomic? (Too coarse → can't express rewrites; too fine →
  slow.) Start medium, split when a card forces it.
- Resolution flexibility: do we allow dynamic added steps mid-resolution (the
  Splice/Through-the-Breach problem)? Decide explicitly; default to "no" like MTGA, but
  don't hard-code assumptions that block it.
- **Decided — rule representation:** a **typed effect IR (enums) interpreted by Rust, NOT a
  CLIPS-style production-rule engine.** We keep the whiteboard *architecture* (card-agnostic
  core + uniform rewrite of pending actions; cards as data) but implement the "rules" as typed
  data — `EventPattern → Effect` (triggers), `ActionPattern → Rewrite` (replacements),
  `StaticContribution` (continuous) — for speed, determinism, and cheap cloning (the RL/MCTS
  constraint that a general forward-chaining engine fights). A future oracle-text → IR compiler
  is the GRP analog; the `Native` hatch covers the tail.
- How much of MTGA's *exact* agenda order to replicate vs. derive straight from CR 117/603/704.
