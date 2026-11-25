// 설정 화면 "알림" 묶음(server 40차 6번 · 결정 24 · V91 · V87)을 jsdom에 띄운다(가짜 fetch): 종류별 표 그리기(폰 ·
// 메신저 칸) · 잠긴 줄 표시 · 조용한 시간 칸 · 저장 요청 모양 · 저장 뒤 다시 그림 · 원격이면 보기만 · 오류 줄(400 ·
// 403) · 읽기 실패 · 설정 화면에 달림 · 경로 표 · 화면 코드에 브라우저 저장소 없음.
import { afterEach, describe, expect, it, vi } from "vitest";
import { SCREEN_PATHS } from "../../web/src/paths";
import { LOAD_FAILED } from "../../web/src/ui/call";
import { mountSetupExtras } from "../../web/src/setup/extras";
import {
  INVALID_QUIET_TEXT,
  LOCKED_OFF_TEXT,
  LOCKED_TEXT,
  REMOTE_NOTE,
  SAVED_TEXT,
} from "../../web/src/setup/notify-model";
import type { NotifySettings } from "../../web/src/setup/notify-model";
import { mountNotify } from "../../web/src/setup/notify-view";
import { mainOf, reach } from "./source-graph";
import { KINDS, notifySettings } from "./notify-fixture";

const VIEW_SOURCE = import.meta.glob<string>("../../web/src/setup/notify-*.ts", {
  query: "?raw",
  import: "default",
  eager: true,
});

interface Call {
  method: string;
  path: string;
  body: unknown;
}

function json(status: number, value: unknown): Response {
  return new Response(JSON.stringify(value), { status, headers: { "Content-Type": "application/json" } });
}

/** 가짜 서버: 설정을 쥐고, 바꾸기는 정해 둔 답(없으면 받은 값으로 바꾸고 읽기 모양으로 답한다)을 낸다. */
class FakeServer {
  calls: Call[] = [];
  failRead = false;
  saveReply: { status: number; body: unknown } | null = null;

  constructor(public settings: NotifySettings) {}

  readonly fetch = vi.fn((input: RequestInfo | URL, init?: RequestInit) => {
    const method = (init?.method ?? "GET").toUpperCase();
    const path = new URL(String(input), "http://local.test").pathname;
    const body = typeof init?.body === "string" ? (JSON.parse(init.body) as unknown) : null;
    this.calls.push({ method, path, body });
    if (method === "POST" && path === "/api/session") return Promise.resolve(json(200, {}));
    if (path !== "/api/notify/settings") return Promise.resolve(json(404, { error: "not_found" }));
    if (method === "GET") {
      return Promise.resolve(this.failRead ? json(500, { error: "internal" }) : json(200, this.settings));
    }
    if (method === "POST") {
      if (this.saveReply) return Promise.resolve(json(this.saveReply.status, this.saveReply.body));
      const sent = body as { kinds: { kind: string; push: boolean; messenger: boolean }[]; quiet: NotifySettings["quiet"] };
      this.settings = {
        ...this.settings,
        kinds: this.settings.kinds.map((row) => {
          const changed = sent.kinds.find((item) => item.kind === row.kind);
          return changed ? { ...row, push: changed.push, messenger: changed.messenger } : row;
        }),
        quiet: sent.quiet,
      };
      return Promise.resolve(json(200, this.settings));
    }
    return Promise.resolve(json(404, { error: "not_found" }));
  });

  sent(method: string): Call[] {
    return this.calls.filter((call) => call.method === method && call.path === "/api/notify/settings");
  }
}

let server: FakeServer;

async function settle(): Promise<void> {
  for (let i = 0; i < 20; i++) await new Promise((resolve) => setTimeout(resolve, 0));
}

/** 자리 하나에 알림 묶음을 띄운다. */
async function open(initial: NotifySettings, prepare?: (fake: FakeServer) => void): Promise<HTMLElement> {
  server = new FakeServer(initial);
  prepare?.(server);
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

function box(root: ParentNode, kind: string, channel: "push" | "messenger"): HTMLInputElement {
  return pick<HTMLInputElement>(root, `[data-role="notify"] tr[data-kind="${kind}"] input[name="${channel}"]`);
}

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  document.body.innerHTML = "";
});

