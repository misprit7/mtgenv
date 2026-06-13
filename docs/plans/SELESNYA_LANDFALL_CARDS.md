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

## Interpreter capabilities required (engine)

Most are currently a no-op in `whiteboard.rs::materialize` (the `_ => {}` arm) or absent from
the IR. Build additive-only; coordinate IR shape with `design`. Ordered to unblock the most
cards first.

| # | Capability | File(s) | Unblocks |
|---|---|---|---|
| C1 | **Creature mana dorks** — gate mana-source selection by summoning sickness (a sick creature can't tap for mana; `mana.rs` currently only checks `tapped`) | mana.rs | Llanowar Elves |
| C2 | **`Effect::PutCounters`** → `Action::AddCounters` (the action already exists for the ETB-counter rewrite) | whiteboard.rs | Sazh's Chocobo, Mossborn Hydra, Earthbender Ascension, Dyadrine |
| C3 | **`Effect::Mill`** → real | whiteboard.rs | Icetill Explorer |
| C4 | **Landfall trigger** — `EventPattern::LandEntersControlled` (a land you control enters); emit on land ETB, match APNAP | ability.rs, priority.rs/whiteboard.rs | Sazh's Chocobo, Mossborn Hydra, Icetill Explorer, Mightform Harmonizer, Earthbender Ascension |
| C5 | **Search basic land → battlefield (tapped) / → hand** — `Effect::Search` real, with the basic-land filter + `ZoneDest` | whiteboard.rs | Erode, Fabled Passage, Escape Tunnel, Earthbender Ascension, Lumbering Worldwagon, Bushwhack |
| C6 | **`Effect::CreateToken`** → real | whiteboard.rs | Dyadrine |
| C7 | **`Effect::Modal` (choose one)** — interpret + the choice request | whiteboard.rs | Bushwhack |
| C8 | **Fight** (`Effect` or `Native`) | whiteboard.rs/native | Bushwhack |
| C9 | **Dynamic `ValueExpr`** — count lands you control, count +1/+1 counters on self (for "double the counters" / `*` P/T) | value.rs | Mossborn Hydra, Lumbering Worldwagon, Dyadrine |
| C10 | **X costs** in casting | mana.rs/priority.rs | Dyadrine |
| C11 | **Conditional ETB-tapped / pay-life dual lands** (replacement w/ player choice) | whiteboard.rs | Temple Garden, Ba Sing Se |
| C12 | **Earthbend** (land → 0/0 haste creature still a land, +N/+N counters, dies/exile → return tapped delayed trigger) — a small subsystem | several | Badgermole Cub, Ba Sing Se, Earthbender Ascension |
| C13 | **Crew / Vehicle** (tap creatures w/ total power ≥ N → becomes creature until EOT) | combat/effects | Lumbering Worldwagon |
| C14 | **Warp** (alt cast cost from hand; exile at next end step; recast from exile) | priority.rs | Mightform Harmonizer |
| C15 | **`Effect::PumpPT` real + "double power until EOT"** (dynamic, EOT duration) | whiteboard.rs/chars | Mightform Harmonizer |
| C16 | **Trigger: becomes the target of an opponent's spell/ability** | ability.rs/priority.rs | Surrak |
| C17 | **Exile from a graveyard + count-card-types-among-exiled dynamic buff** | whiteboard.rs/chars | Keen-Eyed Curator |
| C18 | **Static permissions** — play an extra land each turn; play lands from graveyard | priority.rs | Icetill Explorer |
| C19 | **Mana production via real IR mana abilities** (CR 605) — implement `Effect::AddMana` + source/pay mana from `Ability::Activated{is_mana:true}` (cost + restriction/condition + `ManaSpec`), **retiring the `mana_colors` shortcut**; `ManaSpec.one_of` for "G or W" duals. Use ONLY for **non-type-derived** mana (Llanowar Elves, Hushwood's conditional {W}, any-color/filter lands) | mana.rs, whiteboard.rs, cards/mod.rs | Hushwood Verge, Llanowar Elves, Ba Sing Se, dorks |
| C20 | **Intrinsic basic-land-type mana** (CR 305.6) — derive `{T}: Add {color}` from each permanent's **computed** subtype (Plains→W/Island→U/Swamp→B/Mountain→R/Forest→G), NOT authored. Basics + typed duals (Temple Garden = `Forest Plains`) get **no** mana ability — it follows the land type, so type-changing effects (animate-land, Urborg, Spreading Seas) work for free | mana.rs, chars/ | all basics, Temple Garden, all typed lands |

## Fidelity standard (do not approximate)

Implement card text **faithfully**. The `mana_colors` color-vector is wrong for anything beyond a
plain "{T}: Add X" — mana production is a mana ability (CR 605) and can be conditional/costed/
any-color/dynamic; use the C19 IR path. The **deferred-clause** pattern (documented `// deferred:` +
a rules-text note) is reserved for genuine *subsystems* (earthbend land-animation, crew, warp) — not
avoidable conditionals; and when a card does need an unbuilt subsystem, mark the card **incomplete**
and flag engine to build the capability rather than shipping a behaviorally-wrong version.

## Ease tiers (author cards in this order)

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

Add a `selesnya_landfall()` deck builder (the 60 above) + register it in `preset_deck()` so
it's playable in the CLI/web alongside burn/bears.
