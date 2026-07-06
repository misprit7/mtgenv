# Work Log

Short, dated entries for future-agent consumption. Newest first. One line or a few bullets
per unit of meaningful progress. Keep it terse — detail lives in `docs/` and git history.

## 2026-07-06 (SOS COMPLETE — sos-cards-21 finale: 269→271 authored, last partial cleared)

- **✅ THE SET IS COMPLETE: 271/271 authored · 271 fully-faithful · 0 tracked-partials · 0 Native
  hatches. 871 mtg-core green, whole workspace (incl. mtg-py) builds.** Scryfall-diff verified against
  `set_code='sos'`. The 3 final items shipped:
- **`71a60ea` Resonating Lute** — the granted-mana subsystem (B3). New `StaticContribution::GrantTapMana`
  (+ `chars::granted_tap_mana` reusing `gather_statics`/`affects_matches`) so a granted `{T}: Add …` is
  visible to affordability/auto-pay; mana enumeration now carries a per-tap **count**, `select_payment`
  uses a unit up to `count` times committed to one colour with a one-tap-one-ability source guard,
  `auto_pay` adds the full count (restricted surplus floats). Additive — payment suite green.
- **`0c26308` Petrified Hamlet** — ETB name-choice reusing `ChooseOption{NameCard}` (zero cross-crate
  churn); new `Object.chosen_name` + `Effect::ChooseLandName`; name-keyed ability-legality gate
  (`name_is_chosen`) + name-keyed `{T}:{C}` grant via new `CardFilter::NamedAsChooser` on the B3a path.
- **`074fff2` Nita ability-2 rider** (last partial cleared) — `Effect::ExileTargetThenMayCast` +
  `Action::ExileForCastBy`: cross-player exile-cast (`Object.castable_by` + other-players' exile scan),
  any-type-mana (`spend_any_mana` → `ManaCost::collapse_to_generic` at offer gate + target pre-filter +
  payment), exile-on-leave (`exile_on_leave` → `flashback_cast`; `interpret_counter` gap fixed too).
- **Auto-pay-inert (option-B) roster** (faithful data, manual-only, NOT trainable via auto-pay): Treasure
  token, Goblin Glasswright's Treasure, Great Hall's pay-life mana, Hydro-Channeler's `{1}{T}` — see the
  census table in `docs/plans/SOS_CARDS.md`. Lute + Hamlet's abilities are plain `{T}` ⇒ fully trainable.

## 2026-07-06 (SOS ENDGAME — sos-cards-20: 266→269 authored, 4→1 partials)

- **The roadmap items landed; the "empty queue" opened up.** Census **269/271 authored · 268
  fully-faithful · 1 tracked-partial (Nita ab.2) · 0 Natives · 864 mtg-core green.** Three partials
  cleared, two new cards, two general engine extensions. Every commit green.
- **Subsystem A (layer-4/5 completion) — DONE.** The layer system was already ~complete (types,
  colors, keywords, 7a/b/c P/T); the only gap was **creature-subtype mutation**. New
  `StaticContribution::AddSubtype` / `SetCreatureSubtypes` (layer 4) + a general `Effect::Becomes{
  contributions, base_pt, duration }` (concrete-resolves a dynamic base P/T). → **Fractalize**
  (`3bd4a44`, becomes a G/U Fractal, X+1) + **Great Hall** (`26daeca`, `{5}` → 2/4 Wizard land).
- **`44a4387` Wildgrowth clause 2** — `FloatingRewrite::EntersWithCounters` + `EntersWithCountersRider`
  arm a one-shot ETB rider on a just-cast creature spell (partial cleared).
- **`9795cbb` Rubble Rouser** — modeled the `{T},Exile-gy: Add {R}. When you do, ping` as a NON-mana
  activated ability (`Sequence[AddMana, DealDamage each-opp]`) so the ping is agent-selectable (mana
  abilities aren't offered to RL seats). No engine work.
- **`26daeca` Great Hall** also added: **granted cast-triggers now fire** (extended
  `queue_watching_spellcast_triggers` to scan `GrantAbility` templates, mirroring `queue_self_triggers`)
  + **`Condition::SelfIsCreature`** (computed-type read, the re-animation guard).
- **`70559fc` Hydro-Channeler 2nd** + Great Hall pay-life — authored as **cost-bearing mana abilities
  under the established "option-B"** convention (faithful data, works via the manual mana path, auto-pay-
  inert like Treasures/Goblin Glasswright; §2.6 is the eventual fix). Hydro partial cleared.
- **`9099864` Ral Zarek −7** — `Player.skip_next_turns` (CR 720, consumed by `advance_turn`) +
  `Effect::FlipCoinsSkipNextTurns` (seeded RNG). Partial cleared.
- **Open decision to the lead:** does option-B count as "fully-faithful" for the 271 census (Treasure
  precedent), or build **B1/B2** (auto-pay for cost-bearing mana, §2.6-adjacent)? Recommended option-B.
- **Remaining (3):** Resonating Lute + Petrified Hamlet (both need **B3** — granted-mana enumeration +
  "2-of-one-color" multi-mana; `producible_colors` reads only printed abilities), Nita ab.2 (cross-
  player impulse + any-mana + exile-on-leave). See SOS_CARDS.md NEXT-AGENT block.

## 2026-07-05 (SOS: CARD-COMPLETE MODULO ROADMAP)

- **The full-set relay reached its terminal state: 266/271 authored (98%) — 262 fully-faithful ·
  4 tracked-partial · 5 unauthored · ZERO Native hatches at 271 cards · 854 mtg-core tests.**
  Nineteen agents, one continuous ledger, every commit green. The remaining 9 items (5 unauthored:
  Fractalize, Great Hall, Rubble Rouser, Resonating Lute, Petrified Hamlet; 4 partials: Nita ab.2,
  Ral −7, Wildgrowth, Hydro-Channeler 2nd) ALL ride two engine-roadmap decisions held for the user:
  (1) layer-4/5 completion (type/color-changing continuous effects), (2) the transactional-payment /
  mana-ability-grant class (WHITEBOARD §2.6). Card-agent queue is EMPTY until those land.
- Highlights of the final day: all 5 college Elder Dragons (4 were compositions; Miracle = the one
  real subsystem, stack-trigger reveal window); prepare-DFCs complete (36; copy-only back-face
  design); Paradigm 5/5 Lessons; spell-copy BOTH semantics (707.10 + 707.12); planeswalkers +
  emblems + command zone; floating replacements + death-path interception; counterspell real-cast
  targeting; ~15 silently-inert defect classes killed across the push (unfired triggers, no-op
  costs/actions, dead filter/target arms, bypassed replacement passes).
- Architecture validation: the Effect-IR expressed every card attempted — the Native escape hatch
  is UNEXERCISED at 271 cards (recorded in WHITEBOARD_MODEL with the EffectCtx limitation the first
  real Native will need addressed).

## 2026-07-05 (SOS relay sos-cards-19 — 8 fully-faithful + 1 tracked-partial; census 257→266/271, 833→854 green; still 0 Natives; SET CARD-COMPLETE modulo roadmap)

- **Headline: the ledger's "3 Natives" were all IR-expressible** — the tag was stale. **Steal the Show** (`2e40f25`) was fully
  misdescribed ("control-theft + wheel" → actually a plain modal wheel + I/S-graveyard burn, ZERO new cap). **Mathemagics**
  (`ee72ebb`) = one generic `ValueExpr::Pow2`. Only **Pox Plague** has a real IR-vs-Native tradeoff (flagged to lead).
- **`cd5a6c0` — Choreographed Sparks** (modal spell-copy): wired `CopySpellOnStack`'s Target arm into `collect_specs_into`
  (mode-1 was uncastable for real casts) + new `Effect::CopySpellAsToken` (creature-spell copy → haste token, sac at next end
  step) + new `Qualification::CantBeCopied` (guarded at the `copy_spell_on_stack` choke point) + **made `Action::Sacrifice` a
  first-class applied action** (was a silent no-op in `apply_action`, only worked as a cost).
- **`7c6f148` — Zaffai**: once/turn free-cast I/S from hand — `StaticContribution::FreeCastFromHandOncePerTurn` +
  `PlayableAction::CastFreeFromHand` (new PlayableAction → fixed exhaustive matches in mtg-gre-server + mtg-py).
- **`c98a3c0` — Page, Loose Leaf**: `{T}:{C}` dork + Grandeur (`CostComponent::Discard` by name) → new
  `Effect::RevealFromTopUntilToHand`; `enter_filter_matches` now handles `CardFilter::Named` (was fail-closed).
- **`7310fdd` — Zimone's Experiment**: new `Effect::LookPickCreaturesLands` (look top 5, route creatures→hand / lands→bf tapped).
- **`65b176a` — Flashback**: grant flashback to a graveyard I/S until EOT — new `Object.flashback_until_turn` +
  `Effect::GrantFlashbackUntilEndOfTurn` + `flashback_cost` honors it. (Census diff surfaced Flashback + Zimone — both
  un-bucketed by the ledger.)
- **`8e595e4` — Pox Plague** (pure IR, last of the "3 Natives"): new generic `ValueExpr::Half` + `ValueExpr::LifeTotal{who}` +
  `Effect::ForEachPlayer` (player analogue of `ForEach`, binds `foreach_current` per player in APNAP order → `PlayerRef::Each`
  resolves to the iterated player). Each-player halving of life/hand/permanents as 3 separate passes (CR 608.2).
- **`ce41476` — Nita, Forum Conciliator** (TRACKED-PARTIAL): ability 1 faithful — new `CardFilter::OwnedBy` →
  `SpellCast(Not(OwnedBy(Controller)))` "cast a spell you don't own" → `ForEach` team `PutCounters(+1/+1)`. Ability 2 partial (cost +
  exile real; the cross-player-impulse + any-mana + exile-on-leave rider deferred — the impulse offer only scans your OWN exile, so
  casting a card in the OPPONENT's exile needs new machinery; spec in SOS_CARDS.md NEXT-AGENT block).
- Recorded in WHITEBOARD_MODEL: the Native hatch is still unexercised (0/271) + the EffectCtx state-read/decision-ask limitation the
  first genuinely-inexpressible card must fix.
- **5 unauthored remain — ALL roadmap/layer or lead-deferred** (Fractalize, Great Hall, Rubble Rouser + Resonating Lute, Petrified
  Hamlet). **The set is card-complete modulo the two engine-roadmap items.** No card-agent builds left. See SOS_CARDS.md NEXT-AGENT.

## 2026-07-05 (SOS relay sos-cards-18 — 9 fully-faithful cards + 8 reusable caps; census 248→257/271, 803→833 green)

- **`58e1cca` — Archaic's Agony** (Converge damage + **excess-damage → impulse-exile**). New `Effect::DealDamageExcessImpulse` —
  deals `ColorsSpent` to a target, computes `excess = max(0, amount − (toughness − marked))` from pre-damage state, exiles that
  many from the top of your library with impulse play-permission (`YourNextTurn`).

- **`237e01e` — Mana Sculpt** (Counter + **delayed mana**). New time-based `DelayedTriggerEvent::AtBeginningOfYourNextMainPhase`
  + `fire_main_phase_delayed_triggers` (wired into `PhaseBegan`) + `Action::AddMana` (delayed-trigger pool add) +
  `Effect::AddManaAtNextMainPhase` + `ValueExpr::ManaSpentOfTarget` (reads a `Target::Stack` spell's mana-spent). Arm-before-counter
  so the still-on-stack spell's mana_spent is readable; Wizard-gated.

- **`1d2e271` — Biblioplex Tomekeeper** (`{4}` Artifact Creature — ETB modal "prepare / unprepare a target"). New **modal
  *triggered*-ability support**: `place_trigger` chooses modes + collects the chosen modes' targets; ability resolution threads
  `obj.modes → ctx.chosen_modes`. Reuses the `SetPrepared` cap.

- **`0036255` — Divergent Equation** (dynamic **{X} target COUNT** — "return up to X target I/S from your gy"). Sentinel
  **`TARGET_COUNT_X` on `TargetSpec.max`** resolved to chosen `{X}` at the 2 cast slot-builds (avoids the 203-literal
  `TargetSpec` refactor; keeps true targeting) + plain multi-target `MoveZone` + `ExileOnResolve`.
- **`aec9852` — Skycoach Waypoint** (Land: `{T}: Add {C}` + `{3},{T}: target creature becomes prepared`). New
  **`Effect::SetPrepared{what,prepared}`** (targeted analogue of `BecomePrepared`; also covers Biblioplex's modes — Biblioplex now
  blocked only by modal-triggered-ability support).

- **`2e20d09` — Burrog Barrage** (the flagged "no new cap" was wrong — the conditional-pump target sits in a `Conditional` the
  targeting walk skips, AND the damage must read the *post-pump* power). Fixed cleanly with a small cap: new
  **`Effect::SourcedDamage{source,to,amount,kind}`** — creature-as-source damage (CR 119.2, a reusable "bite" primitive) whose
  *flushing* interpret arm commits the +1/+0 pump before reading `PowerOfTarget`. Pump targets slot-0 via `ChosenIndex` (kept
  out of the non-walked `Conditional`); damage declares both targets. Plus **`ValueExpr::InstantsSorceriesCastThisTurn`** (Burrog
  counts itself at cast → "another" I/S = ≥2).
- **`b85b613` — mill-then-play cap → Ark of Hunger + Tablet of Discovery** (2 cards, 1 cap). New **`Effect::MillThenPlay{who,
  window}`** + **`Object.playable_from_graveyard`** (graveyard analogue of impulse `castable_from_exile`; purely additive,
  exile paths untouched) + graveyard land-play/cast offer scans. `move_to_stack` already pops the graveyard so a milled spell
  casts normally (goes to gy, not exiled). Ark's leave-gy trigger + Tablet's restricted I/S mana reused existing machinery.
- **`cb6922e` — Slumbering Trudge** (stun counters + enters-tapped keyed to X). Reused `Rewrite::EntersWithCountersValue`
  (`3 − X` via `Sum(3, XTimes(-1))`) + the existing CR 702.171 stun untap-skip. Small shared fix: **`EntersTappedUnless` now
  threads the entering object's cast X** (routes through `cond_holds` w/ `{source, x}`) so "enters tapped if X ≤ 2" =
  `EntersTappedUnless(ValueAtLeast(X, 3))` reads the creature's own X — check-lands unaffected (non-value conds still route
  controller-relative).
- **The clean cap-blocked tail is now EXHAUSTED.** Remaining buildables are all subsystem-scale: Divergent Equation (dynamic
  {X} target COUNT — needs a `TargetSpec` dynamic-max, 203 literals, a dedicated refactor), Mana Sculpt (delayed mana — new
  time-based delayed trigger + `Action::AddMana` + a target-spell mana-spent value), Archaic's Agony (excess-damage tracking),
  Great Hall (becomes-a-creature layer animation), Rubble Rouser (mana-ability-with-cost — Hydro-Channeler class), Choreographed
  Sparks (creature-spell-copy-to-token). Plus the 9 special one-offs (lead sketch) + 3 Natives + Fractalize (parked).

## 2026-07-05 (SOS relay sos-cards-17 SECOND WAVE — target-path dynamic-filter fix + 4 cards; census 244→248/271, 794→803 green)

- **`priority.rs` — target-path dynamic-filter fix**: `target_matches_filter` now resolves a dynamic `CardFilter::ManaValueExpr`
  TARGET bound against a source-derived ctx (its `cast_x`/`colors_spent`). Was fail-closed `_ => false` = a dynamic-MV target
  SILENTLY never matched (silent-inert class). Real-path tests prove enumeration (offered/not-offered by the bound).
- **Moseo, Vein's New Dean** — Flying + ETB Pest + Infusion reanimate creature MV≤(life gained this turn) from your gy.
- **Sundering Archaic** — Converge ETB exile opp nonland permanent MV≤(colors spent) + {2}:gy→bottom-library.
- **Ennis, Debate Moderator** — new `cards_exiled_this_turn` tracker (+ `ValueExpr::CardsExiledThisTurn`) → end-step +1/+1;
  ETB = `ExileReturnNextEndStep` timed-blink.
- **Snooping Page** — new `EventPattern::SelfDealsCombatDamageToPlayer` (per-creature, queued from the source in `combat_damage`)
  → draw+lose-1; Repartee grants CantBeBlocked.
- NB: found `Player.instants_sorceries_cast_this_turn` ALREADY exists → **Burrog Barrage** buildable with no new cap (nearest win).

## 2026-07-05 (SOS relay sos-cards-17 FIRST WAVE — spell-copy consumers + LKI-counter & discard subsystems; census 234→244/271, 764→794 green)

- **9 fully-faithful cards + 1 tracked-partial + cleared Colossus + 7 reusable caps.** The clean-compose tail is now EXHAUSTED.
- **`898b23b` — Mica, Reader of Ruins** (sac-artifact spell-copy, pure Silverquill re-skin + Ward). 0 new cap.
- **`a4eb133` — discarded-this-resolution cap** (`Effect::DiscardChosen` "discard any number" + `ValueExpr::DiscardedThisResolution`
  over a per-resolution scratch) → **Borrowed Knowledge** (modal discard-hand-then-draw) + **Colossus dies clause cleared from
  tracked-partial** (discard any number, draw that many + 1).
- **Aziza, Mage Tower Captain** (tap-3 spell-copy over existing `MayPayCost`; caveat: TapCreatures excludes Aziza herself). 0 cap.
- **Mind into Matter** (Draw X + `Search{Hand,min:0}` put-permanent-MV≤X-tapped) + **fixed `interpret_search` dynamic-filter resolution**.
- **Wisdom of Ages** — `Effect::SetNoMaxHandSize` + `Ability::ExileOnResolve` marker + return-all-I/S (ForEach max:999).
- **Practiced Offense** — `Effect::GrantChosenKeyword` ("your choice of X or Y", composes in a Sequence) + TargetPlayer/ForEach counters + flashback.
- **Ambitious Augmenter + Scolding Administrator** — **LKI counter-count**: `Lki` snapshots the counter bag at death; `CountersOnSelf`
  falls back to it off-battlefield (fixed whiteboard AND conditions.rs eval). Reuse the existing `increment_ability()` helper.
- **`f48a776` — Tester of the Tangential (COMPLETE)** — Increment + begin-combat `MayPayCost{ {X} → MoveCounters }`. New caps:
  **`Effect::MoveCounters`** (capped at counters present) + **`MayPayCost`-with-`{X}`** (announce/pay X, thread to reward as X,
  targeted reward = normal ability target). Caveat: target chosen at placement, not reflexively. Cleared from tracked-partial.
- **Mind Roots** — `Effect::PutDiscardedOntoBattlefield` (put a discarded land under your control) over TargetPlayer+Discard-2.
- Census: **244/271 authored (240 faithful · 4 tracked-partial [Ral, Wildgrowth, Hydro-Channeler, Tester]) · 27 unauthored · 0 Natives.**
  Next high-yield cap = **target-path dynamic-filter resolution** (unlocks Moseo + Sundering Archaic). See SOS_CARDS.md census.

## 2026-07-05 (SOS relay sos-cards-16 — the copy-spell / cascade / affinity caps + 4 Elder Dragons + Lumaret's Favor)

- **Shipped 4 of the 5 college Elder Dragons + 5 reusable caps, 732→756 green, census 227→232/271.** Five own-commits:
- **`4dd31ef` — `Effect::CopySpellOnStack{what,count,choose_new_targets}`** (thin loop over the built `copy_spell_on_stack`,
  707.10) **+ Prismari, the Inspiration (Storm)** + **wired the dead `CostComponent::PayLife` no-op into `pay_cost`** for
  Ward—Pay 5 life. Storm = `Triggered{SpellCast(I/S)} → CopySpellOnStack{Triggering, count: SpellsCastThisTurn−1, new targets}`.
  Tests: storm scales 0/1/2 copies across 3 casts, per-copy new-target reselection, Ward taxes 5 life vs countered.
- **`cce33d6` — Silverquill, the Disputant (Casualty 1)** = `Triggered{SpellCast(I/S)} → Optional{IfYouDo{Sacrifice(creature
  power≥1) → CopySpellOnStack{Triggering, count:1}}}`. Timing caveat ledgered (sac trails the true 601.2b window; copy still
  resolves above the spell — observable result matches).
- **`f66c23f` — Witherbloom, the Balancer (Affinity) + `Ability::GrantCostReduction`.** Own affinity composes now
  (`CostReduction{GenericValue(Count creatures), Always, Cast}`); the granted-to-your-I/S clause = the new `GrantCostReduction`
  static that `effective_cast_cost` gathers from EVERY permanent the caster controls, scoped by a spell filter (CR 118).
- **`c7f2a8e` — Quandrix, the Proof (Cascade) + `EventPattern::SelfCast` + `Effect::Cascade`.** SelfCast = "when you cast THIS
  spell" (scans the just-cast spell's own abilities, `queue_self_cast_triggers`) — unblocks cascade AND the Infusion copy-self
  consumers. Cascade (702.83) = exile-top-until-nonland-cheaper + may-free-cast + random-bottom via `state.rng`. Own cascade
  (SelfCast) + granted cascade to your I/S (SpellCast watcher). "from hand" not enforced (rare over-trigger) — ledgered.
- **`42f4b74` — Lumaret's Favor (Infusion copy-self)** — first consumer combining SelfCast + CopySpellOnStack +
  `GainedLifeThisTurn` condition. Gained-life → +4/+8 (copied); no-life → +2/+4.
- **`aad6478` — Social Snub (copy-self edict)** — `Triggered{SelfCast, if CountAtLeast(creatures you control,1),
  Optional{CopySpellOnStack{Triggering,1}}}` + edict/drain (each player sacs a creature, drain 1). Copy doubles it. Census **233/271**.
- **`d874ae2` — Lorehold, the Historian (Miracle) + THE MIRACLE SUBSYSTEM (CR 702.94, lead-approved plan A). ALL 5 ELDER DRAGONS
  DONE.** `Ability::Miracle{cost}` (printed) + `Ability::GrantMiracle{cost,filter}` (granted, mirrors GrantCostReduction);
  `miracle_cost(card,caster)` two-origin check; **`draw()` captures the turn's FIRST card** (0→1 transition, 702.94e) and queues a
  new **`StackObjectKind::MiracleWindow`** (priority respected via the agenda — queued directly from draw(), no new GameEvent);
  on resolution the controller may cast for the miracle cost via new **`CastVariant::Miracle`** (fixed alt-cost, mirrors Warp).
  Lorehold = 5/5 flying-haste + grants miracle {2} to your I/S + opp-upkeep loot (`Optional{IfYouDo{Discard,Draw}}`, gated
  `Not(YourTurn)`). Tests incl. the required 702.94e case (2nd card of the same draw does NOT qualify). Census **234/271 (86%)**.
- **Session total: 7 cards (all 5 college Elder Dragons + Lumaret's Favor + Social Snub) + 6 reusable caps, 732→764 green.** Still
  unblocked for the next agent: target-spell copy consumers (Choreographed Sparks via CopySpellOnStack's Target arm); the medium
  caps (Increment, Ennis exile-tracker, NoMaxHandSize, Moseo, LKI-counter).

## 2026-07-05 (SOS relay sos-cards-15 — the SPELL-LEVEL ADDITIONAL-CAST-COST cap + 4 cards)

- **Shipped the spell-level additional-cast-cost cap (CR 601.2b/f), all 4 cards, 698→713 green.** Three own-commits:
  **Seize the Spoils** (`6318597`) — the rails: `AdditionalCost{options:Vec<Cost>}` (a possibly-modal "or" clause) as an
  **`Ability::AdditionalCost` marker** (not a `CardDef` field — avoids 40+ literals, mirrors `CostReduction`); offer gate
  gates on payability (`additional_costs_payable`; discard excludes the on-stack spell; mana option checked jointly via new
  `ManaCost::plus`); `cast_spell` picks a payable option per clause, folds option mana into the payment, pays non-mana
  components at 601.2f–h → **discarded AT CAST** (a countered spell still paid). **Vicious Rivalry + Fix What's Broken**
  (`a2b6a3a`) — pay-**X**-life: **X-announcement generalized** (announce X when the mana cost has `{X}` OR an additional
  cost references X, bounded by life); **`CostComponent::PayLife` now wired** (was a dead no-op); + reusable
  **`CardFilter::ManaValueExpr`** (dynamic X-keyed MV bound) resolved to a concrete `ManaValue` against the ctx by
  `resolve_dynamic_filter` at `select_for_each` — the ledger's "Dynamic-MV filter" cap (**also unblocks Moseo**). **Soaring
  Stoneglider** (`eed8a13`) — modal (exile two from gy OR pay {1}{W}) on a **creature** cast; exercises the option choice +
  mana fold.
- Census 215→**219/271 (81%)**, 0 Native hatches. Ledger + census updated; the additional-cast-cost triage row struck.
- **Then two more by-cap wins (no architecture):** **Quandrix Charm** (`4b41def`) — new `Effect::SetBasePT` (layer 7b, "base
  P/T X/Y until EOT") lowering to the existing `GrantContinuous{SetBasePT}`; a +1/+1 counter still stacks (tested 6/6).
  **End of the Hunt** (`cd1fbe2`) — new `ValueExpr::GreatestManaValue` feeding a dynamic `ManaValueExpr` for a greatest-MV
  edict (`TargetPlayer(Opponent)` + `Exile{Select}`). Census **221/271 (82%)**, 717 mtg-core green.
- **Then Group Project** (`b7a1e51`) — **widened `Ability::Flashback` from `ManaCost` → a full `Cost`** so a flashback cost can
  be non-mana ("Flashback—Tap three creatures" = the shipped `TapCreatures(3)`); factored `cost_components_payable` out of
  `can_pay_cost`, migrated the 6 existing flashback cards to a `cards::flashback(mana)` helper. Census **222/271 (82%)**, 720 green.
- **Then Moment of Reckoning** (`b2d822d`) — NO new cap: a repeatable `Modal{max:4, allow_repeat:true}` over two existing
  effects (Destroy nonland permanent · MoveZone nonland-permanent-card gy→battlefield); the modal cursor already targets each
  instance. Census **223/271 (82%)**, 722 green. (Noted a minor modal cross-instance-distinctness mask caveat.)
- **Then Daydream** (`497f1b3`) — NO new cap: `Sequence[Blink{target creature you control}, PutCounters{ChosenIndex(0),+1/+1}]`
  (blink reuses the object id, so the locked target still names it) + mana flashback. Census **224/271 (83%)**, 725 green.
- **Then the GRANT-A-TRIGGERED-ABILITY-UNTIL-EOT subsystem (lead-approved, CR 613.1f) + Rabid Attack (`7ede626`) + Root
  Manipulation (`7fa973f`).** `StaticContribution::GrantAbility{template_grp}` + `Effect::GrantAbility` lowering to the existing
  `GrantContinuous` path; grant templates in a **reserved 9800+ block** (`cards/grant_templates.rs`, one `Triggered` def each —
  the lead's revision vs a phantom ability on the granting instant; auto-excluded from `/api/cards` by the ≥9700 threshold);
  `StackObjectKind::Ability` gained `#[serde(default)] source_grp: Option<u32>` (`ability_def` picks the template def) + a
  granted-ability scan in `queue_self_triggers`. Fires synchronously at the death/attack broadcast (before `recompute` expires
  the effect); the queued trigger references the template, so it survives. Tested dies→draw / attacks→gain-life + **post-EOT
  death/attack does NOT trigger**. ZERO regression on the hot trigger path. Census **226/271 (83%)**, 730 green.
- **Then Conciliator's Duelist** (`db859f6`) — **`Effect::ExileReturnNextEndStep`** (timed-blink cap, CR 603.7): exile now +
  arm an `AtBeginningOfNextEndStep` delayed trigger carrying the return `MoveZone{→Battlefield}`. Repartee
  (`SpellCastTargetingCreature`) drives it; ETB = draw + each loses 1. (Ennis reuses this but also needs an unbuilt
  cards-exiled-this-turn tracker.) Census **227/271 (84%)**, 732 green.
- **Session total: 12 cards + 10 reusable engine caps (incl. the grant-ability subsystem + timed-blink) + a Scryfall-diff
  census verification** (215→227/271, 698→732 green).

## 2026-07-05 (SOS relay sos-cards-14 — the FINAL FIVE prepare stragglers + 2 subsystems + honest census)

- **Shipped all 5 remaining prepare stragglers** (683→698 green): **Jadzi // Oracle's Gift** (`7a45fbf`, no new cap —
  `CreateToken{count:X}` + `ForEach{Fractals→PutCounters{Each,X}}`), **Harmonized Trio // Brainstorm** (`5345c20` —
  new `CostComponent::TapCreatures(n)` + `Effect::PutFromHandOnTop`), **Grave Researcher // Reanimate** (`f09c497` —
  new `Effect::ReanimateUnderControl` + `ValueExpr::ManaValueOfTarget` + the `move_object` control-vs-owner source-removal
  fix, so control≠owner now works), **Leech Collector // Bloodletting** (`88465ed` — the **queue-time trigger-condition
  check** across all 4 non-begin-of-step queue sites via `Engine::trigger_queues`; ZERO regression, exhaustive survey found
  Bucket B empty; + `life_gain_events_this_turn`), **Goblin Glasswright // Craft with Pride** (`c7d067c` — **option-B
  Treasure**: cost-bearing mana abilities excluded from the auto-pay pool, paid via `pay_cost` only through manual
  activation; `grp::TREASURE_TOKEN`).
- **Honest full-set CENSUS** (Scryfall-diff): **215/271 authored (79%)**, 4 tracked-partial, 56 unauthored — corrected the
  "complete except a shortlist" framing and flagged the ledger's per-card ⏳ triage table as STALE. Pre-triaged the tail
  by cap (one cap → several cards) for the successor.
- **🚩 Treasure gym-inertness (option B):** agent seats (`manual_mana=false`) can't spend Treasures → inert in training;
  option (A) auto-spend recorded under the WHITEBOARD §2.6 transactional-cast roadmap, not a standalone TODO.

## 2026-07-05 (SOS relay sos-cards-13 — the StackObject cluster: counterspell + 707.10 copy + Lessons)

- **Filled the long-latent counterspell real-cast gap + Brush Off** (commit d6349eb; 662→668 green). Root
  cause was deeper than flagged: `collect_specs_into` never matched `Effect::Counter`/`CounterUnlessPay`, so the
  counter's target spec was silently dropped at cast (specs empty → no target chosen → nothing countered) — that's
  why counterspells only ever worked via `resolve_effect`. Fixed that + wired `target_candidates` StackObject
  enumeration (spells only, excludes the caster's own spell-in-progress) + `target_matches_filter` stack→spell-card
  resolution. → **Brush Off** ({2}{U}{U} Counter target spell + the S12 `Cost({1}{U})` target-dependent reduction's
  first card; real-path tests: counters an opposing spell, won't target itself, can't-counter Surrak, masks to
  affordable targets).
- **CR 707.10 copy-a-spell-ON-the-stack + Pigment Wrangler // Striking Palette** (commit dedb749; →671 green).
  The copy that ISN'T cast (distinct from 707.12 CastCopy's mint+cast): `copy_spell_on_stack` mints an `is_copy`
  copy OVER the original carrying its targets/X/modes (707.10b), optional new-target reselection (707.10c), NO
  SpellCast (707.10a). Delivered via a one-shot delayed trigger: `Effect::CopyNextSpellCast` arms
  `DelayedTriggerEvent::YouCastSpell` (expires unfired at next turn's start), fired from the `SpellCast` broadcast
  → mints a new `StackObjectKind::SpellCopyTrigger`. Reusable for Lumaret's Favor / Twincast-class.
- **Improvisation Capstone — 5th Lesson (⇒ Paradigm 5/5)** (commit a30b648; →674 green).
  `Effect::ExileTopUntilManaValueMayCastFree`: exile from the top until total MV ≥ threshold, then loop offering to
  cast any number of the exiled NONLAND cards for free during resolution (CR 601.3e; uncast cards + lands stay
  exiled). + Paradigm.
- **Then 3 more prepare stragglers (by yield), each a clean reusable cap** (commits ccebbc9, 19e2f5e, 6c3508f;
  →683 green): **Skycoach Conductor // All Aboard** (`Effect::Blink` — CR 603.6e exile-then-return: ETB re-fires,
  counters/damage/summoning-sickness reset via `move_object`); **Emeritus of Truce // Swords to Plowshares** (front
  target-player-Inkling + conditional prepare; back "exile + controller gains life = power" — sequenced GainLife-
  BEFORE-Exile so power reads live, NO LKI plumbing: the general trick for "remove X, then Y = X's own stat");
  **Vastlands Scavenger // Bind to Life** (`Effect::MillThenPutCreatureOntoBattlefield` — mill from your OWN library,
  reanimate a creature from among them; owner==controller so no control override).
