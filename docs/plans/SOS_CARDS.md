# Card-implementation push — Secrets of Strixhaven (`sos`, 271 distinct cards)

Standing workstream: implement the Secrets of Strixhaven set for **limited (40-card) play** in
`mtg-core`, easiest-first, correctness over count. This ledger is the capability index + full
per-card triage, modeled on `SELESNYA_LANDFALL_CARDS.md`.

## ▶ NEXT AGENT — start here (handoff from sos-cards-6, 2026-07-03)

**▶▶ sos-cards-6 handoff (2026-07-03 late night) — READ THIS FIRST. FIRST-PASS MILESTONE DECLARED: 149 authored /
146 fully-faithful / 3 tracked-partial, 562 mtg-core tests green, tree clean, all pushed.** Shipped **8 cards + 8
engine caps + corrected a wrong "first-strike unwired" belief** (first/double-strike combat has been done since
`a15015f`; passing tests prove it — the handoff was wrong). Caps (all with real-path tests): (1) **per-turn
counter-added tracker** `Condition::PutCounterOnSelfThisTurn` (`Object.counter_added_this_turn`, set in the
`AddCounters` executor) → **Fractal Tender**; (2) **`Effect::ForEachTarget{slot,body}`** (apply-to-each of a
VARIABLE multi-target slot, reusing `EffectTarget::Each`; `foreach_current` generalized `ObjId`→`Target` so `Each`
binds players too) → **Homesickness** + **Prismari Charm**; (3) **S19 `ValueExpr::CardsDrawnThisTurn`** → **Fractal
Anomaly**; (4) **`ValueExpr::XOfTriggeringSpell`** (`Object.cast_x` recorded at cast) — completes S21 → **Geometer's
Arthropod**; (5) **"counters put on self" `EventPattern::CountersPutOnSelf{kind}`** + `GameEvent::CountersPut`
broadcast from the `AddCounters` executor → **Pensive Professor**; (6) **S22 `Condition::CastInstantOrSorceryThis
Turn`** (`Player.instants_sorceries_cast_this_turn`); (7) **`Restriction::OnlyIf` wired into the activated-ability
legality gate** (was only honoured for mana abilities) → **Potioner's Trove**; (8) a reusable **`artifact()`**
CardDef builder. Also two zero-cap cards the audit surfaced: **Withering Curse** + **Prismari Charm**.

