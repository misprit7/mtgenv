# Card-implementation push — Secrets of Strixhaven (`sos`, 271 distinct cards)

Standing workstream: implement the Secrets of Strixhaven set for **limited (40-card) play** in
`mtg-core`, easiest-first, correctness over count. This ledger is the capability index + full
per-card triage, modeled on `SELESNYA_LANDFALL_CARDS.md`.

## ▶ NEXT AGENT — start here (handoff from sos-cards-15, 2026-07-05)

**▶▶ sos-cards-15 SHIPPED — the SPELL-LEVEL ADDITIONAL-CAST-COST cap (CR 601.2b/f), all 4 cards + a bonus dynamic-MV
filter. 713 mtg-core green, whole workspace builds, tree clean, LEAD pushes.** Three own-commits (`git log -S` before
re-scoping):
- **`6318597` — rails + Seize the Spoils** (discard-a-card additional cost). New general machinery: **`AdditionalCost{options:
  Vec<Cost>}`** (a possibly-**modal** "or" clause) carried as an **`Ability::AdditionalCost` marker** (NOT a `CardDef` field —
  avoids touching 40+ literals, mirrors the `CostReduction` marker idiom; read via `CardDef::additional_costs()`). Offer gate
  requires every clause payable (`Engine::additional_costs_payable` — discard excludes the on-stack spell; a mana option is
  checked jointly with the base via **`ManaCost::plus`**). `cast_spell` chooses one payable option per clause
  (`choose_additional_options`, asks only when >1 payable), folds a chosen option's mana into the mana payment, and pays the
  non-mana components (`pay_additional_nonmana`) at 601.2f–h → **discarded AT CAST, so a countered spell still paid**.
- **`a2b6a3a` — Vicious Rivalry + Fix What's Broken** (pay-**X**-life additional cost). **X-announcement generalized**: a spell
  announces X when the mana cost has `{X}` **OR** an additional cost references X (`component_uses_x`), bounded by life for
  PayLife; the single chosen X (`ValueExpr::X`, stored on the stack object) is shared. **`CostComponent::PayLife` is now wired**
  (was a dead `_ => {}` no-op) for additional costs, via `change_life` with a ctx carrying the chosen X. Plus the reusable
  **`CardFilter::ManaValueExpr{min,max: Option<Box<ValueExpr>>}`** (dynamic, X-keyed MV bound) — resolved to a concrete
  `ManaValue` against the ctx by **`resolve_dynamic_filter`** at `select_for_each` (ctx-free matchers only see the static
  form). This is the ledger's "Dynamic-MV filter" cap → **also unblocks Moseo** (MV≤life-gained: swap the bound expr).
- **`eed8a13` — Soaring Stoneglider** (modal: exile two from gy OR pay {1}{W}) — exercises the modal option choice + the
  mana-option fold on a **creature** cast (additional costs apply to any card, not just I/S).

- **`4b41def` — Quandrix Charm** (base-P/T-set cap) — modal instant reusing CounterUnlessPay + Destroy-enchantment + the new
  **`Effect::SetBasePT{power,toughness,duration}`** (CR 613 layer 7b), which lowers to the existing `GrantContinuous{SetBasePT}`
  path (a later +1/+1 counter still stacks on top → tested 6/6). No architecture; the base-P/T-set triage row is DONE.