describe("설정 화면 · 알림 묶음 그리기", () => {
  it("종류마다 한 줄 — 글 · 폰 칸 · 메신저 칸을 서버가 준 차례와 값으로 그린다", async () => {
    // 검증: V91 V87 C2
    const settings = notifySettings();
    settings.kinds[5] = { ...settings.kinds[5], push: false };
    settings.kinds[4] = { ...settings.kinds[4], messenger: false };
    const root = await open(settings);
    expect(server.sent("GET").length).toBe(1);
    const rows = [...root.querySelectorAll('[data-role="notify"] [data-role="notify-kinds"] tr[data-kind]')];
    expect(rows.map((row) => row.getAttribute("data-kind"))).toEqual(KINDS.map((row) => row.kind));
    for (const [i, row] of rows.entries()) expect(row.textContent).toContain(KINDS[i].label);
    for (const input of root.querySelectorAll<HTMLInputElement>('tr[data-kind] input[name="push"], tr[data-kind] input[name="messenger"]')) {
      expect(input.type).toBe("checkbox");
    }
    expect(box(root, "daily_summary", "push").checked).toBe(false);
    expect(box(root, "daily_summary", "messenger").checked).toBe(true);
    expect(box(root, "backup_failure", "messenger").checked).toBe(false);
    expect(box(root, "approval_request", "push").checked).toBe(true);
  });

  it("잠긴 줄은 표시가 있고 잠기지 않은 줄은 없다", async () => {
    // 검증: V91
    const root = await open(notifySettings());
    for (const { kind, locked } of KINDS) {
      const row = pick(root, `tr[data-kind="${kind}"]`);
      expect(row.getAttribute("data-locked") === "true", kind).toBe(locked);
      expect((row.textContent ?? "").includes(LOCKED_TEXT), kind).toBe(locked);
    }
  });

  it("조용한 시간 칸: 켜기 · 시작 · 끝을 서버 값으로 그린다", async () => {
    // 검증: V91
    const root = await open(notifySettings({ quiet: { enabled: true, start: "23:30", end: "06:15" } }));
    const quiet = pick(root, '[data-role="notify"] [data-role="notify-quiet"]');
    expect(pick<HTMLInputElement>(quiet, "#notify-quiet-on").checked).toBe(true);
    expect(pick<HTMLInputElement>(quiet, "#notify-quiet-start").value).toBe("23:30");
    expect(pick<HTMLInputElement>(quiet, "#notify-quiet-end").value).toBe("06:15");
  });
});

describe("설정 화면 · 알림 묶음 저장", () => {
  it("한 종류를 끄고 조용한 시간을 켜 저장하면 통째 몸통을 보내고 받은 값으로 다시 그린다", async () => {
    // 검증: V91 C15
    const root = await open(notifySettings());
    box(root, "daily_summary", "push").click();
    box(root, "backup_failure", "messenger").click();
    pick<HTMLInputElement>(root, "#notify-quiet-on").click();
    const start = pick<HTMLInputElement>(root, "#notify-quiet-start");
    start.value = "21:00";
    start.dispatchEvent(new Event("input", { bubbles: true }));
    start.dispatchEvent(new Event("change", { bubbles: true }));
    const end = pick<HTMLInputElement>(root, "#notify-quiet-end");
    end.value = "08:30";
    end.dispatchEvent(new Event("input", { bubbles: true }));
    end.dispatchEvent(new Event("change", { bubbles: true }));
    pick<HTMLButtonElement>(root, 'button[data-action="notify-save"]').click();
    await settle();

    const posts = server.sent("POST");
    expect(posts.length).toBe(1);
    expect(posts[0].body).toEqual({
      kinds: KINDS.map(({ kind }) => ({
        kind,
        push: kind !== "daily_summary",
        messenger: kind !== "backup_failure",
      })),
      quiet: { enabled: true, start: "21:00", end: "08:30" },
    });
    const message = pick(root, '[data-role="notify-message"]');
    expect(message.textContent).toContain(SAVED_TEXT);
    expect(box(root, "daily_summary", "push").checked).toBe(false);
    expect(pick<HTMLInputElement>(root, "#notify-quiet-start").value).toBe("21:00");
  });

  it("잠긴 종류를 둘 다 끄면 서버의 400을 오류 줄로 보이고 칸은 그대로 둔다", async () => {
    // 검증: V91 E4
    const root = await open(notifySettings(), (fake) => {
      fake.saveReply = { status: 400, body: { error: "locked_kind_off", kind: "approval_request" } };
    });
    box(root, "approval_request", "push").click();
    box(root, "approval_request", "messenger").click();
    pick<HTMLButtonElement>(root, 'button[data-action="notify-save"]').click();
    await settle();
    expect(server.sent("POST").length, "검사는 서버가 한다 — 화면은 보낸다").toBe(1);
    const alert = pick(root, '[data-role="notify-message"] [role="alert"]');
    expect(alert.textContent).toContain(LOCKED_OFF_TEXT);
    expect(alert.textContent).toContain("승인 요청");
    expect(box(root, "approval_request", "push").checked).toBe(false);
    expect(box(root, "approval_request", "messenger").checked).toBe(false);
  });

  it("틀린 시각(400)은 시각 안내를, 다른 기기(403)는 이 컴퓨터에서만을 오류 줄로 보인다", async () => {
    // 검증: V91 E4 C15
    const cases: [{ status: number; body: unknown }, string][] = [
      [{ status: 400, body: { error: "invalid_quiet_hours" } }, INVALID_QUIET_TEXT],
      [{ status: 403, body: { error: "local_only" } }, REMOTE_NOTE],
    ];
    for (const [reply, text] of cases) {
      const root = await open(notifySettings(), (fake) => {
        fake.saveReply = reply;
      });
      pick<HTMLButtonElement>(root, 'button[data-action="notify-save"]').click();
      await settle();
      expect(pick(root, '[data-role="notify-message"] [role="alert"]').textContent).toContain(text);
      vi.unstubAllGlobals();
    }
  });
});

