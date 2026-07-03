# Card-implementation push — Secrets of Strixhaven (`sos`, 271 distinct cards)

Standing workstream: implement the Secrets of Strixhaven set for **limited (40-card) play** in
`mtg-core`, easiest-first, correctness over count. This ledger is the capability index + full
per-card triage, modeled on `SELESNYA_LANDFALL_CARDS.md`.

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

| Cap | What it adds | Cards | Status |
|---|---|---|---|
| **S1** Surveil N | look at top N, put any number in graveyard, rest back (CR 701.42) — `Effect::Surveil` | 15 | ✅ **DONE** `cc58a7b` |
| **S5** Opus | `SpellCast(I/S you control)` trigger + `ValueExpr::ManaSpentOnTrigger` + `≥5` condition | 13 | ✅ **DONE** `e85771e` |
| **S8** Repartee | `SpellCast(I/S you control **that targets a creature**)` trigger (inspect cast targets) | 12 | ✅ **DONE** |
| **S4** Infusion | per-turn per-player "gained life this turn" state + a `Condition` reading it | 12 | ✅ **DONE** `89b3581` |
| **S10** Flashback | alt-cast from graveyard for a flashback cost, then exile (Warp-analogue) | 11 | ⏳ |
| **S6** Increment | `SpellCast(you)` trigger + condition "mana spent > this creature's power OR toughness" | 9 | ✅ **DONE** |
| **S7** Converge | `ValueExpr::ColorsOfManaSpent` (ETB counters / X in Converge spells) | 9 | ⏳ |
| **S9** Graveyard-leave | "cards leave your graveyard" trigger + "a card left your graveyard this turn" cond | 8 | ✅ **DONE** (flag `f9b5584` + trigger: LeftGraveyard event snapshot in resolve_effect → Spirit Mascot, Owlin Historian, Garrison Excavator) |
| **S2** Look-and-pick | look at top N, put one/some in hand, rest on bottom (impulse selection) | 8 | ⏳ |
| **S12** Cost-reduction cond. | "costs {N} less if it targets X / you control Y / a card left your gy" (cast-time) | 7 | ⏳ |
| **S14** Copy spell/perm | "copy target spell", "create a token that's a copy of" (heavier small-cap) | 7 | ⏳ **token-copy DONE** (`Effect::CreateTokenCopy`+`TokenCopyMods`, `a8c8a2d` → Applied Geometry); **spell-copy** portion still ⏳ |
| **S17** Ward {cost} | Ward N / Ward—Pay life / Ward—Discard (counter-unless-pay on becoming targeted) | 7 | ⏳ |
| **S15** Impulse play | exile/mill → "you may play it until end of turn / your next turn" | 6 | ⏳ |
| **S3** Stun counters | `CounterKind::Stun` + "would untap → remove a stun counter instead" replacement | 6 | ⏳ |
| **S18** Graveyard-activated | an ability that functions while its card is in the graveyard (recursion) | 6 | ⏳ |
| **S11** Token-with-ability | `TokenSpec` carries an ability (Treasure `{T},Sac`; Pest attack→gain life) | 5 | ⏳ |
| **S13** Restricted mana | mana usable "only to cast instant and sorcery spells" (spend-restriction tag) | 4 | ⏳ |
| **S16** Gain-life trigger | `EventPattern::GainLife` ("whenever you gain life, …") | 3 | ✅ **DONE** |
| **S21** cast-with-{X} trigger | `SpellCast` filtered to "has {X} in its cost" | 2 | ⏳ |
| **S19/S20/S22** | cards-drawn-this-turn value / counters-on-target value / cast-I/S-this-turn cond | 1 ea | ⏳ |
| **misc one-offs** | GreatestMV, DistinctNames, SoftCounter (counter-unless-pay), DirectedDiscard, AltCost, PayXLife, NoMaxHand, GrantAbility | 1–3 ea | ⏳ |
| **Native** | genuine one-offs via the `Native` escape hatch: Mathemagics (2^X), Pox Plague (halving), Steal the Show (wheel) | 4 | ⏳ |

Building **S1, S4, S5, S6, S7, S8, S10** (the seven big-count caps) converts ~**79** T3 cards to authorable.

## Engine reality-check — unimplemented effect leaves (E-caps) — **found during Phase 2**