**KEY LESSON (again): the ledger's "no-cap vein is mined out" was WRONG.** A fresh unauthored-card audit (verified
vs the interpreter) found 2 zero-cap cards + a vein of 1-small-cap cards. **The genuinely-cheap vein is now swept.**
What remains all needs a MODERATE new capability (verified — don't scope as "cheap"):
- **`{X}` in an ACTIVATED ability cost** (`activate_ability` at priority.rs hardcodes `x: None`) → unblocks **Berta,
  Wise Extrapolator** + **Emil, Vastlands Roamer** (both want `{X},{T}: create a Fractal with X counters`; the
  dynamic-token-counters cap + any-color mana are DONE, so this ONE cap clears both). **Best next pick.**
- **`ValueExpr::CountersOnTarget(n)` + a commit-between-steps flush** → **Growth Curve** ("put a +1/+1, THEN double"
  — the double must read the count AFTER the first counter commits; `eval_value` reads pre-commit state, same class
  as the #61 CreateToken-ordering bug — needs a flush like CreateToken got, NOT just the ValueExpr).
- **`CardFilter::Attacking`** (combat-state filter) → **Living History** (or ship tracked-partial, deferring the
  "attacking" restriction). • **Treasure token def** (a token with an ACTIVATED `{T},Sac: any-color mana` ability —
  verify token activated abilities fire; S11 did only TRIGGERED token abilities) → **Seize the Spoils** (`khm`).
- **directed-discard `Effect`** (reveal hand → chooser picks → discard) → **Render Speechless**. • **Slumbering
  Trudge**: stun-counter core authorable now; its enter-tapped-if-X≤2 clause needs X threaded into `EntersTapped
  Unless`'s condition eval (whiteboard.rs ~1454 evals with no X ctx) — or defer that one clause.
- **DistinctNamedLands value** → Emil's counters-trample anthem (in addition to the {X}-activated-cost above).

Bigger subsystems stay DEFERRED (lower ROI, per the milestone call): **spell-copy** (~5 cards but 4 double-blocked
→ ~1 net; a full stack-copy subsystem — NOT worth first-pass), move-counters, conditional cost-reduction (S12),
dynamic-ManaValue, blink-with-delayed-return, graveyard-play/recursion, grant-arbitrary-ability, **Fractalize**
(= milestone-5 SET color/type layers, out of first-pass scope), LKI dies-triggers, Natives. 36 prepare-DFC + 2
planeswalkers + 5 Lessons stay deferred by type.

**PROCESS (unchanged, hard-won):** shared tree → `git commit --only <paths>`, never `-a`/`add -A`/stash; don't
touch `experiments/`; `cargo test -p mtg-core` green at every commit; flip a cap's ledger Status cell in the SAME
commit; **`git log -S "<mechanism>"` before scoping any ⏳ row as new**; **READ THE CODE, don't trust the ledger's
prose** (three wrong "unbuilt" beliefs were overturned this session by checking — first-strike, lifelink earlier,
"mined-out"). Ping the lead at cap boundaries. On fatigue: declare, rewrite THIS block, hand off clean.

---
**Older handoff (sos-cards-5, superseded by the block above — kept for provenance):** Shipped **11 cards,
3 caps, 2 engine fixes; 536 mtg-core tests green; tree clean, all pushed.** Caps: **S17 Ward** (`96dbc35` —
`Effect::CounterUnlessPay` soft-counter + `EffectTarget::Triggering`, threaded via `GameEvent::Targeted.source`
→ `state.trigger_targeting_source` → `ResolutionCtx.triggering_stack`; mana + discard cost paths; `CardFilter::
ItSelf` now matches in `enter_filter_matches`); **S10 flashback FRONT-cap** `Condition::CastFromNotHand`
(`8ed83b1`). Engine fixes: `Effect::MoveZone` was missing from `collect_specs_into` (reanimation/return targets
never collected through the real cast/trigger path); `CreateToken` now flushes at the deferred→imperative
boundary (#61) so "create tokens then affect them" works. Cards: 5 Ward (Colorstorm Stallion, Forum Necroscribe,
Tragedy Feaster, Thornfist Striker, Inkshape Demonstrator), Antiquities on the Loose, Rancorous Archaic,
Aberrant Manawurm, Topiary Lecturer, Hardened Academic (+ Ancestral Anger was already in `vow/`).

**Two lessons that saved/cost time — apply them:** (1) **`git log -S "<mechanism>"` before scoping any ⏳ cap
as new work** — 6 rows had drifted stale (S2/S3/S7/S10/S11/S18 were all done); a full audit reconciled them and
a PROCESS RULE is now in the capability-ledger header (flip the Status cell in the SAME commit as the cap).
(2) **Verify keyword/subsystem wiring by READING the code, not from memory** — "lifelink not combat-wired" was
believed by two sources but `apply_damage` already gains life (CR 702.15) and reads the COMPUTED keyword set, so
even a granted lifelink works; that unblocked 2 cards. ⚠️ **CORRECTION (agent 6, 2026-07-03):** the claim that
"double-strike / first-strike ARE genuinely unwired" was ALSO WRONG (same read-the-code lesson) — `combat/mod.rs::
combat_damage` has had the CR 510.4 two-substep split since `a15015f`; tests `double_strike_deals_twice` +
`first_strike_kills_before_retaliation` prove it, and `deals_in` reads the COMPUTED keyword set so granted FS/DS
works. **Both keywords are DONE.** Queue item #1 below was a no-op; it's struck.

**State of the pool: the no-cap / easy-card vein is MINED OUT.** Every remaining unauthored non-DFC card needs a
genuinely-new cap (see the fresh cap queue below). The big deferred bucket is 36 modal-DFC + Lesson/planeswalker/
named-keyword cards (out of first-pass scope per CLAUDE.md).

Tree clean, **509 mtg-core tests green**, all pushed. This session (sos-cards-4) shipped **5 cards + 4 caps**,
all with tests incl. real-turn-engine integration tests where a trigger fires. Handing off at a natural
boundary (still green — the small/clean caps are largely picked; what remains is heavier). What landed:
- **Multi-target MoveZone** (`12c41f8`, E1 extension) → **Pull from the Grave**. `chosen_targets` is a FLAT
  `Vec<Target>`; a `max>1` slot flattens all picks into it, so the MoveZone arm loops up to `spec.max`.
  **Invariant (in the arm):** a `max>1` slot must be the spell's LAST targeting sub-effect.
- **Source-threaded `Not(ItSelf)`** (`1f6e284`) → **Ascendant Dustspeaker**. `target_candidates` /
  `target_matches_filter` now take `source: Option<ObjId>` + a `CardFilter::ItSelf` arm → "another target"
  excludes the source at the *targeting* layer (not just resolution).
- **S21 cast-with-{X} trigger** (`134444d`, `HasXInCost` in `enter_filter_matches`) → **Matterbending Mage**.
- **`CreateToken.dynamic_counters`** (`9d2a856`) → **Wild Hypothesis** + **Snarl Song** (Snarl Song was FREE:
  cap + S7 `ColorsSpent`). The Quandrix "0/0 Fractal → X/X" pattern; reusable.

**Fresh cap queue (all GENUINELY-NEW — verified unbuilt 2026-07-03; each: one cap, one+ card, one commit, a
real-path test; flip the ledger Status cell in the SAME commit).** Ordered by realistic yield/effort:

1. ~~**First-strike / double-strike combat wiring**~~ — **ALREADY DONE** (agent 6, `a15015f`). The CR 510.4
   two-substep split is in `combat/mod.rs::combat_damage`; tests `double_strike_deals_twice` +
   `first_strike_kills_before_retaliation` prove it. **No card unblocked** — Practiced Offense still needs a modal
   keyword-pick + "counter on each creature target player controls" (target-player + ForEach), both still unbuilt.
2. ~~**Per-turn "counters put on THIS permanent this turn" tracker**~~ — **DONE** (agent 6). `Object.
   counter_added_this_turn` (set in the `AddCounters` executor for `n>0`; reset at turn start for all permanents +
   on zone change) + `Condition::PutCounterOnSelfThisTurn` (reads the source's flag). → **Fractal Tender** authored
   (6th of 8 Ward cards). Remaining Ward: Mica + Prismari (PayLife + spell-copy/storm).
3. **`pay_cost` `PayLife` arm** (tiny) + then Ward—Pay-life cards (**Mica**, **Prismari**) — BUT both are also
   blocked by spell-copy/storm secondaries, so PayLife alone yields 0 cards. Build it only alongside a consumer.
4. ~~**Apply-to-each-of-a-variable-multi-target**~~ — **DONE** (agent 6). New `Effect::ForEachTarget { slot, body }`:
   declares `slot` as a targeting spec at cast (added to `collect_specs_into`), then at resolution binds each chosen
   target to `EffectTarget::Each` in turn and runs `body` (reusing the `foreach_current` machinery — now generalized
   to `Option<Target>` so `Each` can be an object OR a player). → **Homesickness** (`{4}{U}{U}`:
   `TargetPlayer`+`Draw{ChosenTarget(0),2}` then `ForEachTarget` over up-to-2 creatures, `body = Tap{Each}+
   PutCounters{Each,Stun}`) and **Prismari Charm** mode 2 (1 damage to each of one or two "any" targets, incl.
   players). Reusable for any "do X to each of up-to-N targets."
5. **Spell-copy** (S14, ⏳ — token-copy already done). A real subsystem: mint a StackObject copy of a spell above
   the original (CR 707.10) + a "you may choose new targets" reselection. LOW practical yield — of its 7 cards,
   most are ALSO blocked elsewhere (Aziza tap-3 cost, Choreographed Sparks modal+creature-copy-grants, Mica
   Ward-pay-life, Prismari storm); alone it unblocks essentially only **Lumaret's Favor**. Build for the
   subsystem, not the count.
6. **Fractalize** (set-base-P/T + retype, layer work — do carefully). "Target creature *becomes* a green-and-blue
   Fractal, base P/T = X+1, loses all other colors and creature types" = SET/replace color+type layers (not
   Earthbend's ADD): new `StaticContribution::{SetColors,SetCreatureTypes}` + a one-shot `SetBasePT` on a target
   (the current `BecomeCreature` carries no P/T/color/type). Groundwork for other "becomes a Fractal" cards.

The DFC/Lesson/planeswalker/named-keyword bucket (~40 cards) stays DEFERRED per CLAUDE.md first-pass scope.

**Assessed-and-deferred (don't re-derive — the analysis is done):**
- **Mind into Matter** = **3 caps, not 1** (leave until a cheaper consumer): (a) dynamic-MV filter —
  `count_filter_matches` is EXHAUSTIVE and takes **no ctx**, so a `ManaValueAtMost(ValueExpr)` sibling filter
  (ValueExpr *is* Eq/Serialize → fits `CardFilter`) forces threading ctx through it + callers; (b) `MoveZone`
  from a `Select` (put a card from hand → battlefield; MoveZone only handles `Target` today); (c) enter-tapped
  (`ZoneDest`/MoveZone has no tapped flag).
- **Divergent Equation** = dynamic-X target count (`TargetSpec.max` fixed `u32`; needs max = chosen X).
- **Moment of Reckoning** = repeatable modal modes (choose same mode >1×, one target per instance).
- **Ennis** = ETB blink (exile + delayed return next end step) + exile-count-this-turn condition.
- **Tester of the Tangential** = pay-{X}-in-an-ability + a MoveCounters effect (move X counters between
  creatures) — `Not(ItSelf)` (done) covers only its "another target creature".
- **Fractal Anomaly DONE** (agent 6, `ValueExpr::CardsDrawnThisTurn`). **Emil** still needs a differently-named-lands
  value (a new DistinctNames ValueExpr) + Emil's {T} ability (the dynamic-counters cap is ready).

DEFERRED still (never build): DFC/modal, Lessons/Paradigm, planeswalkers, Casualty, Elder-Dragon grants;
dies-triggers need LKI (Arnyn, Cauldron of Essence).

**Blocked set (need an unbuilt cap first — don't burn time on these until the cap lands):**
- **Ward (S17, ◑ mana+discard built)** — Colorstorm Stallion + Forum Necroscribe + Tragedy Feaster + Thornfist
  Striker + Inkshape Demonstrator + **Fractal Tender** DONE (**6 of 8 cards**). ⚠️ **Lifelink IS combat-wired**
  (`apply_damage` gains the source's controller life = damage dealt, CR 702.15, and reads the COMPUTED keyword set
  so a GRANTED lifelink counts) — the earlier "lifelink not combat-wired" note (mine + the audit's) was WRONG; that
  unblocked Inkshape (Repartee grants lifelink) AND **Hardened Academic** (Discard→lifelink). **Fractal Tender**
  `{3}{G}{U}` used the new per-turn counter-added tracker (agent 6). Remaining 2 Ward cards: **Mica** & **Prismari**
  (pay_cost PayLife arm + spell-copy/storm). **Ward—Pay-life needs a `pay_cost` PayLife arm** (IR ready; no-op today).

**▶ Fresh authorable-now list (2026-07-03 unauthored-card audit — verified vs the real engine):** the audit
found `ConditionalStatic`, stun counters, `ValueExpr::{Sum,XTimes,NumTargets,PowerOfTarget}`, `CardFilter::
{Named,ManaValue,PowerAtMost}`, `Effect::{Fight,Distribute,BecomeCreature}` all LIVE. The audit's AUTHORABLE-NOW list is
**fully swept**: Antiquities/Rancorous/Aberrant/Topiary/Thornfist done, Ancestral Anger already in `vow/`. **Plus
2 cards the audit wrongly marked "lifelink-blocked"** — lifelink IS wired, so **Inkshape Demonstrator** (5th Ward
card) and **Hardened Academic** are done too. **Homesickness DONE** (agent 6, `Effect::ForEachTarget`).

⚠️ **CORRECTION (agent 6 audit, 2026-07-03): the "no-cap vein is mined out" claim was WRONG.** A fresh
unauthored-card audit (verified vs the interpreter) found **2 zero-cap cards** — **Prismari Charm** (3-mode modal,
DONE) and **Withering Curse** (all-creatures -2/-2 or Infusion destroy-all, DONE) — plus a live vein of
**one-small-cap** cards. Newly DONE by agent 6: **Geometer's Arthropod** (`XOfTriggeringSpell`). Still-cheap
1-cap wins the audit surfaced (each a single small leaf, some sharing a cap):
- ~~**S22 `Condition` "cast an instant/sorcery this turn"**~~ **DONE** (agent 6) — `Player.instants_sorceries_
  cast_this_turn` (counted in `cast_spell`, reset each turn) + `Condition::CastInstantOrSorceryThisTurn`; ALSO
  wired `Restriction::OnlyIf` into the activated-ability legality gate (was only honoured for mana abilities) +
  a reusable `artifact()` builder. → **Potioner's Trove** DONE. **Burrog Barrage** still needs care — its only
  target sits inside a `Conditional`, which `collect_specs_into` doesn't walk (targeting-collection wrinkle).
- ~~**"counters put on self" `EventPattern`**~~ **DONE** (agent 6) — `EventPattern::CountersPutOnSelf { kind }` +
  `GameEvent::CountersPut` broadcast from the `AddCounters` executor (once per counter-adding event, battlefield
  only). → **Pensive Professor** DONE (Increment→+1/+1→draw). **Berta, Wise Extrapolator** still needs its
  `{X},{T}`-activated Fractal ability + "add one mana of any color" trigger (check any-color `AddMana` + {X}-in-
  activated-cost threading before scoping).
- **S20 `ValueExpr::CountersOnTarget(n)`** → **Growth Curve**. • **`DistinctNamedLands` value** → **Emil**.
- **`CardFilter::Attacking`** → **Living History**. • **Treasure token def** → **Seize the Spoils**.
- **directed-discard `Effect`** → **Render Speechless**. • **Slumbering Trudge** (stun core authorable now;
  enter-tapped clause needs X threaded into the `EntersTappedUnless` condition eval, or defer that clause).
Bigger subsystems (lower ROI, deferred): spell-copy (~5 cards but most double-blocked), move-counters, cost-
reduction (S12), dynamic-ManaValue, blink-with-delayed-return, graveyard-play, grant-arbitrary-ability,
Fractalize (milestone-5 layers), LKI dies-triggers. Recommended next: the two shared 1-cap leaves (S22,
counters-put-on-self) clear 4 cards fast; spell-copy is NOT worth its subsystem cost for the first pass.
Genuinely-absent caps (from the audit): spell-copy, move-counters, counters-on-TARGET value, no-max-hand,
DYNAMIC ManaValue bounds, one-shot set-base-P/T on a target, self "costs less", grant-arbitrary-ability; DFC/
Lesson/planeswalker/named-keyword buckets remain deferred (36 DFC + more).
- **S16 end-step-token timing** — the begin-of-step-trigger cap unblocked the *timing*; any remaining
  end-step-token card is now authorable IF its other clauses are (check per-card).
- **S15 graveyard-play** — Ark of Hunger (mill → play from graveyard); needs a graveyard analog of
  `castable_from_exile` + a graveyard scan in the offer loop. Tablet of Discovery also needs it (+ S13, done).
- **Archaic's Agony** — S7+S15 unblocked but still needs an excess-damage value + multi-card top-of-library
  exile (`TopOfLibrary` is single-card).

Systemic: honour the proposed audit rule (⚠️/✅ trigger section) — every new `Triggered` should fire once
through the REAL turn engine in a test. SHARED TREE: `git commit --only <paths>`; MuZero teammate lives in
`experiments/`.

**Card data lives in the SQLite index, never memory** (CLAUDE.md "Card data"):
```
sqlite3 data/scryfall/cards.sqlite \
  "SELECT mana_cost,type_line,power,toughness,oracle_text,rarity
     FROM cards WHERE set_code='sos' AND name='<card>' ORDER BY released_at DESC LIMIT 1;"
```
Always re-read the oracle text from the db before authoring a card.

## Folder placement (first-printing rule)

Per repo convention a card lives in its **first-printing real-expansion** folder. For SoS that is
`sos/` for **255** of the 271 (the promo/prerelease codes `psos`/`pvow` collapse to `sos`/`vow`).
The genuine older reprints go elsewhere and **may already exist / should be reused, not duplicated**:

| Card | folder | note |
|---|---|---|
| Essence Scatter | `m10` | reprint |
| Last Gasp | `rav` | reprint |
| Quick Study | `woe` | reprint |
| Seize the Spoils | `khm` | reprint |
| Terramorphic Expanse | `tsp` | reprint (verify first-print at author time) |
| Ancestral Anger, Deathcap Glade, Dreamroot Cascade, Shattered Sanctum, Stormcarved Coast, Sundown Pass | `vow` | Crimson Vow reprints (6) |

`Erode` (`sos`) and the five basics (`misc`) are **already implemented** — reuse.

## Triage summary (2026-07-03)

271 distinct cards triaged against the **current** engine (Selesnya-push IR + Crew + Warp):

| Tier | Meaning | Count |
|---|---|---|
| **T1** | vanilla / french-vanilla (implemented keywords only) | **6** (5 basics done + Rearing Embermare) |
| **T2** | expressible in existing IR, **no new cap** | **68** |
| **T3** | needs one small card-agnostic cap (an S-cap below) | **142** |
| **T4** | needs a major subsystem — **deferred** | **55** (36 modal-DFC + 19 subsystem cards) |

The DFC bucket is deferred by CLAUDE.md first-pass scope ("double-faced / split … leave unbuilt").
So the reachable near-term pool is **T1 + T2 (74) then the T3 long tail (142)** as caps land.

## Capability ledger — small caps SoS needs (S-caps)

Card-agnostic caps to build in the Selesnya style (new `EventPattern` / `ValueExpr` / `Condition` /
`Effect` leaf / `Qualification` / `Rewrite` / `TokenSpec` field). Build **highest-leverage first**;
each cap unlocks the bracketed count. `⏳` = not yet built.

> ⚠️ **PROCESS RULE (learned the hard way — S7, S10, S2, S3, S18, S11 all drifted stale):** flip a cap's
> Status cell to ✅/◑ **in the SAME commit that lands the cap**, with the commit hash. Before scoping any
> "⏳" cap as new work, `git log -S "<mechanism/enum name>"` first — the row may already be done. A
> **2026-07-03 audit** re-verified every row against the codebase; genuinely-open caps are now only: S12
> (conditional cost-reduction — only the unconditional `CostReductionGeneric` static exists), S14 spell-copy
> (token-copy done, copy-target-spell not), S19/S20/S22, and most `misc one-offs` + `Native`.

| Cap | What it adds | Cards | Status |
|---|---|---|---|
| **S1** Surveil N | look at top N, put any number in graveyard, rest back (CR 701.42) — `Effect::Surveil` | 15 | ✅ **DONE** `cc58a7b` |
| **S5** Opus | `SpellCast(I/S you control)` trigger + `ValueExpr::ManaSpentOnTrigger` + `≥5` condition | 13 | ✅ **DONE** `e85771e` |
| **S8** Repartee | `SpellCast(I/S you control **that targets a creature**)` trigger (inspect cast targets) | 12 | ✅ **DONE** |
| **S4** Infusion | per-turn per-player "gained life this turn" state + a `Condition` reading it | 12 | ✅ **DONE** `89b3581` |
| **S10** Flashback | alt-cast from graveyard for a flashback cost, then exile (Warp-analogue) | 11 | ✅ **DONE** (offer at priority.rs ~1075 `flashback_cost`/`CastVariant::Flashback`; exile-on-resolve ~1718; `Ability::Flashback{cost:ManaCost}`). **6 cards authored** (Dig Site Inventory, Duel Tactics, Molten Note, Pursue the Past, Tome Blast, **Antiquities on the Loose** `8ed83b1` — front-cap: `Condition::CastFromNotHand` reads the spell's `flashback_cast` flag; that commit ALSO fixed a #61 bug where `CreateToken` staged deferred so a later same-resolution step couldn't see the tokens — CreateToken now flushes/commits at the boundary, unblocking "create tokens then affect them"). ⚠️ **cost is mana-only** — a non-mana flashback cost (Group Project's "tap three creatures") is NOT expressible; a card that *grants* flashback (the card "Flashback") needs a dynamic-ability-grant cap. Remaining 4 front-cap cards: **Practiced Offense** (blocked — grants double-strike/lifelink, not combat-wired), **Daydream** (needs an exiled-card reference for its self-blink), **Group Project** (non-mana flashback cost), **Flashback** (dynamic ability grant). |
| **S6** Increment | `SpellCast(you)` trigger + condition "mana spent > this creature's power OR toughness" | 9 | ✅ **DONE** |
| **S7** Converge | `ValueExpr::ColorsOfManaSpent` (ETB counters / X in Converge spells) | 9 | ✅ **DONE** `ba8c183` (`ValueExpr::ColorsSpent` — `Object.colors_spent` recorded at cast; consumers Arcane Omens, Together as One, Magmablood/Transcendent/Wildgrowth Archaic) |
| **S9** Graveyard-leave | "cards leave your graveyard" trigger + "a card left your graveyard this turn" cond | 8 | ✅ **DONE** (flag `f9b5584` + trigger: LeftGraveyard event snapshot in resolve_effect → Spirit Mascot, Owlin Historian, Garrison Excavator) |
| **S2** Look-and-pick | look at top N, put one/some in hand, rest on bottom (impulse selection) | 8 | ✅ **DONE** (`Effect::LookAndPick{ count, take, take_to, rest_to, take_filter }` — implemented; consumers Flow State, Stress Dream, Stirring Honormancer, Paradox Surveyor, Follow the Lumarets, Visionary's Dance). The ledger previously mis-listed this as ⏳. Geometer's Arthropod still needs "top-X" = reading the *triggering spell's* X (a separate need). |
| **S12** Cost-reduction cond. | "costs {N} less if it targets X / you control Y / a card left your gy" (cast-time) | 7 | ⏳ |
| **S14** Copy spell/perm | "copy target spell", "create a token that's a copy of" (heavier small-cap) | 7 | ◑ **token-copy DONE** (`Effect::CreateTokenCopy`+`TokenCopyMods`, `a8c8a2d` → Applied Geometry); **spell-copy** portion ⏳ — a real subsystem (copy a stack spell per CR 707.10: mint a new StackObject copy above the original + a "you may choose new targets" reselection). **Low practical yield (scoped 2026-07-03):** of the 7 spell-copy cards, most are ALSO blocked elsewhere — Aziza (tap-3-creatures cost), Choreographed Sparks (modal + creature-spell-copy-with-grants + "can't be copied"), Mica (Ward—Pay-life), Prismari (Storm + Elder Dragon). Spell-copy ALONE unblocks essentially only **Lumaret's Favor** (Infusion "copy it if you gained life this turn" + a +2/+4 pump). So build it for the subsystem, not the count. |
| **S17** Ward {cost} | Ward N / Ward—Pay life / Ward—Discard (counter-unless-pay on becoming targeted) | 7 | ◑ **mana DONE** `96dbc35` — `Effect::CounterUnlessPay{ what, cost:Cost }` soft-counter + `EffectTarget::Triggering` (the targeting spell/ability, threaded via `GameEvent::Targeted.source` → `state.trigger_targeting_source` → `ResolutionCtx.triggering_stack`); `CardFilter::ItSelf` now matches in `enter_filter_matches` (source-threaded, opt-in from the targeted path). Reuses `Cost`+`can_pay_cost`/`pay_cost`. Ward constructors live in `cards/helpers.rs` (`ward`/`ward_mana`/`ward_discard`). → **Colorstorm Stallion** (Ward {1}, mana) + **Forum Necroscribe** (Ward—Discard, the non-mana path — reuses the `Discard` cost arms). **Ward—Pay life** (Mica/Prismari): `pay_cost` has NO `PayLife` arm yet (falls to `_ => {}`, so life isn't deducted) — add it first; their *secondaries* are also blocked (spell-copy/storm). Side-fix landed here: `Effect::MoveZone`'s target was missing from `collect_specs_into` (never collected through the REAL cast/trigger path — prior MoveZone tests bypassed casting), now fixed. |
| **S15** Impulse play | exile/mill → "you may play it until end of turn / your next turn" | 6 | ◑ **DONE for exile cases** (`d079eb0` base + `0e17d3e` top-of-library source + land-play) → Practiced Scrollsmith, Elemental Mascot, Suspend Aggression (3). Only **graveyard-play** (milled card played from gy — Ark of Hunger, Tablet) still ⏳; the other 2 S15 cards are cap-blocked (Archaic's Agony=S7, Tablet=S13) |
| **S3** Stun counters | `CounterKind::Stun` + "would untap → remove a stun counter instead" replacement | 6 | ✅ **DONE** `f8ab8ea` (untap-step replacement, CR 702.171) → Procrastinate, Deluge Virtuoso, Fractal Mascot, Rapier Wit. (Was mis-listed ⏳.) |
| **S18** Graveyard-activated | an ability that functions while its card is in the graveyard (recursion) | 6 | ✅ **DONE** `6190bb2` (`CostComponent::ExileSelfFromGraveyard` + graveyard ability enumeration in `legal_priority_actions`) → Eternal Student, Stone Docent. Also `DiscardSelfFromHand` for hand-usable cycling-style abilities (Visionary's Dance). (Was mis-listed ⏳.) |
| **S11** Token-with-ability | `TokenSpec` carries an ability (Treasure `{T},Sac`; Pest attack→gain life) | 5 | ◑ **DONE for grp-id ability tokens** — a `TokenSpec.grp_id` points at a registered token def whose abilities fire (Pest `PEST_TOKEN`=9001, "attack → gain 1 life") → Send in the Pest, Pestbrood Sloth, Essenceknit Scholar. A **Treasure** token (`{T}, Sac: add one mana of any color` — an ACTIVATED mana ability on a token) is not yet verified; check for a registered Treasure def before authoring one. (Was mis-listed ⏳.) |
| **S13** Restricted mana | mana usable "only to cast instant and sorcery spells" (spend-restriction tag) | 4 | ✅ **DONE** `ffcc0df` (`ManaSpec.restriction=InstantSorceryOnly` + `ManaPool.restricted` bucket + `allow_restricted` threaded through the payment path; spell casts pass card-is-I/S, ability costs pass false) → Hydro-Channeler |
| **S16** Gain-life trigger | `EventPattern::GainLife` ("whenever you gain life, …") | 3 | ✅ **DONE** |
| **S21** cast-with-{X} trigger | `SpellCast` filtered to "has {X} in its cost" | 2 | ✅ **DONE** (`134444d` + agent 6) — `HasXInCost` arm in `enter_filter_matches` → **Matterbending Mage**; `ValueExpr::XOfTriggeringSpell` (reads the triggering spell's `Object.cast_x`, recorded at cast alongside `mana_spent`) → **Geometer's Arthropod** (look at top X, keep 1). |
| **S19** cards-drawn-this-turn value | `ValueExpr::CardsDrawnThisTurn` (reads `Player.cards_drawn_this_turn`, reset each turn + incremented in `draw`) | 1 | ✅ **DONE** (agent 6) → **Fractal Anomaly** (0/0 Fractal + X counters, X = cards drawn this turn) |
| **S20/S22** | counters-on-target value / cast-I/S-this-turn cond | 1 ea | ⏳ |
| **misc one-offs** | GreatestMV, DistinctNames, ~~SoftCounter~~, DirectedDiscard, AltCost, PayXLife, NoMaxHand, GrantAbility | 1–3 ea | ⏳ except **SoftCounter (counter-unless-pay) ✅ DONE** via `Effect::CounterUnlessPay` (Ward, `96dbc35`). The rest (GreatestMV/DistinctNames/DirectedDiscard/AltCost/PayXLife/NoMaxHand/GrantAbility) are genuinely unbuilt (verified vs codebase 2026-07-03). |
| **Native** | genuine one-offs via the `Native` escape hatch: Mathemagics (2^X), Pox Plague (halving), Steal the Show (wheel) | 4 | ⏳ |

Building **S1, S4, S5, S6, S7, S8, S10** (the seven big-count caps) converts ~**79** T3 cards to authorable.

## ✅ Trigger-system gap — **found + FIXED 2026-07-03** (`20965a8`)

**RESOLVED.** Both gaps below are fixed: `collect_triggers` now queues each permanent's
`BeginningOfStep(phase)` trigger at phase transitions (`queue_begin_of_step_triggers`); a
non-intervening-if trigger condition (CR 603.2) gates queueing, and an intervening-if (CR 603.4) is
re-checked at put-on-stack + resolution (`trigger_intervening_if_holds`). Scoped to condition-bearing
triggers, so `condition: None` triggers are unaffected. **Turn-engine integration tests prove the 4
revived cards now fire (and gate correctly): Startled Relic Sloth, Essenceknit Scholar, Primary
Research, Additive Evolution** — all four are now genuinely `fully_implemented` (flags never lied
across a session boundary). Ennis (unimplemented) will benefit when authored.

_Original finding (kept for the record):_ tracing Abstract Paintmage's "at the beginning of your first
main phase" trigger surfaced **two real, pre-existing gaps** in the triggered-ability system:

1. **`EventPattern::BeginningOfStep(phase)` permanent triggers are never queued.** `collect_triggers`
   (priority.rs ~2718) handles `PhaseBegan` only for `Phase::End` *delayed* triggers (warp exile); there
   is **no scan that queues a permanent's `BeginningOfStep(phase)` trigger** at a phase transition
   (`queue_self_triggers` is called only for SelfEnters/SelfDies/GainLife/SelfAttacks; zero
   `BeginningOfStep` refs in priority.rs). So any "at the beginning of [your] [upkeep/main/combat/end
   step]" permanent trigger **does not fire through the real turn engine.**
2. **A `Triggered` ability's `condition` field is never evaluated.** Neither `put_trigger_on_stack`
   (~2487) nor `resolve_top`'s ability arm (~2233) reads `condition`/`intervening_if` for a normal
   (non-reflexive) trigger — it extracts `effect` and resolves it unconditionally. So a
   `condition: Some(YourTurn)`-style gate on a triggered ability is silently ignored.

_Impact was:_ Essenceknit Scholar (end-step draw), Startled Relic Sloth (begin-combat exile), Primary
Research (end-step draw), Additive Evolution (begin-combat pump) — all fixed + integration-tested.
Abstract Paintmage / Fractal Tender / S16 end-step-token timing are now **unblocked** (Abstract Paintmage
needs only its first-main-phase trigger authored — the queue + `add_mana`-to-restricted-bucket are wired).

**➕ Proposed systemic audit rule (for a future #60-style pass):** _every `Triggered` ability in the pool
should fire at least once through the REAL turn engine in some test_ (broadcast the event → `run_agenda` →
`resolve_top`), not only via `resolve_effect`-direct. This class of "silently-inert" bug (unqueued
triggers, ignored conditions) is invisible to resolve_effect-direct tests. The 4 integration tests added
here are the seed. The Selesnya pool got this audit (see SELESNYA_LANDFALL_CARDS.md #60); SOS deserves it.

## Engine reality-check — unimplemented effect leaves (E-caps) — **found during Phase 2**

The Phase-1 rubric assumed several `Effect` variants were interpreted; grepping the whiteboard
interpreter (`whiteboard.rs`) shows **six IR leaves are defined but interpreted nowhere** — a card
using one silently no-ops. So the true near-term T2 pool is **smaller than the 68 tallied above**:
some of those cards actually need one of these leaves wired first. These are the highest-leverage
caps (each is a small, card-agnostic interpreter arm lowering to an already-existing `Action`).

| E-cap | Effect leaf | Blocks (examples) | Status |
|---|---|---|---|
| **E1** | `Effect::MoveZone` (bounce / return-to-hand / reanimate) | Zealous Lorecaster, Banishing Betrayal, Proctor's Gaze, Prismari Charm, Matterbending Mage, Pull from the Grave, Moment of Reckoning, Lorehold Charm | ✅ **DONE** `0e85b76` single-target + `12c41f8` multi-target fixed-max ("up to two" → Pull from the Grave). Dynamic-X-count (Divergent Equation) + repeatable-modal (Moment of Reckoning) still need their own caps. |
| **E2** | `Effect::Counter` (counter target spell), respecting `CantBeCountered` | Essence Scatter, Brush Off, Mana Sculpt, Quandrix Charm | ✅ **DONE** `eb2b364` (+ stack-zone static gathering; closed Surrak's deferral) |
| **E3** | `Effect::Discard` (loot "then discard a card"; "target player discards") | Traumatic Critique, Stadium Tidalmage, Charging Strifeknight, Rubble Rouser, Colossus, Rapturous Moment, Borrowed Knowledge, Send in the Pest | ✅ **DONE** `506baf9` |
| **E4** | `Effect::Sacrifice` (as an effect — "each player sacrifices", "sacrifice two lands") | Planar Engineering, Witherbloom Charm, Social Snub (needs S14 copy too), Pox Plague | ✅ **DONE** `b5ea234` (per-player: Controller / EachPlayer / EachOpponent) |
| **E5** | `Effect::Repeat` | (few) | ⏳ |
| **E6** | `Effect::Distribute` | (few) | ⏳ |

**Loud guard (`8604b34`):** `materialize()` is now an **exhaustive** match — a defined-but-unwired
`Effect` leaf `debug_assert!`s loudly in debug/tests instead of silently no-oping (the bug class that
hid Traumatic Critique's discard), and a NEW IR variant with no arm is a *compile* error. The only
remaining loud-assert leaves are E5 `Repeat`, E6 `Distribute`, and `Native` (no runtime yet).

**Wired today (safe for T2 authoring):** DealDamage, Draw, Destroy, Exile, GainLife, LoseLife, PumpPT,
GrantKeyword, GrantQualification, BecomeCreature, AddMana, PutCounters, CreateToken, Fight, Search,
Tap, Modal, Optional, IfYouDo, ForEach, Conditional, Earthbend, **MoveZone, Discard, Counter (new)**.

Next-highest leverage: **E4 Sacrifice** (each-player-sacrifices / sac-as-effect), then the S-caps
(S1 Surveil, S4 Infusion, S5 Opus, …).

## Deferred subsystems (T4 — do NOT build now)

| Subsystem | Cards | Count |
|---|---|---|
| Modal double-faced (DFC) | the `… // …` cards (Emeritus cycle, all creature/spell MDFCs) | 36 |
| Lesson / **Paradigm** (recast-copy-from-exile each main phase) | Decorum Dissertation, Echocasting Symposium, Germination Practicum, Improvisation Capstone, Restoration Seminar | 5 |
| **Planeswalker** loyalty | Professor Dellian Fel, Ral Zarek Guest Lecturer | 2 |
| **Prepare / prepared** marker | Biblioplex Tomekeeper, Skycoach Waypoint | 2 |
| **Storm** | Prismari, the Inspiration | 1 |
| **Cascade** | Quandrix, the Proof | 1 |
| **Miracle** | Lorehold, the Historian | 1 |
| **Casualty** | Silverquill, the Disputant | 1 |
| **Affinity** (dynamic cost) | Witherbloom, the Balancer | 1 |
| **Grandeur** | Page, Loose Leaf | 1 |
| ownership / theft-cast | Nita, Forum Conciliator | 1 |
| name-choice statics | Petrified Hamlet | 1 |
| once-per-turn free-cast permission | Zaffai and the Tempests | 1 |
| grant-mana-ability-to-a-set | Resonating Lute | 1 |

## 10 easiest (author these first — all T1/T2, no new cap)

1. **Quick Study** — `{2}{U}` draw two cards. Pure `Draw`.
2. **Rearing Embermare** — `{4}{R}` 4/5, "Reach, haste" — french-vanilla (T1).
3. **Last Gasp** — `{1}{B}` target creature gets −3/−3 EOT. `PumpPT`.
4. **Essence Scatter** — `{1}{U}` counter target creature spell. `Counter`.
5. **Wander Off** — `{3}{B}` exile target creature. `Exile`.
6. **Grapple with Death** — `{1}{B}{G}` destroy target artifact/creature, gain 1. `Destroy`+`GainLife`.
7. **Interjection** — `{W}` +2/+2 and first strike EOT. `PumpPT`+`GrantKeyword`.
8. **Chase Inspiration** — `{U}` +0/+3 and hexproof EOT. `PumpPT`+`GrantKeyword`.
9. **Oracle's Restoration** — `{G}` +1/+1 EOT, draw a card, gain 1. `PumpPT`+`Draw`+`GainLife`.
10. **Cost of Brilliance** — `{2}{B}` target player draws 2 & loses 2; +1/+1 on up-to-one creature.

(Deep T2 bench also ready: Dissection Practice, Traumatic Critique, Sneering Shadewriter,
Environmental Scientist, Harsh Annotation, Vibrant Outburst, Masterful Flourish, Shopkeeper's Bane.)

## 10 hardest (all T4 — deferred; here for the record)

1. **Prismari, the Inspiration** — Elder Dragon; grants **Storm** to all your I/S spells.
2. **Quandrix, the Proof** — Elder Dragon; has **Cascade** and grants it to your I/S.
3. **Lorehold, the Historian** — Elder Dragon; grants **Miracle {2}** to I/S in hand.
4. **Silverquill, the Disputant** — Elder Dragon; your I/S have **Casualty 1**.
5. **Witherbloom, the Balancer** — Elder Dragon; **Affinity for creatures** + grants it (dynamic cost).
6. **Professor Dellian Fel** — planeswalker; 4 loyalty abilities + emblem (whole PW subsystem).
7. **Ral Zarek, Guest Lecturer** — planeswalker; −7 "flip five coins, skip X turns".
8. **Restoration Seminar** (+ the 4 other Lessons) — **Paradigm**: exile & recast a free copy each main phase.
9. **Nita, Forum Conciliator** — cast spells you don't own + exile-and-cast opponents' graveyard spells.
10. **Petrified Hamlet** — ETB choose a card name, then name-scoped static grants/restrictions.

## Authoring plan

1. **T1/T2 sweep** — the 68 T2 + Rearing Embermare need no engine work; author them first (each: data
   IR + expect-test snapshot + a behaviour test for any effect; honest `fully_implemented`). This is
   the bulk of the immediately-shippable pool.
2. **Cap-then-cards** — build S-caps highest-leverage first (S1 Surveil, then S4/S5/S6/S7/S8/S10), each
   its own commit in the card-agnostic style (new IR node + tests), then author the T3 cards that cap
   unlocks. Keep `cargo test -p mtg-core` green at every commit.
3. **Defer T4** — mark deferred here, do not build. If a T3 card has one deferrable clause beyond its
   cap, ship the core with a documented `// deferred:` note (the established Humility/Rancor pattern).
4. A `sos_limited` preset deck once enough of the pool is playable.

## Full triage table

### T1 — 6 cards

| Card | Caps | Folder | Status | Gating clause |
|---|---|---|---|---|
| Forest | - | `lea` | ✅ basic (misc) | basic land |
| Island | - | `lea` | ✅ basic (misc) | basic land |
| Mountain | - | `lea` | ✅ basic (misc) | basic land |
| Plains | - | `lea` | ✅ basic (misc) | basic land |
| Rearing Embermare | - | `sos` | ✅ done | reach, haste french-vanilla |
| Swamp | - | `lea` | ✅ basic (misc) | basic land |

### T2 — 68 cards

| Card | Caps | Folder | Status | Gating clause |
|---|---|---|---|---|
| Additive Evolution | - | `sos` | ✅ done | fractal token + combat counter, all IR |
| Ancestral Anger | - | `vow` | ✅ done | grant trample, named-card-count pump, draw |
| Arnyn, Deathbloom Botanist | - | `sos` | ⏳ | deathtouch, filtered dies-trigger drain |
| Artistic Process | - | `sos` | ✅ done | modal: 6-to-target / 2-to-each-opp-creature (ForEach chooser:Opponent) / flying+haste token |
| Ascendant Dustspeaker | - | `sos` | ⏳ | flying, ETB counter, exile graveyard card |
| Bogwater Lumaret | - | `sos` | ✅ done | creature-ETB gain-life trigger, IR |
| Borrowed Knowledge | - | `sos` | ⏳ | modal discard hand, draw by count |
| Burrog Banemaker | - | `sos` | ✅ done | deathtouch + activated pump |
| Burrog Barrage | - | `sos` | ⏳ | conditional pump + power-based damage |
| Cauldron of Essence | - | `sos` | ⏳ | dies-drain + activated reanimation |
| Charging Strifeknight | discard-cost | `sos` | ✅ done | haste + {T},Discard-a-card: draw (CostComponent::Discard wired) |
| Chase Inspiration | - | `sos` | ✅ done | pump + grant hexproof |
| Chelonian Tackle | - | `sos` | ✅ done | pump + fight up to one |
| Colossus of the Blood Age | - | `sos` | ◑ partial | ETB drain+gain done; dies rummage (discard N, draw N+1) deferred |
| Cost of Brilliance | - | `sos` | ✅ done | draw, lose life, counter |
| Deathcap Glade | - | `vow` | ✅ done | checkland conditional tap + mana |
| Dina's Guidance | - | `sos` | ✅ done | search creature to hand/graveyard |
| Dissection Practice | - | `sos` | ✅ done | drain + pump modal, all IR |
| Divergent Equation | - | `sos` | ⏳ | X return instant/sorcery cards, exile self |
| Dreamroot Cascade | - | `vow` | ✅ done | checkland conditional tap + mana |
| Eager Glyphmage | - | `sos` | ✅ done | ETB Inkling keyword token |
| Embrace the Paradox | - | `sos` | ✅ done | draw 3 + put land from hand (hand→bf `Search`, min 0) |
| Ennis, Debate Moderator | - | `sos` | ⏳ | blink ETB + conditional end-step counter |
| Environmental Scientist | - | `sos` | ✅ done | ETB search basic land to hand |
| Erode | - | `sos` | ✅ done (sos) | destroy + opponent fetches basic land |
| Essence Scatter | - | `m10` | ✅ done | counter target creature spell |
| Fractalize | - | `sos` | ⏳ | becomes Fractal, base P/T X+1 |
| Glorious Decay | HasKeyword | `sos` | ✅ done | modal destroy-artifact / 4-to-flying-creature (`CardFilter::HasKeyword`) / exile-gy-card+draw (`0622d36`) |
| Grapple with Death | - | `sos` | ✅ done | destroy artifact/creature, gain life |
| Harsh Annotation | - | `sos` | ✅ done | destroy; controller makes Inkling token |
| Heated Argument | Select-exile | `sos` | ✅ done | 6 to target creature; `Optional{IfYouDo{ exile a gy card (Select), 2 to ControllerOfTarget(0) }}` — landed the Select-exile-as-cost machinery (`5596fb4`) |
| Impractical Joke | - | `sos` | ✅ done | 3 damage up-to-one; prevention clause deferrable |
| Interjection | - | `sos` | ✅ done | pump plus first strike |
| Last Gasp | - | `rav` | ✅ done | -3/-3 to target creature |
| Lorehold Charm | - | `sos` | ✅ done | modal: each-opp-sac artifact / reanimate MV<=2 from your gy / mass +1/+1+trample |
| Mage Tower Referee | Multicolored | `sos` | ✅ done | colorless artifact creature; `SpellCast(Multicolored)` (`CardFilter::Multicolored`) → +1/+1 self (`40ee29c`) |
| Masterful Flourish | - | `sos` | ✅ done | pump plus indestructible |
| Mind Roots | - | `sos` | ⏳ | discard two, put discarded land onto battlefield tapped |
| Mind into Matter | - | `sos` | ⏳ | draw X, put permanent from hand into play |
| Mindful Biomancer | - | `sos` | ✅ done | ETB gain life; once-per-turn pump |
| Moment of Reckoning | - | `sos` | ⏳ | modal choose-up-to-four destroy/reanimate |
| Noxious Newt | - | `sos` | ✅ done | deathtouch plus mana ability |
| Oracle's Restoration | - | `sos` | ✅ done | pump, draw, gain life |
| Planar Engineering | - | `sos` | ✅ done | sacrifice lands, search basics onto battlefield |
| Proctor's Gaze | - | `sos` | ✅ done | bounce plus search basic to battlefield |
| Pterafractyl | - | `sos` | ✅ done | enters with X +1/+1 counters (fixed: perm resolution now carries `x` to ETB replacements), ETB gain 2 |
| Pull from the Grave | - | `sos` | ⏳ | return creatures to hand, gain life |
| Quick Study | - | `woe` | ✅ done | draw two cards |
| Rapturous Moment | - | `sos` | ✅ done | draw, discard, add mana ritual |
| Rubble Rouser | - | `sos` | ⏳ | discard/draw ETB; mana ability with damage |
| Shattered Acolyte | - | `sos` | ✅ done | lifelink; sac to destroy artifact/enchantment |
| Shattered Sanctum | - | `vow` | ✅ done | conditional enters-tapped dual land |
| Shopkeeper's Bane | - | `sos` | ✅ done | attack trigger gain life |
| Silverquill Charm | - | `sos` | ✅ done | modal counters/exile/drain |
| Sneering Shadewriter | - | `sos` | ✅ done | ETB lose/gain life |
| Splatter Technique | multi-player-ForEach | `sos` | ✅ done | modal: draw four / 4 to each creature+planeswalker (both players via `EachPlayer` area selector) (`6e6180c`) |
| Stadium Tidalmage | - | `sos` | ✅ done | ETB/attack loot draw-discard |
| Stand Up for Yourself | - | `sos` | ✅ done | destroy target power-3+ creature (Not(PowerAtMost(2))) |
| Startled Relic Sloth | - | `sos` | ✅ done | combat trigger exile graveyard card |
| Stormcarved Coast | - | `vow` | ✅ done | conditional enters-tapped dual |
| Strixhaven Skycoach | - | `sos` | ✅ done | vehicle crew, ETB land search |
| Sundown Pass | - | `vow` | ✅ done | conditional enters-tapped dual |
| Terramorphic Expanse | - | `tsp` | ✅ done | fetch basic land, tapped |
| Traumatic Critique | - | `sos` | ✅ done | X damage, draw then discard |
| Vibrant Outburst | - | `sos` | ✅ done | damage plus tap creature |
| Wander Off | - | `sos` | ✅ done | exile target creature |
| Witherbloom Charm | - | `sos` | ✅ done | modal sac-draw/life/destroy |
| Zealous Lorecaster | - | `sos` | ✅ done | return IS from graveyard |

### T3 — 142 cards

| Card | Caps | Folder | Status | Gating clause |
|---|---|---|---|---|
| Aberrant Manawurm | S5 | `sos` | ⏳ | pump by mana spent on triggering spell |
| Abstract Paintmage | S13,begin-of-step | `sos` | ✅ done | `{U/R}` hybrid + first-main-phase (`BeginningOfStep(PrecombatMain)`/YourTurn) trigger floats restricted `{U}{R}`; integration-tested end-to-end |
| Ajani's Response | S12 | `sos` | ⏳ | conditional cost reduction if targets tapped creature |
| Ambitious Augmenter | S6 | `sos` | ⏳ | Increment mechanic (mana-spent vs power/toughness) |
| Antiquities on the Loose | S10 | `sos` | ⏳ | flashback + cast-from-zone condition |
| Applied Geometry | S14 | `sos` | ✅ done | create token copy of permanent |
| Arcane Omens | S7 | `sos` | ✅ done | Converge colors-of-mana discard |
| Archaic's Agony | S7,S15,ExcessDamage,multi-top-exile | `sos` | ⏳ | S7+S15 now DONE, but still needs: (a) an **excess-damage** value (damage beyond the creature's toughness) and (b) **multi-card** top-of-library impulse-exile (`TopOfLibrary` is single-card) — "exile cards equal to the excess damage, play them until your next turn" |
| Ark of Hunger | S9,S15 | `sos` | ⏳ | graveyard-leave trigger + impulse play |
| Aziza, Mage Tower Captain | S14 | `sos` | ⏳ | copy your instant/sorcery spell |
| Banishing Betrayal | S1 | `sos` | ✅ done | bounce + Surveil 1 |
| Berta, Wise Extrapolator | S6 | `sos` | ⏳ | Increment + counters-placed mana trigger |
| Blech, Loafing Pest | S16 | `sos` | ✅ done | whenever-you-gain-life counter trigger |
| Brush Off | S12 | `sos` | ⏳ | conditional cost reduction if targets a spell |
| Choreographed Sparks | S14 | `sos` | ⏳ | copy instant/sorcery or creature spell |
| Colorstorm Stallion | S5,S14,S17 | `sos` | ⏳ | Ward cost + Opus + token-copy |
| Comforting Counsel | S16 | `sos` | ✅ done | gain-life counter trigger + conditional anthem |
| Conciliator's Duelist | S8 | `sos` | ⏳ | Repartee cast-targets-creature trigger |
| Cuboid Colony | S6 | `sos` | ✅ done | Increment on flash flyer |
| Daydream | S10 | `sos` | ⏳ | blink with counter + flashback |
| Deluge Virtuoso | S3,S5 | `sos` | ✅ done | stun counter ETB + Opus trigger |
| Diary of Dreams | S12 | `sos` | ⏳ | activation cost scales down per counter |
| Dig Site Inventory | S10 | `sos` | ✅ done | counter + vigilance, flashback |
| Duel Tactics | S10 | `sos` | ✅ done | damage + can't-block, flashback |
| Efflorescence | S4 | `sos` | ✅ done | Infusion gained-life-this-turn condition |
| Elemental Mascot | S5,S15 | `sos` | ✅ done | Opus cast-trigger: +1/+0; if 5+ mana spent, impulse-exile top card (`ExileForPlay{TopOfLibrary}`) castable until your next turn |
| Emil, Vastlands Roamer | DistinctNames | `sos` | ⏳ | X = differently-named lands you control |
| End of the Hunt | GreatestMV | `sos` | ⏳ | select greatest-MV creature/pw |
| Essenceknit Scholar | S11 | `sos` | ✅ done | Pest token with attack-lifegain ability |
| Eternal Student | S18 | `sos` | ✅ done | {1}{B},exile-from-graveyard activated ability |
| Exhibition Tidecaller | S5 | `sos` | ✅ done | Opus mill trigger, mana-spent threshold |
| Expressive Firedancer | S5 | `sos` | ✅ done | Opus self-pump, mana-spent threshold |
| Fields of Strife | S1 | `sos` | ✅ done | land ability surveil 1 |
| Fix What's Broken | PayXLife | `sos` | ⏳ | additional cost pay X life; reanimate MV=X |
| Flashback | S10 | `sos` | ⏳ | grants flashback to graveyard card |
| Flow State | S2 | `sos` | ✅ done | look-and-pick top three to hand |
| Follow the Lumarets | S2,S4 | `sos` | ✅ done | filtered look-pick (creature/land) + Infusion take 1→2 |
| Foolish Fate | S4 | `sos` | ✅ done | destroy plus infusion gained-life drain |
| Forum Necroscribe | S8,S17 | `sos` | ⏳ | Ward—Discard + Repartee reanimation |
| Forum of Amity | S1 | `sos` | ✅ done | land ability surveil 1 |
| Fractal Anomaly | S19 | `sos` | ⏳ | X = cards drawn this turn |
| Fractal Mascot | S3 | `sos` | ✅ done | ETB tap plus stun counter |
| Fractal Tender | S6,S17 | `sos` | ⏳ | Increment, Ward, conditional end-step token |
| Garrison Excavator | S9 | `sos` | ✅ done | cards-leave-graveyard trigger makes token |
| Geometer's Arthropod | S2,S21 | `sos` | ⏳ | cast-spell-with-X trigger + look-and-pick |
| Graduation Day | S8 | `sos` | ✅ done | Repartee grants counter |
| Great Hall of the Biblioplex | S13 | `sos` | ⏳ | I/S-restricted mana; animates to creature |
| Group Project | S10 | `sos` | ⏳ | flashback with tap-creatures cost |
| Growth Curve | S20 | `sos` | ⏳ | double +1/+1 counters on a target |
| Hardened Academic | S9 | `sos` | ⏳ | cards-leave-graveyard trigger grants counter |
| Homesickness | S3 | `sos` | ⏳ | draw, tap, stun counters |
| Hungry Graffalon | S6 | `sos` | ✅ done | Increment mechanic |
| Hydro-Channeler | S13 | `sos` | ◑ partial | `{T}: Add {U}` I/S-restricted (S13 lander) done; `{1},{T}: Add any` restricted deferred (mana-ability-with-mana-cost, unmodeled) via `.incomplete()` |
| Imperious Inkmage | S1 | `sos` | ✅ done | ETB surveil 2 |
| Informed Inkwright | S8 | `sos` | ✅ done | Repartee makes Inkling token |
| Inkling Mascot | S8,S1 | `sos` | ✅ done | Repartee grants flying, surveil |
| Inkshape Demonstrator | S17,S8 | `sos` | ⏳ | Ward, Repartee pump/lifelink |
| Killian's Confidence | S18 | `sos` | ⏳ | triggered ability functions from graveyard |
| Lecturing Scornmage | S8 | `sos` | ✅ done | Repartee self-counter |
| Living History | S9 | `sos` | ⏳ | attack trigger gated on graveyard-leave |
| Lumaret's Favor | S14,S4 | `sos` | ⏳ | conditional copy (infusion) plus pump |
| Magmablood Archaic | S5,S7,mono-hybrid | `sos` | ✅ done | Converge; I/S trigger scales by colors |
| Mana Sculpt | S5 | `sos` | ⏳ | counter; delayed mana = mana spent |
| Mathemagics | Native | `sos` | ⏳ | draw 2^X (one-off value) |
| Matterbending Mage | S21 | `sos` | ⏳ | cast-spell-with-X trigger -> unblockable |
| Melancholic Poet | S8 | `sos` | ✅ done | Repartee drain |
| Mica, Reader of Ruins | S14,S17 | `sos` | ⏳ | Ward-pay-life; copy I/S on sacrifice |
| Molten Note | S10 | `sos` | ✅ done | flashback; damage equals mana spent |
| Molten-Core Maestro | S5 | `sos` | ✅ done | Opus cast-trigger with mana-spent condition |
| Moseo, Vein's New Dean | S4,S11 | `sos` | ⏳ | Pest token with ability + Infusion reanimate |
| Muse Seeker | S5 | `sos` | ✅ done | Opus cast-trigger |
| Muse's Encouragement | S1 | `sos` | ✅ done | surveil 2 (keyword-only token) |
| Old-Growth Educator | S4 | `sos` | ✅ done | Infusion gained-life-this-turn condition |
| Orysa, Tide Choreographer | S12 | `sos` | ⏳ | conditional cost reduction on toughness |
| Owlin Historian | S1,S9 | `sos` | ✅ done | surveil + cards-leave-graveyard trigger |
| Paradox Gardens | S1 | `sos` | ✅ done | surveil activated ability |
| Paradox Surveyor | S2 | `sos` | ✅ done | look-and-pick ETB selection |
| Pensive Professor | S6 | `sos` | ⏳ | Increment (plus counter-added trigger) |
| Pest Mascot | S16 | `sos` | ✅ done | whenever-you-gain-life trigger |
| Pestbrood Sloth | S11 | `sos` | ✅ done | Pest token with attack ability |
| Poisoner's Apprentice | S4 | `sos` | ✅ done | Infusion gained-life-this-turn condition |
| Postmortem Professor | S18 | `sos` | ⏳ | exile-from-graveyard recursion + attack drain |
| Potioner's Trove | S22 | `sos` | ⏳ | activate only if cast an I/S this turn |
| Pox Plague | Native | `sos` | ⏳ | halve life/hand/permanents (one-off) |
| Practiced Offense | S10 | `sos` | ⏳ | flashback |
| Practiced Scrollsmith | S15 | `sos` | ✅ done | ETB impulse-exile target noncreature/nonland from your gy, castable until end of your next turn (`ExileForPlay{YourNextTurn}`; `{R/W}` hybrid + first strike) |
| Primary Research | S9 | `sos` | ✅ done | card-left-graveyard-this-turn condition |
| Prismari Charm | S1 | `sos` | ⏳ | surveil mode |
| Procrastinate | S3 | `sos` | ✅ done | stun counters (twice X) |
| Pursue the Past | S10 | `sos` | ✅ done | flashback |
| Quandrix Charm | SoftCounter | `sos` | ⏳ | counter-unless-pay mode |
| Rabid Attack | GrantAbility | `sos` | ⏳ | grant ad-hoc dies-draw ability EOT |
| Rancorous Archaic | S7 | `sos` | ⏳ | Converge counters equal colors spent |
| Rapier Wit | S3 | `sos` | ✅ done | stun counter |
| Rehearsed Debater | S8 | `sos` | ✅ done | Repartee targets-a-creature trigger |
| Render Speechless | DirectedDiscard | `sos` | ⏳ | you choose opponent's discarded card |
| Root Manipulation | GrantAbility | `sos` | ⏳ | grant ad-hoc attacks-gain-life EOT |
| Run Behind | S12 | `sos` | ⏳ | conditional cost reduction targeting attacker |
| Scolding Administrator | S8 | `sos` | ⏳ | Repartee targets-a-creature trigger |
| Seize the Spoils | S11 | `khm` | ⏳ | Treasure token with ability |
| Send in the Pest | S11 | `sos` | ✅ done | Pest token with attack ability |
| Slumbering Trudge | S3 | `sos` | ⏳ | enters with stun counters |
| Snarl Song | S7 | `sos` | ⏳ | converge, colors of mana spent |
| Snooping Page | S8 | `sos` | ⏳ | Repartee: cast IS targeting creature |
| Soaring Stoneglider | AltCost | `sos` | ⏳ | modal additional cost (exile 2 gy or pay) |
| Social Snub | S14 | `sos` | ⏳ | copy this spell |
| Spectacle Summit | S1 | `sos` | ✅ done | activated surveil 1 |
| Spectacular Skywhale | S5 | `sos` | ✅ done | Opus cast-IS trigger, mana spent |
| Spirit Mascot | S9 | `sos` | ✅ done | cards leave graveyard trigger |
| Steal the Show | Native | `sos` | ⏳ | wheel: discard any number, draw that many |
| Stirring Honormancer | S2 | `sos` | ✅ done | look at top X, pick one |
| Stirring Hopesinger | S8 | `sos` | ✅ done | Repartee: cast IS targeting creature |
| Stone Docent | S1,S18 | `sos` | ✅ done | graveyard-activated gain-life + surveil |
| Stress Dream | S2 | `sos` | ✅ done | look-and-pick top two |
| Summoned Dromedary | S18 | `sos` | ⏳ | {1}{W} return this from graveyard to hand |
| Sundering Archaic | S7 | `sos` | ⏳ | converge, colors of mana spent |
| Suspend Aggression | S15 | `sos` | ✅ done | exile target nonland permanent + top of library; each playable through its OWNER's next turn (Sequence of two `ExileForPlay`, per-owner window) |
| Tablet of Discovery | S13,S15 | `sos` | ⏳ | impulse-play milled card; restricted mana |
| Tackle Artist | S5 | `sos` | ✅ done | Opus cast-IS trigger, mana spent |
| Teacher's Pest | S18 | `sos` | ⏳ | {B}{G} return this from graveyard |
| Tenured Concocter | S4 | `sos` | ✅ done | Infusion: gained-life-this-turn condition |
| Tester of the Tangential | S6 | `sos` | ⏳ | Increment trigger |
| Textbook Tabulator | S1,S6 | `sos` | ✅ done | Increment plus surveil 2 |
| The Dawning Archaic | S10,S12 | `sos` | ⏳ | cast from graveyard; count-based cost reduction |
| Thornfist Striker | S4,S17 | `sos` | ⏳ | Ward cost plus Infusion |
| Thunderdrum Soloist | S5 | `sos` | ✅ done | Opus cast-IS trigger, mana spent |
| Titan's Grave | S1 | `sos` | ✅ done | activated surveil 1 |
| Together as One | S7 | `sos` | ✅ done | converge, colors of mana spent |
| Tome Blast | S10 | `sos` | ✅ done | Flashback |
| Topiary Lecturer | S6 | `sos` | ⏳ | Increment; mana equal to power |
| Tragedy Feaster | S4,S17 | `sos` | ⏳ | Ward—Discard plus Infusion |
| Transcendent Archaic | S7 | `sos` | ✅ done | converge, colors of mana spent |
| Ulna Alley Shopkeep | S4 | `sos` | ✅ done | Infusion: gained-life-this-turn condition |
| Unsubtle Mockery | S1 | `sos` | ✅ done | damage plus surveil 1 |
| Vicious Rivalry | PayXLife | `sos` | ⏳ | additional cost pay X life; destroy MV<=X |
| Visionary's Dance | S2 | `sos` | ✅ done | look-and-pick top two |
| Wild Hypothesis | S1 | `sos` | ⏳ | Fractal token; surveil 2 |
| Wildgrowth Archaic | S7,mono-hybrid | `sos` | ◑ partial | converge body done; creature-cast counter-injection trigger deferred |
| Wilt in the Heat | S9,S12 | `sos` | ⏳ | graveyard-leave conditional cost reduction |
| Wisdom of Ages | NoMaxHand | `sos` | ⏳ | no maximum hand size static |
| Withering Curse | S4 | `sos` | ⏳ | Infusion: gained-life-this-turn condition |
| Zimone's Experiment | S2 | `sos` | ⏳ | look-and-pick top five |

### T4 — 55 cards

| Card | Caps | Folder | Status | Gating clause |
|---|---|---|---|---|
| Abigale, Poet Laureate // Heroic Stanza | DFC | `sos` | ⏳ | modal double-faced card |
| Adventurous Eater // Have a Bite | DFC | `sos` | ⏳ | modal double-faced card |
| Biblioplex Tomekeeper | Prepare | `sos` | ⏳ | prepared/unprepared keyword subsystem |
| Blazing Firesinger // Seething Song | DFC | `sos` | ⏳ | modal double-faced card |
| Campus Composer // Aqueous Aria | DFC | `sos` | ⏳ | modal double-faced card |
| Cheerful Osteomancer // Raise Dead | DFC | `sos` | ⏳ | modal double-faced card |
| Decorum Dissertation | Paradigm | `sos` | ⏳ | Lesson Paradigm subsystem |
| Echocasting Symposium | Paradigm | `sos` | ⏳ | Lesson Paradigm subsystem |
| Elite Interceptor // Rejoinder | DFC | `sos` | ⏳ | modal double-faced card |
| Emeritus of Abundance // Regrowth | DFC | `sos` | ⏳ | modal double-faced card |
| Emeritus of Conflict // Lightning Bolt | DFC | `sos` | ⏳ | modal double-faced card |
| Emeritus of Ideation // Ancestral Recall | DFC | `sos` | ⏳ | modal double-faced card |
| Emeritus of Truce // Swords to Plowshares | DFC | `sos` | ⏳ | modal double-faced card |
| Emeritus of Woe // Demonic Tutor | DFC | `sos` | ⏳ | modal double-faced card |
| Encouraging Aviator // Jump | DFC | `sos` | ⏳ | modal double-faced card |
| Germination Practicum | Paradigm | `sos` | ⏳ | Lesson Paradigm subsystem |
| Goblin Glasswright // Craft with Pride | DFC | `sos` | ⏳ | double-faced card |
| Grave Researcher // Reanimate | DFC | `sos` | ⏳ | double-faced card |
| Harmonized Trio // Brainstorm | DFC | `sos` | ⏳ | double-faced card |
| Honorbound Page // Forum's Favor | DFC | `sos` | ⏳ | double-faced card |
| Improvisation Capstone | Paradigm | `sos` | ⏳ | Lesson Paradigm subsystem |
| Infirmary Healer // Stream of Life | DFC | `sos` | ⏳ | double-faced card |
| Jadzi, Steward of Fate // Oracle's Gift | DFC | `sos` | ⏳ | double-faced card |
| Joined Researchers // Secret Rendezvous | DFC | `sos` | ⏳ | double-faced card |
| Kirol, History Buff // Pack a Punch | DFC | `sos` | ⏳ | double-faced card |
| Landscape Painter // Vibrant Idea | DFC | `sos` | ⏳ | double-faced card |
| Leech Collector // Bloodletting | DFC | `sos` | ⏳ | double-faced card |
| Lluwen, Exchange Student // Pest Friend | DFC | `sos` | ⏳ | double-faced card |
| Lorehold, the Historian | Miracle | `sos` | ⏳ | grants miracle keyword subsystem |
| Maelstrom Artisan // Rocket Volley | DFC | `sos` | ⏳ | double-faced card |
| Nita, Forum Conciliator | Native | `sos` | ⏳ | cast-spell-you-don't-own trigger + theft-cast |
| Page, Loose Leaf | Grandeur | `sos` | ⏳ | Grandeur keyword subsystem |
| Petrified Hamlet | NameChoice | `sos` | ⏳ | choose a card name -> name-scoped statics |
| Pigment Wrangler // Striking Palette | DFC | `sos` | ⏳ | modal double-faced card |
| Prismari, the Inspiration | Storm | `sos` | ⏳ | Elder Dragon granting storm |
| Professor Dellian Fel | PW | `sos` | ⏳ | planeswalker loyalty subsystem |
| Quandrix, the Proof | Cascade | `sos` | ⏳ | Elder Dragon granting cascade |
| Quill-Blade Laureate // Twofold Intent | DFC | `sos` | ⏳ | modal double-faced card |
| Ral Zarek, Guest Lecturer | PW | `sos` | ⏳ | planeswalker loyalty subsystem |
| Resonating Lute | GrantAbility | `sos` | ⏳ | grant mana ability to all your lands |
| Restoration Seminar | Paradigm | `sos` | ⏳ | Lesson Paradigm subsystem |
| Sanar, Unfinished Genius // Wild Idea | DFC | `sos` | ⏳ | modal double-faced card |
| Scathing Shadelock // Venomous Words | DFC | `sos` | ⏳ | modal double-faced card |
| Scheming Silvertongue // Sign in Blood | DFC | `sos` | ⏳ | modal double-faced card |
| Silverquill, the Disputant | Casualty | `sos` | ⏳ | casualty keyword subsystem |
| Skycoach Conductor // All Aboard | DFC | `sos` | ⏳ | modal double-faced card |
| Skycoach Waypoint | prepare | `sos` | ⏳ | grants prepared; prepare subsystem |
| Spellbook Seeker // Careful Study | DFC | `sos` | ⏳ | modal double-faced card |
| Spiritcall Enthusiast // Scrollboost | DFC | `sos` | ⏳ | modal double-faced card |
| Strife Scholar // Awaken the Ages | DFC | `sos` | ⏳ | modal double-faced card |
| Studious First-Year // Rampant Growth | DFC | `sos` | ⏳ | modal double-faced card |
| Tam, Observant Sequencer // Deep Sight | DFC | `sos` | ⏳ | modal double-faced card |
| Vastlands Scavenger // Bind to Life | DFC | `sos` | ⏳ | modal double-faced card |
| Witherbloom, the Balancer | Affinity | `sos` | ⏳ | affinity keyword subsystem |
| Zaffai and the Tempests | FreeCast | `sos` | ⏳ | once/turn free-cast permission |

## S10 Flashback — scoped implementation plan (warp-mirror)

Flashback is structurally the **warp** mechanic (alt-cost cast from a non-hand zone + a zone change
when it resolves). Mirror warp site-for-site:

1. `effects/ability.rs`: add `Ability::Flashback { cost: ManaCost }` (like `Ability::Warp`) and
   `CastVariant::Flashback`.
2. `state/mod.rs`: add `Object.flashback_cast: bool` (mirror `warp_cast`); reset it in `move_object`
   (CR 400.7) alongside `warp_cast`.
3. `priority.rs`:
   - `flashback_cost(card)` helper (mirror `warp_cost`, reads `Ability::Flashback`).
   - `legal_priority_actions` (~958): offer `CastVariant::Flashback` for cards **in the graveyard**
     whose def has `Ability::Flashback`, at the card's normal timing (sorcery→sorcery-speed,
     instant→instant-speed). Mirror the warp-from-hand block (~1009) but source = `Zone::Graveyard`.
   - `cost_for_variant` (~1489): `CastVariant::Flashback => self.flashback_cost(card)`.
   - source-zone removal (~1655): allow `Zone::Graveyard` for a flashback cast.
   - set `o.flashback_cast = true` at cast (mirror warp_cast flag ~1508).
   - `resolve_top` (~1928/1992): if `flashback_cast`, move the card to **Exile** instead of graveyard
     (CR 702.34 — "instead of putting it anywhere else, exile it"). This is the one place flashback
     *differs* from warp (warp arms an end-step exile; flashback exiles immediately on resolution).
4. Cards: Daydream, Antiquities on the Loose, Dig Site Inventory, Duel Tactics, Practiced Offense,
   Flashback (the card), etc. — each declares `Ability::Flashback { cost }` + its normal spell effect.

Test: cast a sorcery from graveyard via Flashback → effect resolves → card is in Exile (not graveyard);
and it's no longer offered for a second flashback.

## S11 token-with-ability — ✅ DONE (`bf22f6b`, synthetic token defs)

**Decision (lead-approved):** `TokenSpec.grp_id` (0 = vanilla) + pre-registered token defs in the reserved
**9000+** block (`grp::PEST_TOKEN = 9001`). Rationale: keeps token abilities in *defs* (card-agnostic
law — no name-match), mirrors how MTGA ids tokens, and the reserved block sits far above organically
growing real-card ids (~290) so no collision. **Confirmed** the `/api/cards` catalog filters
`!supertypes.contains(Token)` (server.rs:500), so the Pest def does **not** leak into the deck-builder;
token defs still flow into the art manifest (intended — tokens get art). `SelfAttacks` already fires,
so the Pest's attack-trigger works via `def_of`. Shipped: Send in the Pest, Pestbrood Sloth (Essenceknit
Scholar / Moseo defer — creature-died-this-turn / Infusion-X-reanimate clauses).

### original plan (kept for reference)

Problem: a token's ability lookup is `def_of(id)` → `CardDb.get(chars.grp_id)`; there is no
object-level ability storage, and the db is `Arc<CardDb>`. Keywords already ride on
`TokenSpec.keywords`; only *triggered/activated* token abilities (the Pest dies-trigger) need this.

Approach — give ability-bearing tokens a real `grp_id` pointing at a pre-registered def:
1. `effects/target.rs`: add `grp_id: u32` to `TokenSpec` (no `Default` derive, so **update all 8
   existing `TokenSpec {…}` literals** in `cards/helpers.rs` with `grp_id: 0` — vanilla/keyword-only
   tokens). Bump the `TokenSpec` serde/expect snapshots.
2. `whiteboard.rs` `create_token`: set `chars.grp_id = spec.grp_id;` (0 → no def, as today).
3. `cards/`: pre-register the **Pest token def** (`{}` 1/1 B/G Pest, `Triggered{SelfEnters? no —
   SelfDies, GainLife 1}`) in `starter_db` under a reserved id (e.g. `grp::PEST_TOKEN = 90001`), and
   set `helpers::pest_token().grp_id = PEST_TOKEN`. `SelfDies` is already wired (priority.rs 2539), so
   the trigger fires once the token carries the ability via `def_of`.
4. Cards (4): Send in the Pest, Essenceknit Scholar, Moseo (Vein's New Dean), Pestbrood Sloth.

Test: create a Pest token, kill it (SBA), assert its controller gained 1 life (the dies-trigger fired
through the synthetic def).

## S14 token-copy — ✅ DONE (`a8c8a2d`)
`Effect::CreateTokenCopy { source: EffectTarget, controller, mods: TokenCopyMods }` — the materialize
arm snapshots the source's **copiable** characteristics (its base `chars`: name/types/subtypes/colors/
P·T + abilities via the copied `grp_id`; **not** counters/damage/auras/other continuous effects, CR
707.2) into a `TokenSpec`, applies the `mods` CR 707.9e "except" overrides (`add_card_types` /
`add_subtypes` / `set_power_toughness` / `counters`), then reuses the existing `create_token` path.
`collect_specs_into` gained a `CreateTokenCopy{ source: Target }` arm so the copy target is enumerated
at cast. → **Applied Geometry** (copy a permanent as a 0/0 Fractal + six +1/+1 → a 6/6).
**Deferred token-copy consumers:** Colorstorm Stallion (also needs S17 Ward — build with Ward, uses the
SourceSelf/empty-`mods` copy-self path), Echocasting Symposium (Paradigm, T4). The **spell-copy** half
of S14 ("copy target/that spell" → a copy on the stack — Aziza, Choreographed Sparks, Mica, Social Snub,
Lumaret's Favor) is a **different mechanic** (stack object, not a battlefield token) and is still ⏳.

## Hybrid mana — the next high-value blocker (7 non-DFC cards)

`ManaCost` has no hybrid `{X/Y}` pip. This blocks 7 non-DFC SoS cards (Essenceknit Scholar,
Stirring Honormancer, Moseo, Abstract Paintmage, …) AND their riders. Scope:
- `basics::ManaCost`: add a hybrid-pip representation (e.g. `hybrid: Vec<(Color, Color)>`, each payable
  by either colour; keep `colored`/`generic` as-is).
- `mana::select_payment`: when planning, satisfy each hybrid pip with whichever of its two colours the
  player can produce (try both). `mana_value` counts each hybrid pip as 1.
- Card builders: extend `mana_cost` (or add `mana_cost_hybrid`) to author `{B/G}` etc.
Note: the **"creature died under your control this turn" flag** was scoped + reverted (only consumer,
Essenceknit Scholar, is hybrid-blocked) — rebuild it *with* Essenceknit once hybrid mana lands. Pattern
mirrors `cards_left_graveyard_this_turn`: Player counter, increment in the CreatureDies SBA (by the
creature's controller at death), reset in begin_turn, `Condition::CreatureDiedThisTurn`.

## Remaining cap queue (all engine files released; pick by fresh-context fit)
- **Hybrid mana** (above) — 7 cards, payment-planner change.
- **S7 Converge** — track *colors* of mana spent at cast (extend `auto_pay` to report spent colours →
  record `Object.colors_spent` → `ValueExpr::ColorsOfManaSpent`). ~8 Archaic-cycle cards.
- **S18 graveyard-activated** — activate an ability from the graveyard (discard/exile cost); extend the
  activated-ability enumeration to scan the graveyard for a graveyard-source ability.
- **S9-trigger** (graveyard-leave event), **CreatureDies trigger** (needs LKI), **S14 token-copy**
  (extends S11 — copy the target's `grp_id`+chars onto the token).

## Precedent: revert-rather-than-ship-unused-cap
When a scoped cap's *only* consumer turns out to be blocked by a different missing feature, **revert the
cap** rather than ship engine infra (a field / Condition / ValueExpr) with no card exercising it. Ship
caps only with a card that lands them. (Established when the "creature-died-this-turn" flag's only user,
Essenceknit Scholar, was found hybrid-mana-blocked — flag reverted, rebuild it *with* Essenceknit once
hybrid lands.)

## Hybrid mana — ✅ DONE (`8daf069`, `{X/Y}` two-colour pips)
`ManaCost.hybrid: Vec<(Color,Color)>` (serde-default) + `select_payment` satisfies each hybrid pip with
a unit of either colour (after fixed pips, before generic; shared by `can_pay`+`auto_pay`) + `mana_value`
counts each hybrid pip as 1 + `mana_cost_hybrid()` builder. **Wire:** gym `obs.rs` doesn't encode raw
ManaCost fields (transparent); the web client (`main.ts`) renders from `generic`/`colored` and ignores
`hybrid` → a hybrid card shows its pip incomplete but **does not crash** (graceful, per lead). Follow-up
(UI team): render `{X/Y}` pips in `main.ts`. → Stirring Honormancer.

### Monocolour hybrid `{N/C}` — ✅ DONE (`01fe254`)
`ManaCost.mono_hybrid: Vec<(u32,Color)>` (serde-default) — each `{2/R}` pip payable by ONE mana of the
colour OR `n` generic; `select_payment` prefers the colour side (uses fewer units, never starves a later
pip), else falls back to `n` generic (after fixed + two-colour hybrid, before generic). `mana_value` adds
each pip's `n` (CR 202.3g); `Display` now renders both `{c1/c2}` and `{n/C}` pips; `mana_cost_mono_hybrid()`
builder. **Also fixed a latent bug:** the cast-payment cost at `priority.rs` was dropping `hybrid`
(and would have dropped `mono_hybrid`) — an all-mono-hybrid card would have cast **free** with zero
Converge colours. Now the payment carries `hybrid`+`mono_hybrid` through, so they're actually paid and
their spent colours feed Converge (this also fixes two-colour hybrid under-costing, e.g. Stirring
Honormancer). New `ValueExpr::ColorsSpentOnTrigger` (colours spent on the *triggering* spell — the
colours-of-trigger analogue of `ManaSpentOnTrigger`) for Magmablood's cast-trigger.
→ **Magmablood Archaic** (fully implemented: Converge enters-with `ColorsSpent` + Opus mass-pump by
`ColorsSpentOnTrigger`), **Wildgrowth Archaic** (`.incomplete()`: mono-hybrid + Converge body work; the
creature-cast "enters with X additional counters" trigger is deferred — needs a delayed enters-with
replacement keyed to another spell on the stack, an unbuilt mechanism).
_Latent gap (not blocking, no consumer):_ `mana_spent` (Dyadrine's `ValueExpr::ManaSpent`) is still
computed as `generic + colored` at cast, so it under-counts hybrid/mono-hybrid pips. No hybrid card reads
`ManaSpent` today; fix needs `auto_pay` to also report the unit count spent.

Next hybrid follow-up: rebuild the creature-died flag *with* Essenceknit Scholar (now unblocked); then
Moseo, Abstract Paintmage.

## Discard-cost activated — ✅ DONE (`CostComponent::Discard` wired)
`CostComponent::Discard(SelectSpec)` already existed but was **defined-but-unpaid** (`_ => {}` in
`pay_cost`, `_ => true` in `can_pay_cost`). Now wired: `can_pay_cost` gates on having ≥`min` matching
cards in `spec.zone` (the hand); `pay_cost` calls `pay_discard` (mirrors `pay_sacrifice` — asks which to
discard when there's a choice, moves to graveyard). `can_pay_cost` made `pub(crate)` for card-level cost
tests. → **Charging Strifeknight** (`{T}, Discard a card: Draw`). Unblocks the discard-cost half of
Hardened Academic (still needs S9-trigger — has one) / Rubble Rouser (reflexive-mana, defer).

## S18 graveyard-activated — ✅ DONE (`6190bb2`)
_(scoped plan below, now implemented: `CostComponent::ExileSelfFromGraveyard` + graveyard enumeration in `legal_priority_actions` + exile-on-pay. → Eternal Student, Stone Docent. Postmortem Professor / Rubble Rouser still deferred.)_

### original plan
Cards: **Eternal Student** (`{1}{B}, Exile this from your graveyard: create two Inklings`), **Stone
Docent** (`{W}, Exile this from graveyard: gain 2, surveil 1; sorcery-speed`). (Postmortem Professor /
Rubble Rouser need reanimate-self / reflexive-mana — defer.)
1. `effects/ability.rs`: `CostComponent::ExileSelfFromGraveyard` — both the "exile this card" cost AND
   the marker that this `Activated` ability is usable from the graveyard (no new zone field on
   `Activated`; the cost component signals the zone, keeping the literals unbroken).
2. `priority.rs` `legal_priority_actions`: after the battlefield activated-ability scan, scan
   `player.graveyard`; for each card whose def has an `Activated` ability whose cost contains
   `ExileSelfFromGraveyard`, offer it if the mana is affordable and timing ok (respect
   `Restriction`/sorcery-speed).
3. Paying: exile the card (move to Exile) as part of the cost, then the ability's effect resolves.
4. Test: card in graveyard + mana → offered; activate → card exiled + effect ran (two Inklings).

## S15 impulse-play — ◑ BASE DONE (`d079eb0`) — adopted from orphaned predecessor WIP

**Provenance:** the engine base (steps 1–2 below) was found as ~90%-complete **uncommitted** work in the
shared tree — a predecessor was mid-build when its process was terminated to free resources. Reviewed
hunk-by-hunk against this plan, confirmed compiling + consistent with the warp/flashback idioms, then
hardened with tests I wrote (interpreter arm, ETB exile+grant, offer window/expiry) and landed with the
first consumer card.

**Shipped:** `Effect::ExileForPlay { what, window: PlayWindow }` + `Action::ExileForPlay { obj, until }`
+ `Object.play_until_turn: Option<u32>` (reset on any zone change, CR 400.7) + the **unified** exile-cast
offer loop in `legal_priority_actions` (warp-recast = sorcery-speed/no-limit; impulse = card's own timing
within `play_until_turn`). Whiteboard interpreter arm handles the **`Target`** source with 2-player
"your next turn" arithmetic (+2 if it's already your turn, else +1). → **Practiced Scrollsmith** (ETB
exile a target noncreature/nonland card from your gy, castable until end of your next turn).

**Top-of-library source — ✅ DONE (`0e17d3e`):** `EffectTarget::TopOfLibrary(PlayerRef)` + a `resolve_target`
arm (returns the top card = `library.last()`, no-op on empty); the existing `ExileForPlay` arm handles it
unchanged → Elemental Mascot, Suspend Aggression.

**Land-play-from-exile — ✅ DONE (`0e17d3e`):** the land-drop block in `legal_priority_actions` now also
offers `PlayLand` for an impulse-exiled land (`castable_from_exile` + `play_until_turn` within window),
respecting the land-per-turn limit; `play_land`→`MoveZone`→`move_object` already handles the exile source
zone. (Distinct from the pre-existing `PlayLandsFrom`-permission branch at priority.rs ~977.)

**Still ⏳ — Graveyard-play** (`PlayWindow::ThisTurn` from the graveyard) — Ark of Hunger / Tablet of
Discovery play a **milled** card (graveyard, not exile); `castable_from_exile`/the offer loop scan only
exile. Needs a graveyard analog (a `play_from_graveyard_until` flag + a graveyard scan in the offer loop,
OR generalise the flag zone-agnostically). Defer to a fresh increment WITH Ark of Hunger (Tablet also
needs S13). Revert-unused-cap precedent.

### original scoped plan (foundation already existed)
"Exile [a card] — you may **play** it until [end of turn / end of your next turn]." **Good news:** the
warp-recast mechanism already gives us most of it — `Object.castable_from_exile: bool`
(`state/mod.rs:157`, reset on any zone change per CR 400.7) + an offer loop (`priority.rs:1029-1041`)
that already offers *casting* an exiled card with that flag for its normal mana cost. S15 = **extend**
that, don't rebuild:
1. **Effect to exile-and-permit.** Add `Effect::ImpulseExile { source, count, until }` (or extend an
   exile effect) that moves the card(s) to exile AND sets `castable_from_exile = true` + a new
   `Object.play_until_turn: Option<u32>` marker (absolute turn number). `source` covers: top-of-library
   (Elemental Mascot, Suspend Aggression's top card), a chosen target permanent (Suspend Aggression's
   "exile target nonland permanent"), a target graveyard card (Practiced Scrollsmith).
2. **Offer loop (`priority.rs:1029`) — three gaps to close vs warp-recast:**
   - **Timing:** warp-recast is sorcery-speed only; impulse follows the *card's own* timing (instant/
     Flash → instant speed) — mirror the flashback timing check at `priority.rs:1049-1051`.
   - **Lands:** the flag currently only drives `Cast`; a *land* in exile with the flag needs a
     `play_land`-from-exile offer (impulse "play", not just "cast").
   - **Expiry:** skip the offer when `play_until_turn` has passed. Set it: "until end of turn" =
     current turn number; "until end of your next turn" = your next turn's number (spans an opponent
     turn — compute from turn order). Clear expired markers in `begin_turn` (`priority.rs:687`, next to
     the `life_gained_this_turn = 0` resets) or leave them (expiry is checked at offer time anyway).
3. **Zone note:** Tablet of Discovery plays a **milled** card (from the *graveyard*, not exile). Either
   generalise the flag to "playable-from-current-zone" or scope Tablet separately; the exile cases
   (Elemental Mascot, Suspend Aggression, Practiced Scrollsmith, Archaic's Agony, Ark of Hunger,
   Suspend Aggression, Practiced Offense) are the clean first batch.
4. **Cards:** Elemental Mascot (S5 Opus + impulse), Suspend Aggression, Practiced Scrollsmith
   (mono-hybrid `{R/W}` — done), Archaic's Agony (S7 + impulse), Ark of Hunger (S9 + impulse), Tablet of
   Discovery (S13 + impulse, graveyard-play). Test: exile top card → it's offered as a play → play it →
   resolves; after expiry it's no longer offered.

## S13 restricted-mana — ✅ DONE (`ffcc0df`)

Implemented per the scoped plan below, with one scope note. `ManaSpec.restriction: Option<SpendRestriction>`
(`InstantSorceryOnly`) + a separate `ManaPool.restricted` bucket (empties with the pool). `allow_restricted`
is threaded `payment_units → can_pay_excluding/auto_pay_ex` (thin `can_pay`/`can_pay_ex`/`auto_pay` wrappers
keep the ~26 existing `can_pay` call sites untouched); restricted pool mana + restricted mana sources
(`restricted_mana_sources`, split out of `producible_colors`) fold in only when the cost is an instant/sorcery
cast. Cast/offer sites pass `card is I/S`; ability-cost sites pass `false`. `spend_from_pool` spends restricted
mana first (no waste); `add_mana` routes restricted production to the bucket. → **Hydro-Channeler** (`{T}: Add
{U}` restricted). Tests prove restricted mana pays an I/S cost but not a creature spell / ability cost, both
from a source tap and from floating mana.

**Scope notes:**
- **Hydro-Channeler's 2nd ability** (`{1},{T}: Add any color`, restricted) is **deferred** — it's a mana ability
  with a *mana activation cost*, which the auto-pay source model treats as free-to-tap (would offer free rainbow
  mana). Omitted rather than shipped broken; needs a mana-ability-with-activation-cost cap (also blocks filter lands).
- **Manual `produce_mana`/`usable_mana_sources`** (UI-only path) still don't expose restricted sources — a documented
  UI follow-up (like the hybrid-pip one); the engine/gym auto-pay path is fully correct.
- **Remaining S13 consumers:** Abstract Paintmage (mono-hybrid done + a first-main-phase trigger that floats
  restricted `{U}{R}` — the bucket already handles floating restricted mana, so this is just the trigger + `add_mana`,
  already wired), Great Hall of the Biblioplex (also needs land-animate — defer that clause), Tablet of Discovery
  (also needs S15 graveyard-play).

### original scoped plan (kept for reference)
"Add {U}{R}. **Spend this mana only to cast instant and sorcery spells.**" All 4 cards use the SAME
restriction (I/S-only), so a bool suffices. The cost: threading "am I casting an I/S spell" through the
payment path (the reason the lead flagged it for a fresh, non-tired start).
1. `ManaSpec`: add `restriction: Option<SpendRestriction>` (enum, one variant `InstantSorceryOnly` for
   now). `add_mana` (`whiteboard.rs:644`) routes restricted mana to a new bucket.
2. `ManaPool` (`basics.rs:200`): add `restricted: BTreeMap<Color,u32>` (I/S-only mana). Empty it wherever
   `amounts` empties (CR 500.5).
3. **Thread `allow_restricted: bool`** through `payment_units` → `select_payment` → `auto_pay` /
   `can_pay_excluding`. When true, fold the restricted bucket into the available units; when false, ignore
   it. Keep `can_pay(state,p,cost)` as a thin wrapper defaulting `allow_restricted=false` so the ~8 test
   call sites and non-spell payments are unaffected.
4. **Call sites** (from the survey): spell-cast payment `priority.rs:1753` → pass `card` is instant|sorcery;
   ability-cost `pay_cost`/`can_pay_cost` (`1434`,`1218`) → `false` (restricted mana can't pay ability
   costs); offer gates (`1012`,`1019`,`1034`,`1055`) → per-card `is instant|sorcery`.
5. **Cards:** Hydro-Channeler (`{T}:Add {U}` restricted — cleanest lander), Abstract Paintmage (mono-hybrid
   `{U/R}` done + first-main-phase trigger adds restricted `{U}{R}`), Great Hall of the Biblioplex (also
   needs land-animate — defer that clause), Tablet of Discovery (also needs S15). Ship the cap with
   Hydro-Channeler. Test: restricted mana pays an I/S spell but NOT a creature spell / an ability cost.

## Session note (git hygiene)
Shared **index** in this working tree: plain `git commit` (even after `git add <my paths>`) commits the
WHOLE index and sweeps up teammates' pre-staged files. ALWAYS `git commit --only <explicit paths> -m`.
(Matches the [[shared-tree-git-hygiene]] memory's `git commit -- <paths>` rule — follow it.)
