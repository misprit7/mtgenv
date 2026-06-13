# REPLAY_PLAN — game replays + omniscient spectating

Goal (user): watch replays of games — primarily AI self-play games sampled across training (to
see the agent learn), but also finished human games. Replays live in the lobby web view in a
**separate section** from the live/finished game list. A finished live game's **Spectate**
button becomes **Replay**. In **both replay and spectator mode there is NO hidden information**
— every zone is fully visible, including library order unknown to either player. The viewer can
**step** (forward/back, one frame at a time) or **auto-play at a settable rate**.

## Architecture (one contract, three layers)

The engine is deterministic and `GameState` is `Clone`+serde, so a replay is just a recorded
**stream of omniscient snapshots**. We record snapshots (not seed+decisions for re-execution) so
the viewer is a dumb frame-player with no engine dependency and scrubbing is trivial.

```
 mtg-core (engine)            mtg-gre-server + web (webui)        mtg-py + python (gym)
 ────────────────             ────────────────────────────       ─────────────────────
 god_view(state)  ─────────►  serves replays + viewer       ◄───  writes training replays
 Replay (serde type) = the SHARED CONTRACT all three speak
 record_replay hook           lobby "Replays" section            sample a few games/training
                              + step/auto-play viewer            → data/replays/*.json
```

### 1. Engine (`mtg-core`) — the replay core
- **`god_view(state) -> GodView`**: an omniscient view — every zone of every player fully
  visible, **including each library in order** and all hands, everything face-up. Reuse the
  existing `ObjView`/`CharacteristicsView` shapes so the web client renders it with its existing
  board code; add the full `library`/`hand` lists that `PlayerView` masks. (Either a distinct
  `GodView` type or a `PlayerView` superset with optional full-zone fields populated only here.)
- **`Replay` (serde)** — the contract: metadata `{ players: [{seat, name, deck}], result,
  source: Human|AiTraining{step}, created_at }` + `frames: Vec<ReplayFrame>` where
  `ReplayFrame = { state: GodView, label: String }` (label = what just happened, e.g.
  "P0 casts Lightning Bolt → P1", "P1 declares Grizzly Bears as attacker", "Turn 3 — P0 upkeep").
- **Recording hook**: an `Engine::record_replay` mode (like the existing `record_events`) that
  captures a frame each time the engine asks a decision and on key turn/phase/zone/combat/damage
  events, with the label. Expose the accumulated `Replay` after the game (and incrementally, so a
  live spectator can be fed the same frames). Frame granularity = per decision/action + phase
  boundaries — enough to "play back decisions."
- Stamp `created_at`/timestamps from outside (no `Date::now` in core — pass it in).

### 2. webui (`mtg-gre-server` + `web/`)
- **Storage + serving**: replays are JSON files in gitignored **`data/replays/`**. REST:
  `GET /api/replays` (list metadata), `GET /api/replays/:id` (full replay). On a live game
  finishing, **auto-save its `Replay`** to `data/replays/`.
- **Lobby**: keep the existing games list (live = **Spectate**, finished = **Replay**); add a
  **separate "AI Training Replays" section** listing `source: AiTraining` replays (show training
  step, decks, result).
- **Replay viewer** (web client): load a replay, render frames with the existing board UI but in
  **god-view (no masking)** — show both libraries (in order), both hands, the stack, etc.
  Controls: **step forward / back**, **play/pause**, and a **settable playback rate** (ms per
  frame / decisions-per-second slider). A frame shows its `label`.
- **Spectator mode → god-view**: live spectating must also show **no hidden information** (the
  user's requirement). Switch the spectator feed from a `PlayerView` to the omniscient `god_view`
  (spectators aren't players, so this can't leak to a competitor).

### 3. gym (`mtg-py` + `python/`)
- During training, **sample a few games across the run** (e.g. at eval checkpoints / every N
  updates) and **write their `Replay` to `data/replays/`** tagged `source: AiTraining{step}` so
  they appear in the lobby's AI section. Needs `mtg-py` to expose the engine's replay recording
  (run a game with `record_replay`, get the `Replay`, serialize to the JSON file). Keep it a
  handful per run, not every game.

## Sequencing / ownership
- **Shared contract**: the `Replay`/`GodView` serde schema — **engine owns it**; webui + gym
  consume. Lock the schema first so webui/gym build against it in parallel.
- **engine**: land *after* the in-flight subtype-enum flip (don't stack two big changes); it's
  additive (new view + type + record hook).
- **webui**: can scaffold the lobby section + viewer against the agreed schema immediately; wire
  to real data once engine lands `god_view`/`Replay` + the finished-game auto-save.
- **gym**: wire training export once `mtg-py` can get a `Replay`; lowest priority of the three
  (depends on engine).
- Replays are artifacts → `data/replays/` is gitignored (like `data/scryfall/`).
