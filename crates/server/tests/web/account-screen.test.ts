// 계좌 · 운용 화면(server 30차 2번 · 3번 · C31 · V85 · V86 · R3-1 · R3-3 · R14-1 · R16-5 · F1-6 · E57)을 jsdom에 띄운다
// (가짜 fetch · 가짜 WebSocket). 팀 상태 카드(상태 · 오늘 매매/쉼 · 쉬는 이유), 전체 정지일 때만 "다시 시작" 단추 —
// 확인 뒤에만 보내고 결과를 보임, 계좌 카드(평가액 · 현금 · 오늘 손익 · 낙폭 막대), 보유 표와 빈 상태, 늘 보이는
// 모의계좌 안내, 읽기 실패와 다시 시도, `/ws` 이벤트와 일정 간격의 다시 읽기, 경로 표, 3D에 닿지 않음.
import { afterEach, describe, expect, it, vi } from "vitest";
import { SCREEN_PATHS } from "../../web/src/paths";
import type { AccountReply, TeamReply } from "../../web/src/account/model";
import {
  EMPTY_HOLDINGS,
  FULL_STOP_TEXT,
  PAPER_NOTICE,
  REFRESH_KINDS,
  REFRESH_MS,
  RESTART_CONFIRM,
} from "../../web/src/account/model";
import { mountAccount } from "../../web/src/account/screen";
import { mainOf, reach } from "./source-graph";
import { account, holding, team } from "./account-fixture";

const PAGE = import.meta.glob<string>("../../web/account/index.html", { query: "?raw", import: "default", eager: true });
const MAIN = import.meta.glob("../../web/src/account/main.ts");

interface Call {
  method: string;
  url: string;
  path: string;
}

/** 가짜 서버: 팀 상태 · 계좌를 들고 답한다. 다시 시작은 정해 둔 답을 낸다. */
class FakeServer {
  calls: Call[] = [];
  failTeam = false;
  failAccount = false;
  restartReply: { status: number; body: unknown } = { status: 200, body: { state: "resting" } };

  constructor(
    public team: TeamReply,
    public account: AccountReply,
  ) {}

  readonly fetch = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = String(input);
    const method = (init?.method ?? "GET").toUpperCase();
    const path = new URL(url, "http://local.test").pathname;
    this.calls.push({ method, url, path });
    const json = (status: number, value: unknown) =>
      new Response(JSON.stringify(value), { status, headers: { "Content-Type": "application/json" } });
    if (method === "POST" && path === "/api/session") return json(200, {});
    if (method === "POST" && path === "/api/team/restart") return json(this.restartReply.status, this.restartReply.body);
    if (method !== "GET") return json(404, { error: "not_found" });
    if (path === "/api/team") return this.failTeam ? json(500, { error: "internal" }) : json(200, this.team);
    if (path === "/api/account") return this.failAccount ? json(500, { error: "internal" }) : json(200, this.account);
    return json(404, { error: "not_found" });
  });

  count(method: string, path: string): number {
    return this.calls.filter((call) => call.method === method && call.path === path).length;
  }
}

/** 가짜 실시간 연결: 연 주소를 남기고 시험이 이벤트를 흘려 넣는다. */
class FakeSocket extends EventTarget {
  static opened: FakeSocket[] = [];
  static readonly CONNECTING = 0;
  static readonly OPEN = 1;
  static readonly CLOSING = 2;
  static readonly CLOSED = 3;
  readyState = 0;
  onopen: ((event: Event) => void) | null = null;
  onmessage: ((event: MessageEvent) => void) | null = null;
  onclose: ((event: Event) => void) | null = null;
  onerror: ((event: Event) => void) | null = null;

  constructor(public url: string) {
    super();
    FakeSocket.opened.push(this);
  }

