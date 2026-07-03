// mtgenv web client (TS/Vite) — MTGO-style board over the JSON projection of the Agent boundary.
// Renders real MTG card frames (name, mana, art placeholder, type line, rules, P/T), the
// information-filtered PlayerView, graveyard/exile piles, and the engine's masking: legal cards
// glow and are click-to-act. (Kept in sync with the no-build embedded client in server.rs.)

const $ = (id: string): HTMLElement => document.getElementById(id) as HTMLElement;

type Any = any; // the wire types are the boundary's serde JSON; kept loose on purpose.

let view: Any = null;
let cur: Any = null;
let lastTurn = 0;
const multi = new Set<number>();
let orderSeq: number[] = [];
let stopsView: Any = null; // live stop config echoed by the server
let autoPassTurn: number | null = null; // Enter-hold: turn we're passing all priority stops through
const deckView: Any = {}; // seat → starting decklist (debug peek; RL-safe, pushed once at setup)

// Card art: a baked manifest (grp_id → art_crop/artist), batch-resolved from Scryfall once.
// No runtime Scryfall API calls — we only load the cached CDN images.
let artMap: Any = {};
fetch("/card-art.json").then((r) => r.json()).then((m) => { artMap = m; render(); }).catch(() => {});

const params = new URLSearchParams(location.search);
const gameId = params.get("game");
const replayId = params.get("replay");
const spectating = params.get("spectate") === "1" || params.get("spectate") === "true";
// God-view (no information masking): replays and spectating (the server feeds spectators
// omniscient god frames, so both hands + ordered libraries render — spectators aren't players).
let godMode = !!replayId || spectating;
$("decks").textContent = replayId
  ? `▶ Replay #${replayId}`
  : spectating
    ? `👁 Spectating Game #${gameId}`
    : gameId
      ? `Game #${gameId} · you are Player ${params.get("seat") || "0"}`
      : `P0=${params.get("p0") || "demo"} · P1=${params.get("p1") || "demo"}`;
if (spectating) $("prompt").innerHTML = '<div class="waiting">👁 Spectating — watching the game (you are not playing).</div>';
if (replayId) $("prompt").innerHTML = '<div class="waiting">▶ Replay — god-view playback (no hidden information). Use the bar below.</div>';
// Stops control (top bar): LIVE toggles — send setOption; the server echoes the new config and
// the running game's agent honours it at the next window (no reset). Rendered from stopsView.
// The only user-facing stop toggle is Full Control (stop at every priority window). The former
// auto-pass / smart / resolve-own knobs were collapsed into it engine-side; OFF = the engine's fixed
// `should_auto_pass` elision rule.
const OPT: Array<[string, string, string]> = [
  ["full ctrl", "full_control", "fullcontrol"],
];
function renderStopsControl(): void {
  const host = $("stops"); host.innerHTML = "";
  if (!stopsView) return;
  host.appendChild(document.createTextNode("stops: "));
  OPT.forEach(([label, field, key], i) => {
    if (i) host.appendChild(document.createTextNode(" · "));
    const a = el("a", undefined, `${label}: ${stopsView[field] ? "on" : "off"}`) as HTMLAnchorElement;
    a.href = "#";
    a.onclick = (e) => { e.preventDefault(); setOption(key, !stopsView[field]); };
    host.appendChild(a);
  });
}
function setOption(key: string, on: boolean): void { if (ws) ws.send(JSON.stringify({ type: "setOption", key, on })); }

// Replay mode has no live socket — it plays back recorded god-view frames. Normal/spectate modes
// open the WebSocket as before.
let ws: WebSocket | null = null;
if (replayId) {
  $("conn").textContent = "● replay";
  startReplay();
} else {
  const wsProto = location.protocol === "https:" ? "wss://" : "ws://";
  ws = new WebSocket(`${wsProto}${location.host}/ws${location.search}`);
  ws.onopen = () => { $("conn").textContent = "● connected";
    log("Keys: Space = pass priority / take the only action · Enter = pass through this turn's stops (Esc cancels)"); };
  ws.onclose = () => ($("conn").textContent = "○ disconnected");
  ws.onerror = () => ($("conn").textContent = "connection error");
  ws.onmessage = (e) => handle(JSON.parse(e.data));
}

function handle(m: Any): void {
  if (m.type === "event") { view = m.view; logEvent(m.event); render(); }
  else if (m.type === "godFrame") {
    // Live god-view spectating: omniscient frame (no masking). Render via the same adapter as
    // replays; surface the label of what just happened in the spectating banner.
    view = godToView(m.state, 0);
    if (m.label) $("prompt").innerHTML = `<div class="waiting">👁 ${esc(m.label)}</div>`;
    render();
  }
  else if (m.type === "decide") {
    view = m.view; cur = m; multi.clear(); orderSeq = [];
    // Enter-engaged "pass through this turn's stops" lapses when the turn advances (MTGA parity).
    if (autoPassTurn !== null && view.turn !== autoPassTurn) { autoPassTurn = null; autoPassBadge(); }
    // The engine surfaces only real stops now; just honour the optional "pass this turn's stops" hold.
    if (isPriorityPrompt(cur.prompt) && autoPassEngaged()) { send({ pass: true }); return; }
    render();
  }
  else if (m.type === "gameOver") { cur = null; renderEnd(m.winner); }
  else if (m.type === "log") { log(m.text); }
  else if (m.type === "stops") { stopsView = m; renderStopsControl(); if (view) renderStepBar(); }
  else if (m.type === "decklist") { deckView[m.seat] = m.cards; if (view) render(); }
}

function logEvent(ev: Any): void {
  if (view && view.turn !== lastTurn) { lastTurn = view.turn; log(`—— Turn ${lastTurn} ——`); }
  const t = eventText(ev);
  if (t) log(t);
}

function send(payload: Any): void {
  if (!cur || !ws) return;
  closeVariantMenu();
  ws.send(JSON.stringify(Object.assign(
    { type: "response", id: cur.id, picks: [], number: null, pass: false, order: [] }, payload)));
  cur = null; multi.clear();
  $("prompt").innerHTML = '<div class="waiting">Waiting for the opponent…</div>';
  render();
}

