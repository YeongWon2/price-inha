// 규칙 화면(server 28차 2번 · F5 · C29 · V82 · E28 · E35 · E57)을 jsdom에 띄운다(가짜 fetch).
// 규칙 표 · 절대 상한 · 바뀜 · 냉각중 표시, 개선안 줄(상태 · 실수 · 앞 → 뒤 · 위험), 제안됨 줄에만 승인 · 거절 단추,
// 누르면 번호 · 지문을 그대로 보내고 결과를 그 줄에 보이며 목록을 다시 읽는다, 읽기 실패와 다시 시도,
// 부르는 경로는 표에 있고, 3D에 닿지 않는다.
import { afterEach, describe, expect, it, vi } from "vitest";
import { SCREEN_PATHS } from "../../web/src/paths";
import type { ProposalView, ProposalsReply, RulesReply } from "../../web/src/rules/model";
import { mountRules } from "../../web/src/rules/screen";
import { mainOf, reach } from "./source-graph";
import { noTemplate, proposal, proposalsOf, riskChange, rulesWith } from "./rules-fixture";

const PAGE = import.meta.glob<string>("../../web/rules/index.html", { query: "?raw", import: "default", eager: true });
const MAIN = import.meta.glob("../../web/src/rules/main.ts");

interface Call {
  method: string;
  url: string;
  body: unknown;
}

/**
 * 가짜 서버: 규칙 읽기 · 개선안 목록을 들고 있다. 답하기는 지금 서버처럼 상태를 옮긴다(승인: 위험을 키우지 않으면
 * 반영 · 키우면 냉각중, 거절: 거절됨). `answerOverride`가 있으면 그 답을 그대로 돌려주고 아무것도 옮기지 않는다.
 */
class FakeServer {
  calls: Call[] = [];
  rules: RulesReply;
  proposals: ProposalsReply;
  failRules = false;
  failProposals = false;
  answerOverride: { status: number; body: unknown } | null = null;

  constructor(rules: RulesReply, proposals: ProposalsReply) {
    this.rules = rules;
    this.proposals = proposals;
  }

  private answer(body: { id: number; number: string; fingerprint: string; answer: string }): { status: number; body: unknown } {
    const found = this.proposals.proposals.find((p) => p.id === body.id);
    if (!found || found.state !== "proposed") return { status: 200, body: { result: "rejected", reason: "already_resolved" } };
    if (body.number !== found.number || body.fingerprint !== found.fingerprint) {
      return { status: 200, body: { result: "rejected", reason: "fingerprint_mismatch" } };
    }
    const move = (state: ProposalView["state"]) => {
      this.proposals = {
        proposals: this.proposals.proposals.map((p): ProposalView => (p.id === found.id ? { ...p, state } : p)),
      };
    };
    if (body.answer !== "approve") {
      move("rejected");
      return { status: 200, body: { result: "rejected", reason: "user_rejected" } };
    }
    if (found.increases_risk) {
      move("cooling");
      this.rules = {
        ...this.rules,
        cooling: [...this.rules.cooling, { proposal: found.id, changes: found.changes, applies_on: "next_trading_day" }],
      };
      return { status: 200, body: { result: "cooling", reason: null } };
    }
    move("applied");
    const after = found.changes[0].after;
    this.rules = {
      ...this.rules,
      fields: this.rules.fields.map((f) => (f.key === "risk_per_trade_pct" ? { ...f, value: after } : f)),
      recent: [{ proposal: found.id, applied_at: 1_700_000_000_000, changes: found.changes }, ...this.rules.recent],
    };
    return { status: 200, body: { result: "applied", reason: null } };
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
    if (method === "GET" && url === "/api/rules") return this.failRules ? json(500, { error: "internal" }) : json(200, this.rules);
    if (method === "GET" && url === "/api/proposals") {
      return this.failProposals ? json(500, { error: "internal" }) : json(200, this.proposals);
    }
    if (method === "POST" && url === "/api/proposals/answer") {
      const reply = this.answerOverride ?? this.answer(body as { id: number; number: string; fingerprint: string; answer: string });
      return json(reply.status, reply.body);
    }
    return json(404, { error: "not_found" });
  });

  /** 그 방법 · 경로로 온 요청들. */
  sent(method: string, url: string): Call[] {
    return this.calls.filter((call) => call.method === method && call.url === url);
  }
}

