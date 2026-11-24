// 템플릿 화면(server 27차 3번 · 4번 · F6 · C28 · E5 · E28 · E57 · E58 · V79~V81)을 jsdom에 띄운다(가짜 fetch).
// 고르기 → 목록 다시 읽기, 값 저장(성공 · 상한 넘음 · 승인 대기), 잠김, 승격 기준 표 · 판 이력, 실전 안내가 늘 보임,
// 멈추기 · 실전 운용 시작은 확인 뒤에만, 부르는 경로는 표에 있고, 3D에 닿지 않는다.
import { afterEach, describe, expect, it, vi } from "vitest";
import { SCREEN_PATHS, templateStopPath } from "../../web/src/paths";
import { LIVE_NOTICE, type TemplateEntry, type TemplatesReply } from "../../web/src/templates/model";
import { mountTemplates } from "../../web/src/templates/screen";
import { mainOf, reach } from "./source-graph";
import { replyWith, swingEntry } from "./template-fixture";

const PAGE = import.meta.glob<string>("../../web/templates/index.html", { query: "?raw", import: "default", eager: true });
const MAIN = import.meta.glob("../../web/src/templates/main.ts");

interface Call {
  method: string;
  url: string;
  body: unknown;
}

/** 가짜 서버: 읽기 응답을 들고 있고, 고르기 · 멈추기는 그 응답을 바꾼다. 값 저장 답은 시험이 정한다. */
class FakeServer {
  calls: Call[] = [];
  view: TemplatesReply;
  failView = false;
  adjust: { status: number; body: unknown } = { status: 200, body: { id: 1, revision: 4, state: "simulating" } };

  constructor(view: TemplatesReply) {
    this.view = view;
  }

  readonly fetch = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = String(input);
    const method = (init?.method ?? "GET").toUpperCase();
    const raw = typeof init?.body === "string" ? init.body : null;
    const body = raw ? (JSON.parse(raw) as unknown) : null;
    this.calls.push({ method, url, body });
    const json = (status: number, value: unknown) =>
      new Response(JSON.stringify(value), { status, headers: { "Content-Type": "application/json" } });
    if (method === "POST" && url === "/api/session") return json(200, {});
    if (method === "GET" && url === "/api/templates/current") {
      return this.failView ? json(500, { error: "internal" }) : json(200, this.view);
    }
    if (method === "POST" && url === "/api/templates") {
      const style = (body as { style: string }).style;
      const id = this.view.templates.length + 10;
      const made: TemplateEntry = { ...swingEntry(id, "simulating"), style: style as TemplateEntry["style"] };
      this.view = { current: id, templates: [...this.view.templates, made] };
      return json(200, { id, style, state: "simulating" });
    }
    if (method === "POST" && url === "/api/templates/adjust") return json(this.adjust.status, this.adjust.body);
    const stop = /^\/api\/templates\/(\d+)\/stop$/.exec(url);
    if (method === "POST" && stop) {
      const id = Number(stop[1]);
      this.view = {
        ...this.view,
        templates: this.view.templates.map((t): TemplateEntry => (t.id === id ? { ...t, state: "stopped" } : t)),
      };
      return json(200, { id, state: "stopped" });
    }
    const card = /^\/api\/approvals\/([^/]+)$/.exec(url);
    if (method === "GET" && card) {
      return json(200, { number: decodeURIComponent(card[1]), summary: "실전 승격", fingerprint: "f".repeat(64), short_code: "ffffff", status: "pending" });
    }
    if (method === "POST" && /^\/api\/approvals\/[^/]+\/approve$/.test(url)) return json(200, { result: "approved", reason: null });
    return json(404, { error: "not_found" });
  });

  /** 그 방법 · 경로로 온 요청들. */
  sent(method: string, url: string): Call[] {
    return this.calls.filter((call) => call.method === method && call.url === url);
  }
}

let server: FakeServer;
let confirmAnswer = true;
const confirmSpy = vi.fn(() => confirmAnswer);

async function settle(): Promise<void> {
  for (let i = 0; i < 20; i++) await new Promise((resolve) => setTimeout(resolve, 0));
}

/** 본문 자리 하나에 화면을 띄운다. */
async function open(view: TemplatesReply): Promise<HTMLElement> {
  server = new FakeServer(view);
  vi.stubGlobal("fetch", server.fetch);
  confirmAnswer = true;
  confirmSpy.mockClear();
  vi.stubGlobal("confirm", confirmSpy);
  document.body.innerHTML = '<div id="menu"></div><main id="screen"><h1>템플릿</h1></main>';
  const main = document.getElementById("screen") as HTMLElement;
  await mountTemplates(main);
  await settle();
  return main;
}

