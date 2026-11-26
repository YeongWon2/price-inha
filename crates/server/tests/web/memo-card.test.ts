// 사무실 옆 패널의 "어제 메모" 카드(additions.md 23번 · R7-4 · C25 · V70): `GET /api/memos?days=7`을 읽어 정산된 교훈
// (종목 · 결정 · 결과 · 수익률 · 교훈)과 정산 대기 수를 그리고, 없으면 빈 상태 글, 실패하면 오류 줄, `office.memo`
// 이벤트에 다시 읽는다. 글은 글자로만 넣는다(마크업으로 읽지 않는다).
import { beforeEach, describe, expect, it } from "vitest";
import { API } from "../../web/src/paths";
import { KIND_OFFICE_MEMO, MEMO_CARD_DAYS, mountMemoCard, type Fetcher } from "../../web/src/office/memo-card";
import type { Memo } from "../../web/src/ui/memo-model";

const json = (status: number, body: unknown): Promise<Response> =>
  Promise.resolve(new Response(JSON.stringify(body), { status, headers: { "Content-Type": "application/json" } }));

function fakeFetch(answers: (() => Promise<Response>)[]): { fetcher: Fetcher; urls: string[] } {
  const urls: string[] = [];
  let turn = 0;
  const fetcher: Fetcher = (url) => {
    urls.push(url);
    const answer = answers[Math.min(turn, answers.length - 1)];
    turn += 1;
    return answer();
  };
  return { fetcher, urls };
}

function memo(trade: number, state: Memo["state"], over: Partial<Memo> = {}): Memo {
  return {
    id: trade,
    trade,
    date: "2026-11-21",
    symbol: "000660",
    decision: "buy",
    state,
    ...(state === "settled"
      ? { return_pct: 10, hold_days: 5, verdict: "right" as const, lesson: `교훈 ${trade} 첫 문장. 둘째 문장.` }
      : {}),
    ...over,
  };
}

let host: HTMLElement;
beforeEach(() => {
  document.body.replaceChildren();
  host = document.createElement("section");
  host.id = "memos";
  document.body.appendChild(host);
});

describe("어제 메모 카드", () => {
  it("최근 7일 결정 메모를 읽어 정산된 교훈을 서버 순서대로 그리고 정산 대기 수를 보인다", async () => {
    // 검증: V70 R7-4 C25
    const list = [memo(4, "pending"), memo(3, "settled"), memo(2, "settled", { verdict: "wrong", return_pct: -3 })];
    const { fetcher, urls } = fakeFetch([() => json(200, list)]);
    const card = mountMemoCard(host, fetcher);
    await card.refresh();
    expect(MEMO_CARD_DAYS).toBe(7);
    expect(urls).toEqual([`${API.memos}?days=7`]);
    const box = host.querySelector<HTMLElement>('[data-role="memo-card"]');
    expect(box?.querySelector("h2")?.textContent).toBe("어제 메모");
    const items = [...host.querySelectorAll<HTMLElement>('[data-role="memo-list"] [data-trade]')];
    expect(items.map((item) => item.dataset.trade)).toEqual(["3", "2"]);
    expect(items[0].dataset.verdict).toBe("right");
    expect(items[0].textContent).toContain("000660");
    expect(items[0].textContent).toContain("사기");
    expect(items[0].textContent).toContain("맞음");
    expect(items[0].textContent).toContain("+10.00%");
    expect(items[0].textContent).toContain("교훈 3 첫 문장. 둘째 문장.");
    expect(items[1].textContent).toContain("틀림");
    expect(host.querySelector('[data-role="memo-pending"]')?.textContent).toContain("정산 대기 1건");
    expect(host.querySelector('[data-role="empty"]')).toBeNull();
  });

  it("정산된 메모가 없으면 빈 상태 글, 정산 대기만 있으면 그 수도 보인다", async () => {
    // 검증: V70
    const { fetcher } = fakeFetch([() => json(200, []), () => json(200, [memo(1, "pending")])]);
    const card = mountMemoCard(host, fetcher);
    await card.refresh();
    expect(host.querySelector('[data-role="empty"]')?.textContent).toContain("아직 정산된 결정 메모가 없습니다");
    expect(host.querySelector('[data-role="memo-pending"]')).toBeNull();
    await card.refresh();
    expect(host.querySelector('[data-role="empty"]')).not.toBeNull();
    expect(host.querySelector('[data-role="memo-pending"]')?.textContent).toContain("정산 대기 1건");
  });

  it("교훈 글은 마크업으로 읽지 않고 글자 그대로 보인다", async () => {
    // 검증: R7-4
    const lesson = "<b>굵게</b> 교훈.";
    const { fetcher } = fakeFetch([() => json(200, [memo(1, "settled", { lesson })])]);
    const card = mountMemoCard(host, fetcher);
    await card.refresh();
    expect(host.querySelector("b")).toBeNull();
    expect(host.textContent).toContain(lesson);
  });

  it("읽기에 실패하면 오류 줄이 보이고, office.memo 이벤트면 다시 읽는다", async () => {
    // 검증: C25
    const { fetcher, urls } = fakeFetch([() => json(500, { error: "internal" }), () => json(200, [memo(1, "settled")])]);
    const card = mountMemoCard(host, fetcher);
    await card.refresh();
    expect(host.querySelector('[role="alert"]')?.textContent).toContain("어제 메모를 불러오지 못했습니다");
    expect(KIND_OFFICE_MEMO).toBe("office.memo");
    await card.onEvent(KIND_OFFICE_MEMO);
    expect(urls.length).toBe(2);
    expect(host.querySelector('[role="alert"]')).toBeNull();
    expect(host.querySelectorAll('[data-role="memo-list"] [data-trade]').length).toBe(1);
    await card.onEvent("office.talk");
    expect(urls.length).toBe(2);
  });
});