// ── object/view helpers ─────────────────────────────────────────────────────
// ── replay playback (god-view frame player) ───────────────────────────────────
let replay: Any = null, frameIdx = 0, playing = false, playTimer: Any = null, frameRate = 3;
// Adapt an omniscient GodView frame into the shape render() expects (PlayerView-ish), with NO
// masking: every zone is the real face-up list. Viewpoint seat 0 sits on the bottom half.
function godToView(g: Any, seat: number): Any {
  const players = (g.players || []).map((p: Any) => ({
    player: p.player, life: p.life, poison: p.poison,
    mana_pool: p.mana_pool, counters: p.counters,
    hand_count: (p.hand || []).length, library_count: (p.library || []).length,
    graveyard: p.graveyard || [], exile_public: p.exile || [],
    _hand: p.hand || [], _library: p.library || [], // god-view full zones (library is ordered)
  }));
  const self = players.find((p: Any) => p.player === seat) || players[0] || { _hand: [] };
  return {
    seat, turn: g.turn, active_player: g.active_player, phase: g.phase,
    priority_player: g.priority_player, players,
    me: { hand: self._hand, known_library: [], revealed_to_me: [] },
    battlefield: g.battlefield || [], stack: g.stack || [], combat: g.combat || null,
    stops: null, _god: true,
  };
}
function replaySource(s: Any): string {
  if (s === "Human" || (s && s.Human !== undefined)) return "human game";
  if (s && s.AiTraining) return `AI training — step ${s.AiTraining.step}`;
  return `${s}`;
}
function frameCount(): number { return replay && replay.frames ? replay.frames.length : 0; }
function startReplay(): void {
  ($("replaybar") as HTMLElement).hidden = false;
  fetch(`/api/replays/${encodeURIComponent(replayId as string)}`)
    .then((r) => { if (!r.ok) throw new Error(`HTTP ${r.status}`); return r.json(); })
    .then((rep) => {
      replay = rep;
      const meta = rep.meta || {};
      if (meta.players) $("decks").textContent = `▶ Replay #${replayId} · ` +
        meta.players.map((p: Any) => `P${p.seat} ${p.deck || "?"}`).join(" vs ");
      log(`Loaded replay: ${frameCount()} frames` + (meta.source ? ` · ${replaySource(meta.source)}` : ""));
      showFrame(0);
    })
    .catch((e) => { $("prompt").innerHTML = `<div class="banner">Replay #${esc(replayId as string)} ` +
      `unavailable (${esc(e.message)}). The replay backend may not be wired yet.</div>`; });
}
function showFrame(i: number): void {
  const n = frameCount(); if (!n) return;
  frameIdx = Math.max(0, Math.min(i, n - 1));
  const fr = replay.frames[frameIdx];
  view = godToView(fr.state, 0);
  render();
  $("rbFrame").textContent = `${frameIdx + 1} / ${n}`;
  $("rbLabel").textContent = fr.label || "";
  ($("rbBack") as HTMLButtonElement).disabled = frameIdx === 0;
  ($("rbFwd") as HTMLButtonElement).disabled = frameIdx === n - 1;
  const sc = $("rbScrub") as HTMLInputElement; sc.max = `${n - 1}`; sc.value = `${frameIdx}`; // keep scrubber synced
  if (playing && frameIdx >= n - 1) pauseReplay();
}
function stepReplay(d: number): void { pauseReplay(); showFrame(frameIdx + d); }
function playReplay(): void {
  if (!frameCount()) return;
  if (frameIdx >= frameCount() - 1) frameIdx = -1; // at end → restart from the top
  playing = true; $("rbPlay").textContent = "❚❚";
  playTimer = setInterval(() => showFrame(frameIdx + 1), Math.round(1000 / frameRate));
}
function pauseReplay(): void { playing = false; $("rbPlay").textContent = "▶"; if (playTimer) { clearInterval(playTimer); playTimer = null; } }
function togglePlay(): void { playing ? pauseReplay() : playReplay(); }
if (replayId) {
  $("rbBack").onclick = () => stepReplay(-1);
  $("rbFwd").onclick = () => stepReplay(1);
  $("rbPlay").onclick = togglePlay;
  ($("rbScrub") as HTMLInputElement).oninput = (e) => { pauseReplay(); showFrame(+(e.target as HTMLInputElement).value); };
  ($("rbRate") as HTMLInputElement).oninput = (e) => {
    frameRate = +(e.target as HTMLInputElement).value || 1; $("rbRateV").textContent = `${frameRate}`;
    if (playing) { pauseReplay(); playReplay(); }
  };
}

function norm(o: Any): Any {
  if (o.Hidden) return { hidden: true, id: o.Hidden.id, controller: o.Hidden.controller };
  const v = o.Visible;
  return { id: v.id, chars: v.chars, tapped: !!(v.status && v.status.tapped),
    sick: v.summoning_sick, dmg: v.damage_marked || 0, counters: v.counters,
    controller: v.controller, owner: v.owner, attachments: v.attachments || [] };
}
const meSeat = (): number => view.seat;
const oppId = (): number | null => { const p = view.players.find((p: Any) => p.player !== meSeat()); return p ? p.player : null; };
function bfOf(pid: number): Any[] { return view.battlefield.map(norm).filter((c: Any) => c.controller === pid); }
function isLand(chars: Any): boolean { return (chars.card_types || chars.cardTypes || []).includes("Land"); }
function isCreature(chars: Any): boolean { return (chars.card_types || chars.cardTypes || []).includes("Creature"); }
function pub(pid: number | null): Any { return view.players.find((p: Any) => p.player === pid) || {}; }
// ALL option indices whose object is `id`. A single card can map to several legal options — e.g. a
// spell castable both for its normal cost and an alt-cost (Warp), or any future multi-mode cast. The
// card click disambiguates between them (see onCardClick / showVariantMenu).
function legalIdxs(id: number): number[] {
  if (!cur) return [];
  const objs = cur.prompt.option_objs || cur.prompt.optionObjs || [];
  const out: number[] = []; for (let i = 0; i < objs.length; i++) if (objs[i] === id) out.push(i);
  return out;
}

// ── render ───────────────────────────────────────────────────────────────────
function render(): void {
  if (!view) return;
  $("turn").textContent = `Turn ${view.turn} · ${view.phase} · active P${view.active_player}` +
    (view.priority_player != null ? ` · priority P${view.priority_player}` : "");
  renderRail();
  computeCombat();   // figure out which creatures are engaged BEFORE drawing the halves
  renderHalf("oppoHalf", oppId(), true);
  renderHalf("youHalf", meSeat(), false);
  renderCombatLane();
  renderStack();
  renderStepBar();
  renderHand();
  if (cur) renderPrompt();
  refreshPreview(); // re-derive the hover preview against the freshly-rebuilt DOM
  drawArrows();     // draw stack→target arrows over the rebuilt board
}

// MTGO-style phase/step bar. `stop` = a priority-granting step you can stop at (StopType vocab).
const STEPS: Array<{ phase: string; label: string; stop: boolean }> = [
  { phase: "Untap", label: "Untap", stop: false },
  { phase: "Upkeep", label: "Upkeep", stop: true },
  { phase: "Draw", label: "Draw", stop: true },
  { phase: "PrecombatMain", label: "Main 1", stop: true },
  { phase: "BeginCombat", label: "Combat", stop: true },
  { phase: "DeclareAttackers", label: "Attack", stop: true },
  { phase: "DeclareBlockers", label: "Block", stop: true },
  { phase: "CombatDamage", label: "Damage", stop: true },
  { phase: "EndCombat", label: "End Cbt", stop: true },
  { phase: "PostcombatMain", label: "Main 2", stop: true },
  { phase: "End", label: "End", stop: true },
  { phase: "Cleanup", label: "Cleanup", stop: false },
];
function stopMap(): Any {
  // per_step rows are [phase, on_my_turn, on_opp_turn]; fall back to one-sided shapes gracefully.
  const m: Any = {};
  const src = (stopsView && stopsView.per_step) || (view.stops && view.stops.per_step) || [];
  src.forEach((p: Any) => (m[p[0]] = { mine: !!p[1], opp: p.length > 2 ? !!p[2] : !!p[1] }));
  return m;
}
function renderStepBar(): void {
  const bar = $("stepbar");
  bar.innerHTML = "";
  const stops = stopMap();
  // left legend: which dot is which (top = your turn, bottom = opponent's turn)
  const legend = el("div", "step legend");
  legend.appendChild(el("div", "slabel", "stop"));
  const lz = el("div", "sdots");
  lz.appendChild(el("div", "dotlbl", "opp"));
  lz.appendChild(el("div", "dotlbl", "you"));
  legend.appendChild(lz);
  bar.appendChild(legend);
  STEPS.forEach((st) => {
    const cell = el("div", "step" + (view.phase === st.phase ? " cur" : ""));
    cell.appendChild(el("div", "slabel", st.label));
    if (st.stop) {
      const s = stops[st.phase] || { mine: false, opp: false };
      const dots = el("div", "sdots");
      dots.appendChild(stopDot(st, false, s.opp));   // opponent's turn (top)
      dots.appendChild(stopDot(st, true, s.mine));   // your turn (bottom)
      cell.appendChild(dots);
    }
    bar.appendChild(cell);
  });
}
function stopDot(st: Any, own: boolean, on: boolean): HTMLElement {
  const row = el("div", "sdotrow"); // full-width clickable band → easy to hit (not just the dot)
  const side = own ? "YOUR" : "the opponent's";
  row.title = (on ? "Remove stop on " : "Stop on ") + side + " " + st.label + " (get priority there)";
  row.onclick = (e) => { e.stopPropagation(); toggleStop(st.phase, own, !on); };
  row.appendChild(el("div", "sdot" + (own ? " you" : " opp") + (on ? " on" : "")));
  return row;
}
function toggleStop(phase: string, own: boolean, on: boolean): void {
  // LIVE: the server mutates the shared per-(step, side) stop config + echoes it; the running game's
  // agent honours it at the next priority window — no game reset. `own` = your turn's copy of `step`.
  if (ws) ws.send(JSON.stringify({ type: "setStop", step: phase, own, on }));
}

