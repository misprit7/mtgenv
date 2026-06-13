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
let previewUrl: string | null = null;
let stopsView: Any = null; // live stop config echoed by the server
let autoPassTurn: number | null = null; // Enter-hold: turn we're passing all priority stops through
const deckView: Any = {}; // seat → starting decklist (debug peek; RL-safe, pushed once at setup)

// Card art: a baked manifest (grp_id → art_crop/artist), batch-resolved from Scryfall once.
// No runtime Scryfall API calls — we only load the cached CDN images.
let artMap: Any = {};
fetch("/card-art.json").then((r) => r.json()).then((m) => { artMap = m; render(); }).catch(() => {});

const params = new URLSearchParams(location.search);
$("decks").textContent = `P0=${params.get("p0") || "demo"} · P1=${params.get("p1") || "demo"}`;
// Stops control (top bar): LIVE toggles — send setOption; the server echoes the new config and
// the running game's agent honours it at the next window (no reset). Rendered from stopsView.
const OPT: Array<[string, string, string]> = [
  ["auto-pass", "auto_pass", "autopass"], ["smart", "smart_stops", "smartstops"],
  ["full-ctrl", "full_control", "fullcontrol"], ["resolve", "resolve_own_stack", "resolvestack"],
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
function setOption(key: string, on: boolean): void { ws.send(JSON.stringify({ type: "setOption", key, on })); }

const wsProto = location.protocol === "https:" ? "wss://" : "ws://";
const ws = new WebSocket(`${wsProto}${location.host}/ws${location.search}`);
ws.onopen = () => { $("conn").textContent = "● connected";
  log("Keys: Space = pass priority / take the only action · Enter = pass through this turn's stops (Esc cancels)"); };
ws.onclose = () => ($("conn").textContent = "○ disconnected");
ws.onerror = () => ($("conn").textContent = "connection error");
ws.onmessage = (e) => handle(JSON.parse(e.data));

function handle(m: Any): void {
  if (m.type === "event") { view = m.view; logEvent(m.event); render(); }
  else if (m.type === "decide") {
    view = m.view; cur = m; multi.clear(); orderSeq = [];
    // Enter-engaged "pass through this turn's stops" lapses when the turn advances (MTGA parity).
    if (autoPassTurn !== null && view.turn !== autoPassTurn) { autoPassTurn = null; autoPassBadge(); }
    // While engaged, silently pass priority windows (still surface real choices: targets, blocks, …).
    if (autoPassEngaged() && isPriorityPrompt(cur.prompt)) { send({ pass: true }); return; }
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
  if (!cur) return;
  ws.send(JSON.stringify(Object.assign(
    { type: "response", id: cur.id, picks: [], number: null, pass: false, order: [] }, payload)));
  cur = null; multi.clear();
  $("prompt").innerHTML = '<div class="waiting">Waiting for the opponent…</div>';
  render();
}

// ── object/view helpers ─────────────────────────────────────────────────────
function norm(o: Any): Any {
  if (o.Hidden) return { hidden: true, id: o.Hidden.id, controller: o.Hidden.controller };
  const v = o.Visible;
  return { id: v.id, chars: v.chars, tapped: !!(v.status && v.status.tapped),
    sick: v.summoning_sick, dmg: v.damage_marked || 0, counters: v.counters,
    controller: v.controller, owner: v.owner };
}
const meSeat = (): number => view.seat;
const oppId = (): number | null => { const p = view.players.find((p: Any) => p.player !== meSeat()); return p ? p.player : null; };
function bfOf(pid: number): Any[] { return view.battlefield.map(norm).filter((c: Any) => c.controller === pid); }
function isLand(chars: Any): boolean { return (chars.card_types || chars.cardTypes || []).includes("Land"); }
function pub(pid: number | null): Any { return view.players.find((p: Any) => p.player === pid) || {}; }
function legalIdx(id: number): number { return cur ? (cur.prompt.option_objs || cur.prompt.optionObjs || []).indexOf(id) : -1; }

// ── render ───────────────────────────────────────────────────────────────────
function render(): void {
  if (!view) return;
  $("turn").textContent = `Turn ${view.turn} · ${view.phase} · active P${view.active_player}` +
    (view.priority_player != null ? ` · priority P${view.priority_player}` : "");
  renderRail();
  renderHalf("oppoHalf", oppId(), true);
  renderHalf("youHalf", meSeat(), false);
  renderStack();
  renderStepBar();
  renderHand();
  if (cur) renderPrompt();
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
  lz.appendChild(el("div", "dotlbl", "you"));
  lz.appendChild(el("div", "dotlbl", "opp"));
  legend.appendChild(lz);
  bar.appendChild(legend);
  STEPS.forEach((st) => {
    const cell = el("div", "step" + (view.phase === st.phase ? " cur" : ""));
    cell.appendChild(el("div", "slabel", st.label));
    if (st.stop) {
      const s = stops[st.phase] || { mine: false, opp: false };
      const dots = el("div", "sdots");
      dots.appendChild(stopDot(st, true, s.mine));   // your turn (top)
      dots.appendChild(stopDot(st, false, s.opp));   // opponent's turn (bottom)
      cell.appendChild(dots);
    }
    bar.appendChild(cell);
  });
}
function stopDot(st: Any, own: boolean, on: boolean): HTMLElement {
  const dot = el("div", "sdot" + (own ? " you" : " opp") + (on ? " on" : ""));
  const side = own ? "YOUR" : "the opponent's";
  dot.title = (on ? "Remove stop on " : "Stop on ") + side + " " + st.label + " (get priority there)";
  dot.onclick = (e) => { e.stopPropagation(); toggleStop(st.phase, own, !on); };
  return dot;
}
function toggleStop(phase: string, own: boolean, on: boolean): void {
  // LIVE: the server mutates the shared per-(step, side) stop config + echoes it; the running game's
  // agent honours it at the next priority window — no game reset. `own` = your turn's copy of `step`.
  ws.send(JSON.stringify({ type: "setStop", step: phase, own, on }));
}

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
  rail.appendChild(pinfoEl(pub(meSeat()), true));
}

const STEP_ABBR: Any = {
  PrecombatMain: "MP1", PostcombatMain: "MP2", DeclareAttackers: "ATK", DeclareBlockers: "BLK",
  Upkeep: "UP", Draw: "DR", BeginCombat: "BC", CombatDamage: "CD", EndCombat: "EC", End: "END",
  Untap: "UN", Cleanup: "CL",
};
function stopsSummary(ss: Any): string {
  if (ss.full_control) return "🛑 full control";
  const active = (ss.per_step || []).filter((s: Any) => s[1]).map((s: Any) => STEP_ABBR[s[0]] || s[0]);
  let s = "stops: " + (active.length ? active.join(", ") : "—");
  const flags: string[] = [];
  if (ss.smart_stops) flags.push("smart");
  if (!ss.resolve_own_stack) flags.push("respond-self");
  if (flags.length) s += " · " + flags.join(", ");
  return s;
}

function pinfoEl(p: Any, you: boolean): HTMLElement {
  const d = el("div", "pinfo" + (view.active_player === p.player ? " active" : ""));
  const who = el("div", "who");
  who.innerHTML = `Player ${p.player}` + (you ? ' <span class="you">YOU</span>' : "");
  d.appendChild(who);
  d.appendChild(el("div", "life" + (p.life <= 5 ? " low" : ""), `♥ ${p.life}`));
  const piles = el("div", "piles");
  const deck = you ? deckView[p.player] : null; // your starting decklist (debug peek)
  const libPile = pileEl("Lib", p.library_count ?? p.libraryCount, null, `P${p.player} library`, true);
  if (deck) {
    libPile.classList.add("clk");
    libPile.title = "Your starting decklist";
    libPile.onclick = () => openDecklist(`P${p.player} decklist`, deck);
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
  d.onclick = () => openZone(title, hidden ? null : objs || []);
  return d;
}

function renderHalf(elId: string, pid: number | null, isOppo: boolean): void {
  const host = $(elId);
  host.innerHTML = "";
  if (pid == null) return;
  const perms = bfOf(pid);
  const lands = perms.filter((c) => c.chars && isLand(c.chars));
  const nonlands = perms.filter((c) => !(c.chars && isLand(c.chars)));
  const landRow = zoneRow("lands", lands);
  const creatureRow = zoneRow("", nonlands);
  if (isOppo) { host.appendChild(landRow); host.appendChild(creatureRow); }
  else { host.appendChild(creatureRow); host.appendChild(landRow); }
}

function zoneRow(cls: string, cards: Any[]): HTMLElement {
  const row = el("div", "row " + cls);
  if (!cards.length) { row.appendChild(el("span", "rowlabel", cls === "lands" ? "lands" : "—")); return row; }
  cards.forEach((c) => row.appendChild(cardEl(c, { interactive: true })));
  return row;
}

function renderStack(): void {
  const s = $("stack");
  s.innerHTML = "";
  (view.stack || []).forEach((it: Any) => s.appendChild(cardEl({ id: it.id, chars: it.chars }, {})));
}

function renderHand(): void {
  const h = $("hand");
  h.innerHTML = "";
  h.appendChild(el("div", "hlabel", "hand"));
  const hand = view.me.hand || [];
  hand.forEach((o: Any) => h.appendChild(cardEl(norm(o), { interactive: true, hand: true })));
  if (!hand.length) h.appendChild(el("span", "waiting", "(empty hand)"));
}

// ── card frame ─────────────────────────────────────────────────────────────
function cardEl(c: Any, ctx: Any): HTMLElement {
  if (c.hidden) return el("div", "card back");
  const chars = c.chars || {};
  const idx = ctx.interactive ? legalIdx(c.id) : -1;
  const selected = ctx.interactive && idx >= 0 && multi.has(idx);
  const d = el("div", ["card", colorClass(chars), c.tapped ? "tapped" : "", c.sick ? "sick" : "",
    idx >= 0 ? "legal" : "", selected ? "selected" : ""].filter(Boolean).join(" "));

  const hdr = el("div", "c-hdr");
  hdr.appendChild(el("div", "c-name", chars.name || "—"));
  const mana = el("div", "c-mana");
  manaPips(chars).forEach((p) => mana.appendChild(p));
  hdr.appendChild(mana);
  d.appendChild(hdr);

  const art = el("div", "c-art");
  const info = artMap[chars.grp_id];
  if (info && info.art) {
    art.style.backgroundImage = `url('${info.art}')`;
    art.style.backgroundSize = "cover";
  }
  d.appendChild(art);
  // Hover → full card image preview (follows the cursor).
  if (info && info.img) {
    d.addEventListener("mouseenter", (e) => showPreview(info.img, e as MouseEvent));
    d.addEventListener("mousemove", (e) => { if (previewUrl) positionPreview(e as MouseEvent); });
    d.addEventListener("mouseleave", hidePreview);
  }
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

  if (idx >= 0) d.onclick = () => onCardClick(idx);
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
  const cols = chars.colors || [];
  const cmc = chars.mana_value ?? chars.manaValue ?? 0;
  const generic = Math.max(0, cmc - cols.length);
  if (generic > 0 || cmc === 0) out.push(symImg(String(generic)));
  cols.forEach((c: string) => out.push(symImg(CODE[c] || "C")));
  return out;
}
// Replace {T}/{G}/… tokens in oracle text with their Scryfall symbol SVGs.
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
function onCardClick(idx: number): void {
  if (!cur) return;
  const mode = cur.prompt.mode;
  if (mode === "action" || mode === "selectOne") send({ picks: [idx] });
  else if (mode === "selectMany") { if (multi.has(idx)) multi.delete(idx); else multi.add(idx); render(); renderPrompt(); }
}

// ── prompt panel (non-card options + controls) ───────────────────────────────
function renderPrompt(): void {
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

  const opts = el("div", "opts");
  let boardCount = 0;
  p.options.forEach((label: string, i: number) => {
    if (objs[i] != null) { boardCount++; return; }
    const b = el("button", "opt" + (multi.has(i) ? " sel" : ""), label) as HTMLButtonElement;
    b.onclick = () => {
      if (p.mode === "selectMany") { if (multi.has(i)) multi.delete(i); else multi.add(i); renderPrompt(); }
      else send({ picks: [i] });
    };
    opts.appendChild(b);
  });
  root.appendChild(opts);
  if (boardCount) root.appendChild(el("div", "hint",
    "→ click the highlighted cards on the board" + (p.mode === "selectMany" ? " to select" : "") + "."));

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
function openZone(title: string, objs: Any[] | null): void {
  const g = $("modalGrid"); g.innerHTML = "";
  if (objs == null) {
    $("modalTitle").textContent = `${title} (hidden)`;
    g.innerHTML = '<div class="waiting">This zone is hidden — its contents aren\'t in your view.</div>';
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
    if (art && art.img) {
      row.addEventListener("mouseenter", (ev) => showPreview(art.img, ev as MouseEvent));
      row.addEventListener("mousemove", (ev) => { if (previewUrl) positionPreview(ev as MouseEvent); });
      row.addEventListener("mouseleave", hidePreview);
    }
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

// ── misc ───────────────────────────────────────────────────────────────────────
function nameOf(id: number): string {
  const all: Any[] = ([] as Any[]).concat(view.me.hand || [], view.battlefield || []);
  (view.players || []).forEach((p: Any) => all.push(...(p.graveyard || []), ...(p.exile_public || p.exilePublic || [])));
  for (const o of all) if (o.Visible && o.Visible.id === id) return o.Visible.chars.name;
  for (const s of (view.stack || [])) if (s.source === id) return s.chars.name; // resolving spell/ability
  return "#" + id;
}
function stackName(sid: number): string { for (const s of (view.stack || [])) if (s.id === sid) return s.chars.name; return "spell #" + sid; }
function tgtName(t: Any): string {
  if (t == null) return "?";
  if (t.Player != null) return "P" + t.Player;
  if (t.Object != null) return nameOf(t.Object);
  if (t.Stack != null) return "spell #" + t.Stack;
  return "?";
}
// ── hover full-card preview ────────────────────────────────────────────────────
function showPreview(url: string, ev: MouseEvent): void {
  previewUrl = url;
  const p = $("preview") as HTMLImageElement;
  p.src = url; p.style.display = "block"; positionPreview(ev);
}
function hidePreview(): void { previewUrl = null; $("preview").style.display = "none"; }
function positionPreview(ev: MouseEvent): void {
  const p = $("preview"); const w = 320, h = 446;
  let x = ev.clientX + 22; let y = ev.clientY - h / 2;
  if (x + w > window.innerWidth) x = ev.clientX - w - 22;
  y = Math.max(8, Math.min(y, window.innerHeight - h - 8));
  p.style.left = `${x}px`; p.style.top = `${y}px`;
}

function eventText(ev: Any): string | null {
  const k = Object.keys(ev)[0]; const v = ev[k];
  switch (k) {
    case "PhaseBegan": case "Revealed": return null;
    case "DrewCards": return `P${v.player} draws ${v.count}`;
    case "LifeChanged": return `P${v.player} life ${v.delta >= 0 ? "+" : ""}${v.delta} → ${v.new_total}`;
    case "DamageDealt": return `⚔ ${nameOf(v.source)} deals ${v.amount} to ${tgtName(v.target)}`;
    case "SpellCast": return `P${v.controller} casts ${stackName(v.spell)}`;
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
  if (p.canPass) { send({ pass: true }); return; }                   // priority/optional → pass (default)
  if ((p.mode === "action" || p.mode === "selectOne") && (p.options || []).length === 1) {
    send({ picks: [0] });                                            // sole forced option → take it
  }
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
  if (e.code === "Space" || e.key === " ") { e.preventDefault(); spacePass(); }
  else if (e.key === "Enter") { e.preventDefault(); toggleAutoPass(); }
  else if (e.key === "Escape") { if (autoPassTurn !== null) { autoPassTurn = null; autoPassBadge(); } }
});
