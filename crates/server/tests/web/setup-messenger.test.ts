// 설정 화면 메신저 묶음(server 39차 7번 · 결정 24 · V87 · V88)을 jsdom에 띄운다(가짜 fetch): 상태별 그림(꺼짐 · 받는 중 ·
// 토큰 거절 · 오류 · 연결됨), 토큰 칸 규칙(가린 칸 · 새 비밀번호 자동 완성), 등록 · 지우기 · 코드 승인 · 빼기 요청 모양,
// 오류 줄(400 · 403 · 409 · 423), 원격이면 보기만, 읽기 실패, 경로 표, 설정 화면에 달림.
import { afterEach, describe, expect, it, vi } from "vitest";
import { SCREEN_PATHS } from "../../web/src/paths";
import { LOAD_FAILED } from "../../web/src/ui/call";
import { mountSetupExtras } from "../../web/src/setup/extras";
import {
  DEMO_REFUSED,
  LOCKED_TEXT,
  PAIRED_TEXT,
  PAIRING_HINT,
  REMOTE_NOTE,
  STATE_LABELS,
  WRONG_CODE,
} from "../../web/src/setup/messenger-model";
import type { MessengerState, MessengerStatus } from "../../web/src/setup/messenger-model";
import { mountMessenger } from "../../web/src/setup/messenger-view";
import { mainOf, reach } from "./source-graph";

const TOKEN = "SIHEOM-WEB-TOKEN-0039";
const BOT = "siheom_web_bot";

interface Call {
  method: string;
  url: string;
  path: string;
  body: unknown;
}

function json(status: number, value: unknown): Response {
  return new Response(JSON.stringify(value), { status, headers: { "Content-Type": "application/json" } });
}

function status(state: MessengerState, configured: boolean, paired: boolean, editable = true): MessengerStatus {
  return {
    editable,
    telegram: { configured, bot: configured ? BOT : null, state, paired, pending: 0, locked_until: null },
  };
}

/** 가짜 서버: 메신저 상태를 쥐고, 바꾸기 요청은 정해 둔 답(없으면 성공하고 상태를 바꾼다)을 낸다. */
class FakeServer {
  calls: Call[] = [];
  /** 이 경로들은 500. */
  fail = new Set<string>();
  /** `METHOD 경로` → 정해 둔 답. */
  replies = new Map<string, () => Response>();

  constructor(public state: MessengerStatus) {}

  readonly fetch = vi.fn((input: RequestInfo | URL, init?: RequestInit) => {
    const url = String(input);
    const method = (init?.method ?? "GET").toUpperCase();
    const path = new URL(url, "http://local.test").pathname;
    const body = typeof init?.body === "string" ? JSON.parse(init.body) : null;
    this.calls.push({ method, url, path, body });
    if (this.fail.has(path)) return Promise.resolve(json(500, { error: "internal" }));
    const fixed = this.replies.get(`${method} ${path}`);
    if (fixed) return Promise.resolve(fixed());
    const telegram = this.state.telegram;
    if (method === "GET" && path === "/api/messenger") return Promise.resolve(json(200, this.state));
    if (method === "POST" && path === "/api/messenger/telegram") {
      this.state = { ...this.state, telegram: { ...telegram, configured: true, bot: BOT, state: "polling" } };
      return Promise.resolve(json(200, { registered: true, bot: BOT }));
    }
    if (method === "DELETE" && path === "/api/messenger/telegram") {
      this.state = { ...this.state, telegram: { ...telegram, configured: false, bot: null, state: "off", paired: false } };
      return Promise.resolve(json(200, { removed: true }));
    }
    if (method === "POST" && path === "/api/messenger/pairing/approve") {
      this.state = { ...this.state, telegram: { ...telegram, paired: true } };
      return Promise.resolve(json(200, { paired: true }));
    }
    if (method === "POST" && path === "/api/messenger/pairing/revoke") {
      this.state = { ...this.state, telegram: { ...telegram, paired: false } };
      return Promise.resolve(json(200, { revoked: true }));
    }
    if (method === "POST" && path === "/api/session") return Promise.resolve(json(200, {}));
    return Promise.resolve(json(404, { error: "not_found" }));
  });

  sent(method: string, path: string): Call[] {
    return this.calls.filter((call) => call.method === method && call.path === path);
  }
}

let server: FakeServer;

