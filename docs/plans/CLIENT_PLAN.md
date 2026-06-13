# CLIENT_PLAN — Web Play Interface + GRE-Protocol Server (drop-in MTGA-client compatible)

> Status: **PLAN ONLY.** No implementation yet. Implementation is **blocked on**
> `mtg-core` (board task #1 / ENGINE_PLAN milestones 1–3) for a playable engine. The GRE schema
> *and* transport are now **recovered + log-validated** (task #2 done; transport capture #6) in
> `../mtga-re` — §4 and §5 reflect those facts, not assumptions.
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

> **✅ RECOVERED BY DECOMPILE (task #2).** `decompile` decompiled
> `Wizards.Arena.TcpConnection.dll` / `Wizards.Arena.MessageSerialization.dll` /
> `SharedClientCore.dll` and recovered the full transport + the GRE message schema (254
> messages / 135 enums in `../mtga-re/schema/gre_schema.json`, validated against a 2134-message
> live log with zero mismatches). The facts below are from that decompile, not assumptions.
> Source: `../mtga-re/decompiled/TcpConnection/.../TcpConnection.cs`,
> `.../MessageSerialization/MessageEnvelopeExtensions.cs`, `.../SharedClientCore/FrontDoorConnectionAWS.cs`.

### 4.1 Two transports, one message set
Browsers cannot open raw TCP sockets, and the real MTGA client speaks raw TLS-over-TCP (below),
not WebSocket. So we support **two transport adapters carrying the same GRE message set**:

- **WebSocket (our web client).** axum WebSocket endpoint. Carries either (a) GRE protobuf
  bytes one-message-per-WS-binary-frame (decoded in-browser with `protobuf.js`/`ts-proto`), or
  (b) early on, a **JSON projection** of the same messages. WS frames give us message
  boundaries, so we don't reimplement MTGA's length-framing here.
- **TLS-over-TCP (the real client).** Must replicate MTGA's exact framing (§4.2) so a stock or
  lightly-patched client accepts us. This is the byte-compatible path for the drop-in (§8).

### 4.2 Transport & framing (recovered — exact)
MTGA's GRE link is a **raw TCP socket wrapped in mandatory TLS 1.2**, with a small custom frame
inside the TLS stream:

- **TCP + TLS 1.2.** `TcpConnection.Connect(host, port)` opens a `TcpClient`, wraps it in an
  `SslStream`, and calls `BeginAuthenticateAsClient(host, <no client cert>, SslProtocols.Tls12,
  checkCertificateRevocation: true)`. It then asserts the stream `IsEncrypted && IsSigned` or
  drops the connection. **Server-authenticated TLS, no client certificate.**
- **Cert validation is a constructor-injected `RemoteCertificateValidationCallback` (`certCb`).**
  If `certCb == null` (the stock path) the `SslStream` uses **.NET default validation** — the
  server cert must chain to a trusted root **and match the connect `host`** (+ revocation check).
  This is *not* hard pinning; it's standard hostname/chain validation. (Implication for drop-in
  in §8: because the client validates against the *host it was told to connect to*, and that
  host is supplied dynamically by the match push (§4.3), we can choose a hostname we hold a
  trusted cert for.)
- **Frame = 6-byte header + body, written inside the TLS stream** (`SslStream.Write`). Header
  (protocol version 4, the current `_sendVersion`):

  ```
  byte 0      : version            (currently 4)
  byte 1      : (type & 0x0F) | ((format & 0x0F) << 4)
                  low  nibble = message type  (IMsg.EType: Ping / Pong / Debug / Message)
                  high nibble = serialization format (Protobuf / Json)
  bytes 2..5  : int32 body length, little-endian (BitConverter on x86/Mono)
  bytes 6..   : body (the serialized message envelope)
  ```
  (`num2 = 6 + bodyLength`. v3 used `[version][type:1][len:4]` = also 6 bytes; we target v4.)
- **Transport keepalive.** `Ping`/`Pong` are first-class frame *types* (not GRE messages): the
  client pings on an interval and a v≥3 peer must `Pong` back; there's an inactivity timeout and
  round-trip-time sampling. **Our server must answer pings** to hold the connection (a
  transport-layer concern, below the `Agent` boundary).
- **Message envelope (the body).** `IMessageEnvelope` carries `Format ∈ {Protobuf, Json}`, a
  `Compressed` flag (JSON payloads may be `DtoCompressor`-compressed; protobuf isn't), a
  `TransId` (request/response correlation at the transport layer), and the payload. For
  protobuf, the payload is a `google.protobuf.Any` (TypeUrl + bytes) resolved via a
  `MessageDescriptorRegistry` — i.e. the GRE messages (`GREToClientMessage`,
  `ClientToMatchServiceMessage`, …) are packed as `Any`. **The wire supports both protobuf and
  JSON**, which is convenient: our web client can use the JSON envelope form and the real client
  the protobuf form over the same server.

### 4.3 Handshake, endpoint & session lifecycle (recovered)
The GRE endpoint is **not hardcoded** — it is handed to the client by the matchmaking/front-door
layer after a match is made, then the client opens the TLS link to it:

```
matchmaking push (FrontDoorConnectionAWS):
    MatchInfoV3 { MatchEndpointHost, MatchEndpointPort, MatchId }   ← server tells client WHERE
    (auth SessionId comes from the auth-service wrapper, upstream)
        │
        ▼  client: _tcpConn.Connect(MatchEndpointHost, MatchEndpointPort)  + TLS 1.2 auth
GRE session handshake (over the TLS link):
    client → ConnectReq  { defaultSettings, protoVer, grpVersion, grpChangelist }   ← NO auth token
    server → ConnectResp { status, protoVer, settings, deckMessage, gre/grpVersion, skins, changelists }
        │
        ▼  SubmitDeck ↔ confirmation · die roll / ChooseStartingPlayerReq · MulliganReq ↔ MulliganResp
        ▼  steady state: GameStateMessage(Full) → Diffs + the *Req/*Resp decision loop (§5)
```

Key recovered facts that shape the drop-in:
- **`ConnectReq` carries no auth/session token** — only version & settings negotiation. Auth and
  match identity (`SessionId`, `MatchId`) are bound *upstream* at the match-service/front-door
  layer (`ClientToMatchDoorConnectRequest`), **not** at the GRE TCP session. So the GRE server
  itself does not need to validate a WotC-minted token — it only needs the client to *reach* it.
- **The endpoint host:port is dynamic** (`MatchInfoV3.MatchEndpointHost/Port`). Redirecting the
  client to our server is therefore a matter of controlling that push (§8 Strategy A), not
  rewriting a baked-in address.
- **Envelope correlation** is `TransId` at the transport layer and `msgId`→`respId` at the GRE
  layer (server stamps `msgId`; client echoes it as `respId`). `GreSessionAgent` tracks these to
  match a `DecisionResponse` to the request it answers.

### 4.4 Drop-in feasibility (now de-risked)
With the above, the two former blockers resolve favorably:

1. **Auth token at the GRE session: none.** Auth lives at the match-service layer; the GRE
   `ConnectReq` is tokenless. We do **not** need to mint a WotC token to run the GRE session — we
   need only to get the client to connect to us (control the match push, or patch — §8).
2. **TLS cert: standard hostname/chain validation, not opaque pinning.** Because the client
   validates the cert against the *host the push told it to use*, and we supply that host, we can
   push a hostname we hold a locally-trusted cert for (dev CA installed in the Mono/system trust
   store) — no cert-pinning bypass required. If a future build hard-pins, fall back to patching
   `certCb` (§8 Strategy B). Either way TLS 1.2 is mandatory, so our server must terminate TLS.

The web client (§5–7) needs none of this — it talks to our own WebSocket endpoint. These facts
gate only the *real-client drop-in* (§8).

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
| `Priority { actions, can_pass }` | `ActionsAvailableReq` | `PerformActionResp` (`Action[]` + `autoPassPriority`; `ActionType` Pass/Play/Cast*/Activate/Activate_Mana/Special) |
| `ChooseStartingPlayer { candidates }` | `ChooseStartingPlayerReq` | `ChooseStartingPlayerResp` |
| `Mulligan { .. }` (+ follow-up `SelectCards{BottomForMulligan}`) | `MulliganReq` (`mulliganType`,`freeMulliganCount`,`mulliganCount`) | `MulliganResp` (`MulliganOption`) |
| `ChooseTargets { for_action, slots }` | `SelectTargetsReq` | `SelectTargetsResp` |
| `ChooseModes { .. }` | inner `modalReq` of `CastingTimeOptionsReq` | `CastingTimeOptionsResp` |
| `CastingTimeOptions { for_action, options }` | `CastingTimeOptionsReq` (`CastingTimeOptionType`; embeds `numericInputReq`/`modalReq`/`selectNReq`) | `CastingTimeOptionsResp` |
| `ChooseNumber { reason, min, max, forbidden }` | `NumericInputReq` | `NumericInputResp` |
| `Distribute { .. }` | `DistributionReq` (`minAmount`,`maxAmount`,`minPerTarget`,`targetIds`) | `DistributionResp` |
| `PayCost { cost, mana_sources, non_mana }` | `PayCostsReq` | `PerformActionResp` / `EffectCostResp` (`Make_Payment`/`Activate_Mana`/`FloatMana`/`Special_Payment`) |
| `DeclareAttackers { eligible }` | `DeclareAttackersReq` | `DeclareAttackersResp` |
| `DeclareBlockers { eligible, attackers }` | `DeclareBlockersReq` | `DeclareBlockersResp` |
| `AssignCombatDamage { .. }` | `AssignDamageReq` (order via `OrderReq`) | `AssignDamageResp` |
| `OrderObjects { kind, items }` | `OrderReq` (`OrderCombatDamageReq` folds in here) | `OrderResp` (`ids[]`,`ordering`) |
| `SelectCards { reason, from, min, max, filter }` | `SelectNReq` / `SearchReq` / `RevealHandReq` | `SelectNResp` (`ids[]`) / `SearchResp` / `RevealHandResp` |
| `SelectFromGroups { reason, groups }` | `SelectNGroupReq` / `SelectFromGroupsReq` / `GroupReq` | `SelectNGroupResp` / `SelectFromGroupsResp` / `GroupResp` |
| `ArrangeCards { reason, cards, destinations }` | scry/surveil (via `SelectN`/`Order`) | `SelectNResp` / `OrderResp` |
| `ChooseReplacement { event, applicable }` | `SelectReplacementReq` | `SelectReplacementResp` |
| `ChooseCounterType { options }` | `SelectCountersReq` | `SelectCountersResp` |
| `ChooseOption { reason, options, min, max }` | `PromptReq` (via `Prompt` header) / `StringInputReq` | `StringInputResp` / option resp |
| `ChooseColor { allowed, min, max }` | choose-option-from-list prompt | option resp |
| `Confirm { kind }` | `OptionalActionMessage` / `PromptReq` | `OptionalResp` (`OptionResponse`) |
| (push, no response) state delta | `GameStateMessage{ Full \| Diff }` / `BinaryGameState` | — |
| (push, no response) reveal / UI | `UIMessage` / `TimerStateMessage` / `IntermissionReq` | (none / ack) |

`DecisionResponse` is **selection-into-options** (AGENT_INTERFACE §4: `Pass`/`Index`/`Indices`/
`Number`/`Bool`/`Pairs`/`Amounts`/`Order`/`Arrangement`/`Payment`/`Action`). The GRE server
translates those selections back into the concrete GRE response payloads the protocol expects
(object ids, target maps, damage splits, payment specs). Selection-based responses keep the
web client and the RL policy structurally identical on the answer side — both only ever pick
among engine-enumerated legal options.

**Field-level shapes are now recovered & log-validated** (`../mtga-re/schema/gre_schema.json`:
254 messages, 135 enums, 0 mismatches vs a 2134-message live log). The earlier open items are
resolved:
- *Mulligan/London bottoming:* `MulliganResp` carries only a `MulliganOption` decision →
  bottoming is a follow-up selection, confirming `Mulligan` + `SelectCards{BottomForMulligan}`.
- *Numeric:* `NumericInputReq` carries the bound/constraint fields (min/max + step/disallow) →
  `ChooseNumber.forbidden` maps to `disallowedValues` (design enriched `ChooseNumber` to match).
- *Cast granularity (the round-trip knob):* **resolved** — GRE's `CastingTimeOptionsReq`
  *embeds* `numericInputReq`/`modalReq`/`selectNReq` as inner messages, i.e. the wire itself
  decomposes a cast's options into our `ChooseNumber`/`ChooseModes`/`SelectCards` sub-steps. So
  our CR 601.2 *sequence* model (§11) is exactly what GRE does; a cast is one
  `CastingTimeOptionsReq` envelope whose inner reqs the UI walks through.
- *Combat damage:* `AssignDamageReq` + ordering via `OrderReq` (no separate
  `OrderCombatDamageReq` message in this build) → `AssignCombatDamage` + `OrderObjects`.

The variant set was a correct superset all along; the mapping is now field-exact. (Tracked the
same in AGENT_INTERFACE §9, which `design` marked RESOLVED against the recovered schema.)

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

### Strategy A — Protocol-compatible server + match-push redirect (no binary modification)
Make `mtg-gre-server`'s TLS-over-TCP transport byte-compatible with MTGA's real GRE server
(§4.2), then get the client to connect to us. Because the GRE endpoint is **dynamic** — the
match/front-door push hands the client `MatchInfoV3.MatchEndpointHost/Port` + `MatchId` (§4.3) —
redirection means controlling *that push*, not editing a baked-in address.

- **Pros:** client stays *stock* (no ToS-fraught binary modification); survives client updates
  as long as the GRE protocol is stable; the cleanest expression of "the seam is the protocol."
- **What the recovered transport facts (§4.3/§4.4) make tractable:**
  - **No GRE-session token to forge.** `ConnectReq` is tokenless; auth binds upstream at the
    match service. So we don't need WotC's signing keys to open a GRE session — we need the
    client to *reach* our endpoint.
  - **TLS is solvable without a pinning bypass.** The client validates the server cert against
    *the host the push told it to use* (standard hostname/chain validation, §4.2). Since we
    supply that host, push a hostname we hold a cert for and install a local dev-CA root in the
    Mono/system trust store — default validation then passes, stock binary untouched.
- **Cons / what we must stand up:**
  - **A stand-in front-door/match service.** To originate the push, we run (or intercept) the
    match-service handshake (`ClientToMatchDoorConnectRequest` → a push carrying our
    `MatchEndpointHost/Port`). This is the real scope of Strategy A: a small fake matchmaking
    shim, not just a GRE server. (Alternatively, a MITM proxy that rewrites the real push's
    endpoint fields — but that re-introduces a TLS-interception problem on the match channel.)
  - **Service-mesh isolation.** The client also talks to login/assets/telemetry; the shim must
    satisfy enough of the pre-match flow to reach "match found" and emit the push.
- **Net:** viable as a *local fake front-door + GRE server*; the former blockers (token, cert
  pinning) are no longer hard walls given §4.3/§4.4. The remaining effort is the matchmaking
  shim, not protocol forgery.

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
**Attempt Strategy A first** (no client modification; §4 shows it's no longer wall-blocked by
token/pinning) — its real cost is a small fake front-door that emits the match push to our
endpoint. If standing up the matchmaking shim proves fiddly, or a future build hard-pins the
cert, **fall back to Strategy B's runtime hook** (a `BepInEx`/`MonoMod` plugin hooking
`TcpConnection.Connect`/the cert callback) as the least-invasive patch — it sidesteps the shim
entirely by pointing the client straight at our GRE server. Both consume the same
protocol-compatible server; the only difference is whether the client is persuaded to talk to us
by a fake push or by a hook. Either way the server and engine are unchanged: drop-in = getting
the client to connect to our endpoint.

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
   protobuf** message set — `prost` codegen straight from `../mtga-re/proto/*.proto` (already
   recovered + log-validated); emit real `GREToClientMessage`/`GameStateMessage`, consume
   `ClientToGREMessage`. Front end switches to protobuf-over-WS (`ts-proto`). Re-validate with
   `../mtga-re/bin/validate-logs` against captured Detailed-Logs streams. Exit: our web client
   plays a full game speaking *real GRE messages*; recorded MTGA messages parse/serialize
   identically.

4. **Attempt real-client drop-in.** Stand up the TLS-over-TCP transport with MTGA's exact
   6-byte framing + ping/pong keepalive + the `ConnectReq`/`ConnectResp` handshake (§4.2/§4.3),
   plus a minimal fake front-door that emits the match push to our endpoint. Try **Strategy A**
   (match-push redirect + locally-trusted cert); if the matchmaking shim is fiddly or a build
   hard-pins, fall back to **Strategy B** (BepInEx/MonoMod hook of `TcpConnection.Connect`/the
   cert callback). Exit: the stock MTGA client renders and plays at least the opening of a game
   driven by `mtg-core`.

Milestones 1–2 depend only on `mtg-core` (#1) + `AGENT_INTERFACE.md` (#3). Milestone 3 needs
the recovered schema (#2 — **done**: `../mtga-re/proto` + `schema`). Milestone 4 additionally
needs the transport capture (#6) — most of which is already extracted into §4 here.

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

- **Auth & cert pinning (the drop-in gate) — DE-RISKED (§4.3/§4.4).** Recovered facts: the GRE
  `ConnectReq` is tokenless (auth binds upstream), and TLS uses standard hostname/chain
  validation against the *push-supplied* host (not opaque pinning). So Strategy A's real cost is
  a fake front-door/match shim, not token forgery or a pinning bypass; budget for falling back
  to the Strategy B hook if the shim is fiddly.
- **Protocol drift.** MTGA updates can change the GRE schema/framing; pin the version
  (`2026.59.30.12801`, build-guid `7ad31cf…`) and treat the recovered schema (`../mtga-re`) as a
  snapshot. Frame version is currently `4`. The web client (milestones 1–3) is insulated from
  drift; only milestone 4 isn't.
- **Composite vs. atomic decisions — RESOLVED.** See §5: GRE's `CastingTimeOptionsReq` embeds
  `numericInputReq`/`modalReq`/`selectNReq`, so our CR 601.2 *sequence* of `decide()` calls is
  exactly the wire's own decomposition (the adapter walks the inner reqs). `GreSessionAgent` may
  skip the wire round-trip for a lone-option step (answer `Index(0)` locally) but must **not**
  *elide the decision* — whether a forced single-option decision is issued at all is an
  engine/Arena-profile concern (auto-pass "stops"), uniform across all backends so the decision
  log replays identically (differential-testing + replay, ENGINE_PLAN §8, AGENT_INTERFACE §8.1).
- **State-diff fidelity.** Producing correct `GameStateMessage` *diffs* (not just `Full`) from
  `GameEvent`s is non-trivial; start `Full`-only for our web client, add diffs for the
  real-client transport (the real client likely expects diffs).
- **Scope creep into the service mesh.** Strategy A may drag in login/matchmaking stubs; keep
  that out of `mtg-gre-server` (a separate experiment) so the web client stays clean.
```