// Narrow-viewport (mobile reflow) check — mirrors the `@media (max-width:760px)` breakpoint.
function isMobile(): boolean { return window.matchMedia("(max-width: 760px)").matches; }
function renderRail(): void {
  const rail = $("rail");
  rail.innerHTML = "";
  rail.appendChild(pinfoEl(pub(oppId()), false));
  const ph = el("div", "phasebar");
  let phHtml = `turn <b>${view.turn}</b><br>${view.phase}<br>active <b>P${view.active_player}</b>`;
  // Live stops panel (the seat's actual stop config), once the engine populates view.stops.
  if (view.stops) phHtml += `<br><span class="stopline">${stopsSummary(view.stops)}</span>`;
  ph.innerHTML = phHtml;
  rail.appendChild(ph);
  // Your player strip: on desktop it lives at the bottom of the left rail; on mobile it's mounted into
  // the sticky bottom sheet (#selfSlot inside .logpanel) so your life/piles ride along with the prompt.
  const selfPanel = pinfoEl(pub(meSeat()), true);
  const slot = document.getElementById("selfSlot");
  if (slot) slot.innerHTML = "";
  if (isMobile() && slot) slot.appendChild(selfPanel);
  else rail.appendChild(selfPanel);
}

const STEP_ABBR: Any = {
  PrecombatMain: "MP1", PostcombatMain: "MP2", DeclareAttackers: "ATK", DeclareBlockers: "BLK",
  Upkeep: "UP", Draw: "DR", BeginCombat: "BC", CombatDamage: "CD", EndCombat: "EC", End: "END",
  Untap: "UN", Cleanup: "CL",
};
function stopsSummary(ss: Any): string {
  if (ss.full_control) return "🛑 full control";
  const active = (ss.per_step || []).filter((s: Any) => s[1]).map((s: Any) => STEP_ABBR[s[0]] || s[0]);
  return "stops: " + (active.length ? active.join(", ") : "—");
}

function pinfoEl(p: Any, you: boolean): HTMLElement {
  // `.self`/`.opp` let the mobile reflow place your panel by the prompt and the opponent's up top.
  const d = el("div", "pinfo" + (you ? " self" : " opp") + (view.active_player === p.player ? " active" : ""));
  d.dataset.pid = p.player; // target-arrow destination for player-targeting spells
  // If this player is a legal target of the current choice, make the whole panel a click target.
  const pIdx = playerOptIdx(p.player);
  const targeted = pIdx >= 0 && multi.has(pIdx);
  if (pIdx >= 0) {
    d.classList.add(targeted ? "targeted" : "targetable");
    d.title = (targeted ? "Targeted — click to unselect Player " : "Click to target Player ") + p.player;
    d.onclick = () => onOptionToggle(pIdx);
  }
  const who = el("div", "who");
  who.innerHTML = `Player ${p.player}` + (you ? ' <span class="you">YOU</span>' : "") +
    (targeted ? ' <span class="tgtbadge">🎯 target</span>' : "");
  d.appendChild(who);
  d.appendChild(el("div", "life" + (p.life <= 5 ? " low" : ""), `♥ ${p.life}`));
  // Floating (unspent) mana in this player's pool — visible so you can see what's available.
  const pips = poolPips(p.mana_pool || p.manaPool);
  if (pips.length) {
    const mp = el("div", "floatmana");
    mp.title = `Floating mana — ${pips.length} in pool (empties as steps/phases end)`;
    pips.forEach((s) => mp.appendChild(s));
    d.appendChild(mp);
  }
  const piles = el("div", "piles");
  const deck = you ? deckView[p.player] : null; // your starting decklist (debug peek)
  // God-view (replay/spectate): the library is fully visible IN ORDER (top = index 0) and the hand
  // is face-up. Otherwise the library is hidden and only the viewpoint seat sees its own hand.
  const godLib = godMode && view._god ? p._library : null;
  if (godMode && view._god) {
    piles.appendChild(pileEl("Hand", (p._hand || []).length, p._hand || [], `P${p.player} hand`, false));
  } else {
    // Normal play: your hand opens face-up; the opponent's hand opens as N card backs (hidden).
    const handCount = p.hand_count != null ? p.hand_count : (p.handCount || 0);
    const handObjs = you ? (view.me.hand || []) : null;
    piles.appendChild(pileEl("Hand", you ? (view.me.hand || []).length : handCount, handObjs, `P${p.player} hand`, !you));
  }
  const libPile = pileEl("Lib", p.library_count ?? p.libraryCount, godLib,
    `P${p.player} library${godLib ? " (top first)" : ""}`, !godLib);
  if (deck && !godLib) {
    libPile.classList.add("clk");
    libPile.title = "Your starting decklist";
    libPile.onclick = (e) => { e.stopPropagation(); openDecklist(`P${p.player} decklist`, deck); };
  }
  piles.appendChild(libPile);
  piles.appendChild(pileEl("GY", (p.graveyard || []).length, p.graveyard || [], `P${p.player} graveyard`, false));
  const exile = p.exile_public || p.exilePublic || [];
  piles.appendChild(pileEl("Exile", exile.length, exile, `P${p.player} exile`, false));
  d.appendChild(piles);
  return d;
}

function pileEl(label: string, n: number, objs: Any[] | null, title: string, hidden: boolean): HTMLElement {
  const d = el("div", "pile");
  d.innerHTML = `<div class="n">${n}</div><div class="l">${label}</div>`;
  // stopPropagation so opening a zone doesn't also toggle a player-target on the parent panel.
  // Hidden zones (opp hand, your library) open as `n` card backs rather than nothing.
  d.onclick = (e) => { e.stopPropagation(); openZone(title, hidden ? null : objs || [], hidden ? n : 0); };
  return d;
}
// The prompt-option index that targets player `pid` (a "Player N" option with no board object), or
// -1. Lets the board's player panels act as click targets for player-targeting choices.
function playerOptIdx(pid: number): number {
  if (!cur) return -1;
  const p = cur.prompt;
  if (p.mode !== "selectMany" && p.mode !== "selectOne") return -1;
  const objs = p.option_objs || p.optionObjs || [];
  for (let i = 0; i < p.options.length; i++) {
    if (objs[i] == null && p.options[i] === `Player ${pid}`) return i;
  }
  return -1;
}
// Unified option toggle (side-panel buttons AND board player panels) so every view stays in sync.
function onOptionToggle(i: number): void {
  if (!cur) return;
  const p = cur.prompt;
  if (p.mode === "selectMany") { if (multi.has(i)) multi.delete(i); else multi.add(i); render(); }
  else { send({ picks: [i] }); }
}

function renderHalf(elId: string, pid: number | null, isOppo: boolean): void {
  const host = $(elId);
  host.innerHTML = "";
  if (pid == null) return;
  const perms = bfOf(pid);
  // Auras/Equipment attached to a permanent render BEHIND their host, not as standalone cards.
  const byId: Any = {}; perms.forEach((c) => { byId[c.id] = c; });
  const attached = new Set<number>();
  perms.forEach((c) => (c.attachments || []).forEach((id: number) => attached.add(id)));
  // Skip creatures pulled into the combat lane (blocked attackers + their blockers, and the rest of
  // the attackers) — they render there instead, and return to the band once combat ends.
  const top = perms.filter((c) => !attached.has(c.id) && !engagedIds.has(c.id));
  // Three buckets. Creatures take priority over every other type: an artifact/enchantment/land that
  // is ALSO a creature goes with the creatures (it attacks/blocks). Unknown (no chars, e.g.
  // face-down) defaults to the creature row too. Remaining noncreature permanents split into lands
  // (their own back row) and "others" — noncreature artifacts/enchantments/planeswalkers, which
  // render off to the RIGHT of the creatures in the same band.
  const creatures = top.filter((c) => !c.chars || isCreature(c.chars));
  const lands = top.filter((c) => c.chars && !isCreature(c.chars) && isLand(c.chars));
  const others = top.filter((c) => c.chars && !isCreature(c.chars) && !isLand(c.chars));
  const landRow = zoneRow("lands", lands, byId);
  const creatureRow = creatureBand(creatures, others, byId);
  if (isOppo) { host.appendChild(landRow); host.appendChild(creatureRow); }
  else { host.appendChild(creatureRow); host.appendChild(landRow); }
}

