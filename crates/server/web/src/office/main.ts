// 사무실 화면의 입구: 연결 · 상태 · 채팅 칸 · 화면 고르기(3D 또는 가벼운 화면).
//
// 서버와 주고받는 것(모두 같은 집의 상대 경로):
// - POST /api/session                    세션을 얻는다(관문).
// - GET  /api/office                     접속 스냅샷: {agents:[{agent,place,work}], pending:[응답 객체], last_seq}
// - POST /api/office/chat                담당에게 말하기(C5): {room, agent, text, sourceMessageId} → 응답 객체
// - WS   /ws?last_seq=N                  번호 있는 이벤트 `office.agent` · `office.chat`를 받는다(C2 · E3).
// - WS   이벤트 `office.approval`        승인 카드(10c 14번): 폰과 같은 길 · 같은 지문 대조로 승인 · 거절.
// 스냅샷과 말하기 길은 조립(price-inha)이 Office에 잇는다. 없으면 이벤트만으로 그린다.
import "./office.css";
import { API } from "../paths";
import { KIND_OFFICE_APPROVAL, applyApproval } from "./approval";
import type { OfficeMap, Scene } from "./scene3d";
import { trimLines, type Line } from "./trim";

const SESSION_PATH = API.session;
const SNAPSHOT_PATH = API.office;
const CHAT_PATH = API.officeChat;
const WS_PATH = API.socket;
const MAP_PATH = "./office-map.json";
const RETRY_MIN_MS = 1000;
const RETRY_MAX_MS = 10000;
const FOLLOW_PX = 80;

const AGENT_IDS = ["news", "strategy", "risk", "review", "trader", "leader"];
const PLACE_LABEL: Record<string, string> = { bed: "자기 방 침대", desk: "사무실 자기 자리", meeting: "논의 탁자" };
const WORK_LABEL: Record<string, string> = { idle: "", queued: "대기", thinking: "생각 중", failed: "실패" };
const ROOM_TEAM = "team";

const $ = <T extends HTMLElement = HTMLElement>(id: string): T => document.getElementById(id) as T;

/** 담당 하나의 자리 · 일. */
interface AgentView { place: string; work: string }
/** 이벤트 `office.agent`의 몸통. */
interface AgentEvent extends AgentView { agent: string }
/** 응답 객체(서버 Office의 ChatResponse). */
interface ChatResponse {
  requestId: string;
  room: string;
  agent: string;
  sourceMessageId?: string;
  status: string;
  content?: string;
  error?: string;
  updatedAt?: unknown;
}
type MyLine = Extract<Line, { kind: "me" }>;

const state = {
  map: null as OfficeMap | null,
  names: {} as Record<string, string>,
  agents: new Map<string, AgentView>(AGENT_IDS.map((id) => [id, { place: "bed", work: "idle" }])),
  responses: new Map<string, ChatResponse>(), // requestId → 응답 객체
  lines: new Map<string, Line[]>(),            // 대화 키 → [{kind:"me"|"resp", ...}]
  conversation: ROOM_TEAM as string,          // "team" 또는 담당 id
  lastSeq: null as number | null,
  connected: false,
  view: null as Scene | null,                  // 3D 장면(있으면)
};

// ---------- 화면: 3D 또는 가벼운 화면 ----------

function renderLight() {
  const list = $("light-list");
  list.replaceChildren();
  for (const id of AGENT_IDS) {
    const a = state.agents.get(id) as AgentView;
    const li = document.createElement("li");
    const button = document.createElement("button");
    button.type = "button";
    const name = document.createElement("strong");
    const work = WORK_LABEL[a.work] ? ` · ${WORK_LABEL[a.work]}` : "";
    name.textContent = `${state.names[id] ?? id}${work}`;
    const where = document.createElement("span");
    where.className = "where";
    where.textContent = PLACE_LABEL[a.place] ?? a.place;
    button.append(name, where);
    button.addEventListener("click", () => selectConversation(id));
    li.append(button);
    list.append(li);
  }
}

function showLight(reason: string): void {
  if (state.view) {
    try { state.view.dispose(); } catch { /* 이미 치움 */ }
    state.view = null;
  }
  $("stage").hidden = true;
  $("light").hidden = false;
  const toggle = $("view-toggle");
  toggle.hidden = !reason || reason === "lost" ? true : false;
  renderLight();
}

