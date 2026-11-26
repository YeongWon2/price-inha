// 규칙 화면 · "팀 지침(AI가 배운 것)" 묶음(기능 L · 결정 30 · R8)을 jsdom에 띄운다(가짜 fetch).
// 1층 규칙과 다르다는 한 줄 · 활성 목록(글 · 조건 · 익절 비율 · 쓰임) · 그 컴퓨터면 끄기(요청 경로) · 원격이면 보기만 ·
// 내린 것은 접어 둠 · 지침이 없으면 빈 상태 · 이 묶음만 못 읽으면 다른 묶음은 그대로 · 부르는 경로는 표에 있다.
// 번호 V105는 이 차수에서 설계서에 더할 제안 번호다.
import { afterEach, describe, expect, it, vi } from "vitest";
import { SCREEN_PATHS } from "../../web/src/paths";
import {
  KIND_LABEL,
  LAYER_NOTE,
  rateText,
  readGuidelines,
  retireMessage,
  usesText,
  whyText,
  type GuidelineView,
  type GuidelinesReply,
} from "../../web/src/rules/guide-model";
import { mountRules } from "../../web/src/rules/screen";
import { proposal, proposalsOf, rulesWith } from "./rules-fixture";

interface Call {
  method: string;
  url: string;
}

const GUIDES = "/api/guidelines";

function row(id: string, more: Partial<GuidelineView> = {}): GuidelineView {
  return {
    id,
    kind: "avoid",
    text: `지침 글 ${id}`,
    when: `조건 ${id}`,
    symbol: null,
    basis: ["review:1", "review:2", "review:3"],
    uses: 4,
    wins: 3,
    losses: 1,
    win_rate_pct: 75,
    created_at: 1,
    updated_at: 2,
    state: "active",
    retired_why: null,
    ...more,
  };
}

function reply(more: Partial<GuidelinesReply> = {}): GuidelinesReply {
  return { active: [row("g1"), row("g2", { kind: "enter", uses: 0, wins: 0, losses: 0, win_rate_pct: null })], retired: [], editable: true, ...more };
}

/** 가짜 서버: 규칙 · 개선안 · 금지 종목(빈 값) · 팀 지침. 끄기는 활성에서 내린 것으로 옮긴다. */
class FakeServer {
  calls: Call[] = [];
  guides: GuidelinesReply;
  failGuides = false;
  failRetire = false;

  constructor(value: GuidelinesReply) {
    this.guides = value;
  }

  readonly fetch = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = String(input);
    const method = (init?.method ?? "GET").toUpperCase();
    this.calls.push({ method, url });
    const json = (status: number, value: unknown) =>
      new Response(JSON.stringify(value), { status, headers: { "Content-Type": "application/json" } });
    if (method === "POST" && url === "/api/session") return json(200, {});
    if (method === "GET" && url === "/api/rules") return json(200, rulesWith("1"));
    if (method === "GET" && url === "/api/proposals") return json(200, proposalsOf(proposal(1, "proposed", "1", "0.5")));
    if (method === "GET" && url === "/api/rules/config") {
      return json(200, { banned: [], sectors: [], editable: true, unsectored: [] });
    }
    if (method === "GET" && url === GUIDES) return this.failGuides ? json(500, { error: "internal" }) : json(200, this.guides);
    const retire = /^\/api\/guidelines\/([^/]+)\/retire$/.exec(url);
    if (method === "POST" && retire) {
      if (this.failRetire) return json(403, { error: "forbidden" });
      const id = decodeURIComponent(retire[1]);
      const found = this.guides.active.find((item) => item.id === id);
      if (!found) return json(404, { error: "not_found" });
      this.guides = {
        ...this.guides,
        active: this.guides.active.filter((item) => item.id !== id),
        retired: [{ ...found, state: "retired", retired_why: "person" }, ...this.guides.retired],
      };
      return json(200, { id, state: "retired" });
    }
    return json(404, { error: "not_found" });
  });

  sent(method: string, url: string): Call[] {
    return this.calls.filter((call) => call.method === method && call.url === url);
  }
}

let server: FakeServer;

async function settle(): Promise<void> {
  for (let i = 0; i < 30; i++) await new Promise((resolve) => setTimeout(resolve, 0));
}

