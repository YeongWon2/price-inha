// 계좌 화면의 "전체 정지" 단추(server 40차 2번 · 결정 25 · F4 · R14-1 · V85)를 jsdom에 띄운다(가짜 fetch · 가짜
// WebSocket): 전체 정지가 아니면 단추가 있고 전체 정지면 단추 없이 "다시 시작"만, 확인 한 번(STOP_CONFIRM) 뒤에만
// `POST /api/team/stop`을 보냄, 결과 줄과 다시 읽기, 실패면 오류 줄(409 · 500 · 못 보냄), 화면 모델(canStop ·
// stopResult · stopText), 경로 표.
import { afterEach, describe, expect, it, vi } from "vitest";
import { SCREEN_PATHS } from "../../web/src/paths";
import type { AccountReply, TeamReply } from "../../web/src/account/model";
import {
  RESTART_CONFIRM,
  STOP_CONFIRM,
  STOP_LABEL,
  canRestart,
  canStop,
  stopResult,
  stopText,
  teamStateText,
} from "../../web/src/account/model";
import { mountAccount } from "../../web/src/account/screen";
import { renderStop } from "../../web/src/account/view";
import { account, team } from "./account-fixture";

interface Call {
  method: string;
  path: string;
  body: unknown;
}

/** 가짜 서버: 팀 상태 · 계좌를 들고 답한다. 정지는 정해 둔 답을 내고, 성공이면 팀 상태를 전체 정지로 바꾼다. */
class FakeServer {
  calls: Call[] = [];
  stopReply: { status: number; body: unknown } | "network" = { status: 200, body: { state: "full_stop" } };

  constructor(
    public team: TeamReply,
    public account: AccountReply,
  ) {}

  readonly fetch = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const method = (init?.method ?? "GET").toUpperCase();
    const path = new URL(String(input), "http://local.test").pathname;
    this.calls.push({ method, path, body: init?.body ?? null });
    const json = (status: number, value: unknown) =>
      new Response(JSON.stringify(value), { status, headers: { "Content-Type": "application/json" } });
    if (method === "POST" && path === "/api/session") return json(200, {});
    if (method === "POST" && path === "/api/team/stop") {
      const reply = this.stopReply;
      if (reply === "network") throw new TypeError("fetch failed");
      if (reply.status === 200) this.team = team("full_stop");
      return json(reply.status, reply.body);
    }
    if (method === "POST" && path === "/api/team/restart") return json(200, { state: "resting" });
    if (method !== "GET") return json(404, { error: "not_found" });
    if (path === "/api/team") return json(200, this.team);
    if (path === "/api/account") return json(200, this.account);
    return json(404, { error: "not_found" });
  });

  count(method: string, path: string): number {
    return this.calls.filter((call) => call.method === method && call.path === path).length;
  }
}

/** 가짜 실시간 연결(아무 이벤트도 흘리지 않는다). */
class FakeSocket extends EventTarget {
  readyState = 0;
  onopen: ((event: Event) => void) | null = null;
  onmessage: ((event: MessageEvent) => void) | null = null;
  onclose: ((event: Event) => void) | null = null;
  onerror: ((event: Event) => void) | null = null;

  constructor(public url: string) {
    super();
  }

  send(): void {}

  close(): void {
    this.readyState = 3;
  }
}

let server: FakeServer;

async function settle(): Promise<void> {
  for (let i = 0; i < 20; i++) await new Promise((resolve) => setTimeout(resolve, 0));
}

async function open(teamReply: TeamReply, prepare?: (fake: FakeServer) => void): Promise<HTMLElement> {
  server = new FakeServer(teamReply, account());
  prepare?.(server);
  vi.stubGlobal("fetch", server.fetch);
  vi.stubGlobal("WebSocket", FakeSocket);
  document.body.innerHTML = '<div id="menu"></div><main id="screen"><h1>계좌</h1></main>';
  const main = document.getElementById("screen") as HTMLElement;
  await mountAccount(main);
  await settle();
  return main;
}