async function show3d(): Promise<void> {
  const { webglAvailable, createScene } = await import("./scene3d");
  if (!webglAvailable()) { throw new Error("webgl"); }
  $("stage").hidden = false;
  $("light").hidden = true;
  const scene = createScene({
    host: $("canvas-host"),
    labels: $("labels"),
    map: state.map as OfficeMap,
    names: state.names,
    onPick: (id: string) => selectConversation(id),
    onLost: () => showLight("lost"),
  });
  state.view = scene;
  for (const [id, view] of state.agents) { scene.setAgent(id, view, { snap: true }); }
}

function applyAgent(view: AgentEvent | null | undefined, snap: boolean): void {
  if (!view || !state.agents.has(view.agent)) { return; }
  const next = { place: view.place, work: view.work };
  state.agents.set(view.agent, next);
  if (state.view) { state.view.setAgent(view.agent, next, { snap }); }
  if (!$("light").hidden) { renderLight(); }
  renderTabs();
}

// ---------- 채팅 ----------

function convKeyOf(resp: ChatResponse): string { return resp.room === ROOM_TEAM ? ROOM_TEAM : resp.agent; }

function linesOf(key: string): Line[] {
  if (!state.lines.has(key)) { state.lines.set(key, []); }
  return state.lines.get(key) as Line[];
}

/** 응답 객체는 requestId별로 updatedAt이 크거나 같은 것만 받아 덮어쓴다(늦게 온 옛 상태는 버린다). */
function applyChat(resp: ChatResponse | null | undefined): void {
  if (!resp || typeof resp.requestId !== "string") { return; }
  const old = state.responses.get(resp.requestId);
  if (old && Number(old.updatedAt) > Number(resp.updatedAt)) { return; }
  state.responses.set(resp.requestId, resp);
  if (!old) {
    const lines = linesOf(convKeyOf(resp));
    // 자기 질문 바로 뒤에 붙인다. 질문이 이 화면에 없으면 끝에 붙인다.
    const at = lines.findIndex((l) => l.kind === "me" && l.id === resp.sourceMessageId);
    const entry: Line = { kind: "resp", requestId: resp.requestId };
    if (at >= 0) {
      let end = at + 1;
      for (;;) {
        const next = lines[end];
        if (end >= lines.length || next.kind !== "resp" ||
          state.responses.get(next.requestId)?.sourceMessageId !== resp.sourceMessageId) { break; }
        end++;
      }
      lines.splice(end, 0, entry);
    } else {
      lines.push(entry);
    }
    trimLines(lines, state.responses);
  }
  if (convKeyOf(resp) === state.conversation) { renderLog(false); }
}

