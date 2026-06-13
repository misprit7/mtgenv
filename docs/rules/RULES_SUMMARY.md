# MTG Comprehensive Rules — Engine-Implementer's Map

Working reference for agents building the Rust rules engine in `mtgenv`. Citations like `(509.2)` point into `docs/rules/comprules.txt` (rules effective 2026-02-27). This is a structural map, not a how-to-play guide. Scope target: **two-player, Arena-style** game (`100.1a`). Multiplayer (`800s`), variants (`900s`), and most casual-only card types are out of initial scope (see §13).

> **Subrule lettering quirk:** subrules skip `l` and `o` (confusable with `1`/`0`). Sequence is `…k, m, n, p, q…`. E.g. `704.5k → 704.5m → 704.5n → 704.5p`. Do not assume `a–z` is contiguous when parsing rule numbers.

---

## 1. The 9 Parts (orientation)

| Part | Range | Governs |
|------|-------|---------|
| 1. Game Concepts | 100–123 | Players, colors, mana, objects, permanents, tokens, spells, abilities, targets, special actions, **timing & priority (117)**, costs (118), life, damage, counters |
| 2. Parts of a Card | 200–213 | Name, mana cost, type line (205), P/T, loyalty, defense — the printed fields |
| 3. Card Types | 300–315 | Per-type rules: artifact, creature, enchantment, instant, land, planeswalker, sorcery, battle (310), kindred, + casual types |
| 4. Zones | 400–408 | The 7 zones, public/hidden, ownership, **new-object rule (400.7)** |
| 5. Turn Structure | 500–514 | Phases, steps, turn-based actions, priority handoff |
| 6. Spells, Abilities, Effects | 600–616 | **Casting (601), activating (602), triggers (603), static (604), mana abilities (605), resolving (608), continuous effects & the layer system (611–613), replacement (614), prevention (615), interaction (616)** |
| 7. Additional Rules | 700–732 | Modes, keyword actions (701), keyword abilities (702), **turn-based actions (703), state-based actions (704)**, copying (707), DFCs, controlling players, illegal actions (732) |
| 8. Multiplayer | 800–811 | Options & variants for 3+ players. **Defer.** |
| 9. Casual Variants | 900–905 | Planechase, Vanguard, Commander, Archenemy, Conspiracy. **Defer.** |

The two highest-leverage parts for the engine core are **Part 6** (the stack + continuous effects) and **Part 7's 703/704** (the automatic game machinery).

---

## 2. Core Objects & State

### 2.1 Object taxonomy (109.1)
An **object** is exactly one of: an ability on the stack, a card, a copy of a card, a token, a spell, a permanent, or an emblem. Model `Object` as a tagged union. The same physical card is a *different object* in each zone (see new-object rule).

- **Card** (108): a Magic card or object represented by one. Tokens are **not** cards (108.2b, 111.6).
- **Permanent** (110.1, 403.3): a card **or token on the battlefield**. The permanent *is* the battlefield object, not the card; it references its source. Don't conflate `Card` and `Permanent`. Permanents exist only on the battlefield.
- **Spell** (112.1): a card (or copy) **on the stack**. The card *becomes* a spell as step 1 of casting. A copy is a spell with no card (112.1a).
- **Ability on the stack** (113.1c): an activated/triggered ability that is itself an object; has only the text of its source ability, no other characteristics (405.4, 602.2a, 603.3).
- **Token** (111): represents a permanent not backed by a card. **Emblem** (114): lives in command zone, has only abilities, no other characteristics.

Six permanent types (110.4): **artifact, battle, creature, enchantment, land, planeswalker**. Instants/sorceries can never be permanents (400.4a). "Permanent card" can enter the battlefield; "permanent spell" enters on resolution — **lands are not cast**, so not permanent spells (110.4a/b).

### 2.2 Characteristics — closed list (109.3)
Exactly: **name, mana cost, color, color indicator, card type, subtype, supertype, rules text, abilities, power, toughness, loyalty, defense, hand modifier, life modifier.** Copy effects (707) and the layer system operate on *these*. **NOT characteristics:** tapped status, a spell's targets, owner/controller, what an Aura enchants (109.3). Tokens/copies have only the parts that are characteristics (200.3).

### 2.3 Status — battlefield-only, not a characteristic (110.5)
Four independent booleans, present on every permanent: **tapped/untapped, flipped/unflipped, face-up/face-down, phased-in/phased-out**. Defaults on entry: untapped, unflipped, face up, phased in (110.5b). Status doesn't exist off the battlefield (110.5d). Model as a 4-field struct on `Permanent` only.

### 2.4 Ownership vs. control
- **Owner** (108.3): the player who started with the card in their deck (or brought it in). Permanent property of the card.
- **Controller**: only objects **on the stack or battlefield** have a controller (109.4). Off-stack/off-battlefield, use owner if controller is needed (108.4a). Make controller a nullable field.
- Spell controller = caster (or putter-on-stack for copies) (112.2). Permanent controller defaults to whoever it entered under (110.2). Activated-ability controller = activator; triggered-ability controller = controller of source when it triggered (113.8, 603.3a). Six exceptions where off-zone objects do have a controller (109.4a–g) — mostly variant cards + mana abilities + waiting triggers + emblems.

