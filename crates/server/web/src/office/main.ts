// 사무실 화면의 입구: 연결 · 상태 · 채팅 칸 · 화면 고르기(3D 또는 가벼운 화면).
//
// 서버와 주고받는 것(모두 같은 집의 상대 경로):
// - POST /api/session                    세션을 얻는다(관문).
// - GET  /api/office                     접속 스냅샷: {agents:[{agent,place,work}], pending:[응답 객체], last_seq}
// - POST /api/office/chat                담당에게 말하기(C5): {room, agent, text, sourceMessageId} → 응답 객체
// - WS   /ws?last_seq=N                  번호 있는 이벤트 `office.agent` · `office.chat`를 받는다(C2 · E3).
// - WS   이벤트 `office.approval`        승인 카드(10c 14번): 폰과 같은 길 · 같은 지문 대조로 승인 · 거절.
// - GET  /api/office/looks · POST /api/office/look   담당 외형(20차 4번): 처음에 읽어 외형을 정하고, 고르면 바꾼다.
// - 스냅샷의 `talk` 칸 · WS 이벤트 `office.talk`   팀의 대화(22차): 팀 탭 · 머리 위 말풍선 · 가벼운 화면의 최근 말.
// - GET  /api/office/me · POST /api/office/me/position   내 캐릭터(32차): 저장된 자리에서 시작, 자리는 띄엄띄엄 남긴다.
// - (33차) 팀 방 멘션 글 `{room: "team", text, sourceMessageId}` · 공지 · 부르기는 team-compose.ts · summon-scene.ts.
// 스냅샷과 말하기 길은 조립(price-inha)이 Office에 잇는다. 없으면 이벤트만으로 그린다.
import { mountMenu } from "../ui/menu";
import "./office.css";
import { API, OFFICE_LOOK_PATHS } from "../paths";
import { KIND_OFFICE_APPROVAL, applyApproval } from "./approval";
import { chatLine } from "./chat-lines";
import { chatLineElement } from "./chat-view";
import { Chatter, KIND_OFFICE_TALK, readTalk } from "./chatter";
import { createLookPicker } from "./look-picker";
import { DEFAULT_LOOKS } from "./looks";
import { FollowState } from "./follow";
import { KIND_RULE_PROPOSAL, createOfficeMarks, type OfficeMarks } from "./marks-scene";
import { loadMe, type Me } from "./player-sync";
import { hintText } from "./mention-text";
import { createSummonScene, type SummonScene } from "./summon-scene";
import { summonButton } from "./summon-view";
import { interleave, mountTeamCompose, showMentions, type TeamCompose } from "./team-compose";
import { mountControlsHint, mountToolbar, type Toolbar } from "./toolbar";
import type { OfficeMap, Scene } from "./scene3d";
import { SpeakerCamera } from "./speaker-camera";
import { SPEECH_LIFE_MS } from "./speech";
import { teamFeed, type FeedEntry } from "./team-feed";
import { trimLines, type Line } from "./trim";
import { bindTopicClicks, mountBasisPanel } from "./basis-panel";

const SESSION_PATH = API.session;
const SNAPSHOT_PATH = API.office;
const CHAT_PATH = API.officeChat;
const WS_PATH = API.socket;
const MAP_PATH = "./office-map.json";
const RETRY_MIN_MS = 1000;
const RETRY_MAX_MS = 10000;
const FOLLOW_PX = 80;
// (23차, 22차 N2 걱정) 다시 이을 때 서버가 다시 보내는 옛 office.talk: 이은 뒤 이 시간 안에 온 것 가운데 `at`이
// 말풍선 수명(12초)보다 오래된 것은 말풍선 · 자막 · 발언자 카메라 없이 팀 탭에만 넣는다. 이 창 밖의 실시간 글은
// 브라우저와 서버의 시계가 어긋나도 말풍선이 사라지지 않게 `at`을 보지 않는다.
const REPLAY_WINDOW_MS = 3000;
// 자막 · 발언 중 표시를 다시 볼 간격.
const CAPTION_TICK_MS = 500;