function newId(): string {
  if (window.crypto?.randomUUID) { return window.crypto.randomUUID(); }
  return `m-${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
}

function nameOf(id: string): string { return state.names[id] ?? id; }

function renderTabs(): void {
  const tabs = $("chat-tabs");
  tabs.replaceChildren();
  for (const key of [ROOM_TEAM, ...AGENT_IDS]) {
    const b = document.createElement("button");
    b.type = "button";
    b.setAttribute("role", "tab");
    b.setAttribute("aria-selected", String(key === state.conversation));
    const glyphs: Record<string, string> = { queued: " ⏳", thinking: " 💭", failed: " ❗" };
    const glyph = key === ROOM_TEAM ? "" : glyphs[state.agents.get(key)?.work ?? ""] ?? "";
    b.textContent = (key === ROOM_TEAM ? "팀 방" : nameOf(key)) + glyph;
    b.addEventListener("click", () => selectConversation(key));
    tabs.append(b);
  }
  $("chat-to-wrap").hidden = state.conversation !== ROOM_TEAM;
}

function selectConversation(key: string): void {
  state.conversation = key;
  renderTabs();
  renderLog(true);
}

function lineElement(line: Line): HTMLElement | null {
  const div = document.createElement("div");
  if (line.kind === "me") {
    div.className = "msg me";
    const who = document.createElement("span");
    who.className = "who";
    who.textContent = line.room === ROOM_TEAM ? `나 → ${nameOf(line.agent)}` : "나";
    div.append(who, document.createTextNode(line.text));
    if (line.failed) {
      div.classList.add("failed");
      const retry = document.createElement("button");
      retry.type = "button";
      retry.className = "retry";
      retry.textContent = "다시 보내기";
      retry.addEventListener("click", () => { void send(line.room, line.agent, line.text, line); });
      div.append(document.createTextNode("\n보내지 못함"), retry);
    }
    return div;
  }
  const resp = state.responses.get(line.requestId);
  if (!resp) { return null; }
  const who = document.createElement("span");
  who.className = "who";
  who.textContent = nameOf(resp.agent);
  if (resp.status === "complete") {
    div.className = "msg agent";
    div.append(who, document.createTextNode(resp.content ?? ""));
  } else if (resp.status === "failed") {
    div.className = "msg status failed";
    div.textContent = `${nameOf(resp.agent)}: 실패 (${resp.error ?? "까닭 모름"})`;
  } else {
    div.className = "msg status";
    div.textContent = `${nameOf(resp.agent)}: ${WORK_LABEL[resp.status] ?? resp.status} …`;
  }
  return div;
}

function renderLog(forceBottom: boolean): void {
  const log = $("chat-log");
  const follow = forceBottom || log.scrollHeight - log.scrollTop - log.clientHeight <= FOLLOW_PX;
  log.replaceChildren();
  for (const line of linesOf(state.conversation)) {
    const el = lineElement(line);
    if (el) { log.append(el); }
  }
  if (follow) { log.scrollTop = log.scrollHeight; }
}

async function send(room: string, agent: string, text: string, existing?: MyLine): Promise<void> {
  const line: MyLine = existing ?? { kind: "me", id: newId(), room, agent, text };
  line.failed = false;
  if (!existing) {
    const lines = linesOf(room === ROOM_TEAM ? ROOM_TEAM : agent);
    lines.push(line);
    trimLines(lines, state.responses);
  }
  renderLog(true);
  try {
    const reply = await fetch(CHAT_PATH, {
      method: "POST",
      credentials: "same-origin",
      cache: "no-store",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ room, agent, text, sourceMessageId: line.id }),
    });
    if (!reply.ok) { throw new Error(String(reply.status)); }
    applyChat(await reply.json());
  } catch {
    line.failed = true;
  }
  renderLog(true);
}

function wireChat(): void {
  const select = $<HTMLSelectElement>("chat-to");
  for (const id of AGENT_IDS) {
    const option = document.createElement("option");
    option.value = id;
    option.textContent = nameOf(id);
    select.append(option);
  }
  select.value = "leader";
  const input = $<HTMLTextAreaElement>("chat-input");
  const form = $("chat-form");
  const submit = () => {
    const text = input.value;
    if (!text.trim() || !state.connected) { return; }
    const room = state.conversation === ROOM_TEAM ? ROOM_TEAM : "direct";
    const agent = room === ROOM_TEAM ? select.value : state.conversation;
    input.value = "";
    void send(room, agent, text);
  };
  form.addEventListener("submit", (e) => { e.preventDefault(); submit(); });
  input.addEventListener("keydown", (e) => {
    e.stopPropagation();
    if (e.key === "Enter" && !e.shiftKey && !e.isComposing) {
      e.preventDefault();
      submit();
    }
  });
  input.addEventListener("keyup", (e) => e.stopPropagation());
}

// ---------- 연결 ----------

let retryMs = RETRY_MIN_MS;

function setConn(text: string, kind: string): void {
  const el = $("conn");
  el.textContent = text;
  el.dataset.state = kind;
  state.connected = kind === "ok";
  $("chat-form").setAttribute("aria-disabled", String(!state.connected));
  $<HTMLButtonElement>("chat-send").disabled = !state.connected;
}

function onFrame(text: string): void {
  let frame: { seq?: unknown; kind?: unknown; payload?: unknown } | null;
  try { frame = JSON.parse(text); } catch { return; }
  if (!frame || typeof frame !== "object") { return; }
  if (typeof frame.seq === "number") {
    // 번호 있는 이벤트: 이미 받은 번호는 버리고 마지막 번호를 기억한다.
    if (state.lastSeq !== null && frame.seq <= state.lastSeq) { return; }
    state.lastSeq = frame.seq;
    if (frame.kind === "office.agent") { applyAgent(frame.payload as AgentEvent, false); }
    else if (frame.kind === "office.chat") { applyChat(frame.payload as ChatResponse); }
    else if (frame.kind === KIND_OFFICE_APPROVAL) { applyApproval($("approvals"), frame.payload, (input, init) => fetch(input, init)); }
  }
}

async function loadSnapshot(): Promise<void> {
  try {
    const reply = await fetch(SNAPSHOT_PATH, { credentials: "same-origin", cache: "no-store" });
    if (!reply.ok) { return; }
    const snap = (await reply.json()) as { agents?: AgentEvent[]; pending?: ChatResponse[]; last_seq?: unknown };
    for (const view of snap.agents ?? []) { applyAgent(view, true); }
    for (const resp of snap.pending ?? []) { applyChat(resp); }
    if (typeof snap.last_seq === "number") { state.lastSeq = snap.last_seq; }
  } catch {
    // 스냅샷이 없으면 이벤트만으로 그린다.
  }
}

function retryLater(): void {
  const wait = retryMs;
  retryMs = Math.min(retryMs * 2, RETRY_MAX_MS);
  setTimeout(() => { void start(); }, wait);
}

function openSocket(): void {
  const scheme = location.protocol.replace("http", "ws");
  let url = `${scheme}//${location.host}${WS_PATH}`;
  // 끊겼다가 다시 이을 때는 마지막으로 받은 번호 뒤부터 받는다(E3).
  if (state.lastSeq !== null) { url += `?last_seq=${encodeURIComponent(String(state.lastSeq))}`; }
  let socket: WebSocket;
  try { socket = new WebSocket(url); } catch { retryLater(); return; }
  let closed = false;
  const onClose = () => {
    if (closed) { return; }
    closed = true;
    setConn("연결 끊김 (마지막으로 받은 상태 유지) · 다시 잇는 중", "bad");
    retryLater();
  };
  socket.onopen = () => {
    retryMs = RETRY_MIN_MS;
    setConn("연결됨", "ok");
  };
  socket.onmessage = (event) => { if (typeof event.data === "string") { onFrame(event.data); } };
  socket.onclose = onClose;
  socket.onerror = () => { try { socket.close(); } catch { /* 이미 닫힘 */ } onClose(); };
}

