// 설정 화면 "알림" 묶음의 "팀 논의" 줄(server 41차 2번 · 결정 26 · V91)을 jsdom에 띄운다(가짜 fetch): 서버가 맨 끝에 주는
// `team_talk`(폰 칸 없음 `push: null`)를 알림 종류 줄(`tr[data-kind]`)과 따로 `tr[data-role="notify-talk"]` 한 줄로
// 그림(폰 칸 자리 글 · 메신저 칸만) · 끄고 저장하면 받은 차례 그대로 `push: null`로 실음 · 다른 기기는 잠김 · 화면 모델이
// `push: null`을 읽음.
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  NO_PUSH_TEXT,
  SAVED_TEXT,
  TEAM_TALK_KIND,
  parseNotifySettings,
  saveBody,
} from "../../web/src/setup/notify-model";
import type { NotifySettings } from "../../web/src/setup/notify-model";
import { mountNotify } from "../../web/src/setup/notify-view";
import { KINDS, notifySettings } from "./notify-fixture";

/** 서버가 맨 끝에 주는 "팀 논의" 줄. */
const TALK_ROW = { kind: "team_talk", label: "팀 논의", push: null, messenger: true, locked: false };

/** 41차 서버의 읽기 몸통: 여섯 종류 다음에 팀 논의. */
function withTalk(overrides: Partial<NotifySettings> = {}): NotifySettings {
  const base = notifySettings(overrides);
  return { ...base, kinds: [...base.kinds, { ...TALK_ROW }] };
}

interface Call {
  method: string;
  path: string;
  body: unknown;
}

function json(status: number, value: unknown): Response {
  return new Response(JSON.stringify(value), { status, headers: { "Content-Type": "application/json" } });
}

/** 가짜 서버: 설정을 쥐고, 바꾸기는 받은 값으로 바꾼 뒤 읽기 모양으로 답한다. */
class FakeServer {
  calls: Call[] = [];

  constructor(public settings: NotifySettings) {}

  readonly fetch = vi.fn((input: RequestInfo | URL, init?: RequestInit) => {
    const method = (init?.method ?? "GET").toUpperCase();
    const path = new URL(String(input), "http://local.test").pathname;
    const body = typeof init?.body === "string" ? (JSON.parse(init.body) as unknown) : null;
    this.calls.push({ method, path, body });
    if (method === "POST" && path === "/api/session") return Promise.resolve(json(200, {}));
    if (path !== "/api/notify/settings") return Promise.resolve(json(404, { error: "not_found" }));
    if (method === "GET") return Promise.resolve(json(200, this.settings));
    if (method === "POST") {
      const sent = body as { kinds: { kind: string; messenger: boolean }[] };
      this.settings = {
        ...this.settings,
        kinds: this.settings.kinds.map((row) => {
          const changed = sent.kinds.find((item) => item.kind === row.kind);
          return changed ? { ...row, messenger: changed.messenger } : row;
        }),
      };
      return Promise.resolve(json(200, this.settings));
    }
    return Promise.resolve(json(404, { error: "not_found" }));
  });

  posts(): Call[] {
    return this.calls.filter((call) => call.method === "POST" && call.path === "/api/notify/settings");
  }
}

let server: FakeServer;

async function settle(): Promise<void> {
  for (let i = 0; i < 20; i++) await new Promise((resolve) => setTimeout(resolve, 0));
}

async function open(initial: NotifySettings): Promise<HTMLElement> {
  server = new FakeServer(initial);
  vi.stubGlobal("fetch", server.fetch);
  document.body.innerHTML = '<main><div id="notify"></div></main>';
  const root = document.getElementById("notify") as HTMLElement;
  await mountNotify(root);
  await settle();
  return root;
}

function pick<T extends Element = HTMLElement>(root: ParentNode, selector: string): T {
  const found = root.querySelector<T>(selector);
  expect(found, selector).not.toBeNull();
  return found as T;
}

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  document.body.innerHTML = "";
});

describe("설정 화면 · 알림 묶음 · 팀 논의 줄", () => {
  it("팀 논의는 알림 종류 줄과 따로 한 줄 — 글 · 폰 칸 없음 · 메신저 칸(켜짐)", async () => {
    // 검증: V91 V87 C2
    const root = await open(withTalk());
    const talk = pick(root, '[data-role="notify"] [data-role="notify-kinds"] tr[data-role="notify-talk"]');
    expect(talk.textContent).toContain("팀 논의");
    expect(talk.textContent).toContain(NO_PUSH_TEXT);
    expect(talk.querySelector('input[name="push"]'), "폰 칸이 없다").toBeNull();
    const messenger = pick<HTMLInputElement>(talk, 'input[name="messenger"]');
    expect(messenger.type).toBe("checkbox");
    expect(messenger.checked).toBe(true);
    expect(talk.getAttribute("data-locked")).not.toBe("true");
    const kinds = [...root.querySelectorAll('[data-role="notify"] tr[data-kind]')].map((row) => row.getAttribute("data-kind"));
    expect(kinds, "알림 종류 줄은 여섯 그대로").toEqual(KINDS.map((row) => row.kind));
  });

  it("팀 논의를 끄고 저장하면 받은 차례 그대로 push null · messenger 거짓으로 보내고 다시 그린다", async () => {
    // 검증: V91 C15
    const root = await open(withTalk());
    pick<HTMLInputElement>(root, 'tr[data-role="notify-talk"] input[name="messenger"]').click();
    pick<HTMLButtonElement>(root, 'button[data-action="notify-save"]').click();
    await settle();
    const posts = server.posts();
    expect(posts.length).toBe(1);
    const sent = (posts[0].body as { kinds: { kind: string; push: boolean | null; messenger: boolean }[] }).kinds;
    expect(sent.map((row) => row.kind)).toEqual([...KINDS.map((row) => row.kind), TEAM_TALK_KIND]);
    expect(sent[sent.length - 1]).toEqual({ kind: TEAM_TALK_KIND, push: null, messenger: false });
    expect(sent.slice(0, 6).every((row) => row.push === true && row.messenger === true)).toBe(true);
    expect(pick(root, '[data-role="notify-message"]').textContent).toContain(SAVED_TEXT);
    expect(pick<HTMLInputElement>(root, 'tr[data-role="notify-talk"] input[name="messenger"]').checked).toBe(false);
  });

  it("다른 기기에서는 팀 논의 칸도 잠긴다", async () => {
    // 검증: V91 E3
    const root = await open(withTalk({ editable: false }));
    expect(pick<HTMLInputElement>(root, 'tr[data-role="notify-talk"] input[name="messenger"]').disabled).toBe(true);
  });
});

describe("알림 묶음 화면 모델 · 팀 논의", () => {
  it("push가 null인 줄을 읽고 저장 몸통에 그대로 null로 싣는다", () => {
    // 검증: V91
    const parsed = parseNotifySettings(withTalk());
    expect(parsed).not.toBeNull();
    const last = parsed?.kinds[parsed.kinds.length - 1];
    expect(last).toEqual(TALK_ROW);
    const body = saveBody(parsed as NotifySettings) as { kinds: unknown[] };
    expect(body.kinds[body.kinds.length - 1]).toEqual({ kind: TEAM_TALK_KIND, push: null, messenger: true });
    expect(TEAM_TALK_KIND).toBe("team_talk");
  });

  it("push가 참/거짓/null이 아닌 줄은 여전히 틀린 모양이다", () => {
    // 검증: V91
    const broken = withTalk();
    (broken.kinds[6] as unknown as Record<string, unknown>).push = "no";
    expect(parseNotifySettings(broken)).toBeNull();
  });
});
