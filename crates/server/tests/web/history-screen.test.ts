// 히스토리 화면(server 29차 2번 · F7-7 · F7-9 · C30 · V83 · V84 · E57)을 jsdom에 띄운다(가짜 fetch).
// 탭 셋(기본은 매매, 누르면 그 판만), 매매 줄을 누르면 그 줄 안에 근거 요약과 상태 흐름이 펼쳐지고 다시 누르면 접힘,
// "더 보기"는 next가 있을 때만 있고 누르면 이어 붙음, 거르기를 바꾸면 처음부터 다시 읽음, 하루 정리의 날짜 고르기,
// 승인 기록의 목록과 거르기, 빈 상태 · 읽기 실패와 다시 시도, 부르는 경로는 표에 있고 3D에 닿지 않는다.
import { afterEach, describe, expect, it, vi } from "vitest";
import { SCREEN_PATHS } from "../../web/src/paths";
import type { ApprovalItem, TradeDetail, TradeItem } from "../../web/src/history/model";
import { timeText } from "../../web/src/history/model";
import { mountHistory } from "../../web/src/history/screen";
import { mainOf, reach } from "./source-graph";
import { approval, daySummary, detailOf, emptyDetailOf, trades } from "./history-fixture";

const PAGE = import.meta.glob<string>("../../web/history/index.html", { query: "?raw", import: "default", eager: true });
const MAIN = import.meta.glob("../../web/src/history/main.ts");

const TODAY = "2026-11-16";

interface Call {
  method: string;
  url: string;
  path: string;
  query: URLSearchParams;
}

/**
 * 가짜 서버: 매매 · 승인 요청을 들고 서버처럼 쪽을 낸다(최근 것부터, `before` · `state` · `status`, 한 쪽은 `max`와
 * `pageSize` 가운데 작은 것, `next`는 더 있을 때만). 하루 정리는 날짜가 없으면 오늘이다.
 */
class FakeServer {
  calls: Call[] = [];
  trades: TradeItem[];
  approvals: ApprovalItem[];
  details = new Map<number, TradeDetail>();
  pageSize = 10;
  failTrades = false;
  failSummary = false;
  failApprovals = false;

  constructor(tradeList: TradeItem[], approvalList: ApprovalItem[] = []) {
    this.trades = tradeList;
    this.approvals = approvalList;
  }

  private page<T extends { id: number }>(all: T[], query: URLSearchParams, keep: (item: T) => boolean) {
    const before = query.get("before");
    const max = query.get("max");
    const size = Math.min(max === null ? this.pageSize : Number(max), this.pageSize);
    const matching = all
      .filter(keep)
      .filter((item) => before === null || item.id < Number(before))
      .sort((a, b) => b.id - a.id);
    const shown = matching.slice(0, size);
    const next = matching.length > shown.length && shown.length > 0 ? shown[shown.length - 1].id : null;
    return { shown, next };
  }

  readonly fetch = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = String(input);
    const method = (init?.method ?? "GET").toUpperCase();
    const parsed = new URL(url, "http://local.test");
    const call = { method, url, path: parsed.pathname, query: parsed.searchParams };
    this.calls.push(call);
    const json = (status: number, value: unknown) =>
      new Response(JSON.stringify(value), { status, headers: { "Content-Type": "application/json" } });
    if (method === "POST" && call.path === "/api/session") return json(200, {});
    if (method !== "GET") return json(404, { error: "not_found" });
    if (call.path === "/api/trades") {
      if (this.failTrades) return json(500, { error: "internal" });
      const state = call.query.get("state");
      const { shown, next } = this.page(this.trades, call.query, (t) => state === null || state === "" || t.state === state);
      return json(200, { trades: shown, next });
    }
    const one = /^\/api\/trades\/(\d+)$/.exec(call.path);
    if (one) {
      const id = Number(one[1]);
      const found = this.trades.find((t) => t.id === id);
      if (!found) return json(404, { error: "not_found" });
      return json(200, this.details.get(id) ?? detailOf(found));
    }
    if (call.path === "/api/daily-summary") {
      if (this.failSummary) return json(500, { error: "internal" });
      return json(200, daySummary(call.query.get("date") ?? TODAY));
    }
    if (call.path === "/api/approvals") {
      if (this.failApprovals) return json(500, { error: "internal" });
      const status = call.query.get("status");
      const { shown, next } = this.page(this.approvals, call.query, (a) => status === null || status === "" || a.status === status);
      return json(200, { approvals: shown, next });
    }
    return json(404, { error: "not_found" });
  });

  /** 그 경로로 온 GET들. */
  gets(path: string): Call[] {
    return this.calls.filter((call) => call.method === "GET" && call.path === path);
  }

  /** 그 경로로 온 마지막 GET. */
  last(path: string): Call {
    const list = this.gets(path);
    expect(list.length, `${path}을 불렀다`).toBeGreaterThan(0);
    return list[list.length - 1];
  }
}

