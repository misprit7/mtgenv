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

// Card art: a baked manifest (grp_id → art_crop/artist), batch-resolved from Scryfall once.
// No runtime Scryfall API calls — we only load the cached CDN images.
let artMap: Any = {};
fetch("/card-art.json").then((r) => r.json()).then((m) => { artMap = m; render(); }).catch(() => {});

const params = new URLSearchParams(location.search);
$("decks").textContent = `P0=${params.get("p0") || "demo"} · P1=${params.get("p1") || "demo"}`;
// MTGA-style stops: auto-pass on by default; toggle links start a new game with the new setting.
{
  const ap = params.get("autopass") !== "0";
  const fc = ["1", "on", "true"].includes((params.get("fullcontrol") || "").toLowerCase());
  const link = (label: string, key: string, cur: boolean): string => {
    const p = new URLSearchParams(location.search); p.set(key, cur ? "0" : "1");
    return `<a href="?${p.toString()}">${label}: ${cur ? "on" : "off"}</a>`;
  };
  $("stops").innerHTML = `stops: ${link("auto-pass", "autopass", ap)} · ${link("full-control", "fullcontrol", fc)}`;
}

const wsProto = location.protocol === "https:" ? "wss://" : "ws://";
const ws = new WebSocket(`${wsProto}${location.host}/ws${location.search}`);
ws.onopen = () => ($("conn").textContent = "● connected");
ws.onclose = () => ($("conn").textContent = "○ disconnected");
ws.onerror = () => ($("conn").textContent = "connection error");
ws.onmessage = (e) => handle(JSON.parse(e.data));

function handle(m: Any): void {
  if (m.type === "event") { view = m.view; logEvent(m.event); render(); }
  else if (m.type === "decide") { view = m.view; cur = m; multi.clear(); orderSeq = []; render(); }
  else if (m.type === "gameOver") { cur = null; renderEnd(m.winner); }
  else if (m.type === "log") { log(m.text); }
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
  renderHand();
  if (cur) renderPrompt();
}

function renderRail(): void {
  const rail = $("rail");
  rail.innerHTML = "";
  rail.appendChild(pinfoEl(pub(oppId()), false));
  const ph = el("div", "phasebar");
  ph.innerHTML = `turn <b>${view.turn}</b><br>${view.phase}<br>active <b>P${view.active_player}</b>`;
  rail.appendChild(ph);
  rail.appendChild(pinfoEl(pub(meSeat()), true));
}

function pinfoEl(p: Any, you: boolean): HTMLElement {
  const d = el("div", "pinfo" + (view.active_player === p.player ? " active" : ""));
  const who = el("div", "who");
  who.innerHTML = `Player ${p.player}` + (you ? ' <span class="you">YOU</span>' : "");
  d.appendChild(who);
  d.appendChild(el("div", "life" + (p.life <= 5 ? " low" : ""), `♥ ${p.life}`));
  const piles = el("div", "piles");
  piles.appendChild(pileEl("Lib", p.library_count ?? p.libraryCount, null, ""));
  piles.appendChild(pileEl("GY", (p.graveyard || []).length, p.graveyard, `P${p.player} graveyard`));
  const exile = p.exile_public || p.exilePublic || [];
  piles.appendChild(pileEl("Exile", exile.length, exile, `P${p.player} exile`));
  d.appendChild(piles);
  return d;
}

function pileEl(label: string, n: number, objs: Any[] | null, title: string): HTMLElement {
  const d = el("div", "pile");
  d.innerHTML = `<div class="n">${n}</div><div class="l">${label}</div>`;
  if (objs && objs.length) d.onclick = () => openZone(title, objs);
  else d.style.cursor = "default";
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
    art.title = "Illustrated by " + (info.artist || "?");
    if (info.artist) art.appendChild(el("div", "credit", "🖌 " + info.artist));
  }
  d.appendChild(art);
  d.appendChild(el("div", "c-type", typeLine(chars)));
  const rules = el("div", "c-rules");
  rules.innerHTML = chars.rules_text ? renderText(chars.rules_text) : esc((chars.keywords || []).join(", "));
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
function openZone(title: string, objs: Any[]): void {
  $("modalTitle").textContent = `${title} (${objs.length})`;
  const g = $("modalGrid"); g.innerHTML = "";
  objs.map(norm).forEach((c) => g.appendChild(cardEl(c, {})));
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