### 2.5 Abilities (113) — dispatch categories
Four functional kinds (113.3): **spell ability** (instructions on a resolving instant/sorcery), **activated** (`Cost: Effect`, uses stack), **triggered** (`When/Whenever/At …`, uses stack), **static** (continuously true; generates continuous effects). Special subtypes: **mana abilities** (don't use stack, 605) and **loyalty abilities** (sorcery-speed, once/turn, 606). Critical concept: **zone of function** (113.6) — default is instant/sorcery abilities function on the stack, all others on the battlefield, with many exceptions (CDAs function everywhere (113.6a/604.3); cost-modifiers on the stack; ETB-modifiers as the object enters; etc.). The engine must gate every ability by "is it active in its current zone?".

### 2.6 Counters (122), tokens (111)
- A counter is a marker (122.1): **not an object, not a token, no characteristics.** Same-name counters are fungible. Model as a multiset `name → count` on object/player.
- `+X/+Y` counters modify P/T even off-battlefield (122.1a). **Keyword counters** grant a fixed allowed keyword set (122.1b). Special built-in counters: **shield** (replace destroy/damage, 122.1c), **stun** (replace untap, 122.1d), **loyalty** (209/122.1e), **poison** (10+ = lose, 122.1f), **defense** (210/122.1g), **finality** (gy→exile, 122.1h), **rad** (122.1i).
- **Counters cease to exist on any zone change** (122.2, 400.7) — they are not "removed," so remove-triggers don't fire.
- Token created with full characteristics from its creator (111.3); no characteristic the creator didn't define. **SBA: a token not on the battlefield ceases to exist** (704.5d), but leave-the-battlefield triggers fire first (111.7). A copy of a permanent spell becomes a token but is **not "created"** (608.3f, 111.13).

### 2.7 Life (119) & Damage (120) — model as a pipeline
- Damage only to **battles, creatures, planeswalkers, players** (120.1). 0 damage = no event at all (120.8). Damage doesn't destroy; it's **marked**, then SBAs may destroy (120.5/6, 704.5g).
- Damage results depend on source/recipient (120.3): player → lose life (or poison w/ infect/toxic); planeswalker → remove loyalty; creature → marked damage (or −1/−1 counters w/ wither/infect); lifelink → controller also gains that much; battle → remove defense. Four-step damage sequence (120.4): redirection → deal (prevention/replacement apply, triggers wait) → process into results (result-doublers apply) → event occurs.

---

## 3. Turn Structure (500s)

Five phases per turn, always, even if empty (500.1): **beginning, precombat main, combat, postcombat main, ending.** A priority-having step/phase ends only when **stack empty AND all players pass in succession** (500.2). Untap and (usually) cleanup give no priority and end when their actions finish (500.3).

| Phase | Step | Turn-based actions (happen before priority) | Priority? |
|-------|------|---------------------------------------------|-----------|
| Beginning | **Untap (502)** | (1) phasing (502.1), (2) day/night check (502.2), (3) untap permanents (502.3) | **No** (502.4) |
| Beginning | **Upkeep (503)** | none | Active player (AP) |
| Beginning | **Draw (504)** | AP draws a card (504.1) — *skipped turn-1 for player on the play in 2-player (103.8a)* | AP |
| Precombat Main (505) | — | Saga lore counters (505.4); (Attractions roll 505.5) | AP. Only phase for sorcery-speed casts; land play here (505.6a/b) |
| Combat (506) | **Begin combat (507)** | (multiplayer: choose defender) | AP |
| Combat | **Declare Attackers (508)** | AP declares attackers (508.1) | AP |
| Combat | **Declare Blockers (509)** | Defender declares blockers (509.1) | AP |
| Combat | **Combat Damage (510)** | assign (510.1) then deal simultaneously (510.2); **2 steps if any first/double strike** (510.4) | AP |
| Combat | **End of Combat (511)** | none; "until end of combat" expires at phase end (511.2) | AP |
| Postcombat Main (505) | — | (same as precombat but is "postcombat") | AP |
| Ending (512) | **End (513)** | none | AP |
| Ending (512) | **Cleanup (514)** | (1) discard to max hand size (514.1), (2) remove all marked damage + end "until end of turn"/"this turn" effects, simultaneously (514.2) | **Normally no** (514.3); but if SBAs pending or triggers waiting, do them and give priority, then repeat cleanup (514.3a) |

Key sequencing rules:
- **At step/phase begin:** turn-based actions first (703.3) → SBAs → put triggered abilities on stack → AP gets priority (117.3a, 117.5).
- "At the beginning of [step/phase]" abilities are **triggered**, not turn-based (703.1a, 500.6).
- Mana pools empty as each step/phase ends (500.5, 703.4q).
- Extra turns/phases/steps insert immediately after the named one; **most-recently-created goes first** (500.7–500.10).
- "Skip" = a replacement effect; can't skip something already started (614.10).
- No game events occur between steps/phases/turns (500.12).

---

## 4. Priority, the Stack, and Casting (the agent interface)

This is the engine's central decision loop. Get it exactly right.

### 4.1 The priority loop (117)
- Player with priority may: cast a spell, activate an ability, take a special action (117.1). Instants any time with priority; sorcery-speed only in own main phase with empty stack (117.1a).
- After a player **does** something, they retain priority (117.3c) ("holding priority"). After a spell/ability resolves, AP gets priority (117.3b).
- If a player **passes**, next player in turn order gets priority (117.3d). **If all players pass in succession:** top of stack resolves, or if stack empty, the step/phase ends (117.4, 500.2).
- **Before any player would receive priority** (117.5, the SBA/trigger loop): perform all SBAs as a single event, repeat until none; then put waiting triggered abilities on the stack; repeat the whole thing until no SBAs and no new triggers; *then* hand priority to the player.
- Mana abilities are special: activatable whenever you have priority OR are paying a cost OR a rule/effect asks for mana (117.1d, 605.3a) — even mid-cast/mid-resolution.

### 4.2 The stack (405)
- LIFO. New objects go on top (405.2); resolution takes the top (405.5). "In response" = casting/activating while something is on the stack; the response resolves first (117.7).
- Mana abilities, special actions, turn-based actions, SBAs, static abilities, and effects **do not use the stack** (405.6).
- Simultaneous additions: AP's objects first, then APNAP order; each player orders their own (405.3).

### 4.3 Casting a spell — the 601.2 steps (in order; abort + rewind on failure, 732)
1. **601.2a Move to stack.** Card → top of stack, becomes a spell; caster becomes controller. Cast-time characteristic-modifying continuous effects begin now (611.2f); gain-ability-as-cast one-shots apply now (610.5).
2. **601.2b Announce choices:** modes (700.2), splice reveals, intent to pay alternative/additional/optional costs (kicker, buyback…), **value of X**, hybrid/Phyrexian payment choices. At most one alternative cost and one alternative casting method per spell.
3. **601.2c Choose targets** (115, 608). One per instance of "target"; same object may satisfy multiple distinct "target" words. Number of targets locks now. Targeting triggers trigger (wait until cast finishes).
4. **601.2d Divide/distribute** damage/counters among targets (each gets ≥1). **Division is locked at cast time** and can't be changed by retargeting (115.7f).
5. **601.2e Legality check.** Illegal → rewind.
6. **601.2f Determine total cost** = (mana cost or alt cost) + additional costs + increases − reductions. Cost reductions in any order; mana component floor is `{0}`. **Cost locks in** here; later cost changes ignored.
7. **601.2g Activate mana abilities** (must precede payment).
8. **601.2h Pay total cost.** Non-random/non-library-reveal costs first, then the rest. No partial payments; unpayable costs can't be paid.
9. **601.2i Spell becomes cast.** Cast-modifying effects apply; "when you cast" triggers trigger. If caster had priority, they get it again.

### 4.4 Activating an ability (602)
Form `[Cost]: [Effect] [Activation instructions]` (602.1). Step 602.2a: ability put on stack as a non-card object (revealed if from a hidden zone). Rest mirrors **601.2b–i** (602.2b). Only controller (or owner if none) may activate, unless stated (602.2). `{T}`/`{Q}` abilities of creatures need control since start of your most recent turn unless haste (602.5a, summoning sickness). "Activate only as a sorcery/instant" = timing restriction only (602.5d/e).

### 4.5 Mana abilities (605) — resolve instantly, no stack
A mana ability (605.1a/b): no target, could add mana, not a loyalty ability (activated form); or a triggered ability triggering off mana-ability activation/mana being added that could add mana. **Doesn't go on the stack** — can't be targeted/countered/responded to; resolves immediately (605.3b/605.4a). Still a mana ability even if it currently can't produce mana (605.2). This is why the engine must let mana production happen inside the cost-payment substep, not as a separate stack object.

### 4.6 Special actions (116) — no stack, no response
12 special actions (116.2). The common ones for a 1v1 engine: **play a land** (116.2a — main phase, own turn, empty stack, priority, once/turn by default), **turn a face-down creature face up** (116.2b), ending a continuous effect / stopping a delayed trigger (116.2c). Player gets priority afterward (116.3).

### 4.7 Resolving (608)
1. 608.2a intervening-"if" recheck (see §5.4); 608.2b **target legality recheck** (see §6).
2. Follow instructions in order (608.2c); apply replacements.
3. Make resolution-time choices (608.2d); APNAP for multi-player actions (608.2e/f).
4. **Information is read once, at application time** (608.2h); missing source → LKI (608.2h, 113.7a). Look-back effects are the exception (608.2i).
5. 608.2n: instant/sorcery → owner's graveyard; ability → ceases to exist. 608.2p: "when this resolves" triggers fire.
- Permanent spells (608.3): no targets → enter battlefield; Aura → attach to its target; can't enter → owner's graveyard (608.3e); copy of a permanent spell → becomes a token (608.3f).

---

## 5. Hard Subsystems

### 5.1 State-Based Actions (704)
Checked **every time a player would get priority** (and at cleanup), performed simultaneously as one event, repeated until none apply, *then* triggers go on the stack (704.3, 117.5). Not controlled by any player; ignore mid-resolution game states (704.4 — the Maro example). Multiple SBAs with the same result share one replacement effect (704.7). LKI for SBA-driven departures comes from the pre-SBA state (704.8 — the undying/-1-1 example).

| SBA | Rule | Result |
|-----|------|--------|
| Player at ≤0 life | 704.5a | loses game |
| Drew from empty library since last check | 704.5b | loses game |
| 10+ poison counters | 704.5c | loses game |
| Token outside battlefield | 704.5d | ceases to exist |
| Copy of spell off-stack / copy of card off-stack-and-off-battlefield | 704.5e | ceases to exist |
| Creature toughness ≤0 | 704.5f | to graveyard (regeneration can't save) |
| Creature lethal marked damage (≥ toughness >0) | 704.5g | destroyed (regen can save) |
| Creature dealt deathtouch damage (toughness >0) | 704.5h | destroyed (regen can save) |
| Planeswalker loyalty 0 | 704.5i | to graveyard |
| ≥2 same-name legendary permanents, one controller | 704.5j | **legend rule** — keep one |
| ≥2 `world` permanents | 704.5k | **world rule** — keep newest |
| Aura attached illegally / unattached | 704.5m | to graveyard |
| Equipment/Fortification attached illegally or to player | 704.5n | becomes unattached, stays |
| Creature/battle/other non-Aura attached | 704.5p | becomes unattached, stays |
| Both +1/+1 and −1/−1 counters on a permanent | 704.5q | remove N of each (annihilate pairs) |
| More than allowed-max counters of a kind | 704.5r | remove excess |
| Saga lore ≥ final chapter (not source of pending chapter) | 704.5s | sacrifice |
| Battle defense 0 (not source of pending ability) | 704.5v | to graveyard |
| Battle with no valid protector | 704.5w/x | choose protector or to graveyard |
| ≥2 same-controller Roles on one permanent | 704.5y | keep newest, rest to graveyard |
| `start your engines!` controller has no speed | 704.5z | speed becomes 1 |

(704.5t dungeon, 704.5u space sculptor are niche; 704.6 are variant-only — defer.)

### 5.2 The Layer System (613) — the hardest part
Characteristics are computed from scratch every time they're queried (613.5, instantaneous). Start from the **actual object** (printed values, or token/copy-defined values), then apply all applicable continuous effects in **7 layers**:

| Layer | Rule | Applies |
|-------|------|---------|
| **1** Copy | 613.1a | Copy effects + merge (729). Has sublayers (below). |
| **2** Control | 613.1b | Control-changing effects |
| **3** Text | 613.1c | Text-changing effects (612) |
| **4** Type | 613.1d | Card type / subtype / supertype changes |
| **5** Color | 613.1e | Color changes |
| **6** Ability | 613.1f | Ability add/remove, keyword counters, "can't have" ability |
| **7** P/T | 613.1g | Power/toughness changes (has sublayers) |

**Layer 1 sublayers** (613.2), each applied in timestamp order:
- 1a copy effects (incl. "as enters"/"as turned face up" that set P/T) (613.2a);
- 1b face-down modifications (708.2) (613.2b).
- After layer 1, the object's characteristics are its **copiable values** (613.2c, 707.2).

**Layer 7 sublayers** (613.4), each in timestamp order:
- 7a CDA-defined P/T (604.3);
- 7b **set** P/T to a specific value (and "base P/T" references);
- 7c **modify** P/T (counters and `+N/+N`/`-N/-N` effects);
- 7d **switch** P/T.

**Ordering within a layer/sublayer:**
1. In layers 2–6: CDAs first, then other effects in timestamp order (613.3).
2. **Timestamps** (613.7): object → when it enters a zone (613.7d); static-ability effect → object's timestamp or the granting effect's, whichever is later (613.7a); resolution effect → when created (613.7b); counter → when placed (613.7c); Aura/Equipment → re-timestamped on each attach (613.7e); permanent re-timestamped on flip (613.7f) / transform (613.7g). Simultaneous → APNAP (613.7m).
3. **Dependency** (613.8) overrides timestamp: effect A *depends on* B if (same layer/sublayer) AND applying B would change A's existence/text/what-it-applies-to/what-it-does, AND not exactly one of them is a CDA. Dependent effects wait until their dependencies are applied; dependency **loops** fall back to timestamp order (613.8b). Re-evaluate dependencies after each application (613.8c).

Gotchas: an effect spanning multiple layers applies each part in its own layer to the *same set of objects*, even if the source is gone or the object no longer matches by then (613.6 — the "becomes 2/2 artifact creature" examples). Setting a characteristic ("is white"/"becomes 0/1") is not granting/removing an ability — can't be stripped in layer 6 (113.12). Player-affecting and rules-affecting continuous effects apply after object characteristics, in timestamp order (613.10/613.11). Devotion is computed after layers 1–3 only (700.5a, exception to 613.10).

### 5.3 Replacement (614) & Prevention (615) effects
Both are continuous "shields" that watch for an event and modify/prevent it **before it happens** — they can't act retroactively (614.4, 615.4). Replacement markers: **"instead"** (614.1a), **"skip"** (614.1b), **"[permanent] enters …" / "as enters" / "enters with/as"** (614.1c/d/e). Prevention marker: **"prevent"** (615.1a).

- A replacement effect applies to a given event **at most once** (614.5); a replaced event never happens, a modified event occurs instead and may trigger things (614.6). If the event never happens, the effect does nothing (614.7); 0-damage events are non-events (614.7a/120.8).
- **Regeneration** is an implicit destruction-replacement (614.8): "next time it would be destroyed this turn, instead remove damage, tap it, remove from combat." Can't replace toughness-0 death (704.5f).
- **Self-replacement effects** (614.15) replace part of a resolving spell/ability's own effect; applied **before** other replacements (616.1a).
- **"Can't" effects** (614.17) aren't replacements but follow similar timing; an impossible event can only be replaced by a self-replacement (614.17c); you can't pay a cost that includes an impossible event (614.17b).
- **ETB-modifying replacements** (614.12): to decide which apply, evaluate the permanent's characteristics *as it would exist on the battlefield*, including its own static abilities and the already-applied ETB replacements. Choices made before it enters (614.12a). This is where "enters tapped," "enters with counters," copy-as-it-enters, and anchor-word choices (614.12c) live.
- **Interaction (616):** when multiple replacement/prevention effects would modify one event, the affected object's controller (or owner / affected player) picks one to apply, then re-checks (616.1f). Forced ordering: self-replacement → control-on-enter → copy-on-enter → enter-with-back-face-up → free choice (616.1a–e). An effect can become applicable *because of* another (616.2); inner events are chosen after outer (616.1g — Doubling Season before Voice of All).

### 5.4 Triggered abilities (603)
- Trigger automatically when game state/event matches (603.2); nothing happens at trigger time. **Put on the stack the next time a player would get priority** (117.5, 603.3). Can trigger even when casting/activating is illegal (603.2a).
- **Intervening "if"** (603.4): `When [event], if [condition], [effect]` — checks condition at trigger time (won't trigger if false) AND again on resolution (removed if false). Only applies to an "if" immediately after the trigger condition.
- Ordering on the stack (603.3b): APNAP; first the non-"ability-triggering" triggers (each player orders own), then remaining; recheck SBAs/triggers; repeat.
- `becomes` triggers fire only on the transition, not for pre-existing/entering-in-that-state (603.2e). A trigger fires once per occurrence (603.2c); one event can contain multiple occurrences. Optional ("may")/"unless" triggers still go on the stack; the choice is at resolution (603.5).
- **Zone-change triggers** (603.6): on resolution they look for the object **in the zone it moved to** (603.6); ETB (603.6a) / LTB (603.6c) are the common cases. "Enters with"/"as enters"/"enters tapped" are **static**, not triggered (603.6d). LTB abilities + abilities finding the object in a public destination zone can find the new object via the 400.7 exceptions.
- **Looking back in time** (603.10): some triggers evaluate the game state *immediately before* the event — LTB abilities, leaves-graveyard, public-zone-to-hand/library, phase-out, becomes-unattached, lose-control, spell-countered, loses-the-game, planeswalk-away. (E.g., a "whenever a creature dies, gain life" artifact destroyed simultaneously with creatures still sees them.)
- **State triggers** (603.8): trigger as soon as the game state matches a condition (not an event), re-trigger only after leaving the stack. Distinct from SBAs.
- **Delayed** (603.7) and **reflexive** (603.12) triggers: created during resolution/replacement; delayed triggers wait for the next occurrence of their event (or a stated duration); reflexive triggers check immediately whether the event already happened during the current resolution. Source/controller determined by 603.7d–g.

### 5.5 Combat (506–511) in detail
- Only creatures attack/block; only players, planeswalkers, battles are attacked (506.3). Active player attacks; nonactive player defends (506.2, 2-player).
- **Declare attackers (508.1, turn-based, no stack):** choose untapped, non-battle creatures controlled since turn began (or with haste) (508.1a); announce what each attacks (508.1b); check restrictions (can't-attack) (508.1c) and **requirements** (must-attack: obey the maximum possible set) (508.1d); declare bands; **tap attackers (not a cost)** (508.1f); choose/pay attack costs, total locked in (508.1g–j); creatures become attacking (508.1k); attack triggers trigger (508.1m). Illegal declaration → full rewind (732).
- **Declare blockers (509.1, turn-based, no stack):** choose untapped non-battle creatures, assign each to an attacker attacking that player/their planeswalker/their battle (509.1a); check block restrictions incl. **evasion abilities** (flying/menace/etc., cumulative; gained/lost after declaration doesn't unblock) (509.1b); check block requirements (509.1c); pay block costs (509.1d–f); become blocking; attackers with ≥1 blocker become **blocked** (stay blocked even if blockers leave) (509.1h); block triggers (509.1i).
- **Combat damage (510):** each creature assigns combat damage = its power (510.1a). Unblocked → to what it's attacking (510.1b). Blocked → among its blockers, controller's choice of division (510.1c); a blocked creature with all blockers gone assigns **no** damage (unless trample). Blocker → among creatures it blocks (510.1d). Total assignment validated together (510.1e). Then **all combat damage dealt simultaneously, no priority between assign and deal** (510.2).
- **Damage assignment order / lethal:** for trample and multi-block, a creature must assign at least lethal (toughness − marked damage, deathtouch counts any 1 as lethal) to each blocker before excess; **trample** excess goes to the defending player/planeswalker/battle (702.19). (The assignment-order ritual lives mostly in 509/510 + keyword rules.)
- **First strike / double strike (510.4):** if any combatant has first or double strike when the damage step begins, there are **two** combat damage steps. Step 1: first-strikers + double-strikers. Step 2: everyone else + double-strikers again. Removing/gaining first strike between steps matters.
- **Removed from combat (506.4):** leaving the battlefield, changing controller, phasing out, regenerating, ceasing to be a creature, or explicit removal. A creature whose attacked planeswalker/battle leaves stays an attacker but deals no damage if unblocked (506.4c).
- Multiple combat phases possible via extra-phase effects (505.1a, 506.7). Creatures "put onto the battlefield attacking/blocking" are attacking/blocking but never "attacked"/"blocked" for trigger purposes (508.4, 509.4).

### 5.6 Targeting & legality (115, 608.2b)
- Targets chosen at cast/activation/trigger-on-stack time (115.1, 601.2c); can't change except by effects that say so (115.7). Only permanents are legal targets unless the spell says otherwise or targets a non-battlefield object/player (115.2). A spell/ability is an illegal target for itself (115.5). "Any target" = creature/player/planeswalker/battle (115.4).
- **On resolution (608.2b):** recheck every target. A target no longer in its expected zone is illegal; characteristic changes can also invalidate. **If ALL targets (every "target" instance) are illegal → spell/ability doesn't resolve** (countered by game rules; spell to graveyard). If *some* are legal, it resolves but does nothing to the illegal ones (Plague Spores example). Use LKI if the source has left its zone (608.2b).
- Retargeting rules: "change the target(s)" all-or-nothing to other legal targets (115.7a); "change a target" one (115.7b); "choose new targets" may leave illegal ones (115.7d). Only final target set is checked (115.7e).

### 5.7 Copying (707) & CDAs (604.3)
- A copy takes the **copiable values** of the original: name, mana cost, color indicator, card types/subtypes/supertypes, rules text, P/T, loyalty — *as modified by other copy effects, face-down status, and "as enters/as turned face up" P/T-setting abilities* (707.2). **Not copied:** other continuous effects, type/text-changing effects, status, counters, stickers.
- Copying happens **as the object enters** (707.5) — ETB replacements and triggers of the copied text apply. "As enters" choices are re-made by the copier (707.6). Copy effects may add abilities/modify characteristics that become part of the copiable values (707.9a/b), or state "except it doesn't copy X" (707.9c) — which also suppresses the copied object's CDA for that characteristic (707.9d). For DFCs, copy the currently-up face (707.8); a token copy of a DFC is itself double-faced (707.8a).
- **Characteristic-defining abilities** (604.3): static abilities that define an object's color, subtype, power, or toughness; printed/granted-on-creation/copy-acquired; don't affect other objects; not self-granted; not conditional. **Function in all zones, outside the game, before the game** (604.3, 113.6a). Applied in layer 1a (for P/T-setting "as enters") and layers 7a / first-in-2–6.

### 5.8 Costs (118) & mana
- A cost (118.1) is paid by carrying out its instructions; must be paid fully (118.3); 0 mana/0 life always payable (118.3a/b); activating mana abilities is optional even when paying is mandatory (118.3c).
- **Additional costs** (118.8): paid alongside mana cost; any number may apply; don't change the spell's mana cost (118.8d). **Alternative costs** (118.9): replace the mana cost; **at most one** per spell (118.9a); additional costs & cost mods still apply to the alternative (118.9d). Cost reductions: generic-only reductions hit only generic; over-reduction of colored/colorless spills to generic (118.7a–g).
- Unpayable cost (no mana cost) (118.6): you may *attempt* to cast, but can't pay — unless an alternative cost applies (118.6a).
- "Do X. If you do/don't/can't, …" (118.12): the action is a cost paid on resolution; the clause checks whether the player chose/started to pay, not the actual outcome.
- Mana abilities (605, §4.5) produce mana into the pool; pool empties each step/phase (500.5). `X` is chosen at 601.2b and is fixed thereafter (107.3).

---

## 6. What Makes MTG Hard to Implement

A candid list of where engines bleed:

1. **The layer system (613)** — the single hardest piece. 7 layers, sublayers in 1 and 7, timestamp ordering, CDA precedence, and the **dependency system** (613.8) that can override timestamps and form loops. Recompute characteristics continuously, on demand. Effects span multiple layers and apply to a fixed object set even after the source/match disappears (613.6).
2. **Replacement-effect interaction (616)** — player choice among applicable shields, re-checking after each, forced orderings, self-replacement-first, and inner-vs-outer event nesting (616.1g). Combined with ETB replacements (614.12) that depend on as-it-would-exist characteristics, this is a fixpoint computation.
3. **Last Known Information (LKI)** — when a source/object leaves its zone, abilities/SBAs/targeting use its last state (113.7a, 608.2b, 608.2h, 704.8, glossary). Requires snapshotting object state at the right moments.
4. **Looking back in time (603.10)** — certain triggers evaluate pre-event state. The engine can't just check post-event game state for all triggers.
5. **Copiable values (707.2)** vs. the live characteristics — copy effects read a *specific* slice (layer-1-output), not the final computed values; and copying interacts with face-down, DFCs, CDAs, and "as enters."
6. **Linked abilities (607)** — second ability refers only to what the first did; survives copying/granting with the linkage intact (607.5/707.7); an ability can be in multiple link pairs (607.4); "undefined" choices when only half is copied (607.5a).
7. **Timestamps & dependency** — see (1). Re-timestamping on attach/flip/transform (613.7e–g) makes order dynamic.
8. **Simultaneous events & APNAP** (101.4) — choices ordered AP-then-others, then actions happen at once; APNAP restarts if a later choice forces an earlier player to choose (101.4d). Damage, dies, zone changes, token creation can all be simultaneous (610.3d, 603.10a).
9. **The stack's special cases** — **split second (702.61)** forbids casting spells / activating non-mana abilities while it's on the stack; **can't be countered** (113.6g); mana abilities and special actions bypass the stack entirely (405.6); holding priority.
10. **The new-object rule (400.7)** — every zone change mints a fresh identity, dropping counters/damage/effects/targeting, *except* an enumerated list of carry-overs (400.7a–m). The most common source of "ghost" bugs.
11. **State-based vs. triggered vs. turn-based** — three different automatic systems with different timing (704 ignores mid-resolution; 603 watches events; 703 fires on step/phase boundaries). Mixing them up breaks the priority loop.
12. **Combat damage assignment** — lethal-damage ordering, trample excess, deathtouch-as-lethal, multi-block division, first/double-strike's two steps, and "removed from combat" edge cases (506.4).
13. **Intervening-if double check** (603.4) and **target re-legality** (608.2b) — conditions/targets checked at two distinct times.
14. **Continuous-effect set-fixing** — resolution effects fix their affected set at creation (611.2c); static abilities re-evaluate live (611.3a). Same wording, opposite behavior.

---

## 7. Keyword Actions (701) & Keyword Abilities (702) Index

Surface-area maps so implementers know what they're signing up for. Prioritize **Evergreen**, then **Common**; **Niche** can be stubbed/deferred for a 1v1 Arena-style engine.

### 7.1 Keyword Actions (701)
| Action | Rule | Semantics |
|---|---|---|
| Activate | 701.2 | Put an activated ability on the stack and pay its cost |
| Attach / Unattach | 701.3 | Move an Aura/Equipment/Fortification onto a legal object/player (or remove) |
| Cast | 701.5 | Move a spell to the stack and pay its costs |
| Counter | 701.6 | Cancel a stack object; countered spell → owner's graveyard |
| Create | 701.7 | Put token(s) with given characteristics onto the battlefield |
| Destroy | 701.8 | Move a permanent to its owner's graveyard |
| Discard | 701.9 | Move a card from hand to graveyard |
| Double / Triple | 701.10/.11 | Add power/toughness, or double/triple life/counters/mana/damage |
| Exchange | 701.12 | Swap two things; aborts if it can't fully complete |
| Exile | 701.13 | Move an object to exile |
| Fight | 701.14 | Two creatures deal damage equal to power to each other (non-combat) |
| Goad | 701.15 | Force a creature to attack a player other than the goader |
| Investigate | 701.16 | Create a Clue token |
| Mill | 701.17 | Top N cards of library → graveyard |
| Play | 701.18 | Play a land or cast a card, whichever applies |
| Regenerate | 701.19 | Destruction-replacement shield (see 614.8) |
| Reveal | 701.20 | Show a card to all players |
| Sacrifice | 701.21 | Controller moves own permanent to owner's graveyard (not a destruction) |
| Scry | 701.22 | Look at top N, bottom any, rest on top in any order |
| Search | 701.23 | Look through a zone for matching cards |
| Shuffle | 701.24 | Randomize a library/face-down pile |
| Surveil | 701.25 | Look at top N, any to graveyard, rest on top |
| Tap / Untap | 701.26 | Turn a permanent sideways / upright |
| Transform / Convert | 701.27/.28 | Turn a DFC permanent to its other face |
| Fateseal | 701.29 | Like scry but on an opponent's library |
| Clash | 701.30 | Reveal top card; higher mana value wins |
| Proliferate | 701.34 | Add one more of each existing counter kind to chosen permanents/players |
| Detain | 701.35 | Target can't attack/block/activate until your next turn |
| Populate | 701.36 | Create a token copy of a creature token you control |
| Monstrosity | 701.37 | If not monstrous, add N +1/+1 counters, become monstrous |
| Vote | 701.38 | Each player chooses an option |
| Bolster / Support | 701.39/.41 | Put +1/+1 counters (on least-toughness / on up-to-N targets) |
| Manifest / Manifest Dread | 701.40/.62 | Put a card face down as a 2/2; (dread: choose 1 of top 2) |
| Meld | 701.42 | Combine a meld pair into one back-up permanent |
| Exert | 701.43 | Choose not to untap next untap step |
| Explore | 701.44 | Reveal top: land → hand; else +1/+1 counter (may mill) |
| Adapt | 701.46 | If no +1/+1 counters, add N |
| Amass | 701.47 | Create/grow an Army token with +1/+1 counters |
| Learn | 701.48 | Discard-then-draw, or fetch a Lesson |
| Venture into the Dungeon | 701.49 | Enter/advance a dungeon |
| Connive | 701.50 | Draw then discard; +1/+1 counter if nonland discarded |
| Incubate | 701.53 | Create an Incubator token with N +1/+1 counters |
| The Ring Tempts You | 701.54 | Choose Ring-bearer; upgrade The Ring emblem |
| Face a Villainous Choice | 701.55 | Player picks one of two options |
| Time Travel | 701.56 | Add/remove time counters |
| Discover | 701.57 | Exile until a cheap nonland; cast free or to hand |
| Cloak | 701.58 | Face down as a 2/2 with ward {2} |
| Collect Evidence / Forage | 701.59/.61 | Exile cards from graveyard / exile 3 or sacrifice a Food |
| Suspect | 701.60 | Creature gains menace and can't block |
| Endure | 701.63 | Add N +1/+1 counters or make an N/N Spirit |
| (Niche/new: Harness 701.64, Airbend/Earthbend/Waterbend 701.65–67, Blight 701.68; Planechase/Archenemy actions 701.31–33 — **defer**) |

### 7.2 Keyword Abilities (702) — by tier
**Evergreen** (must support):
| Ability | Rule | Semantics |
|---|---|---|
| Deathtouch | 702.2 | Any nonzero damage is lethal (SBA 704.5h) |
| Defender | 702.3 | Can't attack |
| Double Strike | 702.4 | Deals damage in both combat damage steps |
| Enchant | 702.5 | Restricts what an Aura targets/attaches to |
| Equip | 702.6 | Sorcery-speed: attach Equipment to your creature |
| First Strike | 702.7 | Deals damage in the first combat damage step |
| Flash | 702.8 | Cast any time you could cast an instant |
| Flying | 702.9 | Blockable only by flying/reach |
| Haste | 702.10 | Ignore summoning sickness |
| Hexproof | 702.11 | Can't be targeted by opponents |
| Indestructible | 702.12 | Can't be destroyed; ignores lethal-damage SBA |
| Lifelink | 702.15 | Damage also gains controller that much life |
| Menace | 702.111 | Blockable only by 2+ creatures |
| Protection | 702.16 | Can't be targeted/blocked/enchanted/damaged by quality (DEBT) |
| Reach | 702.17 | Can block fliers |
| Trample | 702.19 | Excess combat damage to the player/planeswalker/battle |
| Vigilance | 702.20 | Doesn't tap to attack |
| Ward | 702.21 | Counters opponent's targeting unless they pay ward cost |

**Common / deciduous** (frequently reprinted; support soon):
Landwalk 702.14, Cycling 702.29, Kicker 702.33, Flashback 702.34, Madness 702.35, Morph 702.37, Affinity 702.41, Convoke 702.51, Dredge 702.52, Changeling 702.73, Evoke 702.74, Persist 702.79, Wither 702.80, Exalted 702.83, Cascade 702.85, Rebound 702.88, Infect 702.90, Undying 702.93, Overload 702.96, Evolve 702.100, Prowess 702.108, Dash 702.109, Exploit 702.110, Devoid 702.114, Escalate 702.120, Crew 702.122, Improvise 702.126, Ascend 702.131, Jump-Start 702.133, Mentor 702.134, Afterlife 702.135, Riot 702.136, Escape 702.138, Foretell 702.143, Disturb 702.146, Decayed 702.147, Training 702.149, Blitz 702.152, Casualty 702.153, Enlist 702.154, Toxic 702.164, Backup 702.165, Bargain 702.166, Disguise 702.168, Plot 702.170, Saddle 702.171, Spree 702.172, Gift 702.174, Offspring 702.175, Impending 702.176, Mobilize 702.181, Mayhem 702.187, Delve 702.66, Suspend 702.62, Bestow 702.103. (Semantics: see the per-rule entries; most are alternative/additional costs, ETB counter placements, or attack/block triggers.)

**Niche / set-specific** (stub or defer): Intimidate 702.13, Shroud 702.18, Banding 702.22, Rampage 702.23, Cumulative Upkeep 702.24, Flanking 702.25, Phasing 702.26, Buyback 702.27, Shadow 702.28, Echo 702.30, Horsemanship 702.31, Fading 702.32, Fear 702.36, Amplify 702.38, Provoke 702.39, Storm 702.40, Entwine 702.42, Modular 702.43, Sunburst 702.44, Bushido 702.45, Soulshift 702.46, Splice 702.47, Offering 702.48, Ninjutsu 702.49, Epic 702.50, Transmute 702.53, Bloodthirst 702.54, Haunt 702.55, Replicate 702.56, Forecast 702.57, Graft 702.58, Recover 702.59, Ripple 702.60, **Split Second 702.61**, Vanishing 702.63, Absorb 702.64, Aura Swap 702.65, Fortify 702.67, Frenzy 702.68, Gravestorm 702.69, Poisonous 702.70, Transfigure 702.71, Champion 702.72, Hideaway 702.75, Prowl 702.76, Reinforce 702.77, Conspire 702.78, Retrace 702.81, Devour 702.82, Unearth 702.84, Annihilator 702.86, Level Up 702.87, Umbra Armor 702.89, Battle Cry 702.91, Living Weapon 702.92, Miracle 702.94, Soulbond 702.95, Scavenge 702.97, Unleash 702.98, Cipher 702.99, Extort 702.101, Fuse 702.102, Tribute 702.104, Dethrone 702.105, Hidden Agenda 702.106, Outlast 702.107, Renown 702.112, Awaken 702.113, Ingest 702.115, Myriad 702.116, Surge 702.117, Skulk 702.118, Emerge 702.119, Melee 702.121, Fabricate 702.123, Partner 702.124, Undaunted 702.125, Aftermath 702.127, Embalm 702.128, Eternalize 702.129, Afflict 702.130, Assist 702.132, Spectacle 702.137, Companion 702.139, Mutate 702.140, Encore 702.141, Boast 702.142, Demonstrate 702.144, Daybound/Nightbound 702.145, Cleave 702.148, Compleated 702.150, Reconfigure 702.151, Read Ahead 702.155, Ravenous 702.156, Squad 702.157, Space Sculptor 702.158, Visit 702.159, Prototype 702.160, Living Metal 702.161, More Than Meets the Eye 702.162, For Mirrodin! 702.163, Craft 702.167, Solved 702.169, Freerunning 702.173, Exhaust 702.177, Max Speed/Start Your Engines! 702.178/.179, Harmonize 702.180, Job Select 702.182, Tiered 702.183, Station 702.184, Warp 702.185, ∞ 702.186, Web-slinging 702.188, Firebending 702.189, Sneak 702.190.

---

## 8. Scope for a 1v1 Arena-style Engine

- Target **two-player** game (`100.1a`). Starting life 20 (`103.4`); player on the play skips first draw (`103.8a`); 7-card hands, London mulligan (`103.5`).
- **In scope:** all of Parts 1–7 except multiplayer-specific subrules. The engine core = zones + objects + the priority/stack loop (117/405/601–603/608) + the layer system (613) + replacement/prevention (614–616) + SBAs/turn-based actions (703/704) + combat (506–511) + the Evergreen and Common keywords (§7).
- **Defer / stub initially:**
  - **Multiplayer (800s):** APNAP still matters in 2-player (active vs nonactive), but range-of-influence, multiple defenders, team turns, "leaves the game" cleanup (800.4) — defer. (In 2-player, a player leaving just ends the game, 104.2a.)
  - **Variants (900s):** Commander, Planechase, Vanguard, Archenemy, Conspiracy — defer, including their command-zone machinery and extra SBAs (704.6).
  - **Casual-only card types:** dungeon (309), plane/phenomenon/scheme/vanguard/conspiracy (311–315), Attractions/stickers (123/717), subgames (728) — defer.
  - **Niche keywords (§7.2)** and rarely-used mechanics: banding (702.22), phasing (702.26) unless needed, ante (407).
  - **Coin/die rolling (705/706)** only when a supported card needs it.
- **Don't skip even in 1v1:** the full priority loop, the complete layer system, replacement-effect interaction, LKI, the new-object rule, intervening-if, target re-legality, and the SBA check sequence. These are load-bearing for correctness on common cards.

---

### Quick rule-number index for drilling in
Priority/timing 117 · Stack 405 · Casting 601 · Activating 602 · Triggers 603 · Static 604 · Mana abilities 605 · Loyalty 606 · Linked 607 · Resolving 608 · Continuous effects 611 · **Layers 613** · Replacement 614 · Prevention 615 · Interaction 616 · Modes 700.2 · Keyword actions 701 · Keyword abilities 702 · Turn-based 703 · **SBAs 704** · Copying 707 · Combat 506–511 · Zones/new-object 400 · Characteristics 109/200s · Targets 115/608.2b · Costs 118.
