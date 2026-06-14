# Card-implementation push — Standard Selesnya Landfall (60 cards)

Source: mtggoldfish "Standard Selesnya Landfall" (deck id 7800414), maindeck = 60 cards,
**20 distinct** (2 are basics we already have). This is the first real card-pool push; it
deliberately stresses the effect interpreter (most of its mechanics are currently no-ops).

**Card data is in the SQLite index, not memory or guesswork** (CLAUDE.md "Card data"):
```
sqlite3 data/scryfall/cards.sqlite \
  "SELECT mana_cost,type_line,power,toughness,oracle_text
     FROM cards WHERE name='Sazh''s Chocobo' ORDER BY released_at DESC LIMIT 1;"
```
Built by `scripts/build_card_index.py` (run via `scripts/setup.sh`). One row per printing;
order `DESC` for current oracle wording, `ASC` for the first printing.

## Decklist (maindeck, 60)

```
4 Erode                         2 Surrak, Elusive Hunter        1 Lumbering Worldwagon
2 Bushwhack                     4 Earthbender Ascension         7 Forest
4 Sazh's Chocobo                1 Temple Garden                 2 Icetill Explorer
1 Keen-Eyed Curator            4 Llanowar Elves                 4 Badgermole Cub
2 Dyadrine, Synthesis Amalgam   4 Fabled Passage                4 Hushwood Verge
4 Mightform Harmonizer          4 Escape Tunnel                 2 Plains
3 Ba Sing Se                    1 Mossborn Hydra
```
(Sideboard not in scope for the first pass.)

## Module organization (cards/)

Refactor `crates/mtg-core/src/cards/mod.rs` (one giant file) into:

```
cards/
  mod.rs              # CardDef, CardDb, the builders (creature/spell/aura/…),
                      # deck builders, build_game; aggregates submodules into starter_db()
  misc/               # the existing starter cards (Lightning Bolt, Grizzly Bears, …),
    mod.rs            #   one file per card (or small grouped files); these have no real set
    ...
  <setcode>/          # one folder per FIRST-printing set, one file per card
    mod.rs
    <card_name>.rs
```
- Folder = the set the card was **first printed in**, real expansions preferred over promos
  (computed below). Basics (Forest/Plains) already exist — leave them.
- One canonical registration path; `starter_db()` (or a renamed `card_db()`) inserts every
  card by calling the submodules. Keep `grp::` id constants but move per-set ids near their
  cards. No re-export shims (CLAUDE.md).

### First-printing set → folder, per card

| Card | folder | cost | type | P/T |
|---|---|---|---|---|
| Llanowar Elves | `lea` | {G} | Creature — Elf Druid | 1/1 |
| Mossborn Hydra | `fdn` | {2}{G} | Creature — Elemental Hydra | 0/0 |
| Temple Garden | `rav` | — | Land — Forest Plains | — |
| Fabled Passage | `eld` | — | Land | — |
| Bushwhack | `bro` | {G} | Sorcery | — |
| Keen-Eyed Curator | `blb` | {G}{G} | Creature — Raccoon Scout | 3/3 |
| Surrak, Elusive Hunter | `tdm` | {2}{G} | Legendary Creature — Human Warrior | 4/3 |
| Escape Tunnel | `mkm` | — | Land | — |
| Hushwood Verge | `dsk` | — | Land | — |
| Lumbering Worldwagon | `dft` | {2}{G} | Artifact — Vehicle | */4 |
| Erode | `sos` | {W} | Instant | — |
| Icetill Explorer | `eoe` | {2}{G}{G} | Creature — Insect Scout | 2/4 |
| Dyadrine, Synthesis Amalgam | `eoe` | {X}{G}{W} | Legendary Artifact Creature — Construct | 0/1 |
| Mightform Harmonizer | `eoe` | {2}{G}{G} | Creature — Insect Druid | 4/4 |
| Sazh's Chocobo | `fin` | {G} | Creature — Bird | 0/1 |
| Earthbender Ascension | `tla` | {2}{G} | Enchantment | — |
| Badgermole Cub | `tla` | {1}{G} | Creature — Badger Mole | 2/2 |
| Ba Sing Se | `tla` | — | Land | — |