async function start(): Promise<void> {
  try {
    const reply = await fetch(SESSION_PATH, { method: "POST", credentials: "same-origin", cache: "no-store" });
    if (!reply.ok) { throw new Error("session"); }
    // 처음 이을 때만 스냅샷을 받는다. 그 뒤에는 마지막 번호부터 이어 받는다.
    if (state.lastSeq === null) { await loadSnapshot(); }
    openSocket();
  } catch {
    setConn("연결 끊김 (마지막으로 받은 상태 유지) · 다시 잇는 중", "bad");
    retryLater();
  }
}

// ---------- 시작 ----------

async function main(): Promise<void> {
  try {
    const reply = await fetch(MAP_PATH, { cache: "no-store" });
    state.map = (await reply.json()) as OfficeMap;
  } catch {
    state.map = null;
  }
  for (const id of AGENT_IDS) { state.names[id] = state.map?.agents?.[id]?.name ?? id; }
  wireChat();
  renderTabs();
  renderLog(true);
  setConn("잇는 중", "wait");

  // 폰처럼 좁은 화면은 처음부터 가벼운 화면이다. 3D가 안 되면 가벼운 화면으로 떨어진다.
  const narrow = window.matchMedia("(max-width: 760px)").matches;
  const toggle = $("view-toggle");
  if (!state.map || narrow) {
    showLight(state.map ? "narrow" : "lost");
    toggle.textContent = "3D 사무실";
  } else {
    try {
      await show3d();
      toggle.hidden = false;
      toggle.textContent = "가벼운 화면";
    } catch {
      showLight("lost");
    }
  }
  toggle.addEventListener("click", async () => {
    if (state.view) {
      showLight("chosen");
      toggle.hidden = false;
      toggle.textContent = "3D 사무실";
    } else {
      try {
        await show3d();
        toggle.textContent = "가벼운 화면";
      } catch {
        showLight("lost");
      }
    }
  });
  void start();
}

void main();