- Net: **6 cards + 6 reusable subsystems** (StackObject counterspell targeting; 707.10 copy-on-stack + "you next
  cast" delayed trigger; exile-top-until-MV free-cast; Blink; mill-then-reanimate; the gain-before-exile stat trick).
  199→~205 authored, 662→**683 mtg-core green**, whole workspace builds, tree clean (lead pushes). **StackObject
  cluster + 4 of the 9 prepare stragglers done; 5 stragglers remain (Leech Collector, Grave Researcher, Jadzi,
  Goblin Glasswright, Harmonized Trio), each blocked on a distinct cap — see the SOS_CARDS.md NEXT-AGENT block.**

## 2026-07-05 (SOS relay sos-cards-12 — PREPARE DFCs: rails + 24 cards)

- **Built the PREPARE-DFC subsystem as a spell-copy CONSUMER (CR 707.12), not a CR 711 transform.**
  `Object.prepared` flag + `Effect::BecomePrepared` (→ `Action::SetPrepared`) + `Ability::Prepare{spell}`
  (front→back link) + back-face spell defs in a reserved 9700+ grp block (`grp::PREPARE_BACK_BLOCK`,
  excluded from `/api/cards`) + `PlayableAction::CastPrepared{source}` offered in `legal_priority_actions`
  at the back face's timing, executed by `Engine::cast_prepared` (mints an `is_copy` copy from the def,
  `cast_spell(Normal)` PAYS the back cost, unprepares the source; copy ceases to exist off the stack).
  **Key payoff: every "becomes prepared" variant is just `Effect::BecomePrepared` on an ordinary trigger/
  ability — zero new trigger machinery** (enters / at-first-main / on-attack / landfall / cast-a-creature /
  cards-leave-gy / tokens-enter / end-step-conditional / an activated source all fell out for free). Did NOT
  widen `Effect::CastCopy` (Paradigm's free path untouched + still green). Commit bfd3d51.
- **27 of ~36 prepare cards shipped** (batches after bfd3d51; 630→662 mtg-core green). Helper
  `helpers::enters_prepared`/`prepared_abilities`. General caps added along the way (both eval paths where
  relevant): `ValueExpr::{LifeGainedThisTurn, CreaturesDiedThisTurn, HandSize, SpellsCastThisTurn}` (+
  `Player.spells_cast_this_turn`) and `Effect::MayTapOrUntap`. **⚠️ Trigger-condition gotcha (found + fixed):**
  `queue_self_triggers`/`queue_watching_*` do NOT check a trigger's `condition` at queue — only
  `queue_begin_of_step_triggers` does. So a condition on a Self*/SpellCast/PermanentEnters trigger MUST use
  `intervening_if: true` (else silently ignored). The 9 remaining prepare cards are each blocked on a distinct
  BACK-FACE cap (blink, {X}{X} tokens, LKI-power, mill-to-bf, Treasure sac-mana, reanimate-controller-override,
  gain-life-first-time [needs queue-time self-trigger condition check], Brainstorm-order, spell-copy-on-stack) —
  precise per-card blocker list in `docs/plans/SOS_CARDS.md`.

## 2026-07-04 (SOS relay sos-cards-11 — the SPELL-COPY subsystem + Paradigm Lessons)

- **Built the spell-copy foundation (CR 707.10/12) — the long-deferred, now load-bearing piece.**
  `CastVariant::WithoutPayingManaCost` wired to a {0} cost (the free-cast primitive); `Effect::CastCopy`
  mints a copy `Object` from a source's copiable base chars (707.2 via grp_id) into `Zone::Stack` and
  casts it through the **existing** `cast_spell` (new targets, X=0); `Object.is_copy` → the copy **ceases
  to exist** off the stack (707.10a, `state.cease_to_exist`, wired into `resolve_top` + `interpret_counter`).
  WHITEBOARD_MODEL §2.5 updated ("a spell on the stack is just an Object → a copy needs almost no new
  machinery"). Commits a1dbc3e, 5e1754a.
- **Paradigm (SoS Lessons mechanic — NOT Learn/sideboard).** `Ability::Paradigm` (self-exile-on-resolve
  marker) + `queue_exile_functioning_triggers` (mirrors the emblem/graveyard `FunctionsFrom` scans) +
  a recurring `BeginningOfStep(PrecombatMain)` optional `CastCopy{SourceSelf}` — dormant until the card
  reaches exile. `helpers::paradigm_abilities` bundles it for all 5 Lessons. **4/5 Lessons authored:**
  Decorum Dissertation (full lifecycle test: cast→self-exile→free copy each turn→copy vanishes),
  Germination Practicum, Restoration Seminar (reanimate), Echocasting Symposium (token-copy). Commit b3efee6, 526b372.
- **The Dawning Archaic** — `Effect::CastForFree{what, exile_on_leave}` (casts the ACTUAL gy card free +
  flashback-style exile rider; distinct from CastCopy). {1}-less-per-I/S cost-reduction arm now exercised. 99fc712.
- **Run Behind** — `Effect::PutOnTopOrBottom` (owner chooses top/bottom of library) + S12 target-dependent
  reduction. 3779976.
- **Prepare-DFC design finding (36 cards):** prepare = `Creature // Spell` where the back face is
  **copy-only** ("cast a copy of its spell") → a **spell-copy consumer**, NOT a CR 712 transform model.
  Design plan sketched to the lead (build = spell-copy + `prepared` bool + `BecomePrepared` leaf +
  back-face defs in a reserved grp block + a prepared-cast offer + a small CastCopy pay/from-def extension).
- 168→172 authored, **630 mtg-core tests green**, 6 commits, tree clean (lead pushes). **Remaining:**
  Improvisation Capstone (heaviest Lesson), Brush Off (StackObject counterspell gap), prepare-DFCs (pending lead OK).

## 2026-07-04/05 (the MuZero re-litigation — user-driven, verdict reversed then completed)

- **User challenged the MuZero negative ("probably a bug") → full re-investigation** (experiments/
  stochastic_muzero/): plumbing triple-audited CLEAN; real root cause = recipe/credit-assignment
  mismatch (`td_steps=5` vs 30–60-step episodes → flat-negative value net → passive low-index
  attractor → all-loss buffer spiral). Winning recipe (shaping 0.5 + td 40 + up 20, plain MuZero):
  **heralds 0 → 0.9 sampled/0.55 fair-greedy (SUCCESS); swine 0 → 0.12 then re-collapse (NOT
  competent; PPO 0.90)**. Combat judgment inconclusive — the honest contrast: PPO OVER-blocks
  (chump 94–97%), MuZero UNDER-plays (block_rate ~0). Two upstream bugs found+fixed (LightZero
  segment-boundary IndexError on long games; missing **kwargs in stochastic forward). Full
  observability retrofit: PPO-parity TB tags (fair-greedy + sampled) on all 4 canonical runs,
  one compact replay per checkpoint in the UI, TB consolidated. Banked at the stopping point.
- **Training replays now compact ON DISK** (440e43b): `replay_json` emits the v2 delta form —
  264–483KB vs 15–78MB (the fat-on-disk behavior predated MuZero; server-side compact had masked it).
- **Auto-priority verified shared UI↔training** (user question): one knob (`full_control`), one
  elision rule, same MTGA default stop set (own main phases only) across web driver, PyGame, fleet,
  and the MuZero adapter; deliberate asymmetries = manual-mana (human seat only) + UI stop overrides.
- **DouZero/DMC research** (user referent): action-as-input Q-head is the scaling answer to the
  Discrete(98) card-ID head (content already in obs rows; pooled head structurally can't read
  per-entity features — pointer-head fixable inside PPO); DMC = td∞ (immune to the flat-value
  failure) but ε≈0 (needs external exploration) and sample-hungry; PerfectDou (PPO+GAE+perfect-info
  critic) beats it — and our engine feeds a perfect-info critic for free. DMC build PARKED per user.
- **SOS full-set relay (user directive: full set, no corners):** agents 8–9 = 11 caps + 10 cards +
  Swamp registered (602 tests): LKI dies-triggers (real snapshot store), S12 cost-modification
  pipeline (cast+activation, target-dependent with candidate-affordability filtering),
  FunctionsFrom(zones) graveyard-trigger class, batched combat-damage event, MayPayCost,
  DirectedDiscard, enters-tapped recursion. No-rewind recorded as pragmatic-not-law (user steer;
  transactional pending-cast = sanctioned evolution, WHITEBOARD §2.6). Agent 10 on planeswalkers
  (groundwork mostly exists — verify-and-finish + Dellian Fel/Ral Zarek), then Lessons, prepare-DFCs.

## 2026-07-04 (sos-cards-10)

- **Planeswalkers: verify-and-finish. 602→609 mtg-core tests green** (163→165 authored, 2 tracked-partial
  PWs). Verified all 4 loyalty points were ALREADY built + tested (enters-with-loyalty via the real cast path;
  sorcery-speed + once-per-turn-per-PW gate; combat damage removes loyalty; ±N activation) — read-the-code,
  no fixes needed. Added an end-to-end `planeswalker_lifecycle_cast_activate_ultimate_dies` (cast→+2→−3→drain→
  0-loyalty SBA).
- **4 reusable primitives:** `planeswalker()` + `loyalty_ability()` builders (cards/mod.rs); `PlayerRef::Each`
  (the player analogue of `EffectTarget::Each` → "any number of target players each do X"); a `CardFilter::
  ManaValue` targeting-filter arm (was fail-closed → now enumerates MV-bounded graveyard/permanent targets).
- **2 cards** (initially tracked-partial): **Professor Dellian Fel** and **Ral Zarek, Guest Lecturer** (+1 Surveil 2 /
  −1 any-number-target-players-discard / −2 MV≤3 reanimation; −7 coin-flip+skip-turns deferred indefinitely).
- **EMBLEMS subsystem (CR 114) — lead-greenlit. 609→611 tests green.** The engine now has a **command zone**
  (`Zone::Command` per-player vec) + emblems: a registered def (reserved 9500+ block, `cards/emblems.rs`) with
  no characteristics carrying a normal `Ability::Triggered` + `FunctionsFrom([Command])`; `Effect::CreateEmblem`
  puts it in the command zone; a `queue_command_functioning_triggers` scan fires it from Command, stamping the
  triggering amount onto `x` so the effect reads "that much" via `ValueExpr::X`. Untouchable (no SBA scans
  Command). Composed agent-9's FunctionsFrom + the token-def pattern (didn't reinvent). → **Dellian's −6 emblem**
  ("whenever you gain life, target opponent loses that much") → **Dellian is now FULLY FAITHFUL**.
- **FLOATING DELAYED-REPLACEMENTS subsystem (CR 614) — lead-greenlit; the "known gap" is filled. 611→616 tests
  green.** `GameState.floating_replacements` (general container: scope + `ActionPattern` + serde-safe
  `FloatingRewrite` + until_turn + one_shot), consulted by the SAME rewrite pass as printed statics (CR 616.1f
  ordering preserved). `Effect::ExileIfWouldDie` → "if it would die this turn, exile it instead". **"Dies" = ANY
  battlefield→graveyard move** (`ActionPattern::WouldDie` covers destruction/sacrifice/legend-rule). Load-bearing
  fixes: SBA creature-death AND `interpret_sacrifice` took a direct `move_object` that bypassed replacements —
  both now route through a shared `death_zone_for`; also **revived the dead `WouldBeDestroyed`/`WouldDie` static
  path** (`affected_object` never covered `Destroy`). Scope invalidates on zone change (CR 400.7) + expires at
  turn start. → **Wilt in the Heat** (165→166 authored). Cleanly unblocks the Dawning Archaic's would-die→exile
  rider; the general container is also the rails for Wildgrowth Archaic (a *delayed enters-with-counters* clause —
  needs a `FloatingRewrite::EntersWithCounters` follow-on, NOT free).

## 2026-07-04 (sos-cards-9)

- **S12 target-dependent cost reduction — the sub-cap agent-8 deferred as risky. 586→590 mtg-core
  tests green** (158→159 authored). `Ability::CostReduction`'s condition became
  `CostReductionCondition::{State(Condition) | TargetMatches(CardFilter)}`;
  `effective_cast_cost` gained a `TargetCtx::{Optimistic | Chosen(&targets)}` context. The offer
  gate applies a target-dependent discount optimistically (a legal matching target exists), and
  `cast_spell` recomputes the final cost from the chosen targets *and* constrains each target
  slot's candidates to what the caster can pay — reductions only lower cost, so the engine never
  offers a target it can't pay for → auto_pay never underpays → **no rewind** (the invariant
  agent-8 flagged). + `CardFilter::Tapped`/`Untapped` arms in `target_matches_filter`. →
  **Ajani's Response** ({4}{W} Destroy, {3} off if it targets a tapped creature); a real-cast-path
  test proves the untapped creature is excluded from the offered targets when only {1}{W} is
  affordable. Orysa migrated to `State(...)` (unchanged behaviour).
- **Enters-tapped (MoveZone) + Teacher's Pest. 590→592 tests.** Added `tapped: bool` to
  `Effect::MoveZone` + `Action::MoveZone` (set in the executor after `move_object` re-untaps, CR
  110.5 — the `Effect::Search { tapped }` analogue). Threaded through the 13 existing MoveZone
  literals (`tapped: false`) + regenerated IR snapshots. → **Teacher's Pest** ({B}{G} Menace +
  gain-life-on-attack + `{B}{G}` graveyard-recursion to the battlefield **tapped** — completes the
  graveyard-recursion trio). Also registered the missing **Swamp** basic land (`grp::SWAMP=5` — no
  black basic land existed).