async function settle(): Promise<void> {
  for (let i = 0; i < 20; i++) await new Promise((resolve) => setTimeout(resolve, 0));
}

/** 자리 하나에 메신저 묶음을 띄운다. */
async function open(initial: MessengerStatus, prepare?: (fake: FakeServer) => void): Promise<HTMLElement> {
  server = new FakeServer(initial);
  prepare?.(server);
  vi.stubGlobal("fetch", server.fetch);
  document.body.innerHTML = '<main><div id="messenger"></div></main>';
  const root = document.getElementById("messenger") as HTMLElement;
  await mountMessenger(root);
  await settle();
  return root;
}

function pick<T extends Element = HTMLElement>(root: ParentNode, selector: string): T {
  const found = root.querySelector<T>(selector);
  expect(found, selector).not.toBeNull();
  return found as T;
}

function text(root: ParentNode, selector: string): string {
  return pick(root, selector).textContent ?? "";
}

function has(root: ParentNode, selector: string): boolean {
  return root.querySelector(selector) !== null;
}

/** 칸에 글을 넣는다(입력 이벤트까지). */
function type(root: ParentNode, selector: string, value: string): void {
  const input = pick<HTMLInputElement>(root, selector);
  input.value = value;
  input.dispatchEvent(new Event("input", { bubbles: true }));
}

async function press(root: ParentNode, action: string): Promise<void> {
  pick<HTMLButtonElement>(root, `[data-role="messenger"] button[data-action="${action}"]`).click();
  await settle();
}

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  document.body.innerHTML = "";
});

describe("메신저 묶음 · 상태별 그림", () => {
  it("토큰이 없으면 꺼짐 · 가린 토큰 칸 · 등록 단추만 있고 슬랙은 준비 중이다", async () => {
    // 검증: V87 N8
    const root = await open(status("off", false, false));
    const box = pick(root, '[data-role="messenger"]');
    expect(text(box, '[data-role="messenger-state"]')).toContain(STATE_LABELS.off);
    expect(pick(box, '[data-role="messenger-state"]').dataset.state).toBe("off");
    const token = pick<HTMLInputElement>(box, "input#messenger-token");
    expect(token.type).toBe("password");
    expect(token.getAttribute("autocomplete")).toBe("new-password");
    expect(has(box, 'button[data-action="messenger-register"]')).toBe(true);
    expect(has(box, 'button[data-action="messenger-remove"]'), "지울 토큰이 없다").toBe(false);
    expect(has(box, '[data-role="messenger-pairing"]')).toBe(false);
    expect(has(box, '[data-role="messenger-paired"]')).toBe(false);
    expect(has(box, '[data-role="messenger-bot"]')).toBe(false);
    // server 41차 고침(대조 표 "고침", 지시서 41차 8번 · 결정 26): "슬랙은 준비 중" 줄이 없어진다.
    expect(has(box, '[data-role="messenger-slack"]'), "슬랙 줄이 없다").toBe(false);
    expect(box.textContent ?? "").not.toContain("슬랙");
  });

  it("받는 중이고 연결된 사람이 없으면 봇 이름 · 지우기 · 페어링 안내와 코드 칸이 있다", async () => {
    // 검증: V88
    const root = await open(status("polling", true, false));
    const box = pick(root, '[data-role="messenger"]');
    expect(text(box, '[data-role="messenger-state"]')).toContain(STATE_LABELS.polling);
    expect(text(box, '[data-role="messenger-bot"]')).toContain(`@${BOT}`);
    expect(has(box, 'button[data-action="messenger-remove"]')).toBe(true);
    const pairing = pick(box, '[data-role="messenger-pairing"]');
    expect(pairing.textContent).toContain(PAIRING_HINT);
    expect(has(pairing, "input#messenger-code")).toBe(true);
    expect(has(pairing, 'button[data-action="messenger-approve"]')).toBe(true);
    expect(has(box, '[data-role="messenger-paired"]')).toBe(false);
  });

  it("연결된 사람이 있으면 연결됨과 빼기 단추가 있고 페어링 칸은 없다", async () => {
    // 검증: V88
    const root = await open(status("polling", true, true));
    const paired = pick(root, '[data-role="messenger"] [data-role="messenger-paired"]');
    expect(paired.textContent).toContain(PAIRED_TEXT);
    expect(has(paired, 'button[data-action="messenger-revoke"]')).toBe(true);
    expect(has(root, '[data-role="messenger-pairing"]')).toBe(false);
  });

  it("토큰 거절 · 오류 상태를 그 글로 보인다", async () => {
    // 검증: V87
    for (const state of ["unauthorized", "error"] as const) {
      const root = await open(status(state, true, false));
      expect(text(root, '[data-role="messenger-state"]')).toContain(STATE_LABELS[state]);
      expect(pick(root, '[data-role="messenger-state"]').dataset.state).toBe(state);
    }
  });
});

