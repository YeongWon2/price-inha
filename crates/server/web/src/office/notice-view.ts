// 팀 방 전체 공지(server 33차 6번, C19 · F11 · E45 · V71 · V72 · V73). 원본에 없는 우리 기능(결정 17).
// - 팀 방 글 칸 옆 "공지" 단추: 누르면 공지 모드(표시 "공지로 보내기"가 보인다), 다시 누르면 보통 글.
//   공지 모드여도 글에 멘션이 있으면 공지가 아니라 멘션 글로 간다(F11 1a).
// - 공지는 채팅에 공지 띠(`div.notice-band`)로 보이고 그 아래 담당별 확인(✓ 답함 · 기다림 · 상한으로 답 못 함 · 실패)이 붙는다.
// - 이벤트: `office.notice` `{"notice", "text", "at"}`(새 공지, 여섯 모두 기다림) · `office.notice_ack`
//   `{"notice", "agent", "state"}`(담당마다 한 번만 바뀐다). 사무실에 들어오면 지난 공지(`GET ?limit=`)로 채운다.
// - 요소는 DOM 함수로만 만든다. 상태는 순수 함수로 다룬다(바꾼 새 목록을 돌려준다).

import { API } from "../paths";

/** 담당 한 명의 확인 상태. `queued`는 보낸 직후의 답(기다림과 같게 그린다). */
export type AckState = "waiting" | "queued" | "answered" | "limit" | "failed";

/** 담당 한 명의 확인. */
export interface NoticeAck {
  agent: string;
  state: AckState;
}

/** 공지 하나(번호 · 가린 글 · 보낸 시각 · 담당별 확인). */
export interface NoticeItem {
  notice: number;
  text: string;
  at: number;
  acks: NoticeAck[];
}

/** 같은 출처 `fetch`의 모양(시험이 가짜를 넘긴다). */
export type Fetcher = (url: string, init?: RequestInit) => Promise<Response>;

/** 공지 단추의 글. */
export const NOTICE_BUTTON = "공지";
/** 공지 모드일 때 글 칸 옆에 보이는 표시. */
export const NOTICE_MODE_LABEL = "공지로 보내기";
/** 공지 띠의 class. */
export const NOTICE_BAND_CLASS = "notice-band";
/** 사무실에 들어올 때 읽는 지난 공지 수. */
export const NOTICE_HISTORY_LIMIT = 20;

/** 확인 상태 → 보이는 글. `answered` "✓ 답함", `waiting` · `queued` "기다림", `limit` "상한으로 답 못 함", `failed` "실패". */
export function ackLabel(state: AckState): string {
  switch (state) {
    case "answered":
      return "✓ 답함";
    case "limit":
      return "상한으로 답 못 함";
    case "failed":
      return "실패";
    default:
      return "기다림";
  }
}

/** 보낼 길: 공지 모드이고 멘션이 없으면 `"notice"`, 그 밖은 `"team"`(멘션 글 — 멘션이 없으면 서버가 안내를 돌려준다). */
export function sendRoute(noticeMode: boolean, hasMention: boolean): "notice" | "team" {
  return noticeMode && !hasMention ? "notice" : "team";
}

const ACK_STATES: readonly AckState[] = ["waiting", "queued", "answered", "limit", "failed"];
/** 한 번 바뀌면 더 바뀌지 않는 확인 상태. */
const SETTLED: readonly AckState[] = ["answered", "limit", "failed"];

function record(value: unknown): Record<string, unknown> | null {
  return typeof value === "object" && value !== null && !Array.isArray(value) ? (value as Record<string, unknown>) : null;
}

function ackState(value: unknown): AckState | null {
  return typeof value === "string" && (ACK_STATES as readonly string[]).includes(value) ? (value as AckState) : null;
}

/** 공지 한 줄(목록 · 이벤트의 모양). 틀리면 `null`. */
function noticeFrom(value: unknown): NoticeItem | null {
  const row = record(value);
  if (!row || typeof row.notice !== "number" || typeof row.text !== "string" || typeof row.at !== "number") return null;
  const acks: NoticeAck[] = [];
  for (const raw of Array.isArray(row.acks) ? row.acks : []) {
    const ack = record(raw);
    const state = ackState(ack?.state);
    if (ack && typeof ack.agent === "string" && state) acks.push({ agent: ack.agent, state });
  }
  return { notice: row.notice, text: row.text, at: row.at, acks };
}

/**
 * `office.notice` 내용 → 새 공지(담당 `agents`가 모두 `waiting`). 모양이 틀리면 `null`.
 * 이미 같은 번호가 `items`에 있으면 `items`를 그대로, 없으면 끝에 더한 새 목록을 돌려준다.
 */
export function applyNoticeEvent(items: readonly NoticeItem[], payload: unknown, agents: readonly string[]): NoticeItem[] {
  const item = noticeFrom(payload);
  if (!item || items.some((i) => i.notice === item.notice)) return [...items];
  item.acks = agents.map((agent) => ({ agent, state: "waiting" }));
  return [...items, item];
}

/**
 * `office.notice_ack` 내용 → 그 공지의 그 담당 확인을 바꾼 새 목록. 확인은 한 번만 바뀐다: 이미 `answered` ·
 * `limit` · `failed`인 확인은 그대로다. 모르는 공지 · 담당 · 모양이 틀린 내용이면 `items` 그대로.
 */
