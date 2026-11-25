// 규칙 화면 · 금지 종목 · 업종표 묶음(server 35차 3번 · 결정 19번 · P5 · P8 · R7-3 · F5)을 jsdom에 띄운다(가짜 fetch).
// 지금 값 표 · 업종 없는 종목 경고, 그 컴퓨터면 줄 더하기 · 지우기 · 저장(요청 몸통 모양), 원격이면 보기만,
// 저장 실패 오류 줄(고치던 줄은 남음), 이 묶음만 못 읽으면 다른 묶음은 그대로, 부르는 경로는 표에 있다.
import { afterEach, describe, expect, it, vi } from "vitest";
import { SCREEN_PATHS } from "../../web/src/paths";
import type { RulesConfigReply } from "../../web/src/rules/config-model";
import { mountRules } from "../../web/src/rules/screen";
import { proposal, proposalsOf, rulesWith } from "./rules-fixture";

interface Call {
  method: string;
  url: string;
  body: unknown;
}

const CONFIG = "/api/rules/config";

function config(more: Partial<RulesConfigReply> = {}): RulesConfigReply {
  return {
    banned: [{ market: "us", symbol: "TSLA" }],
    sectors: [{ symbol: "000660", sector: "반도체" }],
    editable: true,
    unsectored: ["005930"],
    ...more,
  };
}

/** 가짜 서버: 규칙 · 개선안 목록 · 금지 종목 · 업종표. 저장은 받은 값으로 바꿔 읽기와 같은 모양으로 답한다. */
class FakeServer {
  calls: Call[] = [];
  config: RulesConfigReply;
  failConfigRead = false;
  saveOverride: { status: number; body: unknown } | null = null;
  offlineSave = false;

  constructor(value: RulesConfigReply) {
    this.config = value;
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
    if (method === "GET" && url === "/api/rules") return json(200, rulesWith("1"));
    if (method === "GET" && url === "/api/proposals") return json(200, proposalsOf(proposal(1, "proposed", "1", "0.5")));
    if (method === "GET" && url === CONFIG) {
      return this.failConfigRead ? json(500, { error: "internal" }) : json(200, this.config);
    }
    if (method === "POST" && url === CONFIG) {
      if (this.offlineSave) throw new TypeError("network down");
      if (this.saveOverride) return json(this.saveOverride.status, this.saveOverride.body);
      const sent = body as { banned: RulesConfigReply["banned"]; sectors: RulesConfigReply["sectors"] };
      const sectors = [...sent.sectors].sort((a, b) => a.symbol.localeCompare(b.symbol));
      const known = new Set(sectors.map((row) => row.symbol));
      this.config = {
        banned: sent.banned,
        sectors,
        editable: this.config.editable,
        unsectored: ["005930"].filter((symbol) => !known.has(symbol)),
      };
      return json(200, this.config);
    }
    return json(404, { error: "not_found" });
  });

  sent(method: string, url: string): Call[] {
    return this.calls.filter((call) => call.method === method && call.url === url);
  }
}

let server: FakeServer;

async function settle(): Promise<void> {
  for (let i = 0; i < 20; i++) await new Promise((resolve) => setTimeout(resolve, 0));
}

async function open(value: RulesConfigReply, prepare?: (fake: FakeServer) => void): Promise<HTMLElement> {
  server = new FakeServer(value);
  prepare?.(server);
  vi.stubGlobal("fetch", server.fetch);
  document.body.textContent = "";
  const menu = document.createElement("div");
  menu.id = "menu";
  const main = document.createElement("main");
  main.id = "screen";
  const title = document.createElement("h1");
  title.textContent = "규칙";
  main.append(title);
  document.body.append(menu, main);
  await mountRules(main);
  await settle();
  return main;
}

function pick<T extends Element>(root: ParentNode, selector: string): T {
  const found = root.querySelector<T>(selector);
  expect(found, selector).not.toBeNull();
  return found as T;
}

function group(main: HTMLElement): HTMLElement {
  return pick<HTMLElement>(main, '[data-role="rules-config"]');
}

function type(root: ParentNode, selector: string, value: string): void {
  const node = pick<HTMLInputElement | HTMLSelectElement>(root, selector);
  node.value = value;
  node.dispatchEvent(new Event("input", { bubbles: true }));
  node.dispatchEvent(new Event("change", { bubbles: true }));
}

