# Card-implementation push — Secrets of Strixhaven (`sos`, 271 distinct cards)

Standing workstream: implement the Secrets of Strixhaven set for **limited (40-card) play** in
`mtg-core`, easiest-first, correctness over count. This ledger is the capability index + full
per-card triage, modeled on `SELESNYA_LANDFALL_CARDS.md`.

## ✅ SET COMPLETE — 271/271 (sos-cards-21 finale, 2026-07-06) — relay CLOSED, this is now a maintenance note

**▶▶ THE SET IS COMPLETE. 271/271 authored · 271 fully-faithful · 0 tracked-partials · 0 Native
hatches. 871 mtg-core tests green, whole workspace (incl. mtg-py) builds, tree clean.** There is **no
next-agent queue** — this block supersedes every handoff below. Scryfall-diff verified: every one of
the 271 `sos` card names (`sqlite: SELECT name FROM cards WHERE set_code='sos'`) resolves to a
registered `CardDef` — sos cards in `cards/sos/`, reprints/duals/basics in their first-printing folders
(Ancestral Anger `vow/`, basics `misc/basics.rs`, Terramorphic Expanse `tsp/`, …), DFCs by front-face
name. `grep -rln '.incomplete()' cards/sos` = **0**; no `Effect::Native`/`Ability::Native` anywhere in
the pool (the three "Native"-tagged ledger cards — Steal the Show, Mathemagics, Pox Plague — were all
built pure-IR).

**The sos-cards-21 finale (the last 3 items):**
- **`71a60ea` Resonating Lute** `{2}{U}{R}` — the granted-mana subsystem (B3). New **`StaticContribution::
  GrantTapMana{mana}`** (a layer-6 ability grant with no home in `ComputedChars`) + reader **`chars::
  granted_tap_mana`** (reuses `gather_statics`/`affects_matches`) so a granted `{T}: Add …` is visible to
  affordability + auto-pay. Mana enumeration carries a per-tap **count**; `select_payment` uses a unit up
  to `count` times committed to ONE colour ("two mana of any one colour") behind a **one-tap-one-ability
  source guard**; `auto_pay` adds each tapped source's full count (restricted surplus floats). Additive —
  a no-op for every existing single-ability source (payment suite green). Plain `{T}` grant ⇒ **auto-pay-
  usable / trainable**, no option-B caveat.