let server: FakeServer;

async function settle(): Promise<void> {
  for (let i = 0; i < 20; i++) await new Promise((resolve) => setTimeout(resolve, 0));
}

/** 본문 자리 하나에 화면을 띄운다. */
async function open(tradeList: TradeItem[], approvalList: ApprovalItem[] = [], prepare?: (fake: FakeServer) => void): Promise<HTMLElement> {
  server = new FakeServer(tradeList, approvalList);
  prepare?.(server);
  vi.stubGlobal("fetch", server.fetch);
  document.body.innerHTML = '<div id="menu"></div><main id="screen"><h1>히스토리</h1></main>';
  const main = document.getElementById("screen") as HTMLElement;
  await mountHistory(main);
  await settle();
  return main;
}

function pick<T extends Element>(root: ParentNode, selector: string): T {
  const found = root.querySelector<T>(selector);
  expect(found, selector).not.toBeNull();
  return found as T;
}

/** 보이는가: 있고, 자기도 조상도 `hidden`이 아니다. */
function visible(node: Element | null): boolean {
  return node !== null && !(node as HTMLElement).hidden && node.closest("[hidden]") === null;
}

function tab(main: HTMLElement, name: string): HTMLElement {
  return pick<HTMLElement>(main, `[role="tablist"] [role="tab"][data-tab="${name}"]`);
}

function panel(main: HTMLElement, name: string): HTMLElement {
  return pick<HTMLElement>(main, `[role="tabpanel"][data-tab="${name}"]`);
}

async function choose(main: HTMLElement, name: string): Promise<HTMLElement> {
  tab(main, name).click();
  await settle();
  return panel(main, name);
}

/** 그 판의 줄 번호들(그려진 차례). */
function shownIds(root: ParentNode, attr: "data-trade" | "data-approval"): number[] {
  return [...root.querySelectorAll(`[${attr}]`)].map((node) => Number(node.getAttribute(attr)));
}

function tradeRow(main: HTMLElement, id: number): HTMLElement {
  return pick<HTMLElement>(panel(main, "trades"), `[data-role="trades"] [data-trade="${id}"]`);
}

async function select(root: ParentNode, selector: string, value: string): Promise<void> {
  const node = pick<HTMLSelectElement>(root, selector);
  node.value = value;
  expect(node.value, `${selector}에 ${value} 칸이 있다`).toBe(value);
  node.dispatchEvent(new Event("change", { bubbles: true }));
  await settle();
}

async function more(root: HTMLElement): Promise<void> {
  pick<HTMLButtonElement>(root, 'button[data-action="more"]').click();
  await settle();
}