const AGENT_IDS = ["news", "strategy", "risk", "review", "trader", "leader"];
// 20차 고침: 침실은 없다. 일하지 않는 시간(`bed`)은 라운지 · 탕비실 쉼 자리다.
const PLACE_LABEL: Record<string, string> = { bed: "라운지 쉼 자리", desk: "사무실 자기 자리", meeting: "미팅룸 탁자" };
const WORK_LABEL: Record<string, string> = { idle: "", queued: "대기", thinking: "생각 중", failed: "실패" };
const ROOM_TEAM = "team";

const $ = <T extends HTMLElement = HTMLElement>(id: string): T => document.getElementById(id) as T;

/** 담당 하나의 자리 · 일. */
interface AgentView { place: string; work: string }
/** 이벤트 `office.agent`의 몸통((33차) 스냅샷에는 부르기 단계도 실린다). */
interface AgentEvent extends AgentView { agent: string; phase?: unknown; reason?: unknown }
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
  looks: { ...DEFAULT_LOOKS } as Record<string, string>, // 담당 → 외형 id(서버가 정본)
  looksLoaded: false,
  me: null as Me | null,                      // (32차) 내 캐릭터(서버가 정본, 없으면 기본 · 입구)
  meLoaded: false,
};
/** (22차) 팀의 대화: 말풍선 판 · 최근 말 · 팀 탭의 논의 줄. */
const chatter = new Chatter();
/** (23차 7번 (나)) 회의 발언자 카메라(기본 켜짐). */
const speaker = new SpeakerCamera();
/** (32차 6번) 카메라 따라가기(기본 켜짐). 켜져 있으면 발언자 카메라가 끼어들지 않는다. */
const follow = new FollowState();
let toolbar: Toolbar | null = null;
/** (33차) 팀 방 글 칸(멘션 칩 · 공지)과 부르기. */
let compose: TeamCompose | null = null;
let summons: SummonScene | null = null;
/** (34차) 머리 위 ✋ · ✓ · 승인 알림음 · 범례. */
let marks: OfficeMarks | null = null;
/** 이 시각까지 받은 `office.talk`은 다시 이을 때 다시 받은 것일 수 있다. */
let replayUntil = 0;

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
    // (22차 5번) 가장 최근 말(12초) 한 줄.
    const said = chatter.latest(id);
    if (said) {
      const line = document.createElement("span");
      line.className = "said";
      line.textContent = said;
      button.append(line);
    }
    button.addEventListener("click", () => selectConversation(id));
    li.append(button, ...(marks?.badges(id, Date.now()) ?? []), summonButton(id, (agent) => void summons?.summon(agent)));
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
    looks: state.looks,
    onPick: (id: string) => selectConversation(id),
    onLost: () => showLight("lost"),
    follow,
    player: {
      stage: $("stage"),
      me: state.me,
      hooks: {
        openChat: (id: string) => selectConversation(id),
        closeChat: (id: string) => { if (state.conversation === id) { selectConversation(ROOM_TEAM); } },
        conversation: () => state.conversation,
        fetcher: (input, init) => fetch(input, init),
        onSpot: (spot) => { if (state.me) { state.me = { ...state.me, ...spot }; } },
      },
    },
    speech: chatter.board,
    speaker,
    marks: marks?.layer($("stage")),
  });
  state.view = scene;
  for (const [id, view] of state.agents) { scene.setAgent(id, view, { snap: true }); }
  summons?.redraw();
}

