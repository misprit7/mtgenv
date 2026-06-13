# CLIENT_PLAN — Web Play Interface + GRE-Protocol Server (drop-in MTGA-client compatible)

> Status: **PLAN ONLY.** No implementation yet. Implementation is **blocked on**
> `mtg-core` (board task #1 / ENGINE_PLAN milestones 1–3) for a playable engine, and on the
> recovered GRE schema (board task #2 / DECOMPILE_PLAN) for the protocol-compatible phase.
>
> Read first: `docs/design/WHITEBOARD_MODEL.md` §2.6 (decisions carry constraints),
> `docs/plans/ENGINE_PLAN.md` §6 (the decision boundary), `docs/plans/GYM_PLAN.md` §1–3
> (the *other* backend of the same boundary — RL — and the `SocketAgent` concept), and
> `docs/plans/DECOMPILE_PLAN.md` (the GRE protobuf schema + transport this plan consumes).

This plan describes a **web interface to play Magic against the `mtg-core` engine**, built so
that it shares the engine with the RL gym and the scripted AI, and built so closely to MTGA's
own client↔server interface that **the real MTGA client could later be dropped in against our
backend** with no engine changes — by endpoint redirection (if our server is transport- and
protocol-compatible) or by patching/recompiling the Mono client.

The load-bearing idea: **the seam is the GRE protocol itself.** If our server speaks the same
`GREToClientMessage`/`ClientToGREMessage` over the same transport as MTGA's real GRE, then
"our web client" and "the real MTGA client" are interchangeable front ends to the same
engine — exactly mirroring how `RandomAgent`, the Python RL policy, and a human are
interchangeable *back ends* to the same engine.

---

## 1. Goal & the one-boundary principle

**Goal.** A from-scratch web UI that lets a human play full games of Magic driven by
`mtg-core`, structured as a GRE-protocol server so that:

1. a human at the web UI is **just another `Agent` backend** — the same `Agent` /
   `DecisionRequest` / `DecisionResponse` seam used by the scripted AI and the Python RL gym
   (ENGINE_PLAN §6, GYM_PLAN §3); and
2. the wire protocol and transport are **as close to MTGA's as practical**, so the real
   client becomes a drop-in front end later.

**There is exactly one decision boundary** (architecture law, `CLAUDE.md`). Every player
choice — RL policy, scripted heuristic, or human-at-a-browser — flows through the same
`Agent::decide(view, req) -> resp` call where the engine has already enumerated the *legal*
options (masking is the engine's job). The web client is not a new boundary; it is a new
**transport+presentation** layer behind the existing one. This is the project's "easy
switch" goal applied to the *human* seat.

```
                          ONE Agent boundary (mtg-core)
                                     │
       ┌─────────────────────────────┼──────────────────────────────┐
       │                             │                              │
  RandomAgent /              PyAgent (PyO3)               GreSessionAgent  ◄── this plan
  ScriptedAgent              → Python RL policy           → wire protocol → a human
  (in-process)               (GYM_PLAN)                   (web UI) or the real MTGA client
```

The only thing that differs across backends is *who answers `decide()` and over what
transport*. For RL that is an in-process PyO3 channel; for the human it is a GRE message
serialized over a socket/websocket to a browser (or to the real client).

---

## 2. Architecture

```
┌──────────────────────────────────────────────────────────────────────────┐
│  mtg-core  (headless rules engine — the GRE analog)                         │
│  GameState · turn/priority/stack · whiteboard commit · combat · SBAs        │
│  Pauses at each decision point and calls Agent::decide(view, req).          │
└───────────────▲───────────────────────────────────────────┬───────────────┘
   DecisionRequest (legal options enumerated)   │   DecisionResponse (indices)
   GameEvent (observe push)                      │
┌───────────────┴───────────────────────────────▼───────────────────────────┐
│  GreSessionAgent : Agent          (NEW crate: mtg-gre-server)               │
│  - decide():  DecisionRequest  ── map ──▶ GREToClientMessage(*Req)           │
│               DecisionResponse ◀─ map ── ClientToGREMessage(*Resp / action)  │
│  - observe(): GameEvent        ── map ──▶ GameStateMessage (Full / Diff)     │
│  Blocks decide() on a wire round-trip; pushes state diffs server→client.    │
└───────────────┬───────────────────────────────────────────┬───────────────┘
                │  serialize + frame                          │
                ▼  (transport adapters — same messages,       ▼
                    different framing)                          
   ┌─────────────────────────────┐          ┌──────────────────────────────────┐
   │ WebSocket transport (axum)  │          │ TCP/TLS transport (MTGA-compatible)│
   │ GRE msgs as protobuf or a   │          │ byte-for-byte MTGA framing          │
   │ JSON projection over WS     │          │ (length-prefixed protobuf)          │
   └──────────────┬──────────────┘          └─────────────────┬──────────────────┘
                  │                                            │
                  ▼                                            ▼
   ┌─────────────────────────────┐          ┌──────────────────────────────────┐
   │  (1) FROM-SCRATCH WEB CLIENT │          │  (2) REAL MTGA CLIENT (retargeted) │
   │  TS/JS in browser; renders   │          │  stock or patched; endpoint        │
   │  board/hand/stack; submits   │          │  redirected at our server          │
   │  legal actions only          │          │  (drop-in: §8)                     │
   └─────────────────────────────┘          └──────────────────────────────────┘
```

Key points:

- **`mtg-gre-server` is a separate crate that depends only on `mtg-core`.** The core stays
  headless (no axum/tokio/protobuf in `mtg-core`) — same rule that keeps `mtg-py` out of the
  core (ENGINE_PLAN §3, GYM_PLAN §2). Per repo conventions the server's binary is a thin
  `bin/` target.
- **`GreSessionAgent` is the bridge** and the *only* new concept: an `impl Agent` whose
  `decide()` translates the engine's `DecisionRequest` into the matching GRE `*Req` message,
  ships it, and blocks until the matching `*Resp`/action comes back, which it translates into
  a `DecisionResponse`. It is the GRE-protobuf sibling of GYM_PLAN's `SocketAgent` (which
  uses JSON) and `MtgaClientAgent` (DECOMPILE_PLAN §5) — *same trait, different wire*. The
  formal contract that makes this a **thin, lossless, table-driven adapter** (not a
  reinterpretation) is `AGENT_INTERFACE.md` §1.1: all boundary types derive `serde`, each
  variant maps 1:1 onto a GRE `*Req`/`*Resp`, and index-based responses resolve back to
  concrete GRE object refs via the request's own enumerated option vectors. Per that contract,
  **the web client and the real MTGA client are the same backend** — two clients of one GRE
  server.
- **Message semantics vs. transport framing are separated.** The GRE *message set* (the
  recovered protobuf types) is shared by both transports; only the *framing* differs
  (WebSocket frames for the browser, MTGA's TCP framing for the real client). Swapping in the
  real client is therefore a *transport-listener* swap, not a message-logic rewrite.
- **A game is N `Agent` seats.** A match holds one `Box<dyn Agent>` per seat. Human-vs-AI =
  one `GreSessionAgent` + one `ScriptedAgent`; human-vs-human = two `GreSessionAgent`s
  bound to two browser sessions; real-client-vs-our-AI = one TCP `GreSessionAgent` + one
  `ScriptedAgent`. All are constructed at match setup; the engine is unaware which is which.

---

## 3. Why this is the same boundary as the gym (not a parallel system)

GYM_PLAN already defines the `Agent` trait and a transport-agnostic philosophy: "the boundary
is the **trait**, not the transport" (GYM_PLAN §2). It even names a `SocketAgent` for "the
MTGA client and Forge-interim backends." This plan *is* that `SocketAgent`, specialized to:

| Aspect | RL gym (GYM_PLAN) | Web/GRE client (this plan) |
|---|---|---|
| `Agent` impl | `PyAgent` (PyO3 yield) | `GreSessionAgent` (wire round-trip) |
| Transport | in-process FFI / channel | WebSocket (browser) or TCP/TLS (real client) |
| Wire format | none (zero-copy numpy) | GRE protobuf (or a JSON projection early on) |
| Who answers | a PyTorch policy | a human (or the real client's UI) |
| Legal options | engine-enumerated; → action **mask** | engine-enumerated; → only-legal UI affordances |
| State to decider | obs tensor (`PlayerView` → numpy) | `GameStateMessage` (`PlayerView` → protobuf) |

The crucial shared invariant (WHITEBOARD_MODEL §2.6, GYM_PLAN §3): **the engine never asks an
open-ended question.** Every `DecisionRequest` ships the complete enumerated legal set. For
RL that becomes a boolean action mask; for the web UI that becomes "only the legal attackers
are clickable, only legal targets highlight, the X chooser excludes forbidden values." The UI
gets *correctness-by-construction masking for free*, the same way the policy does. Illegal
moves are literally unrepresentable in the client.

---

## 4. Transport / framing / handshake / auth

> **⚠ BLOCKED ON DECOMPILE (task #2).** The exact transport, framing, handshake, and auth are
> being recovered by the `decompile` workstream from `Wizards.Arena.TcpConnection.dll` /
> `Wizards.Arena.MessageSerialization.dll` (DECOMPILE_PLAN Tier-1). The subsections below
> state our **working assumptions** and what we need confirmed; replace with facts as they
> land. Question sent to `decompile`; answers fold in here.

### 4.1 Two transports, one message set
Browsers cannot open raw TCP sockets, and the real MTGA client does not speak WebSocket. So we
support **two transport adapters carrying the same GRE messages**:

- **WebSocket (our web client).** axum WebSocket endpoint. Carries either (a) GRE protobuf
  bytes framed one-message-per-WS-binary-frame (decoded in-browser with `protobuf.js`/
  `ts-proto`), or (b) early on, a **JSON projection** of the same messages (simpler to build
  and debug). WS already gives us message framing, so no custom length-prefix is needed here.
- **TCP/TLS (the real client).** Must replicate MTGA's exact framing so a stock or lightly
  patched client accepts us. **Assumption** (pending decompile): length-prefixed Google
  protobuf over a TCP stream, likely TLS-wrapped. The `GameStateType ∈ {Full,Diff,Binary}`
  enum (DECOMPILE_PLAN) confirms full-vs-diff state messages exist.

### 4.2 Framing (assumption → confirm)
Most Google.Protobuf TCP services use **length-delimited** framing (a varint or fixed-width
length prefix per message). We assume that for the MTGA-compatible TCP transport and will
confirm the exact prefix width/encoding from `Wizards.Arena.MessageSerialization.dll`. For the
WebSocket transport we lean on WS frame boundaries and do not reinvent framing.

### 4.3 Handshake & session lifecycle (assumption → confirm)
The recovered `GREMessageType` catalog includes `ConnectResp` and a setup handshake
(`ChooseStartingPlayerReq`, `SubmitDeckReq`/`SubmitDeckConfirmation`, `MulliganReq`,
`DieRollResultsResp`, `GetSettingsResp`/`SetSettingsResp`). Our server must implement this
opening sequence:

```
client → ConnectReq(/handshake)         server → ConnectResp
        (deck submit)  SubmitDeckReq  ↔  SubmitDeckConfirmation
        die roll / choose starting player → ChooseStartingPlayerReq
        opening hands → MulliganReq ↔ (mulligan resp)
        → steady-state: GameStateMessage(Full) then Diffs + *Req/*Resp decision loop
```

We need from decompile: the **exact first-message** the client sends, whether a
**session token / match id** minted by the login+matchmaking services is required to open the
GRE session, and whether the GRE connect is a *fresh* connection or an *upgrade* of the
matchmaking channel. This determines whether endpoint redirect alone can work (§8).

### 4.4 Auth & TLS (assumption → confirm; the drop-in blocker)
The two questions that decide drop-in feasibility:

1. **Does the GRE session validate an auth/session token** issued by WotC's login/matchmaking
   servers (which we cannot mint)? If yes, endpoint-redirect requires either also stubbing
   login/matchmaking, or patching the client to skip the check (§8).
2. **Does the client pin the server certificate** (or otherwise reject an unknown TLS cert)?
   If yes, a hosts/DNS redirect to our TLS server fails handshake unless we patch out pinning.

These are precisely the items requested from `decompile`. The whole web client (§5–7) can be
built and is fully useful **without** resolving them — they gate only the *real-client
drop-in* (§8), not our own web UI.

### 4.5 What `mtg-gre-server` owns vs. what the client owns
The server owns: framing/serialization, the connect handshake, mapping `DecisionRequest`↔GRE
`*Req`/action, mapping `GameEvent`→`GameStateMessage`, and per-seat session state. The client
owns: rendering `PlayerView`/`GameStateMessage`, presenting *only the enumerated legal
options*, and submitting one `ClientToGREMessage` per decision.

**Transport/UI-only GRE messages are the server's job, not the engine's.** Several
`GREMessageType`s are *not* player decisions and therefore never reach the `Agent` boundary:
`IntermissionReq`, `TimeoutMessage`, `TimerStateMessage`, `UIMessage`, `PredictionResp` (and
`ConnectResp`, settings get/set). The GRE server originates/answers these itself — turn
timers, intermissions between games, UI hints, connection liveness — without consulting
`mtg-core`. (Confirmed with `design`: these live in the server layer, deliberately *outside*
`DecisionRequest`, which stays a pure superset of the engine's actual decision points.) This
keeps the engine boundary clean while still presenting the real client a complete GRE surface.

---

## 5. DecisionRequest ⇄ GRE message mapping

> **Canonical mapping lives in `docs/design/AGENT_INTERFACE.md` §6.1** (owned by `design`,
> task #3, now landed). That table maps every recovered GRE `*Req` onto a `DecisionRequest`
> variant and proves the enum is a strict *superset* of the GRE catalog. The variant names
> below are reconciled to that document — do not let them drift. Keeping our `DecisionRequest`
> structurally aligned to the GRE `*Req` set is exactly what makes the real-client drop-in an
> adapter, not a rewrite (DECOMPILE_PLAN §5, AGENT_INTERFACE §0 law #4).

This plan's contribution on top of AGENT_INTERFACE §6.1 is the **wire round-trip** view: for
each request, what the GRE server *sends down* and what `ClientToGREMessage` it expects *back*
(the response direction the engine sees as a `DecisionResponse`):

| Engine `DecisionRequest` (AGENT_INTERFACE §3) | Server→client GRE `*Req` | Client→server (ClientToGRE) |
|---|---|---|
| `Priority { actions, can_pass }` | `ActionsAvailableReq` | action w/ `ActionType` Pass/Play/Cast*/Activate/Activate_Mana/Special |
| `ChooseStartingPlayer { candidates }` | `ChooseStartingPlayerReq` | choose-player response |
| `Mulligan { .. }` (+ follow-up `SelectCards{BottomForMulligan}`) | `MulliganReq` | `MulliganResp` |
| `ChooseTargets { for_action, slots }` | `SelectTargetsReq` | `SubmitTargetsResp` |
| `ChooseModes { .. }` | part of `CastingTimeOptionsReq` | options response |
| `CastingTimeOptions { for_action, options }` | `CastingTimeOptionsReq` (`CastingTimeOptionType`) | cast-options response |
| `ChooseNumber { reason, min, max, forbidden }` | `NumericInputReq` | numeric response |
| `Distribute { .. }` | `DistributionReq` | distribution response |
| `PayCost { cost, mana_sources, non_mana }` | `PayCostsReq` | `Make_Payment`/`Activate_Mana`/`FloatMana`/`Special_Payment` |
| `DeclareAttackers { eligible }` | `DeclareAttackersReq` | `SubmitAttackersResp` |
| `DeclareBlockers { eligible, attackers }` | `DeclareBlockersReq` | `SubmitBlockersResp` |
| `AssignCombatDamage { .. }` | `AssignDamageReq` (+ `OrderCombatDamageReq`) | `AssignDamageConfirmation` / `OrderDamageConfirmation` |
| `OrderObjects { kind, items }` | `OrderReq` (combat: `OrderCombatDamageReq`) | `OrderResp` |
| `SelectCards { reason, from, min, max, filter }` | `SelectNReq` / `SearchReq` / `RevealHandReq` | `SubmitN`/search response |
| `SelectFromGroups { reason, groups }` | `SelectNGroupReq` / `SelectFromGroupsReq` / `GroupReq` | group response |
| `ArrangeCards { reason, cards, destinations }` | scry/surveil prompt (pending decompile) | arrange response |
| `ChooseReplacement { event, applicable }` | `SelectReplacementReq` | replacement response |
| `ChooseCounterType { options }` | `SelectCountersReq` | counter-type response |
| `ChooseOption { reason, options, min, max }` | `PromptReq` / `StringInputReq` | option response |
| `ChooseColor { allowed, min, max }` | choose-option-from-list prompt | color response |
| `Confirm { kind }` | `PromptReq` / `OptionalActionMessage` / `AllowForceDraw` | binary response |
| (push, no response) state delta | `GameStateMessage{ Full \| Diff }` | — |
| (push, no response) reveal / UI | `RevealHandReq` / `UIMessage` / `TimerStateMessage` | (none / ack) |

`DecisionResponse` is **selection-into-options** (AGENT_INTERFACE §4: `Pass`/`Index`/`Indices`/
`Number`/`Bool`/`Pairs`/`Amounts`/`Order`/`Arrangement`/`Payment`/`Action`). The GRE server
translates those selections back into the concrete GRE response payloads the protocol expects
(object ids, target maps, damage splits, payment specs). Selection-based responses keep the
web client and the RL policy structurally identical on the answer side — both only ever pick
among engine-enumerated legal options.

**Field-level shapes still pending decompile** (shared open list with AGENT_INTERFACE §9):
mulligan/London bottoming encoding, `NumericInputReq` min/max/forbidden, `SelectTargetsReq`
target-map vs criteria, `AssignDamageReq`/`OrderCombatDamageReq` split, and
`PayCostsReq`/`CastingTimeOptionsReq` batched-vs-substepped granularity (this last one sets how
many GRE round-trips a single cast costs and how the web UI sequences its prompts). The
*variant set* is settled; only field details remain.

---

## 6. Web stack recommendation

**Recommendation: a Rust `axum` + `tokio` server (WebSocket transport) serving a small
TypeScript front end. Minimal dependencies; the headless core stays clean.**

### 6.1 Server — new crate `mtg-gre-server`
- **Depends only on `mtg-core`.** No server/protobuf deps leak into the core (architecture
  law). Thin `bin/` entrypoint per repo conventions; the bridge logic is the lib.
- **Deps (minimal):** `axum` (HTTP + WS upgrade), `tokio` (async runtime), `serde`/
  `serde_json` (the early JSON projection + config), `tower-http` (serve the static
  front-end build). Add `prost` **only** at the protocol-compatible phase (§9 milestone 3),
  generated from the `.proto` recovered by decompile; add `rustls`/`tokio-rustls` only for
  the TCP/TLS real-client transport (§9 milestone 4). Do not pull these in earlier.
- **Concurrency model:** `mtg-core` is synchronous and deterministic (GYM_PLAN §1). Run each
  game on its own task/thread; `GreSessionAgent::decide()` blocks that game on a
  `oneshot`/`mpsc` channel fed by the WebSocket task. This keeps the engine's clean
  pure-function loop intact and confines all async to the server crate.

### 6.2 Front end — TypeScript, kept small
- **TS + Vite**, rendered with a lightweight view layer (vanilla TS or `lit`/`preact` — avoid
  a heavyweight SPA framework; the board is a custom DOM/canvas render, not CRUD).
- **Transport:** WebSocket. Early phase: JSON messages (trivial to parse/inspect). Protocol
  phase: decode GRE protobuf with `ts-proto`/`protobuf.js` generated from the recovered
  `.proto`, so the browser sees the *same* messages the real client would.
- **Build:** the front end is its own toolchain under `crates/mtg-gre-server/web/` (or a
  top-level `web/`), `node_modules` gitignored; the production build is served as static
  files by axum. No JS/TS deps enter any Rust crate.

### 6.3 Why this stack
- axum/tokio is the de-facto minimal async Rust web stack; WebSocket support is first-class.
- Keeping the bridge in Rust means **one language owns the `Agent` impl and the protobuf
  framing** — the same code paths can later drive the TCP transport for the real client.
- A thin TS front end avoids coupling presentation to a framework and keeps the door open to
  swapping in protobuf-over-WS without rewriting the data layer.

---

## 7. State push + action submission + how masking surfaces in the UI

**Two channels over one connection**, mirroring the engine's two outward calls
(`observe()` for pushes, `decide()` for prompts):

1. **State push (server→client, no response).** When `mtg-core` emits a `GameEvent`,
   `GreSessionAgent::observe()` translates it into a `GameStateMessage` — `Full` on connect /
   game start, `Diff` for each subsequent change — and pushes it down the socket. The client
   keeps a local mirror of the (information-filtered) game state and re-renders. **Hidden
   information is enforced server-side**: the client only ever receives that seat's
   `PlayerView` (opponent hand as counts, library order hidden) — the *same* masking the RL
   obs encoder uses (GYM_PLAN §4). The client cannot leak what it never received.

2. **Decision prompt (server→client, expects one response).** When the engine needs a choice,
   `decide()` sends the matching `*Req` carrying the **enumerated legal options**. The client
   renders those as the *only* actionable affordances and submits exactly one
   `ClientToGREMessage`. The server maps it to a `DecisionResponse` (indices) and unblocks the
   game.

**Masking in the UI = the enumerated option set, rendered.** Because the engine pre-computes
legality (rules + timing + targeting + mana), the client never decides legality — it just
draws what it was given:

- `Priority`/`ActionsAvailableReq` → only castable cards / activatable abilities are
  highlighted; everything else is inert; `Pass` is always available.
- `DeclareAttackersReq` → only eligible creatures are selectable as attackers.
- `SelectTargetsReq` → only legal targets glow; clicking an illegal object is impossible.
- `NumericInputReq` (X) → the chooser excludes `forbidden` values (WHITEBOARD_MODEL §2.6).

This is the human-facing twin of the RL action mask: same source of truth (engine), same
guarantee (no illegal move can be submitted), different presentation (clickable UI vs.
`-inf` logits). It also means **the web UI cannot desync from the rules** — it has no
independent notion of legality to get wrong.

A small **"stops"/auto-pass** policy (Arena profile, ENGINE_PLAN §9) keeps the human from
being prompted at every trivial priority window: the server auto-passes priority windows with
no legal non-pass action (configurable, like MTGA's stops), so the player is consulted only at
meaningful decision points — the same lever GYM_PLAN §4 uses to cut steps/game.

---

## 8. Two drop-in strategies for the real MTGA client

The goal: get the **stock MTGA client** to render and play a game whose rules are actually run
by `mtg-core`. MTGA being a **Mono** build (DECOMPILE_PLAN: managed CIL, decompilable and
patchable) makes both strategies far more tractable than an IL2CPP target would.

### Strategy A — Protocol-compatible server + endpoint redirect (no binary modification)
Make `mtg-gre-server`'s TCP/TLS transport byte-compatible with MTGA's real GRE server, then
redirect the client's GRE connection to us (hosts-file / DNS override, or a local proxy).

- **Pros:** client stays *stock* (no ToS-fraught binary modification); survives client updates
  as long as the GRE protocol is stable; the cleanest expression of "the seam is the
  protocol."
- **Cons / blockers (all pending decompile §4.4):**
  - **Cert pinning** — if the client pins WotC's cert, a redirect to our TLS endpoint fails
    the handshake. Mitigations all involve touching the client (→ Strategy B) or a system
    trust-store + non-pinned build (unlikely).
  - **Auth/session token** — if opening a GRE session requires a token minted by WotC
    login/matchmaking, we must *also* stand in for those services (stub login + matchmaking so
    they hand the client a token our GRE server accepts), substantially widening scope beyond
    the GRE endpoint.
  - **Service mesh** — the client talks to many services (login, matchmaking/MQTT, assets,
    telemetry). A redirect must isolate *only* the GRE channel and let or stub the rest.
- **Net:** cleanest if and only if (a) no cert pinning and (b) the GRE connect can be opened
  without a WotC-minted token (or with one we can synthesize). Decompile findings decide this.

### Strategy B — Patch / runtime-hook the client (Mono)
Use the Mono toolchain to change the client's behavior: point the GRE endpoint at us, accept
our cert / disable pinning, and bypass the token/matchmaking requirement.

- **Variants, least to most invasive:**
  1. **Runtime hook (preferred):** a `BepInEx`/`MonoMod`/`Harmony` plugin that hooks the
     connection-establishment method at load time to rewrite the endpoint + relax cert
     validation, and short-circuits matchmaking into a "connect to local GRE" path. Nothing on
     disk changes permanently; re-applies across updates if the hooked method signature is
     stable.
  2. **Static IL patch:** edit `Assembly-CSharp.dll` / the transport DLL with `dnSpyEx` /
     `Mono.Cecil` to hardcode the endpoint and neuter pinning. Must re-patch every update.
- **Pros:** full control; removes the cert-pinning and token blockers that gate Strategy A;
  can skip straight into a local match.
- **Cons:** re-do on every client update; the most RE effort; **strictly personal/local —
  never redistribute a patched client or WotC binaries** (§10).

### Recommendation
**Attempt Strategy A first** (it needs no client modification and is the cleanest); the moment
cert pinning or a mandatory WotC token blocks it (likely), **fall back to Strategy B's runtime
hook** as the least-invasive patch. Both consume the same protocol-compatible server — the
only difference is whether the client is persuaded to talk to us by redirection or by hooking.
Either way the server and engine are unchanged: drop-in = pointing the client at our endpoint.

---

## 9. Milestone path

Each milestone is independently useful and most are unblocked by the protocol work — only
milestones 3–4 need the recovered schema / decompile findings.

0. **(Prereq)** `mtg-core` plays a vanilla-creature game with the `Agent` boundary
   (ENGINE_PLAN milestones 1–3) and `AGENT_INTERFACE.md` (task #3) fixes the
   `DecisionRequest`/`Response` enums.

1. **CLI / text client.** A `HumanAgent` (or a local stdin/stdout `GreSessionAgent`) in
   `mtg-cli` that prints the `PlayerView` and the enumerated legal options and reads a chosen
   index from stdin. **Proves "a human is just another `Agent`"** with zero protocol/web work.
   Exit: a human can play a full game vs. `ScriptedAgent` at the terminal.

2. **Minimal web board.** `mtg-gre-server` (axum + WS) with a **JSON projection** of
   `DecisionRequest`/`Response`/state, and a small TS front end that renders hand/board/stack
   and supports **cast / attack / block / pass** with legal-option masking. No protobuf yet.
   Exit: play a full game in the browser vs. `ScriptedAgent`; human-vs-human over two WS
   sessions.

3. **Protocol-compatible server.** Replace the JSON projection with the **recovered GRE
   protobuf** message set (`prost` from decompile's `.proto`); emit real
   `GREToClientMessage`/`GameStateMessage`, consume `ClientToGREMessage`. Front end switches
   to protobuf-over-WS (`ts-proto`). **Validate** by round-tripping captured Detailed-Logs GRE
   streams (DECOMPILE_PLAN Phase 5) through our server. Exit: our web client plays a full game
   speaking *real GRE messages*; recorded MTGA messages parse/serialize identically.

4. **Attempt real-client drop-in.** Stand up the TCP/TLS transport with MTGA-compatible
   framing + the connect handshake (§4.3). Try **Strategy A** (endpoint redirect); on
   cert/token blockers fall back to **Strategy B** (runtime hook). Exit: the stock MTGA client
   renders and plays at least the opening of a game driven by `mtg-core`.

Milestones 1–2 depend only on `mtg-core` (#1) + `AGENT_INTERFACE.md` (#3). Milestones 3–4
additionally depend on the recovered schema + transport facts (#2 / DECOMPILE_PLAN).

---

## 10. Legal / ToS

This is **personal research and interoperability** only — the same posture as DECOMPILE_PLAN
§6, extended to a client:

- **Do not redistribute WotC assets, code, or binaries** — no card images, no DLLs, no
  patched client. The from-scratch web client uses *our own* placeholder art / text; any use
  of real card data is for personal local testing, not redistribution.
- **A patched/hooked MTGA client (Strategy B) is strictly local and personal** — never
  shared or published.
- **Never automate ranked/online play against other humans.** The drop-in target is the real
  client talking to **our** local engine, not to WotC's servers — i.e. the opposite of
  cheating online: we are replacing the *server*, for solo/local interop research.
- Reverse engineering for interoperability (recovering a message schema to build a compatible
  interface) is the recognized rationale; keep scope to the protocol + a local server.

---

## 11. Open questions / risks

- **Auth & cert pinning (the drop-in gate).** Whether endpoint redirect (Strategy A) is even
  possible hinges on decompile §4.4 findings; budget for falling back to Strategy B.
- **Protocol drift.** MTGA updates can change the GRE schema/framing; pin the version
  (`2026.59.30.12801`, build-guid in DECOMPILE_PLAN) and treat the recovered schema as a
  snapshot. The web client (milestones 1–3) is insulated from drift; only milestone 4 isn't.
- **Composite vs. atomic decisions — RESOLVED (with `design`).** A `Priority` selection of a
  `PlayableAction::Cast` does **not** carry modes/targets/X/payment inline. The engine spawns
  follow-up `DecisionRequest`s in CR 601.2 order — `ChooseModes` (601.2b) → `ChooseTargets`
  (601.2c) → `Distribute` if needed (601.2d) → `ChooseNumber` for X (601.2b) → `PayCost`
  (601.2f–h) — **each its own `decide()` call and thus its own GRE round-trip.** So a single
  cast is a short *sequence* of prompts, not one mega-prompt; the web UI guides the player
  through that sequence. (`GreSessionAgent` may skip the **wire round-trip** for a step that
  has a single legal option — answering `Index(0)` locally, identical to what
  `PyAgent`/`ScriptedAgent` would return — but it must **not** *elide the decision itself*:
  the engine still issues every `decide()` call, so all backends are consulted at the same
  points. Whether a forced single-option decision is issued at all is an **engine / Arena-
  profile** concern, like auto-pass "stops" (AGENT_INTERFACE §8, GYM_PLAN §4), kept out of the
  per-agent layer so the decision log replays identically — required for differential-testing-
  vs-Forge and exact replay, ENGINE_PLAN §8.) `CastingTimeOptions` exists so a backend can
  instead mirror GRE's
  **batched** `CastingTimeOptionsReq`, collapsing the cast-time choices into one round-trip.
  The adapter must therefore **handle both shapes** (sequence or batched) and map whichever
  the engine emits. *Remaining knob:* the exact batched-vs-substepped granularity is one of
  the shared §5/AGENT_INTERFACE §9 pending-decompile items
  (`PayCostsReq`/`CastingTimeOptionsReq`) — lock it once the schema lands.
- **State-diff fidelity.** Producing correct `GameStateMessage` *diffs* (not just `Full`) from
  `GameEvent`s is non-trivial; start `Full`-only for our web client, add diffs for the
  real-client transport (the real client likely expects diffs).
- **Scope creep into the service mesh.** Strategy A may drag in login/matchmaking stubs; keep
  that out of `mtg-gre-server` (a separate experiment) so the web client stays clean.
```