- **Exile-as-cost wiring + Postmortem Professor. 592→595 tests.** Wired `CostComponent::Exile(SelectSpec)`
  in `can_pay_cost`/`pay_cost` (`exile_cost_candidates`/`pay_exile_cost`, mirror the Discard pair; excludes
  the source, moves chosen cards to Exile) — it was defined-but-unpaid. → **Postmortem Professor** ({1}{B}
  can't-block Zombie + attack drain + `{1}{B}`,exile-an-I/S-from-gy graveyard-recursion). Reusable for
  future escape/delve.
- **Generalized the target-affordability filter** (lead note): consumes each candidate's full effective cost
  instead of special-casing "reduction present", so a future target-dependent cost *increase* works by
  construction. Recorded no-rewind as a pragmatic economy (transactional pending-cast = the sanctioned evolution,
  GRE model) in WHITEBOARD_MODEL §2.6 + ledger systemic notes; flagged the counterspell StackObject-candidate gap.
- **Graveyard-functioning triggers (new class) + Killian's Confidence. 595→598 tests.** Lead-approved Design B
  generalized: `Ability::FunctionsFrom(Vec<Zone>)` marker (battlefield is the implicit default zone-of-function,
  only deviating cards carry the marker — CR 113.6, zero churn) + a `collect_triggers` graveyard scan
  (`queue_graveyard_functioning_triggers`, reuses `queue_self_triggers`). Plus a batched
  `EventPattern::YouDealCombatDamageToPlayer` (`GameEvent::CombatDamageToPlayerBy`, once per controller per
  combat-damage step, broadcast from `deal_combat_substep`) and `Effect::MayPayCost{cost,then}` ("you may pay …;
  if you do, …" — the mana analogue of `IfYouDo`, broadly reusable). → **Killian's Confidence** (pump+draw spell +
  the graveyard trigger; real-path test drives combat→trigger→pay {W/B}→return-self, plus the unpaid-stays path).
- **Activated-ability cost reduction + Diary of Dreams. 598→602 tests.** Extended the S12 cost-reduction
  mechanism to CR 602: `Ability::CostReduction` gained `scope: CostReductionScope::{Cast|ActivatedAbilities}`;
  `effective_activation_cost(source,&Cost)` applies `ActivatedAbilities`-scoped reductions to an activated
  ability's mana at BOTH the offer gate and `activate_ability` (factored a shared `apply_cost_reduction`).
  → **Diary of Dreams** (SpellCast-I/S→page-counter trigger + `{5},{T}:Draw` costing {1} less per page counter;
  page counter = `CounterKind::Named("page")`, zero enum churn). Real-path tests: offered only once enough
  counters accrue, activating draws, and casting an I/S adds a counter.

## 2026-07-03 (sos-cards-7)

- **SOS: 5 caps + 4 cards, 562→575 mtg-core tests green** (153 authored / 150 fully-faithful).
  Each cap has a real-path test; all committed via `git commit --only` on a shared tree.
  - **{X}-in-an-activated-cost** (`7102d4a`): `activate_ability` now chooses `{X}` (ChooseNumber,
    bounded by affordable mana), folds it into generic mana, and carries it on the stack object; the
    ability-resolution `ResolutionCtx.x` was hardcoded `None` → now `obj.x`. → **Berta, Wise
    Extrapolator** (all 3 clauses; legality→pay→resolve activation with X=3 tested).
  - **S20 CountersOnTarget + flush-before-PutCounters** (`6fe5aaf`): `ValueExpr::CountersOnTarget{
    target, kind }` reads a target's live counter count; the `PutCounters` interpret arm now flushes
    staged actions first (mirrors CreateToken's #61 flush) so "put a +1/+1, then double" reads the
    post-first count. → **Growth Curve**. No counter-card regression across the suite.
  - **CardFilter::Attacking** (`e5207a1`): matches a current declared attacker
    (`CombatState::is_attacking`), added to `target_matches_filter` + exhaustive `count_filter_matches`.
    → **Living History** (ETB Spirit + `YouAttack`/S9-gated pump on a target attacking creature).
  - **DistinctNames value + HasCounter static-scope** (`9b0937f`): `ValueExpr::DistinctNames{zone,
    filter,controller}` (distinct card-names among matching objects) + wired `CardFilter::HasCounter`
    into the layer-system static-scope matcher (`chars/mod.rs::matches_filter`). → **Emil, Vastlands
    Roamer** — counter-gated trample anthem + `{4}{G},{T}` Fractal with X = differently-named lands.
    (This corrected the earlier belief that the {X}-activated-cost cap would clear Emil — it wouldn't;
    Emil uses X = differently-named lands, not a paid {X}.)

## 2026-07-03 (late night)

- **Stochastic MuZero on swine: CONCLUDED — two-config honest negative** (experiments/
  stochastic_muzero/, findings #1+#2). Both pure-sparse (3.0) and gym-Φ-shaped (3.1) collapse into
  an always-mulligan losing basin (value ≈ −0.8 everywhere, 0/30 vs random at the 10%-gate) — the
  pre-flagged crux confirmed: factored sub-decisions dilute lookahead so 50–100 sims can't reach
  winning lines to bootstrap from sparse ±1. Shaping measurably pressured the mulligan attractor
  (visit split 36/14 → 25/25) but couldn't escape the basin. Future-work recorded: macro-composed
  decisions, ~10× budget, Gumbel few-sim, warm-start from the PPO checkpoint. Runs on shared TB.
- **SOS chain summary (agents 4–5): ~126 → ~150 cards, 536 tests.** Ward (CounterUnlessPay on the
  BecomesTargeted seam, 5/8 cards), multi-target MoveZone, Not(ItSelf), cast-{X} trigger,
  dynamic token counters, CastFromNotHand; two more real-path engine fixes (MoveZone targets
  never collected via real casting; CreateToken staging invisible to same-resolution steps) →
  six real-path bugs killed today. **Ledger-vs-git audit: SIX stale cap rows fixed** + header
  PROCESS RULE (status flips in the cap's own commit). Honest remainder: first/double-strike
  wiring, per-turn counter tracker, PayLife, multi-target-each, spell-copy, Fractalize. Agent 6
  spawned on first-strike.

## 2026-07-03 (late night, agent 6)

- **FIRST/DOUBLE-STRIKE ALREADY WIRED — the handoff's #1 task was a no-op (read the code, not the ledger).**
  sos-cards-5 listed "first-strike / double-strike combat wiring" as genuinely-unwired top-priority work
  ("verified by reading `combat_damage`"). It's WRONG: `combat/mod.rs::combat_damage` has had the CR 510.4
  two-substep split since `a15015f` ("#14 checkpoint 1") — `any_first_or_double_strike` → first-strike substep
  (FS+DS deal), `apply_combat_deaths` (SBAs between steps), then the regular substep (DS again + non-FS). Passing
  tests `double_strike_deals_twice` + `first_strike_kills_before_retaliation` already prove it. `deals_in` reads
  the COMPUTED keyword set, so granted first/double strike works too. **No card is unblocked by this** (Practiced
  Offense still needs modal-keyword-pick + counter-on-each-of-a-target-player's-creatures). Queue item #1 removed.
- **cards(sos) — Fractal Tender + per-turn "counter put on this permanent" cap (queue item #2).** New:
  `Object.counter_added_this_turn` (set in the `Action::AddCounters` executor when `n>0`; reset at turn start for
  ALL permanents + on any zone change, CR 400.7) and `Condition::PutCounterOnSelfThisTurn` (reads the source
  object's flag — threaded through both intervening-if check points). **Fractal Tender** `{3}{G}{U}` 3/3 Elf Wizard
  = Ward {2} (S17) + Increment (S6, its +1/+1 sets the flag) + `BeginningOfStep(End)` intervening-if trigger →
  0/0 Fractal token with three +1/+1 counters (`fractal_token(3)`). The 6th of 8 Ward cards. Real-path tests: the
  end-step trigger fires through the engine and gates on the flag; a real `PutCounters`→`AddCounters` sets it. 541 tests.
- **cards(sos) — Homesickness + `Effect::ForEachTarget` cap (queue item #4, apply-to-each-of-a-variable-multi-target).**
  New IR leaf `ForEachTarget { slot: TargetSpec, body }`: the slot is declared as a targeting spec at cast (added
  to `collect_specs_into`), and at resolution each chosen target of the (0–N) slot is bound to `EffectTarget::Each`
  in turn while `body` runs — reusing the existing `foreach_current`/`Each` machinery (no new target-resolution
  path). **Homesickness** `{4}{U}{U}` Instant = `TargetPlayer`+`Draw{ChosenTarget(0),2}` then `ForEachTarget` over
  up-to-2 target creatures with `body = Tap{Each}+PutCounters{Each,Stun}`. Tests: IR snapshot, targeting-collection
  (`target_specs_for` → [Player, Creature 0..2]), and resolution with 2 vs 1 chosen creatures. Reusable for any
  "do X to each of up-to-N target creatures." 545 tests.
- **cards(sos) — Fractal Anomaly + S19 `ValueExpr::CardsDrawnThisTurn`.** New per-player `cards_drawn_this_turn`
  (reset each turn in `begin_turn`, incremented in `Engine::draw` — the single draw path) + a `ValueExpr` reading
  it for `ctx.controller`. **Fractal Anomaly** `{U}` Instant = 0/0 Fractal token with X +1/+1 counters where X =
  cards drawn this turn (reuses `CreateToken.dynamic_counters`; same shape as Wild Hypothesis but count is
  cards-drawn, not `{X}`). Real-path test drives `Engine::draw` then resolves the token → 3 counters. 548 tests.
- **cards(sos) — `ValueExpr::XOfTriggeringSpell` + Geometer's Arthropod (S21 complete).** Records chosen `{X}` on
  the cast object (`Object.cast_x`, alongside `mana_spent`); a ValueExpr reads the triggering spell's `cast_x`.
  Geometer's Arthropod `{G}{U}` = cast-with-{X} trigger (S21) → LookAndPick (S2) top-X keep-one. 551 tests.
- **AUDIT: "no-cap vein mined out" was WRONG.** A verified unauthored-card audit found **2 zero-cap cards** + a
  vein of 1-small-cap cards. Shipped both zero-cap: **Withering Curse** `{1}{B}{B}` (all-creatures -2/-2, or
  Infusion destroy-all — the Splatter `EachPlayer` all-creatures ForEach inside a `GainedLifeThisTurn` Conditional)
  and **Prismari Charm** `{U}{R}` (3-mode modal: Surveil+Draw / 1 dmg to each of 1–2 "any" targets / bounce). The
  charm's mode 2 drove a small generalization: `foreach_current` is now `Option<Target>` (was `ObjId`), so
  `ForEachTarget`/`Each` binds **players** too. 558 tests. New cheap-win queue recorded in the ledger (S22 +
  counters-put-on-self each clear 2 cards).

## 2026-07-03 (late night, agent 6 — cheap-vein sweep)

- **cap+card(sos) — "counters put on self" `EventPattern` + Pensive Professor.** New `EventPattern::
  CountersPutOnSelf { kind }` + `GameEvent::CountersPut { obj, kind, count }` broadcast from the `AddCounters`
  executor (once per counter-adding event, battlefield only; reuses the `counter_added_this_turn` detection point).
  **Pensive Professor** `{1}{U}{U}` 0/2 = Increment + "whenever one or more +1/+1 counters are put on this, draw."
  Real-path test: a +1/+1 fires the trigger through the engine → draw; a -1/-1 does not. 560 tests. (Berta needs its
  {X},{T} Fractal ability + any-color mana trigger — assessing next.)
- **cap+card(sos) — S22 "cast an I/S this turn" + `Restriction::OnlyIf` activation gate + `artifact()` builder →
  Potioner's Trove.** `Player.instants_sorceries_cast_this_turn` (counted in `cast_spell` via the existing `is_is`
  bool, reset each turn) + `Condition::CastInstantOrSorceryThisTurn`. Found `OnlyIf` was only honoured for MANA
  abilities (mana.rs), NOT non-mana activated abilities — wired it into all three `legal_priority_actions`
  activation blocks (battlefield/graveyard/hand) via `conditions::holds_for_source`. Added a reusable `artifact()`
  CardDef builder (colorless permanent). **Potioner's Trove** `{3}` = {T}:any-color mana + {T}:gain 2 gated by the
  OnlyIf. Test proves the gain-life ability is offered iff an I/S was cast this turn. 562 tests. **Berta/Emil
  deferred** — both need `{X}` in an ACTIVATED cost (`activate_ability` hardcodes `x: None`) = a moderate cap.

## 2026-07-03 (night)

- **cards(sos) — Inkshape Demonstrator (5th Ward card) + Hardened Academic (`9be0eb3`), no new cap — LIFELINK
  CORRECTION:** reading `apply_damage` (whiteboard.rs ~1704) showed **lifelink is ALREADY wired** (source's
  controller gains life = damage dealt, CR 702.15) and it reads the COMPUTED keyword set, so a *granted* lifelink
  counts. The "lifelink not combat-wired" belief (mine AND the audit subagent's) was wrong. That unblocked:
  **Inkshape Demonstrator** `{3}{W}` 3/4 (Ward {2} + Repartee grants +1/+0 & lifelink UEOT — the 5th Ward card;
  test drives the real Repartee trigger then verifies the granted lifelink gains 4 life on 4 combat damage) and
  **Hardened Academic** `{R}{W}` 2/1 (Flying/haste + Discard→lifelink activated ability + S9 CardsLeaveYourGraveyard
  → +1/+1 on a target creature). Lesson: verify keyword wiring by READING the damage path, not by memory. 536 tests.
- **cards(sos) — sweep 4 no-cap cards from the refreshed unauthored-card audit (`5cc…` this batch):** an Explore
  subagent re-enumerated all 275 SOS names vs the 125 authored files and triaged the unauthored set against the
  ACTUAL engine (correcting several "not implemented" assumptions — notably `Ability::ConditionalStatic` IS
  live). Authored 4 fully-faithful cards on existing caps: **Rancorous Archaic** `{5}` 2/2 (Trample/Reach +
  Converge `EntersWithCountersValue{ColorsSpent}`); **Aberrant Manawurm** `{3}{G}` 2/5 (Trample + cast-I/S
  `PumpPT{power: ManaSpentOnTrigger}`); **Topiary Lecturer** `{2}{G}` 1/2 (Increment + `{T}: AddMana({G} ×
  PowerOfSelf)`); **Thornfist Striker** `{2}{G}` 3/3 — the **4th Ward card** (Ward {1} + Infusion team anthem as
  two `ConditionalStatic`s: `ModifyPT{+1,0}` and `GrantKeyword(Trample)` gated on `GainedLifeThisTurn`; I'd
  earlier called it blocked, the audit corrected me). Real-path tests each. 531 tests.
- **engine+cards(sos) — S10 flashback front-cap + Antiquities on the Loose + CreateToken ordering fix (`8ed83b1`):**
  `Condition::CastFromNotHand` reads the source spell's `flashback_cast` flag ("if this spell was cast from
  anywhere other than your hand" — the only non-hand cast the engine tracks). Antiquities `{1}{W}{W}` = two 2/2
  Spirit tokens + `Conditional{CastFromNotHand}` → `ForEach` your Spirits `PutCounters(+1/+1)` + Flashback
  {4}{W}{W}; first `spirit_token()` consumer. **The card's flashback test exposed a #61 ordering bug**:
  `CreateToken` staged its tokens as a DEFERRED whiteboard action, so a LATER same-resolution step (the counter
  ForEach) read live battlefield state and didn't see them → 0 counters. Fix: added a `CreateToken` arm to
  `interpret` that flushes (commits) after staging — the deferred→imperative boundary (#61), so "create tokens
  then affect them" works; rewrite pass still runs on the flushed batch (enters-with-counters unaffected). No
  regression across the 13 existing CreateToken cards. Ledger audit (below) found this vein mined out otherwise
  (Practiced Offense/Daydream/Group Project/Flashback each need a distinct hard cap). 523 tests.
- **cards(sos) — Tragedy Feaster (`1ca6d8e`, no new cap):** third Ward card — `{2}{B}{B}` 7/6 Demon, Trample +
  Ward—Discard a card (2nd non-mana Ward) + Infusion downside modeled as a `BeginningOfStep(End)` trigger gated
  on `Condition::YourTurn` (so it fires only on your end step) whose effect is `Conditional{ GainedLifeThisTurn
  ? Nothing : Sacrifice a permanent (any you control, chooser picks) }`. Real end-step-trigger tests both ways
  (no life gained → 2 perms → 1; gained → unchanged). All pieces already existed (YourTurn cond, GainedLife
  cond, Sacrifice effect, begin-of-step cap). 520 tests.
- **engine+cards(sos) — Ward helpers + Forum Necroscribe + MoveZone target-collection fix (`c335bcd`):** moved
  the Ward ability constructors to `cards/helpers.rs` (`ward`/`ward_mana`/`ward_discard`) per the code-org law
  (no sibling-card imports); Colorstorm now uses `ward_mana`. **Forum Necroscribe** (`{5}{B}` 5/4 Troll Warlock)
  = the 2nd Ward card and the **non-mana** Ward path: **Ward—Discard a card** (reuses the `can_pay_cost`/
  `pay_cost` `Discard` arms — the targeting player discards from *their own* hand) + **Repartee** reanimation
  (`SpellCastTargetingCreature(I/S)` → `MoveZone` graveyard→battlefield, same leaf as Lorehold Charm mode 2).
  Its Repartee test is the FIRST to drive a `MoveZone` target through the real trigger path and exposed a
  pre-existing gap: **`Effect::MoveZone` was missing from `collect_specs_into`**, so its `Target` was never
  collected at cast/trigger time (Pull-from-Grave & Lorehold tests bypassed casting via `resolve_effect`).
  Added the arm → reanimation now works through cast→target→resolve. 517 tests.
- **engine+cards(sos) — S17 Ward mana cap + Colorstorm Stallion (`96dbc35`):** Ward {N} (CR 702.21) as a
  `BecomesTargeted{ItSelf, by_opponent}` trigger → new `Effect::CounterUnlessPay{ what, cost: Cost }`
  soft-counter leaf. The targeting spell/ability is referenced via new `EffectTarget::Triggering`, threaded
  from `GameEvent::Targeted{…, source: StackId}` → `state.trigger_targeting_source` (trigger id → targeting
  stack id, mirrors `trigger_source_spell`) → `ResolutionCtx.triggering_stack`. On resolution the *targeting
  player* (not the Ward controller) is offered a `Confirm{PayToPrevent}` **only if `can_pay_cost`**, else the
  object is countered (`interpret_counter`). Reuses the engine `Cost` + `can_pay_cost`/`pay_cost` (made
  `pub(crate)`), so Ward{N}/Pay-life/Discard all share one leaf. `CardFilter::ItSelf` now matches in
  `enter_filter_matches` via an opt-in `Option<ObjId>` source (Some(watcher) only from the targeted path; all
  other callers pass None → behaviour preserved). Consumer **Colorstorm Stallion** (`{1}{U}{R}` 3/3, Ward {1}
  + haste + Opus-copy) — fully faithful; tests drive the real cast→target→ward→pay/decline path (opponent
  can't-pay ⇒ countered, pays {1} ⇒ Erode resolves & destroys it, can-but-declines ⇒ countered). 513 tests.
  ⚠️ `pay_cost` still lacks a `PayLife` arm (no-op) → add before Mica/Prismari's Ward—Pay-life.
- **cards(sos) — Snarl Song (`b58763d`, no new cap):** the dynamic-counters cap immediately made this a free
  card — `CreateToken{count:2, dynamic_counters:[(+1/+1, ColorsSpent)]}` (each of the two 0/0 Fractals enters
  with X counters) + `GainLife(ColorsSpent)`, Converge driven by S7's `ValueExpr::ColorsSpent`. Test at
  colors_spent=3 → two 3/3s + 3 life.
- **engine+cards(sos) — CreateToken dynamic counters + Wild Hypothesis (`9d2a856`):** added
  `Effect::CreateToken.dynamic_counters: Vec<(CounterKind, ValueExpr)>` — counter counts evaluated at
  resolution and baked onto the token's spec at materialize, so a token can enter with e.g. **X** +1/+1
  counters (the Quandrix "0/0 Fractal → X/X" pattern). Field added to all 13 existing CreateToken sites
  (`Vec::new()`) + snapshot regen; blast radius verified contained to those card files. Consumer: **Wild
  Hypothesis** (`{X}{G}` sorcery — 0/0 Fractal with X +1/+1 counters + Surveil 2); tests cover X=3 (enters a
  3/3), X=0 (bare 0/0), and the IR. Reusable for Snarl Song / Fractal Anomaly / Emil once their X-value
  ValueExprs (ColorsSpent exists; cards-drawn / distinct-names don't) land.
- **engine+cards(sos) — S21 cast-with-{X} trigger + Matterbending Mage (`134444d`):** added a `HasXInCost`
  arm to `enter_filter_matches` (the SpellCast-trigger filter path), so `SpellCast(All([ControlledBy,
  HasXInCost]))` = "whenever you cast a spell with {X} in its mana cost" now fires. Consumer: **Matterbending
  Mage** (`{2}{U}` 2/2; ETB return up to one OTHER target creature — reuses the new `Not(ItSelf)` cap; +
  cast-{X} trigger granting itself `CantBeBlocked` (a `Qualification`, honoured by combat `can_block`) until
  EOT). Integration test casts an {X} spell vs a non-{X} spell and asserts the grant fires only for the
  former. **Deferred Mind into Matter** (ledger item #3) after assessing it as a 3-cap card, not 1:
  dynamic-MV filter needs ctx threaded through the exhaustive `count_filter_matches`; plus MoveZone-from-Select
  and enter-tapped. Documented in the ledger.
- **engine+cards(sos) — another-target self-exclusion + Ascendant Dustspeaker (`1f6e284`):** `target_candidates`
  / `target_matches_filter` now carry a `source: Option<ObjId>` (threaded from the spell-cast / activate /
  trigger targeting sites; `None` at spell prechecks + unit tests) with a `CardFilter::ItSelf => source ==
  Some(id)` arm, so **`Not(ItSelf)` excludes an ability's own source at the targeting layer** ("another
  target creature you control"). Consumer: **Ascendant Dustspeaker** (`{4}{W}` 3/4 Flying; ETB +1/+1 on
  another target creature you control; begin-combat gy-exile). End-to-end tests drive the ETB via the real
  engine (`broadcast(ObjectMoved→Battlefield)` → `run_agenda`): counter lands on the *other* creature, and
  when the Dustspeaker is your only creature the trigger has no legal target and is removed (CR 603.3c).
- **engine+cards(sos) — multi-target MoveZone + Pull from the Grave (`12c41f8`):** the `Effect::MoveZone`
  materialize arm now consumes a whole `max>1` target slot (loops up to `spec.max`, one `Action::MoveZone`
  per chosen object) instead of one. Crux was that `chosen_targets`/`StackObject.targets` is a **flat**
  `Vec<Target>` — a "up to two target" slot flattens both picks into it (parse_targets pushes one entry per
  `(slot,cand)` pair). Documented invariant: a `max>1` slot must be the spell's **last targeting sub-effect**
  (the flat cursor can't recover slot boundaries; every real "return up to N" card satisfies it). Consumer:
  **Pull from the Grave** (`{2}{B}` sorcery — return up to two target creature cards from your gy to hand,
  gain 2 life; behaviour test proves BOTH return). Sibling cards need orthogonal caps, NOT this one:
  Divergent Equation = dynamic-X target count; Moment of Reckoning = repeatable modal modes.
- **cards(sos) — S15 impulse-play base + Practiced Scrollsmith (`d079eb0`):** adopted a ~90%-complete
  *orphaned, uncommitted* S15 implementation left in the shared tree by a terminated predecessor;
  reviewed it hunk-by-hunk against the ledger plan, confirmed it compiled + matched the warp/flashback
  idioms, then hardened it with tests I wrote (interpreter arm, ETB exile+grant, offer window/expiry) and
  landed the first consumer card. Ships `Effect`/`Action::ExileForPlay` + `Object.play_until_turn` (reset
  on any zone change) + a unified exile-cast offer loop honouring warp-recast (sorcery-speed) vs impulse
  (card's own timing within the window). **Practiced Scrollsmith** (`{R}{R/W}{W}` 3/2 first strike; ETB
  impulse-exile a target noncreature/nonland from your gy, castable until end of your next turn).
- **cards(sos) — S15 extended: top-of-library source + land-play (`0e17d3e`, `f7bb2c1`):** added
  `EffectTarget::TopOfLibrary(PlayerRef)` (resolve_target → `library.last()`) so `ExileForPlay` can source
  the top card, and a `PlayLand`-from-exile offer for impulse-exiled lands (land-drop-limited, window-gated;
  `play_land`→`move_object` already handled the zone). → **Elemental Mascot** (Opus: cast I/S → +1/+0; if
  5+ mana, impulse-exile top card until your next turn) and **Suspend Aggression** (exile target nonland
  permanent + top of library; per-owner "their next turn" windows fall out of the owner-keyed arm — a
  `Sequence` of two `ExileForPlay`, no new cap). S15 now DONE for all exile cases (3 cards); only
  graveyard-play (Ark of Hunger, milled-card-from-gy) remains, plus 2 cap-blocked cards (S7, S13). 473
  mtg-core tests green (+11 over baseline).
- **engine+cards(sos) — S13 restricted mana (`ffcc0df`):** "spend this mana only to cast instant and sorcery
  spells" (CR 106.6). `ManaSpec.restriction = InstantSorceryOnly` + a separate `ManaPool.restricted` bucket
  (empties with the pool); `allow_restricted` threaded through the payment path (`payment_units →
  can_pay_excluding/auto_pay_ex`, with thin `can_pay`/`auto_pay` wrappers so the ~26 existing call sites are
  untouched). Restricted pool mana + restricted sources (`restricted_mana_sources`, split from
  `producible_colors`) fold in only for an I/S cast; cast/offer sites pass card-is-I/S, ability costs pass
  false; `spend_from_pool` spends restricted-first; `add_mana` routes restricted production to the bucket.
  → **Hydro-Channeler** (`{T}: Add {U}` restricted; its `{1},{T}: Add any` restricted deferred — mana ability
  with a mana cost, unmodeled). Tests: restricted mana pays an I/S but not a creature/ability, from both a
  source tap and floating mana. 477 mtg-core tests green.
- **FINDING — trigger-system gap (`86c7b2e`, no code change):** tracing Abstract Paintmage's "beginning of
  your first main phase" trigger showed (1) `BeginningOfStep(phase)` permanent triggers are **never queued**
  (collect_triggers fires only End-step *delayed* triggers) and (2) a `Triggered` ability's `condition` field
  is **never evaluated**. So Essenceknit Scholar / Startled Relic Sloth / Ennis are latent-partial (their
  begin-of-step/conditional triggers pass only `resolve_effect`-direct tests, not the turn engine), and
  Abstract Paintmage / Fractal Tender / S16 end-step timing are blocked. Flagged to lead — a load-bearing
  trigger-system cap deserving its own careful commit, not a rushed rider.
- **engine+cards(sos) — multi-player ForEach + Splatter Technique (`6e6180c`):** `select_for_each` now spans
  ALL players when `chooser = EachPlayer` (area effects); every other `PlayerRef` stays single-player
  (Artistic Process etc. unchanged). → **Splatter Technique** (modal: draw four / 4 to each creature+planeswalker
  on both battlefields). Behaviour test asserts both sides take 4. 496 mtg-core tests green.
- **engine+cards(sos) — two small filter caps (`0622d36`, `40ee29c`):** `CardFilter::HasKeyword` ("target
  creature with flying" etc., reads computed keywords) → **Glorious Decay** (modal destroy-artifact /
  4-to-flyer / exile-gy+draw); `CardFilter::Multicolored` (≥2 colors) → **Mage Tower Referee** (colorless
  artifact creature, `SpellCast(Multicolored)` → +1/+1 self). Both filters wired in the target/enter/count
  matchers; integration/filter tests included. 493 mtg-core tests green.
- **cards(sos) — Abstract Paintmage (`00e18a9`):** closed the loop on the card that revealed the trigger
  gap. `{U}{U/R}{R}` 2/2 with a `BeginningOfStep(PrecombatMain)`/YourTurn trigger that floats restricted
  `{U}{R}`. Integration-tested: at your first main the trigger fires and the floated mana pays an I/S but
  not a creature; gated off on the opponent's turn. Exercises the begin-of-step-trigger + S13 caps together.
  487 mtg-core tests green (6 cards + 4 caps this session).
- **engine(triggers) — begin-of-step triggers + `Triggered.condition` NOW WIRED (`20965a8`):** fixed the
  gap found above. `collect_triggers` queues each permanent's `BeginningOfStep(phase)` trigger at phase
  transitions; a non-intervening-if trigger condition (CR 603.2) gates queueing, an intervening-if (CR
  603.4) is re-checked at put-on-stack + resolution (`trigger_intervening_if_holds`). Scoped to
  condition-bearing triggers → `condition:None` triggers unaffected (480→484, no regressions). **Turn-engine
  integration tests** (broadcast PhaseBegan → run_agenda → resolve_top) prove all 4 revived cards fire and
  gate: Startled Relic Sloth, Essenceknit Scholar, Primary Research, Additive Evolution — now genuinely
  fully-implemented. Unblocks Abstract Paintmage (just needs its first-main trigger authored). Proposed a
  systemic audit rule in the ledger: every Triggered ability should fire once through the real engine.
- **engine+cards(sos) — Select-based Exile + Heated Argument (`5596fb4`):** `Effect::Exile` now handles
  `EffectTarget::Select` in `interpret` (via `select_for_each`), returning whether the selection reached its
  `min` so a wrapping `IfYouDo` gates correctly (previously it fell to `materialize`'s Target-only arm →
  no-op yet reported "done" → reward fired without exiling). → **Heated Argument** (6 to target creature; you
  may exile a gy card → 2 more to that creature's controller via `ControllerOfTarget(0)`). 480 mtg-core tests
  green. Session total: 5 cards + 3 caps.
- **gym (the combat-judgment ladder — cause definitively isolated):** three controlled experiments
  on "why does it chump-block the trampler at high life": (1) 2.8-swine-500k = user's reshaped
  reward (card-dominant Φ 0.5/0.3/0.2, coef 0.1, 50→80% anneal) → small directional nudge
  (swine-gated block@≥15 life 1.000→0.938), insufficient; (2) blocking-observability enhancement
  (per-attacker blocked-by count + pending-blocker source flag, F_PERM 43→44, equivalence
  fingerprints unchanged — they're obs-independent) after an audit proved ganging was never
  representable; (3) 2.9-swine-500k (single variable = obs) → ZERO behavioral change
  (block_double 0.30→0.18 identical to blind 0.29→0.19). CONCLUSION: the symmetric mirror
  equilibrium is the bottleneck — no gradient punishes the chump when both sides chump. The
  untested PPO lever = an asymmetric/punishing opponent; groundwork all in place.
- **gym (fleet decision_stats parity):** the fleet default now emits per-decision stats (was
  silently dark — the 2.8 run had to use the batched path); "TrackedStats present" added as a
  permanent gate. Instrumented 500k runs now ~460 steps/s ≈ 18 min (was 19+ on batched at equal
  n_envs, with room at higher env counts).
- **17lands winprob track (experiments/winprob17l/):** SOS PremierDraft replay data (583k games)
  → supervised win-prob models: logistic AUC 0.739 / MLP 0.764, superbly calibrated. Standardized
  coefs: COUNTS beat stats (lands ±0.84 > hand ±0.68 > creature count ±0.57 > life ±0.42;
  power-sum ~0). Audit vs the gym's Φ: already near-optimal (AUC 0.710/0.739, ρ=0.965); one
  indicated tweak = shift ~0.1 weight power→life. Frozen logistic blob + flag-gated adapter stub
  shelved (re-center for the 0.554 collector base rate before use). In-gym A/B held for user.
  Interactive report artifact published (582k games, card GIH rankings).
- **Stochastic MuZero track (experiments/stochastic_muzero/):** user-directed; M0 feasibility GO —
  LightZero v0.2.0 in an isolated uv Python 3.11 venv (3.14 unsupported), C++ stochastic-MCTS
  compiles, CUDA torch live, action masks native; the abi3-py39 mtg_py wheel imports in both venvs
  (same engine build). v1 design: single-agent stochastic MDP (opponent+draws = chance codes),
  NOT LightZero's perfect-info board_games mode. Crux to test: does sub-decision-factored lookahead
  reach the block→consequence horizon that model-free PPO provably can't get gradient for in a mirror.
- **SOS (agent chain):** predecessor retired at 105 cards/25 caps; successor added S14 token-copy,
  monocolour hybrid {N/C} (+ fixed hybrid pips DROPPED FROM CAST PAYMENT — Honormancer cast 1 short,
  all-hybrid would've been free), discard-cost, X→ETB-counters fix (enters-with-X was silently 0),
  filtered look-pick, + T2 sweep → ~117 cards / 29 caps / 462 tests; retired clean with S15/S13
  scoped site-by-site. Third agent adopting orphaned S15 WIP (review-hard protocol) → S13.
- **UI:** hybrid mana pips render as official split-circle symbols (game client + lobby, canonical
  pair-order normalization); CMYK reskin verified across themes.

## 2026-07-03 (evening)

- **UI (CMYK identity shipped, game+lobby):** user picked the "CMYK riso" direction from style
  mockups → full reskin as pure CSS-token pass (no layout/JS-behavior change): indigo ink on paper
  (light) / cream ink on indigo stock (dark), cyan/magenta/yellow spot-offset shadows carrying game
  state (magenta+cyan=legal, yellow+magenta=attacking), no decorative tilts. Auto/Light/Dark toggle
  (defaults to system, explicit picks persisted to shared localStorage `mtg_theme`, no-FOUC).
  Contrast 12-14:1 both themes. (be430ae game+embedded, 0424d7f lobby)
- **Replays end-to-end slim:** mtg-core versioned compact format — zone deltas (48108d4) +
  characteristics dedup (d4ac691) → 23.3MB game = 504KB on disk (46×); server saves compact,
  serves full for compat + opt-in `?format=compact` (3f5bd0d, e0444d6); client fetches compact +
  reconstructs client-side with full-endpoint fallback, identity-verified vs full rendering
  (084f726) → browser holds ~0.5MB decoded instead of 23MB (47-53×). Also fixed: replay ids with
  dots (the 2.7 version tags) were 400ing on GET — id sanitizer widened, traversal still blocked
  (2a44b65); fast-rate Pause unresponsive (event-loop starvation from setInterval) → self-scheduling
  frame loop + pointerdown controls (e7c958a); replay bar pinned stationary on mobile (0d42825).
- **gym (tier-3 swine verdict — training healthy, combat judgment did NOT emerge):** 2.7-swine-200k
  converges (greedy 0.895 vs random base 0.535, productive 1.000) but greedy block_rate=1.000 — it
  chump-blocks the trampler like bears. Key control: `block_double` metric shows the vanilla bears
  mirror double-blocks MORE (0.21-0.28) than swine (0.13-0.18) → doubles are pile-on, not anti-trample
  ganging. Finding: MIRROR self-play sits in a stable block-everything equilibrium (blocks rare,
  ~2.4/game; sparse ±1 credit) — the lever is a non-mirror/asymmetric setup, not more steps.
  (fc96f80 Deck::Swine, bb43627 block_double)
- **engine (SOS 73 cards / 14 caps):** S5 Opus (SpellCast trigger queue + ManaSpentOnTrigger +
  ctx-aware Conditional), S8 Repartee (SpellCastTargetingCreature), S6 Increment (Power/ToughnessOfSelf),
  S9 flag, player-as-target (Effect::TargetPlayer → TargetKind::Player slots; Cost of Brilliance /
  Dissection Practice / Exhibition Tidecaller). 377 mtg-core tests.
- **M3 engine side COMPLETE; port landed with numbers:** PyGame→Session port (8800205) deleted the
  per-game OS thread + mpsc channels + PyAgent — gated byte-for-byte by the equivalence fingerprints,
  throughput 1484/1672/1626 → 2254/2730/2714 fps at 32/128/256 envs (**1.5–1.67×**, ad25ad6).
  The agent-removal/Send split was PRICED AND REJECTED after prototyping: the Engine-wrapper+Deref
  plan breaks two-phase borrows on the pervasive `e.method(e.state…)` pattern (recorded as
  do-not-revisit), and `Agent: Send` is a boundary-LAW change forcing Rc→Arc through single-threaded
  CLI code. DECISION: **thread-pinned fleet groups** — each worker thread creates AND steps its own
  Sessions, nothing is Send, zero engine changes, no unsafe in production (RESUMABLE_ENGINE.md
  c0bbc55). The fleet stepper (M3.4, in mtg-py: N workers, batched obs buffers, ONE PyO3 crossing
  per micro-tick) is in progress — phase-gated by the same fingerprint suite + bench.
- **infra:** version-prefixed TB runs (`<major>.<minor>-slug`, e6d00d7, backfilled 2.0-2.6);
  replay recording default-on per training run (--replay-every 25k, d501d24); run-notes TEXT +
  Custom Scalars dashboards (4e0108d); n_envs probe: fps flat ~1.6k at 32/128/256, single Python
  pump core pegged while GPU 13-25% → M3 fleet target.

## 2026-07-03 (afternoon)

- **webui (deck builder shipped):** lobby "Decks" tab — create/duplicate/edit/save custom decks
  (searchable+filterable catalog, tap-to-add, steppers, mana curve, mobile-first), persisted to
  gitignored data/decks/ (survive restarts), full REST CRUD (/api/decks GET/POST/PUT/DELETE) +
  /api/cards catalog; per-seat picker now reads the live deck list; customs resolve in game
  creation after presets (3b4f216, 1325456, eebf28a). Catalog switched from the 37-card preset
  pool to the FULL registry via new `CardDb::iter()` (76976ee engine, 5a90f9e server) — 91 cards
  incl. the whole SOS pool; art manifest regenned for all of them.
- **webui (replays surfaced + overwrite bug):** lobby "Saved games" section lists every finished
  game from data/replays/ (survives restarts; ▶ /play?replay=<id>) (bd6d810); fixed game-id
  counter resetting to 1 per restart and silently clobbering old replay files — now seeds above
  the max saved id (00dcd44). NOTE: some pre-fix replays 1–12 were already clobbered by test games.
- **gym (TB infra + ladder + watchable replays):** run/notes TEXT summaries (--notes + auto config
  block) + Custom Scalars "key metrics" dashboard on every run, backfilled into the 4 existing runs
  (tb_meta.py, 4e0108d); GameLengthCallback (game/turns_mean); LadderEval finally wired into
  train_selfplay's default callbacks (was dead-code drift since June) — old runs can't be backfilled
  (pool pruned), fresh bears export run shows a real ladder (0.70/0.575/0.55/0.50 vs 10/25/50/75%).
  Exported watchable replays to the lobby: 9-game bears greedy learning arc (step 0→200k), the 3
  analyzed bears games, 1 heralds final.
- **gym (bears sanity verdict):** combat mirror converges (greedy: attack/block/productive all
  1.000, win 0.882 vs random baseline 0.502, 0% truncation); learns UNCONDITIONAL attack-all/
  block-all — correct equilibrium for a vanilla 2/2 mirror (trades are always even), so bears
  validates machinery but not combat judgment; next tier needs real tradeoffs (removal/asymmetric
  P/T). Deck::Bears mirror added to mtg-py (c495b19).
- **engine (M3 STARTED — resumable step API):** user greenlit; design doc committed
  (docs/design/RESUMABLE_ENGINE.md, bf6d6af): stackful-coroutine session (all ~24 ask-sites funnel
  through Engine::ask → yield there; game logic byte-identical, ~300 tests protected), blocking
  Agent trait becomes a driver loop, endgame = GIL-free Rust fleet stepping, ONE PyO3 crossing per
  micro-tick. Search/clone-for-MCTS explicitly CUT (user: learned search à la MuZero only, no
  engine support needed) — GYM_PLAN §8-M3 re-scoped to throughput (f1dd9b3). M3.0 spike underway.
- **sos-cards (standing grind):** 40 fully-implemented cards, 8 caps (MoveZone, Discard,
  Counter+stack-statics → Surrak fully faithful = Selesnya 18/18 complete, Sacrifice, Surveil,
  Infusion, token-keywords) + exhaustive-match loud guard (unwired IR leaves can't silently no-op;
  new variants are compile errors). 318 mtg-core tests. CardDb::iter() for the UI catalog.

## 2026-07-03

- **webui (mobile package):** game client is now phone-playable (user drives it over Tailscale).
  One `@media (max-width:760px, max-height:500px)` block: single scrollable column (opp strip top →
  board → hand → your strip + decision prompt as a sticky bottom sheet), log hidden behind a topbar
  toggle, step bar scrolls. Touch: long-press (~450ms) is the ONLY preview on touch (hover preview
  gated to `pointerType==='mouse'` — taps no longer open stuck previews anywhere incl. zone/decklist
  modals), on-screen "⏩ Pass turn" button (was Enter-only), bigger tap targets, heralds topbar
  quick-link. Desktop pixel-identical; embedded mirror kept byte-identical; Playwright-verified at
  390×844/844×390/1280×800. (c0fb637..63c5135) Lobby deck-viewer got the same touch fix separately:
  tap-toggle centered preview, dismiss on any next tap (df52e76).
- **engine + server ("heralds" RL sanity deck):** Mist-Cloaked Herald authored ({U} 1/1 "can't be
  blocked" as a printed self-static over `Qualification::CantBeBlocked`, grp 118, cards/rix/) +
  `heralds` preset (40 Herald + 20 Island) in `preset_deck` (e4b366f, 00aa120); registered in
  DECK_NAMES + lobby picker (5ca543a); art manifest regen for Island/Herald (bd1679f). 235 mtg-core
  tests green.
- **gym (training verified working + shaping default ON):** heralds 200k self-play converges to
  provably-optimal play: greedy attack_rate 1.000, productive_rate 1.000 (0 wasted windows/~3.5k),
  0.972 vs random (baseline 0.478), block 0. cast/playland_rate plateaus (~0.83/0.71) proven to be a
  per-window mutual-exclusion METRIC ARTIFACT, not a learning failure → added `productive_rate`
  StatDef as the artifact-free convergence gauge (a610017). PBRS shaping now defaults to coef 0.5
  annealed (0fe3cb0; eval stays raw ±1); caveat: hurts very short budgets (16k demo → below random;
  that league test pins coef 0). `Deck::Heralds` wired through mtg-py (bdf1f40). Gotchas: concurrent
  same-parent pool_dirs collide on the shared ref-checkpoint path; CUDA nondeterminism under
  concurrent GPU jobs can flip A/B conclusions — compare sequentially. TB: /tmp/mtgenv_tb/.
- **sos-cards (new long-term workstream):** Secrets of Strixhaven (`sos`, 271 cards) triaged into
  ease tiers → `docs/plans/SOS_CARDS.md` (f9ddafb): 74 authorable now (T1+T2), 142 need one small
  cap each (top-7 caps unlock ~79), 55 deferred (36 MDFCs per first-pass scope + big subsystems).
  Grinding easiest-first.
- **engine (no-target cast hang fixed, 2a0b85d):** auras (no spell effect) bypassed the
  castable-targets gate → Pacifism on an empty board was offered, then `ChooseTargets` with 0
  candidates deadlocked at the agent boundary (no escape existed). Fix: `card_castable_targets`
  gates ALL casts (modal-aware + aura enchant spec, factored so offer & decision share one spec);
  `cast_spell` rewinds instead of asking when a required slot is unsatisfiable (601.2f — nothing
  paid yet); debug assert "every DecisionRequest has ≥1 legal response" live at the `ask` seam
  (246 tests green with it). DeclareAttackers/Blockers can't hang (empty declaration always legal).

## 2026-06-14

- **gym (#68 done):** tracked_stats — action-rate summary metrics to TensorBoard, encapsulated +
  extensible. mtg-py `decision_stats.rs` summarizes each finalized engine decision
  (DecisionRequest=opportunities, DecisionResponse=taken) into a flat record, drained via
  `PyGame.take_decision_stats()` → `info['decision_stats']`. `python/mtgenv_gym/tracked_stats.py`
  is a one-`StatDef`-per-stat registry + accumulator + SB3 callback logging `stats/*` per rollout
  (cast/attack/block/playland rate). Wired through `MtgEnv.step` + `BatchedSelfPlayVecEnv` + both
  train loops. Verified tfevents carry the curves. Also verified the post-collapse stop stream and
  kept `full_control=false` for training (71 vs 311 dec/game, ~2.7× more games/s than full control).
- **gym (#66 done):** Tier-1 obs — tie a decision to its source + candidates. Per-row
  is_source/is_candidate flags on bf/hand/stack, me/opp player-candidate globals, and a resolved
  source-card one-hot (`decision_ids`) into both feature extractors. Uses the engine's new
  `ChooseTargets.source` (6fe1580) so triggered/reflexive abilities resolve their source even before
  hitting the stack — selesnya ChooseTargets source-id coverage 44%→100% (Earthbender case fixed).

- **engine (#67 done):** Finished the stop-policy collapse end-to-end. After webui migrated
  gre-server off the dead flags (cbae678) + dropped the client-side `priorityAutoPass` narrowing
  (engine is authoritative now), engine deleted `StopConfig.auto_pass/smart_stops/resolve_own_stack`
  + their setters + the `AutoPassOption` enum + `set_auto_pass_option` + the `StopStateView`
  smart_stops/resolve_own_stack fields (f9a7b1f). `StopConfig` = `full_control` + overrides +
  manual_mana; the view echoes stop state when not under full control; `set_arena_auto_pass` stays
  (gym uses it) → `full_control=!on`. Fully green: 233 mtg-core + 20 gre-server + 16 mtg-py. Also
  added `source: Option<ObjId>` to `DecisionRequest::ChooseTargets` (6fe1580) so trigger/reflexive
  target decisions carry their source for the gym's Tier-1 obs (#66).

- **webui:** Removed the dead stop-policy plumbing (#67, engine collapsed it to one `full_control`
  knob in `5e106df`). Dropped `auto_pass`/`smart_stops`/`resolve_own_stack` from the `Stops` carrier
  + the `smart_stops=true` force (driver.rs), the `ServerMsg::Stops` fields (protocol.rs), the WS
  query flags + `SetOption` cases + `stops_msg` (server.rs), the CLI `autopass`/`smartstops`/
  `resolvestack` commands + status (cli.rs), and the `render.rs` reads — keeping `full_control` +
  per-step overrides + `manual_mana`. Stopped calling the soon-gone `set_arena_auto_pass`/
  `set_smart_stops`/`set_resolve_own_stack` (set `full_control` directly per human seat). **Removed
  the client-side `priorityAutoPass`/`currentPhaseIsStop` narrowing** — the engine's `should_auto_pass`
  IS that rule now, so the client just renders the windows it surfaces (Full Control toggle + the
  Enter-hold skip-turn remain). Verified live: Stops msg is `{full_control, per_step}`; FC OFF →
  stops only where you can act, FC ON → every window; games complete. 20/20 tests green. Engine can
  now drop the dead `StopConfig`/`StopStateView` fields. (Incidental: added `source: None` to the
  options.rs ChooseTargets test for the engine's new #66 field.)

- **engine (stop policy collapse):** Per user, unified the stop config to a **single full-control
  knob** (the separate auto_pass/smart_stops/resolve_own_stack were a three-flag engine model under
  a one-toggle UI veneer + a client-side filter flagged as tech-debt). `should_auto_pass` is now one
  rule, canonicalized engine-side = exactly what the web client used to apply: full control → stop
  every window; else auto-pass unless you have a meaningful (non-mana) action AND it's a marked stop
  or an opponent's object is on the stack. The three old flags are behavior-dead (kept as
  display/transport fields + setters so gre-server/mtg-py still compile; `set_arena_auto_pass(on)`
  now drives `full_control=!on`). Default = full control (headless/replay/tests prompt every window,
  unchanged). 4 stop tests rewritten for the single rule; 233 mtg-core + 20 gre-server green. Commit
  5e106df. Follow-ups delegated: webui removes the dead gre-server plumbing + client filter (#67);
  gym verifies/chooses its training stream (#68 pt 1).

- **webui:** No-mana-cost cards (tokens, abilities on the stack, "—" cards like Living End) now
  render **no** cost pips instead of a bogus "0". The engine already distinguishes None vs Some({0})
  (`CharacteristicsView.mana_cost: Option<ManaCost>` → wire `null` vs object; tokens get `None` via
  `create_token`'s `Default`), so no engine change — the bug was the game-board `manaPips` fallback,
  which fabricated "0" for `cmc === 0` when the structured cost was absent. Now: structured cost
  present (incl. a real `{0}`) → render it ("0" preserved); absent → render nothing unless a positive
  CMC is implied (defensive). Fixed in `embedded_client.html` + `main.ts` (lobby deck viewer already
  returned `[]`). Verified: a token shows 0 pips, a genuine `{0}` still shows "0".

- **webui:** Wired manual mana activation (#36) now that the engine offers it (`cf23567`). Turn it
  ON for every human seat (`set_manual_mana(p, true)` in `apply_stops`/`engine_with_stops`/
  `room_engine`). The engine appends an `ActivateMana` per untapped source; `options.rs` already
  labels it "Tap … for mana" + `action_obj` → the source, so clicking a highlighted land taps it
  (multi-color sources fall through to the existing `ChooseColor` select-many; Ba Sing Se's
  mana-vs-earthbend disambiguates via the variant menu). Added an `is_mana` flag (parallel to
  `Prompt.options`, Priority only) so the client auto-pass rule counts only **non-mana** actions —
  mana being available is never itself a reason to stop. Verified live: tapping a Forest floats
  `{Green:1}` (live via `ManaPoolChanged` → `poolPips`); zero mana-only-on-empty-stack frames across
  a full game (no extra stops — server still surfaces only the 4 default stop phases). Tests:
  `manual_mana_seat_is_offered_activatemana_marked_is_mana` + regenerated decide-frame snapshots.
  Source-control (cast pays floated mana first, untapped lands stay) is proven engine-side. Default
  OFF keeps it out of the gym agent's action space.

- **engine (#36, manual mana):** The engine now offers **manual mana activation at priority** so a
  human can tap SPECIFIC sources (e.g. to control which lands fund a spell) — while keeping mana
  abilities out of auto-stops and the RL agent's action space. New per-seat `StopConfig.manual_mana`
  (default OFF) + `Engine::set_manual_mana`. When ON, `legal_priority_actions` appends one
  `ActivateMana` per untapped usable source; `activate_mana_ability` taps it and floats the mana
  (asking `ChooseColor` for multi-color sources), no stack (CR 605.3). `priority_round` keys the
  SmartStop/auto-pass decision off **non-mana** actions, so manual mana never forces a stop; and the
  enumeration is hidden from default seats, so the gym agent is unaffected (it auto-pays). Casting
  already spends floating mana first (#59), so the chosen sources pay and others stay untapped.
  Added `mana::usable_mana_sources` + `mana::produce_mana` (with the TapCreatureForMana bonus). +1
  test (`manual_mana_lets_a_seat_tap_chosen_sources_then_cast_from_float`), 233 mtg-core green.
  Commit cf23567. UI wiring delegated to webui (set the flag for human seats, click-a-land-to-tap,
  ChooseColor for duals); gym to confirm no ActivateMana in the agent mask.

- **engine (#65):** Fixed Dyadrine — its attack drew a card + made a Robot **even when no counters
  were removed**. The "you may remove a +1/+1 counter from each of two creatures you control. **If
  you do**, draw + create a Robot" was modeled as `Optional{ Sequence[ForEach{min:2}, Draw,
  CreateToken] }`, so the `Sequence` ran the reward regardless of whether the `ForEach` could reach
  two. Added a general `Effect::IfYouDo{cost,reward}` and made `whiteboard::interpret` return a
  *performed* bool (a `ForEach` reports done only when it reaches `min`; an `Optional` only when
  accepted **and** its body performs). Dyadrine is now `IfYouDo{ cost: Optional{ForEach}, reward:
  Sequence[Draw, CreateToken] }`. Gating rides the ForEach's real execution, not a chars-only
  `CountAtLeast` (the condition system can't see +1/+1 counters). +1 regression test (yes-but-only-
  one-counter → no removal, no draw, no token); 232 mtg-core tests green. NB: gre-server must be
  rebuilt/restarted to pick this up.

- **webui:** Fixed combat-lane bug — a blocker that KILLED its attacker (attacker dead, blocker
  survived) went invisible during the damage/end-combat steps and only reappeared at main 2.
  `engagedIds` pulled every declared attacker/blocker out of its band, but `renderCombatLane` did
  `if (!atk) return` — so a matchup whose attacker died was skipped entirely, dropping the surviving
  blocker (it was in neither band nor lane). Fix: (1) only add STILL-ALIVE creatures to `engagedIds`;
  (2) render whoever survived each matchup — a cell with a dead attacker now shows its surviving
  blocker(s) alone; (3) `matchupCell` handles a null attacker. Mirrored in
  `embedded_client.html`/`main.ts`. Verified via Playwright (Llanowar that out-fought its attacker
  now renders once in the lane, not duplicated, never invisible).

- **webui:** Fixed hover-preview not working for spells/abilities on the stack. The `.stack` overlay
  is `pointer-events:none` (so its gaps don't block the board beneath it), but that also made
  `elementFromPoint` — which `refreshPreview` uses to find the hovered `[data-preview]` — hit the
  board *behind* the stack instead of the stack card. Added `.stack .card { pointer-events:auto }`
  so the cards capture the pointer while the container stays click-through. Also restores
  click-to-target a spell on the stack. CSS-only (embedded_client.html + synced style.css);
  verified via Playwright (hovering a stacked Sazh's Chocobo now shows the full-card preview).

- **design (#60 — END-TO-END PER-CARD AUDIT, COMPLETE):** Rebuilt the card-test harness on the engine's
  real play seam (`cast_spell`/`play_land`/`activate_ability`/`resolve_top` + `run_agenda` +
  `declare_attackers_explicit` + `legal_actions`) and drove **all 18 Selesnya cards** through the actual
  cast→pay→resolve loop — real auto-paid mana, targeting, modal choices, costs, and ETB/landfall/attack/
  becomes-targeted triggers — asserting every oracle clause against resolved state (not IR shape). **18/18
  confirmed; re-baseline demoted NO `fully_implemented` flag** (the 17 `true` were unvalidated builder/inline
  defaults; now validated. Surrak stays `false` — inert CbC, sanctioned). **1 bug found → #64 (engine fixed):**
  Keen-Eyed graveyard-exile fizzled (`target_legal` battlefield-only); the old resolve-level test masked it by
  bypassing `resolve_top`'s guard — the exact over-stated-flag class #60 targets. Verified #56/#57 mana fixes
  end-to-end. Highlights: Dyadrine counters=mana-spent (real X-cast → 5/6), Ba Sing Se earthbend {2}{G}+{T}
  (#57), Earthbender landfall reflexive ≥4 buff, Dyadrine/Lumbering attack triggers. ~25 new end-to-end tests;
  231 mtg-core green, clippy clean. Matrix + report in `docs/plans/SELESNYA_LANDFALL_CARDS.md`.
- **engine (#64 — graveyard/stack target fizzle):** **Spec-aware `targets_still_legal` (f445b6e).** `target_legal`
  hardcoded `zone == Battlefield`, so `resolve_top`'s re-check wrongly countered a graveyard target (Keen-Eyed
  Curator's `{1}: exile a graveyard card`) — and would have a stack target. Now each chosen target is re-checked
  against the zone its `TargetSpec` requires (CR 608.2b): `CardInZone` → that zone, `StackObject` → the stack,
  else battlefield (so a battlefield target that *died* still correctly becomes illegal). `targets_still_legal`
  takes the specs (collected in target order via `target_specs_for`, modal-aware); all 4 `resolve_top` call sites
  pass them. Un-ignored design's `keen_eyed_exile_via_full_activation` (now green). 219 tests pass.

- **webui:** Post-#59 (mana-pool rework, `3c35cff`) full re-verification at the UI layer. `ManaPool`
  wire shape unchanged (`{amounts:{Color:n}}`) → `poolPips` needs no change; `mana_pool` still in
  PlayerView; new `ManaPoolChanged` event drives a live view refresh. New driver tests: (a)
  `badgermole_plus_earthbent_forest_makes_mightform_hard_castable` — earthbent (creature) Forest +
  Llanowar = 4 creature-tap mana → the `{2}{G}{G}` hard cast is now offered (alongside Warp); (b)
  `ba_sing_se_earthbend_taps_itself_plus_three_other_lands` — drives the real activation via a
  scripted agent (`skip_opening_deal` + `run_game`) and reads the PlayerView: Ba Sing Se's `{T}` +
  **3 OTHER** lands tapped = 4 (the #57/#59 fix; old bug tapped only 3). Live-UI screenshots: Ba
  Sing Se tap pattern (4 tapped / 1 untapped, `/tmp/basingse_ui.png`) + floating-mana pool
  (`/tmp/pool_shot.png`). Erode (#61) + Fabled (#58) are engine-correctness (no distinct UI
  affordance) — covered by mtg-core tests `erode_destroys_the_target` /
  `fabled_passage_untaps_the_land_at_four_lands`; the UI renders the resulting board state. 19/19
  mtg-gre-server tests green.

- **engine (#59 — mana-pool payment rework):** **Pay through the real pool (3c35cff + e46fe8d); fixes #56 + #57.**
  Replaced the source-based auto-tap (each source = 1 mana, tap-to-pay, bonus dumped to the pool afterward)
  with proper pool-based payment: (1) `pay_cost` pays NON-mana components (TapSelf/Sacrifice/Crew) FIRST and
  commits them, so a source tapped/sacrificed for its own cost is excluded from the mana sources — **Ba Sing
  Se's `{2}{G},{T}` now taps itself for `{T}` + 3 OTHER lands, never itself for both (#57)**; `can_pay_cost`
  mirrors it via `mana::can_pay_excluding`. (2) `auto_pay` taps a sufficient set, produces each tapped source's
  FULL output (base + Σ Badgermole bonus) into `player.mana_pool`, then spends the cost from the pool — floating
  mana first; surplus stays FLOATING and `empty_mana_pools` clears it at the end of every step (CR 500.4).
  Folds in the #56 base+bonus model. (3) Live `GameEvent::ManaPoolChanged` refresh after payment + mana-ability
  resolution (gated from event-log/replay to avoid churn) so webui shows floating mana mid-resolution (#62).
  Wire shape unchanged. Acceptance tests: Ba Sing Se taps-self+3-others (2-others unaffordable); floating mana
  persists across two payments in a step; Badgermole/earthbent afford bonus costs. 215 tests green, 1 ignored
  (design's #63 graveyard-target-fizzle repro). Design's #60 audit unblocked.

- **engine (#61 — effect ordering fix):** **Flush deferred actions before imperative effects (3487141).**
  `resolve_effect` accumulated deferred whiteboard actions (Destroy/Tap/counters) and committed at the END,
  but Search/AddMana run imperatively during interpret — so Erode's `Sequence[Destroy target, fetch a land]`
  let the fetched land enter while the doomed creature was still on the battlefield, wrongly firing its
  landfall. Fix: `flush_pending()` commits the accumulated whiteboard before each imperative effect, so a
  resolving spell's instructions take effect IN ORDER with intermediate state visible across the
  imperative/deferred boundary; deferred-only runs still batch (replacement/prevention unaffected). Regression
  test (verified red without the flush): Erode a landfall creature → no landfall; vanilla → land still fetched.

- **engine (#58 — Fabled Passage NOT reproducible):** **Conditional untap works through the full path (96352df).**
  Drove the real flow — activate `{T},Sacrifice`, pay the cost (sacrifices Fabled Passage), resolve — and the
  fetched land untaps correctly (3 other lands + fetched = 4 → untapped; 2 → tapped). All 3 suspect checks pass
  (`Searched(0)` resolves, `CountAtLeast` counts the fetched land + excludes the sacrificed source, `Tap{tap:false}`
  untaps). `bcff1cd` already works. Added full-path coverage closing the test gap; flagged the lead it's likely a
  Ba Sing Se landfall→Earthbend re-tap or a display issue — need the user's board state.

- **engine (#56 — Badgermole point-fix):** **Count the TapCreatureForMana bonus in affordability + payment
  (fdfea6c).** The solver treated each source as 1 mana; the bonus was added to the pool only AFTER selection,
  so bonus-dependent casts were wrongly blocked. Modeled each creature mana-source as base + bonus mana units;
  only genuinely-surplus bonus floats. Regression: Llanowar+Forest+Badgermole afford {2}{G}; two creature
  sources + Badgermole afford {2}{G}{G}. (NB: lead queued #59 pool-based rework that would supersede this +
  fix #57 — scope decision pending.)

- **webui:** Verified the engine's Badgermole mana point-fix (#56, `fdfea6c`) at the UI projection
  layer. New driver test `badgermole_bonus_makes_warp_mightform_castable` builds the user's exact
  board (Badgermole Cub + Llanowar Elves + Forest untapped, Mightform in hand), pulls
  `Engine::legal_actions`, and projects through `options::prompt_for` — the literal `decide` frame
  the cast menu renders. Result: the cast menu now offers exactly `["Warp Mightform Harmonizer
  {2}{G}"]` (Llanowar's {G}{G} via Badgermole + Forest's {G} = 3 mana); the {2}{G}{G} hard cast is
  correctly withheld at 3 mana (needs a 4th, e.g. an earthbent Forest). The live :8080 binary
  already links the fix.

- **webui:** Combat clarity — blockers now physically move in front of the attacker they block. New
  **combat lane** at the midline: once blocks are declared, each attacker is pulled out of its band
  into a matchup cell with its blocker(s) stacked facing it across a ⚔ (attacker toward its own
  side — opp on top, you on the bottom — mirroring the board); unblocked attackers stand alone with
  a "↓/↑ unblocked" marker. Creatures snap back to their normal rows when combat ends (lane hidden
  when `view.combat`/blockers empty). `computeCombat`/`renderCombatLane`/`matchupCell` + `engagedIds`
  skip in `renderHalf`, mirrored across `embedded_client.html` / `main.ts` / `index.html` (Vite
  shell) / `style.css`. Verified via Playwright on a synthetic combat frame: blocked cell stacks
  attacker over blocker, unblocked cell solo, engaged creatures move to the lane (not duplicated),
  zero console errors. Works in live play and replays/spectate (god-view combat too).

- **engine (#55 — training-hang safety net):** **Loop-guard aborts wedged games to a draw + names the loop
  (e502ba6); deterministic-greedy termination test (dd6e9b6).** A demo self-play training run froze for hours
  spinning one CPU core — a single `env.step()` entered an in-engine infinite loop (no `Agent::decide` call, so
  the Python `max_decisions` cap couldn't catch it). **Part 1 (done):** ALL THREE unbounded engine fixpoint loops
  (`run_agenda` SBA/trigger stabilization, `priority_round` priority passing, and `whiteboard::rewrite` the
  replacement/prevention fixpoint that runs *below* them inside `commit`/`resolve_top` — 8d8ac10) now carry a hard
  iteration ceiling (100k / 1M / 100k — no legal game comes close); tripping it aborts that game to a draw and
  logs the loop + turn/phase/stack. The engine can NEVER spin forever; a wedge degrades to a logged draw that
  *self-diagnoses* the exact site.
  **Part 2 (root cause — RESOLVED, = #49):** swept ~17.5k games (12k random + 1500 greedy without auto-pass; 4000
  greedy/random WITH auto-pass, gym's config) → **zero** trips, and audited every loop (combat all bounded `for`s,
  `move_object` always clears a dying creature, priority depletes resources). gym then confirmed the real freeze
  was the **#49 Selesnya empty-target spin** (engine spun resolving a required-but-empty `ChooseTargets`), NOT a
  demo loop — and the reported "4-hour hang" was largely a lead-side EDT-vs-UTC timestamp misread of a
  minutes-long freeze. **`#49` (5df860e) already fixed it** (gym verified 60/60 Selesnya episodes, no freeze). So
  the demo loop is almost certainly phantom; the loop-guard is kept as **defense-in-depth + self-diagnosis** (an
  engine should never be able to spin forever) per the lead. Tests: loop-guard abort-to-draw + `GreedyAgent`
  never-pass termination (with/without auto-pass). 199 core tests green. (#55 closed.)

- **design (quality pass — behaviour-test coverage, broadly complete):** **16/18 cards now have a card-module
  resolve-level behaviour test** (exercise the *resolved* effect, expect-snapshot or assert on
  zones/counters/P-T/mana — not just the IR). Added this round: Dyadrine (full attack — YesAgent drives the
  Optional/ForEach; expect-snapshot: counters removed → 2/2, drew, +2/2 Robot token), Fabled + Escape (fetch
  a basic → battlefield tapped, via a `TakeItAgent`), Badgermole + Llanowar + Hushwood (resolve the mana
  ability → +{G} in pool), Earthbender (ETB: earthbend a land → 2/2 land-creature + fetch a basic), Bushwhack
  (fight mode via `ResolutionCtx.chosen_modes` → mutual 2 damage) — joining the earlier Lumbering CDA+Crew,
  Mightform double-power, Ba Sing Se earthbend, Sazh's landfall-counter, Mossborn double-counter, Icetill
  mill, Keen-Eyed exile, Erode destroy. Reusable harness pattern: in-crate test mod + `Engine::new` +
  `resolve_effect` + a small scripted agent (Confirm→yes, SelectCards→first n). **The remaining 2 cards —
  Temple Garden (ETB-tapped-unless-pay *replacement*) and Surrak (becomes-targeted *event trigger*) — have no
  standalone resolvable effect**; their behaviour is inherently game-loop-level (ETB replacement application /
  a targeting event) and is covered by engine's cap tests (C11, C16) + their IR snapshots. So every card's
  resolved behaviour is tested. **197 tests green; clippy clean.** Commits 384c327…09d2398.
- **engine (#49 — modal target-legality fix):** **Don't offer a modal mode whose targets can't be chosen
  (5df860e, CR 601.2c/700.2d).** Self-served the repro the lead asked for: instrumented the central `ask`
  to flag any `ChooseTargets` with an effective-required (min≥1) slot + zero candidates, ran 200 Selesnya
  self-play games → **42/200 tripped on Bushwhack** (a "choose one" modal: mode 0 = fight two creatures,
  mode 1 = untargeted search). Root cause: `choose_modes` offered the fight mode even with no creatures in
  play, then emitted a `ChooseTargets` with two empty min=1 slots — a genuine engine leak, **not** a codec
  min=0 misread. Fix: `choose_modes` now offers only *legal* modes (a mode is legal iff every target it
  declares is satisfiable; untargeted modes always legal), mapping the agent's pick back to original mode
  indices; `spell_castable_targets` makes cast-legality modal-aware (≥min legal modes). **0 leaks across 300
  seeds post-fix.** Regression test added; instrumentation removed. 191 core tests green.

- **design (quality pass):** **Behaviour-test coverage expanded across the pool + post-flip self-play
  re-validation.** Added resolve-level behaviour tests (exercise the *resolved* effect, not just IR) for the
  distinct mechanic types: Sazh's Chocobo (landfall → +1/+1 on self), Mossborn Hydra (landfall → double
  +1/+1 counters, a `CountersOnSelf` value), Icetill Explorer (landfall → mill top of library), Keen-Eyed
  Curator (`{1}: Exile` a graveyard card → owner's exile), Erode (destroy target → graveyard) — joining the
  earlier Lumbering CDA+Crew, Mightform double-power, Ba Sing Se earthbend. **10 behaviour tests across 8
  cards** now, covering CDA / crew / pump / earthbend / landfall-counter / double-counter / mill / exile /
  destroy. (Remaining cards' resolved effects — Search-tutoring, mana abilities, Optional/ForEach, reflexive
  triggers, land-permission legality — are covered by engine's cap tests + IR snapshots; card-level tests for
  them would need fragile agent-scripting.) Commits 45de0b2/27a5c1d/+. **188 tests green.** Re-validated the
  full 18/18 deck in self-play (seeds 5/13/88, post Dyadrine/Surrak flips): clean finishes (21–26 turns),
  zero panics — no regressions.
- **gym obs — card-identity one-hot (#50).** Audit first: `grp_id` identity WAS present + effective
  (policy embeds `grp_id % 4096`), just in the `*_ids` field (not the feat arrays the user inspected)
  and hashed. Added an explicit **deck-determined one-hot** per card row: categories = union of both
  decks' unique `grp_id`s (`PyGame.card_vocab()`, new Rust accessor) + 8 token-reserve + 1 unknown;
  built env-side (encoder stays card-agnostic), fed alongside the kept embedding. demo dim=14,
  selesnya=29. +unit tests.
- **gym benchmark — %-trained ladder (#51).** `LadderEval`: current policy vs its own frozen
  snapshots at 10/25/50/75% of budget → `ladder/winrate_vs_NNpct` (non-saturating, self-relative;
  NaN until each fraction reached). Verified on a 12k run.
- **codec infinite-loop fix.** Selesnya self-play hung — a `ChooseTargets`/`SelectCards` slot with
  `min≥1` but no satisfiable candidate spun forever (COMMIT only advanced when `len≥min`). Fix:
  COMMIT finalizes best-effort. +2 regression tests. (Root trigger flagged: task #49.)
- **M4 (#46) — Selesnya landfall pool.** Wired `Deck::Selesnya`; capped env `max_decisions=3000`
  (was 200k) so landfall/lifegain stalls truncate to a draw. **Learns well**: vs_random→0.97,
  vs_initial→0.97 (better than demo's ~0.70). Replay-serde blocker fixed engine-side (#48).
- **reward shaping defaulted** (A/B +0.12/stabler, #45) — `--shaping-coef` default 0.5, annealed.
- **overnight runs launched:** `demo-cardid-2M` + `selesnya-cardid-2M` (sequential, new obs +
  shaping + ladder + self-play eval).
- **webui:** End-to-end QA of the god-view replay viewer (Playwright, live :8080). **Demo replay:**
  lobby lists the finished game with a ▶ Replay button → `/play?replay=<id>`; god-view shows both
  hands face-up + library order (click "Lib" pile → "P# library (top first)" modal, order matches
  raw data exactly); step fwd/back, rate slider actually changes playback speed (frame 2→26 in
  1.2s @ 20/s), scrub-to-end renders "Game over — P0 wins" with fwd disabled; zero console errors.
  **Selesnya (counter-card case, post-#48):** aitrain selesnya replay renders end-to-end incl.
  `+1/+1×2` counters on board; a fresh selesnya lobby game serializes 363 frames with
  `PlusOnePlusOne` counters as string keys (#48 fix confirmed). Replay experience verified for both
  the demo and landfall/"watch it learn" decks. No code changes — verification only.

- **engine (#48 — replay serialization fix):** **String-keyed `CounterBag` (70ded52).**
  `serde_json::to_string(&Replay)` panicked `"key must be a string"` on any Selesnya game with a quest
  counter: `CounterBag` is a `BTreeMap<CounterKind,_>` and serde rejects the non-string keys that
  data-carrying kinds (`Named("quest")` from Earthbender/Dyadrine, `Keyword`) produce — this crashed
  GodView/Replay export for BOTH the webui lobby and gym training-replay export. Fix: a `#[serde(with)]`
  adapter keys the map by `CounterKind`'s canonical `Display`, with a total `FromStr` inverse. **Built-ins
  keep their exact variant-name key** (`"PlusOnePlusOne"`/`"MinusOneMinusOne"`/…) — unchanged wire format,
  so the web client's `CTR_LABEL` lookup is untouched; `Named` is the bare name (`"quest"` → renders
  `quest×N`), `Keyword` is `kw:`-tagged. Round-trip expect-test over a `Replay` with a +1/+1 and a quest
  counter. 183 core tests green; workspace builds. Pinged gym (export unblocked) + webui (no client change).

- **design (#44 — 🎉 DECK COMPLETE):** **Dyadrine flipped + Surrak stack-half → 18/18-minus-Surrak's-countered.**
  (1) **Dyadrine → `fully_implemented: true`** (575bb2a) against cap 0e01d56: `Triggered{ YouAttack, Optional{
  Sequence[ ForEach{ select 2 creatures you control w/ a +1/+1 counter, body: PutCounters{Each, -1} }, Draw,
  CreateToken{2/2 Robot} ] } }` — non-targeting choice (Optional+Select(min:2) gates the reward, no Conditional
  needed). (2) **Surrak's creature-spell-on-stack trigger half** now works via engine d3ee9e9 (`BecomesTargeted`
  fires for stack objects) — **no card change**, the existing filter matches creature spells on the stack;
  docs updated (e9c7390). **Result: 17/18 cards `fully_implemented:true`; the deck is 18/18 fully-faithful
  except Surrak's lone inert can't-be-countered** (deferred per lead — no counterspell in the pool). 182 tests
  green. Ledger + PROJECT_STATE updated to the final state.
  option now reads `Warp Mightform Harmonizer {2}{G}` (cheaper warp cost) instead of `[Warp]`, shown
  in the cast-variant menu; consistent with the rest of the UI (option labels are plain text, no
  mana-symbol SVGs). Fixed my now-stale `fully_implemented_flag_reaches_the_wire_for_a_real_partial_card`
  driver test: Lumbering Worldwagon flipped to fully-implemented (Crew cap landed), so swapped the
  partial-card example to **Surrak, Elusive Hunter** (the lone remaining partial).

- **design (overnight quality):** **Behaviour tests + full-deck self-play re-validation.** Added card-level
  *behaviour* tests (resolve/computed, not just IR snapshots) for the trickiest mechanics: Lumbering `*`/4
  CDA (layer-7a `computed` = lands you control, opponent's lands excluded) + Crew (resolve `BecomeCreature`
  → artifact creature); Mightform double-power (resolve the landfall pump → a 2/2 becomes 4/2, the
  `PowerOfTarget` snapshot); Ba Sing Se earthbend (resolve → a Forest becomes a 2/2 haste land-creature).
  Pattern: in-crate test modules using `Engine::new` (pub) + `resolve_effect` (pub(crate)) + `computed`,
  mirroring engine's cap tests. Commit 45de0b2; 182 tests green. **Re-validated the full Selesnya deck in
  self-play** — `mtg-cli selesnya selesnya` across 7 seeds (1/2/3/7/11/42/99): all clean finishes (14–21
  turns, real winners, **zero panics**) with crew/warp/earthbend/reflexive-chains/land-permissions all live.
  (Warp/Dyadrine-attack/reflexive-chain *resolution* are covered by engine's cap tests + my IR snapshots.)
- **engine (#44 COMPLETE):** **Last two caps — C18 Icetill + Dyadrine c3.** **C18** (`3ca7fef`):
  `StaticContribution::ExtraLandPlays(u32)` + `PlayLandsFrom(Zone)` — player-level land-play
  permissions read by the land-play legality (limit = 1 + ΣExtraLandPlays; offers lands from
  graveyard/exile when permitted). `Player::zone_ids`. **Dyadrine c3** (`0e01d56`): interpret
  `Effect::Optional` (Confirm → run body on yes) + `Effect::ForEach` (`select_for_each`: chooser's
  matching objects, [min,max], body per item via new `EffectTarget::Each`) — reusing the
  single-target PutCounters arm (negative n) for the removal; `count_filter_matches` gained
  HasCounter + ControlledBy. So "you may remove a +1/+1 from each of two creatures you control; if
  you do, draw + Robot token" works (non-targeting ⇒ Optional+Select(min:2) gates the reward). 177
  tests green. **That's the entire #44 upgrade tail done — every in-deck card fully implementable;
  17/18 fully faithful with Surrak the lone deferred partial** (its 2 gaps inert in-deck, deferred
  per lead). 8 caps this session: Conditional/GrantKeyword, reflexive sub-trigger, Badgermole mana,
  Fabled, Escape, Crew, C18, Dyadrine c3.
- **design (#44):** **Icetill Explorer → `fully_implemented: true`** (7350a74) — cap C18 landed (engine
  3ca7fef: `StaticContribution::ExtraLandPlays(n)` + `PlayLandsFrom(Zone)`), so authored its two land-play
  statics (`ExtraLandPlays(1)` + `PlayLandsFrom(Graveyard)`, `affects: itself()`; the engine reads them
  from the controller's permanents). **16/18 fully faithful; 2 partials left** — Dyadrine c3 (cap building)
  + Surrak F+G (both inert in this deck, lead's call). 176 tests green. Also blessed Dyadrine c3's IR
  (`Optional{ Conditional{≥2 creatures w/ +1/+1 counter, Sequence[ForEach(Select 2, remove 1), Draw,
  CreateToken(Robot)] } }`); `CardFilter::HasCounter` already exists. Ledger + trackers updated.
- **design (#44):** **Lumbering Worldwagon → `fully_implemented: true`** (86742c3) — cap C13 Crew landed
  (engine 80d9ab3: `CostComponent::Crew(n)` + `Effect::BecomeCreature`), so authored `Activated{ Crew(4),
  BecomeCreature{ SourceSelf, UntilEndOfTurn } }`. Once crewed it becomes a creature, so its `*`/4 CDA + the
  attacks-trigger fetch finally come live. **15/18 fully faithful; 3 partials left** (Dyadrine/B, Surrak/F+G,
  Icetill/C18). 175 tests green. Ledger + trackers updated.
- **design (#44):** **Escape Tunnel → `fully_implemented: true`** (5a55600) — see prior entry; also flipped
  this round: Badgermole (c2ef012), Fabled (968036e), Escape (5a55600), Lumbering (86742c3); Earthbender
  (e6b9050) earlier. Engine ground the whole lead queue (reflexive/Badgermole/Fabled/Escape/Crew) back-to-back.
- **engine (#44):** **Whole upgrade-tail queue ground through — 5 caps back-to-back, each green.**
  (1) **Reflexive "when you do" sub-trigger** (`2e13694`, CR 603.7c): a targeted `Conditional.then`
  is deferred to a `StackObjectKind::ReflexiveAbility`; its intervening-if re-checks after the parent
  commits + its target is chosen only if the if holds (design's flagged fidelity bug avoided — no
  prompt on sub-threshold, counter always lands). `ResolutionCtx.ability_index` + reflexive_branch/
  reward/node navigate the ability tree serde-safely. (2) **Badgermole reflexive-mana** (`23242f2`):
  `EventPattern::TapCreatureForMana`; auto_pay fires no-stack `AddMana` triggers per creature tapped
  for mana. (3) **Fabled Passage** (`bcff1cd`): `EffectTarget::Searched(n)` (search-result handle via
  transient `Engine.searched_this_resolution`) + `Effect::Tap` → "untap that land if 4+ lands."
  (4) **Escape Tunnel** (`7dd18a9`): `Qualification::CantBeBlocked` (combat reads it) +
  `CardFilter::PowerAtMost` + `Effect::GrantQualification`. (5) **Crew** (`80d9ab3`):
  `CostComponent::Crew(N)` (tap creatures totaling power ≥N) + `Effect::BecomeCreature` →
  GrantContinuous(AddType(Creature), EOT). Also reusable: `Effect::Conditional` (source-aware) +
  `Effect::GrantKeyword` (`d8484d2`). 175 tests green. Surrak skipped (both gaps inert in-deck —
  flagged to lead). Only Dyadrine c3 (Optional + distinct-2-target removal) remains.
- **design (#44):** **Escape Tunnel → `fully_implemented: true`** (5a55600) — cap E landed (engine
  7dd18a9: `Qualification::CantBeBlocked` + `CardFilter::PowerAtMost(n)` + `Effect::GrantQualification`),
  so authored its 2nd ability: `Activated{ {T}+Sac, GrantQualification{ Target(Creature(PowerAtMost(2))),
  CantBeBlocked, UntilEndOfTurn } }`. **14/18 fully faithful; 4 partials left** (Dyadrine/B, Surrak/F+G,
  Icetill/C18, Lumbering/C13). 174 tests green. Ledger + trackers updated.

## 2026-06-13

- **design (#44):** **Fabled Passage → `fully_implemented: true`** (968036e) — cap D landed (engine
  bcff1cd: `EffectTarget::Searched(n)` + `Effect::Tap`), so authored its tail: `Sequence[ fetch_basic_tapped,
  Conditional{ CountAtLeast(lands you control ≥ 4), then: Tap{ Searched(0), tap: false } } ]` ("if you
  control 4+ lands, untap that land" — the new land counts toward the 4). **13/18 fully faithful; 5 partials
  left.** 173 tests green. Ledger + trackers updated.
- **gym (#45 reward shaping):** potential-based shaping (GYM_PLAN §5) in `BatchedSelfPlayVecEnv` —
  `F=γΦ(s')−Φ(s)`, Φ a tanh-bounded mix of Δlife/board-power/card-count from the obs (no engine
  change), annealed to 0 over 60% of training via `ShapingAnneal`; terminal ±1 stays dominant.
  Wired behind `--shaping-coef` (default 0.5). **A/B (3 seeds × 40k, `ab_shaping.py`): shaped 0.712
  vs unshaped 0.591 vs-random (+0.12)** — mainly *stability* (unshaped collapsed on 1 seed → 0.293;
  all 3 shaped stayed 0.69–0.72). 18 gym tests green. Also: 400k keep-busy run
  `demo-selfplay-400k-0613-2337` (570s, 17 lobby replays; vs_initial 0.28→~0.85).

- **webui:** Fixed alt-cost casting (Mightform's Warp) being unreachable in the web UI. The engine
  enumerates Normal + Warp as two `Cast` actions on the **same** spell object; the client's
  click-to-cast did `option_objs.indexOf(id)` → only the first (Normal) was reachable and the button
  list skips on-board options. Now a card with >1 legal cast option pops a **variant chooser menu**
  on click (`legalIdxs`/`onCardClick`/`showVariantMenu` + `.varmenu` CSS, mirrored in
  `embedded_client.html`/`main.ts`/`style.css`); single-option cards cast directly as before.
  Generalizes to any future multi-mode/alt-cost cast. New options.rs test
  `priority_keeps_both_cast_variants_of_one_card_distinct`. (Engine confirmed correct — both
  variants are emitted at `priority.rs:959–966`.)

- **design (#44):** **Badgermole Cub → `fully_implemented: true`** (c2ef012) — cap H landed (engine
  23242f2: `EventPattern::TapCreatureForMana`), so authored its 2nd ability: `Triggered{ TapCreatureForMana,
  AddMana{Controller, {G}} }` (no-stack mana trigger, CR 605.1b). **12/18 fully faithful; 6 partials left.**
  173 tests green. Ledger + trackers updated.
- **design (#44):** **Earthbender Ascension → `fully_implemented: true`** (e6b9050) — cap A reflexive
  sub-trigger landed (engine 2e13694), so authored the full landfall quest-chain: `Triggered{ landfall,
  Sequence[ PutCounters{self, Named("quest"), 1}, Conditional{ ValueAtLeast(CountersOnSelf(quest), 4),
  then: [ +1/+1 on target creature you control, GrantKeyword(Trample, UntilEndOfTurn) ] } ] }`. The
  targeted reward is a reflexive sub-trigger (CR 603.7c) — target chosen only at ≥4, counter always lands.
  **11/18 fully faithful; 7 partials left.** 171 tests green. Ledger updated.
- **design (#44):** **FULL remaining-partial-cap queue staged** (so engine grinds the tail back-to-back; I
  flip each ⚠→✓ as its cap commits). **8 cards still `fully_implemented: false`; 10/18 already fully faithful.**
  (Correction: the capability-ledger reconciliation caught **Icetill Explorer** as an 8th partial — it uses the
  `.incomplete()` builder, not the explicit flag, so an earlier grep missed it; its C18 land-play permissions
  are deferred.) Verified each clause against the engine's current IR/resolution. Caps, each with the card(s)
  it unblocks + the exact card IR (+ **C18** static land-play permissions → Icetill Explorer):
  - **(A) Reflexive sub-trigger** (engine building) — a *targeted* `Conditional.then` / `Optional.body` whose
    target is chosen at 603.3d on a reflexive sub-trigger, only if/when `cond` is met (CR 603.7c). → completes
    **Earthbender Ascension**: `Triggered{ PermanentEnters(land you control), Sequence[ PutCounters{SourceSelf,
    Named("quest"), 1}, Conditional{ ValueAtLeast(CountersOnSelf(Named("quest")), 4), then: Sequence[
    PutCounters{Target(creature you control), PlusOnePlusOne, 1}, GrantKeyword{ChosenIndex(0), Trample,
    UntilEndOfTurn} ] } ] }` (Conditional + GrantKeyword + quest counter already landed d8484d2). Also the
    front of Dyadrine c3's "if you do".
  - **(B) Distinct two-target counter removal** (a `ForEach`/multi-target remove over 2 *distinct*
    creatures-you-control-with-a-counter; `PutCounters` is single-target today) — with (A) → completes
    **Dyadrine c3**: `Triggered{ YouAttack, Optional{ remove +1/+1 from each of two creatures you control,
    reflexive→ Sequence[ Draw{Controller,1}, CreateToken{2/2 colorless Robot artifact creature ×1} ] } }`.
  - **(C) Crew** (CR 702.122) — new `Ability::Crew { n: u32 }`: tap untapped creatures with total power ≥ N →
    `GrantContinuous{ AddType(Creature), UntilEndOfTurn }` on the Vehicle (reuses C12 registry + the AddType
    from earthbend). → completes **Lumbering Worldwagon** (`Ability::Crew { n: 4 }`; its */4 CDA + fetch
    triggers already work).
  - **(D) Searched-permanent reference + `Effect::Untap` + Conditional(lands≥4)** — "Then if you control four
    or more lands, untap that land." Needs a handle to the just-searched permanent (Search binds its result)
    + an untap effect (`Effect::Untap{what}` or `Tap{tap:false}`); Conditional + `CountAtLeast(lands≥4)` exist.
    → completes **Fabled Passage**: append `Conditional{ CountAtLeast(Battlefield, Land, Controller, 4),
    then: Untap{ <searched-ref> } }` after the fetch.
  - **(E) `Qualification::CantBeBlocked` (evasion) + power≤2 target filter + grant-qualification-for-a-duration**
    — "{T}, Sac: Target creature with power 2 or less can't be blocked this turn." Needs a new `CantBeBlocked`
    qualification read by block-legality, a `CardFilter`/TargetKind power≤2 filter, and a way to paint a
    Qualification for `Duration::ThisTurn` (extend GrantKeyword to qualifications, or `Effect::GrantQualification`).
    → completes **Escape Tunnel**'s 2nd ability (the fetch ability already works).
  - **(F) `Target::Stack` targeting in `BecomesTargeted`** — so the trigger also fires when an opponent targets a
    creature *spell you control* on the stack. → completes **Surrak**'s trigger half (battlefield half done, C16).
  - **(G) Stack-zone static gathering + a counter subsystem** — for Surrak's inert "can't be countered"
    `Qualification(CantBeCountered)` static. LOW priority (no counterspell in the pool; nothing to counter).
    Surrak needs both (F)+(G) to reach fully-faithful.
  - **(H) "Tapped a creature for mana" event + reflexive mana trigger** (CR 605, no-stack) — new EventPattern +
    mana-adding trigger. → completes **Badgermole Cub**: `Triggered{ <TappedCreatureForMana>, AddMana{Controller,
    {G}} }` (ETB earthbend already works).
  **Fidelity note (carry forward):** for any targeted reward gated by an intervening-if/"when you do", the
  target MUST be deferred to the reflexive sub-trigger (cap A) — never collect it at the parent trigger, or
  you get prompt-on-fail + (worse) the parent action blocked when no legal target exists. Don't partial-author
  a card whose targeted reward would silently no-op; keep the whole clause deferred until A lands.
- **engine (#44 upgrade tail):** **Reusable caps toward Earthbender (`d8484d2`):** interpret
  `Effect::Conditional` (source-aware intervening-if via `holds_for_source`; `conditions::eval_value`
  gained `CountersOnSelf`) + new `Effect::GrantKeyword{what,keyword,duration}` →
  `GrantContinuous{GrantKeyword,duration}` (grant-trample-until-EOT, wears off at cleanup). Uses
  `CounterKind::Named("quest")` (no new variant). Test: trample granted only at ≥4 counters, off at
  cleanup. **Design caught a real fidelity bug** in the naive Sequence form — a targeted
  `Conditional.then` is a reflexive trigger (CR 603.7c) whose target must defer until the if is met
  (else it prompts on every sub-4 landfall / blocks the counter when you control no creatures). So
  `collect_specs_into` deliberately doesn't pull targets from `Conditional.then`; the **reflexive
  sub-trigger** (deferred-target) is the remaining cap to complete Earthbender — building it next
  with care. 170 tests green.
- **gym (#41 batched inference):** built `BatchedPolicy` (batched `act` + `evaluate`→priors+values —
  the reusable primitive tree search will reuse) and `BatchedSelfPlayVecEnv` (single-threaded lockstep
  pump; opponent decisions batched across games; drop-in for MaskablePPO; `MtgEnv(opponent="external")`
  + `ext_*` API). **Honest finding:** the isolated-forward microbench (batch-1 0.57ms ≈ batch-64
  0.66ms) overpromised — measured end-to-end speedup is only **1.2–1.4× at n=32–64** because `auto_pass`
  makes opponent decisions sparse + envs desync (eff. batch ~9 at n=64) and the tiny net is cheap on
  CPU. Opponent NN is ~33% of step cost; the **dominant cost is the per-decision boundary** (engine +
  PyO3 + obs encoding, ~2.7k env-steps/s no-NN ceiling). The batched infra's real payoff is bigger
  nets + tree search. 13 gym tests green. Also: single uv venv (`python/.venv` + uv.lock, cu130 torch);
  `export_replays` now logs `selfplay/winrate_vs_initial`.
- **design (upgrade tail #44):** **Mightform → `fully_implemented: true`** (7794d2a) — C14 warp piece 3
  (recast-from-exile, 7cc6f9c) completes the card; no IR change, just the flag + docs. Mightform joins
  Keen-Eyed as a fully-faithful "hard" card. **First ⚠ partial flipped to complete in the upgrade tail.**
  169 tests green.
- **engine:** **C14 warp COMPLETE (`7cc6f9c`) — piece 3 recast-from-exile.** `Action::WarpExile` (a
  dedicated exile granting recast permission, distinct from plain `Exile`) sets
  `Object.castable_from_exile`; `legal_priority_actions` scans exile and offers a sorcery-speed
  normal-cost recast; `move_to_stack` removes from hand OR exile. Full warp now plays end to end
  (cast cheap → exile at next end step → recast from exile later as a plain cast, no re-warp).
  Mightform → `fully_implemented: true` (no card change). Last substantial mechanic done; remaining
  is the small upgrade tail (reflexive triggers, crew, can't-be-countered, etc.). 169 tests green.
- **design (upgrade tail #44):** **Mightform's Warp {2}{G} authored** (3a3dbed) against C14 pieces 1+2 —
  added `Ability::Warp { cost: mana_cost(2, &[(Green,1)]) }`: sorcery-speed cast from hand for {2}{G}
  (offered even when normal {2}{G}{G} is unaffordable), then exile at the next end step. Atomic/exploit-free
  (cheap cast always carries the exile). Stays `fully_implemented: false` only for recast-from-exile (piece 3,
  pending) — flips to true with no other change when it lands. 169 tests green.
- **engine:** **C14 warp pieces 1+2 (`c445d78`)** — alt-cost casting + exile-at-next-end-step.
  `Ability::Warp{cost}` + `CastVariant::Warp`: legal_priority_actions offers the warp cast (sorcery
  speed, even when the normal cost is unaffordable — the discount is the point); cast_spell pays the
  warp cost + flags `Object.warp_cast`. On resolution the warp permanent arms a new
  `DelayedTriggerEvent::AtBeginningOfNextEndStep` (reuses the C12 delayed-trigger registry) that
  exiles it, fired at `PhaseBegan{End}`. Pieces 1+2 atomic so the cheap cast always carries its exile
  downside (no free-discount exploit). Test: warp {1} on a normal-{3} creature → warp offered, enters,
  exiled at end step. Mightform is now warp-castable as a faithful partial; piece 3 (recast from
  exile) is the remaining upside. 169 tests green.
- **webui:** Battlefield layout — permanents are now bucketed creatures-first: anything that **is a
  creature** (incl. artifact/enchantment creatures and creature-lands) goes in the creature band;
  remaining noncreature artifacts/enchantments/walkers render in a divided group **off to the right**
  of the creatures; lands keep their own back row. New `isCreature` helper + `creatureBand()` +
  `.permgroup` CSS, mirrored across `embedded_client.html`/`main.ts`/`style.css`. Also regenerated
  `card-art.json` (+Keen-Eyed Curator, now deck-listed → 35 entries; startup warning clean).
- **design:** **🎉 SELESNYA DECK COMPLETE — 18/18 distinct cards = the real mtggoldfish 60.** Authored
  the last card, **Keen-Eyed Curator** (`cards/blb/keen_eyed_curator.rs`, id 117, commit df2abae), as
  **`fully_implemented: true`** against C17 (e002d7a + b18c6f6): `{1}: Exile target card from a graveyard`
  = `Activated{ {1}, Exile{ Target(CardInZone{ Graveyard, Any }) } }`; "+4/+4 & trample while ≥4 card
  types among cards exiled with it" = two `ConditionalStatic` on `ItSelf`, `WhileSourcePresent`,
  `ValueAtLeast(DistinctCardTypesAmongExiledWith, Fixed(4))`. Created the `blb/` set folder + wired it.
  Folded ×1 into the preset → **51 nonbasics + the real 7F/2P, all padding gone.** 168 tests green,
  workspace + clippy clean. The whole Selesnya Landfall card-pool push (C1–C19) is now playable end to
  end. Remaining = **upgrade-only** on already-decked cards (none add deck cards): C14 warp (Mightform),
  Dyadrine's attack ability (2 caps), Surrak's stack-spell half + can't-be-countered, Lumbering's crew,
  Earthbender's quest-chain, Badgermole's reflexive-mana — all faithful tracked-partials, all staged.
- **engine:** **C17 COMPLETE (`b18c6f6`) — Keen-Eyed Curator fully buildable → deck 18/18.** Pieces
  2+3 on top of piece 1's exile mechanics: (2) `Object.exiled_with` records which permanent exiled a
  card (set by `Action::Exile`, reset on zone change) = the "exiled with this creature" set; (3)
  `Ability::ConditionalStatic{contribution, affects, duration, condition}` (additive — gather_statics
  second arm, existing Static cards untouched) contributes to the layer system only while its
  condition holds, evaluated **source-aware** (new `conditions::holds_for_source` threads the source
  ObjId) + `ValueExpr::DistinctCardTypesAmongExiledWith` (counts distinct card types in the source's
  exiled set) + reused `Condition::ValueAtLeast`. Keen-Eyed's +4/+4-and-trample toggles on at the 4th
  distinct exiled type (test). All remaining deck-completing caps now done; only upgrade-only caps
  left (C14 warp, Surrak stack-half, Dyadrine c3, can't-be-countered). 167 tests green.
- **engine:** **Targeting hardening (`861f3aa`)** — `target_matches_filter` now fails CLOSED: an
  unhandled `CardFilter` predicate rejects rather than passing through `_ => true` (the gap class
  that let a creature match "land you control"). All pool predicates handled; 165 green.
- **engine:** **C17 piece 1 (`e002d7a`)** — interpret `Effect::Exile` (was a no-op) → `Action::Exile`
  moves the target to its owner's exile + broadcasts ObjectMoved (delayed exile triggers fire); and
  `target_candidates` now enumerates `TargetKind::CardInZone{zone, filter}` (was unhandled). So
  Keen-Eyed Curator's `{1}: Exile target card from a graveyard` mechanics work; reusable for any
  exile/graveyard-target effect. Test: a graveyard card is a legal candidate and gets exiled.
  Remaining C17 (exile-association + source-aware conditional static + distinct-card-types value)
  proposed to design — pieces 2+3 build once shapes confirmed.
- **webui:** Lobby Games + AI Training Replays lists now show **most-recent first** (games desc by
  id; runs desc by latest checkpoint time, steps still ascending within a run) and both panels have
  a **capped height (340px) with a scrollbar** so long lists don't push the page down.
- **engine:** **Targeting correctness fix (`70c483e`)** — user hit it live (earthbend couldn't target a
  land but could target/clobber a creature; Bushwhack could fight your own creature). Two bugs in
  priority.rs: (1) `target_candidates` drew `TargetKind::Permanent` from creatures-only → lands never
  offered; now enumerates all battlefield permanents. (2) `target_matches_filter` let
  HasCardType/HasSubtype/HasColor/Colorless/Supertype fall through `_ => true`; now enforced over
  computed chars (an animated land still counts as land+creature). Net: earthbend offers only
  lands-you-control; Bushwhack fights only creatures you don't control. Test covers both.
- **webui:** **Card-art coverage check + auto-derived art fetch.** New `dump-cards` bin prints the
  card pool (every `(grp_id, name)` across all selectable decks) as JSON — the single source of
  truth, so `resolve-card-art.py` now fetches art for exactly the engine's deck cards (no more
  hand-maintained grp_id→name list). `driver::deck_card_pool()` backs both that and a startup
  warning (`server::missing_card_art`/`warn_missing_card_art`): `mtg-serve` lists any deck card
  without baked-in art + the fix command. Regenerated `card-art.json` — picked up the 10 Selesnya
  set cards (Erode/Bushwhack/Ba Sing Se/Surrak/Badgermole Cub/Earthbender Ascension/Mightform
  Harmonizer/Fabled Passage/Escape Tunnel/Temple Garden) that had no art. New offline test
  `every_deck_card_has_baked_in_art` guards against future gaps (forces art-first).
- **engine:** **Dyadrine body cap — enters-with-counters = mana spent (`a2e2b13`).** New
  `Rewrite::EntersWithCountersValue{kind, n: ValueExpr}` (dynamic ETB-counter count, evaluated
  against the entering object) + `ValueExpr::ManaSpent` (total mana paid incl. {X}, recorded on
  `Object.mana_spent` by cast_spell, reset on any zone change) — **additive**, leaves the fixed
  `EntersWithCounters{u32}` (Mossborn) untouched. So Dyadrine's body = `Replacement{
  WouldEnterBattlefield(ItSelf), EntersWithCountersValue{ PlusOnePlusOne, ManaSpent } }`; a 0/0 cast
  for {2}{G} enters as a 3/3 (test). With trample + the YouAttack firing (4613d51), Dyadrine ships as
  a faithful partial → +2 to the deck (its "remove 2 counters → draw + Robot token" attack ability
  stays deferred). 163 tests green. Next: C17 (Keen-Eyed Curator) then C14 warp.
- **design:** **Remaining-cap queue pinned for engine (Dyadrine c2/c3, C17, C14).** With 17/18 authored,
  engine is blocked on IR; staged the full pull-ready queue. Verified against the engine's actual
  resolution code (whiteboard.rs): `Effect::PutCounters` resolves a **single** `Target::Object` (no
  multi-target), and `Optional`/`Conditional`/`ForEach` hit the `_ => {}` **no-op** — so several clauses
  need real caps, not approximation.
  - **Dyadrine c2 — DONE.** Engine landed it additively (a2e2b13): new `Rewrite::EntersWithCountersValue{
    kind, n: ValueExpr }` (kept the fixed `EntersWithCounters{u32}` for Mossborn → zero churn) +
    `ValueExpr::ManaSpent` (mana paid at cast incl. X, on `Object.mana_spent`, reset on zone change).
    **Dyadrine authored** as a faithful partial (2e02ceb): `{X}{G}{W}` Legendary Artifact Creature —
    Construct 0/1, trample + `Replacement{ WouldEnterBattlefield(ItSelf), EntersWithCountersValue{
    PlusOnePlusOne, ManaSpent } }`; c3 (attack ability) deferred. Folded ×2 into the preset → 50 nonbasics,
    basics 8F/2P. **Deck now 17/18 distinct cards; only Keen-Eyed Curator (C17) left out.** 165 tests green.
  - **Dyadrine c3 (NOT authorable today — needs 2 caps, do NOT husk)** — "Whenever you attack, you may
    remove a +1/+1 counter from **each of two** creatures you control. **If you do**, draw + make a 2/2
    Robot." `YouAttack` ✓ (4613d51), `Draw`/`CreateToken`(C6)/Robot subtype ✓, `PutCounters` negative-n ✓
    BUT single-target only. Missing: (i) **distinct two-target counter removal** (a `ForEach`/multi-target
    remove, currently no-op) and (ii) the **reflexive "if you do"** gating (`Conditional` is no-op). Both
    are real caps — authoring now = a wrong approximation (un-enforced distinctness + draw/token firing even
    when you couldn't remove from two). Staged, deferred.
  - **C17 Keen-Eyed Curator** (blb, `{G}{G}` 3/3) — "As long as there are four or more card types among
    cards exiled **with** this creature, it gets +4/+4 and has trample." + "`{1}: Exile target card from a
    graveyard.`" Caps: (a) **exile-association** (cards exiled by this ability link to the source),
    (b) `ValueExpr` distinct-card-types-among-those, (c) a **conditional static** (ModifyPT +4/+4 &
    GrantKeyword(Trample) gated on ≥4), (d) **`Effect::Exile` interpreted** (currently `_ => {}` no-op) +
    (e) **CardInZone(Graveyard) targeting**. All-or-nothing (husk otherwise → stays out of the preset).
  - **C14 Warp** (Mightform) — "Warp {2}{G}: cast from hand for the warp cost; exile at the beginning of the
    next end step; then you may cast it from exile on a later turn." Caps: **alt cast cost** + **delayed
    exile-at-next-end-step** (the delayed-trigger registry can host it) + **cast-from-exile permission**.
    Self-contained; completes the already-decked Mightform partial→full.
- **engine:** **Attack triggers wired (`4613d51`)** — they were dead (declare_attackers set combat
  state but never fired an event, so `EventPattern::SelfAttacks` never triggered). Now
  declare_attackers broadcasts `GameEvent::AttackersDeclared{attackers, by}` → SelfAttacks fires per
  attacking creature + new `EventPattern::YouAttack` fires once for the attacking player. Enables any
  "whenever this attacks" creature + Dyadrine's "whenever you attack" half. Test covers both. 162
  tests green. (Dyadrine's other half — "enters with +1/+1 = mana spent" OR remove-counter/reflexive
  — needs design to pin the oracle/IR; flagged.)
- **design:** **Mightform Harmonizer (C15) + Surrak's draw trigger (C16) authored — 17/18 cards.**
  (1) *Mightform Harmonizer* (eoe, faebf93): landfall→double-power snapshot
  `PumpPT{ what: Target(creature you control), power: PowerOfTarget(0), toughness: Fixed(0),
  UntilEndOfTurn }`; `fully_implemented:false` only on warp (C14). Folded ×4 into the selesnya preset
  (standing rule) → 48 nonbasics, basics 9F/3P. (2) *Surrak* becomes-targeted draw trigger (3097fc4):
  `Triggered{ BecomesTargeted{ filter: creature you control, by_opponent: true }, Draw 1 }` — the
  **battlefield-creature half**; the "creature SPELL you control" half is deferred (`Target::Stack`
  targeting unbuilt) → honest under-trigger, Surrak stays `false` (that half + can't-be-countered
  inert). Caught + corrected engine's C16 plan (it had "whenever *this* becomes the target"; real
  oracle is the broader creature-you-control set across BF+stack). 161 mtg-core tests green. Remaining
  out of the preset: Dyadrine + Keen-Eyed Curator (cap-blocked: YouAttack/mana-spent-counters, C17/exile).
- **engine:** **C15 double-power + #43 search-reveal + C16 becomes-targeted** (3 more green commits
  after the earthbend push). (1) `557b6b5` **C15**: materialized `Effect::PumpPT` (was a no-op) as a
  floating `ModifyPT` continuous effect over its target for the given `Duration`, reusing the
  earthbend continuous-effect registry; added `ValueExpr::PowerOfTarget(n)` (snapshot the Nth
  target's computed power at resolution, CR 608.2h) so Mightform's "double its power until EOT" =
  `PumpPT{ power: PowerOfTarget(0), toughness: Fixed(0), UntilEndOfTurn }`; `Duration::UntilEndOfTurn`
  ends at cleanup (CR 514.2, `end_of_turn_continuous_cleanup`). Also gives generic +X/+Y-until-EOT.
  (2) `651867f` **#43**: `ask()` now reveals to the deciding seat the chars of any object its
  `DecisionRequest` references but the view didn't describe — chiefly Search/`SelectCards`
  candidates drawn from the hidden library (fetch lands / Bushwhack / Erode), added to
  `me.revealed_to_me` as `Visible` ObjViews (rest of the library stays masked); also covers
  SelectFromGroups/ArrangeCards/OrderObjects/ChooseTargets/Distribute object refs. Fixes the `#id`
  render. (3) `8d006fd` **C16**: `EventPattern::BecomesTargeted{filter, by_opponent}` +
  `GameEvent::Targeted` — fired per targeted object at the 3 target-lock sites (cast/activate/
  trigger), routed to a `queue_watching_targeted_triggers` watcher-match scoped to opponent sources.
  Surrak's draw trigger is now authorable (permanent half; the creature-spell-on-stack half deferred,
  `Target::Stack`). 161 mtg-core tests green, workspace + clippy clean throughout.
- **engine:** **C12 earthbend fully landed — two new reusable subsystems + the full mechanic**
  (3 green commits). (A) `3d4b636` **floating continuous-effect subsystem** (CR 611): a
  `chars::ContinuousEffect` registry in `GameState` for resolution-granted statics (fixed affected
  set + `StaticContribution`s + `Duration`), folded into the layer system alongside printed statics
  via a `Filter`/`Fixed` scope; `add_continuous_effect`/`expire_continuous_effects`. The reusable
  home for until-EOT pumps + animations. (B) `db81497` **`Effect::Earthbend{target,n}` +
  `Action::GrantContinuous`**: animates the target land to a 0/0 haste land-creature + N +1/+1
  counters. (C) `21171dc` **delayed triggered abilities** (CR 603.7): `GameState.delayed_triggers`
  armed via `Action::RegisterDelayedTrigger`, fired+consumed on the watched object's death/exile,
  put on the stack as `StackObjectKind::DelayedAbility{actions}` (concrete serializable Actions, no
  `Effect` tree) → Earthbend's "when it dies or is exiled, return it tapped". `Action` gained `Eq`
  (additive). Tests: synthetic land-animation + expiry (chars), Earthbend resolve → 2/2 land
  creature (priority), and end-to-end Earthbend 0 → 0/0 dies to SBA → returns tapped as a PLAIN land
  (animation correctly does NOT follow the new object). 157 core tests green. **Unblocks:** design's
  **Ba Sing Se** flips to `fully_implemented: true` with no card change; the return-tapped gap on
  **Badgermole Cub** + **Earthbender Ascension** is closed (they stay incomplete only on their other
  unbuilt mechanics — reflexive mana trigger / quest-counter chain). Reusable for future caps:
  grant-keyword-until-EOT (Ascension's trample) + PumpPT-until-EOT can now use the same registry.
- **design:** **Surrak, Elusive Hunter authored (13th card) + pending-cap IR staged.** (1) Surrak
  (`cards/tdm/surrak_elusive_hunter.rs`, id 112, `{2}{G}` Legendary Human Warrior 4/3, commit
  fc24c83): **trample works today** (printed `Keyword`); `fully_implemented:false` with two tracked
  gaps — **"can't be countered"** modeled CR-correctly as a `Qualification(CantBeCountered)` static on
  `ItSelf`/`Zone::Stack` but **inert** (gather_statics walks only the battlefield + nothing reads the
  marker + no counter subsystem in the pool), and the **becomes-targeted draw trigger** needs cap C16.
  No silent approximation: the unbuildable clauses are absent or inert + flagged. 153 core tests green.
  (2) **Pending-cap card IR staged** (Scryfall-verified oracle text → agreed IR shape, so authoring is
  mechanical when each cap lands):
  - **earthbend (C12) — LANDED + 3 cards authored** (commit d4da45f). Engine shipped
    `Effect::Earthbend{target,n}` + `Action::GrantContinuous` + collect arm (db81497). Corrected: earthbend
    **always targets** "target land you control" (even ETB forms). New shared `helpers::earthbend(n)` →
    `Earthbend{target: Target(Permanent(land you control)), n: Fixed(n)}`. Authored: *Badgermole Cub* (tla,
    `{1}{G}` 2/2, ETB earthbend 1 ✓; "whenever you tap a creature for mana, add {G}" reflexive-mana subsystem
    deferred), *Earthbender Ascension* (tla, `{2}{G}` Ench, ETB `Sequence[earthbend 2, fetch]` ✓; landfall→
    quest-counter→reflexive(≥4)→+1/+1+trample-EOT chain deferred), and flipped *Ba Sing Se*'s `{2}{G},{T}:
    Earthbend 2` activated (Timing::Sorcery) on. All three held `fully_implemented:false` only for the
    earthbend **return-tapped** delayed trigger (engine commit C, imminent) — flip to true with no card change
    when C lands. 156 core tests green. **UPDATE — C12 fully landed** (engine 3d4b636 floating-continuous +
    db81497 Earthbend/GrantContinuous + 21171dc delayed-trigger return-tapped; 157 tests): **Ba Sing Se flipped
    to `fully_implemented: true`** (b524244, no card change — all 3 clauses done). Badgermole/Earthbender stay
    false but the return-tapped gap is trimmed from their notes — each now down to its single real unbuilt
    mechanic (reflexive-mana trigger / quest-counter landfall chain).
  - **selesnya preset folded** (cards/mod.rs): added the 3 now-implemented cards at mtggoldfish quantities —
    Surrak ×2, Badgermole Cub ×4, Earthbender Ascension ×4 (34→44 nonbasics); basics rebalanced 18F/8P → 10F/6P
    to stay at 60 (green-primary, {W} Erode still castable). Still omitted (cap-blocked): Keen-Eyed Curator,
    Dyadrine, Mightform. **STANDING RULE** (lead, no need to re-ask): whenever a landfall card crosses
    unimplemented→at-least-partial, fold it into `selesnya_landfall_deck()` at its mtggoldfish qty and rebalance
    basics; when all 18 are in, the preset = the real 60 and the basic padding is gone.
  - **Mightform Harmonizer (eoe, `{2}{G}{G}` 4/4) — AUTHORED** (faebf93). C15 landed (engine 557b6b5:
    PumpPT materialized + `ValueExpr::PowerOfTarget(n)`). Landfall→double-power as the CR-correct **snapshot**:
    `Triggered{PermanentEnters(land you control), PumpPT{ what: Target(creature you control), power:
    PowerOfTarget(0), toughness: Fixed(0), UntilEndOfTurn }}` (+X/+0 fixed at resolution, X=current power; wears
    off at cleanup CR 514.2). `fully_implemented:false` only on **Warp {2}{G}** (C14: alt-cast + exile-at-end-step
    + recast). Folded ×4 into the selesnya preset (standing rule) → 48 nonbasics, basics 9F/3P; only Keen-Eyed
    Curator + Dyadrine (3 copies) remain as basic padding. **17/18 distinct cards authored; 159 tests green.**
  - **Dyadrine, Synthesis Amalgam** (`{X}{G}{W}` Legendary Artifact Creature — Construct 0/1) — THREE clauses
    (not alternatives): (1) **trample** ✓ today; (2) **"enters with +1/+1 counters = mana spent to cast"** —
    the **includability gate** (without it Dyadrine is a 0/1 husk → can't ship; with 1+2 it's a faithful
    partial like Surrak). IR: `Replacement{ WouldEnterBattlefield(ItSelf), EntersWithCounters{ PlusOnePlusOne,
    n: ValueExpr::ManaSpent } }` — needs a new `ValueExpr::ManaSpent` + **generalize `EntersWithCounters{n}`
    from `u32` → `ValueExpr`** (only Mossborn Hydra's `n:1` literal is affected → I update it to `Fixed(1)` in
    the same window). (3) **"Whenever you attack, you may remove a +1/+1 counter from each of two creatures
    you control. If you do, draw a card and create a 2/2 colorless Robot artifact creature token."** —
    `YouAttack` is now LIVE (engine 4613d51), but the body still needs a **remove-counter effect** (none
    exists — only `PutCounters`) on two targets + reflexive "if you do" + `CreateToken` (C6, exists; Robot
    subtype exists). So clause 3 stays a tracked deferral; Dyadrine ships on 1+2. **Rec to engine: build
    clause-2 mana-spent cap next** (+2 to the deck).
  - **Keen-Eyed Curator** (blb, `{G}{G}` 3/3): **fully blocked** — conditional static keyed on "card types
    among cards exiled with this" (exile-association + count-distinct-types, C17) + `{1}: Exile target card
    from a graveyard` (Effect::Exile uninterpreted + CardInZone targeting skipped).

- **webui:** **Lobby deck viewer + replay-naming/ordering polish + stop-policy tech-debt note.**
  (1) **Deck viewer:** new "Decks" tab in the lobby with a card grid per picker preset (art thumb,
  mana symbols, type line, P/T, ×count, ⚠ partial badge, oracle-text tooltip + full-card hover
  preview); backend `GET /api/decks` + `/api/decks/:name` build from `driver::resolve_deck` +
  `starter_db`. Foundation for a future deck editor. Commits 1ea15d0 (backend), 2b55b50 (UI). (2)
  **AI replay naming (user/gym):** run header now `run · deck · N checkpoints · step-range · date`,
  rows show exact step + result + time; runs ordered chronologically, steps ascending. (3) **Games
  list:** chronological by creation (id asc), stable. Commit 6eb3286. (4) **Stop policy:** flagged
  the client-side `priorityAutoPass` filter (forced `smart_stops`) as TECH-DEBT in driver.rs +
  spec'd the canonical engine-side `should_auto_pass` rewrite (drop smart/resolve/autopass flags) to
  engine as a backlog item (lead-approved); I delete the client filter once engine lands it. NB:
  team-lead fixed the lingering `selesnya`→counters alias in driver.rs (dced893) — selesnya now
  resolves to design's preset; left as theirs.

- **webui:** **Stop-config redesign (user) + selesnya in picker + ability-text/attachment/zone polish.**
  (1) **Stops:** per user, the only toggle is now **Full Control**; auto-pass/smart/resolve toggles
  removed. Off-behaviour is a FIXED rule: STOP iff `[phase is a marked stop OR an opponent spell/
  ability is on top of stack] AND [you have a usable spell/non-mana ability]`. **Implemented as a
  client-side filter** (`priorityAutoPass`) over the engine's surfaced superset — driver forces
  `smart_stops=true` engine-side (surfaces marked-phases + any-action + opp-respond, a superset of
  the rule) and the client narrows it. NB: this layers the final stop policy in the *client*, on top
  of the engine's StopConfig (the engine flag model couldn't express the hybrid rule). Verified: a
  human game auto-passes Untap/Upkeep/Draw and stops at Main 1 (marked + has a play). (2) **selesnya**
  deck added to the lobby picker (dropped my `resolve_deck` "selesnya"→counters alias so it resolves
  to design's official preset; counters kept). (3) Earlier same session: ability stack objects show
  their source card's name+text (dashed marker); auras/equipment offset up-left so names show; replay
  bar hidden in normal play (`[hidden]{display:none!important}`); hidden zones (opp hand / library)
  open as N card backs. Commits c872cdf (stops+selesnya), 17d43fc, 5a31ded, a294feb, 3ddd2ee.

- **gym:** **GYM_PLAN milestone 2 in progress — self-play league (demo deck).** User greenlit M2 +
  switched the agent to the **demo deck** (mirror: forests/mountains/bears/giants/shocks). **M2a**
  (99af24d): policy-opponent seam in `MtgEnv` (callable/object opponent, obs threaded; relative obs
  ⇒ one policy plays both seats). **M2b** (5985d70): self-play league — `OpponentPool` (frozen
  checkpoints sampled per episode, filesystem-coordinated so it works across `SubprocVecEnv`),
  `PoolCheckpoint` callback (atomic save + prune), `selfplay_train.py` (MaskablePPO vs the pool;
  logs win-rate vs random + vs the initial self). **Trains + improves: 0.72 vs random, 0.69 vs its
  random-init self** on demo; slow learning test green. **M2c** (7ef2b29): vectorization explored +
  `throughput.py`. **Finding:** sim is NOT the bottleneck (raw engine 54 games/s/core) — NN
  inference is (per-env opponent `predict`); `SubprocVecEnv` is *counterproductive* (big Dict obs ⇒
  per-step IPC > parallelism on a fast sim); `DummyVecEnv` ~14 games/s/core is fastest. The
  ≥10²/core bar needs **async batched inference** (#41, M3-adjacent) — flagged to lead. **M2d**
  (this commit): `export_replays.py` now records **true self-play** (current policy on both seats),
  run-name-tagged → lobby AI Training Replays. Snapshot/clone still deferred to M3.
- **engine:** **Bushwhack cap: cast-time modal-with-targets** (6a3eb78) — `StackObject.modes`;
  `cast_spell` chooses a modal spell's modes at 601.2b then collects target specs for ONLY the chosen
  modes at 601.2c (added the `Fight` arm to `collect_specs_into` so its two targets get declared);
  chosen modes ride to `ResolutionCtx.chosen_modes` and `choose_modes` reads them instead of re-asking,
  so resolution runs only the chosen mode with cast-locked targets. Behavioral test (cast modal Fight/
  GainLife → Fight runs, both creatures take damage, gain-life doesn't). Existing C7 modal-resolution
  test preserved (resolution-time choice via fallback). Unblocks Bushwhack (design pre-staged it — just
  needs UPDATE_EXPECT). 5th C11–C18 cap today (Sacrifice, ControllerOfTarget, attachments, C11, Bushwhack).

- **engine:** **C11 cap: conditional enters-tapped rewrites** (b98afdc) — two `Rewrite` variants on
  `WouldEnterBattlefield(ItSelf)`: `EntersTappedUnless(Condition)` (check lands — taps iff condition
  fails, no choice) and `EntersTappedUnlessPay{life}` (shock lands — Confirm as it enters; pay→LoseLife
  +untapped, decline→tapped). Made `apply_rewrite` `&mut self` so the shock land can `self.ask`
  mid-ETB-replacement (the architectural bit). Tests cover all 4 branches. Unblocks Temple Garden +
  Ba Sing Se's tapped clause. 118 mtg-core green. (4 caps landed today: Sacrifice, ControllerOfTarget,
  attachments-view, C11.)

- **webui:** **Board visualizations + floating mana (user).** (1) **Stack→target arrows:** spells/
  abilities on the stack draw curved red SVG arrows (full-viewport overlay) from the stack card to
  each target (creature card / player panel), from the already-populated `StackObjView.targets`;
  cards carry `data-oid`/`data-sid`, players `data-pid`. Live in play/replay/spectate. (2)
  **Attachments behind host:** auras/equipment render stacked slightly offset BEHIND their host
  creature (not standalone), reading `ObjView.attachments` — engine populated it (fa808f9); verified
  on real data (a King Cheetah + attached obj 57 renders behind the host). (3) **Floating mana:** each
  player panel shows unspent `mana_pool` mana as Scryfall symbols under the life total. Commits
  a294feb (arrows+attach), 3ddd2ee (floating mana). **Pending engine (#36):** manual mana-ability
  activation — engine skips `is_mana` abilities at priority + auto-taps; spec'd offering
  `ActivateMana` + pay-from-pool-first + keep auto-tap default/no-new-stops (my options.rs already
  renders ActivateMana). **Perf:** also fixed `/api/replays` 3-7s→4ms (read only the `meta` prefix,
  not the full multi-MB files) so the gym's AI-training replays list instantly (commit cc1ae39).

- **engine:** **View: populate `ObjView.attachments`** (fa808f9, webui request) — `visible()` now fills
  each battlefield object's `attachments` with the ids of objects whose `attached_to` points at it
  (battlefield order, stable), instead of a hardcoded empty Vec; flows through `view_for` + `god_view`
  so the client renders auras/equipment behind their host on live games + replays. Test covers it. 116 green.
- **engine:** **C11–C18 cap: `PlayerRef::ControllerOfTarget(n)`** (632982e, atomic per design's bless) —
  resolves to the controller of the Nth object target, **snapshotted into `ResolutionCtx.target_controllers`
  at resolution start** (in `resolve_top`) so it survives that object leaving play mid-resolution.
  `eval_player` reads the snapshot; the static `conditions`/`pt_controller` matchers fall through their
  wildcards. Unblocks **Erode** ("Destroy target creature. Its controller may search…"). Test:
  `Sequence[Destroy, GainLife(ControllerOfTarget(0))]` gives life to the destroyed creature's controller,
  not the caster. 114 mtg-core tests green. (design's fetch lands landed first-try against the prior
  sac-cost cap, d96ec72.) Next: Dyadrine's `EventPattern::YouAttack`.
- **engine:** **C11–C18 cap: `CostComponent::Sacrifice` as an activation cost** (0451182) — pivoted to
  #21 and landed design's #1 gap. `can_pay_cost`/`pay_cost` (which only paid TapSelf/Loyalty) now
  resolve chooser-controlled battlefield permanents matching the spec filter (source-aware: `ItSelf`
  = the cost's source, so `{T}, Sacrifice this:` works) and sacrifice `spec.min` to the graveyard,
  asking `SelectCards` only on a genuine choice. Instant-speed activated abilities on lands already
  worked (`legal_priority_actions` enumerates them for any battlefield permanent, `Timing::Instant`),
  so this **fully unblocks the fetch lands** (Fabled Passage / Escape Tunnel) — design to author. Test
  covers Sacrifice-this. 111 mtg-core tests green. (Tracked-deferred per design: Fabled's "untap that
  land" = searched-permanent handle; Escape's "can't be blocked" = CantBeBlocked qualification.)
- **engine:** **Replay live frame sink** (9ec1fbf) — `Engine::set_replay_sink(Box<dyn FnMut(&ReplayFrame)>)`
  streams each god-view frame to the caller the instant it's captured (in `push_replay_frame`, on the
  game thread), unblocking webui's live god-view spectator (their `observe` only saw a masked
  `PlayerView`, never `GameState`). Installing a sink turns recording on. Non-`Send` `FnMut` (matches
  `Box<dyn Agent>`; engine is built+run on the game thread). Test streams a full game's frames live,
  count/labels matching the kept replay. Frame-size (~18MB/276-frame game) flagged to webui w/ options
  (gzip-on-serve / coarser-granularity knob / delta-frames). 110 mtg-core tests green.
- **gym:** **`mtg-py` replay accessor (REPLAY_PLAN §3 prep)** — exposes the engine's freshly-landed
  replay recording (a533720) through Python so M2 can export training self-play replays. `PyGame`
  gains `record_replay`/`replay_step` ctor args (game thread calls `set_replay_source(AiTraining
  {step})` + `record_replay(true)` before `run_game`, ships the `Replay` in `GameOver`) and
  `replay_json(created_at, names, decks) -> Optional[str]` (serde_json of `engine.replay()`, caller
  stamps the clock/names per the core's no-clock split; `None` unless recorded). Validated the
  locked schema end-to-end through Python: source `AiTraining{step}`, engine-filled `result`,
  god-view `frames` with labels ("game start" → "Turn N — P0 PrecombatMain" → "Mountain →
  Battlefield" …). 10 Rust + 12 pytest tests green. **The sampling/export LOOP stays in M2** (so
  recorded games are real self-play); this is just the isolated, low-risk accessor (lead-approved).
- **webui:** **Replay + omniscient-spectate feature (REPLAY_PLAN) — webui half mostly done.** Against
  engine's locked `crate::replay` schema: (1) **god-view viewer** — a new mode of the game client
  (`/play?replay=<id>`) that fetches `/api/replays/:id` and plays `frames[i].state` through the
  existing board render with NO masking (a `godToView` adapter), opponent hand face-up + both
  players' hand/ordered-library piles openable, playback bar (◀/⏯/▶, frames-per-sec slider, frame
  label; ←/→ + Space keys), no WebSocket. (2) **lobby** — finished games' button → god-view "▶ Replay";
  new "AI Training Replays" section listing `source:AiTraining` replays. (3) **serving** — `GET
  /api/replays` (flattened meta + id + frames, newest-first) + `GET /api/replays/:id` (full;
  sanitized id, traversal→400, missing→404) over the gitignored `data/replays/*.json` store. (4)
  **auto-save** — finished lobby games record an omniscient `Replay` (`driver::finish_game_with_replay`
  → `engine.record_replay`/`replay()`), stamped (names/decks/source=Human/created_at) and written to
  `data/replays/<game-id>.json`, so the finished-game button plays a real game back. Both clients in
  sync; tsc/vite + 13 web tests green. Playwright-verified end-to-end on a real 276-frame counters
  game + on mocked/synthetic frames. Commits c651ec7 (viewer), 6592a08 (lobby), 5bf5fec (serving),
  0dc9463 (auto-save).
- **webui:** **Live god-view spectating — replay/spectate feature COMPLETE (#31/#32/#33 done).** On
  engine's `set_replay_sink` (9ec1fbf): `spawn_game` installs a sink forwarding each omniscient
  `ReplayFrame` to the room `SpectateHub` (cached for late joiners; same frames feed the auto-save).
  New `ServerMsg::GodFrame{state:GodView,label}`; removed the old PlayerView-mirroring `SpectatorTee`.
  Spectator client runs `godMode` + renders god frames via the replay viewer's adapter (both hands
  face-up, ordered libraries openable, live "what happened" label). Playwright-verified: a spectator
  sees the opponent hand (7) + full ordered library (53, top-first) + live labels — zero hidden info.
  13 web tests green. Commit e9237a7. Frame-size mitigation (gzip-on-serve) deferred until it bites.
- **engine:** **Replay core landed (REPLAY_PLAN) — schema locked + recorder.** New `crate::replay`
  serde contract (posted to webui+gym first for parallel build): `GodView` (omniscient, every zone
  of every player face-up, libraries top-first) reusing `ObjView`/`CharacteristicsView`; `Replay`
  = `meta` + `frames`; `ReplayFrame{GodView,label}`; `ReplaySource` `Human|AiTraining{step}`;
  caller-stamped `created_at`/names/decks. `state::view::god_view(&GameState)` builds it (spectator-
  feed entry point). `Engine::record_replay(true)` captures a labelled frame per public event;
  `replay()`/`replay_frame_count()` expose it incrementally (live spectator) + final; `Outcome`/
  `EndReason` gained serde. 3 tests (wire-shape lock, omniscient-frame capture, JSON round-trip).
  Commit a533720; isolated from the lead's die-roll commit. (NB: a pre-existing failing test
  `opening_hands…skips_first_draw` is the lead's 497df25 die-roll, not the replay core — flagged.)
- **webui:** **3 card-UI fixes (user) + #30 badge closed on real data.** (1) **Stuck hover preview:**
  per-card mouseenter/leave listeners got orphaned when the board re-renders under the cursor (the
  hovered node is replaced, so mouseleave never fires → preview stuck). Replaced with a single global
  pointer tracker — cards carry their preview URL in `data-preview`; `refreshPreview()` re-derives the
  hovered card via `elementFromPoint` on every move AND after every render, so nothing can orphan it.
  (2) **Missing card art:** `card-art.json` was a stale 14-card list; regenerated for the full 38-card
  `starter_db` (all resolved on Scryfall, zero missing) — generator dict synced to the pool. (3)
  **Oversized card text:** `.c-rules` now line-clamps (4 lines board / 6 hand) so long oracle text
  (Mossborn Hydra) shows more but ellipsis-clips instead of blowing out of the frame. Embedded+Vite
  synced; tsc/vite clean. Playwright-verified live on a counters game: art 4/4, 0 frame-overflows
  (Sazh's Chocobo truncates), hover never stuck off-card across re-renders + re-acquires. Commit
  d9c7d43. **#30:** wire-level test (a99f23d) proves a real partial card (Lumbering Worldwagon)
  serializes to `fully_implemented:false` through the exact `ServerMsg`, a complete card to `true` —
  badge confirmed end-to-end on real data (render path already Playwright-verified). **#30 done.**
- **webui:** **Replay/spectate feature (REPLAY_PLAN.md) — taken, planned, coordinated.** Owning
  lobby+viewer+serving. Sent team-lead my plan + coordination asks (lock GodView/Replay serde field
  names — esp. ordered-library + both-hands exposure; the `god_view` spectator hand-off API; gitignore
  `data/replays/`). Scaffolding (lobby "AI Training Replays" section, `/play?replay=<id>` god-view
  playback viewer) waits on the locked schema; Rust serving/auto-save/god-view-spectator wait on schema
  + engine replay core. Tasks #31-33 filed.
- **webui:** **New `"counters"` preset deck** (user) + badge wire-test. A G/W landfall + +1/+1-counters
  midrange 60 built server-side (`driver::counters_deck`) from the implemented pool by `grp_id` —
  exercises mana dorks, a conditional dual, ETB draw, three landfall payoffs, counter-doubling +
  Hardened Scales, an anthem, equip/aura, a CDA search vehicle, and two tracked-incomplete cards (so
  the ⚠ badge shows on a real deck). `driver::resolve_deck` routes it through the lobby (now default),
  legacy `?p0/?p1`, and CLI. Commits ade8b5a, a99f23d.
- **engine:** **Subtypes/supertypes flipped from strings → generated enums (CR 205.3/4).** Lead
  directive; I drove the whole no-fallback flag-day flip in one green commit (5b9f63d), design
  parked. `Characteristics`/`ComputedChars.subtypes → Vec<Subtype>`, `supertypes → Vec<Supertype>`;
  `CardFilter::HasSubtype(Subtype)`/`Supertype(Supertype)`; `TokenSpec.subtypes: Vec<Subtype>`.
  Engine matching mostly survives unchanged (`Vec::contains` is generic); rewrote the string-literal
  checks (`mana::basic_land_type_color`, sba/priority Aura/Equipment) to enum matches. All card
  producers + builders migrated (`creature()` takes `CreatureType`; the 7 two-subtype bodies — Human
  Soldier, Elf Archer, … — set the full subtype line after via a `two_subtype_kw` helper / inline,
  preserving fidelity). Views Display-convert enums to the canonical type-line string so the wire/
  webui JSON is byte-identical (also fixed gre-server's `DeckCardView`). Snapshots regen'd. Full
  workspace green (106+13+9), no new warnings. Kills stringly-typed-subtype typo risk. **design to
  review cards/+effects/.**
- **engine:** **`fully_implemented` surfaced in the view (#30, engine side)** + **subtype enums landed
  (step 1).** (1) `CharacteristicsView` gained `fully_implemented: Option<bool>`, populated in
  `view.rs::chars_view` from `CardDef.fully_implemented` (design's 2fdaa77) via grp_id — `Some(true/
  false)` for real cards, `None` for engine objects; webui's ⚠ badge (⚠ iff `Some(false)`) now runs
  on live data. 3-case test. (665773d). (2) Subtypes/supertypes → enums: generated `crate::subtypes`
  (Subtype tagged + Supertype flat, Display/FromStr/serde-as-string) landed additively/green
  (924a321). The hard `Characteristics` field flip (Vec<String>→Vec<Subtype/Supertype>) + matching +
  CardFilter IR variants + every card def is one atomic commit (no additive bridge) — coordinating
  shapes + who-drives with design before the destructive sweep. View stays Vec<String>+Display so
  the wire is unchanged. 106 tests green.
- **webui:** **New `"counters"` preset deck** (user request — "a more complicated deck that uses
  more of the cards/functionality"). A G/W landfall + +1/+1-counters midrange 60 (24 land / 26
  creature / 10 noncreature), built server-side in `driver::counters_deck()` from the implemented
  pool by `grp_id` (kept in the server crate — composes engine cards by id rather than adding a
  `preset_deck` in card-agnostic `mtg-core`). Exercises a broad slice of the engine in one game:
  Llanowar Elves (mana dork) + Hushwood Verge (conditional dual), Elvish Visionary ETB-draw, three
  landfall payoffs (Sazh's Chocobo +1/+1, Mossborn Hydra counter-double, Icetill Explorer mill),
  Hardened Scales counter-replacement, Glorious Anthem static, Bonesplitter equip + Pacifism aura,
  Lumbering Worldwagon `*`/4 CDA + basic search, and keyword bodies — incl. two tracked-incomplete
  cards (Icetill, Worldwagon) so the ⚠ badge shows on a real deck. New `driver::resolve_deck` routes
  the name through the lobby picker (now defaults to `counters`), legacy `?p0/?p1`, and the CLI
  `preset`/`play`. Tests: deck is a legal 60, all ids resolve in `starter_db`, RandomAgent mirror
  plays to a winner (12 web tests green). Verified live: lobby lists it, agent-vs-agent counters
  game runs to `finished{winner:0}`. Commit ade8b5a.
- **webui:** **"not fully implemented" ⚠ card badge** (user/lead request, webui half). A yellow ⚠
  corner badge renders on any card whose view `chars.fully_implemented === false`, with a hover
  tooltip "Not fully implemented:\n<rules_text>" (the deferred clause the engine documents in
  rules_text). No JSON-projection change needed — board `chars` are `CharacteristicsView` serialized
  whole into `PlayerView`, so the field passes through automatically once engine adds it (rules_text
  already present). Wired **forward-compatibly**: strict no-op until the field exists (`undefined`/
  `true` show nothing — no false positives). Mirrored embedded + Vite (+ CSS re-sync). Verified via
  Playwright with synthetic `fully_implemented:false` injection: flagged cards show ⚠ + the
  deferred-clause tooltip; control (no field) shows zero badges. Auto-activates when
  `CardDef.fully_implemented`→view lands (task #30 = real-data pass then). Commit 10a31bf.
- **engine:** **C19 DONE (#28) — `mana_colors` shortcut fully retired; mana production is 100% IR.**
  With design's card-side migration complete (basics → intrinsic CR 305.6 subtype mana;
  Llanowar/Hushwood → `Activated{is_mana}`), removed the `mana_colors` field from `CardDef`, the
  fallback union in `mana::producible_colors`, and trimmed `is_mana_source` to the authored-ability
  check. Mana colour now comes ONLY from IR mana abilities + intrinsic basic-type mana (computed
  subtypes). Migrated the summoning-sick dork test to a real `{T}: Add {G}` ability; cleaned a
  pre-existing dead `CounterKind` import. 105 tests green, workspace builds. `ManaSpec.one_of`
  deferred by mutual agreement (no current/planned card needs constrained "add A or B" mana).
- **engine:** **CR 305.6 — basic-land-type mana is intrinsic, derived from computed subtypes**
  (C19/#28 follow-through, lead-flagged). `mana::mana_sources` now unions three colour sources:
  IR mana abilities (`Ability::Activated{is_mana}`, condition-aware), the **intrinsic** basic-type
  mana read from each permanent's COMPUTED subtypes (`Forest`→{G} … `Plains`→{W}, new
  `basic_land_type_color`), and the legacy `mana_colors` fallback. Reads post-layer subtypes, so
  animated lands / Spreading Seas / Urborg-style type changes carry mana for free; basics + typed
  duals now author as just their type line (no `mana_ability`, no `mana_colors`). New test
  `basic_land_type_mana_is_intrinsic_from_subtype`. 105 tests green, clippy clean. `mana_colors`
  removal + `ManaSpec.one_of` wiring still pending design dropping the shortcut from basics/lands.
- **gym:** **GYM_PLAN milestone 1 COMPLETE — obs encoder + factored action space + MaskablePPO
  beats random.** Replaced M0's flat obs/action with the real thing (gym-side only):
  `layout.rs` (shared entity ordering/sizes so obs row `i` ↔ action slot `i`), `obs.rs` (structured
  `Dict` obs — globals + per-permanent/hand/stack rows with computed P/T, types/colors/keywords,
  status, counters, combat role, **`grp_id` card-embedding ids**; `obs_spec()` so Python builds the
  space, never hard-coded), `codec.rs` (factored `Discrete(98)` vocab `COMMIT/HAND/PERM/PLAYER/
  STACK/MODE/COLOR/NUMBER/YES/NO` + an autoregressive `Interaction` state machine that decomposes
  targets/combat/multi-select/ordering into single-index sub-steps env-side, committing the batched
  `DecisionResponse` only at the end; rare value decisions use the engine's canonical default).
  `lib.rs` drives sub-steps (one engine request → N gym steps). Python: `MtgEnv` is single-agent vs
  a fixed (random) opponent so win-rate-vs-random is measurable; `policy.py` DeepSets extractor
  (grp_id embedding → per-row MLP → masked mean-pool); `train.py` MaskablePPO. **Exit met:** demo
  0.615→**0.770**, burn-vs-bears 0.052→**0.917** win-rate vs random. 9 Rust + 9 pytest tests green
  (incl. a ~20s learning-sanity test). Obs/codec stay swappable for M2/M4; needs zero engine changes.
- **webui:** **lobby spectating + per-decision delay** (user request). Read-only spectating:
  `/ws?game=<id>&spectate=1` subscribes to a per-room `SpectateHub` (a `tokio::broadcast` of seat-0
  view frames, fed by a `SpectatorTee` agent wrapping seat 0 that mirrors every `observe` to the
  hub) — late joiners get the cached current board immediately, then live frames, then GameOver. The
  existing game client renders the stream read-only (`👁 Spectating` banner, no prompts). Per-game
  **decision delay** (`delay_ms` in create form/REST): a `DelayAgent` wraps each non-human seat and
  `sleep`s before each decision, pacing the single-threaded engine so AI-vs-AI games are watchable;
  humans unaffected. Lobby Spectate button now live for non-finished games; ⏱ chip shows the delay;
  also added a DELETE endpoint + ✕ remove / Clear-finished (prior commit). Verified (WS + Playwright):
  delay=0 game finishes instantly → spectator gets cached final view + GameOver; delay=120 game
  streams paced live frames (13 over 2.5s, spread in time); late-join gets the board at t=0. 10 web
  tests green. Commits 197cfe0 (remove/clear), 7b204ab (spectate+delay).
- **design:** **Effect-IR for batch-1 caps + first real cards (Selesnya Landfall push).** Additive
  IR: `CardFilter::Supertype` + `Effect::Search.tapped` + `Effect::Fight` (06ce436);
  `StaticContribution::SetBasePTValue` (7a CDA) + `ManaCost.x` (27879eb); `ValueExpr::CountersOnSelf`
  (w/ engine, d95abe1). Authored 4 fidelity-clean cards (per-first-printing-set folders, each
  expect-tested): **Sazh's Chocobo** (fin) + **Mossborn Hydra** (fdn) (95cd0c8), **Icetill Explorer**
  (eoe) (8ce5ea1), **Lumbering Worldwagon** (dft `*`/4 CDA) (30d865a). **Fidelity standard (user):**
  no silent approximations — incomplete clauses TRACKED in-file, never shipped wrong. Tracked: Icetill
  land-play perms (C18), Lumbering **Crew 4**. **Held on engine caps:** Bushwhack (cast-time
  modal-targets), Erode (`PlayerRef::ControllerOfTarget`), Dyadrine (mana-spent value + "you attack"
  event). **NEXT:** the C19 mana migration engine handed off — `ManaSpec.one_of` + `basic_mana_ability`
  builder + migrate basics/Llanowar/lands off `mana_colors`, incl. Hushwood Verge's conditional `{W}`
  via the new `Condition::CountAtLeast`. **Purge:** holding the 6 card defs until engine clears the
  priority.rs test refs.
- **engine:** **C19 — mana production as first-class IR** (engine side, lead's priority). New
  `conditions.rs` — a pure `Condition` evaluator (`holds`: CountAtLeast/life/turn/All/AnyOf/Not).
  `mana.rs` now derives a source's producible colours from its `Ability::Activated{is_mana:true}`
  mana abilities (the `Effect::AddMana` ManaSpec), gated by `Restriction`/`Condition` — so a
  conditional mana ability (Hushwood Verge {W} iff you control a Forest/Plains) is only offered
  when legal; kept the `mana_colors` shortcut as a transitional fallback so existing lands don't
  regress. `Effect::AddMana` → mana pool (produces + any_color via ChooseColor). 104 mtg-core
  tests green, clippy clean. NEXT (design): add `ManaSpec.one_of` + `basic_mana_ability` builder +
  migrate basics/Llanowar/lands to the IR mana ability; then I wire one_of + remove the fallback.
- **engine:** **partial-card test purge DONE.** Per lead, cleared all test refs to the 6
  soon-deleted partial cards (Humility/Rancor/Fog Bank/Servant/Chandra/Healing Salve) from
  chars/mod.rs + priority.rs + combat/mod.rs. Coverage preserved via self-contained synthetic test
  CardDefs (a `synth_state()` helper: loyalty planeswalker, combat-damage-prevention 0/2,
  0/0-enters-with-counter — keeps the whiteboard replacement-pass tests incl. Hardened Scales +
  CR 616.1f — and a +2/+0-trample aura). Humility test dropped (Nature's Revolt covers 7b). 104
  mtg-core tests green. design is clear to delete the defs (pinged). Leftover: a "Rancor" doc
  comment in design's effects/target.rs (their file, harmless).
- **webui:** **lobby + per-seat agent assignment** (user request). New `lobby.rs`: a server-side
  game registry (`Arc<Lobby>` axum state) where each `Room` configures *both* sides — every seat is
  a `Human`, a `Random` test agent, or `Rl` (stubbed→random for now). REST `GET/POST /api/games`
  (+`/api/games/:id`); the lobby landing page (`/`, new self-contained `lobby_client.html`) lists
  games and creates them; the game client moved to `/play` and binds to `?game=<id>&seat=<n>` (one
  browser tab per seat — open two to play both sides). Rooms **auto-start when every human seat has
  connected** (agent-only games run on create). The rendezvous is one `Mutex<StartState>` (derived
  fullness, drain-to-spawn, double-claim reject, pre-start slot-vacate on disconnect). Load-bearing
  detail: `Box<dyn Agent>` isn't `Send`, so the room stores only `Send` channel *ingredients* per
  seat and the spawned engine thread builds the agents itself (mirrors the legacy path). Added
  `driver::room_engine` (per-seat stop handles) + `state_for_deck_names`; factored the socket loop
  into `server::run_player_socket` shared by legacy + lobby. Legacy `/ws?p0=&p1=` path preserved
  verbatim. Verified end-to-end (REST + WS + Playwright): agent-vs-agent finishes on create; a lone
  seat does NOT start + vacates on disconnect; 2-human auto-starts only after both connect and both
  drive it to GameOver; human+random plays like today; `/play?game=…&seat=0` shows "you are Player
  0"; legacy path still works. 10 web tests green. Commits fd9a72b, ffb820b.
- **gym:** **GYM_PLAN milestone 0 COMPLETE — PyO3 boundary + random self-play.** New crate
  `crates/mtg-py` (PyO3/maturin `cdylib`, depends only on `mtg-core`, abi3-py39 so it builds
  against the box's CPython 3.14): a `PyGame` handle + thread+channel `PyAgent` (port of
  `GreSessionAgent` — game runs on its own OS thread, each seat's `decide` ships `(view, req)` over
  a channel and blocks; Python pulls via `step_to_decision`, answers via `apply`; GIL released
  around the blocking recv). Swappable seams kept minimal but real: `obs.rs` (PlayerView→f32 global
  scalars, `OBS_DIM=54`) and `codec.rs` (every `DecisionRequest`→non-empty canonical legal-response
  list→flat `Discrete(ACTION_DIM=64)` + bool mask; decode clamps — illegal action impossible). Thin
  `python/mtgenv_gym/` (`MtgEnv(gym.Env)` reading dims from the extension, low-level self-play
  driver, smoke test, benchmark). **Exit criteria PASS**: 11k random games (lands/demo/burn-vs-bears,
  auto-pass on+off), 0 panics, **0 empty masks across ~2.2M decisions**, 100% card+zone conservation;
  ~10k–24k decisions/s/thread single-threaded. 6 Rust + 8 pytest tests green. M1 (real obs encoder +
  factored action space + PPO) swaps `obs.rs`/`codec.rs` with no plumbing change; snapshot/clone
  stubbed pending the M3 resumable step API (needs `engine` coordination).
- **engine:** **card-push batch 1 COMPLETE (C9b + C10)** — all of C1–C10 now landed. C9b dynamic
  P/T: `ValueExpr::CountersOnSelf` in eval_value (Mossborn Hydra "double the +1/+1 counters" =
  `PutCounters{SourceSelf, n: CountersOnSelf(+1/+1)}`); `StaticContribution::SetBasePTValue` as a
  layer-7a CDA in chars::compute (Lumbering Worldwagon's power = lands you control), with a chars-
  local ValueExpr evaluator. C10 X-costs: `ManaCost.x`; cast_spell asks `ChooseNumber(ChooseX)`
  bounded by affordable mana, pays generic + X·x, carries X on `StackObject.x`, and resolve_top
  sets `ctx.x` so `ValueExpr::X` reads it. (Also added the missing `CounterKind` import to
  effects/value.rs that design's `CountersOnSelf` addition needed.) 99 mtg-core tests green, clippy
  clean. **C1–C10 done**; design can author all Tier-1/2/3 cards + Mossborn/Lumbering/Dyadrine.
  Remaining: C11–C18 subsystems (dual lands, earthbend, crew, warp, target-trigger, exile-types,
  land permissions) — shaped per-card with design. NOTE: temporarily added a placeholder
  crates/mtg-py/src/lib.rs to unblock a workspace-wide cargo break (gym's crate had Cargo.toml but
  no lib.rs); gym has since filled it in.
- **engine:** **card-push capabilities, batch 2 (C5, C7, C8)**. C7 Modal: added an interactive
  resolution interpreter — `resolve_effect` now drives `interpret()` (asks `ChooseModes`, resolves
  the chosen modes) while still materializing pure leaves with the shared target cursor. C5 Search:
  `interpret_search` (SelectCards → move picks to `ZoneDest` → shuffle a searched library); honors
  `Effect::Search.tapped` (fetch lands enter tapped) + `CardFilter::Supertype` (basic-land filter) —
  both added by design. C8 Fight: `Effect::Fight` → two simultaneous Noncombat `Damage` actions
  (each creature's power to the other; deathtouch/lethal interact via the one whiteboard). 96
  mtg-core tests green, clippy clean. CAVEAT noted to design: Modal chooses its mode at RESOLUTION;
  a modal mode that TARGETS (Bushwhack's fight mode) needs cast-time mode+target selection (the
  Fight effect itself works via locked targets) — a follow-up. Batch-1/2 capabilities C1–C9a + C6–C8
  are ready for card authoring. Pending: C9b (dynamic P/T CDAs), C10 (X costs) — IR asks sent;
  C11–C18 subsystems.
- **webui:** **new default stop set** (user request): your Main 1 + Main 2 (engine Arena default,
  own-turn) PLUS the opponent's Begin-Combat + End step (the instant-speed windows). Made
  `driver::Stops.overrides` per-`(step, own_turn)` (`Vec<(Phase, bool, bool)>`), seeded the two
  opp-side stops in `Stops::default`, applied via `set_stop_side`/`set_override`; server `?stops=`
  param now layers on the defaults and supports a `Name@you|opp:val` side suffix (bare = both
  sides). CLI `stop` cmd toggles both sides + shows the side. Verified the wire echo matches exactly
  (MP1/MP2 mine-only, BeginCombat/End opp-only, rest off). 10 web tests green. Commit 3c4d5a2.
- **engine:** **card-push capabilities, batch 1 (C1–C4, C6, C9-Count)** — all additive-only, no IR
  change (the Effect/ValueExpr nodes existed but were no-ops). C1: mana.rs gates a creature mana
  dork by summoning sickness (CR 302.6). C2: `Effect::PutCounters` → `Action::AddCounters`. C3:
  `Effect::Mill` → real (top N library → graveyard). C4: **landfall** via a new watching-enters
  trigger scan in `collect_triggers` — on any permanent's ETB it scans battlefield permanents for
  `PermanentEnters(filter)` triggers, filter evaluated relative to the watcher's controller (so "a
  land you control enters" works; no `LandEntersControlled` variant needed — proposed reuse to
  design). C6: `Effect::CreateToken` → real (token onto battlefield, summoning-sick; TokenSpec
  keywords still a vanilla no-op). C9: `ValueExpr::Count` → real (count objects in a zone by
  filter + optional controller, e.g. lands you control). 91 mtg-core tests green, clippy clean.
  Pending in this batch: C5 (Search), C7 (Modal), C8 (Fight) need resolution-time agent decisions.
- **lead:** **Card-pool push kicked off — Standard Selesnya Landfall** (60-card deck, 18 unique
  nonbasics). Built a **SQLite card index** (`scripts/build_card_index.py` → `data/scryfall/
  cards.sqlite`, one row per printing, indexed by name/oracle_id) and wired it into `setup.sh`, so
  card lookups are instant instead of `jq`-ing the 550MB JSON (~2 min/pass). Spec + per-card data +
  ease tiers + the interpreter-capability list (C1–C18) → `docs/plans/SELESNYA_LANDFALL_CARDS.md`.
  Delegated on disjoint file seams: **engine** = interpreter capabilities (whiteboard/effects),
  **design** = `cards/` refactor (misc/ + per-set folders by first-printing set) + card authoring.
- **lead:** Implemented the **London mulligan (CR 103.5)** in `start_game::run_mulligans` — rounds
  in turn order, shuffle-hand-into-library + redraw on mulligan, bottom one card per mulligan on
  keep, all through the `Agent` boundary (`Mulligan` + `SelectCards{BottomForMulligan}`). RandomAgent
  keeps every hand (a coin-flip mull is noise for self-play; keeping consumes no RNG so seeded games
  stay deterministic). The web/CLI projection already handled both requests. 85 mtg-core tests green
  + a new scripted-mulligan test.
- **webui:** **play-UX polish from rapid user feedback.** (1) Stop dots are now full-width
  clickable rows (much bigger hit targets, 12px dots) instead of tiny circles; (2) the two
  per-step dots reordered to opponent-on-top / **you-on-bottom** (matches board layout); (3)
  **target-selection feedback** — fixed the missing `button.opt.sel` style so a chosen option is
  visibly highlighted, made the board's player panels click-targets (glow + "🎯 target" badge) for
  player-targeting choices, selected board cards get a 🎯 corner badge, and the prompt shows a
  "🎯 Chosen: …" summary; (4) **Space submits a valid in-progress selection** (declare
  attackers/targets/order/number), not just pass; (5) **SmartStops OFF by default** (`Stops::default`
  + server flag defaults tied to it) — users found "stop wherever I *could* cast" too chatty.
  Verified via Playwright (20 dots, opp/you legend, 18px rows, Bolt→player target highlights on
  panel+button+summary, Space→`picks` frame) + WS (smart_stops=false echo). NOTE: fully honoring
  "no priority on non-stop steps regardless of castable spells" needs an engine tweak — with
  SmartStops off the engine still falls back to `is_unimportant_step`, so it prompts at the
  opponent's main phases + combat steps when you hold an instant. Proposal sent to engine (make
  SmartStops-off auto-pass all non-stop empty-stack windows). 10 web tests green. Commit 81bb8e7.
- **webui:** **per-turn-side stops in the UI + MTGA keybindings.** Consumed the engine's new
  per-`(Phase, own_turn)` stop API: `ServerMsg::Stops.per_step` now carries both sides as
  `(step, on_my_turn, on_opp_turn)`; `ClientMsg::SetStop` gained an `own` flag;
  `stops_msg` zips `effective_steps(true/false)`, `SetStop`→`set_override(step, own, …)`,
  `engine_with_stops` seeds both sides. Phase bar renders **two stop dots per step** (top = your
  turn, bottom = opponent's, dashed ring = opp, independently clickable) with a you/opp legend —
  the user can stop on their own draw but not the opponent's. **Keybindings** (per
  `../mtga-re/docs/priority_stops.md`): **Space** = pass priority / take the sole forced option;
  **Enter** = pass through all of THIS turn's remaining priority stops (mirrors the GRE's
  `autoPassPriority=Yes`/`AutoPassOption.Turn` — a per-turn hold shown by a badge, lapses next turn,
  still surfaces real choices like targets/blocks); **Esc** cancels the hold. Mirrored across the
  embedded + Vite clients (CSS re-synced, dist rebuilt). Verified end-to-end: WS shows 3-tuple
  `per_step` + independent per-side toggles + Arena default (MP1 = your-turn only); a my-Upkeep stop
  set over the socket actually yields an upkeep prompt (engine honors it); Playwright confirms 20
  dots + legend, per-side dot toggle, Space→one `{pass:true}` frame, Enter→multi-stop pass-through +
  badge. 10 web tests green, workspace builds. Commit 8699b79.
- **engine:** **per-turn-side stops** (webui request). `StopConfig.overrides` now keyed by
  `(Phase, own_turn)` (`own_turn = seat == active_player`) so a seat can stop on its OWN draw step
  but not the opponent's. New `StopConfig::stop_for(step, own_turn)` primitive; `set_override` +
  `effective_steps` gain an `own_turn` arg; `Engine::set_stop` stays both-sides (back-compat, CLI
  unchanged), new `Engine::set_stop_side` for one side. Arena default unchanged (MP1/MP2 stop only
  on your own turn). 84 mtg-core tests green, clippy clean. mtg-gre-server callers (3 sites) are
  webui's to adapt — flagged the signatures.
- **webui:** **migrated the web stop policy onto the engine's `stops_handle`** (removes the
  duplicated client-side policy from the earlier entry). The game thread builds the engine via new
  `driver::engine_with_stops(state, agents, human, &Stops)` (auto-pass ON for the human seat) and
  hands the seat's `Arc<Mutex<StopConfig>>` back to the socket task over a tokio oneshot — the
  Engine never leaves the thread (`dyn Agent` isn't `Send`); only the Send handle crosses.
  `GreSessionAgent::decide` is now a plain round-trip (engine elides trivial windows itself);
  `SetStop`→`StopConfig::set_override`, `SetOption`→pub fields, echo reads `StopConfig`. Deleted
  `driver::Stops::{should_ask,is_stop,effective_steps}` (the web/CLI now share the ONE engine
  policy — no drift). Verified: engine auto-passes Upkeep/Draw (first prompt at Main 1), live
  Upkeep toggle lights + yields a real Upkeep prompt with no reset (WS + Playwright); decklist
  intact; 10 web tests green. Commit d21fc14.
- **engine:** task #14 checkpoint 4 — **planeswalkers** (DONE → #14 complete). Groundwork:
  `Characteristics.loyalty` (printed), enters-with-loyalty on battlefield entry (CR 306.5b),
  CR 704.5i 0-loyalty SBA, `Object.used_once_per_turn` (CR 606.3) reset each turn. **4a Loyalty
  abilities:** loyalty cost via design's `CostComponent::Loyalty(i32)` in `can_pay_cost`/`pay_cost`
  (+N adds counters, −N removes, −N gated on loyalty≥N); once-per-turn-per-planeswalker enforced;
  loyalty abilities flow through the existing activated-ability path (sorcery-speed, controller-only
  by construction). Card: Chandra, Pyrogenius (+2 deals 2 to each opponent, −3 deals 4 to target
  creature; −10 ultimate deferred). **4b Attackable:** `declare_attackers` offers the defending
  player's planeswalkers as attack targets (CR 508.1a); `apply_damage` to a planeswalker removes that
  many loyalty counters (CR 120.3/306.8), 0-loyalty SBA handles death. starter_db 37→38, 84 mtg-core
  tests green, clippy clean. IR: only `CostComponent::Loyalty` (design). **#14 done** across 4
  checkpoints (combat keywords → rest of keywords → auras/equipment → planeswalkers). Deferred across
  #14: ward (cost IR), shroud, Rancor recursion, general enchant restrictions, planeswalker ultimates.
- **webui:** **live mid-game stop toggling** (fixed: was resetting the game) + **debug library
  peek**. Stops: moved the MTGA auto-pass/stops POLICY client-side into `GreSessionAgent` over a
  shared `Arc<Mutex<driver::Stops>>` the socket mutates on `SetStop`/`SetOption`; engine auto-pass
  stays OFF so the agent elides windows per the live config — clicking a phase-bar step or top-bar
  toggle now changes stops at the next priority window with **no reset** (verified browser +
  Playwright). (Engine also landed a server-side `stops_handle`; webui doesn't consume it.) Library:
  a player can't see their own library (hidden info; would leak draws to the RL agent if put in
  `PlayerView` — design flagged), so the peek is a **static starting decklist snapshotted server-side
  from `GameState` before run** (`ServerMsg::Decklist`, grouped by card, order discarded) → grouped
  MTGO-style deck-view modal on the Lib pile. Also: removed artist credits, hover→full-card image,
  MTGO phase bar (all 12 steps) above the hand, clickable GY/exile zone viewers. Mirrored across the
  no-build embedded client + the Vite client (rebuilt dist; CSS synced). 10 mtg-gre-server tests green.
  NOTE: `embedded_client.html` is baked via `include_str!` in server.rs — editing it only re-bakes
  when server.rs's mtime changes (touch it before rebuild); and when `web/dist/` exists the server
  serves the Vite build, not the embedded client (keep both in sync).
- **engine:** task #14 checkpoint 3 — **auras + equipment** (the attachment subsystem), in three
  commits. **3a Auras:** `Object.attached_to` + detach-on-zone-change in `move_object`;
  `CardFilter::AttachedHost` matcher (source-relative, like ItSelf → resolves to the host) so a
  "enchanted creature gets …" static lives in the normal global gather scan; Aura spells target a
  creature at cast and enter the battlefield attached on resolution (illegal target → graveyard);
  CR 704.5 Aura fall-off SBA. Card: Rancor (+2/+0 & trample). **3b Equipment + the activated-ability
  path:** `legal_priority_actions` now enumerates non-mana activated abilities (masked by timing /
  restriction / cost / legal target); `activate_ability` puts it on the stack, chooses targets, pays
  the cost (`pay_cost`); `resolve_top` runs Activated alongside Triggered; `Effect::Attach` →
  `Action::AttachTo`; `target_candidates` now honours `ControlledBy` (equip's "creature you control");
  CR 704.5q equipment-unattaches SBA. Card: Bonesplitter (Equip {1}, +2/+0). **3c Qualification
  dimension:** `ComputedChars.qualifications` gathered through layer 6 (`StaticContribution::
  Qualification`), read by combat — `CantAttack` (declare_attackers) + `CantBlock` (can_block). Card:
  Pacifism. starter_db 34→37. 78 mtg-core tests green, clippy clean. IR (design): used
  `Effect::Attach{what,to}`, `CardFilter::AttachedHost`. Deferred: Rancor's return-to-hand recursion
  (needs ReturnToHand + dies-trigger for non-creatures); general enchant restrictions (Auras hardcode
  "enchant creature"). Next: (4) planeswalkers.
- **engine:** webui ask — **live mid-game stop toggling**. `Engine::stops_handle(p) ->
  Arc<Mutex<StopConfig>>`: a UI session holds the handle and toggles a seat's stops from another
  thread; the engine re-reads the shared config at every priority window (no game reset). Moved
  `auto_pass` into per-seat `StopConfig` so the handle is self-contained (1:1 with webui's
  `driver::Stops`); added `StopConfig::set_override`/`effective_steps`. `stop_config(p)` now returns
  an owned clone. Lets webui delete its duplicated stop policy and let the engine own it.
- **engine:** task #14 checkpoint 2 — **rest of the evergreen keywords**. Haste (combat
  `declare_attackers` ignores summoning sickness when the creature has Haste); flash
  (`legal_priority_actions` treats Flash as instant-speed timing); hexproof (`targetable_by` +
  `target_candidates` exclude an opponent-controlled Hexproof permanent, but its own controller
  may still target it); indestructible-vs-destroy confirmed (added `Effect::Destroy` IR →
  `Action::Destroy`, whose `apply_action` skips Indestructible). New IR: only `Effect::Destroy`
  (additive; coordinated mentally with the existing vocabulary — no breaking change). Scryfall-
  verified single-keyword cards added: Raging Goblin (Haste), King Cheetah (Flash), Gladecover
  Scout (Hexproof), Darksteel Myr (colorless Artifact Creature, Indestructible), Murder
  ({1}{B}{B} "Destroy target creature"). starter_db 29→34. 71 mtg-core tests green, clippy
  clean. **Deferred:** ward (needs a cost-payment IR — will ping `design` at checkpoint 3/4)
  and shroud (niche, not in the Keyword enum). Next: (3) auras+equipment, (4) planeswalkers.
- **engine:** task #14 checkpoint 1 — **evergreen COMBAT keywords**. Added PRINTED keywords
  (`Characteristics.keywords: Vec<Keyword>`, seeded into `chars::compute` so the layer system
  layers grants/removes on top). Implemented: first strike & double strike (combat damage now
  has the two-substep model, CR 510.4, with an SBA pass between steps so a creature killed in
  the first-strike step doesn't deal back); trample (assign lethal to blockers, excess to the
  defender); deathtouch (any nonzero damage lethal — `Object.dealt_deathtouch` + SBA 704.5h,
  and 1 counts as lethal for trample assignment); lifelink (source's controller gains the
  damage); vigilance (doesn't tap to attack); menace (single block dropped → stays unblocked);
  defender (can't attack); + indestructible now prevents the lethal-damage/deathtouch SBA (not
  the toughness-0 one). Flying/reach were done in M5. Scryfall-verified single-keyword cards:
  Elvish Archers, Fencing Ace, Argothian Swine, Typhoid Rats, Child of Night, Alaborn
  Grenadier, Alley Strangler, Wall of Stone. 67 mtg-core tests green, workspace green, clippy
  clean. No effects/ change (Keyword enum already complete). Next checkpoints: (2) rest of
  evergreen keywords (haste/flash/hexproof/shroud/ward/indestructible-confirm), (3)
  auras+equipment, (4) planeswalkers.
- **gym:** refreshed `docs/plans/GYM_PLAN.md` to a current spec (was a stale sketch). Removed all
  Forge references (abandoned prior attempt); re-anchored on PyO3+maturin + the *implemented* `Agent`
  boundary (`agent.rs`: 21 `DecisionRequest` variants, `PlayerView`, `RandomAgent`). Key updates:
  obs space maps the real `PlayerView`/`CharacteristicsView` (`grp_id` card-embedding ids,
  hidden-info masking inherited from `view_for`); factored action vocab + boolean mask
  (MaskablePPO), reusing `options.rs`'s 5-shape projection; reward sparse-terminal + annealed
  potential shaping; auto-pass/stops (already in `priority.rs`) as the episode-length lever.
  **Resumable-step API**: documented two shapes — (A) thread+channel `PyAgent` reusing the proven
  `GreSessionAgent` bridge (zero engine change, ship now), (B) a true re-entrant `resume`/`submit`
  engine API (coordinate with `engine`, spec only). Testing reframed to CR expect-tests + captured
  MTGA logs (not a cross-engine oracle). Milestones 0→4. **Spec only — awaiting user review before
  any implementation.**
- **webui:** big UI/UX pass on `mtg-gre-server` (many small commits). (1) **MTGO-style web board**:
  real MTG card frames (name/mana/art/type/rules/PT), opponent-top/you-bottom layout, hand at the
  bottom, left rail (life + Library/Graveyard/Exile piles → click to view zones), readable game-log
  transcript, click-to-act legal-card highlighting; both the no-build embedded client and the TS/Vite
  build kept in sync. (2) **Real card data from Scryfall**: batch-resolved art manifest
  (`resolve-card-art.py` → `card-art.json`, served at `/card-art.json`, zero runtime API calls —
  loads cached CDN images), official mana/tap **symbol SVGs** in costs + inline in oracle text,
  artist + WotC attribution. Exact mana pips + oracle text from design's `mana_cost`/`rules_text`.
  (3) **Deck picker** (demo/burn/bears) in CLI (`play burn bears`, `preset`) + web (top-bar links /
  `?p0=&p1=`). (4) **#12 stops (MTGA auto-pass)**: auto-pass on by default for human play (prompts
  ~5× fewer: 144→29 CLI / 181→70 web), CLI commands (`autopass`/`smartstops`/`fullcontrol`/
  `resolvestack`/`stop <step>`/`stops`) + web toggle links, and a live per-step stops panel reading
  engine's `PlayerView.stops` ("stops: MP1, MP2 · smart"). (5) **#13 layers**: computed P/T +
  granted keywords render for free (Bears 2/2 →+Anthem 3/3 →+Levitation 3/3 [Flying]; Humility →1/1);
  CLI board render now shows P/T+keywords; aliases for anthem/levitation/humility. Server runs on
  :8080. All via the public Agent boundary (the formal client = `GreSessionAgent`/`HumanAgent`; the
  browser is transport+presentation below the line). Full workspace green throughout.
- **engine:** task #13 (ENGINE_PLAN milestone 5) — the **CR-613 layer system** (continuous
  effects), prototype-first (4 snapshot commits). New `chars/`: `ComputedChars` +
  `compute(state, id)` = base ⊕ layered static effects, the 7-layer framework with timestamps
  (613.7: `Object.timestamp` assigned on battlefield entry; effects sorted within a sublayer).
  Layers populated/validated: **6** (Grant/RemoveKeyword), **7b** (SetBasePT), **7c** (ModifyPT
  + ±1/±1 counters); 4/5 (type/color) framework-present, 1–3 (copy/control/text) deferred;
  613.8 dependency = timestamp ordering (genuine card-pair case deferred). Cards (Scryfall):
  **Glorious Anthem** (7c +1/+1, stacks), **Levitation** (6 grant flying), **Humility** (7b set
  base 1/1, modeling only the P/T clause). Dirty→recompute discipline: `GameState.chars_cache`
  + `chars_dirty`, marked on zone/counter changes, rebuilt by the agenda's recompute step
  (`recompute_continuous`); `computed(id)` reads the cache when fresh, else computes on demand
  (always correct). Integrated into **SBA** (death uses computed toughness), **combat**
  (computed power/lethal + **flying evasion** — a granted-flying creature is unblockable by
  non-flyers), and the **view** (battlefield P/T/keywords shown computed, so the UI sees
  anthems/counters). 58 mtg-core tests green (anthem stacking, grant-flying→combat, set-base
  then anthem then counter sublayer order, dirty discipline); workspace green, clippy clean.
  No effects/ change (built over design's `StaticContribution` IR). Deferred: layers 1–5 copy/
  control/text, CDAs, a genuine 613.8 dependency case, RemoveAllAbilities (Humility's other half).
  - **M5-gen: affects-reads-COMPUTED (CR 613.8)** — `compute` no longer pre-filters statics by
    base characteristics; each layer re-checks applicability against the type set computed
    through PRIOR layers. So a land turned into a creature (layer 4) is seen as a creature by
    a layer-6/7 effect. Validated: **Nature's Revolt** ("all lands are 2/2 creatures") + Glorious
    Anthem → your land-creature is 3/3 (anthem reads its computed Creature type), opponent's
    land-creature stays 2/2; combat creature-eligibility now uses computed type too (a
    land-creature can attack/block). 59 mtg-core tests green. STILL deferred (need new
    subsystems/IR, scoped for the lead): layer 1 copy (copiable values + ETB copy), layer 2
    control (auras + computed-controller refactor + `SetController`), layer 3 text, 7a CDAs,
    7d switch, intra-layer 613.8 dependency ordering.
- **engine:** task #12 — **Arena-profile priority auto-pass + MTGA-style stops** (decision
  elision, AGENT_INTERFACE §8.1) layered over the CR-correct priority loop. The engine still
  grants priority at every window; the policy elides the `Priority` prompt (treats it as a
  pass without consulting the agent) per `should_auto_pass`: never auto-passes a stop or under
  full control; with the policy off it always prompts. Rules: auto-pass when no non-pass
  action (except own MP1, a default stop); auto-pass through unimportant steps (upkeep/draw/
  begin+end combat/end) even with actions unless a stop is set; default stops = own MP1/MP2,
  declare-attackers (own turn), declare-blockers (defending). Per-seat `StopConfig`
  (full-control toggle + per-step overrides) on the `Engine`; API: `set_arena_auto_pass`,
  `set_full_control`, `set_stop(p, step, Option<bool>)`, `is_stop`, `stop_config`. **Off by
  default** (paper-CR / deterministic for differential-test + RL replay); a human/UI session
  enables it. Forced choices (targets/order/discard/mulligan/combat declarations) are
  untouched. 49 mtg-core tests green (policy unit tests + an end-to-end spy: minor steps
  elided, full-control prompts everywhere); workspace green, clippy clean. webui pairs the UI
  (stop toggles + full-control + phase/active-stops display).
  - **Refined to decompile's recovered MTGA spec** (../mtga-re/docs/priority_stops.md):
    persistent default stops are now **MP1/MP2 only** (declare-attackers/blockers are forced
    turn-based actions, not priority stops — dropped from defaults vs the task's literal list);
    added **SmartStops** (per-seat, MTGA default ON) = prompt wherever you have a legal play
    (replaces "auto-pass unimportant even with an action"; that's now the SmartStops-OFF mode).
    API adds `set_smart_stops(p, on)`.
  - **stackAutoPassOption = ResolveMyStackEffects** (MTGA default ON, per-seat) now implemented
    (the in-response-to-your-own-spell nuance the user asked about): while your OWN object is on
    top of the stack you auto-pass so it resolves — you're not re-prompted to respond to
    yourself; the opponent is still prompted to respond when they can act; full control / a stop
    override. `set_resolve_own_stack(p, on)`. Also added the MTGA `AutoPassOption` enum
    (UnlessAction/UnlessOpponentAction/ResolveMyStackEffects/ResolveAll/FullControl) +
    `set_auto_pass_option(p, opt)` mapping it onto the seat's flags (vocabulary for the UI; finer
    Turn/EndStep/ResolveAll distinctions approximated, refined later vs byte-exact defaults).
    Deferred: yields/answers, transient stops, captured ConnectResp.settings defaults.
  - **PlayerView.stops echo** (with design): design added `PlayerView.stops: Option<StopStateView
    { full_control, smart_stops, resolve_own_stack, per_step: Vec<(Phase,bool)> }>` (settings-echo,
    render-only, self-only); engine populates it via `Engine::view_for_seat` (per_step from
    `is_stop` over priority-granting steps), so webui renders the real per-seat stop config
    instead of hardcoded defaults. None when the profile is off. The SET half (live toggling)
    stays deferred — it's the real settings sub-protocol, not game state.
- **engine:** task #11 GENERALIZATION (milestone 4 cont.) — the rewrite pass + triggers are
  now beyond the self-scoped prototype (4 snapshot commits): (1) land plays routed through the
  whiteboard + `Rewrite::EntersTapped`/`Action::TapUntap`; (2) a **dies/LTB trigger** (Exultant
  Cultist "when this dies, draw") via the existing SelfDies path (source found in graveyard by
  grp_id); (3) **GLOBAL-scope replacements** — the pass now scans every battlefield permanent's
  `Ability::Replacement` (not just the affected object's own), with `CardFilter::ItSelf` /
  `ControlledBy(Controller)` evaluated against the replacement's source (design added ItSelf +
  `WouldAddCounters{kind,to}`). Validated on **Root Maze** (global "lands enter tapped" taps an
  opponent's land) and **Hardened Scales** (global "+1/+1 on a creature you control → +1 more"
  modifies Servant of the Scale's own enters-with-a-counter — a replacement modifying another
  replacement's output, resolved by the fixpoint → 0/0 enters as 2/2). Converted Servant/Fog
  Bank from `Any` to `ItSelf` (else they'd leak globally). (4) **CR 616.1f** player choice — when
  >1 replacement applies to one event, the affected object's controller picks via
  `DecisionRequest::ChooseReplacement`, then re-check; validated with two Hardened Scales (1+1+1
  ⇒ 3 counters, decision surfaced). 47 mtg-core tests green, workspace green, clippy clean.
- **engine:** task #11 (ENGINE_PLAN milestone 4) — **prototype-first** validation of the
  two architecture-defining subsystems, on 4 Scryfall-verified cards (4 snapshot commits):
  (1) TRIGGERED ABILITIES (CR 603): commit emits events → `collect_triggers` queues matching
  `Ability::Triggered` → agenda drains APNAP → `put_trigger_on_stack` chooses targets
  (603.3d) → resolve via the interpreter. `StackObjectKind::Ability { index }` carries which
  ability fired (looked up by grp_id, persists across zones). Validated: **Elvish Visionary**
  (ETB draw, non-targeting) + **Flametongue Kavu** (ETB 4 to target creature → lethal SBA).
  (2) WHITEBOARD REWRITE PASS (CR 614/616): real materialize→rewrite→commit replacing the M3
  straight-through, with the once-per-replacement guard + fixpoint, wiring design's
  `ActionPattern`/`Rewrite`. Validated: **Servant of the Scale** (Rewrite::EntersWithCounters —
  a 0/0 enters as 1/1 and survives) + **Fog Bank** (WouldBeDealtDamage{Combat}+Prevent — combat
  damage prevented). ETB + spell damage + combat damage now all flow through the whiteboard.
  Added `Object::effective_power/toughness` (counters affect P/T — trivial layer-7c) so the
  enters-with-counter is observable. Each interaction has an expect-test trace; 43 mtg-core
  tests green, full workspace green, clippy clean. CR/design notes (for generalization): a
  `CardFilter::ItSelf` + global-replacement consultation are needed beyond self-scoped
  replacements; 616.1f player-choice among replacements deferred. Coordinated with design (no
  effects/ change needed).
- **webui:** task #8 follow-ups (interactive play deepened). (1) Swapped the temporary driver
  for engine's real `Engine::run_game` (removed duplicated rules logic). (2) Built an
  **expressive CLI** (`mtg-play`): scenario setup (`new`/`life`/`add`/`deck`/`handsize`/`seat`),
  inspection (`show` god-view / `show <p>` filtered `PlayerView`), and a **scriptable** mode
  (`--script`, deterministic) — `cli.rs`/`render.rs`, shared option projection so CLI + web mask
  identically. (3) `play [decks…] [seed]` deals the engine's real decks — **demo** (creatures+burn)
  default, plus the user's **`play burn bears`** matchup — so casting, targeting (Lightning Bolt),
  combat and the damage/deck-out win conditions all surface (game-over prints the `end_reason`).
  The web board now deals the demo deck too (creatures render in-browser). (4) Wired engine's new
  `skip_opening_deal()` so `deal off` plays a hand-built scenario as-is. expect-test snapshots of
  the CLI render + the JSON wire projection (living protocol docs). 13 crate tests green; full
  workspace green. Next: place named starter-db cards in scenarios (`add … "Grizzly Bears"`).
- **engine:** post-M3 follow-ups (3 small commits): (1) adopted design's canonical
  `basics::CardType`, deleted the `state::CardType` duplicate (one import path); (2) added
  scenario hooks for webui's CLI — `Engine::skip_opening_deal()` (play a hand-built scenario
  with no shuffle/deal), public `Engine::legal_actions(p)` (pre-render the masked option set),
  and an `Outcome { winner, turns, reason }` via a new `GameState.end_reason`; (3) task #10 —
  added **Lightning Bolt** ({R}, 3 to any target) and the **Burn** (40 Bolt + 20 Mountain) /
  **Bears** (40 Grizzly Bears + 20 Forest) preset decks + `preset_deck`/`burn_vs_bears_game`;
  `mtg-cli` now takes deck args (`mtg-cli <seed> burn bears`). 39 mtg-core tests green,
  full workspace green (mtg-gre-server 10), clippy clean.
- **engine:** implemented task #9 (ENGINE_PLAN milestone 3) — a minimal PLAYABLE game:
  mana + casting + vanilla creatures + combat. New `cards/` module: `CardDef`
  (Characteristics + design's `Ability` IR) + a `CardDb` registry keyed by `grp_id`; a
  starter set (4 basic lands, Grizzly Bears 2/2, Hill Giant 3/3, Shock = 2 to any target,
  Divination = draw 2, Healing Salve = gain 3) + an R/G demo deck. `GameState` gains
  `card_db: Arc<CardDb>` (serde-skipped — card *data* out of snapshot state) + a `combat`
  field. `mana.rs`: mana sources, affordability, engine auto-tap payment (CR 605/118).
  Casting (CR 601, in `priority.rs`): `Cast` wired into the `Priority` decision with
  sorcery-vs-instant timing, target choice (601.2c), auto-pay, the stack; resolution runs
  the effect IR. `whiteboard.rs`: the **effect interpreter** over design's `Effect`
  (DealDamage/Draw/GainLife/LoseLife/Sequence) → materialize `Action`s → commit + emit
  events (replacement pass deferred to M4). `combat/`: declare attackers/blockers, combat
  damage (single/multi-block w/ `AssignCombatDamage`), simultaneous dealing; `sba.rs` adds
  creature death (704.5f/g). Two `RandomAgent`s now play lands→creatures→attack→damage to
  0 life (mtg-cli demo). 35 mtg-core/cli tests green incl. expect-test snapshots (cast
  Shock, unblocked attack, blocker trade, a full R/G combat-trace game); `cargo build`/
  `test`/`clippy` clean for mtg-core+mtg-cli. Coordinated the interpreter boundary with
  design (engine owns the interpreter over their IR); added `pub mod cards;` to lib.rs.
  Flagged a `CardType` duplication (mine in `state` vs design's in `effects::target`) for
  consolidation into `basics`. Deferred (M4+): keywords, layers, replacement/prevention,
  mana-via-IR, PayCost agent decision (auto-tap for now), mulligans.
- **webui:** implemented task #8 (CLIENT_PLAN M1–M2). New crate `crates/mtg-gre-server`
  (depends only on `mtg-core`): `human.rs` = **M1** stdio `HumanAgent` (a human is just another
  `Agent`); `session.rs` = **M2** `GreSessionAgent` bridging the boundary over a WebSocket via a
  **JSON projection** (`protocol.rs`); `options.rs` = shared request→`Prompt`→response
  projection so CLI + web render the *same* engine-enumerated legal set (masking); `server.rs`
  = axum host (`/ws` + static `web/dist`, with a no-build embedded client fallback). TS/Vite
  front end under `web/` (board/hand/stack + legal-only affordances). A **temporary** lands-only
  `driver.rs` runs the boundary until engine's loop is wired in (it uses only `mtg-core`'s
  public API). Verified: CLI plays a full game (`--bin mtg-play`); browser plays a full game vs
  `RandomAgent` (`--bin mtg-serve`, both embedded + Vite builds, screenshot-checked); `cargo
  build`/`test` green. TODO: swap `driver.rs` for engine #7's `Engine` entry point.
- **engine:** implemented task #7 (ENGINE_PLAN milestone 2) — a runnable lands-only game
  loop. New code in `mtg-core`: `state/` (`GameState`/`Player`/`Object`/`Characteristics`/
  `CardType`, `ObjId`-keyed arena, zones as `ObjId` vecs, `move_object`/`draw`/`shuffle`;
  `state/view.rs` = the `view_for(seat)` hidden-info masking that builds design's
  `PlayerView`), `turn/` (the CR-500s 12-step sequence + `step_grants_priority`/
  `is_main_phase`), `stack.rs` (the LIFO stack + `StackObject`), `sba.rs` (the player-loss
  SBAs 704.5a–c, esp. decking 704.5b), and `priority.rs` (the `Engine`: turn driver,
  turn-based actions, the **priority loop** with hold-priority/APNAP pass counting, and the
  **agenda fixpoint** recompute→SBA(loop)→triggers(APNAP)→priority per WHITEBOARD_MODEL §2.2).
  Choices flow through design's `Agent` trait (`RandomAgent`); only legal action in M2 is
  play-a-land (CR 116.2a), engine-masked. `mtg-cli` is now a lands-only self-play harness
  (`mtg-cli [seed] [lib]`) — two `RandomAgent`s deck each other out with no panics. Added
  `serde` to `Rng` so `GameState` snapshots/replays. 26 tests green incl. expect-test
  snapshots (enumerated legal options at a decision point; the one-turn CR-500s trace);
  `cargo build`/`test`/`clippy` all clean. Did NOT touch design-owned files
  (`agent.rs`/`effects/`/`basics.rs`/`error.rs`); no `lib.rs` change needed (filled existing
  module stubs). Deferred to M3+: mana/casting/combat declarations, the new-object rule on
  zone change (400.7, irrelevant lands-only), mulligans.
- **design:** implemented task #4 — the agent boundary + Effect IR are now real code in
  `mtg-core` (commit 360d3a6). New: `agent.rs` (the `Agent` trait, `DecisionRequest` 21-variant
  enum, `DecisionResponse`, `PlayerView` + view types, all supporting request types, `GameEvent`,
  and a `RandomAgent` reference backend that can only pick legal options); `effects/` split into
  `mod.rs` (the `Effect` IR), `action.rs` (`Action`/`Whiteboard`), `ability.rs` (the 5 ability
  kinds + costs/keywords/qualifications), `value.rs`/`target.rs`/`condition.rs`/`native.rs`
  (the `Native` escape hatch). Plus shared `basics.rs` (Color/Zone/Phase/Status/ManaCost/
  ManaPool/CounterKind/CounterBag/DamageKind/Target/ZoneDest — one canonical home; **engine
  imports these, doesn't redefine**) and `error.rs` (EngineError). `cargo build`+`cargo test`+
  `cargo clippy` all green; 6 unit tests (RandomAgent legality, ChooseNumber constraint
  honoring, determinism-by-seed, serde round-trip). Boundary types derive serde (the §1.1
  GRE-server contract). One open item flagged: batched `CastingTimeOptions` needs a multi-part
  response (decompose vs. structured) — ratify with engine/gym/client at integration.
- **design:** reconciled `AGENT_INTERFACE.md` against the recovered+log-validated GRE schema
  (decompile's `../mtga-re/`) — §9 now RESOLVED, not open. Confirmed strict-superset holds
  (variant set unchanged); enriched `ChooseNumber` to match `NumericInputReq` exactly
  (`step`/`disallow_even`/`disallow_odd`; `forbidden`↔`disallowedValues`). Key validation:
  GRE `CastingTimeOptionReq` embeds `numericInputReq`/`modalReq`/`selectNReq` as inner
  messages — i.e. GRE's own wire literally decomposes a cast's options into our
  ChooseNumber/ChooseModes/SelectCards sub-steps. `TargetSelection` ≅ our `TargetSlot`.
  Also added §8.1: decision *elision* (auto-pass / forced-single-option) is an engine/Arena-
  profile concern, uniform across all backends (load-bearing for differential-testing/replay).
- **client:** wrote `docs/plans/CLIENT_PLAN.md` (task #5) — web play UI + a **GRE-protocol
  server** (`mtg-gre-server` crate, axum + WebSocket, depends only on `mtg-core`) fronting the
  engine. A human at the web UI is just another `Agent` backend (`GreSessionAgent`) — same
  single boundary as RL/Gym and scripted AI. The seam is the GRE protocol itself, so the
  **real MTGA client can be dropped in** (two strategies: protocol-compatible server +
  endpoint redirect, vs. patch/runtime-hook the Mono client). Milestones: CLI text client →
  minimal web board (JSON) → protocol-compatible server (recovered protobuf) → real-client
  drop-in. Reconciled the DecisionRequest⇄GRE mapping to `AGENT_INTERFACE.md` §6.1; the docs
  now cross-reference (design added §1.1 GRE-server serialization contract).
- **client (follow-up):** decompile #2 landed → folded the **recovered + log-validated GRE
  transport + schema** into CLIENT_PLAN §4/§5/§8 (no longer assumptions): wire = TLS 1.2 over
  TCP, custom **6-byte frame** `[ver=4][type|format][int32 LE len]` inside the TLS stream +
  ping/pong keepalive; envelope = `IMessageEnvelope{Protobuf|Json, Compressed, TransId}` w/
  protobuf payload as `Any`; **endpoint is dynamic** (match push `MatchInfoV3.MatchEndpointHost/
  Port`+`MatchId`); GRE `ConnectReq` is **tokenless** (auth binds upstream). Net: real-client
  drop-in **de-risked** — no GRE token to forge, TLS solvable via controlling the pushed
  hostname + local dev-CA (no pinning bypass). Mapping table updated to exact recovered resp
  names; sent transport facts to decompile for their #6.
- **design:** wrote `docs/design/AGENT_INTERFACE.md` — the single `Agent` trait +
  `DecisionRequest`/`DecisionResponse` enums + `PlayerView` (info-filtered, hidden zones
  masked) + the Effect IR / whiteboard `Action` / `Native` hatch (Rust sketches). The
  `DecisionRequest` set is a proven **superset** of the recovered MTGA GRE `*Req` catalog
  (coverage matrices in §6). Masking is the
  engine's job. Asked `decompile` for field-level GRE Req/Resp shapes (§9 open questions);
  variant set not expected to change. Task #4 (implement agent.rs + effects/) blocked on
  the workspace scaffold (#1).
- **Project bootstrapped into a planned project.** Established docs, the architecture, and
  the implementation plans.
- Downloaded the MTG Comprehensive Rules (eff. 2026-02-27) → `docs/rules/`
  (`MagicCompRules_20260227.pdf` + extracted `comprules.txt`).
- Wrote `docs/rules/RULES_SUMMARY.md` — engine-implementer's map of the CR (layers, SBAs,
  priority/stack, combat, replacement/triggers, keyword index), with rule numbers.
- **Architecture decided: the MTGA "whiteboard" model** (per WotC dev diaries) →
  `docs/design/WHITEBOARD_MODEL.md`. Card-agnostic core + declarative effect rules that
  rewrite a pending-actions whiteboard; agenda pipeline; qualifications; layers; LKI.
- Wrote `docs/plans/ENGINE_PLAN.md` (Rust workspace, milestones, agent boundary, testing
  CR-derived expect tests + MTGA logs), `docs/plans/GYM_PLAN.md` (PyO3+maturin, action masking,
  self-play), `docs/plans/DECOMPILE_PLAN.md` (MTGA protocol recovery).
- Recon: **MTGA is a Mono build** (not IL2CPP), Steam install, **protobuf** GRE protocol
  (`Wizards.MDN.GreProtobuf.dll`). Decompile is the easy path; work to live in `../mtga-re`.
- Wrote `CLAUDE.md` (orientation + conventions) and these trackers. Initialized git history.