The Phase-1 rubric assumed several `Effect` variants were interpreted; grepping the whiteboard
interpreter (`whiteboard.rs`) shows **six IR leaves are defined but interpreted nowhere** — a card
using one silently no-ops. So the true near-term T2 pool is **smaller than the 68 tallied above**:
some of those cards actually need one of these leaves wired first. These are the highest-leverage
caps (each is a small, card-agnostic interpreter arm lowering to an already-existing `Action`).

| E-cap | Effect leaf | Blocks (examples) | Status |
|---|---|---|---|
| **E1** | `Effect::MoveZone` (bounce / return-to-hand / reanimate) | Zealous Lorecaster, Banishing Betrayal, Proctor's Gaze, Prismari Charm, Matterbending Mage, Pull from the Grave, Moment of Reckoning, Lorehold Charm | ✅ **DONE** `0e85b76` (single-target; multi-target "up to two" still TODO) |
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
| Artistic Process | - | `sos` | ⏳ | modal damage + flying token, all IR |
| Ascendant Dustspeaker | - | `sos` | ⏳ | flying, ETB counter, exile graveyard card |
| Bogwater Lumaret | - | `sos` | ✅ done | creature-ETB gain-life trigger, IR |
| Borrowed Knowledge | - | `sos` | ⏳ | modal discard hand, draw by count |
| Burrog Banemaker | - | `sos` | ✅ done | deathtouch + activated pump |
| Burrog Barrage | - | `sos` | ⏳ | conditional pump + power-based damage |
| Cauldron of Essence | - | `sos` | ⏳ | dies-drain + activated reanimation |
| Charging Strifeknight | - | `sos` | ⏳ | haste + tap/discard loot activated |
| Chase Inspiration | - | `sos` | ✅ done | pump + grant hexproof |
| Chelonian Tackle | - | `sos` | ✅ done | pump + fight up to one |
| Colossus of the Blood Age | - | `sos` | ⏳ | ETB drain + dies rummage draw |
| Cost of Brilliance | - | `sos` | ✅ done | draw, lose life, counter |
| Deathcap Glade | - | `vow` | ✅ done | checkland conditional tap + mana |
| Dina's Guidance | - | `sos` | ✅ done | search creature to hand/graveyard |
| Dissection Practice | - | `sos` | ✅ done | drain + pump modal, all IR |
| Divergent Equation | - | `sos` | ⏳ | X return instant/sorcery cards, exile self |
| Dreamroot Cascade | - | `vow` | ✅ done | checkland conditional tap + mana |
| Eager Glyphmage | - | `sos` | ✅ done | ETB Inkling keyword token |
| Embrace the Paradox | - | `sos` | ⏳ | draw three + put land from hand |
| Ennis, Debate Moderator | - | `sos` | ⏳ | blink ETB + conditional end-step counter |
| Environmental Scientist | - | `sos` | ✅ done | ETB search basic land to hand |
| Erode | - | `sos` | ✅ done (sos) | destroy + opponent fetches basic land |
| Essence Scatter | - | `m10` | ✅ done | counter target creature spell |
| Fractalize | - | `sos` | ⏳ | becomes Fractal, base P/T X+1 |
| Glorious Decay | - | `sos` | ⏳ | modal destroy/damage/exile-draw |
| Grapple with Death | - | `sos` | ✅ done | destroy artifact/creature, gain life |
| Harsh Annotation | - | `sos` | ✅ done | destroy; controller makes Inkling token |
| Heated Argument | - | `sos` | ⏳ | damage; optional graveyard-exile extra damage |
| Impractical Joke | - | `sos` | ✅ done | 3 damage up-to-one; prevention clause deferrable |
| Interjection | - | `sos` | ✅ done | pump plus first strike |
| Last Gasp | - | `rav` | ✅ done | -3/-3 to target creature |
| Lorehold Charm | - | `sos` | ⏳ | modal: opp-sac / reanimate MV<=2 / anthem |
| Mage Tower Referee | - | `sos` | ⏳ | multicolored-cast trigger self-counter |
| Masterful Flourish | - | `sos` | ✅ done | pump plus indestructible |
| Mind Roots | - | `sos` | ⏳ | discard two, put discarded land onto battlefield tapped |
| Mind into Matter | - | `sos` | ⏳ | draw X, put permanent from hand into play |
| Mindful Biomancer | - | `sos` | ✅ done | ETB gain life; once-per-turn pump |
| Moment of Reckoning | - | `sos` | ⏳ | modal choose-up-to-four destroy/reanimate |
| Noxious Newt | - | `sos` | ✅ done | deathtouch plus mana ability |
| Oracle's Restoration | - | `sos` | ✅ done | pump, draw, gain life |
| Planar Engineering | - | `sos` | ✅ done | sacrifice lands, search basics onto battlefield |
| Proctor's Gaze | - | `sos` | ✅ done | bounce plus search basic to battlefield |
| Pterafractyl | - | `sos` | ⏳ | enters with X counters, ETB gain life |
| Pull from the Grave | - | `sos` | ⏳ | return creatures to hand, gain life |
| Quick Study | - | `woe` | ✅ done | draw two cards |
| Rapturous Moment | - | `sos` | ✅ done | draw, discard, add mana ritual |
| Rubble Rouser | - | `sos` | ⏳ | discard/draw ETB; mana ability with damage |
| Shattered Acolyte | - | `sos` | ✅ done | lifelink; sac to destroy artifact/enchantment |
| Shattered Sanctum | - | `vow` | ✅ done | conditional enters-tapped dual land |
| Shopkeeper's Bane | - | `sos` | ✅ done | attack trigger gain life |
| Silverquill Charm | - | `sos` | ✅ done | modal counters/exile/drain |
| Sneering Shadewriter | - | `sos` | ✅ done | ETB lose/gain life |
| Splatter Technique | - | `sos` | ⏳ | modal draw/area damage |
| Stadium Tidalmage | - | `sos` | ✅ done | ETB/attack loot draw-discard |
| Stand Up for Yourself | - | `sos` | ⏳ | destroy target power-3+ creature |
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
| Abstract Paintmage | S13 | `sos` | ⏳ | I/S-restricted mana added each main phase |
| Ajani's Response | S12 | `sos` | ⏳ | conditional cost reduction if targets tapped creature |
| Ambitious Augmenter | S6 | `sos` | ⏳ | Increment mechanic (mana-spent vs power/toughness) |
| Antiquities on the Loose | S10 | `sos` | ⏳ | flashback + cast-from-zone condition |
| Applied Geometry | S14 | `sos` | ✅ done | create token copy of permanent |
| Arcane Omens | S7 | `sos` | ✅ done | Converge colors-of-mana discard |
| Archaic's Agony | S7,S15 | `sos` | ⏳ | Converge damage + impulse-play exiled cards |
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
| Elemental Mascot | S5,S15 | `sos` | ⏳ | Opus + impulse play top card |
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
| Follow the Lumarets | S2,S4 | `sos` | ⏳ | look-and-pick plus infusion gained-life |
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
| Hydro-Channeler | S13 | `sos` | ⏳ | mana only for instants/sorceries |
| Imperious Inkmage | S1 | `sos` | ✅ done | ETB surveil 2 |
| Informed Inkwright | S8 | `sos` | ✅ done | Repartee makes Inkling token |
| Inkling Mascot | S8,S1 | `sos` | ✅ done | Repartee grants flying, surveil |
| Inkshape Demonstrator | S17,S8 | `sos` | ⏳ | Ward, Repartee pump/lifelink |
| Killian's Confidence | S18 | `sos` | ⏳ | triggered ability functions from graveyard |
| Lecturing Scornmage | S8 | `sos` | ✅ done | Repartee self-counter |
| Living History | S9 | `sos` | ⏳ | attack trigger gated on graveyard-leave |
| Lumaret's Favor | S14,S4 | `sos` | ⏳ | conditional copy (infusion) plus pump |
| Magmablood Archaic | S5,S7 | `sos` | ⏳ | Converge; I/S trigger scales by colors |
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
| Practiced Scrollsmith | S15 | `sos` | ⏳ | impulse cast exiled graveyard card |
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
| Suspend Aggression | S15 | `sos` | ⏳ | impulse play exiled cards |
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
| Wildgrowth Archaic | S7 | `sos` | ⏳ | converge, colors of mana spent |
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
(UI team): render `{X/Y}` pips in `main.ts`. Monocolour hybrid (`{2/G}`, Wildgrowth Archaic) still
deferred. → Stirring Honormancer. Next: rebuild the creature-died flag *with* Essenceknit Scholar (now
unblocked); then Moseo, Abstract Paintmage.

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

## Session note (git hygiene)
Shared **index** in this working tree: plain `git commit` (even after `git add <my paths>`) commits the
WHOLE index and sweeps up teammates' pre-staged files. ALWAYS `git commit --only <explicit paths> -m`.
(Matches the [[shared-tree-git-hygiene]] memory's `git commit -- <paths>` rule — follow it.)