- **`cd1fbe2` — End of the Hunt** (GreatestMV cap) — greatest-MV edict: `TargetPlayer(Opponent)` + `Exile{Select}` whose filter
  is the new **`ValueExpr::GreatestManaValue{filter,controller}`** feeding a dynamic `ManaValueExpr{min:g,max:g}` (reuses the
  additional-cast-cost session's `resolve_dynamic_filter`). The GreatestMV row is DONE.
- **`b7a1e51` — Group Project** (non-mana-flashback cap) — **widened `Ability::Flashback{cost: ManaCost}` → a full `Cost`** so a
  flashback cost can be non-mana (Group Project's "Flashback—Tap three creatures" = the shipped `TapCreatures(3)`). Offer gate +
  cast path pay the flashback components (factored `Engine::cost_components_payable` out of `can_pay_cost`; `pay_additional_nonmana`
  pays them at cast); the 6 existing flashback cards migrated to the new `cards::flashback(mana)` helper. Flashback-non-mana row DONE.

**Census now 222/271 authored (82%). 0 Native escape hatches. Rows DONE: additional-cast-cost · base-P/T-set · GreatestMV · Flashback-non-mana.**

### ▶ Where sos-cards-15 points you (unchanged tail, minus the additional-cast-cost row)
Work the by-cap triage below (grouped by yield). Highest-yield remaining caps: **Grant-a-triggered-ability-until-EOT** (Rabid
Attack, Root Manipulation), **timed-blink** (Ennis, Conciliator's Duelist — reuse `Effect::Blink` + a delayed end-step return),
**base-P/T-set** (Quandrix Charm mode 3), **Flashback-with-a-non-mana-cost** (Group Project — reuse `TapCreatures(3)`), and the
small one-off value caps (GreatestMV, NoMaxHandSize, Increment, LKI-counter-count, discarded-this-resolution). **Moseo is now
cheap** (dynamic-MV reanimate filter shipped — `ManaValueExpr` with a life-gained bound expr). The 5 Elder Dragons + 3 Natives
+ Fractalize + special one-offs still need lead-approved design sketches. Read the by-cap list + census buckets below.

*(sos-cards-15 still active — will rewrite this block fully on retirement. For now: additional-cast-cost cap done, ledger + census
current, pointing the tail at the next-highest-yield caps.)*

## ▶ Prior — handoff from sos-cards-14, 2026-07-05

**▶▶ sos-cards-14 HANDOFF — READ FIRST. SCOPE = FULL SET (215/271 authored); bar = general CR capability
("nicest way that extends for any future card").** **698 mtg-core tests green, whole workspace builds, tree clean,
LEAD pushes.** sos-cards-14 finished **the FINAL FIVE prepare stragglers (Jadzi, Harmonized Trio, Grave Researcher,
Leech Collector, Goblin Glasswright)** + **2 reusable engine subsystems** (queue-time trigger-condition check; the
option-B sac-for-mana Treasure) + the **honest Scryfall-diff FULL-SET CENSUS** (the "★ FINAL FULL-SET CENSUS"
section — read it; it corrects the stale ⏳ triage table and buckets the 56 remaining unauthored cards).

### ✅ SHIPPED by sos-cards-14 (commits `7a45fbf` Jadzi · `5345c20` Harmonized Trio · `f09c497` Grave Researcher · `88465ed` Leech Collector · `c7d067c` Goblin Glasswright; `git log -S` before re-scoping)
- **Reusable caps this session:** `CostComponent::TapCreatures(n)` (tap-N-others cost, Crew-modeled), `Effect::PutFromHandOnTop`
  (Brainstorm), `Effect::ReanimateUnderControl` + `ValueExpr::ManaValueOfTarget` + the **`move_object` control-vs-owner
  source-removal fix** (control≠owner now works — reanimate/steal), the **queue-time trigger-condition check** (helper
  `Engine::trigger_queues` on all 4 non-begin-of-step queue sites — a non-intervening-if `condition` now gates at event
  time; ZERO regression, Bucket B empty) + `Player.life_gain_events_this_turn` / `ValueExpr::LifeGainEventsThisTurn`, and the
  **option-B Treasure** (`Cost::is_simple_tap_mana`, auto-pay pool excludes cost-bearing mana abilities, manual activation
  pays them via `pay_cost` — see the ⚠️ TREASURE flag block).

### ▶ REMAINING = the tail (56 unauthored) — **triaged by cap so ONE cap unlocks SEVERAL cards** (sos-cards-14 pre-scoped this)
Every remaining buildable card needs a small NEW cap (the pure-existing-machinery cards are all harvested). Build the cap →
the bracketed cards fall out. Grouped by yield (verify oracle from sqlite; real-path test each):
- ~~**Additional-cast-cost (spell-level, CR 601.2f)** → Seize the Spoils, Vicious Rivalry, Fix What's Broken, Soaring
  Stoneglider.~~ ✅ **DONE (sos-cards-15)** — `AdditionalCost`/`Ability::AdditionalCost` + PayLife wiring + `ManaValueExpr`
  dynamic-MV filter. See the sos-cards-15 SHIPPED block at the top.
- **Grant-a-triggered-ability-until-EOT** → **Rabid Attack** (grant "when this dies, draw"), **Root Manipulation** (anthem +
  menace + attack-trigger). A continuous grant of a full `Ability::Triggered`.
- **Exile-and-return-at-next-end-step (timed blink, reuse `Effect::Blink` + a delayed return trigger)** → **Ennis, Debate
  Moderator**, **Conciliator's Duelist** (Repartee returns).
- ~~**Base-P/T-set until EOT (layer 7b)** → **Quandrix Charm**.~~ ✅ **DONE (sos-cards-15, `4b41def`)** — `Effect::SetBasePT`
  lowering to `GrantContinuous{SetBasePT}`. Reusable for any "has base P/T X/Y until EOT".
- ~~**Flashback with a NON-mana cost** → **Group Project**.~~ ✅ **DONE (sos-cards-15, `b7a1e51`)** — `Ability::Flashback`
  now carries a full `Cost`; reusable for any non-mana flashback/alternative cost.
- ~~**GreatestMV** (highest-mana-value among a set) → **End of the Hunt**.~~ ✅ **DONE (sos-cards-15, `cd1fbe2`)** —
  `ValueExpr::GreatestManaValue`. **NoMaxHandSize** (player static) → **Wisdom of Ages** (also needs mass-return-I/S + self-exile).
  **Increment** (mana-spent vs P/T self-counter) → **Tester of the Tangential**, **Ambitious Augmenter**. **LKI-counter-count**
  → **Scolding Administrator** (move the counters it died with). **Dynamic-MV reanimate filter** → **Moseo** (MV≤life-gained).
  **Discarded-this-resolution tracking** → **Mind Roots**, **Borrowed Knowledge**, and **Colossus of the Blood Age** (partial
  dies-clause). **Copy-a-spell (S14, exists)** consumers → **Choreographed Sparks, Lumaret's Favor, Aziza, Mica** (sac-artifact-copy).
- **⚠️ Do NOT start without a lead-approved design sketch:** the **5 Elder Dragons** (Prismari=Storm, Quandrix=Cascade,
  Lorehold=Miracle, Silverquill=Casualty, Witherbloom=Affinity — five genuine subsystems). **Deferred:** 3 Natives
  (Mathemagics/Pox Plague/Steal the Show), Fractalize (milestone-5 layers), the special-one-off legends/permanents (Grandeur/
  theft-cast/name-choice/free-cast/grant-mana/prepare-marker). See the census buckets.

**PROCESS (unchanged, hard-won):** shared tree → `git commit --only <paths>`; never `-a`/`add -A`/stash; DON'T touch
`experiments/`; `cargo test -p mtg-core` green at EVERY commit; flip a cap's ledger Status cell in the SAME commit; **`git
log -S "<mechanism>"` + READ THE CODE before scoping any ⏳ row as new (the ⏳ triage table is STALE — trust the census);**
real-path integration test for every mechanism; expect-test snapshots; ping the lead at subsystem boundaries + design-sketch
new subsystems before building; honest flags; keep the ledger + WORKLOG + PROJECT_STATE current. On fatigue: declare, rewrite
THIS block, hand off clean.

*(sos-cards-14 retiring at a clean boundary — the final five prepare stragglers + 2 subsystems shipped, honest census
delivered, tail pre-triaged by cap for the successor.)*

### ✅ SHIPPED by sos-cards-13 (all real-path tested; `git log -S` before re-scoping — beliefs drift)
- **StackObject counterspell real-cast targeting** — the "counterspells never work through the real cast path"
  gap's REAL root cause was `collect_specs_into` never matching `Effect::Counter`/`CounterUnlessPay` (spec silently
  dropped → no target → nothing countered). Fixed + `target_candidates` StackObject arm (spells only, excludes the
  caster's own spell-in-progress) + `target_matches_filter` `Target::Stack`→spell-card resolution. → **Brush Off**.
- **CR 707.10 copy-a-spell-ON-the-stack** (the copy that is NOT cast, distinct from 707.12 `CastCopy`):
  `copy_spell_on_stack(spell, by, choose_new_targets)` mints an `is_copy` copy over the original (carries its
  targets/X/modes, optional `rechoose_copy_targets`, NO SpellCast). Delivered via a one-shot delayed trigger:
  `Effect::CopyNextSpellCast` → `DelayedTriggerEvent::YouCastSpell{filter, choose_new_targets}` (expires unfired at
  next turn's start, fired from the SpellCast broadcast) → `StackObjectKind::SpellCopyTrigger`. → **Pigment Wrangler
  // Striking Palette**. **Reusable for Lumaret's Favor / Twincast-class** (add a thin `Effect::CopySpellOnStack{what}`
  delegating to `copy_spell_on_stack`).
- **`Effect::ExileTopUntilManaValueMayCastFree`** (exile-top-until-total-MV, then may-cast-any-number-free during
  resolution, CR 601.3e) → **Improvisation Capstone** (⇒ **Paradigm 5/5 Lessons**).
- **`Effect::Blink`** (CR 603.6e exile-then-return; ETB re-fires, counters/damage/summoning-sickness reset via
  `move_object`) → **Skycoach Conductor // All Aboard**.
- **The gain-before-exile stat trick** (NO LKI plumbing): for "remove X, then Y = X's OWN stat", sequence the
  value-reading effect BEFORE the removal so the stat reads live (`Sequence[GainLife{ControllerOfTarget(0),
  PowerOfTarget(0)}, Exile{target}]`). → **Emeritus of Truce // Swords to Plowshares** (front = target-player Inkling
  + conditional prepare). ⚠️ The genuine LKI-into-ValueExpr cap is only needed where the value depends on the removal
  having happened (no current card).
- **`Effect::MillThenPutCreatureOntoBattlefield`** (mill from your OWN library, reanimate a creature from among the
  milled set; owner==controller so no control override) → **Vastlands Scavenger // Bind to Life**.

### ▶ sos-cards-14 PROGRESS — 4 of the final 5 SHIPPED (695 mtg-core green), only Goblin Glasswright (③) remains (lead scope decision)
- ✅ **Grave Researcher // Reanimate** (commit `f09c497`, back id 9733) — SKETCH 1 built as a **dedicated `Effect::Reanimate
  UnderControl`** (NOT a widened MoveZone — 24 existing MoveZone sites, too much churn; mirrors the `MillThenPutCreature
  OntoBattlefield` precedent). + `ValueExpr::ManaValueOfTarget` (both eval paths) + the `move_object` control-vs-owner
  source-removal fix (battlefield/stack sources remove from the CONTROLLER's vec — a **no-op for every existing card**;
  full suite green). Real-path tests incl. steal-from-opp-gy + dies-to-owner's-gy.
- ✅ **Leech Collector // Bloodletting** (commit `88465ed`, back id 9734) — SKETCH 2 built. The **queue-time condition check**
  (helper `Engine::trigger_queues`, mirrors begin-of-step's `!intervening_if` gate) added to ALL 4 non-begin-of-step queue
  sites (`queue_self_triggers` + spellcast/enters/**you_attack** siblings). **ZERO regression confirmed** — 695 green, the 3
  Bucket-A cards (Emeritus of Conflict/Abundance, Living History; all intervening_if:true) explicitly re-verified passing.
  + `Player.life_gain_events_this_turn` (reset each turn; bumped in the `LifeChanged{delta>0}` handler BEFORE the queue loop)
  + `ValueExpr::LifeGainEventsThisTurn` (both paths). Front gate = exactly-1 (All/Not), `intervening_if:false`.

### ▶ ORIGINAL sketches (retained for the record)
- ✅ **Jadzi, Steward of Fate // Oracle's Gift** (commit `7a45fbf`, back id 9731) — **NO new cap.** {X}{X} = `ManaCost.x=2`
  (charges 2X; `cast_x`→`ValueExpr::X`). Back = `Sequence[CreateToken{fractal(0), count:X}, ForEach{Fractals you control →
  PutCounters{Each, +1/+1, X}}]` (the shipped Blech `ForEach{…max:999…}` selects ALL matching, so new + pre-existing
  Fractals both get counters). Front = enters-prepared + a 2nd `SelfEnters` trigger (draw 2, discard 2).
- ✅ **Harmonized Trio // Brainstorm** (commit `5345c20`, back id 9732) — **2 contained caps, NOT flagged.**
  `CostComponent::TapCreatures(u32)` (count-based sibling of Crew; reuses `crew_candidates`/select-N-and-tap) drives the
  front's "{T}, Tap two untapped creatures you control:" activated prepare. `Effect::PutFromHandOnTop{who,count}` (select
  N hand cards ordered → library top, first-chosen on top; `move_object` pushes to the tail=top) drives Brainstorm =
  `Sequence[Draw 3, PutFromHandOnTop 2]`.

---

## ⚠️ TREASURE / SAC-FOR-MANA — OPTION (B) SHIPPED, with a load-bearing agent-seat limit (lead-approved)

**Goblin Glasswright // Craft with Pride SHIPPED (commit `c7d067c`)** via the lead-approved **option (B) exclude-from-
autopay** Treasure model — `grp::TREASURE_TOKEN` (colourless artifact, `{T}, Sacrifice this: Add one mana of any color`),
`helpers::treasure_token()`, back = `CreateToken{treasure}`. Engine: `Cost::is_simple_tap_mana()`; the auto-pay source
enumeration (`mana::mana_sources_kind`, `include_cost_bearing:false`) **excludes cost-bearing mana abilities**, while the
manual path (`usable_mana_sources`, `true`) includes them; `Engine::activate_mana_ability` now pays a cost-bearing mana
ability through `pay_cost` (taps + **sacrifices**) then floats the mana; `create_token` gives non-creature tokens no P/T.

**🚩 AGENT/GYM-SEAT FLAG (must carry forward):** under (B), agent/replay seats run `manual_mana = false`, so they are
**never offered `ActivateMana`** — a Treasure is **inert in training** (a sacrificeable artifact that can never be spent for
mana by an auto-pay seat), and any spell affordable ONLY via a Treasure is **uncastable for the RL agent**. Accepted as a
first-pass limit. **Option (A)** (auto-spending non-tap mana sources — sac-for-mana, convoke-class, Phyrexian — as decisions
in one payment flow) is **recorded as part of the future transactional-pending-cast re-architecture (WHITEBOARD_MODEL §2.6,
the no-rewind→GRE-style evolution), NOT a standalone TODO.** The same lever also completes **Hydro-Channeler's 2nd ability**
(a `{1},{T}` mana-ability-with-mana-cost — same class) if/when (A) lands.

---

## ★ FINAL FULL-SET CENSUS (sos-cards-14, 2026-07-05) — Scryfall-diff verified, corrects the stale ⏳ triage

**Method:** diffed the real 271-card `sos` set (`data/scryfall/cards.sqlite`, `set_code='sos'`) against every authored card
name across `crates/mtg-core/src/cards/**` (DFC fronts matched on the pre-`//` name). This is ground truth — **the ledger's
per-card ⏳ triage table below is STALE** (dozens of ⏳ rows are actually shipped: Pull from the Grave, Aberrant Manawurm,
Brush Off, Antiquities on the Loose, Stun/Look-and-pick/Graveyard-activated subsystems, …). Trust code + this diff, not the table.

**Headline (Scryfall-diff RE-VERIFIED 2026-07-05 by sos-cards-15): 222 / 271 authored (82%). 218 fully faithful · 4
tracked-partial · 49 unauthored. 0 Native escape hatches used. 720 mtg-core tests green.** (Diff method: every sos set name —
front face, pre-`//` — checked against string literals in `crates/mtg-core/src/cards/**`; the 49 unauthored match the buckets
below exactly. sos-cards-15 added Seize the Spoils, Vicious Rivalry, Fix What's Broken, Soaring Stoneglider, Quandrix Charm,
End of the Hunt, Group Project.) (Goblin Glasswright shipped since the first census; Seize the Spoils remains — it
needs an ADDITIONAL-CAST-COST cap "as an additional cost, discard a card", NOT just the Treasure.) (215 counts sos-set cards
covered by a def in ANY set folder — 200 are sos-first-printed modules;
~15 are reprints whose defs live in their first-printing folders.)

### The 4 TRACKED-PARTIAL cards (authored, one documented clause each deferred)
1. **Ral Zarek, Guest Lecturer** (`ral_zarek_guest_lecturer.rs`, id 365) — +1/−1/−2 fully faithful; **−7 ultimate deferred**
   (coin-flip randomness primitive + skip-turns tracker — neither in the core). The shortlist "coin-flip+skip-turns" item.
2. **Wildgrowth Archaic** (`wildgrowth_archaic.rs`, id 308) — mono-hybrid cost + Trample/Reach + Converge self-enter done;
   **"whenever you cast a creature spell, THAT creature enters with X extra +1/+1 counters" deferred** (needs a delayed
   enters-with-counters replacement keyed to another spell still on the stack — unbuilt).
3. **Hydro-Channeler** (`hydro_channeler.rs`, id 321) — 1st ability (`{T}: Add {U}`, I/S-restricted) done; **2nd ability
   `{1},{T}: Add any color (restricted)` deferred** — a mana ability with a *mana activation cost*; the auto-pay source model
   treats sources as free-to-tap. **Same class as the Treasure sac-for-mana (③) — a general fix there unlocks this too.**
4. **Colossus of the Blood Age** (`colossus_of_the_blood_age.rs`, id 314) — ETB (3 dmg each opp + gain 3) done; **dies clause
   "discard any number, then draw that many PLUS ONE" deferred** (needs a "cards discarded this resolution" value — unbuilt).

### The (now 56) UNAUTHORED cards (bucketed honestly — not all are "deferred by design")
- ✅ **Goblin Glasswright — SHIPPED** (was ③; commit `c7d067c`). **Seize the Spoils** now the nearest Treasure-adjacent card,
  but it needs an **additional-cast-cost cap** ("as an additional cost, discard a card") — a new spell-level cast-cost
  subsystem (CR 601.2f), NOT just the Treasure. Deferred until that cap is built (no card uses additional cast costs yet).
- **Design-deferred major subsystems (~16):** the **5 college Elder Dragons** — Lorehold the Historian (Miracle), Prismari
  the Inspiration (Storm), Quandrix the Proof (Cascade), Silverquill the Disputant (Casualty), Witherbloom the Balancer
  (Affinity); **3 Natives** (never authored, no Native hack used) — Mathemagics (2^X), Pox Plague (halving), Steal the Show
  (wheel/theft); **Fractalize** (milestone-5 SET-type + base-P/T layers); **Grandeur** — Page, Loose Leaf; **theft/ownership-
  cast** — Nita, Forum Conciliator; **name-choice static** — Petrified Hamlet; **once/turn free-cast** — Zaffai and the
  Tempests; **grant-mana-ability** — Resonating Lute; **non-DFC prepare markers** — Biblioplex Tomekeeper, Skycoach Waypoint.
- **Treasure-blocked (unlocked by ③'s work) (1):** Seize the Spoils (create two Treasure tokens).
- **Cap-blocked / buildable-but-not-yet-reached (~39):** modal/one-off spells & creatures whose caps are unbuilt or which
  were simply not reached — e.g. Quandrix Charm (modal), Ambitious Augmenter (Increment), Archaic's Agony (excess-damage +
  multi-card impulse-exile), Ark of Hunger (graveyard impulse-play), Sundering Archaic, Rubble Rouser (mana-ability-with-
  damage), Mind Roots / Mind into Matter (put-permanent-into-play), Moment of Reckoning (modal ×4), Divergent Equation
  (X-return I/S), Choreographed Sparks / Lumaret's Favor / Aziza (spell-copy consumers — the S14 copy subsystem exists),
  Tablet of Discovery (S13), and combat tricks/spells (Rabid Attack, Vicious Rivalry, Social Snub, Practiced Offense, End of
  the Hunt, Fix What's Broken, Daydream, Flashback, Mana Sculpt, Root Manipulation, Wisdom of Ages, Group Project, Borrowed
  Knowledge, Burrog Barrage, Zimone's Experiment, …) + creatures (Conciliator's Duelist, Scolding Administrator, Snooping
  Page, Slumbering Trudge, Soaring Stoneglider, Mica/Moseo/Ennis/Tester legends) + lands (Great Hall of the Biblioplex).

**Honest bottom line:** the **prepare sub-track is complete** (the "final five" — Jadzi, Harmonized Trio, Grave Researcher,
Leech Collector shipped; Goblin Glasswright ③ pending). The **full set is 79% authored**; the remaining 57 are ~16 design-
deferred (Elder Dragons / Natives / layers / special one-offs) + ~40 cap-blocked-or-not-yet-reached buildable cards + ③. The
set is NOT "complete except a tiny shortlist" — that framing tracked only the prepare sub-track, not the whole 271.

---

### ▶ REMAINING for sos-cards-14: **Goblin Glasswright // Craft with Pride** (③) — awaiting lead's scope pick (A/B/C below)
- ✅ **Jadzi, Steward of Fate // Oracle's Gift** (commit `7a45fbf`, back id 9731) — **NO new cap.** {X}{X} = `ManaCost.x=2`
  (charges 2X; `cast_x`→`ValueExpr::X`). Back = `Sequence[CreateToken{fractal(0), count:X}, ForEach{Fractals you control →
  PutCounters{Each, +1/+1, X}}]` (the shipped Blech `ForEach{…max:999…}` selects ALL matching, so new + pre-existing
  Fractals both get counters). Front = enters-prepared + a 2nd `SelfEnters` trigger (draw 2, discard 2).
- ✅ **Harmonized Trio // Brainstorm** (commit `5345c20`, back id 9732) — **2 contained caps, NOT flagged.**
  `CostComponent::TapCreatures(u32)` (count-based sibling of Crew; reuses `crew_candidates`/select-N-and-tap) drives the
  front's "{T}, Tap two untapped creatures you control:" activated prepare. `Effect::PutFromHandOnTop{who,count}` (select
  N hand cards ordered → library top, first-chosen on top; `move_object` pushes to the tail=top) drives Brainstorm =
  `Sequence[Draw 3, PutFromHandOnTop 2]`.

### ▶ REMAINING for sos-cards-14: the **3 flagged** stragglers — DESIGN SKETCHES below (each own-commit), pinged to lead

**SKETCH 1 — Grave Researcher // Reanimate (reanimate-controller-override + `ManaValueOfTarget`; LOW regression risk).**
Control model VERIFIED (Explore): battlefield is a per-player `Vec` keyed such that an on-bf object sits in its
**controller**'s vec (move_object pushes to `to_owner` and sets `controller=to_owner`); `Object` has distinct
`owner`/`controller`; "creatures you control" counts `o.controller==p`. The gap: `move_object`'s **source removal**
(state/mod.rs:745) removes from `o.owner`'s vec — fine today (owner==controller everywhere) but wrong once control≠owner.
Plan: (a) add `ValueExpr::ManaValueOfTarget(u32)` (whiteboard Path A arm next to `PowerOfTarget`; conditions.rs Path B
falls through to 0, add for parity). (b) `Effect::MoveZone` gains `controller: Option<PlayerRef>` (None→owner); lowered to
`Action::MoveZone{new_controller: Option<PlayerId>}`; commit handler passes `new_controller.unwrap_or(owner)` as `to_owner`.
(c) fix `move_object` source removal: for a **battlefield** source remove from `o.controller`'s vec, else `o.owner`'s — a
**no-op for all existing behavior** (owner==controller), correct once a reanimated opp creature later leaves play. Card:
back Reanimate = `Sequence[MoveZone{Target(CardInZone{Graveyard,Creature}), Battlefield, controller:Some(Controller)},
LoseLife{Controller, ManaValueOfTarget(0)}]`; front = `BeginningOfStep(Upkeep)`+`YourTurn` → `Sequence[Surveil 1,
Conditional{ValueAtLeast(Count{gy creatures, Controller}, 3) → BecomePrepared}]` + Prepare. Guard = full suite (move_object
change is inert for every current card).

**SKETCH 2 — Leech Collector // Bloodletting (queue-time trigger-condition check; ZERO regression, own commit).**
Regression survey (Explore, exhaustive): **Bucket B is EMPTY** — no non-begin-of-step `Triggered` in the pool sets
`condition:Some + intervening_if:false`. The only 3 conditioned non-begin-of-step triggers (Emeritus of Abundance
SelfAttacks, Emeritus of Conflict SpellCast, Living History YouAttack) are all `intervening_if:true`. Plan mirrors
`queue_begin_of_step_triggers` EXACTLY — gate the condition at queue time **only when `!intervening_if`** — so those 3 are
untouched (they still defer to `trigger_intervening_if_holds` at resolution). Purely enabling. Apply to `queue_self_triggers`
(covers Leech's GainLife) + for generality the siblings `queue_watching_spellcast_triggers` / `queue_watching_enters_triggers`
/ **`queue_you_attack_triggers`** (the 4th sibling the survey flagged). Plus: `Player.life_gain_events_this_turn: u32`
(reset each turn beside `life_gained_this_turn`; **increment by 1 in the `LifeChanged{delta>0}` handler BEFORE the
GainLife queue loop** so the 1st gain reads ==1) + `ValueExpr::LifeGainEventsThisTurn{who}` (both eval paths). Card: front =
`prepared_abilities(BLOODLETTING, GainLife, Some(exactly-1), intervening_if:false)` where exactly-1 =
`All(ValueAtLeast(LGEtt,1), Not(ValueAtLeast(LGEtt,2)))`; back Bloodletting = `LoseLife{EachOpponent, 2}`.

**SKETCH 3 — Goblin Glasswright // Craft with Pride (Treasure sac-for-mana; HARDEST — needs a SCOPE decision).**
Explore confirms the real wall: `is_mana:true` abilities **bypass `pay_cost`** — affordability (`payment_units`/
`mana_sources_kind`) counts any untapped `AddMana` source **ignoring its cost.components**, and payment (`mana.rs::auto_pay`)
only flips `status.tapped` (no Engine access → can't `move_object`/broadcast a sacrifice). So a naively-registered Treasure
would tap for mana but **never sacrifice** = a reusable mana rock (a real gameplay bug, not cosmetic). The token DEF itself
is trivial (Potioner's Trove + `CostComponent::Sacrifice(sacrifice_self())`, "any color" = `ManaSpec{any_color:Some(1)}`).
Options (lead's call):
  - **(A) FULL** — carry each mana-source's non-tap cost through an Engine-level payment (route `is_mana` abilities with
    extra components through `pay_cost`/`pay_sacrifice`). Correct + general, but re-architects the core mana affordability/
    payment path that EVERY cast exercises → biggest/riskiest change of the three.
  - **(B) EXCLUDE-FROM-AUTOPAY (my recommendation)** — exclude sac-cost mana sources from `auto_pay`/affordability
    enumeration; the Treasure is usable only via MANUAL mana-ability activation (`activate_mana_ability`), which I route
    through `pay_cost` so it sacrifices correctly and floats the mana (then the cast spends floating mana). CR-correct
    (mana abilities may be activated in the priority window), localized to the source enumeration, no auto_pay rewrite —
    but the auto-payer won't spend Treasures (the AI must manually pop them, a legal action).
  - **(C) DEFER** — ship "Create a Treasure token" but track sac-for-mana as a known engine gap (Treasure taps, never
    sacs). Honest, smallest, but leaves a genuine gameplay bug (infinite mana over turns).

---

### ✅ Prior — sos-cards-11 SHIPPED (superseded header; detail retained below)

**sos-cards-11** built **the long-deferred SPELL-COPY subsystem** and its consumers (630 green then).

### ✅ SHIPPED (all real-path tested; `git log -S` before re-scoping — beliefs drift)
- **SPELL-COPY (CR 707.10/12) — the reusable foundation.** `CastVariant::WithoutPayingManaCost`→{0}
  (free-cast primitive); **`Effect::CastCopy{source, controller}`** mints a copy `Object` from the source's
  copiable base chars (707.2 via grp_id) into `Zone::Stack`, casts it through the EXISTING `cast_spell`
  (new targets, X=0, SpellCast fires); **`Object.is_copy`** → the copy **ceases to exist** off the stack
  (707.10a, `state.cease_to_exist`, in `resolve_top` + `interpret_counter`, checked BEFORE the flashback/
  paradigm exile branch). Key realization: *a spell on the stack is just an Object → a copy needs almost no
  new machinery.* WHITEBOARD_MODEL §2.5 updated.
- **`Effect::CastForFree{what, exile_on_leave}`** — casts the ACTUAL targeted card free (vs CastCopy's copy);
  `exile_on_leave` reuses the flashback exile-on-leave-stack flag. → **The Dawning Archaic** ({1}-less-per-I/S
  reduction arm now exercised; SelfAttacks → free-cast up-to-one gy I/S + exile rider).
- **Paradigm (SoS Lessons keyword — NOT Learn/sideboard).** `Ability::Paradigm` (self-exile-on-resolve marker;
  `resolve_top` routes the original to exile) + **`queue_exile_functioning_triggers`** (mirrors the emblem/
  graveyard `FunctionsFrom` scans, fired from `PhaseBegan` gated to the active player) + a recurring
  `BeginningOfStep(PrecombatMain)` optional `CastCopy{SourceSelf}`. `helpers::paradigm_abilities()` bundles
  all three for the 5 Lessons. **4/5 Lessons DONE:** Decorum Dissertation (carries the full lifecycle test),
  Germination Practicum, Restoration Seminar (reanimate), Echocasting Symposium (token-copy).
- **`Effect::PutOnTopOrBottom`** (owner chooses top/bottom of library, `ConfirmKind::PutOnTop`) → **Run Behind**
  (+ S12 target-dependent reduction, `TargetMatches(Attacking)`).

### ✅ sos-cards-12 PROGRESS (2026-07-05)
- **PREPARE-DFC RAILS + 4 representative cards SHIPPED** (commit `bfd3d51`; 172→176 authored, 630→638 mtg-core
  green). Built exactly the approved design (spell-copy CONSUMER, no CR 711 transform): **`Object.prepared`**
  flag + **`Effect::BecomePrepared`** (lowers to **`Action::SetPrepared`**; every "becomes prepared" clause is an
  ordinary trigger/ability — zero new trigger machinery) + **`Ability::Prepare{spell}`** (front→back link) +
  back-face spell defs in the reserved **9700+ grp block** (`grp::PREPARE_BACK_BLOCK`, excluded from `/api/cards`)
  + **`PlayableAction::CastPrepared{source}`** offered in `legal_priority_actions` at the back face's timing,
  executed by **`Engine::cast_prepared`** (mints an `is_copy` copy from the back-face def, `cast_spell(Normal)`
  **pays** the back cost, unprepares the source; copy ceases to exist off the stack, CR 707.10a). **DESIGN NOTE:**
  I did NOT widen `Effect::CastCopy` — the prepared cast is a *priority action*, not an effect resolution, so a
  dedicated `cast_prepared` calling `cast_spell` directly is cleaner than a paid/def-source flag on the effect
  (Paradigm's free `CastCopy` stays untouched — its free-path test is unchanged & green). Affordability masking is
  exact: `effective_cast_cost` reads only the cast card's OWN reductions and back faces have none, so the offer's
  printed-cost check == what `cast_spell` charges (no drift). 4 cards, each oracle-verified + real-path tested:
  **Adventurous Eater // Have a Bite** (enters-prepared — the flagship full-lifecycle test), **Scathing Shadelock
  // Venomous Words** (at-first-main, `YourTurn`-gated), **Encouraging Aviator // Jump** (on-attack + a re-prepare
  loop; instant back → instant-speed offer), **Lluwen // Pest Friend** (an ACTIVATED prepare source — exile-a-
  creature-from-gy cost — + enters-prepared; back = Pest token).
- **PREPARE FAN-OUT: 27 of ~36 SHIPPED** (662 mtg-core green). Helper **`helpers::enters_prepared` /
  `prepared_abilities`** (Prepare marker + a becomes-prepared trigger) — every card is 2 defs (front creature +
  back spell, ids 377+/9704+). **Design proved out: every "becomes prepared" variant is just `Effect::BecomePrepared`
  on an existing trigger — zero new trigger machinery.** Value/effect caps added along the way (all general, both
  eval paths where relevant): `ValueExpr::LifeGainedThisTurn{who}`, `CreaturesDiedThisTurn`, `HandSize{who}`,
  `SpellsCastThisTurn{who}` (+ `Player.spells_cast_this_turn` counter), and `Effect::MayTapOrUntap`.
  Shipped: Adventurous Eater//Have a Bite, Scathing Shadelock//Venomous Words, Encouraging Aviator//Jump,
  Lluwen//Pest Friend, Studious First-Year//Rampant Growth, Landscape Painter//Vibrant Idea, Blazing
  Firesinger//Seething Song, Honorbound Page//Forum's Favor, Quill-Blade Laureate//Twofold Intent, Strife
  Scholar//Awaken the Ages, Campus Composer//Aqueous Aria, Cheerful Osteomancer//Raise Dead, Spellbook
  Seeker//Careful Study, Maelstrom Artisan//Rocket Volley, Tam//Deep Sight (landfall), Abigale//Heroic Stanza
  (cast-a-creature), Kirol//Pack a Punch (cards-leave-gy), Spiritcall//Scrollboost (tokens-enter), Sanar//Wild
  Idea, Emeritus of Abundance//Regrowth (attack+lands≥8), Emeritus of Ideation//Ancestral Recall
  (attack+MayPayCost exile-8), Scheming Silvertongue//Sign in Blood (2nd-main+life≥2), Emeritus of Woe//Demonic
  Tutor (end-step+died≥2), Infirmary Healer//Stream of Life ({X}-spell), Elite Interceptor//Rejoinder
  (MayTapOrUntap), Joined Researchers//Secret Rendezvous (hand-compare), Emeritus of Conflict//Lightning Bolt
  (3rd-spell).
  ⚠️ **TRIGGER-CONDITION GOTCHA (found + used, applies to future cards):** `queue_self_triggers` and
  `queue_watching_spellcast_triggers`/`queue_watching_enters_triggers` do **NOT** check a trigger's `condition`
  at queue time — only `queue_begin_of_step_triggers` does. So a condition on a Self*/SpellCast/PermanentEnters
  trigger MUST use **`intervening_if: true`** (enforced at resolution via `trigger_intervening_if_holds`); with
  `intervening_if: false` the condition is silently IGNORED. (BeginningOfStep triggers may use `false` — checked
  at queue.) Emeritus of Conflict's gate was initially `false` → fixed to `true` + a real 3-cast integration test.

- **▶ REMAINING PREPARE: 5 cards (was 9; #3 Emeritus of Truce, #5 Vastlands Scavenger, #6 Skycoach, #9 Pigment Wrangler DONE by sos-cards-13) — each blocked on a distinct BACK-FACE (or activation-cost) cap, NOT prepare.**
  The prepare front/trigger for every one is trivial (`Effect::BecomePrepared`); what's unbuilt is the back
  effect / front cost. Precise blockers (build the cap → the card is mechanical; back ids continue from 9727):
  1. **Leech Collector // Bloodletting** — front "gain life for the FIRST time each turn": needs a
     `Player.life_gain_events_this_turn` counter **AND** queue-time condition-checking added to `queue_self_triggers`
     (mirroring `queue_begin_of_step_triggers`) so a `GainLife` trigger can gate on "events==1" AT event time — an
     intervening-if (resolution) check fails when two gains batch before the trigger resolves. Back = each opponent
     loses 2 (`LoseLife` EachOpponent, built). ⚠️ The queue-time change touches all self-triggers → own commit + regression.
  2. **Grave Researcher // Reanimate** — front is BUILDABLE NOW (`Sequence[Surveil 1, Conditional{CountAtLeast(gy
     creatures≥3) → BecomePrepared}]`, all pieces exist; upkeep trigger + YourTurn). Back needs a
     `ValueExpr::ManaValueOfTarget` (lose life = the reanimated card's MV) **and** a MoveZone controller-override
     (reanimate a creature from ANY graveyard to the battlefield *under your control* — Forum Necroscribe only does
     your-own-gy where owner==you, so cross-gy steal needs `Action::MoveZone` to carry a controller).
  3. ~~**Emeritus of Truce // Swords to Plowshares**~~ ✅ **DONE (sos-cards-13)** — front ETB = target-player Inkling
     + `Conditional{ ValueAtLeast(Count{opp creatures}, Sum(Count{your creatures}, 1)) → BecomePrepared }` (all
     pieces existed). Back Swords: **no LKI cap needed** — sequence the life gain BEFORE the exile (`Sequence[GainLife{
     ControllerOfTarget(0), PowerOfTarget(0)}, Exile{target}]`) so "its power" reads the live creature (identical
     value, since the same resolution then removes it) and `ControllerOfTarget` reads the resolution-start snapshot.
     **General trick for "remove X, then Y = X's own stat": read the stat before the removal — no LKI plumbing.**
     Back id 9729. (The genuine LKI-into-ValueExpr cap is still only needed where the value depends on the removal
     having happened, which no current card requires.)
  4. **Jadzi, Steward of Fate // Oracle's Gift** — back `{X}{X}` create X Fractals then X counters on each Fractal
     you control: dynamic-X token count + a for-each-Fractal counter pass. Heaviest back.
  5. ~~**Vastlands Scavenger // Bind to Life**~~ ✅ **DONE (sos-cards-13)** — back = `Effect::MillThenPutCreatureOnto
     Battlefield { who, count }`: mill N from your OWN library (captures the milled set), then a mandatory pick of a
     creature card from among them → battlefield (yours, owner==controller, so NO control override). Front = 4/4
     Deathtouch (back id 9730). Real-path test: mill 7 (a Bears among 6 Forests) → the Bears is reanimated.
  6. ~~**Skycoach Conductor // All Aboard**~~ ✅ **DONE (sos-cards-13)** — back blink built as the reusable
     `Effect::Blink { what }` (CR 603.6e): exile the target then return it as a NEW object (ETB re-fires, counters/
     damage/auras/summoning-sickness reset via `move_object`, CR 400.7). Front = 2/3 Flash/Flying/vigilance (back
     id 9728). Real-path test: blink an Elvish Visionary → its ETB "draw" re-fires, counter+damage cleared, sick.
  7. **Goblin Glasswright // Craft with Pride** — back "create a Treasure token": a Treasure token def whose ability
     is a **sacrifice-cost mana ability** (flagged since sos-cards-7 — the mana payment path only taps, no sac-for-mana).
  8. **Harmonized Trio // Brainstorm** — front cost "{T}, Tap two untapped creatures you control" (a convoke-like
     tap-N-others cost, unbuilt) + back Brainstorm's "put two on top in any order" (library-order primitive).
  9. ~~**Pigment Wrangler // Striking Palette**~~ ✅ **DONE (sos-cards-13)** — back "when you next cast an I/S this
     turn, copy that spell (new targets)" built as the CR 707.10 copy-a-spell-on-the-stack subsystem (see S14 row):
     `Effect::CopyNextSpellCast` → `DelayedTriggerEvent::YouCastSpell` → `StackObjectKind::SpellCopyTrigger` →
     `copy_spell_on_stack` (mint+push over the original, NOT cast; optional new-target reselection).

### ▶ REMAINING for YOU (sos-cards-12) — ✅ ALL THREE DONE by sos-cards-13 (the StackObject cluster)
1. ✅ **Improvisation Capstone DONE (sos-cards-13)** — the 5th Lesson (⇒ **Paradigm now 5/5 Lessons**).
   `Effect::ExileTopUntilManaValueMayCastFree { who, total_mana_value }` (imperative): exile from the top one card
   at a time until the exiled cards' total MV ≥ threshold, then loop offering the controller to cast any number of
   the exiled NONLAND cards for free (real `cast_spell(WithoutPayingManaCost)` during resolution, CR 601.3e —
   `SelectCards(min:0,max:1)` per pick, stack-order-preserving; uncast cards + lands stay exiled). + Paradigm.
2. ✅ **Brush Off DONE (sos-cards-13)** — see the SHIPPED block + S12 row. Real counterspell cast-path (the
   StackObject-enumeration gap was really `collect_specs_into` dropping `Effect::Counter`'s spec).
3. **PREPARE-DFCs — RAILS + 24 of ~36 SHIPPED (see the sos-cards-12 PROGRESS block above).** The 12 remaining are
   each blocked on a distinct **back-face-effect (or activation-cost) cap, NOT prepare** — the precise
   per-card blocker list is in that PROGRESS block (build the cap → the card is mechanical: front creature with
   `helpers::enters_prepared`/`prepared_abilities` + a back spell def at 9724+). Cheapest next: Elite Interceptor
   (a tap-or-untap leaf), Grave Researcher (front buildable now; back needs `ManaValueOfTarget` + reanimate-to-bf).

**PROCESS (unchanged, hard-won):** shared tree → `git commit --only <paths>` (`git add` a NEW file first),
never `-a`/`add -A`/stash; DON'T touch `experiments/` (MuZero + GPU); `cargo test -p mtg-core` green at every
commit; flip a cap's ledger Status cell in the SAME commit; **`git log -S "<mechanism>"` + READ THE CODE before
scoping any ⏳ row as new**. Real-path integration test for every mechanism; expect-test snapshots. Ping the lead
at subsystem boundaries + design-sketch new subsystems (prepare-DFCs sketched — build once the lead OKs) before
building. On fatigue: declare, rewrite THIS block, hand off clean. Read the **Systemic notes** (no-rewind economy
+ the counterspell/StackObject gap) below before scoping cost/targeting/counterspell work.

*(sos-cards-11 retiring at a clean boundary — spell-copy subsystem + Paradigm + 6 cards shipped, all green,
prepare-DFC design delivered, this block rewritten for the successor.)*

---
## ▶ Prior handoff — sos-cards-10 (superseded by the block above)

## ▶ NEXT AGENT — (handoff from sos-cards-10, 2026-07-04)

**▶▶ sos-cards-10 HANDOFF (2026-07-04) — READ FIRST. SCOPE = FULL SET; quality bar = general CR capability
("nicest way that extends for any future card"), not the minimal hack.** **166 authored / 616 mtg-core tests
green, tree clean, LEAD pushes.** sos-cards-10 shipped **3 subsystems + 3 cards + 2 dead-path revivals**
(full detail in "Prior handoff — sos-cards-10" below): **planeswalkers** (verify-and-finish; the loyalty
groundwork was already built — 4 primitives incl. `PlayerRef::Each` + a fail-closed `CardFilter::ManaValue`
targeting fix), **emblems / `Zone::Command`** (CR 114 — Dellian −6 → **Dellian fully faithful**), and the
**floating delayed-replacement subsystem** (CR 614 — `GameState.floating_replacements`, `Effect::ExileIfWouldDie`,
"dies" = any battlefield→graveyard move; **revived the dead `WouldBeDestroyed`/`WouldDie` static path** and
**routed SBA-death + sacrifice through the replacement pass** — both had bypassed it via direct `move_object`)
→ **Wilt in the Heat**. Ral Zarek is the one tracked-partial (−7 coin-flip+skip-turns deferred indefinitely).

### ▶ Sketches & plans for YOU (sos-cards-11) — design-sketch to the lead before building any subsystem

**⚠️ TWO READ-THE-CODE CORRECTIONS from sos-cards-10 (so you scope right — beliefs drift in this ledger):**
- **Wildgrowth Archaic is NOT free** off the floating-replacement cap. Its deferred clause ("whenever you cast a
  creature spell, THAT creature enters with X additional +1/+1 counters") is a *delayed enters-with-counters* on a
  future object — a **modest extension on the FloatingReplacement rails**: add a `FloatingRewrite::EntersWithCounters`
  variant + match `ActionPattern::WouldEnterBattlefield` for floating riders (currently only `WouldDie` is matched for
  floaters). Not a freebie, but the container + pass already exist — small follow-on.
- **The Dawning Archaic's exile rider rides FLASHBACK, not my cap.** "If that spell would be put into your graveyard,
  exile it instead" is a **spell leaving the STACK** (stack→graveyard, CR 608.2n), not a creature dying (battlefield→
  graveyard). `Effect::ExileIfWouldDie` = battlefield→graveyard only. But the flashback machinery already exiles a
  spell as it leaves the stack (`Object.flashback_cast` → exile-on-leave-stack in `resolve_top`) — so set that flag
  on the free-cast card and the rider is free THAT way.

**A. THE DAWNING ARCHAIC** ({10} Legendary Avatar 7/7, Reach — ⏳ ~1 moderate cap): cost reduction ({1} per I/S in
your gy) is **DONE** (`GenericValue(Count{I/S in gy})`, built + now exercise it). Reach = done. Remaining: a
**`SelfAttacks` trigger → "you may cast target I/S card from your graveyard without paying its mana cost"** (a
free-cast of a DIFFERENT graveyard card — like flashback's cast-from-gy but granted by another permanent; target a
gy I/S card, cast free, set the `flashback_cast`/exile-on-leave-stack flag so the "exile instead of graveyard" rider
comes along). Reuses: `EventPattern::SelfAttacks`, `TargetKind::CardInZone{Graveyard}`, the flashback cast+exile
path. The genuinely-new bit is "cast target [gy] card for free" as a granted one-off (vs the card's own flashback).

**B. PARADIGM — the SOS "Lessons" mechanic (NOT real-Strixhaven "Learn").** ⚠️ **READ-THE-CODE: the lead's brief
called this "Lessons/Learn (CR 715 outside-the-game / sideboard-pool)" — that's real Strixhaven and DOES NOT apply
here.** This set has **NO "Learn" cards, no sideboard/outside-the-game mechanic** (verified vs sqlite). The 5
`Sorcery — Lesson` cards — **Decorum Dissertation** {3}{B}{B} (target player draws 2, loses 2), **Germination
Practicum** {3}{G}{G} (two +1/+1 on each creature you control), **Restoration Seminar** {5}{W}{W} (reanimate a
nonland permanent card from your gy), **Echocasting Symposium** {4}{U}{U} (target player makes a token copy of
target creature you control), **Improvisation Capstone** {5}{R}{R} (exile top until MV≥4, cast any # free) — all
carry **Paradigm**: *"Then exile this spell. After you first resolve a spell with this name, you may cast a copy of
it from exile without paying its mana cost at the beginning of each of your first main phases."* Paradigm = **3
engine pieces** (the middle one is the big subsystem — design-sketch before building):
  1. **Self-exile-on-resolve** — the Lesson exiles ITSELF on resolve (not to graveyard) + records a "Paradigm recast"
     marker on the exiled object. Distinct from impulse-play (`castable_from_exile` casts the CARD once); Paradigm
     keeps the card in exile permanently and casts COPIES. Adapt the flashback exile-on-leave-stack + impulse
     `castable_from_exile` machinery.
  2. **A recurring optional free-cast trigger from exile** — "at the beginning of each of your first main phases, you
     may cast a copy" = `EventPattern::BeginningOfStep(Phase::PrecombatMain)` (gated to your turn), OPTIONAL, anchored
     to the exiled object. **Composes with the emblem precedent**: `Ability::FunctionsFrom(vec![Zone::Exile])` + a
     `queue_*_functioning_triggers` exile-zone scan (mirror the `Zone::Command` one I built for emblems).
  3. **SPELL-COPY (CR 707.12 "cast a copy") — THE BIG UNBUILT PIECE.** "cast a copy of it from exile" mints a
     StackObject copy of the Lesson on the stack (copiable characteristics from the card), lets you choose new
     targets, casts it free. This is the ledger's long-deferred **spell-copy subsystem** (real StackObject-copy +
     new-target reselection, CR 707.10/12). **Build spell-copy FIRST — it's the reusable foundation** (also unblocks
     the set's other spell-copy cards AND overlaps The Dawning Archaic's free-cast-from-a-nonhand-zone), then
     Paradigm = spell-copy + self-exile + the recurring trigger. The 5 Lessons' underlying effects range easy
     (Decorum/Germination) → moderate (Restoration reanimate, Echocasting token-copy [`CreateTokenCopy` mostly
     built]) → heaviest (Improvisation's impulse-cast-multiple).

**C. Remaining S12 cost-reduction cards** (mechanism done): **Run Behind** (needs "put target on top OR bottom of
owner's library, owner chooses" — a small owner-side binary decision), **Brush Off** (needs `TargetKind::StackObject`
real-path candidate enumeration — the counterspell gap in the Systemic notes below; its own commit w/ real
counterspell cast-path tests, per the lead — it's been latent too long). **Wildgrowth Archaic** = the modest
`FloatingRewrite::EntersWithCounters` extension above.

**PROCESS (unchanged, hard-won):** shared tree → `git commit --only <paths>` (stage a NEW file with `git add`
first), never `-a`/`add -A`/stash; DON'T touch `experiments/` (MuZero + GPU runs live there); `cargo test -p
mtg-core` green at every commit; flip a cap's ledger Status cell in the SAME commit; **`git log -S "<mechanism>"`
+ READ THE CODE before scoping any ⏳ row as new** (three sos-cards-10 corrections above prove beliefs drift BOTH
ways). Real-path integration test for every mechanism; expect-test snapshots. Ping the lead at subsystem boundaries
+ design-sketch new subsystems (spell-copy / Paradigm) before building. On fatigue: declare, rewrite THIS block,
hand off clean. **Read the Systemic notes (no-rewind economy + the counterspell/StackObject gap) below before
scoping cost/targeting/counterspell work.**

*(sos-cards-10 retiring here at a clean boundary — 3 subsystems + 3 cards shipped, tree clean, all green, this
block rewritten for the successor.)*

---
### ▶ Prior handoff — sos-cards-10 (full detail; superseded by the block above)

**▶▶ sos-cards-10 HANDOFF (2026-07-04) — full detail.** 163→166 authored / **616 mtg-core tests green, tree clean** (LEAD pushes).
**PLANESWALKERS DONE** + **EMBLEMS (CR 114 / Zone::Command) DONE** + **FLOATING DELAYED-REPLACEMENTS (CR 614)
DONE** (all lead-greenlit) → **Professor Dellian Fel FULLY FAITHFUL** + **Wilt in the Heat** shipped (only Ral
stays tracked-partial: its −7 coin-flip+skip-turns is deferred indefinitely). Shipped **3 cards + 5 reusable
primitives + 2 subsystems**, each with real-path tests, `git commit --only` on the shared tree:

**FLOATING DELAYED-REPLACEMENTS subsystem (CR 614, commit after dc5f5da) — the "known gap" (cards/mod.rs:156)
is now FILLED.** `GameState.floating_replacements: Vec<FloatingReplacement>` (general container: `scope` +
`pattern: ActionPattern` + `rewrite: FloatingRewrite` (serde-safe subset of `Rewrite`) + `until_turn` +
`one_shot`), consulted by the SAME rewrite pass as printed statics (CR 616.1f ChooseReplacement ordering
preserved — tested). `Effect::ExileIfWouldDie` registers "if [it] would die this turn, exile it instead".
**"Dies" = ANY battlefield→graveyard move (CR 700.4)** — `ActionPattern::WouldDie` + `Rewrite::ExileInstead`
cover destruction, sacrifice, and (future) legend-rule; `affected_object` extended to death actions.
⚠️ **Load-bearing fix:** SBA creature-death AND `interpret_sacrifice` took a **direct `move_object`** that
bypassed the replacement pass — both now route through a shared `death_zone_for` (reuses `applicable_
replacements`). Also **revived the previously-dead `WouldBeDestroyed`/`WouldDie` static-replacement path**
(`affected_object` never covered `Destroy`, so any "would be destroyed" static was unreachable). Scope
invalidates on zone change (CR 400.7, in `move_object`) + expires at turn start. → **Wilt in the Heat**
(5 dmg + exile-if-dies; real-path tests: lethal-damage-exiles, sacrifice-exiles, invalidation, 2-rider
ChooseReplacement ordering). **Cleanly unblocks the Dawning Archaic's would-die→exile rider.** (The general
container is also the right rails for **Wildgrowth Archaic**, but that clause is a *delayed enters-with-counters*
on the next-cast creature — needs a `FloatingRewrite::EntersWithCounters` variant + `WouldEnterBattlefield`
matched for floating riders; a modest follow-on, NOT free.)

**PLANESWALKERS + EMBLEMS (earlier this session):**

**EMBLEMS subsystem (CR 114, commit after d62e155):** the engine now has a **command zone**
(`Zone::Command` = a per-player `Player.command` vec) and emblems. An emblem is a registered def in the
reserved **9500+** block (`cards/emblems.rs`, mirrors `tokens.rs`) with **no characteristics** (CR 114.2)
carrying a normal `Ability::Triggered` + `Ability::FunctionsFrom(vec![Zone::Command])`. `Effect::CreateEmblem
{emblem}` (→ `Action::CreateEmblem` → `create_emblem`) puts one in the controller's command zone; a new
`queue_command_functioning_triggers` scan (mirrors the graveyard one) fires its triggers from Command,
stamping the triggering amount onto the trigger's `x` so the effect reads "**that much**" as `ValueExpr::X`.
Emblems are untouchable (no SBA/removal scans Command). → **Dellian's −6** ("whenever you gain life, target
opponent loses that much"). **Composed, didn't reinvent** (agent-9's FunctionsFrom + the token-def pattern).
Catalog filter (mtg-gre-server) now also excludes empty-card_type defs. **This generalizes to every future
emblem AND gives the engine its command zone.**

1. **Verified the 4 planeswalker points are ALREADY BUILT + TESTED** (as the handoff predicted — read-the-code
   confirmed, no fixes needed): (1) **enters with printed loyalty** through the REAL cast path — `resolve_top`
   routes a permanent spell → `move_object` → `enter_with_loyalty` (state/mod.rs:712), not just `add_card`;
   (2) **loyalty abilities are sorcery-speed + once/turn per PW across all of them** — the activation gate reads
   `Timing::Sorcery`→`sorcery_speed` + `Restriction::OncePerTurn`→`used_once_per_turn` (priority.rs:1145/1157);
   tests `loyalty_ability_is_once_per_turn_across_all_abilities`, `cannot_activate_a_minus_ability_without_enough_
   loyalty`; (3) **combat damage removes loyalty** — the `Action::Damage` executor decrements `CounterKind::Loyalty`
   saturating (whiteboard.rs:1834); test `combat::a_planeswalker_can_be_attacked_and_loses_loyalty`; (4) the
   **±N activation path** pays loyalty at `activate_ability` (`pay_cost` Loyalty arm) — tests `loyalty_plus/minus_
   ability_*`. Added a NEW end-to-end `priority::planeswalker_lifecycle_cast_activate_ultimate_dies` (cast from hand
   → enters loyalty 5 → +2→7 → −3 kills a creature→4 → drain to 0 → 0-loyalty SBA dies).
2. **`planeswalker()` + `loyalty_ability()` builders** (cards/mod.rs) — the general PW primitives (Legendary +
   PlaneswalkerType subtype + starting loyalty; a loyalty ability = sorcery/once-per-turn/`Loyalty(±N)` cost).
3. **`PlayerRef::Each`** (value.rs + `eval_player`, whiteboard.rs) — the player analogue of `EffectTarget::Each`
   (reads the same `foreach_current` cursor). Makes "**any number of target players each do X**" expressible as
   `ForEachTarget{ slot: player, body: …{ who: Each } }`. **Blast radius was 1 arm** (every other `PlayerRef` match
   routes through `eval_player` via `other =>` or a wildcard).
4. **`CardFilter::ManaValue` targeting arm** (priority.rs `target_matches_filter`) — was **fail-closed** (`_ =>
   false`), so any "target card with mana value ≤ N" was un-enumerable through the real cast/activation path.
   Now reads `o.chars.mana_value()` (mirrors the `count_filter_matches` arm). Reusable for every MV-bounded target.
5. **Professor Dellian Fel** `{2}{B}{G}` loyalty 5 (**tracked-partial**): +2 gain 3 life / 0 draw-a-card-lose-1 /
   −3 destroy target creature — all faithful; **−6 emblem DEFERRED** (needs the CR 114 emblem subsystem).
6. **Ral Zarek, Guest Lecturer** `{1}{B}{B}` loyalty 3 (**tracked-partial**): +1 Surveil 2 / −1 any-number-of-
   target-players-each-discard (via `PlayerRef::Each`) / −2 reanimate a MV≤3 creature from your graveyard — all
   faithful; **−7 DEFERRED** (needs a coin-flip randomness primitive + a skip-turns mechanism, neither built).

**▶ DEFERRED PW-completion subsystems (design-sketch to the lead before building; each is a real subsystem, not a
hack):** (a) **Emblems (CR 114)** — a command-zone object with abilities but no characteristics, can't be removed;
Dellian's −6 needs a triggered emblem ("whenever you gain life, target opponent loses that much"). The clean shape
is a `Zone::Command` emblem object carrying an `Ability::Triggered`; likely also unblocks future PW ultimates.
(b) **Coin flips + skip-turns** (Ral −7) — a `flip N coins` randomness leaf (seeded RNG already in the engine) +
an extra/skipped-turn tracker on `Player`. Lower priority (one ultimate).

**▶ RECOMMENDED NEXT ORDER (unchanged from the brief, minus planeswalkers):**
- **Remaining S12 cards** (the cost-reduction MECHANISM is done; each blocked on a DIFFERENT secondary — see the
  detailed list under "Remaining S12 cards" further down): **Run Behind** (top-or-bottom owner-choice), **Brush Off**
  (needs `TargetKind::StackObject` real-path enumeration — the counterspell gap in the Systemic notes), **The Dawning
  Archaic** (free-cast-an-I/S-from-gy-on-attack), **Wilt in the Heat** (exile-if-would-die replacement rider).
- **Lessons/Learn** (CR 715 outside-the-game / a sideboard-pool concept — **design-sketch to the lead first**; gym
  decks may need a sideboard notion — note the boundary).
- **Prepare-DFCs** (~36 — the CR 712 card-faces model: face selection on cast, characteristics from the active face
  through the layer system; the biggest single piece — **design-sketch first**).

**PROCESS (unchanged, hard-won):** shared tree → `git commit --only <paths>` (stage a NEW file with `git add`
first), never `-a`/`add -A`/stash; DON'T touch `experiments/` (MuZero + GPU runs live there); `cargo test -p
mtg-core` green at every commit; flip a cap's ledger Status cell in the SAME commit; **`git log -S "<mechanism>"`
+ READ THE CODE before scoping any ⏳ row as new** (beliefs have drifted in BOTH directions). Real-path integration
test for every mechanism; expect-test snapshots. Ping the lead at subsystem boundaries + design sketches for
Emblems / Lessons / prepare-DFCs before building. On fatigue: declare, rewrite THIS block, hand off clean.

---
### ▶ Prior handoff — sos-cards-9 (superseded by the block above, kept for provenance)

**▶▶ sos-cards-9 HANDOFF (2026-07-04) — READ FIRST. SCOPE = FULL SET; quality bar = general CR capability,
not the minimal hack.** 158→163 authored / all fully-faithful, **602 mtg-core tests green, tree clean** (LEAD
pushes). Shipped **6 caps + 5 cards + the missing Swamp basic land** (each a real-path test, `git commit --only`
on the shared tree; MuZero's `experiments/` untouched):
1. **S12 target-dependent cost reduction** (`583f30f`) — the risky sub-cap agent-8 deferred. `CostReduction`'s
   condition is now `CostReductionCondition::{State(Condition) | TargetMatches(CardFilter)}`; `effective_cast_cost`
   takes a `TargetCtx::{Optimistic | Chosen(&targets)}`. Offer gate applies a target-dependent discount
   optimistically (a legal matching target exists); `cast_spell` recomputes the final cost from the CHOSEN
   targets AND constrains each target slot's candidates to what the caster can pay (reductions only lower cost →
   base affordable keeps all, else only discount-granting targets) — auto_pay never underpays, **no rewind**.
   + `CardFilter::Tapped`/`Untapped`. → **Ajani's Response** (real-cast test proves the untapped creature is not
   offered when only the reduced cost is affordable). Orysa migrated to `State(...)`.
2. **enters-tapped MoveZone** (`9bd7fa1`) — `tapped: bool` on `Effect::MoveZone` + `Action::MoveZone` (set after
   `move_object` re-untaps, CR 110.5; mirrors `Effect::Search{tapped}`). → **Teacher's Pest** (gy→battlefield
   tapped). **Also registered the missing Swamp basic land** (`grp::SWAMP=5` — no black basic existed!).
3. **Exile-as-cost** (`eadceae`) — wired `CostComponent::Exile(SelectSpec)` (was defined-but-unpaid;
   `exile_cost_candidates`/`pay_exile_cost` mirror the Discard pair, exclude the source). → **Postmortem Professor**.
   Reusable for escape/delve. **The graveyard-recursion trio (Summoned Dromedary/Teacher's Pest/Postmortem) is
   now COMPLETE.**
4. **graveyard-functioning triggers (NEW CLASS)** (`5b79e8d`-range) — `Ability::FunctionsFrom(Vec<Zone>)` marker
   (lead-approved **Design B generalized**: battlefield is the implicit default zone-of-function, only deviating
   cards carry the marker; CR 113.6; generalizes to hand/exile by adding zones) + `collect_triggers` graveyard
   scan + batched `EventPattern::YouDealCombatDamageToPlayer` (`GameEvent::CombatDamageToPlayerBy`, once/controller/
   combat-damage-step) + **`Effect::MayPayCost{cost,then}`** ("you may pay …; if you do, …" — the mana analogue of
   `IfYouDo`, broadly reusable). → **Killian's Confidence** (real-path: combat damage → gy trigger → pay {W/B} →
   return self; + the declined/unpayable path stays in gy).
5. **activated-ability cost reduction** (extends S12 to CR 602) — `Ability::CostReduction` gained
   `scope: CostReductionScope::{Cast|ActivatedAbilities}`; `effective_activation_cost(source,&Cost)` applies
   `ActivatedAbilities`-scoped reductions to an activated ability's mana, at BOTH the offer gate and
   `activate_ability`; factored a shared `apply_cost_reduction` helper. → **Diary of Dreams** (page counter =
   `CounterKind::Named("page")`, zero enum churn; `{5},{T}:Draw` costs {1} less per page counter).

**▶ RECOMMENDED NEXT ORDER (all remaining need a genuine subsystem — none is a quick win):**
- **The big three (DESIGN-SKETCH TO THE LEAD BEFORE EACH; lead wants Planeswalkers FIRST — most groundwork):**
  **Planeswalkers** — ⚠️ **the groundwork is MOSTLY BUILT (verify by reading before scoping!):** `CardType::
  Planeswalker`, `CostComponent::Loyalty(±N)` (with the "can't pay −N without N loyalty" check), the **0-loyalty
  SBA** (`sba.rs`), `used_once_per_turn` + `OncePerTurn` restriction, AND **direct attacks** (combat's `may_attack`
  defender list already includes the defender's planeswalkers, `combat/mod.rs` ~139) all EXIST, plus a
  `planeswalker_enters_with_loyalty_and_dies_at_zero` test. **Verify these 4 before building:** (1) enters-with-
  starting-loyalty from card data; (2) loyalty abilities offered at sorcery speed + once/turn *across all* the
  PW's loyalty abilities; (3) combat damage to a planeswalker REMOVES loyalty counters (CR 120.3 — check the
  `Action::Damage` executor handles a `Target::Object(pw)`); (4) the loyalty-ability activation path. Then author
  **Professor Dellian Fel** + **Ral Zarek, Guest Lecturer** (emblems, CR 114, may be deferrable per-card — a
  command-zone token with a static). Likely a small-to-moderate finish, not a from-scratch subsystem. Then
  **Lessons/Learn** (CR 715 outside-the-game
  / a sideboard-pool concept — gym decks may need a sideboard notion; note the boundary), then **Prepare-DFCs**
  (~36 — a real card-faces model in the CR 712 shape: face selection on cast, characteristics from the active
  face through the layer system; the biggest piece).
- **Remaining S12 cards** (target-dependent MECHANISM done; each blocked on a DIFFERENT secondary): **Run Behind**
  (uses the new cap w/ `Attacking` filter; needs "put target on top OR bottom of owner's library, owner chooses"
  — an owner-side binary decision, no clean existing primitive — a small decision-plumbing effect); **Brush Off**
  (uses the cap w/ the `Cost({1}{U})` arm + an I/S-spell filter; ALSO needs `TargetKind::StackObject` candidate
  enumeration in the real cast path — `target_candidates` returns empty for StackObject, so counterspells are only
  tested via `resolve_effect` — a separate cap + stack-target filter matching); **Diary of Dreams** (activated-
  ability cost reduction — a per-ability variant of my cast-time work applied at `activate_ability`; + a Page
  `CounterKind` + a SpellCast-I/S→add-page-counter trigger); **The Dawning Archaic** (`GenericValue(Count{I/S in
  gy})` arm already built — the reduction is DONE; needs a free-cast-an-I/S-from-gy-on-attack trigger);
  **Wilt in the Heat** (reduction is FREE via `State(CardLeftGraveyardThisTurn)` — existing pipeline; needs a
  "if that creature would die this turn, exile it instead" delayed replacement rider).
- **The big three (DESIGN-SKETCH TO THE LEAD BEFORE EACH):** Lessons/Learn (CR 715 outside-the-game/sideboard),
  Planeswalkers (CR 306/606 — `CostComponent::Loyalty` + a PW-dies test already exist), Prepare-DFCs (~36, the
  card-faces model — the biggest piece).

**PROCESS (unchanged, hard-won):** shared tree → `git commit --only <paths>` (stage a NEW file with `git add`
first), never `-a`/`add -A`/stash; DON'T touch `experiments/` (MuZero + GPU runs live there); `cargo test -p
mtg-core` green at every commit; flip a cap's ledger Status cell in the SAME commit; **`git log -S "<mechanism>"`
+ READ THE CODE before scoping any ⏳ row as new** (beliefs have drifted in both directions). Real-path integration
test (cast/activate→pay→target→resolve) for every mechanism; expect-test snapshots (`UPDATE_EXPECT=1` to regen).
Ping the lead at subsystem boundaries + design sketches for new classes / the big three. On fatigue: declare,
rewrite THIS block, hand off clean.

### ▶ Systemic notes (cross-cutting — read before scoping cost/targeting/counterspell work)
- **No-rewind is a pragmatic economy, NOT architecture law** (user directive, 2026-07-04). The cast path
  currently pre-masks so nothing needs undoing (target-dependent cost modifiers filter target candidates by
  affordability — see `cast_spell`). Keep exact pre-filtering where it stays cheap (RL values exact masks), but
  when a mechanic makes pre-filtering **combinatorial** (convoke/improvise-class alt-payments, stacked cost
  modifiers × restricted mana, modal×X×affordability), the sanctioned path is a **transactional pending-cast**:
  snapshot/hold the cast context, allow cancel/rollback before commitment — exactly MTGA's GRE pending-cast+cancel
  model (mirroring the GRE is a project goal). Don't contort future designs to preserve no-rewind. Recorded in
  `docs/design/WHITEBOARD_MODEL.md` §2.6. The candidate filter already consumes each candidate's *full* effective
  cost (not "reduction present"), so a future target-dependent cost **increase** works by construction.
- ~~**Counterspell targeting has NEVER gone through the real cast path (latent gap).**~~ **FILLED (sos-cards-13,
  commit w/ Brush Off).** Three pieces, each general: (a) `target_candidates` StackObject arm — enumerates every
  **spell** stack object (abilities on the stack = Stifle-class targets, out of first-pass scope) as
  `Target::Stack(sid)`, EXCLUDING the spell being cast (`source`) so a counterspell isn't offered as a target of
  itself (601.2c puts it on the stack first; matches MTGA); (b) `target_matches_filter` `Target::Stack` branch —
  resolves a stack target to its underlying spell's card object and applies the filter to that, so "creature spell"
  / "instant or sorcery spell" / "spell you control" read the spell's computed chars; (c) **the actual root cause:
  `collect_specs_into` never matched `Effect::Counter`/`CounterUnlessPay`**, so the counter's target spec was
  silently dropped at cast (`specs` empty → no target chosen → nothing countered). That's why Essence Scatter et al.
  were only ever exercised via `resolve_effect` with a hand-built `Target::Stack`. Now real counterspell casts work:
  choose a stack target, re-checked at resolution (608.2b), `CantBeCountered` respected. → **Brush Off** (real
  cast-path tests: counter an opposing creature spell; self not offered; can't-counter Surrak; target-dependent
  {1}{U} reduction masked to affordable targets, no rewind).

---
### Prior handoff — sos-cards-8 (superseded by the block above, kept for provenance)

**▶▶ sos-cards-8 HANDOFF (2026-07-04) — SCOPE IS NOW THE FULL SET** (T4 deferral REVOKED —
prepare-DFCs, Lessons, planeswalkers, spell-copy, Fractalize, all subsystems in scope). Quality bar:
each subsystem built as the GENERAL CR capability, not the minimal hack. **153→158 authored / 155 fully-
faithful / 3 tracked-partial, 586 mtg-core tests green, tree clean (commits local, not yet pushed — ask lead).**
Shipped **5 cards + 5 caps**, each with a real-path test, via `git commit --only` on the shared tree:
1. **`Effect::DirectedDiscard` + `TargetKind::Player(PlayerFilter)`** (`4faa6d9`) — "target opponent reveals
   hand, YOU choose a nonland, they discard it" (chooser ≠ discarder, CR 701.8) + a general player-target
   restriction (`Any`/`Opponent`/`You`; `Effect::TargetPlayer` now carries the filter — 5 existing consumers
   updated to `Any`). → **Render Speechless**.
2. **`CostComponent::ActivateFromGraveyard`** (`4b70bc1`) — a pure graveyard-usability marker (no cost effect)
   decoupling "this activated ability functions from the graveyard" from S18's `ExileSelfFromGraveyard` (which
   is marker AND exile cost); the graveyard scan accepts either. → **Summoned Dromedary** (`{1}{W}`: return
   self gy→hand, via `MoveZone{SourceSelf→Hand}`).
3. **LKI dies-triggers (CR 603.10a)** (`3ef761d`) — **load-bearing.** New `GameState.last_known: BTreeMap<ObjId,
   Lki>` (Lki = computed chars + controller), captured in `move_object` when a permanent LEAVES the battlefield
   (before status/controller reset); `ComputedChars` gained serde. Wired `EventPattern::CreatureDies(filter)`
   (was defined-but-unfired) via `queue_watching_dies_triggers` + a new LKI-aware `dies_filter_matches` (the
   dies analogue of `enter_filter_matches`, reading the LKI snapshot). + `CardFilter::ToughnessAtMost`. →
   **Arnyn, Deathbloom Botanist** (deathtouch + drain when a P/T≤1 creature you control dies) + **Cauldron of
   Essence** (drain when a creature you control dies + sac-cost sorcery reanimation). ⚠️ **LKI groundwork for
   the WHOLE future** — every dies/LTB-trigger and "draw cards = its power"-style effect should read `last_known`.
   Only the FILTER path reads LKI so far; when a dies-trigger's *effect/value* needs the dead object's stats,
   thread the LKI into `ResolutionCtx` (not built yet — no consumer). `SelfDies` effects still read the live
   (graveyard) object; fine for current self-dies cards, revisit when one reads its own dying stats.
4. **S12 cost-reduction pipeline (CR 601.2f / 118)** (`9621fef`) — `Ability::CostReduction{amount, condition}`
   (`CostReductionAmount::{Generic(u32)|GenericValue(ValueExpr)|Cost(ManaCost)}`) + `effective_cast_cost(p,card,
   base)` applied at BOTH the offer gate AND `cast_spell` (so affordability == payment for state/count conditions)
   + `ValueExpr::TotalToughness`. → **Orysa** (costs {3} less if creatures you control total toughness ≥10).
   ⚠️ **Only state/count conditions so far** (exact affordability). **Target-dependent** (Ajani's Response, Brush
   Off, Run Behind) is a distinct sub-cap: the reduction depends on CHOSEN targets, so the offer gate must be
   optimistic (offer if a qualifying target EXISTS makes it affordable) and the actual reduction computed from
   chosen targets at cast — mind the no-rewind invariant (over-offer → auto_pay underpays). The `GenericValue`
   and `Cost` (coloured) arms are built but not yet exercised by a card.

**▶ NEXT AGENT — recommended order (adjust with judgment; the lead's suggested order is in the brief):**
- **S12 cost-reduction — finish it.** The general pipeline is IN (`effective_cast_cost`, state/count conditions,
  Orysa). Remaining 6 cards need: (a) **target-dependent affordability** (Ajani's Response — Destroy target
  creature, {3} less if targets a TAPPED creature — is FULLY faithful once this lands; also Brush Off, Run Behind).
  Add `CostReductionAmount`/condition awareness of chosen targets: offer gate optimistic (a qualifying target
  exists → reduced), actual reduction from chosen targets at cast; guard the no-rewind invariant. (b) **coloured
  reduction** consumer (Brush Off's {1}{U}, `Cost` arm built) + Counter (built). (c) **activated-ability cost
  reduction** (Diary of Dreams — attach a reduction to an `Activated` ability, per page counter). (d) **Wilt in
  the Heat** ({2} less if `CardLeftGraveyardThisTurn`, cond built — trivial; needs an exile-if-would-die
  replacement rider). (e) **The Dawning Archaic** (`GenericValue(Count{I/S in gy})`, arm built) + S10-on-attack.
- **Enters-tapped** (`ZoneDest` has no tapped flag; 43 literals so DON'T add a required field — add a small
  builder or a separate `Effect::MoveZone` tapped variant / an entering-tapped continuous). Unblocks the rest of
  graveyard-recursion (**Teacher's Pest** gy→battlefield tapped) + Mind Roots / Mind into Matter enters-tapped.
- **Postmortem Professor** — needs an exile-an-I/S-from-gy cost variant (like `ExileSelfFromGraveyard` but exile
  a DIFFERENT gy card) + a "can't block" qualification (Defender = can't-attack; can't-block is separate) + the
  `ActivateFromGraveyard` marker (done) for its gy→battlefield reanimation.
- **Killian's Confidence** — triggered-ability-that-functions-from-graveyard (combat-damage trigger → pay {W/B}
  → return self gy→hand). A NEW class: triggered (not activated) abilities usable from the graveyard.
- Then the lead's list: dynamic-ManaValue filters, blink-with-delayed-return, move-counters, grant-arbitrary-
  ability (layer 6), repeatable-modal + dynamic-X targeting, spell-copy, Fractalize.
- **The big three (design-sketch to the lead BEFORE building):** Lessons/Learn (OutsideTheGame zone), Planeswalkers
  (NOTE: `CostComponent::Loyalty` + a `planeswalker_enters_with_loyalty_and_dies_at_zero` test ALREADY exist —
  groundwork is partly there; read it first), Prepare-DFCs (36 — card-faces model, biggest piece).

**PROCESS (unchanged, hard-won):** shared tree → `git commit --only <paths>` (stage a NEW file with `git add`
first), never `-a`/`add -A`/stash; DON'T touch `experiments/` (muzero-debug lives there + GPU runs); `cargo test
-p mtg-core` green at every commit; flip a cap's ledger Status cell in the SAME commit; **`git log -S "<mechanism>"`
+ READ THE CODE before scoping any ⏳ row as new** (several drifted stale historically); real-path integration
test (cast→pay→target→resolve) for every mechanism. Ping the lead at subsystem boundaries. On fatigue: declare,
rewrite THIS block, hand off clean.

---
## Prior handoffs (superseded by the block above, kept for provenance)

**▶▶ sos-cards-7 HANDOFF (2026-07-03) — 153 authored / 150 fully-faithful / 3 tracked-partial,
575 mtg-core tests green, tree clean, all pushed.** Shipped **5 caps + 4 cards**, each with a real-path test
(activation-with-X, put-then-double, YouAttack-trigger-on-`AttackersDeclared`, distinct-named-lands activation),
all committed via `git commit --only` on the shared tree:
1. **{X}-in-an-activated-cost** (`7102d4a`) — `activate_ability` now `ChooseNumber{ChooseX}`s (bounded by affordable
   mana), folds `chosen_x*pips` into generic, carries X on the stack object; ability-resolution `ResolutionCtx.x`
   was hardcoded `None` → now `obj.x`. → **Berta, Wise Extrapolator** (all 3 clauses).
2. **S20 `ValueExpr::CountersOnTarget{target,kind}` + flush-before-`PutCounters`** (`6fe5aaf`) — the `PutCounters`
   interpret arm now flushes staged actions first (mirrors CreateToken's #61 flush) so "put a +1/+1, then double"
   reads the post-first count. → **Growth Curve**. Full suite confirms **no counter-card regression**.
3. **`CardFilter::Attacking`** (`e5207a1`) — matches a current declared attacker (`CombatState::is_attacking`),
   added to `target_matches_filter` + exhaustive `count_filter_matches`. → **Living History** (ETB Spirit +
   `YouAttack`/S9-gated pump on a target attacking creature).
4. **`ValueExpr::DistinctNames{zone,filter,controller}`** (distinct card-names among matching objects) + wired
   **`CardFilter::HasCounter` into the layer-system static-scope matcher** (`chars/mod.rs::matches_filter`, was
   `_ => false`) (`9b0937f`) → **Emil, Vastlands Roamer** (counter-gated trample anthem + `{4}{G},{T}` Fractal with
   X = differently-named lands). ⚠️ Corrected the sos-cards-6 belief that {X}-activated-cost would clear Emil — it
   would NOT; Emil's X = differently-named lands, not a paid {X} (always verify the oracle).

**▶ NEXT AGENT — the moderate queue is now down to heavier single-card caps (each ~1–2 caps, one card):**
- **directed-discard `Effect`** → **Render Speechless** (`{2}{W}{B}`): "target opponent reveals their hand, YOU
  choose a nonland card, that player discards it" + "put two +1/+1 counters on up to one target creature." Needs a
  NEW interactive `Effect` leaf (reveal target player's hand → the CHOOSER/caster picks a matching card → that
  player discards it — unlike `interpret_discard` where the discarder chooses) + a player target (slot 0) and a
  creature target (slot 1). Only unblocks THIS card in SOS (scoped 2026-07-03).
- **Treasure token with an ACTIVATED mana ability** → **Seize the Spoils** (`khm`): a token with `{T}, Sacrifice:
  add one mana of any color`. ⚠️ HEAVIER than it looks — that's a *sacrifice-cost mana ability*, and the mana
  payment path (`auto_pay`/`usable_mana_sources`) only *taps* sources; it has no "sacrifice for mana" support.
  Verify/extend the mana system before scoping as cheap. (S11 did only TRIGGERED token abilities.)
- **Slumbering Trudge** — stun-counter core is authorable now (S3 done); its "enters tapped unless X≤2" clause needs
  X threaded into `EntersTappedUnless`'s condition eval (whiteboard.rs ~1454 evals with no X ctx) — or defer that
  one clause and ship tracked-partial.
- Bigger subsystems stay **DEFERRED** (lower ROI): spell-copy (~1 net card), move-counters, cost-reduction (S12),
  dynamic-ManaValue, blink-with-delayed-return, graveyard-play, grant-arbitrary-ability, Fractalize (= milestone-5
  SET color/type layers), LKI dies-triggers. 36 prepare-DFC + 2 planeswalkers + 5 Lessons stay deferred by type.

**PROCESS (unchanged, hard-won):** shared tree → `git commit --only <paths>` (stage a NEW file with `git add`
first, then `--only` it), never `-a`/`add -A`/stash; don't touch `experiments/`; `cargo test -p mtg-core` green at
every commit; flip a cap's ledger Status cell in the SAME commit; **`git log -S "<mechanism>"` + READ THE CODE
before scoping any ⏳ row as new** (multiple prose beliefs were wrong in BOTH directions). Ping the lead at cap
boundaries. On fatigue: declare, rewrite THIS block, hand off clean.

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
- ~~**`{X}` in an ACTIVATED ability cost**~~ **DONE (sos-cards-7)** — `activate_ability` now `ChooseNumber{ChooseX}`s
  (bounded by affordable mana), folds `chosen_x * pips` into generic, carries X on the stack object; the
  ability-resolution `ResolutionCtx.x` was hardcoded `None`, now `obj.x`. → **Berta, Wise Extrapolator** authored
  (all 3 clauses fully-faithful, 3 real-path tests incl. legality→pay→resolve activation with X=3). ⚠️ **The handoff
  belief that this ALSO clears Emil was WRONG** — verify-the-oracle: Emil's `{4}{G},{T}` uses X = differently-named
  lands, NOT a paid `{X}`. Emil still needs a **`DistinctNamedLands` value** (unbuilt) + its conditional trample anthem.
- ~~**`ValueExpr::CountersOnTarget(n)` + a commit-between-steps flush**~~ **DONE (sos-cards-7)** → **Growth Curve**.
  Added `ValueExpr::CountersOnTarget { target, kind }` (reads live count of a counter kind on the Nth chosen target)
  + a flush-before-`PutCounters` interpret arm (mirrors CreateToken's #61 flush) so "put a +1/+1, THEN double" reads
  the post-first-counter count. Full suite (568) confirms no counter-card regression.
- ~~**`CardFilter::Attacking`** (combat-state filter)~~ **DONE (sos-cards-7)** → **Living History** (ETB Spirit + a
  `YouAttack`/S9-gated pump on a target attacking creature). Added `CardFilter::Attacking` (matches a current
  declared attacker via `CombatState::is_attacking`) to `target_matches_filter` + the exhaustive
  `count_filter_matches`; real-turn test fires the trigger on `AttackersDeclared` and gates on the intervening-if.
  • **Treasure token def** (a token with an ACTIVATED `{T},Sac: any-color mana` ability —
  verify token activated abilities fire; S11 did only TRIGGERED token abilities) → **Seize the Spoils** (`khm`).
- **directed-discard `Effect`** (reveal hand → chooser picks → discard) → **Render Speechless**. • **Slumbering
  Trudge**: stun-counter core authorable now; its enter-tapped-if-X≤2 clause needs X threaded into `EntersTapped
  Unless`'s condition eval (whiteboard.rs ~1454 evals with no X ctx) — or defer that one clause.
- ~~**DistinctNamedLands value** → Emil~~ **DONE (sos-cards-7)** — `ValueExpr::DistinctNames{zone,filter,controller}`
  (distinct card-names among matching objects) + wired `CardFilter::HasCounter` into the layer-system static-scope
  matcher (`chars/mod.rs::matches_filter`) for Emil's "creatures you control with +1/+1 counters have trample" anthem.

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
| **S12** Cost-reduction cond. | "costs {N} less if it targets X / you control Y / a card left your gy" (cast-time) | 7 | ◑ **PIPELINE + STATE + TARGET-DEPENDENT DONE** (sos-cards-8 `9621fef` pipeline; sos-cards-9 target-dependent) — `Ability::CostReduction{amount:CostReductionAmount::{Generic\|GenericValue\|Cost}, condition:CostReductionCondition::{State(Condition)\|TargetMatches(CardFilter)}}` + `effective_cast_cost(p,card,base,TargetCtx::{Optimistic\|Chosen(&targets)})`. **State** cond → **Orysa**. **Target-dependent** (CR 601.2f, sos-cards-9): the offer gate applies the discount optimistically (a legal matching target exists → best-case cost), `cast_spell` recomputes the FINAL cost from the CHOSEN targets *and* constrains each target slot's candidates to what the caster can pay (reductions only lower cost → base affordable keeps all; else only discount-granting targets), so auto_pay never underpays — **no rewind** (the load-bearing invariant agent-8 flagged). + `CardFilter::Tapped`/`Untapped` arms. → **Ajani's Response** (Destroy + {3}-off-if-targets-tapped; real-cast test proves the untapped creature is NOT offered when only {1}{W} is affordable). ✅ **Brush Off DONE (sos-cards-13)** — Counter target spell + `Cost({1}{U})` coloured arm's first card + `TargetMatches(instant-or-sorcery spell)`, the first real-cast-path counterspell (needed the StackObject-enumeration fill: `collect_specs_into` was silently dropping `Effect::Counter`'s target spec — see the Systemic note). **Remaining:** **Run Behind** (uses this cap; needs a "put on top/bottom of library, owner's choice" effect) — but Run Behind is DONE per the sos-cards-11 handoff (verify); ~~**Diary of Dreams**~~ **DONE** (sos-cards-9) — activated-ability cost reduction via `CostReductionScope::{Cast\|ActivatedAbilities}` + `effective_activation_cost` (applied at the activated-ability offer gate + `activate_ability`); page counter = `CounterKind::Named("page")` (zero enum churn); **The Dawning Archaic** = `GenericValue(Count{I/S in gy})` [arm built, untested] + free-cast-on-attack trigger; **Wilt in the Heat** = `State(CardLeftGraveyardThisTurn)` (free via the existing pipeline) + 5 dmg + exile-if-dies replacement rider. |
| **S14** Copy spell/perm | "copy target spell", "create a token that's a copy of", "cast a copy of" | 7 | ◑ **token-copy DONE** (`Effect::CreateTokenCopy`+`TokenCopyMods`, `a8c8a2d` → Applied Geometry). **CAST-A-COPY (CR 707.12) DONE (sos-cards-11, `5e1754a`)** — `Effect::CastCopy{source, controller}` mints a copy `Object` from the source's copiable base chars (707.2 via grp_id) into `Zone::Stack` and casts it via the real `cast_spell(WithoutPayingManaCost)`; `Object.is_copy` → ceases to exist off the stack (707.10a, `state.cease_to_exist`). Powers **Paradigm** (5 Lessons) and is the foundation for **prepare-DFCs** (36 cards — see the NEXT-AGENT design plan). **COPY-A-SPELL-ON-THE-STACK (CR 707.10) DONE (sos-cards-13)** — the copy that ISN'T cast: `copy_spell_on_stack(spell, by, choose_new_targets)` mints an `is_copy` copy from the spell's copiable chars (707.2) and pushes a `StackObject` OVER the original carrying its targets/X/modes (707.10b), with an optional `rechoose_copy_targets` reselection (707.10c); NO `SpellCast` (707.10a — no cast triggers). Delivered via a one-shot delayed trigger: `Effect::CopyNextSpellCast{filter, choose_new_targets}` → `DelayedTriggerEvent::YouCastSpell{filter, choose_new_targets}` (armed on resolve, expires unfired at next turn's start) → fires a new `StackObjectKind::SpellCopyTrigger{spell, choose_new_targets}` on the controller's next matching `SpellCast`. → **Pigment Wrangler // Striking Palette** (prepare front + "copy your next I/S this turn, new targets" — real-path test: bolt + its copy both hit for 3, copy ceases to exist). **Reusable for**: **Lumaret's Favor** (Infusion "copy it if you gained life this turn" — add `Effect::CopySpellOnStack{what}` delegating to `copy_spell_on_stack`), Twincast-class "copy target spell". Other 707.10 cards still double-blocked (Aziza tap-3-cost, Choreographed Sparks modal+grants, Mica Ward—Pay-life, Prismari Storm). |
| **S17** Ward {cost} | Ward N / Ward—Pay life / Ward—Discard (counter-unless-pay on becoming targeted) | 7 | ◑ **mana DONE** `96dbc35` — `Effect::CounterUnlessPay{ what, cost:Cost }` soft-counter + `EffectTarget::Triggering` (the targeting spell/ability, threaded via `GameEvent::Targeted.source` → `state.trigger_targeting_source` → `ResolutionCtx.triggering_stack`); `CardFilter::ItSelf` now matches in `enter_filter_matches` (source-threaded, opt-in from the targeted path). Reuses `Cost`+`can_pay_cost`/`pay_cost`. Ward constructors live in `cards/helpers.rs` (`ward`/`ward_mana`/`ward_discard`). → **Colorstorm Stallion** (Ward {1}, mana) + **Forum Necroscribe** (Ward—Discard, the non-mana path — reuses the `Discard` cost arms). **Ward—Pay life** (Mica/Prismari): `pay_cost` has NO `PayLife` arm yet (falls to `_ => {}`, so life isn't deducted) — add it first; their *secondaries* are also blocked (spell-copy/storm). Side-fix landed here: `Effect::MoveZone`'s target was missing from `collect_specs_into` (never collected through the REAL cast/trigger path — prior MoveZone tests bypassed casting), now fixed. |
| **S15** Impulse play | exile/mill → "you may play it until end of turn / your next turn" | 6 | ◑ **DONE for exile cases** (`d079eb0` base + `0e17d3e` top-of-library source + land-play) → Practiced Scrollsmith, Elemental Mascot, Suspend Aggression (3). Only **graveyard-play** (milled card played from gy — Ark of Hunger, Tablet) still ⏳; the other 2 S15 cards are cap-blocked (Archaic's Agony=S7, Tablet=S13) |
| **S3** Stun counters | `CounterKind::Stun` + "would untap → remove a stun counter instead" replacement | 6 | ✅ **DONE** `f8ab8ea` (untap-step replacement, CR 702.171) → Procrastinate, Deluge Virtuoso, Fractal Mascot, Rapier Wit. (Was mis-listed ⏳.) |
| **S18** Graveyard-activated | an ability that functions while its card is in the graveyard (recursion) | 6 | ✅ **DONE** `6190bb2` (`CostComponent::ExileSelfFromGraveyard` + graveyard ability enumeration in `legal_priority_actions`) → Eternal Student, Stone Docent. Also `DiscardSelfFromHand` for hand-usable cycling-style abilities (Visionary's Dance). (Was mis-listed ⏳.) |
| **S11** Token-with-ability | `TokenSpec` carries an ability (Treasure `{T},Sac`; Pest attack→gain life) | 5 | ◑ **DONE for grp-id ability tokens** — a `TokenSpec.grp_id` points at a registered token def whose abilities fire (Pest `PEST_TOKEN`=9001, "attack → gain 1 life") → Send in the Pest, Pestbrood Sloth, Essenceknit Scholar. A **Treasure** token (`{T}, Sac: add one mana of any color` — an ACTIVATED mana ability on a token) is not yet verified; check for a registered Treasure def before authoring one. (Was mis-listed ⏳.) |
| **S13** Restricted mana | mana usable "only to cast instant and sorcery spells" (spend-restriction tag) | 4 | ✅ **DONE** `ffcc0df` (`ManaSpec.restriction=InstantSorceryOnly` + `ManaPool.restricted` bucket + `allow_restricted` threaded through the payment path; spell casts pass card-is-I/S, ability costs pass false) → Hydro-Channeler |
| **S16** Gain-life trigger | `EventPattern::GainLife` ("whenever you gain life, …") | 3 | ✅ **DONE** |
| **S21** cast-with-{X} trigger | `SpellCast` filtered to "has {X} in its cost" | 2 | ✅ **DONE** (`134444d` + agent 6) — `HasXInCost` arm in `enter_filter_matches` → **Matterbending Mage**; `ValueExpr::XOfTriggeringSpell` (reads the triggering spell's `Object.cast_x`, recorded at cast alongside `mana_spent`) → **Geometer's Arthropod** (look at top X, keep 1). |
| **S19** cards-drawn-this-turn value | `ValueExpr::CardsDrawnThisTurn` (reads `Player.cards_drawn_this_turn`, reset each turn + incremented in `draw`) | 1 | ✅ **DONE** (agent 6) → **Fractal Anomaly** (0/0 Fractal + X counters, X = cards drawn this turn) |
| **{X}-in-activated-cost** | choose `{X}` when activating an ability (CR 602.2b), fold into mana paid, carry on the stack object so `ValueExpr::X` reads it at resolution — mirrors the spell-cast X path | 1 | ✅ **DONE** (sos-cards-7) — `activate_ability` (priority.rs) `ChooseNumber{ChooseX}` bounded by affordable mana + folds `chosen_x * pips` into generic; ability-resolution `ResolutionCtx.x` was `None`, now `obj.x`. → **Berta, Wise Extrapolator** (`{X},{T}: Fractal with X counters`). NOTE: Emil's `{4}{G},{T}` does NOT use a paid `{X}` — its X = differently-named lands (needs a `DistinctNamedLands` value, a separate cap). |
| **S20** counters-on-target value | `ValueExpr::CountersOnTarget { target, kind }` (reads live count of a counter kind on the Nth chosen target) + a flush-before-`PutCounters` interpret arm so a prior counter-add commits before the read | 1 | ✅ **DONE** (sos-cards-7) → **Growth Curve** ("+1/+1 counter, then double"). The flush mirrors CreateToken's #61 fix; the full suite (568 tests) confirms no counter-card regression. |
| **S22** cast-I/S-this-turn cond | (done — see NEXT-AGENT block) | 1 | ✅ **DONE** (agent 6) |
| **misc one-offs** | GreatestMV, ~~DistinctNames~~, ~~SoftCounter~~, ~~DirectedDiscard~~, AltCost, PayXLife, NoMaxHand, GrantAbility | 1–3 ea | ⏳ except **SoftCounter ✅** (`Effect::CounterUnlessPay`, Ward `96dbc35`), **DistinctNames ✅** (sos-cards-7, `ValueExpr::DistinctNames`), and **DirectedDiscard ✅ DONE** (sos-cards-8 `4faa6d9` — `Effect::DirectedDiscard{who,chooser,count,filter}` chooser≠discarder + `TargetKind::Player(PlayerFilter::{Any,Opponent,You})` general player-target restriction → **Render Speechless**). The rest (GreatestMV/AltCost/PayXLife/NoMaxHand/GrantAbility) genuinely unbuilt. |
| **LKI dies-triggers** | last-known-info store (CR 603.10a) + `CreatureDies(filter)` wiring so other permanents' filtered dies-triggers fire, matched against the dead object's pre-death chars/controller | 2+ | ✅ **DONE** (sos-cards-8 `3ef761d`) — `GameState.last_known` captured in `move_object`, `queue_watching_dies_triggers`/`dies_filter_matches`, `CardFilter::ToughnessAtMost` → **Arnyn, Cauldron of Essence**. LKI store is groundwork for ALL future dies/LTB abilities (effect-time LKI reads still TODO). |
| **graveyard-recursion** | `CostComponent::ActivateFromGraveyard` (pure gy-usability marker, no cost effect — cf. S18's `ExileSelfFromGraveyard`) for "{cost}: return this from your graveyard" self-recursion | 3+ | ◑ **self→hand + self→battlefield-tapped DONE** — `4b70bc1` (self→hand) → **Summoned Dromedary**; sos-cards-9 (self→battlefield TAPPED, via the new **enters-tapped** cap below) → **Teacher's Pest** (completes the trio's tapped-reanimation). **Postmortem Professor** DONE (sos-cards-9): self `Qualification::CantBlock` static + `SelfAttacks` drain (`Sequence[LoseLife EachOpponent, GainLife]`) + graveyard-recursion whose cost exiles *another* I/S card from the gy via the newly-wired **`CostComponent::Exile`** (see below). **Killian's Confidence** DONE (sos-cards-9): the new-class **graveyard-functioning triggered abilities** cap (see below). ✅ **The whole graveyard-recursion vein is now cleared.** |
| **enters-tapped (MoveZone)** | `tapped: bool` on `Effect::MoveZone` + `Action::MoveZone` (set in the executor after `move_object` re-untaps, CR 110.5 — the `Effect::Search { tapped }` analogue for reanimation/bounce-to-battlefield) | 3 | ✅ **DONE** (sos-cards-9) → **Teacher's Pest** (gy→battlefield tapped). Also registered the **Swamp** basic land (`grp::SWAMP=5` — was missing; no black mana source existed). Now unblocks the enters-tapped *clause* of **Mind Roots** (discard 2, put a discarded land tapped) + **Mind into Matter** (put a permanent from hand tapped) — each still needs its OTHER clauses (Mind Roots = put-from-hand/discard-driven; Mind into Matter = draw-X + put-from-hand + dynamic-MV). |
| **Exile-as-cost** | `CostComponent::Exile(SelectSpec)` wired in `can_pay_cost`/`pay_cost` (`exile_cost_candidates`/`pay_exile_cost`, mirror the Discard pair; excludes the source; moves chosen cards to Exile) — was defined-but-unpaid ("for escape/delve"). | 1+ | ✅ **DONE** (sos-cards-9) → **Postmortem Professor** ("Exile an I/S card from your graveyard:"). Reusable for future escape/delve. |
| **graveyard-functioning triggers** | `Ability::FunctionsFrom(Vec<Zone>)` marker (CR 113.6 — battlefield is the implicit default zone-of-function; only deviating cards carry the marker, zero churn) + `collect_triggers` graveyard scan (`queue_graveyard_functioning_triggers`, reuses `queue_self_triggers`) + batched `EventPattern::YouDealCombatDamageToPlayer` / `GameEvent::CombatDamageToPlayerBy` (once per controller per combat-damage step, broadcast from `deal_combat_substep`) + `Effect::MayPayCost{cost,then}` ("you may pay …; if you do, …" — the mana analogue of `IfYouDo`). | 1+ | ✅ **DONE** (sos-cards-9) → **Killian's Confidence**. `FunctionsFrom` generalizes to hand/exile (madness/suspend) by adding zones to the scan; `MayPayCost` is broadly reusable. |
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
| Arnyn, Deathbloom Botanist | LKI-dies | `sos` | ✅ done | deathtouch + `CreatureDies` LKI trigger (P/T≤1 you control) drain 2/gain 2 |
| Artistic Process | - | `sos` | ✅ done | modal: 6-to-target / 2-to-each-opp-creature (ForEach chooser:Opponent) / flying+haste token |
| Ascendant Dustspeaker | - | `sos` | ⏳ | flying, ETB counter, exile graveyard card |
| Bogwater Lumaret | - | `sos` | ✅ done | creature-ETB gain-life trigger, IR |
| Borrowed Knowledge | - | `sos` | ⏳ | modal discard hand, draw by count |
| Burrog Banemaker | - | `sos` | ✅ done | deathtouch + activated pump |
| Burrog Barrage | - | `sos` | ⏳ | conditional pump + power-based damage |
| Cauldron of Essence | LKI-dies | `sos` | ✅ done | `CreatureDies(you control)` LKI drain + sac-cost sorcery reanimation |
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
| Ajani's Response | S12 | `sos` | ✅ done | target-dependent cost reduction ({3} off if targets a tapped creature) + Destroy; lander for the S12 target-dependent sub-cap |
| Ambitious Augmenter | S6 | `sos` | ⏳ | Increment mechanic (mana-spent vs power/toughness) |
| Antiquities on the Loose | S10 | `sos` | ⏳ | flashback + cast-from-zone condition |
| Applied Geometry | S14 | `sos` | ✅ done | create token copy of permanent |
| Arcane Omens | S7 | `sos` | ✅ done | Converge colors-of-mana discard |
| Archaic's Agony | S7,S15,ExcessDamage,multi-top-exile | `sos` | ⏳ | S7+S15 now DONE, but still needs: (a) an **excess-damage** value (damage beyond the creature's toughness) and (b) **multi-card** top-of-library impulse-exile (`TopOfLibrary` is single-card) — "exile cards equal to the excess damage, play them until your next turn" |
| Ark of Hunger | S9,S15 | `sos` | ⏳ | graveyard-leave trigger + impulse play |
| Aziza, Mage Tower Captain | S14 | `sos` | ⏳ | copy your instant/sorcery spell |
| Banishing Betrayal | S1 | `sos` | ✅ done | bounce + Surveil 1 |
| Berta, Wise Extrapolator | S6,{X}-in-activated-cost | `sos` | ✅ done | Increment (S6) + CountersPutOnSelf→AddMana any-color + `{X},{T}` Fractal via the new {X}-in-activated-cost cap |
| Blech, Loafing Pest | S16 | `sos` | ✅ done | whenever-you-gain-life counter trigger |
| Brush Off | S12 | `sos` | ⏳ | conditional cost reduction if targets a spell |
| Choreographed Sparks | S14 | `sos` | ⏳ | copy instant/sorcery or creature spell |
| Colorstorm Stallion | S5,S14,S17 | `sos` | ⏳ | Ward cost + Opus + token-copy |
| Comforting Counsel | S16 | `sos` | ✅ done | gain-life counter trigger + conditional anthem |
| Conciliator's Duelist | S8 | `sos` | ⏳ | Repartee cast-targets-creature trigger |
| Cuboid Colony | S6 | `sos` | ✅ done | Increment on flash flyer |
| Daydream | S10 | `sos` | ⏳ | blink with counter + flashback |
| Deluge Virtuoso | S3,S5 | `sos` | ✅ done | stun counter ETB + Opus trigger |
| Diary of Dreams | S12-activated | `sos` | ✅ done | SpellCast(I/S)→page-counter trigger + `{5},{T}:Draw` with activated-ability cost reduction ({1} less per page counter) |
| Dig Site Inventory | S10 | `sos` | ✅ done | counter + vigilance, flashback |
| Duel Tactics | S10 | `sos` | ✅ done | damage + can't-block, flashback |
| Efflorescence | S4 | `sos` | ✅ done | Infusion gained-life-this-turn condition |
| Elemental Mascot | S5,S15 | `sos` | ✅ done | Opus cast-trigger: +1/+0; if 5+ mana spent, impulse-exile top card (`ExileForPlay{TopOfLibrary}`) castable until your next turn |
| Emil, Vastlands Roamer | DistinctNames,HasCounter-static | `sos` | ✅ done | `Static GrantKeyword(Trample)` scoped by `CardFilter::HasCounter` (now wired into the layer-system static matcher) + `{4}{G},{T}` Fractal with X=`ValueExpr::DistinctNames{lands you control}` counters |
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
| Growth Curve | S20 | `sos` | ✅ done | +1/+1 counter on target you control, then double — `ValueExpr::CountersOnTarget` + the new flush-before-`PutCounters` interpret arm (reads post-first-counter count) |
| Hardened Academic | S9 | `sos` | ⏳ | cards-leave-graveyard trigger grants counter |
| Homesickness | S3 | `sos` | ⏳ | draw, tap, stun counters |
| Hungry Graffalon | S6 | `sos` | ✅ done | Increment mechanic |
| Hydro-Channeler | S13 | `sos` | ◑ partial | `{T}: Add {U}` I/S-restricted (S13 lander) done; `{1},{T}: Add any` restricted deferred (mana-ability-with-mana-cost, unmodeled) via `.incomplete()` |
| Imperious Inkmage | S1 | `sos` | ✅ done | ETB surveil 2 |
| Informed Inkwright | S8 | `sos` | ✅ done | Repartee makes Inkling token |
| Inkling Mascot | S8,S1 | `sos` | ✅ done | Repartee grants flying, surveil |
| Inkshape Demonstrator | S17,S8 | `sos` | ⏳ | Ward, Repartee pump/lifelink |
| Killian's Confidence | gy-triggers,MayPayCost | `sos` | ✅ done | pump+draw spell + graveyard-functioning trigger (`FunctionsFrom`) on batched combat-damage → `MayPayCost {W/B}` return-self |
| Lecturing Scornmage | S8 | `sos` | ✅ done | Repartee self-counter |
| Living History | S9,CardFilter::Attacking | `sos` | ✅ done | ETB Spirit token + `YouAttack` trigger, intervening-if `CardLeftGraveyardThisTurn` (S9), pumps a target attacking creature (+2/+0) via new `CardFilter::Attacking` |
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
| Orysa, Tide Choreographer | S12 | `sos` | ✅ done | cost {3} less if total toughness≥10 (S12 pipeline + `ValueExpr::TotalToughness`) + ETB draw 2 |
| Owlin Historian | S1,S9 | `sos` | ✅ done | surveil + cards-leave-graveyard trigger |
| Paradox Gardens | S1 | `sos` | ✅ done | surveil activated ability |
| Paradox Surveyor | S2 | `sos` | ✅ done | look-and-pick ETB selection |
| Pensive Professor | S6 | `sos` | ⏳ | Increment (plus counter-added trigger) |
| Pest Mascot | S16 | `sos` | ✅ done | whenever-you-gain-life trigger |
| Pestbrood Sloth | S11 | `sos` | ✅ done | Pest token with attack ability |
| Poisoner's Apprentice | S4 | `sos` | ✅ done | Infusion gained-life-this-turn condition |
| Postmortem Professor | S18,Exile-cost | `sos` | ✅ done | can't-block static + attack drain + `{1}{B}`,exile-an-I/S-from-gy graveyard-recursion (wired `CostComponent::Exile`) |
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
| Render Speechless | DirectedDiscard,PlayerFilter | `sos` | ✅ done | `DirectedDiscard` (you choose opp's discard) + `TargetKind::Player(Opponent)` |
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
| Summoned Dromedary | ActivateFromGraveyard | `sos` | ✅ done | vigilance + `{1}{W}` graveyard-recursion (self→hand) via the marker |
| Sundering Archaic | S7 | `sos` | ⏳ | converge, colors of mana spent |
| Suspend Aggression | S15 | `sos` | ✅ done | exile target nonland permanent + top of library; each playable through its OWNER's next turn (Sequence of two `ExileForPlay`, per-owner window) |
| Tablet of Discovery | S13,S15 | `sos` | ⏳ | impulse-play milled card; restricted mana |
| Tackle Artist | S5 | `sos` | ✅ done | Opus cast-IS trigger, mana spent |
| Teacher's Pest | S18,enters-tapped | `sos` | ✅ done | Menace + SelfAttacks gain-life + `{B}{G}` graveyard-recursion to battlefield **tapped** (new enters-tapped MoveZone cap) |
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
| Wilt in the Heat | S9,S12 | `sos` | ✅ done | S12 reduction + 5 dmg + floating "would-die→exile" delayed-replacement cap (CR 614) (`this session`) |
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
| Professor Dellian Fel | PW | `sos` | ✅ done | fully faithful — +2/0/−3 + −6 emblem (CR 114 Zone::Command subsystem) (`this session`) |
| Quandrix, the Proof | Cascade | `sos` | ⏳ | Elder Dragon granting cascade |
| Quill-Blade Laureate // Twofold Intent | DFC | `sos` | ⏳ | modal double-faced card |
| Ral Zarek, Guest Lecturer | PW | `sos` | ◐ tracked-partial | +1/−1/−2 faithful; −7 coin-flip+skip-turns deferred (`this session`) |
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