function zoneRow(cls: string, cards: Any[], byId?: Any): HTMLElement {
  const row = el("div", "row " + cls);
  if (!cards.length) { row.appendChild(el("span", "rowlabel", cls === "lands" ? "lands" : "—")); return row; }
  cards.forEach((c) => row.appendChild(permEl(c, byId || {})));
  return row;
}

// The creature band: creatures fill from the left; noncreature artifacts/enchantments/walkers sit in
// a separate group off to the right (divider + right-aligned), so combat permanents stay visually
// distinct from static/utility ones.
function creatureBand(creatures: Any[], others: Any[], byId?: Any): HTMLElement {
  const row = el("div", "row creatures");
  if (!creatures.length && !others.length) { row.appendChild(el("span", "rowlabel", "—")); return row; }
  const cg = el("div", "permgroup cg");
  creatures.forEach((c) => cg.appendChild(permEl(c, byId || {})));
  row.appendChild(cg);
  if (others.length) {
    const og = el("div", "permgroup og");
    others.forEach((c) => og.appendChild(permEl(c, byId || {})));
    row.appendChild(og);
  }
  return row;
}

// ── combat lane: blockers move in front of the attacker they block ───────────────
// `combatByAtk` maps attackerId → [blockerId…] (only once blocks are declared); `engagedIds` is
// every creature pulled out of its band into the lane. Both are recomputed each render and cleared
// when combat ends, so creatures snap back to their normal rows afterward.
let combatByAtk: Map<number, number[]> | null = null;
let engagedIds: Set<number> = new Set();
function computeCombat(): void {
  combatByAtk = null; engagedIds = new Set();
  const c = view && view.combat;
  if (!c || !(c.blockers || []).length) return; // lane only appears once a block exists
  // Only pull creatures that are STILL on the battlefield into the lane — a creature that died in
  // combat is gone, and a SURVIVOR must keep being rendered (in the lane below). Adding a dead-or-
  // alive id blindly is what made survivors-whose-attacker-died vanish until combat cleared.
  const alive = new Set<number>();
  (view.battlefield || []).forEach((o: Any) => { if (o.Visible) alive.add(o.Visible.id); });
  combatByAtk = new Map();
  (c.blockers || []).forEach((pr: Any) => {      // pr = [blocker, attacker]
    const bl = pr[0], atk = pr[1];
    if (!combatByAtk!.has(atk)) combatByAtk!.set(atk, []);
    combatByAtk!.get(atk)!.push(bl);
    if (alive.has(bl)) engagedIds.add(bl);       // surviving blockers
  });
  (c.attackers || []).forEach((a: Any) => { if (alive.has(a[0])) engagedIds.add(a[0]); }); // surviving attackers
}
function renderCombatLane(): void {
  const host = $("combatLane");
  host.innerHTML = "";
  if (!combatByAtk || !view.combat) { host.hidden = true; return; }
  const allById: Any = {}; (view.battlefield || []).map(norm).forEach((o: Any) => { allById[o.id] = o; });
  // One cell per declared attacker, rendering whoever SURVIVED. If the attacker died but a blocker
  // lived, the cell still shows the surviving blocker (so it never disappears); skip a matchup only
  // when nothing on either side survives.
  let any = false;
  (view.combat.attackers || []).forEach((a: Any) => {
    const atk = allById[a[0]] || null; // may have died in combat
    const blks = (combatByAtk!.get(a[0]) || []).map((id: number) => allById[id]).filter(Boolean); // survivors
    if (!atk && !blks.length) return;
    host.appendChild(matchupCell(atk, blks, allById));
    any = true;
  });
  host.hidden = !any;
}
// One attacker and the creatures blocking it, stacked so they face off across a ⚔. The attacker
// sits toward its own controller's side (opponent on top, you on the bottom) — mirroring the board —
// so a blocker visibly stands in front of the attacker it's stopping. Unblocked attackers stand
// alone with an arrow toward the player they're hitting.
function matchupCell(atk: Any, blks: Any[], allById: Any): HTMLElement {
  const cell = el("div", "matchup" + (blks.length ? "" : " unblocked"));
  if (!atk) { // attacker died in combat → show the surviving blocker(s) on their own
    const blkRow = el("div", "matchrow blk");
    blks.forEach((b) => blkRow.appendChild(permEl(b, allById)));
    cell.appendChild(blkRow);
    return cell;
  }
  const atkMine = atk.controller === view.seat;
  const atkRow = el("div", "matchrow atk"); atkRow.appendChild(permEl(atk, allById));
  if (blks.length) {
    const blkRow = el("div", "matchrow blk");
    blks.forEach((b) => blkRow.appendChild(permEl(b, allById)));
    const vs = el("div", "vs", "⚔");
    if (atkMine) { cell.appendChild(blkRow); cell.appendChild(vs); cell.appendChild(atkRow); }
    else { cell.appendChild(atkRow); cell.appendChild(vs); cell.appendChild(blkRow); }
  } else {
    const thru = el("div", "vs through", atkMine ? "↑ unblocked" : "↓ unblocked");
    if (atkMine) { cell.appendChild(thru); cell.appendChild(atkRow); }
    else { cell.appendChild(atkRow); cell.appendChild(thru); }
  }
  return cell;
}

// A battlefield permanent plus any auras/equipment attached to it, stacked slightly offset behind
// it (CR 303/301 attachments). Returns just the card when there are no attachments.
function permEl(c: Any, byId: Any): HTMLElement {
  const atts = (c.attachments || []).map((id: number) => byId[id]).filter(Boolean);
  const card = cardEl(c, { interactive: true });
  if (!atts.length) return card;
  const wrap = el("div", "attachwrap");
  // Attached cards sit behind, each nudged UP and to the LEFT so its top (name) peeks out above the
  // host — and the host's own name stays fully visible on top.
  atts.forEach((a: Any, i: number) => {
    const ac = cardEl(a, { interactive: true, attach: true });
    ac.style.left = `${-(i + 1) * 15}px`;
    ac.style.top = `${-(i + 1) * 19}px`;
    ac.style.zIndex = `${i + 1}`;
    wrap.appendChild(ac);
  });
  card.style.position = "relative";
  card.style.zIndex = `${atts.length + 1}`;
  wrap.appendChild(card);
  wrap.style.marginLeft = `${atts.length * 15}px`;
  wrap.style.marginTop = `${atts.length * 19}px`;
  return wrap;
}

