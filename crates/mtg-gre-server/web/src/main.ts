// mtgenv web client (TS/Vite). Talks the JSON projection of the Agent boundary (protocol.rs):
// it renders the information-filtered PlayerView and, on each `decide`, draws ONLY the
// engine-enumerated legal options (the masking) — an illegal move is unrepresentable.

// ── wire types (the JSON projection; PlayerView fields are snake_case) ──────────────────────
type Mode = "action" | "selectOne" | "selectMany" | "number" | "order";

interface Prompt {
  title: string;
  mode: Mode;
  options: string[];
  canPass: boolean;
  min: number;
  max: number;
  numMin: number;
  numMax: number;
}

interface CharsView {
  name: string;
  card_types: string[];
}
interface VisibleObj {
  id: number;
  chars: CharsView;
  controller: number;
  status: { tapped: boolean };
}
type ObjView = { Visible: VisibleObj } | { Hidden: { id: number; controller: number } };

interface PublicView {
  player: number;
  life: number;
  hand_count: number;
  library_count: number;
  graveyard: ObjView[];
}
interface PlayerView {
  seat: number;
  turn: number;
  active_player: number;
  phase: string;
  players: PublicView[];
  me: { hand: ObjView[] };
  battlefield: ObjView[];
  stack: { chars: CharsView }[];
}

type ServerMsg =
  | { type: "event"; event: Record<string, unknown>; view: PlayerView }
  | { type: "decide"; id: number; prompt: Prompt; view: PlayerView }
  | { type: "gameOver"; winner: number | null }
  | { type: "log"; text: string };

interface Decide {
  id: number;
  prompt: Prompt;
  view: PlayerView;
}

// ── state ────────────────────────────────────────────────────────────────────────────────────
const $ = (id: string) => document.getElementById(id) as HTMLElement;
let view: PlayerView | null = null;
let cur: Decide | null = null;
const multi = new Set<number>();

// Deck selection: ?p0=&p1= (burn/bears/demo) flows through to the server via the WS query.
const deckParams = new URLSearchParams(location.search);
$("decks").textContent = `you (P0) = ${deckParams.get("p0") || "demo"} · opponent (P1) = ${
  deckParams.get("p1") || "demo"
}`;
const wsProto = location.protocol === "https:" ? "wss://" : "ws://";
const ws = new WebSocket(`${wsProto}${location.host}/ws${location.search}`);
ws.onopen = () => ($("conn").textContent = "connected");
ws.onclose = () => ($("conn").textContent = "disconnected");
ws.onerror = () => ($("conn").textContent = "connection error");
ws.onmessage = (e) => handle(JSON.parse(e.data) as ServerMsg);

function handle(m: ServerMsg): void {
  switch (m.type) {
    case "event":
      view = m.view;
      log(eventText(m.event));
      render();
      break;
    case "decide":
      view = m.view;
      cur = m;
      multi.clear();
      render();
      renderPrompt(m.prompt);
      break;
    case "gameOver":
      cur = null;
      renderEnd(m.winner);
      break;
    case "log":
      log(m.text);
      break;
  }
}

interface Reply {
  picks?: number[];
  number?: number | null;
  pass?: boolean;
  order?: number[];
}
function send(r: Reply): void {
  if (!cur) return;
  ws.send(
    JSON.stringify({
      type: "response",
      id: cur.id,
      picks: r.picks ?? [],
      number: r.number ?? null,
      pass: r.pass ?? false,
      order: r.order ?? [],
    })
  );
  cur = null;
  multi.clear();
  $("prompt").innerHTML = '<div class="waiting">Waiting for the opponent…</div>';
  render();
}

// ── rendering ──────────────────────────────────────────────────────────────────────────────
function render(): void {
  if (!view) return;
  const me = view.seat;
  const opp = view.players.find((p) => p.player !== me);
  const mine = view.players.find((p) => p.player === me);
  $("opp").innerHTML = opp ? seatHtml(opp, false) + zoneHtml("Battlefield", bfOf(opp.player)) : "";
  $("me").innerHTML =
    (mine ? seatHtml(mine, true) : "") +
    zoneHtml("Battlefield", bfOf(me)) +
    zoneHtml("Hand", view.me.hand, legalCardIds());
  $("stack").innerHTML = view.stack.length
    ? view.stack.map((s) => cardHtml(s.chars.name, [], false, false)).join("")
    : '<span class="empty">empty</span>';
}

function seatHtml(p: PublicView, you: boolean): string {
  const tag = you ? '<span class="you"> (you)</span>' : "";
  const turnline = you
    ? `<div class="stat">Turn ${view!.turn} · ${view!.phase} · active P${view!.active_player}</div>`
    : "";
  return (
    `<div class="seat"><div class="who">Player ${p.player}${tag}</div>` +
    `<div class="life">♥ ${p.life}</div></div>` +
    `<div class="stat">hand ${p.hand_count} · library ${p.library_count} · grave ${p.graveyard.length}</div>` +
    turnline
  );
}

function bfOf(pid: number): ObjView[] {
  return view!.battlefield.filter((o) => "Visible" in o && o.Visible.controller === pid);
}

function zoneHtml(label: string, objs: ObjView[], legalIds?: Set<number>): string {
  const items = objs.map((o) => objCardHtml(o, legalIds)).join("");
  return `<h2>${label}</h2><div class="zone">${items || '<span class="empty">empty</span>'}</div>`;
}