function pick<T extends Element = HTMLElement>(root: ParentNode, selector: string): T {
  const found = root.querySelector<T>(selector);
  expect(found, selector).not.toBeNull();
  return found as T;
}

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  document.body.innerHTML = "";
});

describe("계좌 화면 · 전체 정지 단추", () => {
  it("전체 정지가 아니면 팀 상태 카드에 위험 단추 모양의 '전체 정지'가 있고, 전체 정지면 단추 없이 '다시 시작'만 있다", async () => {
    // 검증: V85 R14-1 F4
    for (const state of ["operating", "resting", "daily_limit_halt", "anomaly_halt", "full_stop"]) {
      const main = await open(team(state));
      const card = pick(main, '[data-role="team"]');
      const stops = card.querySelectorAll<HTMLButtonElement>('button[data-action="stop"]');
      const restarts = main.querySelectorAll('button[data-action="restart"]');
      if (state === "full_stop") {
        expect(stops.length, `${state}: 정지 단추 없음`).toBe(0);
        expect(restarts.length, `${state}: 다시 시작만`).toBe(1);
      } else {
        expect(stops.length, `${state}: 정지 단추 하나`).toBe(1);
        expect(stops[0].textContent).toContain(STOP_LABEL);
        expect(stops[0].dataset.kind, `${state}: 위험 단추 모양(공통 부품의 danger)`).toBe("danger");
        expect(restarts.length, `${state}: 다시 시작 없음`).toBe(0);
      }
      vi.unstubAllGlobals();
    }
  });

  it("확인을 거절하면 아무것도 보내지 않는다", async () => {
    // 검증: V85 R14-1
    const main = await open(team("operating", "trade"));
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(false);
    pick<HTMLButtonElement>(main, 'button[data-action="stop"]').click();
    await settle();
    expect(confirm).toHaveBeenCalledTimes(1);
    expect(confirm).toHaveBeenCalledWith(STOP_CONFIRM);
    expect(server.count("POST", "/api/team/stop")).toBe(0);
    expect(main.querySelector('button[data-action="restart"]')).toBeNull();
  });

  it("확인 한 번 뒤에 몸통 없이 정지를 보내고, 결과를 보이고 다시 읽어 '다시 시작'만 보인다", async () => {
    // 검증: V85 R14-1 F4 C2
    const main = await open(team("operating", "trade"));
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(true);
    const teamReads = server.count("GET", "/api/team");
    const accountReads = server.count("GET", "/api/account");
    pick<HTMLButtonElement>(main, 'button[data-action="stop"]').click();
    await settle();
    expect(confirm).toHaveBeenCalledTimes(1);
    expect(confirm).toHaveBeenCalledWith(STOP_CONFIRM);
    expect(confirm).not.toHaveBeenCalledWith(RESTART_CONFIRM);
    const posts = server.calls.filter((call) => call.method === "POST" && call.path === "/api/team/stop");
    expect(posts.length).toBe(1);
    expect(posts[0].body === null || posts[0].body === undefined || posts[0].body === "").toBe(true);
    const result = pick(main, '[data-role="stop-result"]');
    expect(result.getAttribute("role")).toBe("status");
    expect(result.textContent).toContain("전체 정지했습니다");
    expect(server.count("GET", "/api/team")).toBeGreaterThan(teamReads);
    expect(server.count("GET", "/api/account")).toBeGreaterThan(accountReads);
    expect(pick(main, '[data-role="team-state"]').textContent).toContain("전체 정지");
    expect(main.querySelector('button[data-action="stop"]')).toBeNull();
    expect(main.querySelectorAll('button[data-action="restart"]').length).toBe(1);
  });

  it("이미 전체 정지(409)면 오류 줄을 보이고 다시 읽는다", async () => {
    // 검증: V85 E4
    const main = await open(team("resting"), (fake) => {
      fake.stopReply = { status: 409, body: { error: "already_stopped" } };
    });
    vi.spyOn(window, "confirm").mockReturnValue(true);
    const teamReads = server.count("GET", "/api/team");
    pick<HTMLButtonElement>(main, 'button[data-action="stop"]').click();
    await settle();
    const error = pick(main, '[data-role="stop-error"]');
    expect(error.getAttribute("role")).toBe("alert");
    expect(error.textContent).toContain("전체 정지하지 못했습니다");
    expect(error.textContent).toContain("이미 전체 정지입니다");
    expect(server.count("GET", "/api/team")).toBeGreaterThan(teamReads);
  });

  it("서버가 받지 않거나(500) 보내지 못하면 오류 줄을 보이고 단추는 남는다", async () => {
    // 검증: V85 E4
    for (const reply of [{ status: 500, body: { error: "internal" } }, "network" as const]) {
      const main = await open(team("operating"), (fake) => {
        fake.stopReply = reply;
      });
      vi.spyOn(window, "confirm").mockReturnValue(true);
      pick<HTMLButtonElement>(main, 'button[data-action="stop"]').click();
      await settle();
      const error = pick(main, '[data-role="stop-error"]');
      expect(error.getAttribute("role")).toBe("alert");
      expect(error.textContent).toContain("전체 정지하지 못했습니다");
      expect(main.querySelectorAll('button[data-action="stop"]').length, "단추는 남는다").toBe(1);
      vi.restoreAllMocks();
      vi.unstubAllGlobals();
    }
  });
});