function renderStack(): void {
  const s = $("stack");
  s.innerHTML = "";
  (view.stack || []).forEach((it: Any) => {
    const card = cardEl({ id: it.id, chars: stackChars(it) }, {});
    if (isAbilityStack(it)) card.classList.add("ability");
    card.dataset.sid = it.id; // target-arrow source (stack id space, distinct from object ids)
    s.appendChild(card);
  });
}
// A triggered/activated ability on the stack: the engine projects it with an empty "Ability" chars
// (no grp_id), but it carries `source` (the permanent it came from). Surface THAT card's name +
// oracle text + art — the right detail for the current one-ability-per-card pool. (The precise
// per-ability text would be an engine-side effect formatter; this is the faithful client view.)
function isAbilityStack(it: Any): boolean {
  const c = it.chars || {};
  return !c.grp_id || c.name === "Ability";
}
function stackChars(it: Any): Any {
  if (!isAbilityStack(it)) return it.chars;
  const src = it.source != null ? findVisibleChars(it.source) : null;
  if (!src) return it.chars; // source not in a visible zone (rare) → leave as "Ability"
  return { name: src.name, card_types: ["Ability"], subtypes: [], supertypes: [],
    colors: src.colors || [], mana_value: 0, rules_text: src.rules_text || "",
    keywords: [], grp_id: src.grp_id };
}
function findVisibleChars(id: number): Any {
  const me = view.me || {};
  const zones: Any[] = [view.battlefield, me.hand, me.revealed_to_me, me.known_library];
  (view.players || []).forEach((p: Any) => zones.push(p.graveyard, p.exile_public || p.exilePublic, p._hand, p._library));
  for (const z of zones) {
    for (const o of (z || [])) if (o && o.Visible && o.Visible.id === id) return o.Visible.chars;
  }
  return null;
}
// An option button. When the option refers to a real card (resolvable in the view — incl. Search
// candidates now revealed in me.revealed_to_me), show its art thumbnail + name as a card tile.
function optButton(i: number, label: string, objId: Any): HTMLElement {
  const chars = objId != null ? findVisibleChars(objId) : null;
  const grp = chars && chars.grp_id;
  const b = el("button", "opt" + (multi.has(i) ? " sel" : "") + (grp ? " optcard" : "")) as HTMLButtonElement;
  if (grp) {
    const info = artMap[grp];
    if (info && info.art) { const t = el("div", "optart"); t.style.backgroundImage = `url('${info.art}')`; b.appendChild(t); }
    b.appendChild(el("span", "optname", chars.name || label));
  } else {
    b.textContent = label;
  }
  b.onclick = () => onOptionToggle(i);
  return b;
}

function renderHand(): void {
  const h = $("hand");
  h.innerHTML = "";
  h.appendChild(el("div", "hlabel", "hand"));
  const hand = view.me.hand || [];
  hand.forEach((o: Any) => h.appendChild(cardEl(norm(o), { interactive: true, hand: true })));
  if (!hand.length) h.appendChild(el("span", "waiting", "(empty hand)"));
  // God-view (replay / spectate): render the opponent's hand face-up at the top, too.
  const oh = $("oppHand") as HTMLElement;
  if (godMode && view._god) {
    oh.hidden = false; oh.innerHTML = "";
    oh.appendChild(el("div", "hlabel", "opp hand"));
    const opp = (view.players || []).find((p: Any) => p.player !== view.seat);
    const cards = (opp && opp._hand) || [];
    cards.forEach((o: Any) => oh.appendChild(cardEl(norm(o), { hand: true })));
    if (!cards.length) oh.appendChild(el("span", "waiting", "(empty)"));
  } else {
    oh.hidden = true;
  }
}

// ── card frame ─────────────────────────────────────────────────────────────
function cardEl(c: Any, ctx: Any): HTMLElement {
  if (c.hidden) return el("div", "card back");
  const chars = c.chars || {};
  const idxs = ctx.interactive ? legalIdxs(c.id) : [];
  const idx = idxs.length ? idxs[0] : -1;
  const selected = ctx.interactive && idx >= 0 && multi.has(idx);
  const d = el("div", ["card", colorClass(chars), c.tapped ? "tapped" : "", c.sick ? "sick" : "",
    idx >= 0 ? "legal" : "", selected ? "selected" : "", ctx.attach ? "attach" : ""].filter(Boolean).join(" "));
  if (c.id != null) d.dataset.oid = c.id; // for target arrows + attachment lookup

  const hdr = el("div", "c-hdr");
  hdr.appendChild(el("div", "c-name", chars.name || "—"));
  const mana = el("div", "c-mana");
  manaPips(chars).forEach((p) => mana.appendChild(p));
  hdr.appendChild(mana);
  d.appendChild(hdr);

  // Dev quality reminder: engine flags a card whose printed text is only partially modeled.
  // (Forward-compatible: only shows when the view explicitly says `fully_implemented === false`.)
  if (chars.fully_implemented === false) {
    d.classList.add("partial");
    const warn = el("div", "warnbadge", "⚠");
    warn.title = "Not fully implemented" + (chars.rules_text ? ":\n" + chars.rules_text : "");
    d.appendChild(warn);
  }

  const art = el("div", "c-art");
  const info = artMap[chars.grp_id];
  if (info && info.art) {
    art.style.backgroundImage = `url('${info.art}')`;
    art.style.backgroundSize = "cover";
  }
  d.appendChild(art);
  // Hover → full card image preview. Stored as a data-attr and resolved by the global pointer
  // tracker (`refreshPreview`) rather than per-element mouseenter/leave listeners — so when the
  // board re-renders and this element is replaced out from under the cursor, the preview can't
  // get orphaned/stuck open (the tracker re-derives what's under the pointer every frame).
  if (info && info.img) d.dataset.preview = info.img;
  d.appendChild(el("div", "c-type", typeLine(chars)));
  const rules = el("div", "c-rules");
  // Computed keywords (incl. layer-granted, e.g. Flying from Levitation) bold on top, then text.
  const kw: string[] = chars.keywords || [];
  let rulesHtml = kw.length ? `<b>${esc(kw.join(", "))}</b>` : "";
  if (chars.rules_text) rulesHtml += (rulesHtml ? "<br>" : "") + renderText(chars.rules_text);
  rules.innerHTML = rulesHtml;
  d.appendChild(rules);
  if (chars.power != null) d.appendChild(el("div", "c-pt", `${chars.power}/${chars.toughness}`));

  if (view.combat) {
    if ((view.combat.attackers || []).some((a: Any) => a[0] === c.id)) d.appendChild(el("div", "badge atk", "ATK"));
    if ((view.combat.blockers || []).some((b: Any) => b[0] === c.id)) d.appendChild(el("div", "badge blk", "BLK"));
  }
  if (c.dmg > 0) d.appendChild(el("div", "badge dmg", `${c.dmg}✶`));
  const cc = (c.counters && c.counters.counts) || {};
  const ck = Object.keys(cc);
  if (ck.length) {
    const wrap = el("div", "ctrs");
    ck.forEach((k) => wrap.appendChild(el("span", "ctr", `${counterLabel(k)}×${cc[k]}`)));
    d.appendChild(wrap);
  }

  if (idxs.length) d.onclick = (e: MouseEvent) => onCardClick(idxs, e);
  return d;
}
const CTR_LABEL: Any = { PlusOnePlusOne: "+1/+1", MinusOneMinusOne: "−1/−1" };
function counterLabel(k: string): string { return CTR_LABEL[k] || k; }