let server: FakeServer;

async function settle(): Promise<void> {
  for (let i = 0; i < 20; i++) await new Promise((resolve) => setTimeout(resolve, 0));
}

/** 본문 자리 하나에 화면을 띄운다. */
async function open(rules: RulesReply, proposals: ProposalsReply, prepare?: (fake: FakeServer) => void): Promise<HTMLElement> {
  server = new FakeServer(rules, proposals);
  prepare?.(server);
  vi.stubGlobal("fetch", server.fetch);
  document.body.innerHTML = '<div id="menu"></div><main id="screen"><h1>규칙</h1></main>';
  const main = document.getElementById("screen") as HTMLElement;
  await mountRules(main);
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

/** 개선안 줄 하나. */
function row(main: HTMLElement, id: number): HTMLElement {
  return pick<HTMLElement>(main, `[data-role="proposals"] [data-proposal="${id}"]`);
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("규칙 화면 · 지금 규칙", () => {
  it("규칙 표에 칸마다 지금 값과 상한이 보이고 절대 상한 목록이 있다", async () => {
    // 검증: V82 C29 R7-3
    const main = await open(rulesWith("1"), proposalsOf());
    const risk = pick<HTMLElement>(main, '[data-role="rules"] tr[data-key="risk_per_trade_pct"]');
    expect(pick(risk, '[data-role="value"]').textContent).toContain("1%");
    expect(pick(risk, '[data-role="cap"]').textContent).toContain("상한 2%");
    expect(risk.dataset.changed).not.toBe("true");
    expect(risk.querySelector('[data-role="changed"]')).toBeNull();
    const positions = pick<HTMLElement>(main, '[data-role="rules"] tr[data-key="max_positions"]');
    expect(pick(positions, '[data-role="value"]').textContent).toContain("5");
    expect(pick(main, '[data-role="caps"] [data-key="sector_weight_pct"]').textContent).toContain("상한 50%");
    expect(main.querySelectorAll('[data-role="caps"] [data-key]').length).toBe(6);
  });

  it("최근에 반영된 개선안이 바꾼 칸에는 '바뀜'과 앞 값이 보인다", async () => {
    // 검증: V82 F5
    const recent = [{ proposal: 3, applied_at: 1_700_000_000_000, changes: [riskChange("1", "0.5", false)] }];
    const main = await open(rulesWith("0.5", { recent }), proposalsOf(proposal(3, "applied", "1", "0.5")));
    const risk = pick<HTMLElement>(main, '[data-role="rules"] tr[data-key="risk_per_trade_pct"]');
    expect(pick(risk, '[data-role="value"]').textContent).toContain("0.5%");
    expect(risk.dataset.changed).toBe("true");
    const mark = pick(risk, '[data-role="changed"]').textContent ?? "";
    expect(mark).toContain("바뀜");
    expect(mark).toContain("1%");
    const daily = pick<HTMLElement>(main, '[data-role="rules"] tr[data-key="daily_loss_limit_pct"]');
    expect(daily.querySelector('[data-role="changed"]')).toBeNull();
  });

  it("냉각중 개선안이 걸린 칸에는 다음 거래일부터의 값이 보인다", async () => {
    // 검증: V82 E28 C29
    const cooling = [{ proposal: 4, changes: [riskChange("1", "1.5", true)], applies_on: "next_trading_day" as const }];
    const main = await open(rulesWith("1", { cooling }), proposalsOf(proposal(4, "cooling", "1", "1.5")));
    const risk = pick<HTMLElement>(main, '[data-role="rules"] tr[data-key="risk_per_trade_pct"]');
    expect(pick(risk, '[data-role="value"]').textContent).toContain("1%");
    const note = pick(risk, '[data-role="cooling"]').textContent ?? "";
    expect(note).toContain("다음 거래일부터");
    expect(note).toContain("1.5%");
  });

  it("템플릿이 없으면 규칙 표 자리에 빈 상태가 보이고 절대 상한은 그대로 보인다", async () => {
    // 검증: V82 C29
    const main = await open(noTemplate(), proposalsOf());
    expect(main.querySelector('[data-role="no-template"]')).not.toBeNull();
    expect(main.querySelector('[data-role="rules"] tr[data-key]')).toBeNull();
    expect(main.querySelectorAll('[data-role="caps"] [data-key]').length).toBe(6);
    expect(main.querySelector('[data-role="no-proposals"]')).not.toBeNull();
  });
});

describe("규칙 화면 · 개선안 목록", () => {
  it("개선안 줄에 상태 · 실수 종류 · 앞 → 뒤 값 · 위험을 키우는가가 보인다", async () => {
    // 검증: V82 F5 E28
    const main = await open(
      rulesWith("1"),
      proposalsOf(proposal(1, "applied", "1", "0.5"), proposal(2, "proposed", "1", "1.5"), proposal(3, "rejected", "1", "0.8")),
    );
    const risky = row(main, 2);
    expect(risky.dataset.state).toBe("proposed");
    expect(pick(risky, '[data-role="state"]').textContent).toContain("제안됨");
    expect(pick(risky, '[data-role="mistake"]').textContent).toContain("손절이 늦음");
    expect(pick<HTMLElement>(risky, '[data-role="risk"]').dataset.risky).toBe("true");
    expect(pick(risky, '[data-role="risk"]').textContent).toContain("위험을 키움");
    const change = pick<HTMLElement>(risky, '[data-role="change"][data-key="risk_per_trade_pct"]');
    expect(change.textContent).toMatch(/1%.*→.*1\.5%/);
    expect(change.dataset.widened).toBe("true");

    expect(pick(row(main, 1), '[data-role="state"]').textContent).toContain("반영됨");
    expect(pick<HTMLElement>(row(main, 1), '[data-role="risk"]').dataset.risky).toBe("false");
    expect(pick(row(main, 3), '[data-role="state"]').textContent).toContain("거절됨");
  });

  it("승인 · 거절 단추는 제안됨 줄에만 있다", async () => {
    // 검증: V82 E35
    const main = await open(
      rulesWith("1"),
      proposalsOf(
        proposal(1, "proposed", "1", "0.5"),
        proposal(2, "cooling", "1", "1.5"),
        proposal(3, "applied", "1", "0.5"),
        proposal(4, "rejected", "1", "0.5"),
      ),
    );
    expect(row(main, 1).querySelector('button[data-action="approve"]')).not.toBeNull();
    expect(row(main, 1).querySelector('button[data-action="reject"]')).not.toBeNull();
    for (const id of [2, 3, 4]) {
      expect(row(main, id).querySelector('button[data-action="approve"]'), `${id}`).toBeNull();
      expect(row(main, id).querySelector('button[data-action="reject"]'), `${id}`).toBeNull();
    }
  });

  it("개선안이 없으면 빈 상태가 보인다", async () => {
    // 검증: V82
    const main = await open(rulesWith("1"), proposalsOf());
    expect(main.querySelector('[data-role="no-proposals"]')).not.toBeNull();
    expect(main.querySelector('[data-role="proposals"] [data-proposal]')).toBeNull();
  });
});

describe("규칙 화면 · 승인 · 거절", () => {
  it("승인하면 번호 · 지문을 그대로 보내고, 다시 읽어 반영과 바뀐 값을 그 줄과 규칙 표에 보인다", async () => {
    // 검증: V82 V21 E35 F5
    const safe = proposal(5, "proposed", "1", "0.5");
    const main = await open(rulesWith("1"), proposalsOf(safe));
    const readsBefore = server.sent("GET", "/api/rules").length;
    const listsBefore = server.sent("GET", "/api/proposals").length;

    click(row(main, 5), 'button[data-action="approve"]');
    await settle();

    expect(server.sent("POST", "/api/proposals/answer").map((call) => call.body)).toEqual([
      { id: 5, number: safe.number, fingerprint: safe.fingerprint, answer: "approve" },
    ]);
    expect(server.sent("GET", "/api/rules").length).toBeGreaterThan(readsBefore);
    expect(server.sent("GET", "/api/proposals").length).toBeGreaterThan(listsBefore);
    const done = row(main, 5);
    expect(pick(done, '[data-role="state"]').textContent).toContain("반영됨");
    expect(pick(done, '[data-role="answer-result"]').textContent).toContain("반영");
    expect(done.querySelector('button[data-action="approve"]')).toBeNull();
    const risk = pick<HTMLElement>(main, '[data-role="rules"] tr[data-key="risk_per_trade_pct"]');
    expect(pick(risk, '[data-role="value"]').textContent).toContain("0.5%");
    expect(risk.dataset.changed).toBe("true");
  });

  it("위험을 키우는 개선안을 승인하면 그 줄에 '다음 거래일부터'가 보이고 냉각중이 된다", async () => {
    // 검증: V82 E28 V21
    const main = await open(rulesWith("1"), proposalsOf(proposal(6, "proposed", "1", "1.5")));
    click(row(main, 6), 'button[data-action="approve"]');
    await settle();
    const done = row(main, 6);
    expect(pick(done, '[data-role="state"]').textContent).toContain("냉각중");
    expect(pick(done, '[data-role="answer-result"]').textContent).toContain("다음 거래일부터");
    const risk = pick<HTMLElement>(main, '[data-role="rules"] tr[data-key="risk_per_trade_pct"]');
    expect(pick(risk, '[data-role="value"]').textContent).toContain("1%");
    expect(pick(risk, '[data-role="cooling"]').textContent).toContain("1.5%");
  });

  it("거절하면 answer가 reject이고 그 줄에 거절이 보이며 규칙은 그대로다", async () => {
    // 검증: V82 V21
    const pending = proposal(7, "proposed", "1", "0.5");
    const main = await open(rulesWith("1"), proposalsOf(pending));
    click(row(main, 7), 'button[data-action="reject"]');
    await settle();
    expect(server.sent("POST", "/api/proposals/answer").map((call) => call.body)).toEqual([
      { id: 7, number: pending.number, fingerprint: pending.fingerprint, answer: "reject" },
    ]);
    const done = row(main, 7);
    expect(pick(done, '[data-role="state"]').textContent).toContain("거절됨");
    expect(pick(done, '[data-role="answer-result"]').textContent).toContain("거절");
    const risk = pick<HTMLElement>(main, '[data-role="rules"] tr[data-key="risk_per_trade_pct"]');
    expect(pick(risk, '[data-role="value"]').textContent).toContain("1%");
    expect(risk.dataset.changed).not.toBe("true");
  });

  it("거절된 답(이미 답함)의 까닭이 그 줄에 보인다", async () => {
    // 검증: V82 E35
    const main = await open(rulesWith("1"), proposalsOf(proposal(8, "proposed", "1", "0.5")), (fake) => {
      fake.answerOverride = { status: 200, body: { result: "rejected", reason: "already_resolved" } };
    });
    click(row(main, 8), 'button[data-action="approve"]');
    await settle();
    expect(pick(row(main, 8), '[data-role="answer-result"]').textContent).toContain("이미");
  });

  it("답하기가 실패(503)하면 그 줄에 다시 시도하라는 글이 보이고 단추는 그대로다", async () => {
    // 검증: V82 E57
    const main = await open(rulesWith("1"), proposalsOf(proposal(9, "proposed", "1", "0.5")), (fake) => {
      fake.answerOverride = { status: 503, body: { error: "unavailable" } };
    });
    click(row(main, 9), 'button[data-action="approve"]');
    await settle();
    const after = row(main, 9);
    expect(pick(after, '[data-role="answer-result"]').textContent).toContain("다시 시도");
    expect(pick(after, '[data-role="state"]').textContent).toContain("제안됨");
    expect(after.querySelector('button[data-action="approve"]')).not.toBeNull();
  });
});

describe("규칙 화면 · 못 읽음 · 경로 · 가벼움", () => {
  for (const which of ["rules", "proposals"] as const) {
    it(`${which === "rules" ? "규칙" : "개선안 목록"}을 못 읽으면 '불러오지 못함 · 다시 시도'가 보이고 다시 시도하면 그린다`, async () => {
      // 검증: E57 C29
      const main = await open(rulesWith("1"), proposalsOf(proposal(1, "proposed", "1", "0.5")), (fake) => {
        if (which === "rules") fake.failRules = true;
        else fake.failProposals = true;
      });
      expect(pick(main, '[role="alert"]').textContent).toContain("불러오지 못함");
      server.failRules = false;
      server.failProposals = false;
      click(main, 'button[data-action="retry"]');
      await settle();
      expect(main.querySelector('[role="alert"]')).toBeNull();
      expect(main.querySelector('[data-role="rules"] tr[data-key="risk_per_trade_pct"]')).not.toBeNull();
      expect(main.querySelector('[data-role="proposals"] [data-proposal="1"]')).not.toBeNull();
    });
  }

  it("화면이 부르는 경로는 모두 규칙 화면의 경로 표에 있다", async () => {
    // 검증: D11 D2 C29
    const seen = new Set<string>();
    const record = () => server.calls.forEach((call) => seen.add(call.url));
    let main = await open(rulesWith("1"), proposalsOf(proposal(1, "proposed", "1", "0.5")));
    click(row(main, 1), 'button[data-action="approve"]');
    await settle();
    record();
    main = await open(rulesWith("1"), proposalsOf(proposal(2, "proposed", "1", "0.5")));
    click(row(main, 2), 'button[data-action="reject"]');
    await settle();
    record();
    const table = SCREEN_PATHS.rules;
    for (const url of seen) expect(table, url).toContain(url);
    expect([...table].sort()).toEqual(["/api/session", "/api/rules", "/api/proposals", "/api/proposals/answer"].sort());
  });

  it("규칙 main은 메뉴와 화면을 그리고 '준비 중'이 아니다", async () => {
    // 검증: V82 V75 F12
    server = new FakeServer(rulesWith("1"), proposalsOf());
    vi.stubGlobal("fetch", server.fetch);
    vi.resetModules();
    const html = PAGE["../../web/rules/index.html"];
    document.body.innerHTML = new DOMParser().parseFromString(html, "text/html").body.innerHTML;
    await MAIN["../../web/src/rules/main.ts"]();
    await settle();
    const current = [...document.querySelectorAll('#menu nav [aria-current="page"]')];
    expect(current.map((a) => a.textContent?.trim())).toEqual(["규칙"]);
    expect(document.querySelectorAll("h1").length).toBe(1);
    expect(document.body.textContent).not.toContain("준비 중");
    expect(document.querySelector('[data-role="rules"] tr[data-key="risk_per_trade_pct"]')).not.toBeNull();
  });

  it("규칙 화면 소스는 three · 사무실 3D에 닿지 않고 공통 부품을 쓴다", () => {
    // 검증: V77 N21
    const { files, bare } = reach(mainOf("rules"));
    expect(files.has("../../web/src/rules/screen.ts"), "화면 모듈에 닿는다").toBe(true);
    expect(files.has("../../web/src/rules/model.ts"), "화면 모델에 닿는다").toBe(true);
    expect(files.has("../../web/src/ui/parts.ts"), "공통 부품을 쓴다").toBe(true);
    expect([...bare].filter((name) => name === "three" || name.startsWith("three/"))).toEqual([]);
    expect([...files].filter((file) => file.includes("scene3d") || file.includes("/src/office/"))).toEqual([]);
  });
});