describe("계좌 화면 · 전체 정지 모델", () => {
  it("정지 단추는 전체 정지가 아닌 상태에서만, 다시 시작은 전체 정지에서만", () => {
    // 검증: V85 R14-1
    for (const state of ["operating", "resting", "daily_limit_halt", "anomaly_halt", "unknown_state"]) {
      expect(canStop(state), state).toBe(true);
    }
    expect(canStop("full_stop")).toBe(false);
    expect(canRestart("full_stop")).toBe(true);
  });

  it("정지 응답을 결과와 글로 읽는다", () => {
    // 검증: V85 E4
    expect(stopResult(200, { state: "full_stop" })).toEqual({ kind: "stopped", state: "full_stop" });
    expect(stopText(stopResult(200, { state: "full_stop" }))).toBe(
      `전체 정지했습니다 — 지금 상태: ${teamStateText("full_stop")}`,
    );
    expect(stopResult(409, { error: "already_stopped" })).toEqual({ kind: "refused", reason: "이미 전체 정지입니다" });
    expect(stopResult(400, { error: "x", message: "까닭 글" })).toEqual({ kind: "refused", reason: "까닭 글" });
    expect(stopResult(500, { error: "internal" })).toEqual({ kind: "refused", reason: "서버가 받지 않았습니다" });
    expect(stopResult(0, null)).toEqual({ kind: "refused", reason: "서버가 받지 않았습니다" });
    expect(stopText({ kind: "refused", reason: "이미 전체 정지입니다" })).toBe(
      "전체 정지하지 못했습니다 — 이미 전체 정지입니다",
    );
  });

  it("확인 글은 막히는 것 · 계속 도는 것 · 다시 시작하는 곳을 말한다", () => {
    // 검증: V85 F4 R14-1
    expect(STOP_CONFIRM).toContain("새 주문이 모두 막힙니다");
    expect(STOP_CONFIRM).toContain("손절은 계속");
    expect(STOP_CONFIRM).toContain("다시 시작은 이 화면에서만");
  });

  it("정지 자리는 위험 단추 하나이고 누르면 넘긴 함수를 부른다", () => {
    // 검증: V85
    const onStop = vi.fn();
    const node = renderStop(onStop);
    expect(node.getAttribute("data-role")).toBe("stop");
    const buttons = node.querySelectorAll<HTMLButtonElement>('button[data-action="stop"]');
    expect(buttons.length).toBe(1);
    expect(buttons[0].dataset.kind).toBe("danger");
    buttons[0].click();
    expect(onStop).toHaveBeenCalledTimes(1);
  });

  it("계좌 화면의 경로 표에 정지 경로가 있다", () => {
    // 검증: D11 C2
    expect(SCREEN_PATHS.account).toContain("/api/team/stop");
    expect(SCREEN_PATHS.account).toContain("/api/team/restart");
  });
});