  /** 이벤트 하나를 받은 것처럼 한다. */
  emit(frame: unknown): void {
    const event = new MessageEvent("message", { data: JSON.stringify(frame) });
    this.onmessage?.(event);
    this.dispatchEvent(event);
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

/** 본문 자리 하나에 화면을 띄운다. */
async function open(teamReply: TeamReply, accountReply: AccountReply, prepare?: (fake: FakeServer) => void): Promise<HTMLElement> {
  server = new FakeServer(teamReply, accountReply);
  prepare?.(server);
  FakeSocket.opened = [];
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

function text(root: ParentNode, selector: string): string {
  return pick(root, selector).textContent ?? "";
}

/** 모의계좌 안내가 보인다. */
function expectNotice(main: HTMLElement): void {
  const notices = [...main.querySelectorAll('[data-role="paper-notice"]')];
  expect(notices.length, "모의계좌 안내 하나").toBe(1);
  expect(notices[0].textContent).toContain(PAPER_NOTICE);
}

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  document.body.innerHTML = "";
});

describe("계좌 화면 · 팀 상태", () => {
  it("띄우면 세션을 얻고 팀 상태와 계좌를 읽어 상태 · 오늘 매매를 보인다", async () => {
    // 검증: V86 R3-1 F1-6 C31
    const main = await open(team("operating", "trade"), account([holding("000660")]));
    expect(server.count("POST", "/api/session")).toBe(1);
    expect(server.count("GET", "/api/team")).toBeGreaterThan(0);
    expect(server.count("GET", "/api/account")).toBeGreaterThan(0);
    const card = pick(main, '[data-role="team"]');
    expect(text(card, '[data-role="team-state"]')).toContain("운용 중");
    expect(text(card, '[data-role="team-plan"]')).toContain("오늘 매매합니다");
    expect(card.querySelector('[data-role="team-reason"]')).toBeNull();
  });

  it("쉬는 날이면 쉼과 쉬는 이유를 보인다", async () => {
    // 검증: V86 F1-6
    const main = await open(team("resting", "rest", "변동성이 너무 큼"), account());
    const card = pick(main, '[data-role="team"]');
    expect(text(card, '[data-role="team-state"]')).toContain("쉼");
    expect(text(card, '[data-role="team-plan"]')).toContain("오늘 쉽니다");
    expect(text(card, '[data-role="team-reason"]')).toContain("변동성이 너무 큼");
  });

  it("상태마다 그 글을 보이고 전체 정지일 때만 까닭과 '다시 시작' 단추가 있다", async () => {
    // 검증: V85 V86 R3-3 R14-1 F1-6
    const cases: [string, string][] = [
      ["operating", "운용 중"],
      ["resting", "쉼"],
      ["daily_limit_halt", "하루 한도 멈춤"],
      ["anomaly_halt", "이상 감지 멈춤"],
      ["full_stop", "전체 정지"],
    ];
    for (const [state, label] of cases) {
      const main = await open(team(state), account());
      const card = pick(main, '[data-role="team"]');
      expect(text(card, '[data-role="team-state"]'), state).toContain(label);
      const restart = main.querySelectorAll('button[data-action="restart"]');
      expect(restart.length, `${state}: 다시 시작 단추`).toBe(state === "full_stop" ? 1 : 0);
      expect((card.textContent ?? "").includes(FULL_STOP_TEXT), `${state}: 전체 정지 까닭`).toBe(state === "full_stop");
      expectNotice(main);
      vi.unstubAllGlobals();
    }
  });
});

describe("계좌 화면 · 다시 시작", () => {
  it("확인을 거절하면 아무것도 보내지 않는다", async () => {
    // 검증: V85 R3-3 R14-1
    const main = await open(team("full_stop"), account());
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(false);
    pick<HTMLButtonElement>(main, 'button[data-action="restart"]').click();
    await settle();
    expect(confirm).toHaveBeenCalledTimes(1);
    expect(confirm).toHaveBeenCalledWith(RESTART_CONFIRM);
    expect(server.count("POST", "/api/team/restart")).toBe(0);
    expect(main.querySelector('[data-role="restart-result"]')?.textContent ?? "").toBe("");
  });

  it("확인 뒤에만 다시 시작을 보내고, 다시 시작됨을 보이고 팀 상태 · 계좌를 다시 읽는다", async () => {
    // 검증: V85 R3-3 R14-1 C31
    const main = await open(team("full_stop"), account());
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(true);
    const teamReads = server.count("GET", "/api/team");
    const accountReads = server.count("GET", "/api/account");
    server.team = team("resting");
    pick<HTMLButtonElement>(main, 'button[data-action="restart"]').click();
    await settle();
    expect(confirm).toHaveBeenCalledTimes(1);
    expect(server.count("POST", "/api/team/restart")).toBe(1);
    const result = pick(main, '[data-role="restart-result"]');
    expect(result.getAttribute("role")).toBe("status");
    expect(result.textContent).toContain("다시 시작했습니다");
    expect(server.count("GET", "/api/team")).toBeGreaterThan(teamReads);
    expect(server.count("GET", "/api/account")).toBeGreaterThan(accountReads);
    expect(text(main, '[data-role="team-state"]')).toContain("쉼");
    expect(main.querySelector('button[data-action="restart"]')).toBeNull();
    expectNotice(main);
  });

  it("승인이 필요하면 승인 번호를 보인다", async () => {
    // 검증: V85 R3-3 R14-1
    const main = await open(team("full_stop"), account(), (fake) => {
      fake.restartReply = { status: 200, body: { state: "needs_approval", number: "A-0042" } };
    });
    vi.spyOn(window, "confirm").mockReturnValue(true);
    pick<HTMLButtonElement>(main, 'button[data-action="restart"]').click();
    await settle();
    const result = text(main, '[data-role="restart-result"]');
    expect(result).toContain("승인이 필요합니다");
    expect(result).toContain("A-0042");
  });

  it("거절되면 그 이유를 보이고 단추는 남는다", async () => {
    // 검증: V85 R3-3 E9
    const main = await open(team("full_stop"), account(), (fake) => {
      fake.restartReply = {
        status: 409,
        body: { error: "keys_required", message: "증권사 키와 AI 키를 먼저 다시 등록해야 다시 시작할 수 있습니다" },
      };
    });
    vi.spyOn(window, "confirm").mockReturnValue(true);
    pick<HTMLButtonElement>(main, 'button[data-action="restart"]').click();
    await settle();
    const result = text(main, '[data-role="restart-result"]');
    expect(result).toContain("다시 시작하지 못했습니다");
    expect(result).toContain("증권사 키와 AI 키를 먼저 다시 등록해야");
    expect(main.querySelector('button[data-action="restart"]')).not.toBeNull();
  });
});

describe("계좌 화면 · 계좌 카드와 보유 표", () => {
  it("계좌 카드에 통화마다 총 평가액 · 쓸 수 있는 현금 · 오늘 손익 · 낙폭과 한도 대비 막대가 보인다", async () => {
    // 검증: V86 R3-1 R16-5 C31
    const main = await open(team("operating", "trade"), account([holding("000660")], { drawdown: "5", limit: "20" }));
    const card = pick(main, '[data-role="account"]');
    const krw = pick(card, '[data-currency="KRW"]');
    expect(text(krw, '[data-role="equity-total"]')).toContain("9,974,925원");
    expect(text(krw, '[data-role="cash"]')).toContain("9,499,925원");
    const dayPnl = pick(krw, '[data-role="day-pnl"]');
    expect(dayPnl.textContent).toContain("-25,075원");
    expect(dayPnl.getAttribute("data-trend")).toBe("down");
    expect(text(krw, '[data-role="drawdown"]')).toContain("5.00%");
    expect(text(krw, '[data-role="drawdown"]')).toContain("20.00%");
    expect(Number(pick(krw, '[data-role="drawdown-bar"]').getAttribute("data-ratio"))).toBeCloseTo(0.25, 10);
    const usd = pick(card, '[data-currency="USD"]');
    expect(text(usd, '[data-role="equity-total"]')).toContain("$10,000.00");
  });

  it("낙폭이 한도를 넘으면 막대는 가득(1)이고, 한도를 모르면 막대가 없다", async () => {
    // 검증: V86 R16-5
    let main = await open(team("full_stop"), account([], { drawdown: "25", limit: "20" }));
    expect(pick(main, '[data-currency="KRW"] [data-role="drawdown-bar"]').getAttribute("data-ratio")).toBe("1");
    vi.unstubAllGlobals();
    main = await open(team("resting"), account([], { drawdown: "0", limit: null }));
    expect(main.querySelector('[data-currency="KRW"] [data-role="drawdown-bar"]')).toBeNull();
    expect(text(main, '[data-currency="KRW"] [data-role="drawdown"]')).toContain("0.00%");
  });

  it("보유 표에 종목마다 수량 · 평균 단가 · 지금 값 · 평가 · 부호 붙은 평가 손익과 오름내림이 보인다", async () => {
    // 검증: V86 R3-1 R16-5
    const main = await open(
      team("operating", "trade"),
      account([
        holding("000660"),
        holding("035420", { quantity: "2", average_price: "200000", last_price: "210000", value: "420000", pnl: "20000" }),
      ]),
    );
    const table = pick(main, '[data-role="holdings"]');
    const rows = [...table.querySelectorAll("tr[data-symbol]")];
    expect(rows.map((row) => row.getAttribute("data-symbol"))).toEqual(["000660", "035420"]);
    const first = rows[0].textContent ?? "";
    for (const part of ["000660", "5", "100,015원", "95,000원", "475,000원", "-25,075원"]) expect(first).toContain(part);
    expect(rows[0].querySelector('[data-trend="down"]')).not.toBeNull();
    expect(rows[1].textContent).toContain("+20,000원");
    expect(rows[1].querySelector('[data-trend="up"]')).not.toBeNull();
    expect(table.querySelector('[data-role="empty"]')).toBeNull();
  });

  it("보유가 없으면 보유 표 자리에 빈 상태가 보인다", async () => {
    // 검증: V86 E57
    const main = await open(team("resting"), account([]));
    const holdings = pick(main, '[data-role="holdings"]');
    expect(holdings.querySelectorAll("tr[data-symbol]").length).toBe(0);
    expect(text(holdings, '[data-role="empty"]')).toContain(EMPTY_HOLDINGS);
  });
});

describe("계좌 화면 · 읽기 실패", () => {
  it("계좌를 못 읽으면 오류 줄과 다시 시도가 보이고, 안내는 그대로 보이며 다시 시도하면 그린다", async () => {
    // 검증: E57 V86
    const main = await open(team("operating", "trade"), account([holding("000660")]), (fake) => {
      fake.failAccount = true;
    });
    expect(main.querySelector('[role="alert"]')).not.toBeNull();
    expectNotice(main);
    const before = server.count("GET", "/api/account");
    server.failAccount = false;
    pick<HTMLButtonElement>(main, 'button[data-action="retry"]').click();
    await settle();
    expect(server.count("GET", "/api/account")).toBeGreaterThan(before);
    expect(main.querySelector('[role="alert"]')).toBeNull();
    expect(main.querySelector('[data-role="holdings"] tr[data-symbol="000660"]')).not.toBeNull();
    expectNotice(main);
  });

  it("팀 상태를 못 읽어도 오류 줄과 다시 시도가 보인다", async () => {
    // 검증: E57 V86
    const main = await open(team("operating"), account(), (fake) => {
      fake.failTeam = true;
    });
    expect(main.querySelector('[role="alert"]')).not.toBeNull();
    expect(main.querySelector('button[data-action="retry"]')).not.toBeNull();
    expectNotice(main);
    server.failTeam = false;
    pick<HTMLButtonElement>(main, 'button[data-action="retry"]').click();
    await settle();
    expect(main.querySelector('[role="alert"]')).toBeNull();
    expect(text(main, '[data-role="team-state"]')).toContain("운용 중");
  });
});

describe("계좌 화면 · 다시 읽기", () => {
  it("같은 출처의 /ws를 열고, 하루 시작 · 마감 · 판단 이벤트를 받으면 팀 상태와 계좌를 다시 읽는다", async () => {
    // 검증: V86 R3-1 D11
    const main = await open(team("resting"), account());
    expect(FakeSocket.opened.length, "실시간 연결을 연다").toBeGreaterThan(0);
    const socket = FakeSocket.opened[FakeSocket.opened.length - 1];
    const url = new URL(socket.url);
    expect(["ws:", "wss:"]).toContain(url.protocol);
    expect(url.host).toBe(location.host);
    expect(url.pathname).toBe("/ws");

    socket.readyState = 1;
    socket.onopen?.(new Event("open"));
    socket.dispatchEvent(new Event("open"));
    const teamReads = server.count("GET", "/api/team");
    const accountReads = server.count("GET", "/api/account");
    server.team = team("operating", "trade");
    socket.emit({ seq: 7, at: 0, kind: REFRESH_KINDS[0], payload: {} });
    await settle();
    expect(server.count("GET", "/api/team")).toBeGreaterThan(teamReads);
    expect(server.count("GET", "/api/account")).toBeGreaterThan(accountReads);
    expect(text(main, '[data-role="team-state"]')).toContain("운용 중");
  });

  it(`실시간 연결과 별개로 ${REFRESH_MS}밀리초마다 다시 읽는다(가짜 타이머)`, async () => {
    // 검증: V86 R3-1
    vi.useFakeTimers({ toFake: ["setInterval", "clearInterval", "setTimeout", "clearTimeout"] });
    server = new FakeServer(team("resting"), account());
    FakeSocket.opened = [];
    vi.stubGlobal("fetch", server.fetch);
    vi.stubGlobal("WebSocket", FakeSocket);
    document.body.innerHTML = '<div id="menu"></div><main id="screen"><h1>계좌</h1></main>';
    const main = document.getElementById("screen") as HTMLElement;
    const mounted = mountAccount(main);
    await vi.advanceTimersByTimeAsync(0);
    await mounted;
    await vi.advanceTimersByTimeAsync(0);
    const teamReads = server.count("GET", "/api/team");
    const accountReads = server.count("GET", "/api/account");
    expect(teamReads).toBeGreaterThan(0);

    await vi.advanceTimersByTimeAsync(REFRESH_MS - 100);
    expect(server.count("GET", "/api/account"), "간격 전에는 다시 읽지 않는다").toBe(accountReads);
    await vi.advanceTimersByTimeAsync(200);
    expect(server.count("GET", "/api/team")).toBeGreaterThan(teamReads);
    expect(server.count("GET", "/api/account")).toBeGreaterThan(accountReads);
    expectNotice(main);
  });
});

describe("계좌 화면 · 경로 · 가벼움", () => {
  it("화면이 부르는 경로는 모두 계좌 화면의 경로 표에 있고 바깥 주소가 없다", async () => {
    // 검증: D11 D2 C31
    const main = await open(team("full_stop"), account([holding("000660")]));
    vi.spyOn(window, "confirm").mockReturnValue(true);
    pick<HTMLButtonElement>(main, 'button[data-action="restart"]').click();
    await settle();
    const seen = new Set(server.calls.map((call) => call.path));
    for (const socket of FakeSocket.opened) seen.add(new URL(socket.url).pathname);
    const table = SCREEN_PATHS.account;
    for (const path of seen) expect(table, path).toContain(path);
    for (const path of ["/api/session", "/api/team", "/api/account", "/api/team/restart", "/ws"]) {
      expect(seen.has(path), `${path}을 부른다`).toBe(true);
    }
    expect(server.calls.every((call) => !call.url.includes("://"))).toBe(true);
  });

  it("계좌 main은 메뉴와 화면을 그리고 '준비 중'이 아니며 모의계좌 안내가 보인다", async () => {
    // 검증: V86 V75 F12
    server = new FakeServer(team("operating", "trade"), account([holding("000660")]));
    FakeSocket.opened = [];
    vi.stubGlobal("fetch", server.fetch);
    vi.stubGlobal("WebSocket", FakeSocket);
    vi.resetModules();
    const html = PAGE["../../web/account/index.html"];
    document.body.innerHTML = new DOMParser().parseFromString(html, "text/html").body.innerHTML;
    await MAIN["../../web/src/account/main.ts"]();
    await settle();
    const current = [...document.querySelectorAll('#menu nav [aria-current="page"]')];
    expect(current.map((a) => a.textContent?.trim())).toEqual(["계좌"]);
    expect(document.querySelectorAll("h1").length).toBe(1);
    expect(document.body.textContent).not.toContain("준비 중");
    expect(document.querySelector('[data-role="team"]')).not.toBeNull();
    expect(document.querySelector('[data-role="holdings"] tr[data-symbol="000660"]')).not.toBeNull();
    expectNotice(document.getElementById("screen") as HTMLElement);
  });

  it("계좌 화면 소스는 three · 사무실 3D에 닿지 않고 공통 부품을 쓴다", () => {
    // 검증: V77 N21
    const { files, bare } = reach(mainOf("account"));
    expect(files.has("../../web/src/account/screen.ts"), "화면 모듈에 닿는다").toBe(true);
    expect(files.has("../../web/src/account/model.ts"), "화면 모델에 닿는다").toBe(true);
    expect(files.has("../../web/src/account/view.ts"), "그리기 모듈에 닿는다").toBe(true);
    expect(files.has("../../web/src/ui/parts.ts"), "공통 부품을 쓴다").toBe(true);
    expect([...bare].filter((name) => name === "three" || name.startsWith("three/"))).toEqual([]);
    expect([...files].filter((file) => file.includes("scene3d") || file.includes("/src/office/"))).toEqual([]);
  });
});