function pick<T extends Element>(root: ParentNode, selector: string): T {
  const found = root.querySelector<T>(selector);
  expect(found, selector).not.toBeNull();
  return found as T;
}

function click(root: ParentNode, selector: string): void {
  pick<HTMLElement>(root, selector).click();
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("템플릿 화면 · 고르기", () => {
  it("템플릿이 없어도 단타 · 스윙 · 중장기 고르기 단추가 있다", async () => {
    // 검증: V79 R10-1 F6
    const main = await open(replyWith(null));
    const buttons = [...main.querySelectorAll<HTMLButtonElement>('[data-role="styles"] button[data-style]')];
    expect(buttons.map((b) => [b.dataset.style, b.textContent?.trim()])).toEqual([
      ["day_trade", "단타"],
      ["swing", "스윙"],
      ["long_term", "중장기"],
    ]);
    expect(main.querySelector('[data-role="fields"]')).toBeNull();
  });

  it("스타일을 고르면 고르기 요청 뒤 목록을 다시 읽고 새 템플릿을 그린다", async () => {
    // 검증: V79 R10-1 C28
    const main = await open(replyWith(null));
    const readsBefore = server.sent("GET", "/api/templates/current").length;
    click(main, '[data-role="styles"] button[data-style="swing"]');
    await settle();
    expect(server.sent("POST", "/api/templates").map((call) => call.body)).toEqual([{ style: "swing" }]);
    expect(server.sent("GET", "/api/templates/current").length).toBe(readsBefore + 1);
    const chose = server.calls.findIndex((call) => call.method === "POST" && call.url === "/api/templates");
    const reread = server.calls.findIndex((call, index) => index > chose && call.url === "/api/templates/current");
    expect(reread, "고른 뒤에 다시 읽는다").toBeGreaterThan(chose);
    expect(pick(main, '[data-role="state"]').textContent).toContain("모의중");
    expect(main.querySelector('[data-role="fields"] tr[data-key="risk_per_trade_pct"]')).not.toBeNull();
  });
});

describe("템플릿 화면 · 값 맞추기", () => {
  it("칸마다 지금 값 · 상한 · 입력 칸이 있고 모의중이면 입력할 수 있다", async () => {
    // 검증: V79 V80 R10-2
    const main = await open(replyWith(swingEntry(1, "simulating")));
    const row = pick(main, '[data-role="fields"] tr[data-key="risk_per_trade_pct"]');
    expect(pick(row, '[data-role="value"]').textContent).toContain("1%");
    expect(pick(row, '[data-role="cap"]').textContent).toContain("상한 2%");
    expect(pick<HTMLInputElement>(row, "input").disabled).toBe(false);
  });

  it("상한을 넘으면 그 칸 옆에 상한 안내가 보이고 지금 값은 그대로다", async () => {
    // 검증: V80 E5 R10-2
    const main = await open(replyWith(swingEntry(1, "simulating")));
    server.adjust = { status: 400, body: { error: "over_cap", fields: ["risk_per_trade_pct"] } };
    const row = pick(main, '[data-role="fields"] tr[data-key="risk_per_trade_pct"]');
    const input = pick<HTMLInputElement>(row, "input");
    input.value = "3";
    input.dispatchEvent(new Event("input", { bubbles: true }));
    click(main, 'button[data-action="save"]');
    await settle();
    expect(server.sent("POST", "/api/templates/adjust").map((call) => call.body)).toEqual([
      { id: 1, changes: { risk_per_trade_pct: "3" } },
    ]);
    const after = pick(main, '[data-role="fields"] tr[data-key="risk_per_trade_pct"]');
    expect(pick(after, '[data-role="field-note"]').textContent).toContain("상한 2%");
    expect(pick(after, '[data-role="value"]').textContent).toContain("1%");
    const other = pick(main, '[data-role="fields"] tr[data-key="daily_loss_limit_pct"]');
    expect(other.querySelector('[data-role="field-note"]')?.textContent ?? "").not.toContain("상한");
    expect(pick(main, '[data-role="save-result"]').textContent).toContain("상한");
  });

  it("승인 대기가 되면 '승인 뒤 다음 거래일부터'와 승인 번호가 보인다", async () => {
    // 검증: E28 V81 R11-2
    const main = await open(replyWith(swingEntry(1, "simulating")));
    server.adjust = {
      status: 200,
      body: { state: "needs_approval", proposal: 3, widened: ["risk_per_trade_pct"], number: "AP-31" },
    };
    const input = pick<HTMLInputElement>(main, '[data-role="fields"] tr[data-key="risk_per_trade_pct"] input');
    input.value = "1.5";
    input.dispatchEvent(new Event("input", { bubbles: true }));
    click(main, 'button[data-action="save"]');
    await settle();
    const result = pick(main, '[data-role="save-result"]').textContent ?? "";
    expect(result).toContain("승인 뒤 다음 거래일부터");
    expect(result).toContain("AP-31");
  });

  it("저장되면 목록을 다시 읽고 저장 문구를 보인다", async () => {
    // 검증: V79 C28
    const main = await open(replyWith(swingEntry(1, "simulating")));
    const readsBefore = server.sent("GET", "/api/templates/current").length;
    const input = pick<HTMLInputElement>(main, '[data-role="fields"] tr[data-key="risk_per_trade_pct"] input');
    input.value = "0.5";
    input.dispatchEvent(new Event("input", { bubbles: true }));
    click(main, 'button[data-action="save"]');
    await settle();
    expect(server.sent("POST", "/api/templates/adjust").length).toBe(1);
    expect(server.sent("GET", "/api/templates/current").length).toBeGreaterThan(readsBefore);
    expect(pick(main, '[data-role="save-result"]').textContent).toContain("4");
  });

  it("승격대기 · 멈춤이면 모든 입력 칸이 잠기고 승인 안내는 없다", async () => {
    // 검증: V79 E28
    for (const state of ["promotion_pending", "stopped"] as const) {
      const main = await open(replyWith(swingEntry(1, state)));
      const inputs = [...main.querySelectorAll<HTMLInputElement>('[data-role="fields"] input')];
      expect(inputs.length, state).toBe(3);
      for (const input of inputs) expect(input.disabled, state).toBe(true);
      expect(main.querySelector('[data-role="approval-notice"]'), state).toBeNull();
    }
  });

  it("실전중이면 입력할 수 있고 값 표 위에 승인이 필요하다는 안내가 보인다", async () => {
    // 검증: V79 E28 V81
    const main = await open(replyWith(swingEntry(1, "live")));
    const inputs = [...main.querySelectorAll<HTMLInputElement>('[data-role="fields"] input')];
    expect(inputs.length).toBe(3);
    for (const input of inputs) expect(input.disabled).toBe(false);
    const notice = pick<HTMLElement>(main, '[data-role="approval-notice"]');
    expect(notice.textContent).toContain("승인");
    const table = pick<HTMLElement>(main, '[data-role="fields"]');
    expect(notice.compareDocumentPosition(table) & Node.DOCUMENT_POSITION_FOLLOWING, "안내가 값 표 위").toBeTruthy();
    const simulating = await open(replyWith(swingEntry(1, "simulating")));
    expect(simulating.querySelector('[data-role="approval-notice"]')).toBeNull();
  });

  it("실전중 저장이 잠금(409)으로 거절되면 실전중 · 승인 뜻의 문구가 보인다", async () => {
    // 검증: E28 E57
    const main = await open(replyWith(swingEntry(1, "live")));
    server.adjust = { status: 409, body: { error: "locked" } };
    const input = pick<HTMLInputElement>(main, '[data-role="fields"] tr[data-key="risk_per_trade_pct"] input');
    input.value = "0.5";
    input.dispatchEvent(new Event("input", { bubbles: true }));
    click(main, 'button[data-action="save"]');
    await settle();
    const result = pick(main, '[data-role="save-result"]').textContent ?? "";
    expect(result).toContain("실전중");
    expect(result).toContain("승인");
  });
});

describe("템플릿 화면 · 상태 · 승격 기준 · 판 이력", () => {
  it("상태 배지와 승격 기준 표에서 모자란 기준이 보인다", async () => {
    // 검증: V81 E58 R11-2
    const main = await open(replyWith(swingEntry(1, "simulating")));
    expect(pick(main, '[data-role="state"]').textContent).toContain("모의중");
    const rows = [...main.querySelectorAll<HTMLElement>('[data-role="promotion"] tr[data-criterion]')];
    expect(rows.map((row) => row.dataset.criterion)).toEqual(["trades", "trading_days", "net_pnl", "max_drawdown", "out_of_sample"]);
    const short = rows.filter((row) => row.dataset.short === "true").map((row) => row.dataset.criterion);
    expect(short).toEqual(["trades", "net_pnl", "out_of_sample"]);
    expect(rows[0].textContent).toContain("30");
    expect(rows[0].textContent).toContain("12");
  });

  it("판 이력에 판마다 바뀐 칸과 넓힘 표시가 보인다", async () => {
    // 검증: V81 R11-2 E28
    const main = await open(replyWith(swingEntry(1, "live")));
    const history = pick(main, '[data-role="revisions"]');
    expect([...history.querySelectorAll<HTMLElement>("[data-revision]")].map((node) => node.dataset.revision)).toEqual(["1", "2", "3"]);
    const narrowed = pick<HTMLElement>(history, '[data-revision="2"] [data-key="risk_per_trade_pct"]');
    expect(narrowed.dataset.widened).not.toBe("true");
    expect(narrowed.textContent).not.toContain("넓힘");
    const widened = pick<HTMLElement>(history, '[data-revision="3"] [data-key="daily_loss_limit_pct"]');
    expect(widened.dataset.widened).toBe("true");
    expect(widened.textContent).toContain("넓힘");
    expect(widened.textContent).toContain("3%");
    expect(widened.textContent).toContain("4%");
  });
});

describe("템플릿 화면 · 실전 운용 시작 · 멈추기", () => {
  it("실전 안내(실제 주문은 나가지 않는다)가 어느 상태에서도 늘 보인다", async () => {
    // 검증: E58 V81
    const views = [replyWith(null), ...(["simulating", "promotion_pending", "live", "stopped"] as const).map((s) => replyWith(swingEntry(1, s)))];
    for (const view of views) {
      const main = await open(view);
      const notice = pick<HTMLElement>(main, '[data-role="live-notice"]');
      expect(notice.textContent).toContain(LIVE_NOTICE);
      expect(notice.textContent).toContain("나가지 않");
      expect(notice.hidden).toBe(false);
      expect(pick(main, 'button[data-action="promote"]'), "실전 운용 시작 단추 옆").toBeTruthy();
    }
  });

  it("실전 운용 시작은 승격대기일 때만 누를 수 있고, 확인 뒤에만 승인 카드 길로 승인한다", async () => {
    // 검증: E58 V81 F6
    for (const state of ["simulating", "live", "stopped"] as const) {
      const main = await open(replyWith(swingEntry(1, state)));
      expect(pick<HTMLButtonElement>(main, 'button[data-action="promote"]').disabled, state).toBe(true);
    }
    const main = await open(replyWith(swingEntry(1, "promotion_pending")));
    const promote = pick<HTMLButtonElement>(main, 'button[data-action="promote"]');
    expect(promote.disabled).toBe(false);

    confirmAnswer = false;
    promote.click();
    await settle();
    expect(confirmSpy).toHaveBeenCalled();
    expect(server.sent("POST", "/api/approvals/AP-7/approve")).toEqual([]);

    confirmAnswer = true;
    pick<HTMLButtonElement>(main, 'button[data-action="promote"]').click();
    await settle();
    expect(server.sent("GET", "/api/approvals/AP-7").length).toBeGreaterThan(0);
    expect(server.sent("POST", "/api/approvals/AP-7/approve").map((call) => call.body)).toEqual([
      expect.objectContaining({ fingerprint: "f".repeat(64) }),
    ]);
  });

  it("멈추기 단추는 실전중일 때만 있다", async () => {
    // 검증: V81 E58 F6
    for (const state of ["simulating", "promotion_pending", "stopped"] as const) {
      const main = await open(replyWith(swingEntry(1, state)));
      expect(main.querySelector('button[data-action="stop"]'), state).toBeNull();
    }
    expect((await open(replyWith(null))).querySelector('button[data-action="stop"]')).toBeNull();
    expect((await open(replyWith(swingEntry(1, "live")))).querySelector('button[data-action="stop"]')).not.toBeNull();
  });

  it("멈추기는 확인한 뒤에만 요청하고, 멈춘 뒤 다시 읽어 멈춤을 보인다", async () => {
    // 검증: V81 E58 F6
    const main = await open(replyWith(swingEntry(1, "live")));
    confirmAnswer = false;
    click(main, 'button[data-action="stop"]');
    await settle();
    expect(confirmSpy).toHaveBeenCalledTimes(1);
    expect(server.sent("POST", "/api/templates/1/stop")).toEqual([]);

    confirmAnswer = true;
    click(main, 'button[data-action="stop"]');
    await settle();
    expect(server.sent("POST", "/api/templates/1/stop").length).toBe(1);
    expect(pick(main, '[data-role="state"]').textContent).toContain("멈춤");
    expect(main.querySelector('button[data-action="stop"]')).toBeNull();
  });
});

describe("템플릿 화면 · 못 읽음 · 경로 · 가벼움", () => {
  it("목록을 못 읽으면 '불러오지 못함 · 다시 시도'가 보이고 다시 시도하면 그린다", async () => {
    // 검증: E57 C28
    server = new FakeServer(replyWith(swingEntry(1, "simulating")));
    server.failView = true;
    vi.stubGlobal("fetch", server.fetch);
    document.body.innerHTML = '<div id="menu"></div><main id="screen"><h1>템플릿</h1></main>';
    const main = document.getElementById("screen") as HTMLElement;
    await mountTemplates(main);
    await settle();
    expect(pick(main, '[role="alert"]').textContent).toContain("불러오지 못함");
    expect(pick<HTMLElement>(main, '[data-role="live-notice"]').hidden).toBe(false);

    server.failView = false;
    click(main, 'button[data-action="retry"]');
    await settle();
    expect(main.querySelector('[data-role="fields"] tr[data-key="risk_per_trade_pct"]')).not.toBeNull();
  });

  it("화면이 부르는 경로는 모두 템플릿 화면의 경로 표에 있다", async () => {
    // 검증: D11 D2 C28
    expect(templateStopPath(7)).toBe("/api/templates/7/stop");
    expect(templateStopPath("a/b")).toBe("/api/templates/a%2Fb/stop");
    const seen = new Set<string>();
    const record = () => server.calls.forEach((call) => seen.add(call.url));
    let main = await open(replyWith(null));
    click(main, '[data-role="styles"] button[data-style="swing"]');
    await settle();
    record();
    main = await open(replyWith(swingEntry(1, "simulating")));
    click(main, 'button[data-action="save"]');
    await settle();
    record();
    main = await open(replyWith(swingEntry(1, "live")));
    click(main, 'button[data-action="stop"]');
    await settle();
    record();
    main = await open(replyWith(swingEntry(1, "promotion_pending")));
    click(main, 'button[data-action="promote"]');
    await settle();
    record();
    const table = SCREEN_PATHS.templates;
    for (const url of seen) {
      const pattern = url.replace(/^\/api\/templates\/\d+\/stop$/, "/api/templates/{id}/stop").replace(/^\/api\/approvals\/[^/]+(\/approve)?$/, "/api/approvals/{id}$1");
      expect(table, url).toContain(pattern);
    }
    expect([...table].sort()).toEqual(
      [
        "/api/session",
        "/api/templates/current",
        "/api/templates",
        "/api/templates/adjust",
        "/api/templates/{id}/stop",
        "/api/approvals/{id}",
        "/api/approvals/{id}/approve",
      ].sort(),
    );
  });

  it("템플릿 main은 메뉴와 화면을 그리고 '준비 중'이 아니다", async () => {
    // 검증: V79 V75 F12
    server = new FakeServer(replyWith(swingEntry(1, "simulating")));
    vi.stubGlobal("fetch", server.fetch);
    vi.resetModules();
    const html = PAGE["../../web/templates/index.html"];
    document.body.innerHTML = new DOMParser().parseFromString(html, "text/html").body.innerHTML;
    await MAIN["../../web/src/templates/main.ts"]();
    await settle();
    const current = [...document.querySelectorAll('#menu nav [aria-current="page"]')];
    expect(current.map((a) => a.textContent?.trim())).toEqual(["템플릿"]);
    expect(document.querySelectorAll("h1").length).toBe(1);
    expect(document.body.textContent).not.toContain("준비 중");
    expect(document.querySelector('[data-role="fields"] tr[data-key="risk_per_trade_pct"]')).not.toBeNull();
  });

  it("템플릿 화면 소스는 three · 사무실 3D에 닿지 않는다", () => {
    // 검증: V77 N21
    const { files, bare } = reach(mainOf("templates"));
    expect(files.has("../../web/src/templates/screen.ts"), "화면 모듈에 닿는다").toBe(true);
    expect(files.has("../../web/src/templates/model.ts"), "화면 모델에 닿는다").toBe(true);
    expect(files.has("../../web/src/ui/parts.ts"), "공통 부품을 쓴다").toBe(true);
    expect([...bare].filter((name) => name === "three" || name.startsWith("three/"))).toEqual([]);
    expect([...files].filter((file) => file.includes("scene3d") || file.includes("/src/office/"))).toEqual([]);
  });
});