export function applyAckEvent(items: readonly NoticeItem[], payload: unknown): NoticeItem[] {
  const row = record(payload);
  const state = ackState(row?.state);
  if (!row || !state || typeof row.notice !== "number" || typeof row.agent !== "string") return [...items];
  return items.map((item) => {
    if (item.notice !== row.notice) return item;
    const acks = item.acks.map((ack) =>
      ack.agent === row.agent && !SETTLED.includes(ack.state) ? { agent: ack.agent, state } : ack,
    );
    return { ...item, acks };
  });
}

/** `GET` [`API.officeNotices`]의 본문(최근 것 먼저) → 보낸 순서(오래된 것 먼저)의 공지들. 모양이 틀린 줄은 버린다. */
export function noticesFromList(body: unknown): NoticeItem[] {
  if (!Array.isArray(body)) return [];
  const items = body.map(noticeFrom).filter((item): item is NoticeItem => item !== null);
  return items.reverse();
}

/**
 * 공지 띠: `div.notice-band`(`data-notice`) 안에 공지 글(`.notice-text`)과 담당별 확인 줄(`.notice-acks` 안의
 * `span.ack`, `data-agent` · `data-state`, 글 `"{이름} {ackLabel}"`). 이름은 `names`에서, 없으면 id.
 */
export function noticeElement(item: NoticeItem, names: Readonly<Record<string, string>>): HTMLElement {
  const band = document.createElement("div");
  band.className = NOTICE_BAND_CLASS;
  band.dataset.notice = String(item.notice);
  const tag = document.createElement("span");
  tag.className = "notice-tag";
  tag.textContent = NOTICE_BUTTON;
  const text = document.createElement("div");
  text.className = "notice-text";
  text.textContent = item.text;
  const acks = document.createElement("div");
  acks.className = "notice-acks";
  for (const ack of item.acks) {
    const chip = document.createElement("span");
    chip.className = "ack";
    chip.dataset.agent = ack.agent;
    chip.dataset.state = ack.state;
    chip.textContent = `${names[ack.agent] ?? ack.agent} ${ackLabel(ack.state)}`;
    acks.append(chip);
  }
  band.append(tag, text, acks);
  return band;
}

/** 공지 단추와 모드 표시. */
export interface NoticeToggle {
  readonly button: HTMLButtonElement;
  /** 모드 표시(공지 모드가 아니면 `hidden`). */
  readonly marker: HTMLElement;
  isOn(): boolean;
  set(on: boolean): void;
}

/** `host`에 공지 단추(글 [`NOTICE_BUTTON`], `aria-pressed`)와 모드 표시([`NOTICE_MODE_LABEL`])를 단다. 누르면 모드를 뒤집고 `onChange`. */
export function mountNoticeToggle(host: HTMLElement, onChange: (on: boolean) => void): NoticeToggle {
  const button = document.createElement("button");
  button.type = "button";
  button.className = "notice-toggle";
  button.textContent = NOTICE_BUTTON;
  const marker = document.createElement("span");
  marker.className = "notice-mode";
  marker.textContent = NOTICE_MODE_LABEL;
  let on = false;
  const set = (next: boolean): void => {
    on = next;
    button.setAttribute("aria-pressed", on ? "true" : "false");
    button.classList.toggle("on", on);
    marker.hidden = !on;
  };
  set(false);
  button.addEventListener("click", () => {
    set(!on);
    onChange(on);
  });
  host.append(button, marker);
  return { button, marker, isOn: () => on, set };
}

/** 공지 보내기의 결과: 성공이면 번호와 담당별 첫 상태, 아니면 서버의 오류 이름(닿지 못하면 `"network"`). */
export type NoticeReply = { ok: true; notice: number; acks: NoticeAck[] } | { ok: false; error: string };

/** 공지를 보낸다: `POST` [`API.officeNotice`] `{"text", "sourceMessageId"}`(JSON, 같은 출처). 던지지 않는다. */
export async function postNotice(fetcher: Fetcher, text: string, sourceMessageId: string): Promise<NoticeReply> {
  try {
    const reply = await fetcher(API.officeNotice, {
      method: "POST",
      credentials: "same-origin",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ text, sourceMessageId }),
    });
    const body = record(await reply.json().catch(() => null));
    if (!reply.ok || !body || typeof body.notice !== "number") {
      return { ok: false, error: typeof body?.error === "string" ? body.error : `http_${reply.status}` };
    }
    const acks = noticeFrom({ notice: body.notice, text: "", at: 0, acks: body.acks })?.acks ?? [];
    return { ok: true, notice: body.notice, acks };
  } catch {
    return { ok: false, error: "network" };
  }
}

/** 지난 공지를 읽는다: `GET` [`API.officeNotices`]`?limit=<limit>` → [`noticesFromList`]. 실패하면 빈 목록(던지지 않는다). */
export async function loadNotices(fetcher: Fetcher, limit: number = NOTICE_HISTORY_LIMIT): Promise<NoticeItem[]> {
  try {
    const reply = await fetcher(`${API.officeNotices}?limit=${limit}`, { credentials: "same-origin" });
    if (!reply.ok) return [];
    return noticesFromList(await reply.json());
  } catch {
    return [];
  }
}