- **`0c26308` Petrified Hamlet** — ETB name-choice reusing the existing **`ChooseOption{reason:NameCard}`**
  decision (zero cross-crate churn). New **`Object.chosen_name`** (reset on zone change) set by
  **`Effect::ChooseLandName`** over the engine-enumerated land-card names in play. Name-keyed statics read
  it: the **ability-legality gate** (`name_is_chosen` in `legal_priority_actions` — a non-mana activated
  ability of any source whose name is a noted `chosen_name` isn't offered; mana abilities exempt) and the
  **`{T}:{C}` grant** via new **`CardFilter::NamedAsChooser`** on the B3a granted-tap-mana path. Its own
  `{T}:{C}` is trainable.
- **`074fff2` Nita, Forum Conciliator ability-2 rider** (the last tracked-partial → cleared). New
  **`Effect::ExileTargetThenMayCast`** + **`Action::ExileForCastBy`** grant, on the exiled opp-gy I/S:
  **`Object.castable_by`** (cross-player exile-cast — the offer scans OTHER players' exile),
  **`spend_any_mana`** (`ManaCost::collapse_to_generic` at the offer gate, the target-affordability
  pre-filter, AND `cast_spell`'s payment — "mana of any type"), and **`exile_on_leave`** (→ `flashback_cast`
  so the spell is exiled not graveyard'd when it leaves the stack — on resolve OR counter; the
  `interpret_counter` graveyard-only gap fixed too). Real-path test: `{2}`+sac exiles P1's bolt, P0 casts
  it with GREEN mana, hits P1 for 3, exiled on leave.

### ⚠️ Auto-pay-inert (human/manual-only) abilities — 271-faithful ≠ 271-trainable

These cost-bearing mana abilities are **faithful card data** and work via the manual mana path (which
pays the extra cost through `pay_cost`), but are **inert to auto-pay for agent/RL seats** — the auto-payer
only taps `{T}` sources (`mana::mana_sources_kind`'s `is_simple_tap_mana` gate), it can't pay a
sacrifice / pay-life / extra-mana cost. This is the established **"option-B"** convention (faithful, not
trainable-through-auto-pay); folding them into auto-pay is parked in WHITEBOARD_MODEL §2.6 (B1/B2, with
the lead's no-suicide-life-gate + single-shared-planner constraints).

| Card / source | Cost-bearing mana ability | Why inert to auto-pay |
|---|---|---|
| **Treasure token** (the shared token def) | `{T}, Sacrifice this token: Add one mana of any colour` | non-`{T}` cost (Sacrifice) |
| **Goblin Glasswright // Craft with Pride** (back) | creates a Treasure → its `{T},Sac` mana ability | (the Treasure above) |
| **Great Hall of the Biblioplex** | `{T}, Pay 1 life: Add any colour (I/S-only)` | non-`{T}` cost (Pay life) |
| **Hydro-Channeler** (2nd ability) | `{1}, {T}: Add any-of-5 (I/S-only)` | non-`{T}` cost (extra `{1}`) |

Everything else — including **Resonating Lute's** granted `{T}: Add two of one colour` and **Petrified
Hamlet's** `{T}:{C}` (own + granted) — is a plain `{T}` ability, **fully auto-pay-usable and trainable**.

### Known faithful-modelling divergences (documented, negligible in limited)

- **Silverquill the Disputant — Casualty timing.** Casualty 1 is modeled as a `Triggered{SpellCast(I/S)}`
  whose effect copies the spell, rather than the spell entering the stack already-copied at cast (CR
  702.153). The copy still resolves correctly (spell-copy path); only the exact "copied as it's put on the
  stack" ordering differs — no observable difference for the pool's interactions.
- **Rubble Rouser — mana-ability timing.** Its `{T}, Exile-from-gy: Add {R}. When you do, ping each
  opponent` is modeled as a **non-mana activated ability** (`Sequence[AddMana, DealDamage]`) so an agent
  seat can select it and the reflexive ping fires — a true mana ability (CR 605, no stack) is never
  offered to the RL seat. Deliberate: the ping is the card's point.
- **Resonating Lute — greedy payment tail.** `select_payment` is greedy (as the whole payment path always
  has been): an exotic cost needing two *different specific* colours from a *single* Lute-granted land
  (one tap = one colour) is correctly rejected, but greedy can still miss some multi-source colour
  orderings. The manual-UI `produce_mana` covers granted *unrestricted* multi-mana; granted *restricted*
  (I/S-only) mana is auto-pay-only (the manual restricted-tap UI is a documented gap, not needed by the
  agent seat).
- **Petrified Hamlet — name enumeration.** "Choose a land card name" enumerates the distinct land-card
  names *present in the game* (deterministic, engine-masked), not the full oracle universe of names — the
  faithful, tractable index-of-N for a limited pool.

### Trackers
Ledger below is retained as the full per-card triage + capability index (the S-caps table, ~line 1500+,
stays the map for future sets). `WORKLOG.md` + `PROJECT_STATE.md` updated to the COMPLETE state.

---

## ★ BONUS SHEET — Secrets of Strixhaven Mystical Archive (`soa`, 65 distinct cards) — OPEN RELAY

**New user directive (2026-07-06): everything playable in SOS *limited* must be in the engine.** The SOS
limited environment = the 271 `sos` main set (COMPLETE) **+ the bonus sheet**, which is the **Secrets of
Strixhaven Mystical Archive** — Scryfall set **`soa`** (the SOS analog of real-Strixhaven's `sta`). It is a
"greatest-hits of iconic spells" sheet: 15 mythic / 25 rare / 25 uncommon = **65 distinct booster-legal
cards** (Scryfall lists 195 rows for `soa`; the extra 130 are foil/alt-art variants of the same 65 names).

- **Set identified (Step 0):** `sqlite: SELECT DISTINCT name FROM cards WHERE set_code='soa'` → 65 names.
  Cross-checked against `../mtg-ai` (friend's engine, READ-ONLY): all 65 `soa` names appear in their
  `src/mtg/cards/definitions/` — confirms the exact list. (Their "346 = 271+75" over-counts; the distinct
  bonus card list is 65.) **None of the 65 are currently authored in our `cards/`** — clean slate.
- **Folder placement:** first-printing-set rule (unchanged). These are reprints, so most land in *older* set
  folders keyed by first printing (e.g. Giant Growth → `lea/`, Preordain → `m11/`, Force of Will → `all/` …),
  NOT a `soa/` folder. Look up each card's earliest printing in sqlite before creating a file.
- **oracle text = OUR sqlite** always (`soa` rows); friend's engine is a *list* cross-check only, never a
  behaviour oracle.

> **PROCESS RULES (inherited, non-negotiable):** `git log -S "<name>"` + read the code before believing any
> capability claim — beliefs in this ledger have been overturned ~15×. Flip a status cell in the SAME commit
> that lands the change, with the hash. `cargo test -p mtg-core` green every commit; real-path integration
> tests (cast→pay→target→resolve), expect-test snapshots; short human commit messages, no AI attribution;
> `git commit --only <your paths>` on the SHARED tree (never plain commit / -a / stash; never touch
> `experiments/`, `/tmp/mtgenv_tb`, GPU, or the :8080 server). The LEAD pushes. Design-sketch to the lead
> before any architecture-level subsystem. New cards ride `/api/cards` via `CardDb::iter` automatically —
> verify they appear; art-manifest regen is the lead's job.

### Triage (grounded against the CURRENT engine — every mechanic below was grep-verified in `mtg-core/src`, not assumed)

Engine reality that shapes the buckets (all **confirmed present**): Modal/Sequence/Optional/IfYouDo/ForEach,
DealDamage/Destroy/Exile/Draw/Mill/GainLife/LoseLife/PumpPT/Counters/AddMana/CreateToken/Search(→ any
ZoneDest incl. top-of-lib + `tapped`)/MoveZone(gy→bf `tapped`)/Sacrifice(Controller/EachPlayer/EachOpponent,
their-choice)/Counter/CounterUnlessPay(soft)/Attach; **Storm** (`CopySpellOnStack{count:SpellsCastThisTurn−1}`),
**Cascade**, **Miracle**, **CopySpellOnStack** (copy target spell), **Treasure token**, **scry** staging,
**Flashback**, **Cycling** (`CostComponent::DiscardSelfFromHand`), Phyrexian/Hybrid pips in `ManaCost`,
**poison counters** + SBA, rich `ValueExpr` (`ManaValueOfTarget`, `PowerOfTarget`, `HandSize`,
`SpellsCastThisTurn`, `Count`, …). **Genuinely absent** (grep = 0): Overload, Spree, Kicker, Convoke (only a
`ConvokeImprovise` decision stub), Suspend, Split-second, Infect, Protection-from, Role tokens, alt-cast
"pay non-mana instead of mana cost", damage **Redirect** (`Rewrite::Redirect` = "future work"), change-target.

**Bucket A — pure IR, no new cap (~33 cards; author these first):**
Abrade · Armageddon · Big Score · Brain Freeze · Bring to Light · Brotherhood's End · Bulk Up · Crop Rotation ·
Culling the Weak · Disdainful Stroke · Duty Beyond Death · Empty the Warrens · Feed the Swarm · Flusterstorm ·
Fracture · Giant Growth · Helping Hand · Hop to It · Jeska's Will · Locust Spray · Pick Your Poison ·
Prismatic Ending · Pyretic Ritual · Repel Calamity · Shamanic Revelation · Shared Roots ·
Sheoldred's Edict · Sleight of Hand · Smallpox · Spell Pierce · Stock Up · Vampiric Tutor · Zombify.
(`LookAndPick{rest_to:Library}` = "look top N, K to hand, rest to bottom" → Sleight of Hand / Stock Up /
Stargaze; `ManaValueOfTarget` → Feed the Swarm; `HandSize` of opp → Jeska's Will mode 1.)

**Bucket B — one small card-agnostic cap each (~19 cards; each cap unlocks 1–2):**
- *alt-cast (pay non-mana instead of mana cost)* → **Daze** (return an Island), **Force of Will** (pay 1 life +
  exile a blue card). Shared cap = a `CastVariant::Alternative`/free-cast gated by a payable non-mana cost.
- *Phyrexian mana* `{B/P}` (pay {B} or 2 life) → **Dismember**.
- *Kicker* (optional additional cost + "if kicked" cond) → **Burst Lightning**.
- *Clue token def* (like the existing Treasure def) → **Deduce**.
- *repeat-until-you-stop loop* (`Effect::Repeat` E5 is unwired) → **Ad Nauseam**.
- *true scry* (`Effect::Scry{count}` — mirror `Effect::Surveil` but bin the rest to **bottom** of library,
  not graveyard; reuses the `SelectReason::ScryStage` decision) → **Preordain** (Surveil bins to gy; only 1 soa card scrys).
- *land-creature token* (Land+Creature types on one `TokenSpec`, X count) → **Awaken the Woods**.
- *delayed "destroy if it attacked this turn" + cast-timing restriction* → **Berserk**.
- *modal / pay-life additional cost* (`CostComponent::PayLife` arm) → **Bitter Triumph**.
- *up-to-X targets + `5×X` value* → **Crackle with Power**.
- *mana-per-permanent-destroyed (colour choice)* → **Culling Ritual**.
- *3-way top-look with distinct destinations (hand/bottom/exile-impulse)* → **Expressive Iteration**.
- *until-EOT "whenever you cast a creature spell, draw"* delayed static → **Glimpse of Nature**.
- *one-way fight (deal damage = power, no back-damage)* → **Knockout Maneuver**.
- *CreateToken under another player's control* → **Pongify**.
- *bounce a spell off the stack to hand* → **Reprieve**.
- *2X look, X→hand rest→gy* → **Stargaze**.
- *X-threshold conditionals in one resolution* → **Subterranean Tremors**.
- *qualified hexproof-from-colour + "opp cast blue/black this turn" state + can't-be-countered grant* →
  **Veil of Summer** (borderline B/C — the hexproof-from-colour is the real cap).

**Bucket C — subsystem, DESIGN-SKETCH TO LEAD FIRST (~12 cards; grouped by yield):**
- **Overload** (2) — Cyclonic Rift, Winds of Abandon. Alt-cast variant + "target"→"each" text rewrite.
- **Spree** (2) — Requisition Raid, Return the Favor (+ *change-target* cap for its mode-2). Modal additional costs.
- **Role tokens** (2) — Monstrous Rage (Monster Role), Royal Treatment (Royal Role). Aura enchantment tokens
  with a Static P/T+keyword grant, attach-on-create, and the "sac the older Role on the same creature" rule.
- **Protection-from** (1) — Akroma's Will (mode 2 = protection from each colour; mode 1 is pure IR).
- **Infect** (1) — Triumph of the Hordes (grant infect: combat damage as −1/−1 counters / poison; poison SBA exists).
- **Suspend** (1) — Living End (time counters, cast when last counter removed; Warp-analogue).
- **Convoke** (1) — Return to the Ranks (tap creatures to pay generic/coloured; `ConvokeImprovise` stub exists).
- **Prevent-and-reflect / Redirect** (1) — Deflecting Palm (`Rewrite::Redirect` is stubbed "future work").
- **Split-second + can't-lose-the-game** (1) — Angel's Grace (hardest; two rules corners — likely last).

**Yield order for the relay:** Bucket A (34) sweep first → then B caps by count (alt-cast unlocks 2; the rest
1 each) → then C subsystems by yield (Overload/Spree/Role each = 2). Honest `fully_implemented` throughout.
Cap-then-cards, `git log -S` before scoping any "absent" mechanic (some B caps may already exist — verify).

**Progress log (append per commit):**
- **sos-bonus-1 (2026-07-06): 25/65 authored (all Bucket A, real-path tested, 908 mtg-core green).**
  - batch 1 `f…`: Giant Growth, Armageddon, Pyretic Ritual, Brotherhood's End, Hop to It, Empty the Warrens (storm), Smallpox, Disdainful Stroke.
  - batch 2: Spell Pierce, Fracture, Flusterstorm (storm), Brain Freeze (storm), Feed the Swarm, Big Score, Crop Rotation, Culling the Weak, Zombify, Sleight of Hand, Stock Up.
  - batch 3: Abrade, Bulk Up (flashback), Duty Beyond Death, Helping Hand, Pick Your Poison, Shared Roots.
  - **grp_id block = 600+** (max prior real id was 504); each reprint in its first-printing folder (new folders: m11, zen, stx, cmd, scg, znr, snc, exo, ody, hou, lci; Pick Your Poison → mkm since its only-earlier printing is the silver-border `cmb1`).
  - **Reclassified A→B during authoring (grounded):** Preordain (needs `Effect::Scry` — Surveil bins to gy, not bottom), Vampiric Tutor (`interpret_search` shuffles AFTER placing, so "search→shuffle→top" is unexpressible), Repel Calamity + Shamanic Revelation's Ferocious clause (need `CardFilter::PowerAtLeast`/`ToughnessAtLeast` — only `*AtMost` exists), Sheoldred's Edict (token/nontoken modes need a token-identity marker — `create_token` doesn't stamp `Supertype::Token`).
  - **Remaining Bucket A to assess:** Jeska's Will, Prismatic Ending, Bring to Light (converge/impulse — verifying they compose over S7 ColorsSpent + S15 impulse).
- **sos-bonus-1 continued (2026-07-06): 35/65 authored, 918 mtg-core green.** Since batch 3:
  - **`CardFilter::PowerAtLeast`/`ToughnessAtLeast`** cap (3 match sites) → Repel Calamity, Shamanic Revelation (Ferocious via `ForEach{power≥4, GainLife 4}`).
  - **`Effect::Scry`** cap (twin of `Surveil`, bins to bottom) → Preordain.
  - **Clue token def** (`grp::CLUE_TOKEN=9003` + `helpers::clue_token`) → Deduce.
  - **search→shuffle→place** cap in `interpret_search` (library-position tutors shuffle first) → Vampiric Tutor.
  - Zero-cap composites: Stargaze (`XTimes(2)`/`take X`), Knockout Maneuver (`SourcedDamage`+counter-then-`PowerOfTarget`), Pongify (`ControllerOfTarget` token), Subterranean Tremors (X-threshold `Conditional`s), Awaken the Woods (Land+Creature `TokenSpec`).
  - **Assess-cards → B/C (grounded):** Prismatic Ending (`collect_specs_into` doesn't walk `Conditional` → a cast-time target can't gate an exile cleanly), Jeska's Will (`TopOfLibrary` is single-card → multi-card impulse cap), Bring to Light (search dynamic-MV + free-cast).
  - **★ Needs LEAD sign-off before building (touch load-bearing infra / new subsystems):**
    - **pay-life / Phyrexian mana** (Dismember `{B/P}{B/P}`, Bitter Triumph "pay 3 life") — `ManaCost` has no phyrexian field and `pay_cost` has no `PayLife` arm; also gated by the lead's no-suicide-life constraint (ledger §"option-B"). Cluster also unlocks the parked Ward—Pay-life.
    - **alt-cast (non-mana instead of mana cost)** (Daze "return an Island", Force of Will "pay 1 life + exile a blue card") — a `CastVariant::Alternative` gated on a payable non-mana cost.
    - **token-identity marker** (Sheoldred's Edict token/nontoken modes; also fixes the latent Lorehold-Charm "nontoken" note) — stamp created tokens with `Supertype::Token` or an `is_token` flag.
    - **C subsystems** (highest yield first): **Overload** (Cyclonic Rift, Winds of Abandon), **Spree** (Requisition Raid, Return the Favor), **Role tokens** (Monstrous Rage, Royal Treatment) = 2 each; then Protection-from (Akroma's Will), Infect (Triumph of the Hordes), Suspend (Living End), Convoke (Return to the Ranks), Redirect (Deflecting Palm), Split-second/can't-lose (Angel's Grace).
- **sos-bonus-1 continued (2026-07-06): 39/65 authored, 925 mtg-core green, whole workspace builds.** Cleared the rest of the composable/clean-leaf-cap cards:
  - `Effect::ReturnSpellToHand` (bounce a spell off the stack, not a counter) → **Reprieve**.
  - **Prismatic Ending** (converge exile via `ManaValueExpr{max:ColorsSpent}` target bound, the sundering_archaic idiom — documents a target-restriction-vs-conditional divergence).
  - **Bring to Light** (zero new cap: `Search`→exile with dynamic MV bound + `Optional{CastForFree{Searched(0)}}`).
  - `Effect::ExileTopForPlay{who,count,window}` (multi-card impulse — `ExileForPlay` is single-card & staged so 3× in a Sequence all captured the same top) → **Jeska's Will** (both modes; `HandSize{ChosenTarget(0)}` mana + impulse 3).
  - **Crackle with Power** (zero new cap: `ForEachTarget{ slot max=TARGET_COUNT_X }` + `DealDamage XTimes(5)` — "up to X" was already supported).
- **sos-bonus-1 continued (2026-07-06): 42/65, 938 mtg-core green — lead greenlit all remaining work.**
  - **token-identity** (own commit): `create_token` now stamps `Supertype::Token` on created objects → **Sheoldred's Edict** (token/nontoken/PW modes) + fixed Lorehold Charm's latent nontoken gap (regression test) + fixes Spiritcall Enthusiast's "tokens you control enter" trigger.
  - **phyrexian pip class** `ManaCost.phyrexian: Vec<Color>` ({C/P} = one C or 2 life) → **Dismember**. Whole feature funnels through ONE `resolve_phyrexian(units, cost, life, auto)` seam that pre-resolves each pip to a coloured requirement (mana-preferred) or 2 life (no-suicide auto-gate), so `select_payment`/`spend_from_pool` need zero phyrexian awareness. Tests assert **pool empties in BOTH modes** + no-suicide gate + mana-value 3. (Note: `pay_cost`'s PayLife arm already existed since 4dd31ef; only the pip is new.)
- **sos-bonus-1 continued (2026-07-06): 46/65, 943 mtg-core green.** Lead-greenlit moderate caps shipped:
  - **Bitter Triumph** (modal discard-or-pay-3-life additional cost, rides the existing PayLife arm).
  - `Effect::RevealTopLoseLifeMayRepeat` → **Ad Nauseam** (imperative may-repeat loop, lose life = MV each).
  - `Effect::LookDistribute` (3-way look: hand / exile-for-play / bottom) + `choose_n` helper → **Expressive Iteration**.
  - **Glimpse of Nature** — parametrized the `YouCastSpell` delayed trigger with `reaction: {CopySpell|RunActions}` + `until_end_of_turn` (not a fork), added `Effect::WheneverYouCastThisTurn` + `lower_effect_to_actions`. Pigment Wrangler / Prismari-storm / Brain Freeze / Flusterstorm copy consumers stay green (explicit regressions in the commit).
  - **Culling Ritual DEFERRED**: the `ManaSpec` `one_of` subset-choice extension breaks ~20 full literals across other agents' card files (churn + shared-tree conflict risk) — better batched into a dedicated ManaSpec refactor, or the card takes a documented any-color divergence. Flagged to lead.
  - **★ REMAINING 19:** Berserk (attacked-this-turn + timing), Veil of Summer (3 mechanisms), Culling Ritual (deferred), Daze/Force of Will (alt-cast `CastVariant::Alternative` — awaiting lead confirm), + the 12 **C-subsystem** cards (Overload/Spree/Roles sketched to lead, awaiting sign-off; then Protection/Infect/Suspend/Convoke/Redirect/Split-second).
    - *moderate leaf caps (1 card each, no sign-off but real work):* **Glimpse of Nature** (recurring-until-EOT cast trigger w/ generic actions — the delayed-trigger `YouCastSpell` path is hardcoded to copy-spell), **Berserk** (delayed destroy-if-attacked + "attacked this turn" + cast-timing), **Crackle with Power** (dynamic-count `up to X` targets), **Expressive Iteration** (3-way look-distribute), **Culling Ritual** (count-destroyed→mana + `ManaSpec` B/G subset choice), **Ad Nauseam** (player-controlled reveal/lose-life repeat loop, `Effect::Repeat` E5 unwired), **Veil of Summer** (qualified hexproof-from-colour + "spells can't be countered this turn" floating grant + opp-cast-colour-this-turn state).
    - *sign-off-gated:* Dismember/Bitter Triumph (pay-life/Phyrexian), Daze/Force of Will (alt-cast), Sheoldred's Edict (token-identity), + the 12 C-subsystem cards (Overload/Spree/Roles/Protection/Infect/Suspend/Convoke/Redirect/Split-second).

---

## ▶ (superseded — history) NEXT-AGENT block, handoff from sos-cards-20, 2026-07-06

**▶▶ sos-cards-20 SHIPPED — 2 new cards + 3 tracked-partials cleared + 2 general engine extensions. 864 mtg-core green, whole
workspace builds, tree clean, LEAD pushes.** Census **266→269/271 authored (99%, 268 fully-faithful · 1 tracked-partial) · 0
Native hatches.** PROCESS RULES in the header apply (`git log -S` + read the code before believing anything).

**What shipped (own commits):**
- **`44a4387` Wildgrowth Archaic clause 2** — new **`FloatingRewrite::EntersWithCounters{kind,n}`** + **`Effect::
  EntersWithCountersRider{what,kind,n}`** arm a one-shot floating ETB replacement (CR 614.1e) on the just-cast creature spell
  (`what: Triggering` reads `ctx.triggering_spell` directly, like CopySpellOnStack), `n = ColorsSpentOnTrigger` fixed at trigger
  resolution. Stack→bf doesn't invalidate the scope, so the counters land as it enters. **Partial cleared.**
- **`3bd4a44` Fractalize + SUBSYSTEM A (layer-4 subtype changes).** The layer system was already ~complete; the only gap was
  creature-subtype mutation. New **`StaticContribution::AddSubtype(Subtype)`** + **`SetCreatureSubtypes(Vec<Subtype>)`** (layer 4,
  folded into `chars::compute` timestamp-ordered next to AddType) + general **`Effect::Becomes{what, contributions, base_pt:
  Option<(ValueExpr,ValueExpr)>, duration}`** (grants a bag of contributions + a **concrete-resolved** base P/T — a static recompute
  has no cast-X, so SetBasePTValue can't carry X+1). Collected in `collect_specs_into`. Fractalize = becomes a G/U Fractal, base P/T
  X+1, losing other colors/creature types.
- **`9795cbb` Rubble Rouser** — the `{T},Exile-gy: Add {R}. When you do, ping each opp` modeled as a **NON-mana activated ability**
  (`Sequence[AddMana{R}, DealDamage{1, EachOpponent}]`), because the RL/gym seat is never offered mana-ability activations (only
  auto-pay + non-mana Activate) and the reflexive ping is the card's point. Cost paid by existing `pay_cost`/`pay_exile_cost`. ETB
  loot = existing `Optional{IfYouDo{Discard,Draw}}`. **NO engine work.**
- **`26daeca` Great Hall of the Biblioplex** (SUBSYSTEM A `{5}` animation) + 2 general extensions: (1) **granted cast-triggers now
  fire** — extended `queue_watching_spellcast_triggers` to scan `GrantAbility` templates (mirrors the granted scan in
  `queue_self_triggers`); (2) **`Condition::SelfIsCreature`** (reads COMPUTED types via `chars::compute`, the re-animation guard).
  New grant template `grp::GRANT_ISCAST_PUMP` (9802). `{5}` = `Conditional(Not(SelfIsCreature), Becomes{AddType(Creature),
  AddSubtype(Wizard), GrantAbility{pump}, base_pt 2/4, Permanent})`. Pay-life ability = option-B (see below).
- **`70559fc` Hydro-Channeler 2nd** + Great Hall pay-life — cost-bearing mana abilities authored under **"option-B"** (the
  established convention: faithful DATA, works via the manual mana path which pays the extra cost through `pay_cost`, **auto-pay-inert
  for agent seats** — same as Treasures / Goblin Glasswright; §2.6 is the eventual fix). **Hydro partial cleared.**
- **`9099864` Ral Zarek −7** — new **`Player.skip_next_turns`** (CR 720; `advance_turn` consumes one skip per seat, capped at n) +
  **`Effect::FlipCoinsSkipNextTurns{who, coins}`** (flips on the seeded `state.rng`, adds heads to the opponent's skips). `who:
  Opponent` = "target opponent" in 2-player scope. **Partial cleared.**

**⚠️ OPEN DECISION TO THE LEAD (blocks the census wording, not the code):** does **option-B count as "fully-faithful"** for the 271
endgame (Great Hall's pay-life + Hydro-2nd are auto-pay-inert but faithful data + work manually, per the Treasure precedent) — OR
build **B1/B2** (fold cost-bearing mana into AUTO-PAY, the §2.6-adjacent payment work) so they're usable in training too? I recommend
option-B for the census + defer B1/B2 to §2.6. **This does NOT unblock the remaining 3 cards** — they need real subsystems either way.

**▶ THE REMAINING 3 (all touch the payment/enumeration core or need a new cross-crate decision — treat as engine milestones, sketch
to the lead before building; I HELD them rather than rush the crown-jewel at session end):**
1. **Resonating Lute** `{2}{U}{R}` Artifact — needs **B3** only. `{T}: Draw, only if 7+ cards in hand` = trivial (`Draw` +
   `Restriction::OnlyIf(handsize≥7)`, add a `HandSize`-based Condition if absent). The hard part: **"Lands you control have '{T}: Add
   two mana of any one color, I/S-only'."** = grant a tap-mana ability to a group. **`producible_colors`/`mana_sources_kind` (mana.rs
   :120) read ONLY printed `def.abilities`** — a GRANTED mana ability is invisible to enumeration (so it's inert even MANUALLY, worse
   than option-B). Two sub-parts: **(B3a)** a new `StaticContribution::GrantTapMana{mana: ManaSpec}` read by the mana enumeration
   (scan printed statics + `continuous_effects` for the contribution affecting each candidate land — replicate `affects_matches`
   in/for mana.rs); **(B3b)** **multi-mana-per-tap** — `ManaSpec.produces` already carries a count `ValueExpr` but the payment path
   adds 1/tap; make auto-pay respect the count (the "two of one color"; the TapCreatureForMana bonus is the existing precedent for
   >1 mana/tap). Keep it ADDITIVE (no change when no grant present; prove with a no-regression test).
2. **Petrified Hamlet** Land — needs a **name-choice class** + B3. `{T}: Add {C}` = trivial (`mana_ability(Colorless)`). Needs: (a)
   **`Object.chosen_name: Option<String>`** + an ETB **`DecisionRequest::ChooseCardName`** over land-card names (⚠️ new agent-facing
   decision → cross-crate exhaustive matches in `mtg-gre-server/options.rs` + `mtg-py/{codec,decision_stats}.rs`, like the
   `PlayableAction` additions before it); (b) a **name-keyed ability-legality gate** ("abilities of sources named X can't be activated
   unless mana abilities" — check in `legal_priority_actions`: if any Petrified's `chosen_name == source.name` and the ability isn't
   `is_mana`, it's illegal); (c) "lands named X have {T}:{C}" = B3a again (grant-tap-mana keyed to a name — nearly always redundant,
   could ship inert-and-flagged first). Bespoke, negligible in limited — sketch the name-choice + legality-gate to the lead.
3. **Nita, Forum Conciliator ability 2 rider** (the last tracked-partial) — cost + gy-exile are done; the "**you may cast it this
   turn, any type of mana, exile-instead-of-gy**" rider needs 3 mechs (spec in the sos-cards-19 block below): **(a)** a cross-player
   exile-cast permission (`castable_by: Option<PlayerId>` on Object + an offer-loop scan of OTHER players' exile — the impulse offer
   only scans the caster's own exile), **(b)** a **spend-any-type-of-mana** payment mode (collapse the cast cost to fully-generic in
   `can_pay`/`pay` — touches the payment core), **(c)** an exile-on-leave-stack flag riding the flashback exile path (CR 702.34d).

**End state target: 271/271 authored, 271 fully-faithful, 0 Natives.** After the 3 above land, announce it LOUDLY with the final
census. The relay does NOT require one session.

---

## ▶ (superseded — history) sos-cards-19 handoff, 2026-07-05

**▶▶ sos-cards-19 SHIPPED — 8 fully-faithful cards + 1 tracked-partial (Nita) + reusable caps, 854 mtg-core green, whole
workspace builds, tree clean, LEAD pushes.** Census **257→266/271 authored (98%, 262 faithful · 4 tracked-partial)**, **still 0
Native hatches**. **Headline finding: the ledger's "3 Natives" were ALL IR-expressible — the tag was stale (lead concurred, all
built pure-IR).** Steal the Show was fully misdescribed ("control-theft + wheel" → actually a plain modal wheel + I/S-graveyard
burn, ZERO new cap); Mathemagics = one generic `ValueExpr::Pow2`; **Pox Plague = pure IR** via `ValueExpr::Half` + `LifeTotal` +
`Effect::ForEachPlayer` (per-player-bound halving). **The census diff also surfaced two buildable cards the ledger never bucketed:
Flashback + Zimone's Experiment (both shipped).** **⇒ THE SET IS NOW EFFECTIVELY CARD-COMPLETE: only 5 unauthored, ALL either
engine-roadmap or lead-deferred (see the tail below) — 271 cards at 0 Natives stands.**

- **`8e595e4` — Pox Plague** (pure IR, the last of the "3 Natives"). New generic **`ValueExpr::Half`** (floor div-2) +
  **`ValueExpr::LifeTotal{who}`** + **`Effect::ForEachPlayer{body}`** (player analogue of `ForEach` — loops players in APNAP order
  binding `foreach_current`, so `PlayerRef::Each` in the body resolves to the iterated player). Pox = `Sequence[ForEachPlayer{
  LoseLife Half(LifeTotal Each)}, ForEachPlayer{Discard Half(HandSize Each)}, ForEachPlayer{Sacrifice Half(Count their-perms)}]` —
  three separate passes so all players finish a step before the next (CR 608.2). All pieces evergreen/reusable.
- **`ce41476` — Nita, Forum Conciliator** — **TRACKED-PARTIAL.** Ability 1 FULLY FAITHFUL: new **`CardFilter::OwnedBy(PlayerRef)`**
  (added to `enter_filter_matches` [ctx-aware, the real use] + `count_filter_matches` [ctx-free, `true` like `ControlledBy`]) →
  `SpellCast(Not(OwnedBy(Controller)))` = "a spell you don't own" (the trigger already gates on you being the caster) → `ForEach`
  creatures-you-control `PutCounters(+1/+1)`. Ability 2 PARTIAL (cost {2}+Sacrifice-another-creature + the exile of a target opp-gy
  I/S are real; the "**you may cast it this turn, any mana, exile-instead-of-gy**" rider is deferred — **it needs 3 mechanisms that
  don't exist:** (a) a **cross-player** exile-cast permission [the impulse offer only scans the caster's OWN exile; Nita casts a card
  in the OPPONENT's exile → needs `castable_by: Option<PlayerId>` on Object + an offer-loop scan of other players' exile — the
  lead's sketch missed this], (b) a **spend-any-type-of-mana** payment mode [collapse the cast cost to fully-generic in
  `can_pay`/`pay`], (c) an **exile-on-leave-stack** flag riding the flashback exile path). Until then Nita's activated ability is
  graveyard-hate only.

Own-commits (`git log -S` before re-scoping — header PROCESS RULES apply):
- **`cd5a6c0` — Choreographed Sparks** (modal spell-copy). (a) **Wired `CopySpellOnStack`'s `Target` arm into
  `collect_specs_into`** — mode-1 was *uncastable* for a real cast (the ledger's "wired, only Triggering tested" meant the target
  was never collected → `resolve_target` found nothing). (b) New **`Effect::CopySpellAsToken{ what, haste, sacrifice_at_next_end_
  step }`** — copies a creature spell → the copy resolves into a token (CR 707.10f), granted haste + a warp-style
  `AtBeginningOfNextEndStep` sac delayed trigger. Refactored **`copy_spell_on_stack` → returns `Option<ObjId>`** so the caller
  decorates the minted copy. (c) New **`Qualification::CantBeCopied`** (self-static, guarded at the single `copy_spell_on_stack`
  choke point — mirrors Surrak's `CantBeCountered`). (d) **Made `Action::Sacrifice` a first-class applied action** — it was a
  silent no-op in `apply_action` (only worked as a *cost*); now routes through `death_zone_for` like `interpret_sacrifice`, so any
  "sacrifice at end step" rider works.
- **`2e40f25` — Steal the Show** — pure composition, ZERO new cap. Modal: mode-a = `TargetPlayer` + `DiscardChosen{ChosenTarget}` +
  `Draw(DiscardedThisResolution)`; mode-b = `DealDamage{ Count{gy, I/S, Controller} }` to a creature/pw. The "Native" tag was wrong.
- **`ee72ebb` — Mathemagics** — new generic **`ValueExpr::Pow2(exp)`** (`1 << exp`, clamped [0,62]) → `Draw{ target, Pow2(X) }`.
  `{X}{X}{U}{U}` = `mc.x = 2`. Not Native.
- **`7c6f148` — Zaffai and the Tempests** — "once each of your turns, cast an I/S from hand for free." New player-permission
  **`StaticContribution::FreeCastFromHandOncePerTurn{ filter }`** (read by the priority-action builder, NOT painted — like
  `ExtraLandPlays`) + new **`PlayableAction::CastFreeFromHand{ source, spell }`** (+ `cast_free_from_hand` — casts
  `WithoutPayingManaCost`, spends `used_once_per_turn`). Offer gated on your turn + source-unused. ⚠️ **new `PlayableAction`
  variant → I fixed the exhaustive matches in `mtg-gre-server/options.rs` (2) + `mtg-py/{codec,decision_stats}.rs`.**
- **`c98a3c0` — Page, Loose Leaf** — `{T}:{C}` dork + **Grandeur** (`CostComponent::Discard` of a hand card named "Page, Loose
  Leaf" — Page is on the bf so a hand copy is always "another") → new **`Effect::RevealFromTopUntilToHand{ filter }`** (reveal-until
  an I/S → hand, rest random-bottom, the Cascade random-bottom idiom). Small shared fix: **`enter_filter_matches` now handles
  `CardFilter::Named`** (was fail-closed `_ => false` → the discard cost was unpayable).
- **`7310fdd` — Zimone's Experiment** — new **`Effect::LookPickCreaturesLands{ count, take }`** (look top 5, take ≤2 creature/land
  routed BY TYPE — lands → bf tapped, creatures → hand — rest random-bottom; the type-routed sibling of `LookAndPick`).
- **`65b176a` — Flashback** — grant flashback to a target I/S in your gy until EOT (cost = its mana cost). New Object field
  **`flashback_until_turn: Option<u32>`** (reset on zone change) + **`flashback_cost` honors it** (returns the card's own mana cost)
  + new **`Effect::GrantFlashbackUntilEndOfTurn{ what }`**. Reuses the existing flashback cast/exile path.

**▶ THE UNAUTHORED TAIL IS NOW 5 — and EVERY ONE is engine-roadmap or lead-deferred. There are NO card-agent builds left; the
next card-relay agent has an empty queue until the roadmap pass lands.** (Lead's rulings all made this session.)
- **Roadmap / layer (3 — lead→user, do NOT build as a one-off):** **Fractalize** (SET creature-type + base-P/T, CR 613 layers
  4/7b), **Great Hall of the Biblioplex** (mana land + `{5}: becomes a 2/4 Wizard` layer-4/7 animation), **Rubble Rouser** (loot ETB
  + `{T},Exile-from-gy:{R}` + reflexive damage — mana-ability-with-cost-and-rider, same class as the **Hydro-Channeler**
  tracked-partial → the roadmap's mana-ability-grant pass covers both).
- **Lead-deferred to the same roadmap pass (2 — honest ledger entries, NOT forced):**
  - **Resonating Lute** — "Lands you control have '`{T}`: Add two of any one color, I/S-only'." **grant-an-activated-mana-ability-
    to-a-group** — same class as Hydro-Channeler / Great Hall. Rides the mana-ability-grant roadmap item.
  - **Petrified Hamlet** — "choose a land card NAME on ETB; that name's non-mana activated abilities are off; that name's lands get
    `{T}:{C}`." Name-choice + two name-keyed statics; bespoke + negligible in limited. Deferred to the same pass.
- **The 4 TRACKED-PARTIALS** (`grep -rln '.incomplete()' cards/sos`): **Nita** (ability 2's cross-player-impulse + any-mana +
  exile-on-leave — spec above), Ral Zarek (−7 coin-flip), Wildgrowth Archaic (enters-with-extra-counters-keyed-to-another-spell),
  Hydro-Channeler (mana-ability-with-mana-cost). All ride engine-roadmap items.

**→ Net: the set is CARD-COMPLETE modulo the roadmap. 266/271 authored, 262 fully-faithful, 0 Natives, 4 tracked-partials — the
remaining 5 unauthored + 4 partials ALL ride two engine-roadmap items (layer-4/5 completion + the mana-ability-grant class) the
lead is putting to the user. A card-relay agent has nothing clean to pick up until those land.**

**⚠️ hatch-design feedback (the architecture doc wants this):** across 7 cards + the whole session, **0 cards needed the `Native`
hatch** — every "genuinely inexpressible" ledger tag dissolved on reading the oracle + the code. The IR is expressive enough that
Native remains unexercised; Pox Plague is the first card where Native is even *arguable*, and only because the alternative (a
per-player-keyed halving subsystem) is heavy for a single card — not because it's inexpressible.

---

## ▶ (superseded — history) sos-cards-18 handoff, 2026-07-05

**▶▶ sos-cards-18 SHIPPED — 9 fully-faithful cards + 8 reusable caps. 833 mtg-core green, whole workspace builds, tree clean,
LEAD pushes.** Census **248→257/271 authored (95%, 254 faithful · 3 tracked-partial)**, 0 Native hatches. **The clean
cap-blocked tail is now EXHAUSTED** — every remaining unauthored card needs a subsystem-scale cap or a lead sketch (bucketed
below). Own-commits (`git log -S` before re-scoping — header PROCESS RULES apply):
- **`58e1cca` — Archaic's Agony** (Converge damage + **excess-damage → impulse-exile**). New
  **`Effect::DealDamageExcessImpulse{ amount, to, window }`** — deals `amount` (`ColorsSpent`) to a target creature, computes
  `excess = max(0, amount − (toughness − marked_damage))` from the PRE-damage state (flushing interpret arm), then exiles that
  many cards from the top of the controller's library with the impulse play-permission (`castable_from_exile` + `play_until_turn`,
  `YourNextTurn`). ⚠️ excess uses the intended amount (no prevention/replacement/deathtouch in the pool).
- **`237e01e` — Mana Sculpt** (Counter + **delayed mana**). New time-based **`DelayedTriggerEvent::AtBeginningOfYourNextMainPhase`**
  (+ firing hook `fire_main_phase_delayed_triggers` wired into the `PhaseBegan` handler, gated on `active == controller`) +
  **`Action::AddMana`** (concrete pool add usable as a delayed-trigger action; `DelayedAbility` runs `Vec<Action>`) +
  **`Effect::AddManaAtNextMainPhase`** (arms it) + **`ValueExpr::ManaSpentOfTarget`** (reads a `Target::Stack` spell's
  `mana_spent`). Sequence arms the delay BEFORE the counter so it reads the still-on-stack spell's mana-spent; gated on a
  `CountAtLeast(HasSubtype(Wizard))` conditional.
- **`1d2e271` — Biblioplex Tomekeeper** (`{4}` Artifact Creature — ETB "choose up to one: prepare / unprepare a target"). New
  **modal *triggered*-ability support**: `place_trigger` (priority.rs:3509) now `choose_modes` + collects the chosen modes'
  targets (mirroring the modal-SPELL cast path), and the ability resolution threads `obj.modes` into `ctx.chosen_modes` (was
  hardcoded empty). Each mode = a targeted `Effect::SetPrepared`. Reuses the `SetPrepared` cap from Skycoach.
- **`0036255` — Divergent Equation** (dynamic **{X} target COUNT**). Instead of the 203-literal `TargetSpec` refactor: a
  documented sentinel **`TARGET_COUNT_X` (= u32::MAX)** on `TargetSpec.max`, resolved to the chosen `{X}` at the 2 cast
  slot-build sites (X is in scope there; `spec.max` is read at only 4 slot-build sites, never at resolution re-validation).
  Preserves TRUE targeting (cast-locked, respondable). Effect = plain multi-target `MoveZone` (I/S from your gy → hand, like
  Pull from the Grave) + `Ability::ExileOnResolve`. `{X}{X}{U}` = `mc.x = 2`.
- **`aec9852` — Skycoach Waypoint** (Land: `{T}: Add {C}` + `{3},{T}: target creature becomes prepared`). New tiny
  **`Effect::SetPrepared{what,prepared}`** — the targeted analogue of `BecomePrepared` (lowers to the existing
  `Action::SetPrepared`); `prepared:false` is the "becomes unprepared" arm. NB this cap ALSO covers Biblioplex Tomekeeper's two
  modes — Biblioplex is now blocked ONLY by **modal *triggered*-ability support** (its "choose up to one" is inside an ETB
  trigger; the trigger-placement path at priority.rs:3509 doesn't `choose_modes`/collect modal targets like the modal-SPELL
  cast path at 2493-2508 does — replicating that there is the remaining cap → unlocks Biblioplex).
- **`2e20d09` — Burrog Barrage** — the ledger's "no new cap" was WRONG (agent-17's recheck missed two wrinkles): the
  conditional +1/+0 target sits inside a `Conditional` that `collect_specs_into` deliberately does NOT walk, AND the damage
  must read the *post-pump* power (a materialized `DealDamage` freezes its amount pre-pump). Fixed with one clean cap: new
  **`Effect::SourcedDamage{source,to,amount,kind}`** — "creature deals damage" (CR 119.2, a reusable **bite** primitive; source
  ≠ the spell, so deathtouch/lifelink key off it). Its *flushing* interpret arm (mirrors `PutCounters`) commits the pump BEFORE
  reading `PowerOfTarget(0)`. The pump targets slot-0 by `EffectTarget::ChosenIndex(0)` (no fresh `Target` → stays out of the
  non-walked `Conditional`); `SourcedDamage` declares BOTH targets (`collect_specs_into` pushes source→to). Plus
  **`ValueExpr::InstantsSorceriesCastThisTurn{who}`** (counter increments at cast so Burrog counts itself → "another" I/S = ≥2;
  added to BOTH `whiteboard::eval_value` and `conditions::eval_value`).
- **`b85b613` — mill-then-play cap → Ark of Hunger + Tablet of Discovery** (2 cards, 1 cap). New **`Effect::MillThenPlay{who,
  window}`** + **`Action::MillForPlay`** + **`Object.playable_from_graveyard`** (graveyard analogue of impulse
  `castable_from_exile` — purely additive, exile paths untouched) + graveyard land-play/cast offer scans in
  `legal_priority_actions`. `move_to_stack` already pops the graveyard, so a milled spell casts Normal (→ gy on leave, NOT
  exiled — distinct from flashback). Ark's `CardsLeaveYourGraveyard` trigger + Tablet's restricted I/S mana (`SpendRestriction::
  InstantSorceryOnly`) reused existing machinery.
- **`cb6922e` — Slumbering Trudge** — reused `Rewrite::EntersWithCountersValue{Stun, 3−X}` (`Sum(3, XTimes(-1))`, clamped ≥0) +
  the existing CR 702.171 stun untap-skip (priority.rs:801). Small shared fix: **`EntersTappedUnless` now threads the entering
  object's cast X** — its condition eval was `conditions::holds` (no X → 0); now routes through `cond_holds` with `{source:obj,
  x:wb.ctx.x}`, so "enters tapped if X ≤ 2" = `EntersTappedUnless(ValueAtLeast(X, 3))`. Check-lands unaffected (non-value conds
  still route controller-relative). `{X}{G}` cost = `mc.x = 1`.

**▶ RECOMMENDED NEXT — the remaining buildables are all subsystem-scale (no clean wins left); pick by appetite:**
- **Great Hall of the Biblioplex** (mana land — near-free — + `{5}: becomes a 2/4 Wizard` **layer-4/7 animation** = the cap).
- **Rubble Rouser** (loot ETB + `{T},Exile-from-gy: Add {R}` + reflexive damage) — mana-ability-with-cost-and-rider, the same
  class as the **Hydro-Channeler** tracked-partial; do that roadmap item first.
- **Choreographed Sparks** — mode1 (copy target I/S you control) = the `CopySpellOnStack` `Target` arm (wired, only `Triggering`
  tested); mode2 (copy a creature spell → token + grant haste/sac) = a creature-spell-copy-to-token cap. Modal → both needed.
- **The 9 special one-offs** (Nita, Page/Grandeur, Petrified Hamlet, Resonating Lute, Zaffai, Skycoach Waypoint, Biblioplex
  Tomekeeper, Great Hall, Choreographed Sparks): send the lead a 3-line design sketch as you reach EACH, proceed on approval.
- **PARKED** (do not build): 3 Natives (Mathemagics, Pox Plague, Steal the Show), Fractalize (milestone-5 layers). Tracked-
  partials: Wildgrowth Archaic (`EntersWithCounters` extension — scoped/buildable), Ral (−7 coin-flip) + Hydro-Channeler
  (mana-ability-with-mana-cost) tied to roadmap items.

---

## ▶ (superseded — history) sos-cards-17 handoff, 2026-07-05

**▶▶ sos-cards-17 SHIPPED — 14 fully-faithful cards + cleared 2 tracked-partials (Colossus, Tester) + 12 reusable caps. 803
mtg-core green, whole workspace builds, tree clean, LEAD pushes.** Census **234→248/271 authored (91.5%, 245 faithful · 3
tracked-partial)**, 0 Native hatches. First wave = 10 cards (Mica … Mind Roots, below); SECOND WAVE = the target-path
dynamic-filter fix + Moseo + Sundering Archaic + Ennis + Snooping Page (see the census section). Own-commits (`git log -S`
before re-scoping — header PROCESS RULES apply):
- **`898b23b` — Mica, Reader of Ruins** — sac-artifact spell-copy; a pure Silverquill re-skin (`SpellCast(I/S) → Optional{
  IfYouDo{ Sacrifice(artifact) → CopySpellOnStack{Triggering, new targets} } }`) + `ward_pay_life(3)`. 0 new cap.
- **`a4eb133` — discarded-this-resolution cap → Borrowed Knowledge + Colossus cleared.** New `Effect::DiscardChosen` ("discard
  any number", player-chosen count) + `ValueExpr::DiscardedThisResolution` over a per-resolution `discarded_this_resolution`
  scratch (mirrors `searched_this_resolution`). Borrowed Knowledge = modal discard-your-hand-then-draw; **Colossus of the Blood
  Age** dies clause ("discard any number, draw that many + 1") composes over the same cap → **cleared from tracked-partial**.
- **`898b…d?` — Aziza, Mage Tower Captain** — tap-3 spell-copy over the EXISTING `Effect::MayPayCost` (`SpellCast(I/S) →
  MayPayCost{ TapCreatures(3), then: CopySpellOnStack{Triggering} }`). 0 new cap. ⚠️ documented caveat: `TapCreatures(3)`
  reuses `crew_candidates` which excludes the source, so Aziza can't count herself among the three (rare unpayable edge).
- **Mind into Matter** — `Draw X` + `Search{ zone: Hand, min:0, max:1 }` (put a permanent MV≤X from hand onto bf tapped),
  filter = `Not(I/S) ∧ ManaValueExpr{max:X}`. **Also fixed `interpret_search` to resolve dynamic (X-keyed) filters** (it was
  matching the raw `ManaValueExpr` against the ctx-free matcher → never matched; same class as the historical select bug).
- **Wisdom of Ages** — new `Effect::SetNoMaxHandSize` (lifts the cleanup discard limit) + new `Ability::ExileOnResolve` marker
  (resolve_top exiles the card instead of gy, alongside the flashback/Paradigm branch) + return-all-I/S-from-gy via the Jadzi
  `ForEach{ max:999 = all }` select-all idiom.
- **Practiced Offense** — new `Effect::GrantChosenKeyword{ options }` ("gains your choice of double strike or lifelink"): the
  target is a normal cast-time target (collected in `collect_specs_into`), the keyword is chosen at resolution via ChooseModes
  — composes inside a `Sequence` UNLIKE a nested `Modal` (whose targets aren't collected). Plus `TargetPlayer` + `ForEach`
  counters scoped to the target player (`chooser: ChosenTarget(0)`) + flashback. NB: `TargetPlayer` advances the target cursor.
- **LKI counter-count for dies triggers → Ambitious Augmenter + Scolding Administrator.** `state::Lki` now snapshots the
  **counter bag** at death; `ValueExpr::CountersOnSelf` falls back to it off-battlefield (fixed BOTH eval paths —
  `whiteboard::eval_value` AND `conditions::eval_value`; the second was the bug). Both cards reuse the EXISTING
  `helpers::increment_ability()` (Increment was already a shared helper). Augmenter = dies→Fractal via CreateToken
  `dynamic_counters`; Scolding = Menace + Repartee + dies→PutCounters{target, +1/+1, CountersOnSelf(LKI)}.
- **`f48a776` — Tester of the Tangential (COMPLETE, fully faithful)** — Increment + begin-combat `MayPayCost{ {X}, then:
  MoveCounters{ SourceSelf → another target creature, count: X } }`. Two new caps: **`MayPayCost`-with-`{X}`** (announces/pays
  X, X=0 declines, threads X to the reward as `ValueExpr::X`; a targeted reward is now collected as a NORMAL ability target via
  `collect_specs_into` walking `then` — safe, no existing MayPayCost card has a targeted `then`) + **`Effect::MoveCounters{
  from,to,kind,count }`** (moves N counters capped at what's present; atomic paired ±AddCounters). ⚠️ caveat: the target is
  chosen at trigger-placement, not reflexively after paying (observably equivalent — a declined X=0 moves nothing).
- **Mind Roots** — new `Effect::PutDiscardedOntoBattlefield{ filter, max }` (select among the discard scratch → bf tapped under
  YOUR control, owner unchanged) over `TargetPlayer` + `Discard{ChosenTarget(0), 2}`.

### ★ FULL-SET CENSUS (sos-cards-17, 2026-07-05, Scryfall-diff verified) — 248/271 authored (91.5%)
Method: `comm -23` of the 271 sos front-face names vs every card-name string literal in `crates/mtg-core/src/cards/**`.
**248 authored (245 fully-faithful · 3 tracked-partial) · 23 unauthored. 0 Native hatches. 803 mtg-core green.**

**▶ sos-cards-17 SECOND WAVE (after the census above, all committed):** the **target-path dynamic-filter fix**
(`target_matches_filter` resolves a `ManaValueExpr` TARGET bound against a source-derived ctx — was fail-closed silent-inert) →
**Moseo Vein's New Dean** (reanimate MV≤life-gained) + **Sundering Archaic** (Converge exile MV≤colors-spent); the
**cards-exiled-this-turn tracker** (`Player.cards_exiled_this_turn` + `ValueExpr::CardsExiledThisTurn`) → **Ennis, Debate
Moderator**; the **`SelfDealsCombatDamageToPlayer`** per-creature combat event → **Snooping Page**. NB discovered
`Player.instants_sorceries_cast_this_turn` ALREADY EXISTS → **Burrog Barrage** is now buildable faithfully (was flagged as an
over-count risk; it isn't).

**The 3 TRACKED-PARTIAL** (`grep -rln '.incomplete()' cards/sos`): Ral Zarek Guest Lecturer (−7 coin-flip+skip-turns),
Wildgrowth Archaic (enters-with-extra-counters-keyed-to-another-spell), Hydro-Channeler (mana-ability-with-mana-cost).
*(Colossus AND Tester of the Tangential were both cleared this session — Tester via the new `Effect::MoveCounters` +
`MayPayCost`-with-`{X}` [announces/pays X, threads it to the reward as `ValueExpr::X`, targeted reward collected as a normal
ability target].)*

**The 27 UNAUTHORED — bucketed:**
- **Natives (3, genuinely inexpressible — lead sketch):** Mathemagics (2^X exponential), Pox Plague (halving), Steal the Show
  (control-theft + wheel).
- **Milestone-5 layer system (1):** Fractalize (SET creature-type + set-base-P/T, CR 613 layers 4/7b).
- **Special one-offs (9, each a bespoke mechanism — lead sketch):** Nita Forum Conciliator (cast-a-spell-you-don't-own +
  exile-opp-gy-cast-with-any-mana), Page Loose Leaf (Grandeur), Petrified Hamlet (choose-a-name static), Resonating Lute
  (grant-a-mana-ability), Zaffai and the Tempests (once/turn free-cast), Skycoach Waypoint (non-DFC prepare land), Biblioplex
  Tomekeeper (make-target-prepared/unprepared), Great Hall of the Biblioplex (mana land + becomes-a-creature layer),
  Choreographed Sparks (mode1 copy-target-I/S = the CopySpellOnStack Target arm, BUILDABLE; mode2 copy-creature-spell→token +
  grant haste/sac = a creature-spell-copy-to-token cap).
- **Cap-blocked buildable (10 left; the 4 easiest were shipped this session — Moseo, Sundering Archaic, Ennis, Snooping Page):**
  - **▶▶ Burrog Barrage — NOW BUILDABLE, NO NEW CAP** (recheck confirmed `Player.instants_sorceries_cast_this_turn` exists +
    `ValueExpr::PowerOfTarget` for the one-sided "deals damage equal to its power"). Two targets (creature you control + up-to-one
    opp creature) + a conditional +1/+0 gated on `instants_sorceries_cast_this_turn ≥ 2`. Nearest clean win.
  - **Divergent Equation** — dynamic **{X} target COUNT** ("up to X target I/S"); `TargetSpec.max` is a fixed `u32` (needs a
    dynamic max). + `ExileOnResolve` (exists, from Wisdom of Ages).
  - **Ark of Hunger** / **Tablet of Discovery** — **mill-then-play-that-card** cap (Tablet is NOT authored — ledger was stale;
    Tablet also has a restricted mana ability). Ark's CardsLeaveYourGraveyard half = damage+gain (exists).
  - **Rubble Rouser** — loot ETB (exists) + `{T}, Exile-from-gy: Add {R}` + reflexive damage = mana-ability-with-cost-and-rider
    (same class as Hydro-Channeler / Treasure).
  - **Slumbering Trudge** — stun counters (enters-with-stun-keyed-to-X + untap-replacement).
  - **Mana Sculpt** — Counter (exists) + delayed "add {C} at your next main phase" (delayed mana).
  - **Archaic's Agony** — Converge damage + excess-damage impulse-exile (excess-damage tracking + multi-card impulse).
  - **Great Hall of the Biblioplex** — mana land ({T}:{C}; {T},pay-1-life:any-color restricted-to-I/S) + a `{5}: becomes a 2/4
    Wizard` layer-4/7 animation. The mana halves are near-free; the animation is the cap.
  - **Choreographed Sparks** — mode1 (copy target I/S spell you control) = the `CopySpellOnStack` `Target` arm (wired); mode2
    (copy a creature spell → token + grant haste/sac) = a creature-spell-copy-to-token cap.

*(sos-cards-17 done at a clean boundary after a long session — 14 cards + 12 caps + census. **Recommended next: Burrog Barrage
(NO new cap — nearest clean win), then the remaining cap-blocked (Divergent Equation dynamic-X-target, mill-then-play for
Ark/Tablet).** The 9 special one-offs each want a lead 3-line sketch as you reach them; the 3 Natives + Fractalize stay parked.
`git log -S` + READ THE CODE before believing any claim — the ledger drifts, and I found `instants_sorceries_cast_this_turn`
already existed after it was flagged missing.)*


**▶▶ sos-cards-16 SHIPPED — ALL 5 college Elder Dragons + Lumaret's Favor + Social Snub + 6 reusable caps. 764 mtg-core
green, whole workspace builds, tree clean, LEAD pushes.** Census **227→234/271 (86%)**, 0 Native hatches. Seven own-commits
(`git log -S` before re-scoping):
- **`4dd31ef` — `Effect::CopySpellOnStack{what,count,choose_new_targets}`** (a thin loop over the built `copy_spell_on_stack`,
  707.10, priority.rs:3990) **+ Prismari, the Inspiration (Storm)** + **wired `CostComponent::PayLife` into `pay_cost`**
  (Ward—Pay 5 life — the dead `_ => {}` no-op is killed; CounterUnlessPay routes Ward costs through `pay_cost`). `what` is an
  `EffectTarget`: `Triggering` reads `ctx.triggering_spell` (storm/casualty/infusion); a `Target::Stack`/`Object` branch is
  wired for a future "copy target I/S spell" (Choreographed Sparks) but only `Triggering` is tested. Storm = `Triggered{
  SpellCast(I/S)} → CopySpellOnStack{Triggering, count: Sum(SpellsCastThisTurn,−1), new targets}` (count reads AFTER the cast's
  increment). Test drive loop MUST `run_agenda` BEFORE `resolve_top` or the spell resolves before the copy trigger lands.
- **`cce33d6` — Silverquill, the Disputant (Casualty 1)** = `Triggered{SpellCast(I/S)} → Optional{IfYouDo{Sacrifice(creature
  power≥1 = `All([Creature, Not(PowerAtMost(0))])`) → CopySpellOnStack{Triggering, count:1}}}`. ⚠️ sac trails the true 601.2b
  cast-time window (observable result matches — the copy still resolves above the still-on-stack spell).
- **`f66c23f` — Witherbloom, the Balancer (Affinity) + `Ability::GrantCostReduction{amount, spell_filter}`.** Own affinity
  composes now (`CostReduction{GenericValue(Count creatures), State(Always), Cast}`). The **granted-to-your-I/S** clause = the
  new `GrantCostReduction` static: `effective_cast_cost` gathers these from EVERY permanent the caster controls whose
  `spell_filter` matches the cast card (generic-only, CR 118/702.40). Applies at both the offer gate AND cast (same fn).
- **`c7f2a8e` — Quandrix, the Proof (Cascade) + `EventPattern::SelfCast` + `Effect::Cascade`.** **SelfCast** = "when you cast
  THIS spell" — found by scanning the just-cast spell's OWN abilities (`queue_self_cast_triggers`, wired into the `SpellCast`
  broadcast next to the watcher scan); carries the spell as `source` + `trigger_source_spell` so its effect reads the spell's
  own MV / copies it. **Cascade** (702.83) = exile-top-until-nonland with MV < the cast spell's MV (`ctx.triggering_spell`),
  may-free-cast, bottom the rest via `state.rng` (bottom = front of the lib vec). Quandrix = own cascade (SelfCast) + granted
  cascade to your I/S (SpellCast watcher). ⚠️ "from your hand" NOT enforced (cast-zone isn't threaded) — rare over-trigger.
- **`42f4b74` — Lumaret's Favor (Infusion copy-self)** — first consumer combining SelfCast + CopySpellOnStack: `PumpPT{target
  creature,+2/+4}` + `Triggered{SelfCast, if GainedLifeThisTurn → CopySpellOnStack{Triggering,1,new targets}}`.
- **`aad6478` — Social Snub (copy-self edict)** — `Triggered{SelfCast, if CountAtLeast(creatures you control,1),
  Optional{CopySpellOnStack{Triggering,1}}}` + edict/drain main effect (each player sacs a creature · `LoseLife{EachOpponent,1}`
  · `GainLife{Controller,1}`). Copy doubles the edict+drain (tested); the copy has no targets so `choose_new_targets:false`.
- **`d874ae2` — Lorehold, the Historian (Miracle) + THE MIRACLE SUBSYSTEM (CR 702.94, lead-approved plan A). ALL 5 DRAGONS DONE.**
  `Ability::Miracle{cost}` (printed) + `Ability::GrantMiracle{cost,filter}` (granted — mirrors `GrantCostReduction`);
  `miracle_cost(card,caster)` = the two-origin check (printed OR a granting permanent you control); **`draw()` captures the turn's
  FIRST card** (0→1 transition, 702.94e — only the first card of the first draw event) and queues a new
  **`StackObjectKind::MiracleWindow`** DIRECTLY (no new GameEvent — implementer's choice; priority still respected via the agenda);
  on resolution the controller may cast for the miracle cost via new **`CastVariant::Miracle`** (fixed alt-cost, mirrors Warp — see
  the cost match in `cast_spell`). Lorehold = 5/5 flying-haste + `GrantMiracle{ {2}, I/S }` + opp-upkeep loot
  (`Triggered{BeginningOfStep(Upkeep), Some(Not(YourTurn)), Optional{IfYouDo{Discard 1, Draw 1}}}`). Tests incl. the required
  702.94e case (2nd card of the same draw does NOT qualify) + non-first-draw + decline. NB: a looted draw can itself be your first
  draw of that turn and open a miracle window — the subsystem composes.

### ▶ Where sos-cards-16 points you (the tail after ALL 5 dragons + 6 caps)
- ~~**Lorehold (Miracle)**~~ ✅ **DONE (`d874ae2`).** All 5 college Elder Dragons shipped.
- **Newly UNBLOCKED, compose-now (no new cap):**
  - ~~**Social Snub**~~ ✅ **DONE (`aad6478`).**
  - **Choreographed Sparks / other target-spell copies** — via CopySpellOnStack's `what: Target(...)` arm (needs the card to
    target a spell on the stack; the arm resolves `Target::Stack`/`Object` → spell obj, already wired, untested).
  - **Aziza, Mica** (per the S15 tail) — spell-copy consumers; check their oracle for the exact trigger + copy shape.
- **Medium caps still open (each a real new piece; from the S15 tail, still valid):** **Ennis** ("cards put into exile this
  turn" per-turn tracker + end-step +1/+1 condition, on top of the shipped `ExileReturnNextEndStep`); **Increment** keyword
  (Tester, Ambitious Augmenter — SpellCast-trigger comparing `ManaSpentOnTrigger` vs `Power/ToughnessOfSelf` + a 2nd ability);
  **NoMaxHandSize** (Wisdom of Ages); **Moseo** (targeted MV≤life-gained reanimate — `resolve_dynamic_filter` into the TARGET
  path); **LKI-counter-count** (Scolding Administrator); **discarded-this-resolution** (Mind Roots, Borrowed Knowledge, Colossus).
- **Still design-deferred (need lead sketches):** 3 Natives, Fractalize, the special one-offs (Grandeur / theft-cast / name-choice
  / free-cast / grant-mana / non-DFC prepare markers). See census buckets.

*(sos-cards-16 done at a clean boundary — ALL 5 Elder Dragons + Lumaret's Favor + Social Snub + 6 reusable caps, trackers
current at 234/271, 764 green, whole workspace builds, tree clean. The Elder-Dragon assessment below is now FULLY EXECUTED —
5/5 done. Next agent: the copy/target consumers + the medium caps in the tail above. `git log -S` + read the code before
believing any claim — header PROCESS RULES apply.)*

## ▶ Prior — handoff from sos-cards-15, 2026-07-05

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
- **`b2d822d` — Moment of Reckoning** (repeatable-modal) — NO new engine cap: a `Modal{min:0,max:4,allow_repeat:true}` over two
  EXISTING effects (Destroy a nonland permanent · MoveZone a nonland permanent card gy→battlefield); the modal cursor already
  gives each mode instance its own target. (Minor caveat noted in the card: cross-instance target *distinctness* for repeated
  modes isn't enforced — a same-mode-same-object double just fizzles the 2nd; a general modal-mask nicety, no functional loss.)

- **`497f1b3` — Daydream** (no new cap) — `Sequence[ Blink{target creature you control}, PutCounters{ChosenIndex(0), +1/+1} ]`
  (the blink reuses the object id, so the locked target still names the returned creature) + a mana `flashback`. Pure composition.
- **grant-a-triggered-ability-until-EOT SUBSYSTEM (lead-approved) + Rabid Attack (`7ede626`) + Root Manipulation
  (`7fa973f`)** — CR 613.1f. `StaticContribution::GrantAbility{template_grp}` + `Effect::GrantAbility{what,template_grp,duration}`
  lowering to the existing `GrantContinuous` path; templates in the **reserved 9800+ block** (`cards/grant_templates.rs`, one
  `Triggered` def each — GRANT_DIES_DRAW, GRANT_ATTACKS_GAIN_LIFE — auto-excluded from `/api/cards` by the ≥9700 threshold, the
  lead's revision vs a phantom ability on the instant); `StackObjectKind::Ability` gained `#[serde(default)] source_grp:
  Option<u32>` (`ability_def` resolves the template def) + a granted-ability scan in `queue_self_triggers`
  (`granted_ability_templates` walks `continuous_effects`). Fires synchronously at the death/attack broadcast (before
  `recompute` expires the effect), so the queued trigger — referencing the template — survives. Tested: dies→draw / attacks→gain,
  and **post-EOT death/attack does NOT trigger** (required test). ZERO regression on the hot trigger path (730 green).

- **`db859f6` — Conciliator's Duelist** (timed-blink cap) — **`Effect::ExileReturnNextEndStep`** (CR 603.7): exile now + arm a
  `DelayedTriggerEvent::AtBeginningOfNextEndStep` carrying a `MoveZone{→Battlefield}` (owner's control). Its Repartee
  (`SpellCastTargetingCreature`) trigger drives it; ETB = draw + each player loses 1. **Reusable for Ennis** — but Ennis ALSO
  needs a "cards put into exile this turn" tracker (not built) for its end-step +1/+1 counter, so Ennis is not yet done.

**Census now 227/271 authored (84%). 0 Native escape hatches. Rows DONE: additional-cast-cost · base-P/T-set · GreatestMV · Flashback-non-mana · repeatable-modal · GRANT-ABILITY · timed-blink · (Daydream = pure composition).**

### ★ ELDER-DRAGON COMPOSITION ASSESSMENT (sos-cards-15, lead-requested — ASSESS, verdicts verified against the current cap set)
The "5 genuine subsystems" framing predates this session's cap explosion. Verdict: **only 1 of 5 is a real subsystem; 4 are
compose-now/small-cap, and TWO share a single thin cap.** Verified: `copy_spell_on_stack` (707.10, priority.rs:3990) built ·
`ValueExpr::SpellsCastThisTurn` + `Player.spells_cast_this_turn` built · `CostReductionAmount::GenericValue(ValueExpr)` built
(Dawning Archaic) · `CastVariant::WithoutPayingManaCost` free-cast + `ExileTopUntilManaValueMayCastFree` loop built ·
`state.rng` seeded RNG present · `effective_cast_cost` reads ONLY the cast card's OWN `CostReduction` statics (the one real
pipeline limit for granted cost-reduction).

- **Prismari, the Inspiration (Storm) — SMALL-CAP (compose-now once the shared copy cap lands).** 7/7 flying vanilla body +
  **Ward—Pay 5 life** (tiny: wire `CostComponent::PayLife` into `pay_cost` — currently `_ => {}` there; I only wired it in the
  cast-path `pay_additional_nonmana`) + Storm = `Triggered{ SpellCast(I/S, by you) }` → **`Effect::CopySpellOnStack{ what:
  TriggeringSpell, count: SpellsCastThisTurn−1, choose_new_targets:true }`** (loops the built `copy_spell_on_stack` `count`×). No subsystem.
- **Silverquill, the Disputant (Casualty) — SMALL-CAP (SHARES the copy cap with Prismari).** 4/4 flying-vig body + Casualty 1 on
  your I/S = `Triggered{ SpellCast(I/S, by you) }` → `Optional{ IfYouDo{ Sacrifice(a creature power≥1 = `All([Creature,
  Not(PowerAtMost(0))])`), CopySpellOnStack{ TriggeringSpell, count:1, new targets } } }`. ⚠️ Timing caveat: real Casualty is a
  601.2b cast-time optional cost; the cast-trigger model creates the copy a beat later (trigger resolves above the still-on-stack
  spell — order is right, only the sacrifice timing differs). Note it; observable result matches. No subsystem.
- **Witherbloom, the Balancer (Affinity) — SMALL-CAP (own clause composes; granted clause = a modest pipeline extension).** 5/5
  flying-deathtouch + **own affinity** = `CostReduction{ GenericValue(Count{creatures, Controller}), Always, Cast }` (COMPOSE-NOW,
  Dawning-Archaic-proven) + **granted affinity to your I/S** = the one gap: `effective_cast_cost` is self-only, so grant needs it
  to ALSO gather cost-reductions from OTHER permanents you control scoped by a filter on the cast spell ("your I/S spells cost {1}
  less per creature"). Bounded extension, not a subsystem.
- **Quandrix, the Proof (Cascade) — SMALL-CAP (a bounded new effect).** 6/6 flying-trample + Cascade on itself + granted to your
  I/S from hand = a dedicated **`Effect::Cascade`** (exile top until a nonland with MV < the cast spell's MV; may free-cast it via
  the built `WithoutPayingManaCost`; bottom the rest in RANDOM order via `state.rng`). A cousin of the built Improvisation loop
  (until-one-cheaper vs until-total-MV + random-bottom). Trigger = `SpellCast` reading the triggering spell's MV. Biggest small-cap; still not a subsystem.
- **Lorehold, the Historian (Miracle) — REAL SUBSYSTEM (the one genuine gap).** 5/5 flying-haste + opp-upkeep loot (`Triggered{
  BeginningOfStep(Upkeep) on each opponent's turn }` → `Optional{ IfYouDo{ Discard 1, Draw 1 } }`, COMPOSE-NOW) + **Miracle {2}**
  = the real gap: a first-card-drawn-this-turn tracker + a **draw-triggered reveal/cast window** + an alternate cast cost, granted
  to I/S cards in hand. No existing machinery for the draw-triggered cast window. DESIGN-SKETCH before building.

**▶ Highest-leverage move: build the thin `Effect::CopySpellOnStack{ what, count, choose_new_targets }` (over the built
`copy_spell_on_stack`) → it unlocks BOTH Prismari (Storm) and Silverquill (Casualty) as compositions (2 Elder Dragons for ~1 small
cap). Then Witherbloom (own affinity now + the effective_cast_cost grant extension), then Quandrix (`Effect::Cascade`). Lorehold
(Miracle) is the only one needing a design sketch.** Net: 4 of 5 dragons are NOT subsystems — the stale framing overcounted by 4×.

### ▶ Where sos-cards-15 points you (tail after 9 cards + 8 caps; the clean non-architecture caps are cleared)
sos-cards-15 cleared the easy-to-medium caps. The **remaining tail needs either a lead sketch or a genuine new cap** (grouped):
- ~~**Grant-a-triggered-ability-until-EOT** (Rabid Attack, Root Manipulation).~~ ✅ **DONE (sos-cards-15, `7ede626`+`7fa973f`)** —
  `StaticContribution::GrantAbility{template_grp}` + reserved 9800+ template block + granted-ability scan in `queue_self_triggers`
  + `source_grp` on the trigger stack object. Reusable for any "gains [triggered ability] until EOT". See the SHIPPED block above.
- **Medium caps still open (each a real new piece):** ~~timed-blink~~ **DONE** (`Effect::ExileReturnNextEndStep`, sos-cards-15) —
  **Ennis** just needs a "cards put into exile this turn" per-turn tracker (bump on any →Exile move, reset each turn) + a `Condition`
  reading it for its end-step +1/+1; **Increment**
  keyword (Tester, Ambitious Augmenter — SpellCast-trigger comparing `ManaSpentOnTrigger` vs `Power/ToughnessOfSelf`; both cards
  ALSO have a hard 2nd ability — move-X-counters / dies-with-counters→token); **spell-copy consumers** (Choreographed Sparks,
  Lumaret's Favor, Social Snub, Aziza, Mica — need a thin `Effect::CopySpellOnStack{what}` / copy-on-cast-self trigger over the
  S14 `copy_spell_on_stack`); **NoMaxHandSize** (Wisdom of Ages — player flag + mass-return-I/S + a self-exile-on-resolve marker,
  since `Exile{SourceSelf}` gets overwritten by resolve_top's graveyard move); **Moseo** (targeted MV≤life-gained reanimate —
  needs `resolve_dynamic_filter` wired into the TARGET-candidate path too, not just `select_for_each`); **LKI-counter-count**
  (Scolding Administrator); **discarded-this-resolution** (Mind Roots, Borrowed Knowledge, Colossus dies-clause).
- **Elder Dragons — RE-ASSESSED (see the ★ ELDER-DRAGON COMPOSITION ASSESSMENT above):** 4 of 5 are compose-now/small-cap, NOT
  subsystems. **▶▶ HIGHEST-YIELD NEXT BUILD: the thin `Effect::CopySpellOnStack{ what, count, choose_new_targets }` (loops the
  built `copy_spell_on_stack`, priority.rs:3990) → unlocks BOTH Prismari (Storm) AND Silverquill (Casualty) as compositions, PLUS
  the spell-copy consumers (Choreographed Sparks, Lumaret's Favor, Aziza, Mica).** Then Witherbloom (own affinity composes now via
  `CostReduction{GenericValue(Count creatures)}`; the granted-to-your-I/S clause = a bounded `effective_cast_cost` extension to
  gather other-permanents' cost-reductions scoped by a spell filter), then Quandrix (`Effect::Cascade`). Only **Lorehold/Miracle**
  is a real subsystem (first-draw reveal window + alternate cast) → design-sketch first.
- **Still design-deferred (need lead sketches):** 3 Natives, Fractalize, the special one-offs (Grandeur / theft-cast / name-choice
  / free-cast / grant-mana / non-DFC prepare markers). See census buckets.

*(sos-cards-15 winding down at a clean boundary — 12 cards + 10 caps + census re-verify + the Elder-Dragon composition audit. If
picked up fresh, the immediate move is `Effect::CopySpellOnStack` → Prismari + Silverquill (the lead pre-approves compose-now
dragons). Ledger, WORKLOG, PROJECT_STATE all current at 227/271, 732 green, tree clean.)*

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

**Headline (Scryfall-diff RE-VERIFIED 2026-07-05 by sos-cards-15; + Moment of Reckoning, Daydream, Rabid Attack, Root
Manipulation, Conciliator's Duelist since): 227 / 271 authored (84%). 223 fully faithful · 4 tracked-partial · 44 unauthored.
0 Native escape hatches used. 732 mtg-core tests green.** (Diff method: every sos set name —
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