const LETTER: Any = { White: "w", Blue: "u", Black: "b", Red: "r", Green: "g" };
function colorClass(chars: Any): string {
  if (isLand(chars)) return "land";
  const cols = chars.colors || [];
  if (cols.length > 1) return "multi";
  if (cols.length === 1) return LETTER[cols[0]] || "colorless";
  return "colorless";
}
function typeLine(chars: Any): string {
  const sup = (chars.supertypes || []).join(" ");
  const types = (chars.card_types || chars.cardTypes || []).join(" ");
  const sub = (chars.subtypes || []).join(" ");
  let s = [sup, types].filter(Boolean).join(" ");
  if (sub) s += " — " + sub;
  return s || "—";
}
const WUBRG = ["White", "Blue", "Black", "Red", "Green", "Colorless"];
const CODE: Any = { White: "W", Blue: "U", Black: "B", Red: "R", Green: "G", Colorless: "C" };
// A real Magic mana/cost symbol from Scryfall's official SVG set.
function symImg(code: string, cls?: string): HTMLElement {
  const i = el("img", "ms" + (cls ? " " + cls : "")) as HTMLImageElement;
  i.src = "https://svgs.scryfall.io/card-symbols/" + code + ".svg";
  i.alt = "{" + code + "}";
  i.loading = "lazy";
  return i;
}
function manaPips(chars: Any): HTMLElement[] {
  if (isLand(chars)) return [];
  const out: HTMLElement[] = [];
  const mc = chars.mana_cost; // exact structured cost when the view carries it
  if (mc) {
    const colored = mc.colored || {};
    const totalC = (Object.values(colored) as number[]).reduce((a, b) => a + b, 0);
    if (mc.generic > 0 || (mc.generic === 0 && totalC === 0)) out.push(symImg(String(mc.generic)));
    WUBRG.forEach((c) => { const n = colored[c] || 0; for (let i = 0; i < n; i++) out.push(symImg(CODE[c])); });
    return out;
  }
  // No structured mana cost in the view → the card has NO cost (a token, an ability on the stack, or
  // a genuinely costless card like Living End) and shows NOTHING — distinct from a real {0} (which
  // arrives as mana_cost {generic:0} and renders "0" above). Only approximate from CMC if the view
  // actually implies a positive cost (defensive; current views always carry mana_cost).
  const cmc = chars.mana_value ?? chars.manaValue ?? 0;
  if (cmc <= 0) return out; // no cost → no pips (NOT a "0")
  const cols = chars.colors || [];
  const generic = Math.max(0, cmc - cols.length);
  if (generic > 0) out.push(symImg(String(generic)));
  cols.forEach((c: string) => out.push(symImg(CODE[c] || "C")));
  return out;
}
// Replace {T}/{G}/… tokens in oracle text with their Scryfall symbol SVGs.
// Floating mana in a player's pool → real mana symbols (one per mana, by colour).
function poolPips(pool: Any): HTMLElement[] {
  const out: HTMLElement[] = [];
  const amts = (pool && pool.amounts) || {};
  WUBRG.forEach((c) => { const n = amts[c] || 0; for (let i = 0; i < n; i++) out.push(symImg(CODE[c])); });
  return out;
}
function renderText(text: string): string {
  return esc(text).replace(/\{([^}]+)\}/g, (m, code) => {
    const c = code.toUpperCase().replace(/\//g, "");
    return '<img class="ms ms-text" src="https://svgs.scryfall.io/card-symbols/' + c + '.svg" alt="' + esc(m) + '">';
  });
}
function esc(s: string): string {
  return String(s).replace(/[&<>"]/g, (c) => (({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" }) as Any)[c]);
}

// ── click-to-act ─────────────────────────────────────────────────────────────
function onCardClick(idxs: number[], e?: MouseEvent): void {
  if (!cur || !idxs.length) return;
  const mode = cur.prompt.mode;
  if (mode === "selectMany") {
    const i = idxs[0];
    if (multi.has(i)) multi.delete(i); else multi.add(i); render(); renderPrompt();
    return;
  }
  // action / selectOne: one option → act immediately; several options on the same card (e.g. Normal
  // + Warp / any alt-cost cast) → pop a small menu so the player chooses the variant.
  if (idxs.length === 1) { send({ picks: [idxs[0]] }); return; }
  showVariantMenu(idxs, e);
}

// A small popup anchored at the click, one button per legal option on the clicked card. Used when a
// card has multiple cast variants (the engine pre-enumerated each as its own action).
function closeVariantMenu(): void { const m = document.getElementById("varmenu"); if (m) m.remove(); }
function showVariantMenu(idxs: number[], e?: MouseEvent): void {
  closeVariantMenu();
  const opts = cur.prompt.options || [];
  const menu = el("div", "varmenu"); menu.id = "varmenu";
  menu.appendChild(el("div", "varhdr", "Choose how to cast"));
  idxs.forEach((i) => {
    const b = el("button", "varitem", opts[i] || ("Option " + i));
    b.onclick = (ev: MouseEvent) => { ev.stopPropagation(); closeVariantMenu(); send({ picks: [i] }); };
    menu.appendChild(b);
  });
  document.body.appendChild(menu);
  const x = (e && e.clientX != null) ? e.clientX : 80, y = (e && e.clientY != null) ? e.clientY : 80;
  const r = menu.getBoundingClientRect();
  menu.style.left = Math.max(6, Math.min(x, window.innerWidth - r.width - 8)) + "px";
  menu.style.top = Math.max(6, Math.min(y, window.innerHeight - r.height - 8)) + "px";
  setTimeout(() => {
    document.addEventListener("click", closeVariantMenu, { once: true });
    document.addEventListener("keydown", function esc(k: KeyboardEvent) {
      if (k.key === "Escape") { closeVariantMenu(); document.removeEventListener("keydown", esc); }
    });
  }, 0);
}

// ── prompt panel (non-card options + controls) ───────────────────────────────
function renderPrompt(): void {
  closeVariantMenu(); // a fresh prompt invalidates any open cast-variant menu
  const p = cur.prompt;
  const root = $("prompt");
  root.innerHTML = "";
  root.appendChild(el("div", "title", p.title));
  const objs = p.option_objs || p.optionObjs || [];

  if (p.mode === "number") {
    const inp = el("input") as HTMLInputElement;
    inp.type = "number"; inp.min = String(p.numMin); inp.max = String(p.numMax); inp.value = String(p.numMin);
    root.appendChild(inp);
    root.appendChild(el("div", "hint", `Enter ${p.numMin}–${p.numMax}.`));
    addActions(root, [actBtn("Submit", () => send({ number: clamp(parseInt(inp.value || "0", 10), p.numMin, p.numMax) }))]);
    return;
  }
  if (p.mode === "order") {
    const opts = el("div", "opts");
    p.options.forEach((label: string, i: number) => {
      const pos = orderSeq.indexOf(i);
      const b = el("button", "opt" + (pos >= 0 ? " sel" : ""), (pos >= 0 ? `[${pos + 1}] ` : "") + label) as HTMLButtonElement;
      b.onclick = () => { if (pos >= 0) orderSeq.splice(pos, 1); else orderSeq.push(i); renderPrompt(); };
      opts.appendChild(b);
    });
    root.appendChild(opts);
    root.appendChild(el("div", "hint", "Click in resolution order (first clicked resolves first)."));
    const sub = actBtn("Submit order", () => send({ order: orderSeq.slice() })) as HTMLButtonElement;
    sub.disabled = orderSeq.length !== p.options.length;
    addActions(root, [sub, passBtn("Reset", () => { orderSeq = []; renderPrompt(); })]);
    return;
  }

  // Multi-slot target choice (e.g. Bushwhack-fight: slot 0 = a creature you control, slot 1 = one
  // you don't). Render each slot as its own group + enforce each slot's count independently.
  const slots = p.targetSlots || p.target_slots || [];
  if (slots.length) {
    slots.forEach((slot: Any) => {
      const sec = el("div", "slotgroup");
      sec.appendChild(el("div", "slothdr", slot.description +
        (slot.min === slot.max ? " — pick " + slot.min : " — " + slot.min + "–" + slot.max)));
      const sopts = el("div", "opts");
      let boardN = 0;
      for (let i = slot.start; i < slot.start + slot.len; i++) {
        if (objs[i] != null && document.querySelector(`[data-oid="${objs[i]}"]`)) { boardN++; continue; }
        sopts.appendChild(optButton(i, p.options[i], objs[i]));
      }
      sec.appendChild(sopts);
      if (boardN) sec.appendChild(el("div", "hint", "→ click the highlighted card(s) on the board"));
      root.appendChild(sec);
    });
    if (multi.size) root.appendChild(el("div", "chosen", "🎯 Chosen: " +
      [...multi].sort((a, b) => a - b).map((i) => p.options[i]).join(", ")));
    const sub = actBtn("Submit", () => send({ picks: [...multi].sort((a, b) => a - b) })) as HTMLButtonElement;
    sub.disabled = !slots.every((s: Any) => {
      const c = [...multi].filter((i) => i >= s.start && i < s.start + s.len).length;
      return c >= s.min && c <= s.max;
    });
    const acts2: HTMLElement[] = [sub];
    if (p.canPass) acts2.push(passBtn("Pass", () => send({ pass: true })));
    addActions(root, acts2);
    return;
  }

  // Render a button for every option EXCEPT those whose object is actually on the board (those are
  // clicked on the card). An option whose object isn't on the board — e.g. a library card offered by
  // a Search (Erode/Bushwhack/Worldwagon) — still needs a button, else there's no way to pick it.
  const opts = el("div", "opts");
  let boardCount = 0;
  p.options.forEach((label: string, i: number) => {
    const onBoard = objs[i] != null && document.querySelector(`[data-oid="${objs[i]}"]`);
    if (onBoard) { boardCount++; return; }
    opts.appendChild(optButton(i, label, objs[i]));
  });
  root.appendChild(opts);
  if (boardCount) root.appendChild(el("div", "hint",
    "→ click the highlighted cards / players on the board" + (p.mode === "selectMany" ? " to select" : "") + "."));

  // Show exactly what's chosen so far (board cards + player/side-panel options).
  if (p.mode === "selectMany" && multi.size) {
    const chosen = [...multi].sort((a, b) => a - b).map((i) => p.options[i]).join(", ");
    root.appendChild(el("div", "chosen", "🎯 Chosen: " + chosen));
  }

  const acts: HTMLElement[] = [];
  if (p.mode === "selectMany") {
    const sub = actBtn(`Submit (${multi.size})`, () => send({ picks: [...multi].sort((a, b) => a - b) })) as HTMLButtonElement;
    sub.disabled = multi.size < p.min || multi.size > p.max;
    acts.push(sub);
  }
  if (p.canPass) acts.push(passBtn("Pass", () => send({ pass: true })));
  if (acts.length) addActions(root, acts);
}
function addActions(root: HTMLElement, btns: HTMLElement[]): void { const r = el("div", "actions"); btns.forEach((b) => r.appendChild(b)); root.appendChild(r); }
function actBtn(t: string, fn: () => void): HTMLElement { const b = el("button", "act", t) as HTMLButtonElement; b.onclick = fn; return b; }
function passBtn(t: string, fn: () => void): HTMLElement { const b = el("button", "pass", t) as HTMLButtonElement; b.onclick = fn; return b; }