function applyAgent(view: AgentEvent | null | undefined, snap: boolean): void {
  if (!view || !state.agents.has(view.agent)) { return; }
  const next = { place: view.place, work: view.work };
  const before = state.agents.get(view.agent)?.work ?? "idle";
  state.agents.set(view.agent, next);
  if (!snap) { marks?.work(view.agent, before, next.work, Date.now()); }
  chatter.work(view.agent, next.work);
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
  chatter.chat(resp);
  if (!old) {
    chatter.stamp(resp.requestId);
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
  renderCaption();
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
    // (33차 7번) 담당 탭 옆 부르기 단추.
    if (key !== ROOM_TEAM) { tabs.append(summonButton(key, (agent) => void summons?.summon(agent))); }
  }
  $("chat-to-wrap").hidden = true;
  compose?.setTeam(state.conversation === ROOM_TEAM);
  renderCaption();
}

/** (23차 6번 · 7번 (가)) 화면 아래 자막과 팀 탭 위 `○○ 발언 중…`. 없으면 숨긴다. */
function renderCaption(): void {
  const line = chatter.caption(state.names);
  const cap = $("caption");
  if (cap.textContent !== (line ?? "")) { cap.textContent = line ?? ""; cap.title = line ?? ""; }
  cap.hidden = line === null;
  const speaking = state.conversation === ROOM_TEAM ? chatter.speaking(state.names) : null;
  const bar = $("speaking");
  if (bar.textContent !== (speaking ?? "")) { bar.textContent = speaking ?? ""; }
  bar.hidden = speaking === null;
}

function selectConversation(key: string): void {
  summons?.conversation(state.conversation, key);
  state.conversation = key;
  renderTabs();
  renderLog(true);
}

