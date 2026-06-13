# MTGA Client<->Server Decision Protocol — Decompile & Schema-Recovery Plan

Status: **PLAN ONLY** (read-only recon done; no decompilation performed yet).
Goal: Recover the authoritative GRE (Game Rules Engine) message schema from the
real MTGA client so the Rust engine (`mtgenv`) agent-decision interface can
*mirror* it, making a later switch to driving the real client an
implementation-swap-only change.

All decompile work will live in a **separate repo** at
`/home/xander/dev/p-mtg/mtga-re` (to be created in Phase 0; does **not** exist yet).
This repo (`mtgenv`) only gets the generated schema + the Rust trait design.

---

## 1. Recon findings

### Install location (Steam / Proton)
MTGA is a **Steam** title (appid `2141910`); launcher
`/home/xander/.local/share/applications/Magic The Gathering Arena.desktop`
→ `Exec=steam steam://rungameid/2141910`. The game binaries live in the Steam
common dir (not in the Proton prefix under `compatdata/2141910`):

```
/home/xander/.local/share/Steam/steamapps/common/MTGA/
├── MTGA.exe                     (666 KB)
├── UnityPlayer.dll              (31 MB)   Unity 2022.3.62 runtime
├── MonoBleedingEdge/                      <-- Mono runtime shipped with game
└── MTGA_Data/
    ├── Managed/                 (228 *.dll managed assemblies)
    ├── boot.config             build-guid=7ad31cfde8ac41bcbc1f0d975465aced
    ├── globalgamemanagers*, level*, *.assets, resources.assets*
    └── Logs/Logs/UTC_Log - *.log
```

### Mono vs IL2CPP — VERDICT: **Mono** (definitive)
Evidence:
- `MTGA_Data/Managed/Assembly-CSharp.dll` **EXISTS** (332,800 bytes) — the Mono marker.
- `MTGA_Data/il2cpp_data/Metadata/global-metadata.dat` — **ABSENT**.
- No `GameAssembly.dll` / `GameAssembly.so` anywhere — **ABSENT** (IL2CPP marker absent).
- `MonoBleedingEdge/` runtime directory present.
- `file Assembly-CSharp.dll` → `PE32 ... Mono/.Net assembly`;
  `file Wizards.MDN.GreProtobuf.dll` → `PE32+ ... x86-64 Mono/.Net assembly`.

This is the **easy path**: managed CIL assemblies decompile directly to readable
C#. No Il2CppDumper / global-metadata reconstruction needed. The plan below is
written for the Mono path; an IL2CPP fallback is noted only in case a future
client update switches runtimes.