function renderEnd(winner: number | null): void {
  const w = winner == null ? "draw" : `Player ${winner}`;
  const youWon = view && winner === view.seat;
  $("prompt").innerHTML = `<div class="banner">Game over — winner: ${w}${youWon ? " 🎉 (you!)" : ""}</div>`;
  log(`GAME OVER — winner: ${w}`);
}

// ── zone viewer modal ─────────────────────────────────────────────────────────
function openZone(title: string, objs: Any[] | null, backs?: number): void {
  const g = $("modalGrid"); g.innerHTML = "";
  if (objs == null) {
    // Hidden zone: show one generic card back per card (the count is known even if contents aren't).
    const n = backs || 0;
    $("modalTitle").textContent = `${title} (${n}${n ? " · hidden" : ""})`;
    if (n > 0) { for (let i = 0; i < n; i++) g.appendChild(el("div", "card back")); }
    else { g.innerHTML = '<div class="waiting">This zone is hidden — its contents aren\'t in your view.</div>'; }
  } else if (!objs.length) {
    $("modalTitle").textContent = `${title} (0)`;
    g.innerHTML = '<div class="waiting">(empty)</div>';
  } else {
    $("modalTitle").textContent = `${title} (${objs.length})`;
    objs.map(norm).forEach((c) => g.appendChild(cardEl(c, {})));
  }
  $("modal").classList.add("show");
}
// Decklist peek (your library is hidden in real MTG; this is the static starting deck list,
// grouped, order discarded — a debug aid, never fed to the agent).
function openDecklist(title: string, entries: Any[]): void {
  const g = $("modalGrid"); g.innerHTML = "";
  const total = entries.reduce((a, e) => a + (e.count || 0), 0);
  $("modalTitle").textContent = `${title} — ${total} cards (starting library, order hidden)`;
  const list = el("div", "decklist");
  entries.forEach((e) => {
    const c = e.chars;
    const row = el("div", "dlrow " + colorClass(c));
    const art = artMap[c.grp_id];
    const thumb = el("div", "dlthumb");
    if (art && (art.art || art.img)) thumb.style.backgroundImage = `url('${art.art || art.img}')`;
    if (art && art.img) row.dataset.preview = art.img;
    row.appendChild(el("div", "dlcount", `${e.count || 1}×`));
    row.appendChild(thumb);
    row.appendChild(el("div", "dlname", c.name));
    const pips = el("div", "dlpips");
    manaPips(c).forEach((p) => pips.appendChild(p));
    row.appendChild(pips);
    list.appendChild(row);
  });
  g.appendChild(list);
  $("modal").classList.add("show");
}
$("modalClose").onclick = () => $("modal").classList.remove("show");
$("modal").onclick = (e) => { if (e.target === $("modal")) $("modal").classList.remove("show"); };
// Mobile "Log" toggle: the game log is hidden by default on narrow screens (only the current prompt
// stays always-on); this flips `body.show-log` to reveal/collapse the history.
const logToggle = document.getElementById("logToggle");
if (logToggle) logToggle.onclick = () => { logToggle.classList.toggle("on", document.body.classList.toggle("show-log")); };

// ── misc ───────────────────────────────────────────────────────────────────────
function nameOf(id: number): string {
  const all: Any[] = ([] as Any[]).concat(view.me.hand || [], view.battlefield || []);
  (view.players || []).forEach((p: Any) => all.push(...(p.graveyard || []), ...(p.exile_public || p.exilePublic || [])));
  for (const o of all) if (o.Visible && o.Visible.id === id) return o.Visible.chars.name;
  for (const s of (view.stack || [])) if (s.source === id) return s.chars.name; // resolving spell/ability
  return "#" + id;
}
function stackName(sid: number): string { for (const s of (view.stack || [])) if (s.id === sid) return s.chars.name; return "spell #" + sid; }
// " → target1, target2" for a stack object's targets (for the log), or "" if none.
function stackTgtSuffix(sid: number): string {
  const s = (view.stack || []).find((x: Any) => x.id === sid);
  const ts = (s && s.targets) || [];
  return ts.length ? " → " + ts.map(tgtName).join(", ") : "";
}
function tgtName(t: Any): string {
  if (t == null) return "?";
  if (t.Player != null) return "P" + t.Player;
  if (t.Object != null) return nameOf(t.Object);
  if (t.Stack != null) return "spell #" + t.Stack;
  return "?";
}
// ── hover full-card preview ────────────────────────────────────────────────────
function showPreview(url: string, ev: MouseEvent): void {
  const p = $("preview") as HTMLImageElement;
  p.src = url; p.style.display = "block"; positionPreview(ev);
}
function hidePreview(): void { $("preview").style.display = "none"; }
function positionPreview(ev: MouseEvent): void {
  const p = $("preview"); const w = 320, h = 446;
  let x = ev.clientX + 22; let y = ev.clientY - h / 2;
  if (x + w > window.innerWidth) x = ev.clientX - w - 22;
  y = Math.max(8, Math.min(y, window.innerHeight - h - 8));
  p.style.left = `${x}px`; p.style.top = `${y}px`;
}
// Global pointer tracking: the single source of truth for what the preview shows. We re-derive the
// element under the cursor (`elementFromPoint`) on every move AND after every render, so a card
// replaced/removed under a stationary cursor can never leave a stale preview on screen. (`#preview`
// is `pointer-events:none`, so it never shadows the card beneath it.)
let ptrX = -1, ptrY = -1;
function refreshPreview(): void {
  if (ptrX < 0) return;
  const t = document.elementFromPoint(ptrX, ptrY);
  const card = t && (t as HTMLElement).closest ? (t as HTMLElement).closest("[data-preview]") : null;
  const url = card ? (card as HTMLElement).dataset.preview : null;
  if (url) showPreview(url, { clientX: ptrX, clientY: ptrY } as MouseEvent);
  else hidePreview();
}
document.addEventListener("mousemove", (e) => { ptrX = e.clientX; ptrY = e.clientY; refreshPreview(); }, true);
// Cursor leaves the document entirely (e.g. out the top of the window) → drop the preview.
document.addEventListener("mouseout", (e) => { if (!(e as MouseEvent).relatedTarget) { ptrX = ptrY = -1; hidePreview(); } });