async function open(value: GuidelinesReply, prepare?: (fake: FakeServer) => void): Promise<HTMLElement> {
  server = new FakeServer(value);
  prepare?.(server);
  vi.stubGlobal("fetch", server.fetch);
  document.body.textContent = "";
  const menu = document.createElement("div");
  menu.id = "menu";
  const main = document.createElement("main");
  main.id = "screen";
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
  return pick<HTMLElement>(main, '[data-role="guidelines"]');
}

function item(root: ParentNode, id: string): HTMLElement {
  return pick<HTMLElement>(root, `[data-role="guidelines-active"] [data-guideline="${id}"]`);
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("규칙 화면 · 팀 지침 묶음 · 보기", () => {
  it("1층 규칙과 다르다는 한 줄과 활성 지침의 글 · 조건 · 익절 비율 · 쓰임을 보인다", async () => {
    // 검증: V105 R8
    const main = await open(reply());
    const box = group(main);
    expect(box.textContent).toContain("팀 지침(AI가 배운 것)");
    expect(pick(box, '[data-role="guidelines-note"]').textContent).toBe(LAYER_NOTE);
    const first = item(box, "g1");
    expect(pick(first, '[data-role="kind"]').textContent).toBe(KIND_LABEL.avoid);
    expect(pick(first, '[data-role="text"]').textContent).toBe("지침 글 g1");
    expect(pick(first, '[data-role="when"]').textContent).toContain("조건 g1");
    expect(pick(first, '[data-role="rate"]').textContent).toBe(rateText(row("g1")));
    expect(pick(first, '[data-role="uses"]').textContent).toBe(usesText(row("g1")));
    const second = item(box, "g2");
    expect(pick(second, '[data-role="kind"]').textContent).toBe("들어가기");
    expect(pick(second, '[data-role="rate"]').textContent).toContain("아직");
    // 다른 묶음은 그대로다.
    expect(main.querySelector('[data-role="rules"] tr[data-key="risk_per_trade_pct"]')).not.toBeNull();
    expect(main.querySelector('[data-role="proposals"] [data-proposal="1"]')).not.toBeNull();
    expect(main.querySelector('[role="alert"]')).toBeNull();
  });

  it("지침이 없으면 빈 상태를 보인다", async () => {
    // 검증: V105 R8
    const main = await open(reply({ active: [], retired: [] }));
    const box = group(main);
    expect(pick(box, '[data-role="no-guidelines"]').textContent).toContain("아직");
    expect(box.querySelector('[data-role="guidelines-retired"]')).toBeNull();
  });

  it("내린 지침은 접힌 묶음 안에 까닭과 함께 있다", async () => {
    // 검증: V105 R8
    const main = await open(reply({ retired: [row("g7", { state: "retired", retired_why: "poor_win_rate" })] }));
    const folded = pick<HTMLDetailsElement>(group(main), '[data-role="guidelines-retired"]');
    expect(folded.tagName).toBe("DETAILS");
    expect(folded.open).toBe(false);
    const gone = pick<HTMLElement>(folded, '[data-guideline="g7"]');
    expect(pick(gone, '[data-role="why"]').textContent).toBe(whyText("poor_win_rate"));
    expect(group(main).querySelector('[data-role="guidelines-active"] [data-guideline="g7"]')).toBeNull();
  });
});

describe("규칙 화면 · 팀 지침 묶음 · 끄기", () => {
  it("그 컴퓨터면 끄기 단추로 그 지침을 끄고 다시 읽어 내린 것으로 옮긴다", async () => {
    // 검증: V104 V105 R8
    const main = await open(reply());
    pick<HTMLButtonElement>(item(group(main), "g1"), 'button[data-action="retire-guideline"]').click();
    await settle();
    expect(server.sent("POST", "/api/guidelines/g1/retire")).toHaveLength(1);
    expect(server.sent("GET", GUIDES).length).toBeGreaterThanOrEqual(2);
    const box = group(main);
    expect(box.querySelector('[data-role="guidelines-active"] [data-guideline="g1"]')).toBeNull();
    const folded = pick<HTMLDetailsElement>(box, '[data-role="guidelines-retired"]');
    expect(pick(folded, '[data-guideline="g1"] [data-role="why"]').textContent).toBe(whyText("person"));
    expect(pick(box, '[data-role="guidelines-result"]').textContent).toBe(retireMessage(200, { id: "g1", state: "retired" }).text);
    expect(main.querySelector('[data-role="rules"] tr[data-key="risk_per_trade_pct"]')).not.toBeNull();
  });

  it("끄지 못하면 결과 줄에 까닭이 남고 지침은 그대로다", async () => {
    // 검증: V105 R8
    const main = await open(reply(), (fake) => {
      fake.failRetire = true;
    });
    pick<HTMLButtonElement>(item(group(main), "g1"), 'button[data-action="retire-guideline"]').click();
    await settle();
    const box = group(main);
    expect(pick(box, '[data-role="guidelines-result"]').textContent).toBe(retireMessage(403, { error: "forbidden" }).text);
    expect(item(box, "g1")).not.toBeNull();
  });

  it("원격이면 끄기 단추가 없고 보기만 한다는 줄이 있다", async () => {
    // 검증: V104 V105 R8
    const main = await open(reply({ editable: false }));
    const box = group(main);
    expect(box.querySelector('button[data-action="retire-guideline"]')).toBeNull();
    expect(pick(box, '[data-role="guidelines-readonly"]').textContent).toContain("이 컴퓨터");
  });
});

describe("규칙 화면 · 팀 지침 묶음 · 못 읽음 · 경로", () => {
  it("이 묶음만 못 읽으면 그 묶음 안에만 못 읽음을 보이고 다른 묶음은 그대로다", async () => {
    // 검증: V105 E57
    const main = await open(reply(), (fake) => {
      fake.failGuides = true;
    });
    const box = group(main);
    expect(pick(box, '[data-role="guidelines-load-failed"]').textContent).toContain("불러오지 못");
    expect(main.querySelector('[role="alert"]')).toBeNull();
    expect(main.querySelector('[data-role="rules"] tr[data-key="risk_per_trade_pct"]')).not.toBeNull();
    expect(main.querySelector('[data-role="proposals"] [data-proposal="1"]')).not.toBeNull();
  });

  it("묶음이 부르는 경로는 모두 규칙 화면의 경로 표에 있다", async () => {
    // 검증: D11 V105
    const main = await open(reply());
    pick<HTMLButtonElement>(item(group(main), "g2"), 'button[data-action="retire-guideline"]').click();
    await settle();
    const known = SCREEN_PATHS.rules;
    for (const call of server.calls) {
      const covered = known.some((path) => {
        if (path === call.url) return true;
        if (!path.includes("{id}")) return false;
        const [head, tail] = path.split("{id}");
        return call.url.startsWith(head) && call.url.endsWith(tail);
      });
      expect(covered, call.url).toBe(true);
    }
    expect(known).toContain(GUIDES);
    expect(known).toContain("/api/guidelines/{id}/retire");
  });
});

describe("팀 지침 묶음의 글(순수 계산)", () => {
  it("응답을 읽고 모양이 다르면 null이다", () => {
    // 검증: V105
    expect(readGuidelines(reply())).toEqual(reply());
    expect(readGuidelines({ active: "x" })).toBeNull();
    expect(readGuidelines(null)).toBeNull();
    expect(readGuidelines({ active: [{ id: 1 }], retired: [], editable: true })).toBeNull();
  });

  it("익절 비율 · 쓰임 · 까닭 · 끄기 결과를 쉬운 말로 쓴다", () => {
    // 검증: V105 R8
    expect(rateText(row("g1"))).toBe("익절 75% (4번 중 3번)");
    expect(rateText(row("g2", { uses: 0, wins: 0, losses: 0, win_rate_pct: null }))).toBe("아직 끝난 매매 없음");
    expect(usesText(row("g1"))).toBe("쓰임 4번");
    expect(whyText("merged")).toContain("합");
    expect(whyText("poor_win_rate")).toContain("익절");
    expect(whyText("unused")).toContain("쓰이지");
    expect(whyText("blocking_trades")).toContain("막");
    expect(whyText("person")).toContain("끔");
    expect(whyText(null)).toBe("");
    expect(retireMessage(200, { id: "g1", state: "retired" })).toEqual({ kind: "ok", text: "지침을 껐습니다." });
    expect(retireMessage(403, {}).kind).toBe("failed");
    expect(retireMessage(403, {}).text).toContain("이 컴퓨터");
    expect(retireMessage(404, {}).text).toContain("없");
    expect(retireMessage(0, null).text).toContain("연결");
  });
});