describe("메신저 묶음 · 바꾸기", () => {
  it("등록은 토큰을 POST하고 다시 읽어 받는 중으로 그리며 토큰 칸을 비운다", async () => {
    // 검증: V87 N8
    const root = await open(status("off", false, false));
    type(root, "input#messenger-token", ` ${TOKEN} `);
    await press(root, "messenger-register");
    expect(server.sent("POST", "/api/messenger/telegram").map((call) => call.body)).toEqual([{ token: TOKEN }]);
    expect(server.sent("GET", "/api/messenger").length, "다시 읽는다").toBeGreaterThanOrEqual(2);
    expect(text(root, '[data-role="messenger-state"]')).toContain(STATE_LABELS.polling);
    expect(pick<HTMLInputElement>(root, "input#messenger-token").value, "토큰 칸을 비운다").toBe("");
    expect(root.textContent ?? "").not.toContain(TOKEN);
  });

  it("지우기는 DELETE를 보내고 다시 읽어 꺼짐으로 그린다", async () => {
    // 검증: V88
    const root = await open(status("polling", true, true));
    await press(root, "messenger-remove");
    expect(server.sent("DELETE", "/api/messenger/telegram").length).toBe(1);
    expect(text(root, '[data-role="messenger-state"]')).toContain(STATE_LABELS.off);
    expect(has(root, '[data-role="messenger-paired"]')).toBe(false);
  });

  it("코드 승인은 다듬은 코드를 POST하고 다시 읽어 연결됨을 그린다", async () => {
    // 검증: V88
    const root = await open(status("polling", true, false));
    type(root, "input#messenger-code", " abcd2345 ");
    await press(root, "messenger-approve");
    expect(server.sent("POST", "/api/messenger/pairing/approve").map((call) => call.body)).toEqual([{ code: "ABCD2345" }]);
    expect(text(root, '[data-role="messenger-paired"]')).toContain(PAIRED_TEXT);
  });

  it("빼기는 몸통 없이 POST하고 다시 읽어 페어링 안내로 돌아간다", async () => {
    // 검증: V88
    const root = await open(status("polling", true, true));
    await press(root, "messenger-revoke");
    expect(server.sent("POST", "/api/messenger/pairing/revoke").map((call) => call.body)).toEqual([null]);
    expect(text(root, '[data-role="messenger-pairing"]')).toContain(PAIRING_HINT);
  });
});

describe("메신저 묶음 · 오류 줄", () => {
  it("틀린 코드(400)면 오류 줄에 맞지 않는다고 보이고 그림은 그대로다", async () => {
    // 검증: V88
    const root = await open(status("polling", true, false), (fake) =>
      fake.replies.set("POST /api/messenger/pairing/approve", () => json(400, { error: "wrong_code", message: WRONG_CODE })),
    );
    type(root, "input#messenger-code", "ZZZZZZZZ");
    await press(root, "messenger-approve");
    expect(text(root, '[data-role="messenger"] [role="alert"]')).toContain(WRONG_CODE);
    expect(has(root, '[data-role="messenger-pairing"]')).toBe(true);
  });

  it("잠김(423)이면 잠김 글을 보인다", async () => {
    // 검증: V88
    const until = new Date(2026, 10, 16, 11, 5).getTime();
    const root = await open(status("polling", true, false), (fake) =>
      fake.replies.set("POST /api/messenger/pairing/approve", () =>
        json(423, { error: "locked", message: LOCKED_TEXT, locked_until: until }),
      ),
    );
    type(root, "input#messenger-code", "ABCD2345");
    await press(root, "messenger-approve");
    const alert = text(root, '[data-role="messenger"] [role="alert"]');
    expect(alert).toContain(LOCKED_TEXT);
    expect(alert).toContain("11:05");
  });

  it("다른 기기로 거절(403)되면 이 컴퓨터에서만 바꿀 수 있다고 보인다", async () => {
    // 검증: D2 V46
    const root = await open(status("off", false, false), (fake) =>
      fake.replies.set("POST /api/messenger/telegram", () => json(403, { error: "forbidden" })),
    );
    type(root, "input#messenger-token", TOKEN);
    await press(root, "messenger-register");
    expect(text(root, '[data-role="messenger"] [role="alert"]')).toContain(REMOTE_NOTE);
    expect(pick<HTMLInputElement>(root, "input#messenger-token").value, "실패해도 토큰 칸을 비운다").toBe("");
  });

  it("시연 모드(409)면 시연 안내를 보인다", async () => {
    // 검증: V87 E56
    const root = await open(status("off", false, false), (fake) =>
      fake.replies.set("POST /api/messenger/telegram", () => json(409, { error: "demo_mode", message: DEMO_REFUSED })),
    );
    type(root, "input#messenger-token", TOKEN);
    await press(root, "messenger-register");
    expect(text(root, '[data-role="messenger"] [role="alert"]')).toContain(DEMO_REFUSED);
  });

  it("상태를 읽지 못하면 이 묶음에만 오류 줄을 보이고 던지지 않는다", async () => {
    // 검증: E57
    const root = await open(status("off", false, false), (fake) => fake.fail.add("/api/messenger"));
    expect(text(root, '[data-role="messenger"] [role="alert"]')).toContain(LOAD_FAILED);
    expect(has(root, "input#messenger-token")).toBe(false);
  });
});