// ── stack → target arrows ──────────────────────────────────────────────────────
const SVGNS = "http://www.w3.org/2000/svg";
const ARROW_DEFS =
  '<defs><marker id="arrowhead" markerWidth="9" markerHeight="9" refX="7.5" refY="3" orient="auto">' +
  '<path d="M0,0 L8,3 L0,6 Z" fill="#ff5a5a"/></marker></defs>';
function targetEl(t: Any): Element | null {
  if (t == null) return null;
  if (t.Player != null) return document.querySelector(`[data-pid="${t.Player}"]`);
  if (t.Object != null) return document.querySelector(`.half [data-oid="${t.Object}"], .attachwrap [data-oid="${t.Object}"]`);
  if (t.Stack != null) return document.querySelector(`[data-sid="${t.Stack}"]`);
  return null;
}
function centerOf(elm: Element): { x: number; y: number } {
  const r = elm.getBoundingClientRect(); return { x: r.left + r.width / 2, y: r.top + r.height / 2 };
}
function drawArrows(): void {
  const svg = $("arrows");
  svg.innerHTML = ARROW_DEFS;
  const stack = (view && view.stack) || [];
  stack.forEach((st: Any) => {
    const fromEl = document.querySelector(`[data-sid="${st.id}"]`);
    if (!fromEl) return;
    const a = centerOf(fromEl);
    (st.targets || []).forEach((t: Any) => {
      const toEl = targetEl(t);
      if (!toEl) return;
      const b = centerOf(toEl);
      const mx = (a.x + b.x) / 2, my = (a.y + b.y) / 2;
      const dx = b.x - a.x, dy = b.y - a.y, len = Math.hypot(dx, dy) || 1;
      const bow = Math.min(60, len * 0.22);
      const cx = mx - (dy / len) * bow, cy = my + (dx / len) * bow;
      const path = document.createElementNS(SVGNS, "path");
      path.setAttribute("d", `M${a.x},${a.y} Q${cx},${cy} ${b.x},${b.y}`);
      path.setAttribute("fill", "none");
      path.setAttribute("stroke", "#ff5a5a");
      path.setAttribute("stroke-width", "2.5");
      path.setAttribute("stroke-linecap", "round");
      path.setAttribute("opacity", "0.9");
      path.setAttribute("marker-end", "url(#arrowhead)");
      svg.appendChild(path);
      const dot = document.createElementNS(SVGNS, "circle");
      dot.setAttribute("cx", `${a.x}`); dot.setAttribute("cy", `${a.y}`); dot.setAttribute("r", "3");
      dot.setAttribute("fill", "#ff5a5a");
      svg.appendChild(dot);
    });
  });
}
// On resize/rotation re-run render so the player strip re-homes between the left rail (desktop) and
// the sticky bottom sheet (mobile), then redraw the target arrows against the new layout.
window.addEventListener("resize", () => { if (view) render(); else drawArrows(); });
window.addEventListener("scroll", drawArrows, true);

function eventText(ev: Any): string | null {
  const k = Object.keys(ev)[0]; const v = ev[k];
  switch (k) {
    case "PhaseBegan": case "Revealed": return null;
    case "DrewCards": return `P${v.player} draws ${v.count}`;
    case "LifeChanged": return `P${v.player} life ${v.delta >= 0 ? "+" : ""}${v.delta} → ${v.new_total}`;
    case "DamageDealt": return `⚔ ${nameOf(v.source)} deals ${v.amount} to ${tgtName(v.target)}`;
    case "SpellCast": return `P${v.controller} casts ${stackName(v.spell)}${stackTgtSuffix(v.spell)}`;
    case "ObjectMoved": return `${nameOf(v.obj)} → ${v.to}`;
    case "PermanentDied": return `💀 ${nameOf(v.obj)} dies`;
    case "ValueChosen": return `P${v.player} ${v.label} = ${v.value}`;
    case "GameEnded": return `GAME OVER — winner ${v.winner == null ? "draw" : "P" + v.winner}`;
    default: return `${k} ${JSON.stringify(v)}`;
  }
}
function el(tag: string, cls?: string, text?: string): HTMLElement {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (text != null) e.textContent = text;
  return e;
}
function clamp(n: number, lo: number, hi: number): number { return Math.max(lo, Math.min(hi, isNaN(n) ? lo : n)); }
function log(line: string): void { const d = $("log"); d.textContent += line + "\n"; d.scrollTop = d.scrollHeight; }

// ── keyboard shortcuts (MTGA-style; see ../mtga-re/docs/priority_stops.md) ─────────────────────
// Space = pass priority / take the sole default action. Enter = pass through ALL of this turn's
// remaining priority stops at once (mirrors the GRE's PerformActionResp.autoPassPriority=Yes /
// AutoPassOption.Turn — a per-turn hold that lapses next turn). Esc cancels the hold.
function isPriorityPrompt(p: Any): boolean { return !!p && p.mode === "action" && p.canPass; }

// (The web stop policy now lives entirely in the engine's `should_auto_pass` — it surfaces only the
// real stop windows, so every `decide` we receive IS a stop. The old client-side `priorityAutoPass`
// narrowing over a forced superset is gone; we just render what the engine sends.)
function autoPassEngaged(): boolean { return autoPassTurn !== null && view && view.turn === autoPassTurn; }
function autoPassBadge(): void {
  let b = $("autopassBadge");
  if (!b) { b = el("div", "autopass-badge"); b.id = "autopassBadge"; document.body.appendChild(b); }
  b.textContent = "Passing this turn's stops — Enter or Esc to cancel";
  b.style.display = autoPassTurn !== null ? "block" : "none";
}
function spacePass(): void {
  if (!cur) return;
  const p = cur.prompt;
  // 1) A valid in-progress selection → accept/submit it (e.g. declare attackers, targets).
  if (p.mode === "selectMany") {
    const n = multi.size;
    if (n >= p.min && n <= p.max) { send({ picks: [...multi].sort((a, b) => a - b) }); return; }
    if (p.canPass) { send({ pass: true }); return; }
    return;                                                            // selection not yet valid → wait
  }
  if (p.mode === "order") { if (orderSeq.length === p.options.length) send({ order: orderSeq.slice() }); return; }
  if (p.mode === "number") {
    const inp = document.querySelector("#prompt input") as HTMLInputElement | null;
    if (inp) send({ number: clamp(parseInt(inp.value || "0", 10), p.numMin, p.numMax) });
    return;
  }
  // 2) Priority / optional → pass (the default action).
  if (p.canPass) { send({ pass: true }); return; }
  // 3) A single forced option → take it.
  if ((p.mode === "action" || p.mode === "selectOne") && (p.options || []).length === 1) send({ picks: [0] });
}
function toggleAutoPass(): void {
  if (!view) return;
  if (autoPassTurn === view.turn) { autoPassTurn = null; autoPassBadge(); return; } // pressed again → off
  autoPassTurn = view.turn; autoPassBadge();
  if (cur && isPriorityPrompt(cur.prompt)) send({ pass: true });     // pass the open window now; chain continues
}
window.addEventListener("keydown", (e) => {
  if ($("modal").classList.contains("show")) { if (e.key === "Escape") $("modal").classList.remove("show"); return; }
  const tag = (e.target as HTMLElement | null)?.tagName;
  if (tag === "INPUT" || tag === "TEXTAREA") return;                  // don't hijack number entry / text fields
  // Replay mode: Space = play/pause, ←/→ = step a frame.
  if (replayId) {
    if (e.code === "Space" || e.key === " ") { e.preventDefault(); togglePlay(); }
    else if (e.key === "ArrowLeft") { e.preventDefault(); stepReplay(-1); }
    else if (e.key === "ArrowRight") { e.preventDefault(); stepReplay(1); }
    return;
  }
  if (e.code === "Space" || e.key === " ") { e.preventDefault(); spacePass(); }
  else if (e.key === "Enter") { e.preventDefault(); toggleAutoPass(); }
  else if (e.key === "Escape") { if (autoPassTurn !== null) { autoPassTurn = null; autoPassBadge(); } }
});
