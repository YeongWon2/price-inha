// 히스토리 화면의 "결정 메모" 탭(additions.md 23번 · R7-4 · C25 · E54 · V70): `GET /api/memos?max=`로 첫 쪽을 읽어
// 줄(결정 날짜 · 종목 · 결정 · 상태 · 결과 · 수익률 · 교훈 또는 까닭)을 그리고, "더 보기"는 `next`가 있을 때만 있고
// 누르면 `before=next`로 이어 붙인다. 빈 상태 · 읽기 실패와 다시 시도. 화면은 서버가 정한 순서 · 판정을 그대로 그린다.
import { beforeEach, describe, expect, it } from "vitest";
import { API } from "../../web/src/paths";
import { MEMO_PAGE_MAX, mountMemoTab, type Fetcher } from "../../web/src/history/memos";
import type { Memo } from "../../web/src/ui/memo-model";

const json = (status: number, body: unknown): Promise<Response> =>
  Promise.resolve(new Response(JSON.stringify(body), { status, headers: { "Content-Type": "application/json" } }));

function fakeFetch(answer: (url: string) => Promise<Response>): { fetcher: Fetcher; urls: string[] } {
  const urls: string[] = [];
  const fetcher: Fetcher = (url) => {
    urls.push(url);
    return answer(url);
  };
  return { fetcher, urls };
}

function memo(id: number, state: Memo["state"]): Memo {
  const base: Memo = { id, trade: id + 100, date: "2026-11-21", symbol: "000660", decision: "buy", state };
  if (state === "settled") return { ...base, return_pct: 4, hold_days: 5, verdict: "right", lesson: `교훈 ${id}.` };
  if (state === "unsettled") return { ...base, verdict: "unknown", unsettled_reason: "no_price" };
  return { ...base, reason: "그때 까닭" };
}

let panel: HTMLElement;
beforeEach(() => {
  document.body.replaceChildren();
  panel = document.createElement("section");
  document.body.appendChild(panel);
});

function shown(): string[] {
  return [...panel.querySelectorAll<HTMLElement>('[data-role="memos"] [data-memo]')].map((row) => row.dataset.memo ?? "");
}

describe("히스토리 · 결정 메모 탭", () => {
  it("처음 열면 첫 쪽을 읽어 최근 것부터 줄을 그리고 줄마다 상태 · 결과 · 교훈 또는 까닭을 보인다", async () => {
    // 검증: V70 R7-4 C25 E54
    const { fetcher, urls } = fakeFetch(() =>
      json(200, { memos: [memo(3, "settled"), memo(2, "unsettled"), memo(1, "pending")], next: null }),
    );
    const tab = mountMemoTab(panel, fetcher);
    expect(tab.loaded).toBe(false);
    await tab.open();
    expect(tab.loaded).toBe(true);
    expect(urls).toEqual([`${API.memos}?max=${MEMO_PAGE_MAX}`]);
    expect(shown()).toEqual(["3", "2", "1"]);
    const rows = [...panel.querySelectorAll<HTMLElement>("[data-memo]")];
    expect(rows[0].dataset.trade).toBe("103");
    expect(rows[0].dataset.state).toBe("settled");
    expect(rows[0].textContent).toContain("정산됨");
    expect(rows[0].textContent).toContain("맞음");
    expect(rows[0].textContent).toContain("+4.00%");
    expect(rows[0].textContent).toContain("교훈 3.");
    expect(rows[1].textContent).toContain("정산 못 함");
    expect(rows[1].textContent).toContain("시세 없음");
    expect(rows[2].textContent).toContain("정산 대기");
    expect(rows[2].textContent).toContain("그때 까닭");
    expect(panel.querySelector('button[data-action="more"]')).toBeNull();
    await tab.open();
    expect(urls.length).toBe(1);
  });

  it("더 보기는 next가 있을 때만 있고 누르면 before=next로 이어 붙인다", async () => {
    // 검증: C25
    const { fetcher, urls } = fakeFetch((url) =>
      url.includes("before=")
        ? json(200, { memos: [memo(1, "pending")], next: null })
        : json(200, { memos: [memo(3, "settled"), memo(2, "settled")], next: 2 }),
    );
    const tab = mountMemoTab(panel, fetcher);
    await tab.open();
    const more = panel.querySelector<HTMLButtonElement>('button[data-action="more"]');
    expect(more).not.toBeNull();
    more?.click();
    await tab.settled();
    expect(urls[1]).toBe(`${API.memos}?before=2&max=${MEMO_PAGE_MAX}`);
    expect(shown()).toEqual(["3", "2", "1"]);
    expect(panel.querySelector('button[data-action="more"]')).toBeNull();
  });

  it("메모가 없으면 빈 상태 글, 읽기에 실패하면 오류 줄과 다시 시도", async () => {
    // 검증: C25
    let fail = true;
    const { fetcher, urls } = fakeFetch(() => (fail ? json(500, {}) : json(200, { memos: [], next: null })));
    const tab = mountMemoTab(panel, fetcher);
    await tab.open();
    expect(panel.querySelector('[role="alert"]')).not.toBeNull();
    const retry = panel.querySelector<HTMLButtonElement>('button[data-action="retry"]');
    expect(retry).not.toBeNull();
    fail = false;
    retry?.click();
    await tab.settled();
    expect(urls.length).toBe(2);
    expect(panel.querySelector('[role="alert"]')).toBeNull();
    expect(panel.querySelector('[data-role="empty"]')?.textContent).toContain("아직 결정 메모가 없습니다");
  });
});