describe("메신저 묶음 · 원격 · 경로 · 설정 화면", () => {
  it("다른 기기면 입력 · 단추 없이 보기만 하고 안내를 보인다", async () => {
    // 검증: D2 V46 V88
    const root = await open(status("polling", true, true, false));
    const box = pick(root, '[data-role="messenger"]');
    expect(box.querySelectorAll("input, button, select, textarea").length, "보기만").toBe(0);
    expect(text(box, '[data-role="messenger-remote"]')).toContain(REMOTE_NOTE);
    expect(text(box, '[data-role="messenger-state"]')).toContain(STATE_LABELS.polling);
    expect(box.textContent ?? "").toContain(PAIRED_TEXT);
  });

  it("부르는 경로는 모두 설정 화면의 경로 표에 있고 바깥 주소가 없다", async () => {
    // 검증: D11 D2 C2
    const root = await open(status("off", false, false));
    type(root, "input#messenger-token", TOKEN);
    await press(root, "messenger-register");
    type(root, "input#messenger-code", "ABCD2345");
    await press(root, "messenger-approve");
    await press(root, "messenger-revoke");
    await press(root, "messenger-remove");
    const seen = new Set(server.calls.map((call) => call.path));
    for (const path of seen) expect(SCREEN_PATHS.setup, path).toContain(path);
    for (const path of ["/api/messenger", "/api/messenger/telegram", "/api/messenger/pairing/approve", "/api/messenger/pairing/revoke"]) {
      expect(seen.has(path), `${path}을 부른다`).toBe(true);
    }
    expect(server.calls.every((call) => !call.url.includes("://"))).toBe(true);
  });

  it("설정 보강 묶음이 메신저 묶음을 함께 단다", async () => {
    // 검증: V88 C32
    server = new FakeServer(status("polling", true, false));
    vi.stubGlobal("fetch", server.fetch);
    document.body.innerHTML = '<main><div id="extras"></div></main>';
    const root = document.getElementById("extras") as HTMLElement;
    await mountSetupExtras(root);
    await settle();
    expect(text(root, '[data-role="messenger"] [data-role="messenger-state"]')).toContain(STATE_LABELS.polling);
    expect(server.sent("GET", "/api/messenger").length).toBeGreaterThanOrEqual(1);
  });

  it("설정 화면 소스는 메신저 묶음에 닿고 three · 사무실 3D에는 닿지 않는다", () => {
    // 검증: V77 N21
    const { files, bare } = reach(mainOf("setup"));
    expect(files.has("../../web/src/setup/messenger-view.ts"), "메신저 묶음을 단다").toBe(true);
    expect(files.has("../../web/src/setup/messenger-model.ts")).toBe(true);
    expect([...bare].filter((name) => name === "three" || name.startsWith("three/"))).toEqual([]);
    expect([...files].filter((file) => file.includes("scene3d") || file.includes("/src/office/"))).toEqual([]);
  });
});