function lineElement(line: Line): HTMLElement | null {
  if (line.kind === "me") {
    // (23차 8번) 내 글은 오른쪽 녹색 말풍선. 팀 방에서는 받는 담당을 작게 덧붙인다.
    const view = chatLine({ kind: "me", key: line.id, text: showMentions(line.text) }, state.names);
    const div = chatLineElement(view, line.room === ROOM_TEAM && line.agent ? `→ ${nameOf(line.agent)}` : undefined);
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
  // (23차 8번) 담당 답은 왼쪽에 굵은 이름 + 글, 글이 아직 없으면 `○○: 생각 중` 줄.
  const view = chatLine(
    { kind: "reply", key: resp.requestId, agent: resp.agent, status: resp.status, content: resp.content }, state.names);
  if (resp.status === "failed") {
    return chatLineElement({ ...view, text: `실패 (${resp.error ?? "까닭 모름"})` }, undefined, "failed");
  }
  return chatLineElement(view);
}

/** (22차 2번) 팀 탭: 내 글 · 담당 답 · 논의 줄을 시각 순서로. */
function teamElements(lines: Line[]): (HTMLElement | null)[] {
  const byKey = new Map<string, Line>();
  const entries: FeedEntry[] = [];
  for (const line of lines) {
    const key = line.kind === "me" ? line.id : line.requestId;
    byKey.set(key, line);
    const resp = line.kind === "resp" ? state.responses.get(line.requestId) : undefined;
    entries.push(line.kind === "me"
      ? { kind: "me", key, at: chatter.stamp(key), agent: line.agent, text: line.text }
      : { kind: "reply", key, at: chatter.stamp(key), agent: resp?.agent ?? "", text: resp?.content ?? "" });
  }
  const talks = new Map<string, FeedEntry>();
  for (const talk of chatter.talkEntries()) { talks.set(talk.key, talk); entries.push(talk); }
  const rows = teamFeed(entries, state.names).map((row) => {
    const talk = talks.get(row.key);
    if (row.kind === "talk" && talk?.kind === "talk") {
      const view = chatLine({ kind: "talk", key: talk.key, agent: talk.agent, text: talk.text, topic: talk.topic }, state.names);
      return { at: row.at, el: chatLineElement(view, undefined, "talk") };
    }
    const line = byKey.get(row.key);
    return { at: row.at, el: line ? lineElement(line) : null };
  });
  // (33차 6번) 공지 띠를 보낸 시각 순서로 끼운다.
  return interleave<HTMLElement | null>(rows, compose?.bands() ?? []);
}

function renderLog(forceBottom: boolean): void {
  const log = $("chat-log");
  const follow = forceBottom || log.scrollHeight - log.scrollTop - log.clientHeight <= FOLLOW_PX;
  log.replaceChildren();
  const lines = linesOf(state.conversation);
  const elements = state.conversation === ROOM_TEAM ? teamElements(lines) : lines.map(lineElement);
  for (const el of elements) {
    if (el) { log.append(el); }
  }
  if (follow) { log.scrollTop = log.scrollHeight; }
}

async function send(room: string, agent: string, text: string, existing?: MyLine): Promise<void> {
  const line: MyLine = existing ?? { kind: "me", id: newId(), room, agent, text };
  chatter.stamp(line.id);
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
      // (33차 1번) 팀 방 멘션 글은 담당 없이 보낸다(서버가 멘션으로 답할 담당을 정한다).
      body: JSON.stringify(agent ? { room, agent, text, sourceMessageId: line.id } : { room, text, sourceMessageId: line.id }),
    });
    if (!reply.ok) { throw new Error(String(reply.status)); }
    const body = await reply.json();
    if (Array.isArray(body?.responses)) {
      for (const resp of body.responses) { applyChat(resp); }
      compose?.say(hintText(body.hint));
    } else {
      applyChat(body);
    }
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
  compose = mountTeamCompose({
    form: $("chat-form"),
    textarea: input,
    members: () => AGENT_IDS.map((agent) => ({ agent, name: nameOf(agent) })),
    names: () => state.names,
    agents: AGENT_IDS,
    connected: () => state.connected,
    sendTeam: (text) => void send(ROOM_TEAM, "", text),
    changed: () => { if (state.conversation === ROOM_TEAM) { renderLog(false); } },
    fetcher: (url, init) => fetch(url, { ...init, cache: "no-store" }),
  });
  const submit = () => {
    if (state.conversation === ROOM_TEAM) { compose?.submit(); return; }
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
    else if (frame.kind === KIND_OFFICE_TALK) { onTalk(frame.payload, frame.seq, !replayedOld(frame.payload)); }
    else if (frame.kind === KIND_OFFICE_APPROVAL) {
      // (35차 6번) 답이 서버에서 실패하면 기다리는 승인을 다시 읽어 ✋를 맞춘다(소리 없이).
      applyApproval($("approvals"), frame.payload, (input, init) => fetch(input, init), () => {
        void refreshApprovals(false);
      });
      marks?.onApproval(frame.payload);
    }
    else if (frame.kind === KIND_RULE_PROPOSAL) { void refreshApprovals(true); }
    else if (frame.kind === "office.notice") { compose?.onNotice(frame.payload); }
    else if (frame.kind === "office.notice_ack") { compose?.onAck(frame.payload); }
    else if (frame.kind === "office.summon") { summons?.onEvent(frame.payload); }
  }
}

/** 다시 이은 직후에 다시 받은 옛 논의 글인가(`at`이 말풍선 수명보다 오래됨). */
function replayedOld(payload: unknown): boolean {
  const now = Date.now();
  if (now >= replayUntil) { return false; }
  const talk = readTalk(payload);
  return talk !== null && now - talk.at >= SPEECH_LIFE_MS;
}

/** (22차) 팀 대화 한 줄: 팀 탭에 넣고, 실시간이면 말풍선 · 최근 말 · 자막 · 발언자 카메라에도. */
function onTalk(payload: unknown, seq: number, live: boolean): void {
  if (!chatter.talk(payload, seq, live)) { return; }
  const talk = live ? readTalk(payload) : null;
  if (talk) { speaker.talk(talk.agent, Date.now()); }
  renderCaption();
  if (state.conversation === ROOM_TEAM) { renderLog(false); }
  if (!$("light").hidden) { renderLight(); }
}