## Capability ledger (reconciled — what was actually built)

> **Status (2026-06-14):** the push is **delivered** — all 18 distinct cards authored and the
> `selesnya`/`landfall` preset (`cards::selesnya_landfall_deck`) **is the real mtggoldfish 60**
> (51 nonbasics + 7 Forest / 2 Plains, no padding). It plays end-to-end (validated: `mtg-cli
> selesnya selesnya` across seeds, clean finishes, zero panics). **🎉 Upgrade tail COMPLETE — 17/18
> cards are fully faithful; the deck is 18/18 fully-faithful *minus only Surrak's inert "can't be
> countered"*** (the single remaining deferred clause, kept as a documented standing gap per the lead
> — there is no counterspell in the pool, so it has nothing to act on). Every substantial clause on
> every card is implemented. **No card is husked or approximated.**
>
> This is the authoritative capability index (the original C-plan, reconciled to reality):
> ✅ built · ⏳ deferred. Commit refs given for the recent/subsystem caps.

| # | Capability | Status | Cards (✓ = fully faithful) |
|---|---|---|---|
| C1 | Creature mana dorks (summoning-sickness mana gate) | ✅ | Llanowar Elves ✓ |
| C2 | `Effect::PutCounters` → `Action::AddCounters` | ✅ | Sazh's Chocobo ✓, Mossborn Hydra ✓ (Earthbender quest pending A) |
| C3 | `Effect::Mill` | ✅ | Icetill Explorer (mill works; card ⏳ on C18) |
| C4 | Landfall trigger (`EventPattern::PermanentEnters(land you control)`) | ✅ | Sazh's ✓, Mossborn ✓, Icetill, Mightform ✓, Earthbender |
| C5 | Search basic land → battlefield(tapped)/hand | ✅ | Erode ✓, Bushwhack ✓ (Fabled/Escape/Lumbering fetch works; cards ⏳) |
| C6 | `Effect::CreateToken` | ✅ | (Dyadrine's Robot token — in deferred c3) |
| C7 | `Effect::Modal` (choose one) | ✅ | Bushwhack ✓ |
| C8 | `Effect::Fight` | ✅ | Bushwhack ✓ |
| C9 | Dynamic `ValueExpr` (count lands / `CountersOnSelf`) | ✅ | Mossborn ✓; Lumbering `*/4` CDA works (⏳ Crew) |
| C10 | X costs in casting | ✅ | Dyadrine (`{X}` works) |
| C11 | Conditional ETB-tapped / pay-life duals | ✅ | Temple Garden ✓, Ba Sing Se ✓ |
| C12 | **Earthbend** (animate land + counters + dies/exile→return-tapped) | ✅ `3d4b636`+`db81497`+`21171dc` | Ba Sing Se ✓; Badgermole/Earthbender ETB works (⏳ on H / A) |
| C13 | **Crew** (CR 702.122) — `CostComponent::Crew(n)` + `Effect::BecomeCreature{what,duration}` | ✅ `80d9ab3` | Lumbering Worldwagon ✓ |
| C14 | **Warp** (alt cast + exile-at-end-step + recast-from-exile) | ✅ `c445d78`+`7cc6f9c` | Mightform Harmonizer ✓ |
| C15 | `Effect::PumpPT` + double-power snapshot (`ValueExpr::PowerOfTarget`) | ✅ `557b6b5` | Mightform ✓ |
| C16 | becomes-targeted trigger (`EventPattern::BecomesTargeted{filter,by_opponent}`) | ✅ `8d006fd` + `d3ee9e9` (battlefield + stack halves) | Surrak — trigger fully works; only can't-be-countered/G deferred |
| C17 | Exile-from-graveyard + count-card-types buff | ✅ `e002d7a`+`b18c6f6` | Keen-Eyed Curator ✓ |
| C18 | **Static land-play permissions** — `StaticContribution::ExtraLandPlays(n)` + `PlayLandsFrom(Zone)` | ✅ `3ca7fef` | Icetill Explorer ✓ |
| C19 | Mana production via real IR mana abilities (CR 605) | ✅ | Hushwood Verge ✓, Llanowar ✓, Ba Sing Se ✓ |
| C20 | Intrinsic basic-land-type mana (CR 305.6) | ✅ | all basics, Temple Garden ✓ |

### Ad-hoc capabilities (coined in-flight, unnumbered)

Needed and built as specific cards demanded them — not in the original C-plan:

| Capability | Status | Enables |
|---|---|---|
| `CostComponent::Sacrifice` (sacrifice as a cost) | ✅ `0451182` | fetch lands (Fabled, Escape); Lumbering crew cost |
| `PlayerRef::ControllerOfTarget(u32)` | ✅ `632982e` | Erode (the destroyed permanent's controller fetches) |
| **Floating continuous-effect subsystem** (`chars::ContinuousEffect` registry, CR 611) | ✅ `3d4b636` | earthbend animation, until-EOT pumps, grant-keyword-EOT |
| **Delayed triggered abilities** (CR 603.7, `DelayedTriggerEvent`) | ✅ `21171dc` | earthbend return-tapped; warp exile-at-end-step |
| `Action::GrantContinuous` | ✅ `db81497` | earthbend, PumpPT, GrantKeyword |
| `ValueExpr::PowerOfTarget` (resolution snapshot) | ✅ `557b6b5` | Mightform double-power |
| `Rewrite::EntersWithCountersValue` + `ValueExpr::ManaSpent` | ✅ `a2e2b13` | Dyadrine (enters with counters = mana spent) |
| Attack-trigger firing (`EventPattern::SelfAttacks` / `YouAttack`) | ✅ `4613d51` | Dyadrine attack (deferred c3); Lumbering "or attacks" |
| `Effect::Conditional` interp + `Effect::GrantKeyword` + `CounterKind::Named` | ✅ `d8484d2` | Earthbender quest-chain; any intervening-if / grant-keyword-EOT |
| **Reflexive "when you do" sub-trigger** (`StackObjectKind::ReflexiveAbility`; targeted `Conditional.then` deferred — target chosen at 603.3d only if the if holds, CR 603.7c) | ✅ `2e13694` | Earthbender Ascension ✓; the "if you do" front for Dyadrine c3 |
| `EventPattern::TapCreatureForMana` (no-stack triggered mana ability, CR 605.1b) | ✅ `23242f2` | Badgermole Cub ✓ |
| `EffectTarget::Searched(n)` (reference the Nth permanent a Search fetched) + `Effect::Tap{tap}` materialized | ✅ `bcff1cd` | Fabled Passage ✓ |
| `Effect::GrantQualification` + `Qualification::CantBeBlocked` (combat `can_block` reads it) + `CardFilter::PowerAtMost(n)` | ✅ `7dd18a9` | Escape Tunnel ✓ |
| `Effect::Exile` interp + `TargetKind::CardInZone{Graveyard}` | ✅ `e002d7a` | Keen-Eyed exile ability |
| `Ability::ConditionalStatic` + `ValueExpr::DistinctCardTypesAmongExiledWith` + exile-association (`Object.exiled_with`) | ✅ `b18c6f6` | Keen-Eyed conditional +4/+4 & trample |
| Permanent-targeting fix (enumerate all permanents; enforce `HasCardType`/`All`/`Not`/`ControlledBy`) | ✅ `70c483e`+`861f3aa` | earthbend land-targeting end-to-end |
| `Ability::Warp{cost}` + `CastVariant::Warp` + `Action::WarpExile` + `Object.castable_from_exile` | ✅ `c445d78`+`7cc6f9c` | Mightform warp |

### Upgrade tail — caps (COMPLETE except G)

All flipped `fully_implemented:false → true` as their cap landed (task #44, done). The **only remaining
deferred clause in the whole deck is G** — Surrak's inert "can't be countered" (no counterspell in the
pool), a documented standing gap per the lead. Strikethrough = landed + card flipped.

| Cap | Card | Deferred clause |
|---|---|---|
| ~~**A** reflexive sub-trigger~~ ✅ **DONE** `2e13694` | ~~Earthbender Ascension~~ ✓ flipped `e6b9050` | landfall → quest → when-you-do(≥4) → +1/+1 + trample |
| ~~**B** `Effect::Optional` + `Effect::ForEach` + `EffectTarget::Each`~~ ✅ **DONE** `0e01d56` | ~~Dyadrine, Synthesis Amalgam~~ ✓ flipped `575bb2a` | attack → remove a +1/+1 from *each of two* creatures → draw + Robot |
| ~~**C13** Crew (CR 702.122)~~ ✅ **DONE** `80d9ab3` | ~~Lumbering Worldwagon~~ ✓ flipped `86742c3` | Crew 4 |
| ~~**D** searched-permanent reference + `Effect::Tap{tap:false}`~~ ✅ **DONE** `bcff1cd` | ~~Fabled Passage~~ ✓ flipped `968036e` | "if you control 4+ lands, untap that land" |
| ~~**E** `Qualification::CantBeBlocked` + power≤2 filter + grant-qual-for-duration~~ ✅ **DONE** `7dd18a9` | ~~Escape Tunnel~~ ✓ flipped `5a55600` | "{T},Sac: target power≤2 creature can't be blocked this turn" |
| ~~**F** `Target::Stack` in `BecomesTargeted`~~ ✅ **DONE** `d3ee9e9` (no card change — same filter matches creature spells on the stack) | Surrak, Elusive Hunter | "or a creature spell you control" trigger half |
| **G** stack-zone static gathering + a counter subsystem | Surrak, Elusive Hunter | "This spell can't be countered" — **the lone remaining gap; deferred per lead** (inert: no counterspell in the pool) |
| ~~**C18** static land-play permissions~~ ✅ **DONE** `3ca7fef` | ~~Icetill Explorer~~ ✓ flipped `7350a74` | "play an additional land" + "play lands from your graveyard" |
| ~~**H** "tapped a creature for mana" event + reflexive mana trigger~~ ✅ **DONE** `23242f2` | ~~Badgermole Cub~~ ✓ flipped `c2ef012` | "whenever you tap a creature for mana, add {G}" |

## Fidelity standard (do not approximate)

Implement card text **faithfully**. The `mana_colors` color-vector is wrong for anything beyond a
plain "{T}: Add X" — mana production is a mana ability (CR 605) and can be conditional/costed/
any-color/dynamic; use the C19 IR path. The **deferred-clause** pattern (documented `// deferred:` +
a rules-text note) is reserved for genuine *subsystems* (earthbend land-animation, crew, warp) — not
avoidable conditionals; and when a card does need an unbuilt subsystem, mark the card **incomplete**
and flag engine to build the capability rather than shipping a behaviorally-wrong version.

## Ease tiers (author cards in this order)

> _Historical — the authoring order that was used. All 18 cards are now authored; see the
> capability ledger above for current per-card state._

**Tier 1 — easy (C1–C4):** Llanowar Elves, Sazh's Chocobo, Mossborn Hydra, Icetill Explorer
(landfall-mill now; defer its land permissions C18), Hushwood Verge (two mana abilities, the
{W} one conditional on controlling Forest/Plains).

**Tier 2 — fetch & duals (C5, C11):** Fabled Passage, Escape Tunnel (defer the unblockable
second ability), Erode, Temple Garden.

**Tier 3 — modal / tokens / X (C6–C10):** Bushwhack, Dyadrine, Surrak (trample + can't-be-
countered now via `Qualification::CantBeCountered`; defer the C16 target-trigger).

**Tier 4 — subsystems (C12–C18):** Earthbender Ascension, Badgermole Cub, Ba Sing Se (all
earthbend), Lumbering Worldwagon (crew), Mightform Harmonizer (warp + double power), Keen-Eyed
Curator (exile-types buff).

Where a card has a clause beyond the current engine, **implement the core and leave a
documented `// deferred:` note in the card's rules text** (the established pattern — see
Humility/Rancor) rather than blocking. Every card gets an `expect-test` snapshot of its IR
and, where it changes play, a behaviour test.

## Deck builder

> _✅ Done — `cards::selesnya_landfall_deck()` is the real mtggoldfish 60, registered in
> `preset_deck()` as `"selesnya"` / `"landfall"`, playable in CLI/web alongside burn/bears._

Add a `selesnya_landfall()` deck builder (the 60 above) + register it in `preset_deck()` so
it's playable in the CLI/web alongside burn/bears.

## #60 — End-to-end per-card audit (real cast → pay → resolve)

**Why.** The `fully_implemented` flags are *builder/inline defaults* (17 `true`, 1 `false`),
not validated through the real play loop. Every behaviour test to date calls
`resolve_effect` directly, **bypassing casting + cost payment** — so mana/cost bugs (#56
Badgermole affordability, #57 Ba Sing Se {T} double-count) and conditional sub-clauses (#58
Fabled untap) slipped through. #60 drives each card through the *actual* priority path and
asserts **every** oracle clause against resolved game state, then honestly re-baselines.

**Harness.** Uses the engine's real driving methods (the seam proven by
`priority.rs::fabled_passage_untaps_via_full_activation`): `e.play_land(p, card)`,
`e.cast_spell(p, card, variant)` (real mana payment), `e.activate_ability(p, src, AbilityRef)`,
then `e.resolve_top()` — with scripted test `Agent`s answering targeting / mode / payment
`DecisionRequest`s. **Blocked** until those four are `pub(crate)` (requested from engine; today
they're private to `priority.rs`).

**Two passes** (mana legs depend on #59 mana-pool payment rework):
- **Pass 1** (needs only the 4-method exposure): non-mana legs — land plays, `{T}`/`{T},Sac`
  and other no-mana abilities, and ETB/landfall/attack triggers driven via `play_land` /
  `activate_ability` + `resolve_top`. Re-baseline cards fully covered by non-mana legs.
- **Pass 2** (needs #59): real `cast_spell` mana payment for every creature/spell; mana-cost
  abilities (Ba Sing Se earthbend `{2}{G}`, Keen-Eyed exile `{1}`); Dyadrine counters = mana
  spent; verify #56/#57 fixed. Finalize all flags.

**Per-card audit matrix** (status: ☐ pending / ✅ pass / ❌ fail). "Gap" = clause with *no*
current behaviour test (resolve-level or otherwise) that the audit must add.

Two blockers remain for the rest: **`run_agenda`** (the trigger-processor; requested `pub(crate)`)
gates every ETB/landfall/attack *trigger*; **#57/#59** (mana) gate the Ba Sing Se earthbend leg and
Hushwood's `{W}`-restriction (enforced in the PayCost path, which `legal_actions` doesn't surface).

| grp | card | clauses to assert end-to-end | E2E status (this audit) |
|----|------|------------------------------|-------------------------|
| 100 | Llanowar Elves | 1/1 Elf Druid; taps for exactly 1 G; summoning-sick gate | ☐ cast-path doable now (queued) |
| 101 | Hushwood Verge | enters untapped; `{T}:G` always; `{T}:W` only if control Forest/Plains | `{T}:G` resolve ✅; **{W}-restriction blocked on #59** (PayCost path) |
| 102 | Sazh's Chocobo | base P/T; landfall = +1 `+1/+1`; only *your* lands | ☐ **blocked: `run_agenda`** (landfall) |
| 103 | Mossborn Hydra | Trample; enters w/ 1 `+1/+1`; landfall **doubles** counter count | ☐ **blocked: `run_agenda`** (landfall) |
| 104 | Icetill Explorer | landfall mill 1; +1 land/turn; **play lands from graveyard** | land-permissions: engine synthetic test ✅; landfall mill ☐ **blocked: `run_agenda`** |
| 105 | Lumbering Worldwagon | power = #lands (CDA); ETB *or* attack fetch basic tapped; Crew 4 | ☐ ETB/attack **blocked: `run_agenda`**; CDA/Crew doable |
| 106 | Fabled Passage | fetch basic tapped; if ≥4 lands → untap it | ✅ **E2E full-activation** (engine `priority.rs`) |
| 107 | Escape Tunnel | fetch basic tapped; **power≤2 target can't be blocked this turn** | ✅ **E2E activate-path** (real `{T}`+Sacrifice + target + grant) |
| 108 | Erode | destroy target creature/pw; **its controller may fetch basic tapped** (#61 ✅) | ✅ **E2E cast-path** (real `{W}` + target + auto-snapshotted opponent rider) |
| 109 | Temple Garden | ETB: pay 2 life → untapped **else tapped**; taps G/W | ✅ **E2E play-land** (both shock branches) |
| 110 | Ba Sing Se | ETB tapped unless control basic; `{T}:G`; `{2}{G},{T}` earthbend 2 | ETB ✅ **E2E play-land** (both branches); `{T}:G` resolve ✅; earthbend ☐ **#57** |
| 111 | Bushwhack | mode A fetch basic to **hand**; mode B fight (your vs their) | ✅ **E2E cast-path** (both modes, modal selection + targeting) |
| 112 | Surrak | can't-be-countered [**deferred, sanctioned**]; Trample; becomes-targeted → draw | ☐ becomes-targeted **blocked: `run_agenda`**; Trample/CbC done |
| 113 | Badgermole Cub | ETB earthbend 1; tap-creature-for-mana → **extra {G}** (#56 ✅ fixed) | ☐ **blocked: `run_agenda`** (ETB + mana trigger) |
| 114 | Earthbender Ascension | ETB earthbend 2 + fetch; landfall quest counter; ≥4 → `+1/+1`+trample | ☐ **blocked: `run_agenda`** (ETB + landfall) |
| 115 | Mightform Harmonizer | landfall doubles target power (snapshot +X/+0) EOT; Warp `{2}{G}` | ☐ landfall **blocked: `run_agenda`**; Warp post-mana |
| 116 | Dyadrine | Trample; **enters w/ counters = mana spent**; attack → remove 1 from 2, draw + Robot | **counters=mana ✅ E2E cast** (X=3→5/6); attack trigger ☐ **`run_agenda`** |
| 117 | Keen-Eyed Curator | static +4/+4 & trample at ≥4 exiled types; `{1}:` exile gy card | ☐ `{1}` exile activate-path doable now (queued); static computed ✅ |

**Tally so far:** 7 cards have ≥1 clause confirmed through the REAL play loop (106 Fabled, 107 Escape
Tunnel, 108 Erode, 109 Temple Garden, 110 Ba Sing Se ETB, 111 Bushwhack, 116 Dyadrine counters) — incl.
the hardest mana clause (Dyadrine counters = mana spent, via real `cast_spell` + X). No `fully_implemented`
flag has been demoted: every clause driven so far matches its oracle text. The remaining ~10 cards are
trigger-based (blocked on `run_agenda`) or mana-leg (blocked on #57/#59).

**Honest baseline going in:** all 17 `true` flags are unvalidated through the real path;
Surrak is correctly `false` (can't-be-countered deferred, no counterspell in pool). Flags will
be confirmed or demoted **only** by the matrix above turning ✅/❌.