### Protocol = protobuf (definitive)
- `Wizards.MDN.GreProtobuf.dll` (1,428,480 bytes) — generated protobuf message types for the GRE.
- `Google.Protobuf.dll` (401,576 bytes) — Google.Protobuf runtime (proto3, C# codegen).
- `Wizards.Arena.Models.Protobuf.dll` (395,264 bytes) — additional protobuf models.
- `Wizards.Arena.MessageSerialization.dll`, `Wizards.Arena.TcpConnection.dll` — transport/serialization.
- No `protobuf-net`; this is **Google.Protobuf (proto3) C# generated code**, which
  means classes carry `Parser`, `Descriptor`, `*FieldNumber` constants, and the
  field/enum layout is fully recoverable from the generated reflection metadata.

### Key managed DLL inventory (`MTGA_Data/Managed/`, 228 DLLs total)
Network/message/game-logic relevant subset (size, name):

| Size      | DLL                                         | Role |
|-----------|---------------------------------------------|------|
| 12.3 MB   | `Core.dll`                                  | Card/game data definitions (huge; serialized type metadata) |
| 1.51 MB   | `SharedClientCore.dll`                      | Shared client game logic |
| **1.43 MB** | **`Wizards.MDN.GreProtobuf.dll`**         | **PRIMARY TARGET — GRE protobuf messages** |
| 691 KB    | `Newtonsoft.Json.dll`                       | (JSON; some logs are JSON) |
| 401 KB    | `Google.Protobuf.dll`                       | proto3 runtime |
| 395 KB    | `Wizards.Arena.Models.Protobuf.dll`         | Secondary protobuf models |
| 332 KB    | `Assembly-CSharp.dll`                        | Game-specific glue / managers |
| 143 KB    | `Wizards.Arena.Models.dll`                  | Domain models |
| 110 KB    | `Wizards.Arena.DeckValidation.Core.dll`     | Deck/format rules |
| 49 KB     | `Wizards.Arena.Enums.dll`                   | Shared enums |
| 37 KB     | `Wizards.Mtga.Interfaces.dll`               | Interfaces |
| 27 KB     | `Wizards.Arena.TcpConnection.dll`           | TCP transport |
| 12 KB     | `Wizards.Arena.MessageSerialization.dll`    | Wire (de)serialization |
| 44 KB     | `MQTTnet.Extensions.ManagedClient.dll`      | MQTT (matchmaking/queue transport) |
| 557 KB    | `Facepunch.Steamworks.Win64.dll`            | Steam integration |

### Version
- **MTGA client version: `2026.59.30.12801`** (from newest
  `MTGA_Data/Logs/Logs/UTC_Log - 06-07-2026 01.25.13.log`:
  `Version: 2026.59.30.12801 / 2026.59.30.12801.12`).
- Unity engine: `2022.3.62` (`MTGA.exe` FileVersion `2022.3.62.7762112`).
- `build-guid=7ad31cfde8ac41bcbc1f0d975465aced` (boot.config) — pin this in the recovered schema dir for provenance.

### Confirmed message/decision type names (via `strings` on `Wizards.MDN.GreProtobuf.dll`)
Top-level envelopes confirmed: `GREToClientMessage`, `ClientToGREMessage`,
`GameStateMessage` (field `gameStateMessages_`, `GameStateType` ∈
`{None, Full, Diff, Binary}`), `ClientToMatchServiceMessage`.

Full `GREMessageType` enum (the **server→client request catalog** — what the GRE
asks the agent to decide), recovered:

```
None, ConnectResp, GameStateMessage, QueuedGameStateMessage, BinaryGameState,
DieRollResultsResp, GetSettingsResp, SetSettingsResp,
ChooseStartingPlayerReq, MulliganReq, SubmitDeckReq, SubmitDeckConfirmation,
ActionsAvailableReq, DeclareAttackersReq, SubmitAttackersResp,
DeclareBlockersReq, SubmitBlockersResp, AssignDamageReq, AssignDamageConfirmation,
OrderCombatDamageReq, OrderDamageConfirmation, OrderReq,
SelectTargetsReq, SubmitTargetsResp, SelectNReq, SelectNGroupReq,
SelectCountersReq, SelectReplacementReq, SelectFromGroupsReq,
SearchReq, SearchFromGroupsReq, GroupReq, DistributionReq,
PayCostsReq, CastingTimeOptionsReq, NumericInputReq, StringInputReq,
PromptReq, RevealHandReq, IntermissionReq, GatherReq, AllowForceDraw,
OptionalActionMessage, EdictalMessage, IllegalRequest, TimeoutMessage,
TimerStateMessage, UIMessage, PredictionResp
```

`ActionType` enum (client→GRE chosen actions) includes: `Pass`, `Play`, `Cast`
(+ `CastAdventure/CastMDFC/CastLeft/CastRight/CastOmen/CastPrototype/CastSuspended/
CastCommanderFromCommandZone/CastWithoutPayingManaCost`), `Activate`,
`Activate_Mana`, `FloatMana`, `Make_Payment`, `Special`, `Special_Payment`,
`Special_TurnFaceUp`, `OpeningHandAction`, `CombatCost`, `ResolutionCost`.
`CastingTimeOptionType` covers X/Bargain/Casualty/Conspire/Kicker-style choices.

→ This is exactly the decision surface we need to mirror: mulligan, declare
attackers/blockers, select targets, order triggers/damage, pay costs, choose
numbers/X, distribute, search, group/order, choose starting player.

### Complementary validation source (side note only)
The user runs an MTGA log follower (`/home/xander/.mtga_follower.ini` present;
contains a client `token`). MTGA's "Detailed Logs (Plugin Support)" option emits
the **live GRE message stream** into `Player.log` / `UTC_Log` as JSON
(`GreToClientEvent` / `ClientToGreMessage` blocks). This is NOT the chosen
approach (we do full IL decompile for the authoritative schema), but the captured
log stream is an excellent **ground-truth validator**: round-trip recorded
messages through the recovered schema to confirm field numbers/oneofs/enums.

### Tool availability
- `dotnet` → present at `/home/xander/.dotnet/dotnet` (**8.0.406**). `ilspycmd`
  installable via `dotnet tool install -g ilspycmd`. **Not yet installed.**
- `java` → present (`openjdk 21.0.11`). Available for AvaloniaILSpy / JADX-style tools if needed.
- `mono` → **MISSING** (`mono not found`). Not required: ilspycmd runs on
  .NET 8, and the target DLLs are read as bytes, not executed.
- `ilspycmd` → **not installed** (install in Phase 0).
- No IL2CPP tooling needed (Mono confirmed).

---

## 2. Toolchain

### Primary (Mono path — what we will use)
- **ilspycmd** (ILSpy CLI, runs on installed .NET 8):
  ```
  /home/xander/.dotnet/dotnet tool install -g ilspycmd
  # ensures ~/.dotnet/tools is on PATH
  export PATH="$PATH:$HOME/.dotnet/tools"
  ilspycmd --version
  ```
  Use it to (a) decompile whole assemblies to C# projects, and (b) list types.
  ```
  # full C# decompile of the GRE protobuf assembly into a project tree
  ilspycmd -p -o ./out/GreProtobuf  Wizards.MDN.GreProtobuf.dll
  # plus the model/transport assemblies
  ilspycmd -p -o ./out/Models       Wizards.Arena.Models.Protobuf.dll
  ilspycmd -p -o ./out/Enums        Wizards.Arena.Enums.dll
  ```
- **GUI cross-check (optional):** AvaloniaILSpy (cross-platform ILSpy GUI) or
  dnSpyEx (Windows/Wine) for interactive browsing of a tricky oneof. Optional,
  not required for the batch extraction.

### Protobuf schema recovery (the important part)
Google.Protobuf C# codegen embeds a **serialized FileDescriptorProto** in each
generated `*Reflection` class (a base64/byte[] blob fed to
`FileDescriptor.FromGeneratedCode`). Recovering `.proto` is therefore two
independent routes — do **both** and reconcile:
1. **Descriptor route (authoritative):** locate the embedded descriptor byte[]
   in the decompiled `*Reflection` classes, decode it with `protoc`/`protobuf`
   tooling back into `.proto`. Tools:
   - `protoc` (e.g. `protobuf` package) to round-trip a `FileDescriptorSet`.
   - or a tiny .NET helper that calls
     `FileDescriptor.FromGeneratedCode(...)` then walks
     `descriptor.Proto` → write `.proto` (most robust; reuses the same
     `Google.Protobuf.dll` we extracted).
2. **Source route (cross-check):** read the decompiled C# message classes for
   `*FieldNumber` constants, field C# types, `oneof` cases, and `[Original
   Name]` enum attributes; emit `.proto` from those. Confirms route 1.

### IL2CPP fallback (NOT needed now — documented for future client updates)
If a future MTGA build ships IL2CPP (`GameAssembly.so` + `global-metadata.dat`
appear, `Managed/` disappears):
- **Il2CppDumper** or **Il2CppInspector** (both run on .NET / mono) consuming
  `GameAssembly.so` + `il2cpp_data/Metadata/global-metadata.dat` to reconstruct
  type/method metadata and (with Il2CppInspector) a Ghidra/IDA script.
  This is a much heavier workflow (no method bodies as clean C#; protobuf
  descriptors must be carved from `.data`). Re-evaluate the plan if this happens.

---

## 3. Target artifacts (what to extract, and how to find it)

### Tier 1 — Wire envelopes (must-have)
From `Wizards.MDN.GreProtobuf.dll`:
- `GREToClientMessage` — server→client; carries `greToClientMessages_`
  (repeated `GreToClientMessage`), each tagged by `GREMessageType`.
- `ClientToGREMessage` — client→server; carries chosen actions/responses.
- `ClientToMatchServiceMessage` (wraps `ClientToGREMessage` for transport).
- `GameStateMessage` (+ `GameStateType {None,Full,Diff,Binary}`), and the object
  model it references: `GameObject`, `Annotation`, `ZoneInfo`, `PlayerInfo`,
  `TurnInfo`, `Players`, `GameInfo`, `Zone`, etc.

### Tier 2 — Decision request/response pairs (the agent interface surface)
Each "Req" is a server prompt; each matching "Resp"/results is the agent's
answer. Extract message + nested option types for:
- **Mulligan:** `MulliganReq` → `MulliganResp`.
- **Choose starting player:** `ChooseStartingPlayerReq`.
- **Declare attackers:** `DeclareAttackersReq` → `SubmitAttackersResp`
  (`DeclareAttackers*` results, attacker→defender + requirements).
- **Declare blockers:** `DeclareBlockersReq` → `SubmitBlockersResp`.
- **Combat damage:** `AssignDamageReq`/`AssignDamageConfirmation`,
  `OrderCombatDamageReq`/`OrderDamageConfirmation`.
- **Targets:** `SelectTargetsReq` → `SubmitTargetsResp` (target maps/criteria).
- **Order triggers/effects:** `OrderReq` → `OrderResp`.
- **Select N / groups / counters / replacement / from-groups:** `SelectNReq`,
  `SelectNGroupReq`, `SelectCountersReq`, `SelectReplacementReq`,
  `SelectFromGroupsReq`, `GroupReq`/`GroupResp`, `DistributionReq`.
- **Search:** `SearchReq`, `SearchFromGroupsReq`.
- **Pay costs / casting-time options:** `PayCostsReq`, `CastingTimeOptionsReq`
  (X / kicker / additional / alternative costs via `CastingTimeOptionType`).
- **Numeric / string / prompt / reveal / gather:** `NumericInputReq`,
  `StringInputReq`, `PromptReq`, `RevealHandReq`, `GatherReq`,
  `ActionsAvailableReq`, `AllowForceDraw`, `OptionalActionMessage`.

### Tier 3 — Action vocabulary & enums
- `GameAction` / the action submitted in `ClientToGREMessage` carrying
  `ActionType` (Pass/Play/Cast.../Activate.../Make_Payment/Special...).
- All enums: `GREMessageType`, `ActionType`, `GameStateType`, `AbilityType`,
  `AnnotationType`, `CastingTimeOptionType`, zone/phase/step/visibility enums.
  (Many live in `Wizards.Arena.Enums.dll` and inside GreProtobuf.)

### How to find them (mechanical)
- Namespace sweep: in the decompiled tree, list all types under the GRE protobuf
  namespace; group by `*Req` / `*Resp` / `*Message` / `*Type` suffix.
- String/symbol search already validated above
  (`grep -oE "GREMessageType_[A-Za-z]+"`, `ActionType_*`, `*FieldNumber`).
- For each `*Req`, find its handler/usage in `SharedClientCore.dll` /
  `Assembly-CSharp.dll` to learn which `*Resp` it expects (confirms pairing).

---

## 4. Extraction workflow (step-by-step)

### Phase 0 — Set up `../mtga-re` repo (do later, not now)
```
/home/xander/dev/p-mtg/mtga-re/
├── README.md                # provenance: MTGA version, build-guid, date, scope
├── .gitignore               # ignore vendored DLLs + raw decompiled C# (do NOT commit Wizards' code)
├── bin/                     # small helper executables, each its own build target
│   ├── descriptor-dump/     # .NET tool: FileDescriptor -> .proto
│   └── schema-emit/         # emit machine-readable JSON schema from .proto/descriptors
├── input/                   # (gitignored) copies of the target DLLs for offline work
├── decompiled/              # (gitignored) ilspycmd C# output
├── proto/                   # recovered .proto files  (DERIVED, keep — interop spec)
├── schema/                  # gre_schema.json  (canonical machine-readable schema)
└── docs/                    # notes, message-pairing table, enum tables
```
Provenance note in README: client `2026.59.30.12801`, Unity `2022.3.62`,
`build-guid 7ad31cfde8ac41bcbc1f0d975465aced`, extracted-on date, source path
`/home/xander/.local/share/Steam/steamapps/common/MTGA/MTGA_Data/Managed`.

### Phase 1 — Stage inputs (read-only copy)
Copy (don't edit in place) the Tier-1/2/3 DLLs into `input/`:
`Wizards.MDN.GreProtobuf.dll`, `Wizards.Arena.Models.Protobuf.dll`,
`Wizards.Arena.Models.dll`, `Wizards.Arena.Enums.dll`, `Google.Protobuf.dll`,
`SharedClientCore.dll`, `Assembly-CSharp.dll`,
`Wizards.Arena.MessageSerialization.dll`, `Wizards.Arena.TcpConnection.dll`.

### Phase 2 — Decompile to C#
`ilspycmd -p -o decompiled/<name> input/<name>.dll` for each. Produces readable
C# incl. the protobuf `*Reflection` classes with embedded descriptors.

### Phase 3 — Recover `.proto` (descriptor route, authoritative)
`bin/descriptor-dump` (small .NET 8 console app, references the extracted
`Google.Protobuf.dll`): for each generated `*Reflection` type, read its
`Descriptor` / call `FileDescriptor.FromGeneratedCode`, then serialize
`descriptor.ToProto()` into a `FileDescriptorSet`; run `protoc --decode_raw` or a
descriptor→.proto printer to write `proto/*.proto`. Reconcile against the
source-route reading of `*FieldNumber` constants (Phase 3b cross-check). Output:
complete `.proto` with messages, oneofs, enums, field numbers.

### Phase 4 — Emit canonical machine-readable schema
`bin/schema-emit` produces `schema/gre_schema.json`: a flat, language-neutral
description (message name, fields[{name, number, type, label, oneof_group},...],
enums[{name, values[{name, number}]}], and the Req↔Resp pairing table). This
JSON is the contract the Rust side consumes. Keep `.proto` too (human spec +
future `prost`/`tonic` codegen if we ever speak the wire directly).

### Phase 5 — Validate against captured logs
Enable MTGA "Detailed Logs", capture a few games via the existing follower,
extract `GreToClientEvent`/`ClientToGreMessage` JSON, and round-trip them through
the recovered schema (field numbers + enum names must match). Discrepancy ⇒ fix
the schema. This closes the loop without reverse-engineering the live TLS stream.

### Phase 6 — Generate the Rust mirror (lands in `mtgenv`)
From `schema/gre_schema.json`, generate Rust types and the agent trait (Section 5)
into `mtgenv` (e.g. `src/protocol/` mirror types + `src/agent.rs` trait). Either
hand-author the trait and codegen the data enums, or `prost` from `.proto` for a
1:1 wire-compatible layer behind the trait.

---

## 5. Mapping to the engine's agent interface

### The pattern to mirror (MTGA's own GRE protocol)
The drop-in target *is* the reference. The recovered GRE `*Req`/`*Resp` catalog
(see `../mtga-re/schema/gre_schema.json` and `../mtga-re/docs/GRE_DECISIONS.md`)
is itself an **abstract decision interface**: at every point a player must choose,
the GRE server emits a `*Req` that pre-enumerates the legal options, and the client
replies with the matching `*Resp`. The catalog covers exactly the choices the
engine must surface — `MulliganReq`, `ChooseStartingPlayerReq`,
`DeclareAttackersReq`/`DeclareBlockersReq`, `SelectTargetsReq`, `AssignDamageReq`,
`OrderReq` (triggers/blockers/combat-damage order), `CastingTimeOptionsReq`
(X/kicker/alt+additional costs), `SelectNReq`/`DistributionReq`/`NumericInputReq`,
`OptionalActionMessage` (confirm), and the `ActionsAvailableReq` → `PerformActionResp`
priority loop. Crucially, **MTGA already does the legal-option masking server-side
and routes every choice through a single decision boundary** — e.g.
`DeclareAttackersReq` ships `qualifiedAttackers` + per-creature `legalDamageRecipients`
+ `mustAttack`. That is exactly the swap-by-implementation shape `mtgenv` wants, and
it independently validates our `Agent`/`DecisionRequest` design: mirror the GRE
catalog 1:1 and the engine's built-in AI, a Python RL agent, and the real MTGA
client all become interchangeable backends behind one seam.

### Rust design (lands in `mtgenv`)
Define **one decision-request enum and one decision-response enum** that mirror
MTGA's `*Req`/`*Resp` pairs, and a single trait the engine calls:

```rust
// src/protocol/  — mirrors GreToClientMessage / ClientToGreMessage exactly (from schema)
pub enum DecisionRequest {            // <- MTGA *Req types
    Mulligan(MulliganReq),
    ChooseStartingPlayer(ChooseStartingPlayerReq),
    DeclareAttackers(DeclareAttackersReq),
    DeclareBlockers(DeclareBlockersReq),
    SelectTargets(SelectTargetsReq),
    AssignDamage(AssignDamageReq),
    OrderTriggers(OrderReq),
    PayCosts(PayCostsReq),
    CastingTimeOptions(CastingTimeOptionsReq),
    SelectN(SelectNReq),
    Distribution(DistributionReq),
    NumericInput(NumericInputReq),
    Prompt(PromptReq),
    // ...one variant per GREMessageType *Req
}
pub enum DecisionResponse {           // <- MTGA *Resp / submitted GameAction
    Mulligan(MulliganResp),
    SubmitAttackers(SubmitAttackersResp),
    SubmitBlockers(SubmitBlockersResp),
    SubmitTargets(SubmitTargetsResp),
    OrderResp(OrderResp),
    PerformAction(GameAction /* ActionType + payload */),
    // ...
}