/** 서버의 담당 → 외형을 받아 둔다(목록 밖 값은 버린다). */
function takeLooks(body: unknown): Record<string, string> {
  const agents = (body as { agents?: Record<string, unknown> } | null)?.agents ?? {};
  for (const id of AGENT_IDS) {
    const look = agents[id];
    if (typeof look === "string") { state.looks[id] = look; }
  }
  return { ...state.looks };
}

function applyLooks(): void {
  if (state.view) { for (const id of AGENT_IDS) { state.view.setLook(id, state.looks[id]); } }
}

async function loadLooks(): Promise<void> {
  try {
    const reply = await fetch(OFFICE_LOOK_PATHS.list, { credentials: "same-origin", cache: "no-store" });
    if (!reply.ok) { return; }
    takeLooks(await reply.json());
    state.looksLoaded = true;
    applyLooks();
  } catch {
    // 못 읽으면 기본 외형으로 그린다.
  }
}

async function chooseLook(agent: string, look: string): Promise<Record<string, string>> {
  const reply = await fetch(OFFICE_LOOK_PATHS.choose, {
    method: "POST",
    credentials: "same-origin",
    cache: "no-store",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ agent, look }),
  });
  if (!reply.ok) { throw new Error(String(reply.status)); }
  return takeLooks(await reply.json());
}

async function loadSnapshot(): Promise<void> {
  try {
    const reply = await fetch(SNAPSHOT_PATH, { credentials: "same-origin", cache: "no-store" });
    if (!reply.ok) { return; }
    const snap = (await reply.json()) as {
      agents?: AgentEvent[]; pending?: ChatResponse[]; last_seq?: unknown; talk?: { seq?: unknown }[];
      pending_approvals?: unknown;
    };
    for (const view of snap.agents ?? []) { applyAgent(view, true); summons?.fromSnapshot(view.agent, view.phase, view.reason); }
    for (const resp of snap.pending ?? []) { applyChat(resp); }
    // (22차 1번) 최근 팀 대화(지난 것이라 말풍선은 띄우지 않는다).
    for (const talk of snap.talk ?? []) { if (typeof talk.seq === "number") { onTalk(talk, talk.seq, false); } }
    if (typeof snap.last_seq === "number") { state.lastSeq = snap.last_seq; }
    // (34차 1번) 기다리는 승인의 ✋를 채운다(소리 없이, E49).
    marks?.sync(snap.pending_approvals, false);
  } catch {
    // 스냅샷이 없으면 이벤트만으로 그린다.
  }
}

/** (34차 1번 · 5번) 기다리는 승인만 다시 읽어 맞춘다(다시 이을 때 · 개선안 요청이 생겼을 때). */
async function refreshApprovals(ringNew: boolean): Promise<void> {
  try {
    const reply = await fetch(SNAPSHOT_PATH, { credentials: "same-origin", cache: "no-store" });
    if (!reply.ok) { return; }
    const snap = (await reply.json()) as { pending_approvals?: unknown };
    marks?.sync(snap.pending_approvals, ringNew);
  } catch {
    // 못 읽으면 지금 표시를 그대로 둔다(다음 이음에 다시 맞춘다).
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
  const resuming = state.lastSeq !== null;
  if (resuming) { url += `?last_seq=${encodeURIComponent(String(state.lastSeq))}`; }
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
    if (resuming) { replayUntil = Date.now() + REPLAY_WINDOW_MS; void refreshApprovals(false); }
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
    if (!state.looksLoaded) { await loadLooks(); }
    if (!state.meLoaded) {
      state.me = await loadMe((input, init) => fetch(input, { ...init, cache: "no-store" }));
      state.meLoaded = state.me !== null;
    }
    if (state.lastSeq === null) { await loadSnapshot(); await compose?.load(); }
    openSocket();
  } catch {
    setConn("연결 끊김 (마지막으로 받은 상태 유지) · 다시 잇는 중", "bad");
    retryLater();
  }
}