describe("설정 화면 · 알림 묶음 원격 · 실패 · 달기", () => {
  it("다른 기기면 저장 단추가 없고 모든 칸이 잠기며 이 컴퓨터에서만 바꿀 수 있다고 보인다", async () => {
    // 검증: V91 V87 C15
    const root = await open(notifySettings({ editable: false }));
    const block = pick(root, '[data-role="notify"]');
    expect(block.querySelector('button[data-action="notify-save"]')).toBeNull();
    const inputs = [...block.querySelectorAll<HTMLInputElement>("input")];
    expect(inputs.length).toBeGreaterThan(0);
    expect(inputs.every((input) => input.disabled), "보기만").toBe(true);
    expect(pick(block, '[data-role="notify-remote"]').textContent).toContain(REMOTE_NOTE);
    expect(server.sent("POST").length).toBe(0);
  });

  it("읽기가 실패하면 이 묶음에 오류 줄만 보이고 던지지 않는다", async () => {
    // 검증: V91 E4
    const root = await open(notifySettings(), (fake) => {
      fake.failRead = true;
    });
    const alert = pick(root, '[data-role="notify"] [role="alert"]');
    expect(alert.textContent).toContain(LOAD_FAILED);
    expect(root.querySelector('button[data-action="notify-save"]')).toBeNull();
  });

  it("설정 화면 보강 묶음에 알림 묶음이 달리고 설정을 읽는다", async () => {
    // 검증: V91 V87
    server = new FakeServer(notifySettings());
    vi.stubGlobal("fetch", server.fetch);
    document.body.innerHTML = '<main><div id="extras"></div></main>';
    const root = document.getElementById("extras") as HTMLElement;
    await mountSetupExtras(root);
    await settle();
    expect(server.sent("GET").length).toBeGreaterThanOrEqual(1);
    expect(root.querySelectorAll('[data-role="notify"] tr[data-kind]').length).toBe(KINDS.length);
  });

  it("설정 화면 소스는 알림 묶음에 닿고, 알림 묶음 코드는 브라우저 저장소 · 쿠키 · HTML 넣기를 쓰지 않는다", () => {
    // 검증: V46 N8 D11
    const { files } = reach(mainOf("setup"));
    expect(files.has("../../web/src/setup/notify-view.ts"), "알림 묶음을 단다").toBe(true);
    expect(files.has("../../web/src/setup/notify-model.ts")).toBe(true);
    expect(Object.keys(VIEW_SOURCE).length).toBe(2);
    for (const [name, text] of Object.entries(VIEW_SOURCE)) {
      for (const banned of ["localStorage", "sessionStorage", "indexedDB", "document.cookie", "innerHTML", "insertAdjacentHTML"]) {
        expect(text.includes(banned), `${name}: ${banned}`).toBe(false);
      }
    }
  });

  it("설정 화면의 경로 표에 알림 설정 경로가 있다", () => {
    // 검증: D11 C2
    expect(SCREEN_PATHS.setup).toContain("/api/notify/settings");
  });
});