/// The single seam. Engine calls this; nothing else knows who decides.
pub trait Agent {
    fn decide(&mut self, game: &GameView, req: &DecisionRequest) -> DecisionResponse;
}
```

Three interchangeable implementations behind `trait Agent` (the whole point):
- **(a) Rust engine / built-in AI** — `RuleBasedAgent`, in-process.
- **(b) Python RL agent** — `SocketAgent`: serialize `GameView` + `DecisionRequest`
  to JSON/proto over a socket and block for `DecisionResponse` (the Gym env drives it
  from the Python side; see `GYM_PLAN.md`).
- **(c) Real MTGA client** — `MtgaClientAgent`: translate `DecisionRequest` →
  the actual `GREToClientMessage`/`ClientToGREMessage` protobuf and drive the
  live client. Because the Rust enums were **generated from the recovered MTGA
  schema**, this implementation is a thin (de)serialization adapter — no engine
  changes. **Swap = construct a different `Box<dyn Agent>`.**

Keeping `DecisionRequest`/`DecisionResponse` structurally identical to
`GREToClientMessage`/`ClientToGREMessage` (same field names/numbers/enums) is what
makes (c) "implementation-swap-only".

---

## 6. Legal / ToS considerations

- The MTGA EULA / WotC Fan Content Policy prohibit redistributing Wizards'
  code, assets, or client binaries, and prohibit cheating / unfair automated
  play against other players.
- This effort is **personal research and interoperability** only: recovering a
  message-schema description to design a compatible interface — not redistributing
  Wizards' software.
- Do **not** commit or publish decompiled C# source, the DLLs, or game assets.
  Keep `input/` and `decompiled/` git-ignored; only the **derived** interop
  artifacts (`.proto`, `schema.json`) and our own code are version-controlled.
- Reverse engineering for interoperability is the recognized rationale here;
  keep the scope to the protocol schema and avoid anything that automates
  ranked/online play against other humans.
- Treat the recovered schema as a private spec; the goal is a clean-room-style
  Rust interface, not a re-host of MTGA.

---

## 7. Phasing & effort estimate

| Phase | Work | Effort |
|------:|------|--------|
| 0 | Create `../mtga-re` repo skeleton, install `ilspycmd`, PATH | 0.5 day |
| 1 | Stage input DLLs, write README provenance | 0.5 day |
| 2 | `ilspycmd` decompile of Tier-1/2/3 assemblies | 0.5 day |
| 3 | Recover `.proto` (descriptor dump tool + source cross-check) | 2–3 days |
| 4 | `schema/gre_schema.json` emitter + Req↔Resp pairing table | 1–2 days |
| 5 | Validate vs captured Detailed-Logs GRE stream | 1–2 days |
| 6 | Generate Rust mirror types + `trait Agent` into `mtgenv`; wire built-in agent | 2–3 days |
| 7 | (later) Python `SocketAgent` + real-client `MtgaClientAgent` adapters | open-ended |

**Headline effort to a usable, validated schema + Rust trait (Phases 0–6):
~1.5–2 weeks.** The Mono+protobuf combination is the best-case scenario; the
embedded proto descriptors make schema recovery largely mechanical rather than
guesswork. Highest-risk step is Phase 3 (descriptor decoding / oneof fidelity),
de-risked by the dual descriptor+source routes and the Phase-5 log validation.

---

### Appendix — exact paths
- Install: `/home/xander/.local/share/Steam/steamapps/common/MTGA/`
- Managed DLLs: `…/MTGA/MTGA_Data/Managed/`
- Primary target: `…/Managed/Wizards.MDN.GreProtobuf.dll`
- Logs (validator): `…/MTGA/MTGA_Data/Logs/Logs/UTC_Log - *.log`
- Decompile repo (to create): `/home/xander/dev/p-mtg/mtga-re/`
- Engine to mirror into: `/home/xander/dev/p-mtg/mtgenv/` (`src/`)
- Recovered schema/transport (the reference): `/home/xander/dev/p-mtg/mtga-re/schema/gre_schema.json`, `…/mtga-re/docs/{GRE_DECISIONS,REQ_RESP_FIELDS,transport}.md`
- Tooling: `/home/xander/.dotnet/dotnet` (8.0.406); install `ilspycmd` (`dotnet tool install -g ilspycmd`); `mono` not needed.