async function click(root: ParentNode, selector: string): Promise<void> {
  pick<HTMLElement>(root, selector).click();
  await settle();
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("규칙 화면 · 금지 종목 · 업종표 · 보기", () => {
  it("금지 종목과 업종표를 표로 그리고 다른 묶음도 그대로 그린다", async () => {
    // 검증: R7-3 F5 V82 P5 P8
    const main = await open(config({ banned: [{ market: "us", symbol: "TSLA" }, { market: "kr", symbol: "000270" }] }));
    const box = group(main);
    expect(box.textContent).toContain("금지 종목");
    expect(box.textContent).toContain("업종표");
    expect(pick(box, '[data-role="banned"] tr[data-banned="us:TSLA"]').textContent).toContain("TSLA");
    expect(pick(box, '[data-role="banned"] tr[data-banned="kr:000270"]').textContent).toContain("000270");
    const sector = pick(box, '[data-role="sectors"] tr[data-sector="000660"]');
    expect(sector.textContent).toContain("반도체");
    expect(main.querySelector('[data-role="rules"] tr[data-key="risk_per_trade_pct"]')).not.toBeNull();
    expect(main.querySelector('[data-role="proposals"] [data-proposal="1"]')).not.toBeNull();
    expect(server.sent("GET", CONFIG).length).toBe(1);
  });

  it("업종 없는 지켜보는 종목이 있으면 경고 줄이 보이고, 없으면 보이지 않는다", async () => {
    // 검증: P8 R7-3 V82
    let main = await open(config({ unsectored: ["005930", "035720"] }));
    expect(pick(group(main), '[data-role="unsectored"]').textContent).toContain(
      "업종이 없어 살 수 없는 종목: 005930, 035720",
    );
    main = await open(config({ unsectored: [] }));
    expect(group(main).querySelector('[data-role="unsectored"]')).toBeNull();
  });

  it("비었으면 빈 상태 글이 보인다", async () => {
    // 검증: R7-3 V82
    const main = await open(config({ banned: [], sectors: [] }));
    expect(group(main).querySelector('[data-role="no-banned"]')).not.toBeNull();
    expect(group(main).querySelector('[data-role="no-sectors"]')).not.toBeNull();
  });

  it("원격이면 보기만 하고 '이 컴퓨터에서만 바꿀 수 있습니다'가 보인다", async () => {
    // 검증: R7-3 C3 C16 M6
    const main = await open(config({ editable: false }));
    const box = group(main);
    expect(pick(box, '[data-role="config-readonly"]').textContent).toContain("이 컴퓨터에서만 바꿀 수 있습니다");
    expect(pick(box, '[data-role="sectors"] tr[data-sector="000660"]').textContent).toContain("반도체");
    for (const action of ["add-banned", "remove-banned", "add-sector", "remove-sector", "save-config"]) {
      expect(box.querySelector(`button[data-action="${action}"]`), action).toBeNull();
    }
    expect(box.querySelector("input"), "입력 칸이 없다").toBeNull();
  });

  it("이 묶음만 읽지 못하면 묶음 안에 못 읽음이 보이고 다른 묶음은 그대로다", async () => {
    // 검증: E57 R7-3 C29
    const main = await open(config(), (fake) => {
      fake.failConfigRead = true;
    });
    expect(pick(group(main), '[data-role="config-load-failed"]').textContent).toContain("불러오지 못");
    expect(main.querySelector('[data-role="rules"] tr[data-key="risk_per_trade_pct"]')).not.toBeNull();
    expect(main.querySelector('[data-role="proposals"] [data-proposal="1"]')).not.toBeNull();
  });
});

describe("규칙 화면 · 금지 종목 · 업종표 · 고치기", () => {
  it("줄을 더하고 지운 뒤 저장하면 두 칸뿐인 몸통을 보내고 답한 값으로 다시 그린다", async () => {
    // 검증: R7-3 F5 P5 P8 D2
    const main = await open(config());
    let box = group(main);
    type(box, 'select[name="banned-market"]', "kr");
    type(box, 'input[name="banned-symbol"]', " 000270 ");
    await click(box, 'button[data-action="add-banned"]');
    box = group(main);
    expect(box.querySelector('tr[data-banned="kr:000270"]')).not.toBeNull();
    type(box, 'input[name="sector-symbol"]', "005930");
    type(box, 'input[name="sector-name"]', "  전자  ");
    await click(box, 'button[data-action="add-sector"]');
    box = group(main);
    expect(pick(box, 'tr[data-sector="005930"]').textContent).toContain("전자");
    await click(box, 'tr[data-banned="us:TSLA"] button[data-action="remove-banned"]');
    box = group(main);
    expect(box.querySelector('tr[data-banned="us:TSLA"]')).toBeNull();
    expect(server.sent("POST", CONFIG).length, "저장 전에는 보내지 않는다").toBe(0);

    await click(box, 'button[data-action="save-config"]');
    const saves = server.sent("POST", CONFIG);
    expect(saves.length).toBe(1);
    expect(saves[0].body).toEqual({
      banned: [{ market: "kr", symbol: "000270" }],
      sectors: [
        { symbol: "000660", sector: "반도체" },
        { symbol: "005930", sector: "전자" },
      ],
    });
    box = group(main);
    expect(pick(box, '[data-role="config-result"]').textContent).toContain("저장했습니다");
    expect(box.querySelector('[data-role="unsectored"]'), "답한 값의 경고로 다시 그린다").toBeNull();
    expect(box.querySelector('[data-role="config-error"]')).toBeNull();
  });

  it("업종표 줄을 지우고 저장하면 그 줄이 빠진 몸통을 보낸다", async () => {
    // 검증: R7-3 F5 P8
    const main = await open(config());
    await click(group(main), 'tr[data-sector="000660"] button[data-action="remove-sector"]');
    await click(group(main), 'button[data-action="save-config"]');
    expect(server.sent("POST", CONFIG)[0].body).toEqual({ banned: [{ market: "us", symbol: "TSLA" }], sectors: [] });
  });

  it("틀린 줄은 더하지 않고 까닭을 보이며 요청을 보내지 않는다", async () => {
    // 검증: R7-3 E4 P5 P8
    const main = await open(config());
    let box = group(main);
    type(box, 'input[name="banned-symbol"]', "삼성전자");
    await click(box, 'button[data-action="add-banned"]');
    box = group(main);
    expect(pick(box, '[data-role="config-problem"]').textContent?.trim().length).toBeGreaterThan(0);
    type(box, 'input[name="sector-symbol"]', "000660");
    type(box, 'input[name="sector-name"]', "전자");
    await click(box, 'button[data-action="add-sector"]');
    box = group(main);
    expect(pick(box, 'tr[data-sector="000660"]').textContent).toContain("반도체");
    expect(box.querySelectorAll('[data-role="sectors"] tr[data-sector]').length).toBe(1);
    expect(server.sent("POST", CONFIG).length).toBe(0);
  });

  for (const [label, prepare, expected] of [
    ["503", (fake: FakeServer) => (fake.saveOverride = { status: 503, body: { error: "unavailable" } }), "다시 시도"],
    ["400", (fake: FakeServer) => (fake.saveOverride = { status: 400, body: { error: "invalid_input" } }), "올바르지 않"],
    ["403", (fake: FakeServer) => (fake.saveOverride = { status: 403, body: { error: "loopback_only" } }), "이 컴퓨터에서만"],
    ["연결 실패", (fake: FakeServer) => (fake.offlineSave = true), "다시 시도"],
  ] as const) {
    it(`저장이 실패(${label})하면 오류 줄이 보이고 고치던 줄은 그대로 남는다`, async () => {
      // 검증: R7-3 E57 F5
      const main = await open(config(), prepare);
      let box = group(main);
      type(box, 'input[name="sector-symbol"]', "005930");
      type(box, 'input[name="sector-name"]', "전자");
      await click(box, 'button[data-action="add-sector"]');
      await click(group(main), 'button[data-action="save-config"]');
      box = group(main);
      const error = pick(box, '[data-role="config-error"]');
      expect(error.getAttribute("role")).toBe("alert");
      expect(error.textContent).toContain(expected);
      expect(box.querySelector('tr[data-sector="005930"]'), "고치던 줄은 남는다").not.toBeNull();
      expect(box.querySelector('[data-role="config-result"]')).toBeNull();
      expect(pick<HTMLButtonElement>(box, 'button[data-action="save-config"]').disabled).toBe(false);
    });
  }

  it("화면이 부르는 경로는 모두 규칙 화면의 경로 표에 있다", async () => {
    // 검증: D11 D2 C29
    const main = await open(config());
    await click(group(main), 'tr[data-sector="000660"] button[data-action="remove-sector"]');
    await click(group(main), 'button[data-action="save-config"]');
    const table = SCREEN_PATHS.rules;
    for (const call of server.calls) expect(table, call.url).toContain(call.url);
    expect(table).toContain(CONFIG);
  });
});