/** 번호 하나에서 n개까지 거꾸로. */
function down(from: number, count: number): number[] {
  return Array.from({ length: count }, (_, i) => from - i);
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("히스토리 화면 · 탭", () => {
  it("탭 셋(매매 · 하루 정리 · 승인 기록)이 있고 기본은 매매 판만 보인다", async () => {
    // 검증: V83 C30 F12
    const main = await open(trades(3));
    const tabs = [...main.querySelectorAll<HTMLElement>('[role="tablist"] [role="tab"]')];
    expect(tabs.map((node) => node.dataset.tab)).toEqual(["trades", "summary", "approvals"]);
    expect(tabs.map((node) => node.textContent?.trim())).toEqual(["매매", "하루 정리", "승인 기록"]);
    expect(tabs.map((node) => node.getAttribute("aria-selected"))).toEqual(["true", "false", "false"]);
    for (const node of tabs) {
      const target = node.getAttribute("aria-controls");
      expect(target, "탭은 판을 가리킨다").toBeTruthy();
      const pointed = main.querySelector(`#${target}`);
      expect(pointed?.getAttribute("role")).toBe("tabpanel");
      expect(pointed?.getAttribute("data-tab")).toBe(node.dataset.tab);
    }
    expect(visible(panel(main, "trades"))).toBe(true);
    expect(visible(panel(main, "summary"))).toBe(false);
    expect(visible(panel(main, "approvals"))).toBe(false);
    expect(shownIds(panel(main, "trades"), "data-trade")).toEqual([3, 2, 1]);
  });

  it("탭을 누르면 그 탭이 골라지고 그 판만 보인다", async () => {
    // 검증: V83 V84 C30
    const main = await open(trades(3), [approval(1), approval(2)]);
    const summary = await choose(main, "summary");
    expect(tab(main, "summary").getAttribute("aria-selected")).toBe("true");
    expect(tab(main, "trades").getAttribute("aria-selected")).toBe("false");
    expect(visible(summary)).toBe(true);
    expect(visible(panel(main, "trades"))).toBe(false);
    expect(visible(panel(main, "approvals"))).toBe(false);

    const approvals = await choose(main, "approvals");
    expect(visible(approvals)).toBe(true);
    expect(visible(summary)).toBe(false);
    expect(shownIds(approvals, "data-approval")).toEqual([2, 1]);

    const tradesPanel = await choose(main, "trades");
    expect(visible(tradesPanel)).toBe(true);
    expect(visible(approvals)).toBe(false);
    expect(tab(main, "trades").getAttribute("aria-selected")).toBe("true");
    expect(shownIds(tradesPanel, "data-trade")).toEqual([3, 2, 1]);
  });
});

describe("히스토리 화면 · 매매", () => {
  it("매매 목록은 최근 것부터이고 줄에 종목 · 상태 글 · 만든 시각이 보인다", async () => {
    // 검증: V83 C30
    const list = trades(3, (id) => (id === 2 ? "sold" : "judged"));
    const main = await open(list);
    expect(shownIds(panel(main, "trades"), "data-trade")).toEqual([3, 2, 1]);
    const row = tradeRow(main, 2);
    expect(row.dataset.state).toBe("sold");
    expect(pick(row, '[data-role="state"]').textContent).toContain("팔림");
    expect(pick(row, '[data-role="symbol"]').textContent).toContain(list[1].symbol);
    expect(pick(row, '[data-role="time"]').textContent).toContain(timeText(list[1].created_at));
    expect(pick(tradeRow(main, 3), '[data-role="state"]').textContent).toContain("판단됨");
  });

  it("매매 줄을 누르면 그 줄 안에 근거 요약과 상태 흐름이 펼쳐지고 다시 누르면 접힌다", async () => {
    // 검증: V83 R6-3 F7 C30
    const main = await open(trades(3));
    const toggle = pick<HTMLButtonElement>(tradeRow(main, 2), 'button[data-action="toggle"]');
    expect(toggle.getAttribute("aria-expanded")).toBe("false");
    expect(visible(tradeRow(main, 2).querySelector('[data-role="detail"]'))).toBe(false);
    toggle.click();
    await settle();

    expect(server.gets("/api/trades/2").length).toBe(1);
    const row = tradeRow(main, 2);
    const detail = pick<HTMLElement>(row, '[data-role="detail"]');
    expect(visible(detail)).toBe(true);
    expect(pick(row, 'button[data-action="toggle"]').getAttribute("aria-expanded")).toBe("true");
    const basis = pick<HTMLElement>(detail, '[data-role="basis"]');
    expect([...basis.querySelectorAll<HTMLElement>("[data-key]")].map((line) => line.dataset.key)).toEqual([
      "hypothesis",
      "trigger",
      "entry",
      "stop",
      "target",
      "scores",
      "facts",
    ]);
    expect(pick(basis, '[data-key="hypothesis"]').textContent).toContain("실적 개선");
    expect(pick(basis, '[data-key="trigger"]').textContent).toContain("거래량 증가");
    expect(pick(basis, '[data-key="entry"]').textContent).toContain("100,000원");
    expect(pick(basis, '[data-key="stop"]').textContent).toContain("95,000원");
    expect(pick(basis, '[data-key="target"]').textContent).toContain("110,000원");
    expect(pick(basis, '[data-key="scores"]').textContent).toContain("70점");
    expect(pick(basis, '[data-key="facts"]').textContent).toContain("주요사항보고서(자기주식취득결정)");
    const flow = [...pick<HTMLElement>(detail, '[data-role="flow"]').querySelectorAll<HTMLElement>("[data-state]")];
    expect(flow.map((line) => line.dataset.state)).toEqual(["discussing", "judged", "awaiting_approval"]);
    const created = trades(3)[1].created_at;
    expect(flow[2].textContent).toContain("승인대기");
    expect(flow[2].textContent).toContain(timeText(created + 2_000));
    expect(pick(detail, '[data-role="side"]').textContent).toContain("사기");
    expect(pick(detail, '[data-role="side"]').textContent).toContain("7");
    // 다른 줄은 펼쳐지지 않는다. 근거는 매매 하나와 함께 오므로 근거 경로를 부르지 않는다.
    expect(visible(tradeRow(main, 3).querySelector('[data-role="detail"]'))).toBe(false);
    expect(server.calls.some((call) => call.path.endsWith("/basis"))).toBe(false);

    pick<HTMLButtonElement>(tradeRow(main, 2), 'button[data-action="toggle"]').click();
    await settle();
    expect(visible(tradeRow(main, 2).querySelector('[data-role="detail"]'))).toBe(false);
    expect(pick(tradeRow(main, 2), 'button[data-action="toggle"]').getAttribute("aria-expanded")).toBe("false");
  });

  it("판단 전 매매를 펼치면 근거 칸이 없음으로 보인다", async () => {
    // 검증: V83 F7
    const list = trades(2, () => "discussing");
    const main = await open(list, [], (fake) => fake.details.set(1, emptyDetailOf(list[0])));
    pick<HTMLButtonElement>(tradeRow(main, 1), 'button[data-action="toggle"]').click();
    await settle();
    const basis = pick<HTMLElement>(tradeRow(main, 1), '[data-role="detail"] [data-role="basis"]');
    expect(pick(basis, '[data-key="hypothesis"]').textContent).toContain("없음");
    expect(pick(basis, '[data-key="entry"]').textContent).toContain("없음");
    const flow = tradeRow(main, 1).querySelectorAll('[data-role="flow"] [data-state]');
    expect(flow.length).toBe(1);
  });

  it("'더 보기'는 다음 쪽이 있을 때만 있고 누르면 다음 쪽을 이어 붙인다", async () => {
    // 검증: V83 C30
    const main = await open(trades(25));
    const tradesPanel = panel(main, "trades");
    expect(shownIds(tradesPanel, "data-trade")).toEqual(down(25, 10));
    expect(server.last("/api/trades").query.get("before")).toBeNull();

    await more(tradesPanel);
    expect(server.last("/api/trades").query.get("before")).toBe("16");
    expect(shownIds(tradesPanel, "data-trade")).toEqual(down(25, 20));

    await more(tradesPanel);
    expect(server.last("/api/trades").query.get("before")).toBe("6");
    expect(shownIds(tradesPanel, "data-trade")).toEqual(down(25, 25));
    expect(tradesPanel.querySelector('button[data-action="more"]')).toBeNull();

    const small = await open(trades(3));
    expect(panel(small, "trades").querySelector('button[data-action="more"]')).toBeNull();
  });

  it("상태 거르기를 바꾸면 처음 쪽부터 다시 읽고, 더 보기에도 같은 거르기를 보낸다", async () => {
    // 검증: V83 C30
    const main = await open(trades(30, (id) => (id % 3 === 0 ? "sold" : "judged")));
    const tradesPanel = panel(main, "trades");
    await more(tradesPanel);
    expect(shownIds(tradesPanel, "data-trade").length).toBe(20);

    await select(tradesPanel, 'select[data-role="trade-state"]', "sold");
    const filtered = server.last("/api/trades");
    expect(filtered.query.get("state")).toBe("sold");
    expect(filtered.query.get("before")).toBeNull();
    expect(shownIds(tradesPanel, "data-trade")).toEqual([30, 27, 24, 21, 18, 15, 12, 9, 6, 3]);
    for (const row of tradesPanel.querySelectorAll<HTMLElement>("[data-trade]")) expect(row.dataset.state).toBe("sold");
    expect(tradesPanel.querySelector('button[data-action="more"]')).toBeNull();

    await select(tradesPanel, 'select[data-role="trade-state"]', "judged");
    expect(shownIds(tradesPanel, "data-trade").length).toBe(10);
    await more(tradesPanel);
    const next = server.last("/api/trades");
    expect(next.query.get("state")).toBe("judged");
    expect(next.query.get("before")).not.toBeNull();

    await select(tradesPanel, 'select[data-role="trade-state"]', "");
    const all = server.last("/api/trades");
    expect(all.query.get("state") ?? "").toBe("");
    expect(all.query.get("before")).toBeNull();
    expect(shownIds(tradesPanel, "data-trade")).toEqual(down(30, 10));
  });

  it("매매가 없으면 빈 상태가 보이고 더 보기는 없다", async () => {
    // 검증: V83 C30
    const main = await open([]);
    const tradesPanel = panel(main, "trades");
    expect(tradesPanel.querySelector('[data-role="empty"]')).not.toBeNull();
    expect(tradesPanel.querySelectorAll("[data-trade]").length).toBe(0);
    expect(tradesPanel.querySelector('button[data-action="more"]')).toBeNull();
  });

  it("매매 목록을 못 읽으면 '불러오지 못함 · 다시 시도'가 보이고 다시 시도하면 그린다", async () => {
    // 검증: E57 C30
    const main = await open(trades(3), [], (fake) => {
      fake.failTrades = true;
    });
    const tradesPanel = panel(main, "trades");
    expect(pick(tradesPanel, '[role="alert"]').textContent).toContain("불러오지 못함");
    expect(main.querySelector('[role="tablist"]')).not.toBeNull();
    server.failTrades = false;
    pick<HTMLButtonElement>(tradesPanel, 'button[data-action="retry"]').click();
    await settle();
    expect(tradesPanel.querySelector('[role="alert"]')).toBeNull();
    expect(shownIds(tradesPanel, "data-trade")).toEqual([3, 2, 1]);
  });
});

describe("히스토리 화면 · 하루 정리", () => {
  it("하루 정리 탭을 처음 고르면 오늘의 한 일 · 손익 · 복기가 보이고 날짜 칸은 그 날짜다", async () => {
    // 검증: V84 R4-3 F7 C30
    const main = await open(trades(1));
    const summary = await choose(main, "summary");
    const call = server.last("/api/daily-summary");
    expect(call.query.get("date")).toBeNull();
    const today = daySummary(TODAY);
    expect(pick(summary, '[data-role="summary-date"]').textContent).toContain(TODAY);
    expect(pick(summary, '[data-part="done"]').textContent).toContain(today.done);
    expect(pick(summary, '[data-part="pnl"]').textContent).toContain(today.pnl);
    expect(pick(summary, '[data-part="review"]').textContent).toContain(today.review);
    expect(pick<HTMLInputElement>(summary, 'input[type="date"][data-role="date"]').value).toBe(TODAY);
  });

  it("날짜를 고르면 그 날짜로 읽어 그날의 정리를 보인다", async () => {
    // 검증: V84 C30 F7
    const main = await open(trades(1));
    const summary = await choose(main, "summary");
    const input = pick<HTMLInputElement>(summary, 'input[type="date"][data-role="date"]');
    input.value = "2026-11-13";
    input.dispatchEvent(new Event("change", { bubbles: true }));
    await settle();
    expect(server.last("/api/daily-summary").query.get("date")).toBe("2026-11-13");
    const friday = daySummary("2026-11-13");
    expect(pick(summary, '[data-role="summary-date"]').textContent).toContain("2026-11-13");
    expect(pick(summary, '[data-part="done"]').textContent).toContain(friday.done);
    expect(pick(summary, '[data-part="pnl"]').textContent).toContain(friday.pnl);
    expect(pick(summary, '[data-part="review"]').textContent).toContain(friday.review);
    expect(summary.textContent).not.toContain(daySummary(TODAY).done);
  });

  it("하루 정리를 못 읽으면 그 판에 오류 줄이 보이고 다시 시도하면 그린다", async () => {
    // 검증: E57 V84
    const main = await open(trades(1), [], (fake) => {
      fake.failSummary = true;
    });
    const summary = await choose(main, "summary");
    expect(pick(summary, '[role="alert"]').textContent).toContain("불러오지 못함");
    expect(panel(main, "trades").querySelector('[role="alert"]')).toBeNull();
    server.failSummary = false;
    pick<HTMLButtonElement>(summary, 'button[data-action="retry"]').click();
    await settle();
    expect(summary.querySelector('[role="alert"]')).toBeNull();
    expect(pick(summary, '[data-part="done"]').textContent).toContain(daySummary(TODAY).done);
  });
});

describe("히스토리 화면 · 승인 기록", () => {
  it("승인 기록은 최근 것부터이고 줄에 승인 번호 · 결과 글 · 가린 요약 · 시각이 보인다", async () => {
    // 검증: V84 C30
    const list = [approval(1, "approved"), approval(2, "rejected"), approval(3)];
    const main = await open(trades(1), list);
    const approvals = await choose(main, "approvals");
    expect(server.last("/api/approvals").query.get("before")).toBeNull();
    expect(shownIds(approvals, "data-approval")).toEqual([3, 2, 1]);
    const row = pick<HTMLElement>(approvals, '[data-role="approvals"] [data-approval="2"]');
    expect(row.dataset.status).toBe("rejected");
    expect(pick(row, '[data-role="status"]').textContent).toContain("거절");
    expect(pick(row, '[data-role="number"]').textContent).toContain("A-0002");
    expect(pick(row, '[data-role="summary"]').textContent).toContain(list[1].summary);
    expect(pick(row, '[data-role="time"]').textContent).toContain(timeText(list[1].created_at));
    expect(pick(approvals, '[data-approval="3"] [data-role="status"]').textContent).toContain("기다림");
  });

  it("승인 기록을 상태로 거르면 처음 쪽부터 다시 읽고, 더 보기로 이어 붙인다", async () => {
    // 검증: V84 C30
    const list = Array.from({ length: 24 }, (_, i) => approval(i + 1, (i + 1) % 2 === 0 ? "rejected" : "pending"));
    const main = await open(trades(1), list);
    const approvals = await choose(main, "approvals");
    expect(shownIds(approvals, "data-approval")).toEqual(down(24, 10));
    await more(approvals);
    expect(server.last("/api/approvals").query.get("before")).toBe("15");
    expect(shownIds(approvals, "data-approval")).toEqual(down(24, 20));

    await select(approvals, 'select[data-role="approval-status"]', "rejected");
    const filtered = server.last("/api/approvals");
    expect(filtered.query.get("status")).toBe("rejected");
    expect(filtered.query.get("before")).toBeNull();
    expect(shownIds(approvals, "data-approval")).toEqual([24, 22, 20, 18, 16, 14, 12, 10, 8, 6]);
    await more(approvals);
    const next = server.last("/api/approvals");
    expect(next.query.get("status")).toBe("rejected");
    expect(next.query.get("before")).toBe("6");
    expect(shownIds(approvals, "data-approval")).toEqual([24, 22, 20, 18, 16, 14, 12, 10, 8, 6, 4, 2]);
    expect(approvals.querySelector('button[data-action="more"]')).toBeNull();
  });

  it("승인 요청이 없으면 빈 상태, 못 읽으면 오류 줄과 다시 시도", async () => {
    // 검증: V84 E57
    const empty = await open(trades(1), []);
    const emptyPanel = await choose(empty, "approvals");
    expect(emptyPanel.querySelector('[data-role="empty"]')).not.toBeNull();
    expect(emptyPanel.querySelectorAll("[data-approval]").length).toBe(0);

    const main = await open(trades(1), [approval(1)], (fake) => {
      fake.failApprovals = true;
    });
    const approvals = await choose(main, "approvals");
    expect(pick(approvals, '[role="alert"]').textContent).toContain("불러오지 못함");
    server.failApprovals = false;
    pick<HTMLButtonElement>(approvals, 'button[data-action="retry"]').click();
    await settle();
    expect(approvals.querySelector('[role="alert"]')).toBeNull();
    expect(shownIds(approvals, "data-approval")).toEqual([1]);
  });
});

describe("히스토리 화면 · 경로 · 가벼움", () => {
  it("화면이 부르는 경로는 모두 히스토리 화면의 경로 표에 있다(번호 자리는 {id})", async () => {
    // 검증: D11 D2 C30
    const main = await open(trades(25), Array.from({ length: 12 }, (_, i) => approval(i + 1)));
    pick<HTMLButtonElement>(tradeRow(main, 25), 'button[data-action="toggle"]').click();
    await settle();
    await more(panel(main, "trades"));
    await select(panel(main, "trades"), 'select[data-role="trade-state"]', "judged");
    const summary = await choose(main, "summary");
    const input = pick<HTMLInputElement>(summary, 'input[type="date"][data-role="date"]');
    input.value = "2026-11-13";
    input.dispatchEvent(new Event("change", { bubbles: true }));
    await settle();
    const approvals = await choose(main, "approvals");
    await more(approvals);
    await select(approvals, 'select[data-role="approval-status"]', "pending");

    const seen = new Set(server.calls.map((call) => call.path.replace(/^\/api\/trades\/\d+$/, "/api/trades/{id}")));
    const table = SCREEN_PATHS.history;
    for (const path of seen) expect(table, path).toContain(path);
    for (const path of ["/api/trades", "/api/trades/{id}", "/api/daily-summary", "/api/approvals"]) {
      expect(seen.has(path), `${path}을 부른다`).toBe(true);
    }
    expect(server.calls.every((call) => !call.url.includes("://"))).toBe(true);
  });

  it("히스토리 main은 메뉴와 화면을 그리고 '준비 중'이 아니다", async () => {
    // 검증: V83 V75 F12
    server = new FakeServer(trades(2));
    vi.stubGlobal("fetch", server.fetch);
    vi.resetModules();
    const html = PAGE["../../web/history/index.html"];
    document.body.innerHTML = new DOMParser().parseFromString(html, "text/html").body.innerHTML;
    await MAIN["../../web/src/history/main.ts"]();
    await settle();
    const current = [...document.querySelectorAll('#menu nav [aria-current="page"]')];
    expect(current.map((a) => a.textContent?.trim())).toEqual(["히스토리"]);
    expect(document.querySelectorAll("h1").length).toBe(1);
    expect(document.body.textContent).not.toContain("준비 중");
    expect(document.querySelector('[role="tablist"]')).not.toBeNull();
    expect(document.querySelector('[data-role="trades"] [data-trade="2"]')).not.toBeNull();
  });

  it("히스토리 화면 소스는 three · 사무실 3D에 닿지 않고 공통 부품을 쓴다", () => {
    // 검증: V77 N21
    const { files, bare } = reach(mainOf("history"));
    expect(files.has("../../web/src/history/screen.ts"), "화면 모듈에 닿는다").toBe(true);
    expect(files.has("../../web/src/history/model.ts"), "화면 모델에 닿는다").toBe(true);
    expect(files.has("../../web/src/history/view.ts"), "그리기 모듈에 닿는다").toBe(true);
    expect(files.has("../../web/src/ui/parts.ts"), "공통 부품을 쓴다").toBe(true);
    expect([...bare].filter((name) => name === "three" || name.startsWith("three/"))).toEqual([]);
    expect([...files].filter((file) => file.includes("scene3d") || file.includes("/src/office/"))).toEqual([]);
  });
});
