// 팀 방 글 칸 붙이기(server 33차 5번 · 6번, C18 · C19 · E43 · E44 · F11): 사무실 채팅 칸에 멘션 칩 에디터 ·
// "공지" 단추 · 안내 줄을 달고, 공지 띠 목록(지난 공지 채우기 · `office.notice` · `office.notice_ack`)을 쥔다.
// - 팀 방이면 멘션 칩 에디터와 공지 단추가 보이고, 1:1 방이면 지금의 글 칸(textarea)이 그대로 보인다.
// - 보낼 길은 `sendRoute`: 공지 모드이고 멘션이 없으면 공지, 그 밖은 멘션 글(멘션 없으면 서버가 `no_mention` 안내).
// - 요소는 DOM 함수로만 만든다.

import { mountMentionEditor, type MentionEditor } from "./mention-editor";
import { hasMention, type Member } from "./mention-text";
import {
  applyAckEvent,
  applyNoticeEvent,
  loadNotices,
  mountNoticeToggle,
  noticeElement,
  postNotice,
  sendRoute,
  type Fetcher,
  type NoticeItem,
  type NoticeToggle,
} from "./notice-view";

/** 공지 보내기 실패 안내. */
const NOTICE_FAILED: Record<string, string> = {
  too_fast: "조금 뒤에 다시 보내세요",
  text_too_long: "공지가 너무 깁니다",
};

/** 붙일 때 받는 것. */
export interface TeamComposeOptions {
  /** 채팅 글 칸 묶음(`#chat-form`). 안내 줄 · 팀 방 글 칸을 여기에 단다. */
  form: HTMLElement;
  /** 1:1 방 글 칸(팀 방에서는 숨긴다). */
  textarea: HTMLElement;
  /** 담당 명단(멘션 목록). */
  members: () => readonly Member[];
  /** 담당 id → 이름. */
  names: () => Readonly<Record<string, string>>;
  /** 담당 id들(새 공지의 확인 줄 순서). */
  agents: readonly string[];
  /** 연결돼 있는가(끊기면 보내지 않는다). */
  connected: () => boolean;
  /** 멘션 글을 보낸다(팀 방 채팅 길). */
  sendTeam: (text: string) => void;
  /** 공지 목록이 바뀌었다(팀 방을 다시 그린다). */
  changed: () => void;
  fetcher: Fetcher;
}

/** 붙은 팀 방 글 칸. */
export interface TeamCompose {
  /** 팀 방인가에 따라 글 칸을 바꾼다. */
  setTeam(on: boolean): void;
  /** 글 칸 아래 안내(빈 값이면 숨긴다). */
  say(text: string | null): void;
  /** 보내기 단추 · Enter. */
  submit(): void;
  /** 들어올 때 지난 공지를 채운다. */
  load(): Promise<void>;
  /** `office.notice` 이벤트. */
  onNotice(payload: unknown): void;
  /** `office.notice_ack` 이벤트. */
  onAck(payload: unknown): void;
  /** 팀 방에 끼울 공지 띠들(보낸 시각과 요소). */
  bands(): { at: number; el: HTMLElement }[];
}

/** 보낼 글 `@[이름]`을 채팅에 보일 `@이름`으로(가림 `\`을 푼다). */
export function showMentions(text: string): string {
  return text.replace(/@\[((?:\\.|[^\]\\])*)\]/g, (_, name: string) => `@${name.replace(/\\(.)/g, "$1")}`);
}

/** 시각 순서로 줄들 사이에 공지 띠를 끼운다(같은 시각이면 줄이 먼저). */
export function interleave<T>(rows: readonly { at: number; el: T }[], extra: readonly { at: number; el: T }[]): T[] {
  const out: T[] = [];
  let i = 0;
  const sorted = [...extra].sort((a, b) => a.at - b.at);
  for (const row of rows) {
    while (i < sorted.length && sorted[i].at < row.at) { out.push(sorted[i].el); i += 1; }
    out.push(row.el);
  }
  for (; i < sorted.length; i += 1) { out.push(sorted[i].el); }
  return out;
}

export function mountTeamCompose(options: TeamComposeOptions): TeamCompose {
  const { form, textarea } = options;
  const doc = form.ownerDocument;
  const hint = doc.createElement("div");
  hint.className = "compose-hint";
  hint.setAttribute("role", "status");
  hint.setAttribute("aria-live", "polite");
  hint.hidden = true;
  const box = doc.createElement("div");
  box.className = "team-compose";
  const tools = doc.createElement("div");
  tools.className = "team-tools";
  form.prepend(hint);
  textarea.after(box);
  box.after(tools);

  let notices: NoticeItem[] = [];
  let sending = false;
  const say = (text: string | null): void => {
    hint.textContent = text ?? "";
    hint.hidden = !text;
  };

  const editor: MentionEditor = mountMentionEditor(box, {
    members: options.members,
    onSubmit: () => submit(),
  });
  editor.input.setAttribute("aria-label", "팀 방에 쓰기 (@로 담당 부르기)");
  editor.input.dataset.placeholder = "@로 담당을 부르세요 (Enter 보내기)";
  // 방향키 · 글자가 사무실 걷기로 새지 않게 한다(1:1 글 칸과 같다).
  editor.input.addEventListener("keydown", (e) => e.stopPropagation());
  editor.input.addEventListener("keyup", (e) => e.stopPropagation());
  editor.input.addEventListener("input", () => say(null));
  const toggle: NoticeToggle = mountNoticeToggle(tools, (on) => {
    editor.input.classList.toggle("notice-on", on);
    editor.input.focus();
  });

  async function sendNotice(text: string): Promise<void> {
    sending = true;
    const id = globalThis.crypto?.randomUUID?.() ?? `n-${Date.now().toString(36)}`;
    const reply = await postNotice(options.fetcher, text, id);
    sending = false;
    if (!reply.ok) {
      say(`공지를 보내지 못했습니다 — ${NOTICE_FAILED[reply.error] ?? reply.error}`);
      return;
    }
    editor.clear();
    toggle.set(false);
    editor.input.classList.remove("notice-on");
  }

  function submit(): void {
    const text = editor.text();
    if (!text.trim() || !options.connected() || sending) { return; }
    say(null);
    if (sendRoute(toggle.isOn(), hasMention(editor.pieces())) === "notice") {
      void sendNotice(text);
      return;
    }
    editor.clear();
    options.sendTeam(text);
  }

  return {
    setTeam(on: boolean): void {
      box.hidden = !on;
      tools.hidden = !on;
      textarea.hidden = on;
      if (!on) { say(null); }
    },
    say,
    submit,
    async load(): Promise<void> {
      const loaded = await loadNotices(options.fetcher);
      const known = new Set(notices.map((n) => n.notice));
      notices = [...loaded.filter((n) => !known.has(n.notice)), ...notices].sort((a, b) => a.at - b.at);
      options.changed();
    },
    onNotice(payload: unknown): void {
      notices = applyNoticeEvent(notices, payload, options.agents);
      options.changed();
    },
    onAck(payload: unknown): void {
      notices = applyAckEvent(notices, payload);
      options.changed();
    },
    bands: () => notices.map((item) => ({ at: item.at, el: noticeElement(item, options.names()) })),
  };
}