function objCardHtml(o: ObjView, legalIds?: Set<number>): string {
  if ("Hidden" in o) return cardHtml("(hidden)", [], false, false);
  const v = o.Visible;
  const legal = !!legalIds && legalIds.has(v.id);
  return cardHtml(v.chars.name, v.chars.card_types ?? [], !!v.status?.tapped, legal);
}

function cardHtml(name: string, types: string[], tapped: boolean, legal: boolean): string {
  const isLand = types.includes("Land");
  const cls = ["card", isLand ? "land" : "", tapped ? "tapped" : "", legal ? "legal" : ""]
    .filter(Boolean)
    .join(" ");
  const ty = types.join(" ");
  return `<div class="${cls}"><div class="nm">${esc(name)}</div>${ty ? `<div class="ty">${esc(ty)}</div>` : ""}</div>`;
}

// hand-card ids that map to a legal action in the current prompt (masking highlight)
function legalCardIds(): Set<number> {
  const s = new Set<number>();
  if (cur && cur.prompt.mode === "action") {
    const names = cur.prompt.options.map((o) => o.replace(/^.*?—\s*/, "").trim());
    for (const o of view!.me.hand) {
      if ("Visible" in o && names.includes(o.Visible.chars.name)) s.add(o.Visible.id);
    }
  }
  return s;
}

// ── prompt (the enumerated legal options, rendered) ────────────────────────────────────────
function renderPrompt(p: Prompt): void {
  const root = $("prompt");
  root.innerHTML = "";
  root.appendChild(el("div", "title", p.title));

  if (p.mode === "action" || p.mode === "selectOne") {
    const opts = el("div", "opts");
    p.options.forEach((label, i) => {
      const b = el("button", "opt", label) as HTMLButtonElement;
      b.onclick = () => send({ picks: [i] });
      opts.appendChild(b);
    });
    root.appendChild(opts);
    if (p.mode === "action") {
      const row = el("div", "row");
      const pass = el("button", "pass", p.canPass ? "Pass priority" : "Pass") as HTMLButtonElement;
      pass.onclick = () => send({ pass: true });
      row.appendChild(pass);
      root.appendChild(row);
    }
  } else if (p.mode === "selectMany") {
    const opts = el("div", "opts");
    p.options.forEach((label, i) => {
      const b = el("button", "opt", label) as HTMLButtonElement;
      b.onclick = () => {
        if (multi.has(i)) multi.delete(i);
        else multi.add(i);
        b.classList.toggle("sel");
        updateSubmit(p);
      };
      opts.appendChild(b);
    });
    root.appendChild(opts);
    root.appendChild(el("div", "hint", `Choose ${p.min}–${p.max}.`));
    const row = el("div", "row");
    const submit = el("button", "act", "Submit") as HTMLButtonElement;
    submit.id = "submitMany";
    submit.onclick = () => send({ picks: [...multi].sort((a, b) => a - b) });
    row.appendChild(submit);
    root.appendChild(row);
    updateSubmit(p);
  } else if (p.mode === "number") {
    const inp = el("input") as HTMLInputElement;
    inp.type = "number";
    inp.min = String(p.numMin);
    inp.max = String(p.numMax);
    inp.value = String(p.numMin);
    root.appendChild(inp);
    root.appendChild(el("div", "hint", `Enter ${p.numMin}–${p.numMax}.`));
    const row = el("div", "row");
    const submit = el("button", "act", "Submit") as HTMLButtonElement;
    submit.onclick = () => send({ number: clamp(parseInt(inp.value || "0", 10), p.numMin, p.numMax) });
    row.appendChild(submit);
    root.appendChild(row);
  } else if (p.mode === "order") {
    const opts = el("div", "opts");
    p.options.forEach((label, i) => opts.appendChild(el("div", "opt", `${i + 1}. ${label}`)));
    root.appendChild(opts);
    const row = el("div", "row");
    const submit = el("button", "act", "Keep this order") as HTMLButtonElement;
    submit.onclick = () => send({ order: p.options.map((_, i) => i) });
    row.appendChild(submit);
    root.appendChild(row);
  }
}

function updateSubmit(p: Prompt): void {
  const b = document.getElementById("submitMany") as HTMLButtonElement | null;
  if (!b) return;
  b.disabled = multi.size < p.min || multi.size > p.max;
}

function renderEnd(winner: number | null): void {
  const w = winner === null ? "draw" : `Player ${winner}`;
  const youWon = !!view && winner === view.seat;
  $("prompt").innerHTML = `<div class="banner">Game over — winner: ${w}${youWon ? " 🎉 (you!)" : ""}</div>`;
  log(`GAME OVER — winner: ${w}`);
}

// ── helpers ──────────────────────────────────────────────────────────────────────────────────
function eventText(ev: Record<string, unknown>): string {
  const k = Object.keys(ev)[0];
  return `${k} ${JSON.stringify(ev[k])}`;
}
function el(tag: string, cls?: string, text?: string): HTMLElement {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (text != null) e.textContent = text;
  return e;
}
function clamp(n: number, lo: number, hi: number): number {
  return Math.max(lo, Math.min(hi, isNaN(n) ? lo : n));
}
function esc(s: string): string {
  return s.replace(/[&<>]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;" }[c] as string));
}
function log(line: string): void {
  const d = $("log");
  d.textContent += `${line}\n`;
  d.scrollTop = d.scrollHeight;
}