/** (23차 7번 (나)) 단추 "자동 카메라 켜짐/꺼짐"(기본 켜짐). */
function wireAutoCamera(): void {
  const button = $<HTMLButtonElement>("auto-camera");
  const paint = () => {
    const on = speaker.enabled();
    button.setAttribute("aria-pressed", String(on));
    button.textContent = on ? "자동 카메라 켜짐" : "자동 카메라 꺼짐";
  };
  button.addEventListener("click", () => { speaker.setEnabled(!speaker.enabled(), Date.now()); paint(); });
  paint();
  button.hidden = false;
}

/** (32차 6번) 카메라 도구 줄 8개 · 오른쪽 아래 조작 안내. 따라가기 켜짐을 단추와 발언자 카메라에 알린다. */
function wireCameraTools(): void {
  const stage = $("stage");
  toolbar = mountToolbar(stage, (id) => state.view?.tool(id), follow.on());
  mountControlsHint(stage);
  follow.onChange((on) => {
    speaker.setFollowing(on, Date.now());
    toolbar?.setFollowing(on);
  });
}

// ---------- 시작 ----------

async function main(): Promise<void> {
  // 공통 메뉴(server 26차 3번): 위 줄(#bar) 맨 앞 자리에 단다. 3D 층 · 채팅은 그대로다.
  mountMenu(document.getElementById("menu"), "office");
  try {
    const reply = await fetch(MAP_PATH, { cache: "no-store" });
    state.map = (await reply.json()) as OfficeMap;
  } catch {
    state.map = null;
  }
  for (const id of AGENT_IDS) { state.names[id] = state.map?.agents?.[id]?.name ?? id; }
  if (state.map) {
    summons = createSummonScene({
      map: state.map,
      view: () => state.view,
      fetcher: (url, init) => fetch(url, { ...init, cache: "no-store" }),
      nameOf,
      openChat: (id) => selectConversation(id),
      say: (text) => compose?.say(text),
    });
  }
  const legend = document.createElement("div");
  legend.className = "legend-wrap";
  $("look-toggle").after(legend);
  marks = createOfficeMarks({
    approvals: $("approvals"),
    fetcher: (input, init) => fetch(input, init),
    names: state.names,
    legendRoot: legend,
    gestures: document,
  });
  marks.onChange(() => { if (!$("light").hidden) { renderLight(); } });
  wireChat();
  renderTabs();
  renderLog(true);
  setConn("잇는 중", "wait");
  const picker = createLookPicker({
    panel: $("looks"),
    agents: AGENT_IDS,
    nameOf,
    current: () => state.looks,
    choose: chooseLook,
    applied: () => applyLooks(),
  });
  $("look-toggle").addEventListener("click", () => {
    if (!$("looks").hidden) { picker.close(); return; }
    picker.open(state.conversation === ROOM_TEAM ? undefined : state.conversation);
  });
  // 처음 이음(세션 · 외형 · 스냅샷)을 먼저 해 3D가 고른 외형으로 바로 그려지게 한다. 실패하면 뒤에서 다시 잇는다.
  await start();

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
  wireCameraTools();
  wireAutoCamera();
  setInterval(renderCaption, CAPTION_TICK_MS);
  // (22차 5번) 가벼운 화면의 최근 말은 12초가 지나면 내린다.
  let lightSaid = "";
  setInterval(() => {
    if ($("light").hidden) { return; }
    const said = AGENT_IDS.map((id) => chatter.latest(id) ?? "").join("\u0001");
    if (said !== lightSaid) { lightSaid = said; renderLight(); }
  }, 1000);
  const onToggle = async () => {
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
  };
  toggle.addEventListener("click", () => { void onToggle(); });
}

void main();

// (server 50차 · F7-7 · C23) 근거 패널: 논의 줄의 종목 표시를 누르면 그 종목의 최근 매매의 근거 카드를 연다.
bindTopicClicks($("chat-log"), mountBasisPanel($("chat"), (input, init) => fetch(input, init)));
