// 히스토리 화면의 매매 상세에 근거 카드(server 50차 · F7-7 · C23 · V68 · V69 · V83): 매매 줄을 펼치면 그 매매 하나
// (`GET /api/trades/{id}`)의 근거에 실린 담당별 의견 · 팀장 요약으로 근거 카드가 보인다. 근거 경로는 부르지 않는다.
import { afterEach, describe, expect, it, vi } from "vitest";
import { mountHistory } from "../../web/src/history/screen";
import type { TradeDetail } from "../../web/src/history/model";
import { detailOf, trades } from "./history-fixture";
import { oldBasis, recordedBasis } from "./basis-card-fixture";

const calls: string[] = [];

/** 매매 둘: 2번은 의견 기록이 있고, 1번은 칸이 생기기 전 매매다. */
function detail(id: number): TradeDetail {
  const base = detailOf(trades(2)[id - 1]);
  const extra = id === 2 ? recordedBasis() : oldBasis();
  const { sources: _sources, ...fields } = extra;
  void _sources;
  return { ...base, basis: { ...base.basis, ...fields } as TradeDetail["basis"] };
}

const fakeFetch = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
  const url = String(input);
  calls.push(url);
  const json = (status: number, value: unknown) =>
    new Response(JSON.stringify(value), { status, headers: { "Content-Type": "application/json" } });
  if ((init?.method ?? "GET").toUpperCase() === "POST") return json(200, {});
  const path = new URL(url, "http://local.test").pathname;
  if (path === "/api/trades") return json(200, { trades: trades(2).reverse(), next: null });
  const one = /^\/api\/trades\/(\d+)$/.exec(path);
  if (one) return json(200, detail(Number(one[1])));
  return json(404, { error: "not_found" });
});

async function settle(): Promise<void> {
  for (let i = 0; i < 20; i++) await new Promise((resolve) => setTimeout(resolve, 0));
}

async function open(): Promise<HTMLElement> {
  calls.length = 0;
  vi.stubGlobal("fetch", fakeFetch);
  document.body.replaceChildren();
  const main = document.createElement("main");
  document.body.append(main);
  await mountHistory(main);
  await settle();
  return main;
}

async function expand(main: HTMLElement, id: number): Promise<HTMLElement> {
  const row = main.querySelector<HTMLElement>(`[data-trade="${id}"]`);
  expect(row, `매매 ${id} 줄`).not.toBeNull();
  row?.querySelector<HTMLButtonElement>('button[data-action="toggle"]')?.click();
  await settle();
  const card = row?.querySelector<HTMLElement>('[data-role="detail"] [data-role="basis-card"]');
  expect(card, "펼친 줄 안의 근거 카드").not.toBeNull();
  return card as HTMLElement;
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("히스토리 근거 카드", () => {
  it("의견 기록이 있는 매매를 펼치면 담당 줄 · 찬반 수 · 팀장 요약이 있는 근거 카드가 보인다", async () => {
    // 검증: V68 V69 V83 C23
    const main = await open();
    const card = await expand(main, 2);
    const agents = [...card.querySelectorAll<HTMLElement>('[data-role="opinions"] > li')].map((li) => li.dataset.agent);
    expect(agents).toEqual(["scout", "strategy", "risk", "review"]);
    expect(card.querySelector('[data-agent="scout"] [data-part="direction"]')?.textContent).toBe("기권");
    expect(card.querySelector('[data-role="votes"]')?.textContent).toContain("강세 1 · 약세 1 · 중립 1 · 기권 1");
    expect(card.querySelector('[data-role="leader"] [data-part="flip"]')?.textContent).toContain("거래량이 다시 줄면");
    expect(card.querySelector('[data-role="sources"]')?.textContent).toContain("시세");
    // 근거는 매매 하나와 함께 오므로 근거 경로를 부르지 않는다.
    expect(calls.some((url) => url.endsWith("/basis"))).toBe(false);
  });

  it("칸이 생기기 전 매매를 펼치면 근거 카드에 '기록 없음'이 보이고 옛 근거 요약은 그대로다", async () => {
    // 검증: V69 E53 V83
    const main = await open();
    const card = await expand(main, 1);
    expect(card.querySelector('[data-role="no-record"]')?.textContent).toBe("기록 없음");
    const row = main.querySelector<HTMLElement>('[data-trade="1"]');
    expect(row?.querySelector('[data-role="basis"] [data-key="hypothesis"]')?.textContent).toContain("실적 개선");
  });
});
