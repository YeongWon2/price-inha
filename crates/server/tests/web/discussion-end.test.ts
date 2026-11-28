// 69차: 지켜보기 · 오류로 끝난 논의의 표시(M2 상태표는 그대로 — 표시만). 서버가 논의중 매매에 붙이는
// `watched: {"reason_code", "at"}` · `failed: {"reason_code", "at"}` 칸을 히스토리 화면의 매매 줄 · 상태 흐름 ·
// 상태 거르기와 사무실 근거 패널이 "지켜봄 — <까닭 글>" · "논의가 끝나지 못함 — <까닭 글>"로 그린다.
import { afterEach, describe, expect, it, vi } from "vitest";
import { discussionEndKind, discussionEndText } from "../../web/src/ui/discussion-end";
import {
  TRADE_FILTERS,
  flowRows,
  timeText,
  tradeFilterLabel,
  tradeRows,
  type TradeItem,
} from "../../web/src/history/model";
import { mountHistory } from "../../web/src/history/screen";
import { mountBasisPanel, type FetchLike } from "../../web/src/office/basis-panel";
import { daySummary, emptyDetailOf, trade } from "./history-fixture";
import { oldBasis } from "./basis-card-fixture";

const ENDED_AT = Date.UTC(2026, 10, 16, 1, 30);

/** 지켜보기로 끝난 논의중 매매. */
function watchedTrade(id: number, code = "missing_data"): TradeItem {
  return { ...trade(id), watched: { reason_code: code, at: ENDED_AT } };
}

/** 오류로 끝난 논의중 매매. */
function failedTrade(id: number): TradeItem {
  return { ...trade(id), failed: { reason_code: "discussion_error", at: ENDED_AT } };
}

async function settle(): Promise<void> {
  for (let i = 0; i < 20; i++) await new Promise((resolve) => setTimeout(resolve, 0));
}

function json(status: number, value: unknown): Response {
  return new Response(JSON.stringify(value), { status, headers: { "Content-Type": "application/json" } });
}

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe("끝난 논의의 글", () => {
  it("지켜보기 까닭 코드는 쉬운 말로, 오류는 '논의가 끝나지 못함'으로 쓴다", () => {
    // 검증: V83 C30 M2
    const at = ENDED_AT;
    expect(discussionEndText({ watched: { reason_code: "missing_data", at } })).toBe("지켜봄 — 자료가 모자라 지켜봄");
    expect(discussionEndText({ watched: { reason_code: "consensus_below", at } })).toBe(
      "지켜봄 — 합의 점수가 기준에 못 미쳐 지켜봄",
    );
    expect(discussionEndText({ watched: { reason_code: "role_unavailable", at } })).toBe(
      "지켜봄 — 담당이 답하지 못해 지켜봄",
    );
    expect(discussionEndText({ watched: { reason_code: "discussion_bounded", at } })).toBe(
      "지켜봄 — 논의가 길어져 지켜봄",
    );
    expect(discussionEndText({ watched: { reason_code: "rationale_incomplete", at } })).toBe(
      "지켜봄 — 근거가 비어 지켜봄",
    );
    expect(discussionEndText({ watched: { reason_code: "leader_watch", at } })).toBe("지켜봄 — 팀장이 지켜보기로 정함");
    expect(discussionEndText({ watched: { reason_code: "new_code", at } })).toBe("지켜봄");
    expect(discussionEndText({ failed: { reason_code: "discussion_error", at } })).toBe(
      "논의가 끝나지 못함 — 논의 중 오류가 났음",
    );
    expect(discussionEndText({ failed: { reason_code: "gather_failed", at } })).toBe(
      "논의가 끝나지 못함 — 자료를 모으다 오류가 났음",
    );
    expect(discussionEndText({ failed: { reason_code: "new_code", at } })).toBe("논의가 끝나지 못함");
    expect(discussionEndText({})).toBeNull();
    expect(discussionEndKind({ watched: { reason_code: "missing_data", at } })).toBe("watched");
    expect(discussionEndKind({ failed: { reason_code: "discussion_error", at } })).toBe("failed");
    expect(discussionEndKind({})).toBeNull();
  });

  it("매매 줄의 상태 글과 상태 흐름의 끝 줄이 끝난 논의를 보인다", () => {
    // 검증: V83 C30 M2
    const rows = tradeRows({ trades: [watchedTrade(3), failedTrade(2), trade(1)], next: null });
    expect(rows.map((row) => [row.state, row.ended, row.stateLabel])).toEqual([
      ["discussing", "watched", "지켜봄 — 자료가 모자라 지켜봄"],
      ["discussing", "failed", "논의가 끝나지 못함 — 논의 중 오류가 났음"],
      ["discussing", null, "논의중"],
    ]);
    const item = watchedTrade(3);
    const flow = flowRows({ ...emptyDetailOf(item), watched: item.watched });
    expect(flow.map((row) => [row.state, row.label, row.at])).toEqual([
      ["discussing", "논의중", timeText(item.created_at)],
      ["watched", "지켜봄 — 자료가 모자라 지켜봄", timeText(ENDED_AT)],
    ]);
    expect(flowRows(emptyDetailOf(trade(1))).map((row) => row.state)).toEqual(["discussing"]);
  });

  it("상태 거르기는 논의중 · 지켜봄 · 끝나지 못함을 나눈다", () => {
    // 검증: V83 C30 M2
    expect(TRADE_FILTERS.slice(0, 3)).toEqual(["discussing", "watched", "failed"]);
    expect(TRADE_FILTERS.slice(0, 3).map(tradeFilterLabel)).toEqual(["논의중", "지켜봄", "끝나지 못함"]);
    expect(TRADE_FILTERS.slice(3)).toEqual([
      "judged",
      "awaiting_approval",
      "blocked",
      "ordered",
      "holding",
      "sold",
      "reviewed",
    ]);
  });
});

describe("히스토리 화면의 끝난 논의", () => {
  async function open(list: TradeItem[]): Promise<{ main: HTMLElement; urls: string[] }> {
    const urls: string[] = [];
    const fetcher = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input);
      urls.push(url);
      const parsed = new URL(url, "http://local.test");
      if ((init?.method ?? "GET").toUpperCase() === "POST") return json(200, {});
      if (parsed.pathname === "/api/trades") {
        const state = parsed.searchParams.get("state");
        const kept = list.filter((t) => {
          if (state === null || state === "") return true;
          if (state === "watched") return t.watched !== undefined;
          if (state === "failed") return t.failed !== undefined;
          if (state === "discussing") return t.state === "discussing" && !t.watched && !t.failed;
          return t.state === state;
        });
        return json(200, { trades: kept, next: null });
      }
      const one = /^\/api\/trades\/(\d+)$/.exec(parsed.pathname);
      if (one) {
        const found = list.find((t) => t.id === Number(one[1]));
        if (!found) return json(404, { error: "not_found" });
        return json(200, { ...emptyDetailOf(found), watched: found.watched, failed: found.failed });
      }
      if (parsed.pathname === "/api/daily-summary") return json(200, daySummary("2026-11-16"));
      if (parsed.pathname === "/api/approvals" || parsed.pathname === "/api/memos") {
        return json(200, { approvals: [], memos: [], next: null });
      }
      return json(404, { error: "not_found" });
    });
    vi.stubGlobal("fetch", fetcher);
    document.body.innerHTML = '<div id="menu"></div><main id="screen"><h1>히스토리</h1></main>';
    const main = document.getElementById("screen") as HTMLElement;
    await mountHistory(main);
    await settle();
    return { main, urls };
  }

  function row(main: HTMLElement, id: number): HTMLElement {
    const found = main.querySelector<HTMLElement>(`[data-role="trades"] [data-trade="${id}"]`);
    expect(found, `매매 ${id} 줄`).not.toBeNull();
    return found as HTMLElement;
  }

  it("지켜봄 · 끝나지 못함 매매 줄은 그 글로, 펼치면 상태 흐름 끝에 그 줄이 있다", async () => {
    // 검증: V83 C30 M2
    const { main } = await open([watchedTrade(3), failedTrade(2), trade(1)]);
    const watched = row(main, 3);
    expect(watched.dataset.state).toBe("discussing");
    expect(watched.dataset.ended).toBe("watched");
    expect(watched.querySelector('[data-role="state"]')?.textContent).toBe("지켜봄 — 자료가 모자라 지켜봄");
    expect(row(main, 2).dataset.ended).toBe("failed");
    expect(row(main, 2).querySelector('[data-role="state"]')?.textContent).toBe("논의가 끝나지 못함 — 논의 중 오류가 났음");
    expect(row(main, 1).dataset.ended).toBeUndefined();
    expect(row(main, 1).querySelector('[data-role="state"]')?.textContent).toBe("논의중");

    watched.querySelector<HTMLButtonElement>('button[data-action="toggle"]')?.click();
    await settle();
    const steps = [...row(main, 3).querySelectorAll<HTMLElement>('[data-role="flow"] [data-state]')];
    expect(steps.map((node) => node.dataset.state)).toEqual(["discussing", "watched"]);
    expect(steps[1].textContent).toContain("지켜봄 — 자료가 모자라 지켜봄");
    expect(steps[1].textContent).toContain(timeText(ENDED_AT));
  });

  it("상태 거르기에 지켜봄 · 끝나지 못함이 따로 있고 고르면 그 글자로 읽는다", async () => {
    // 검증: V83 C30 M2
    const { main, urls } = await open([watchedTrade(3), failedTrade(2), trade(1)]);
    const select = main.querySelector<HTMLSelectElement>('select[data-role="trade-state"]');
    expect(select).not.toBeNull();
    const options = [...(select as HTMLSelectElement).options].map((option) => [option.value, option.textContent]);
    expect(options.slice(0, 4)).toEqual([
      ["", "전체"],
      ["discussing", "논의중"],
      ["watched", "지켜봄"],
      ["failed", "끝나지 못함"],
    ]);
    (select as HTMLSelectElement).value = "watched";
    select?.dispatchEvent(new Event("change", { bubbles: true }));
    await settle();
    const last = new URL(urls.filter((url) => url.startsWith("/api/trades?") || url === "/api/trades").at(-1) ?? "", "http://local.test");
    expect(last.searchParams.get("state")).toBe("watched");
    const shown = [...main.querySelectorAll<HTMLElement>('[data-role="trades"] [data-trade]')].map((node) => node.dataset.trade);
    expect(shown).toEqual(["3"]);
  });
});

describe("사무실 근거 패널의 끝난 논의", () => {
  function panelWith(extra: Record<string, unknown>) {
    document.body.replaceChildren();
    const host = document.createElement("aside");
    document.body.append(host);
    const fetcher: FetchLike = vi.fn(async (input: string) => {
      const parsed = new URL(input, "http://local.test");
      if (parsed.pathname === "/api/trades") return json(200, { trades: [{ id: 7, symbol: "035420" }], next: null });
      if (parsed.pathname === "/api/trades/7/basis") return json(200, { id: 7, summary: "요약", ...oldBasis(), ...extra });
      return json(404, { error: "not_found" });
    });
    return mountBasisPanel(host, fetcher);
  }

  it("지켜보기로 끝난 매매의 근거를 열면 카드 위에 '지켜봄 — <까닭 글>' 줄이 있다", async () => {
    // 검증: V68 C23 M2
    const panel = panelWith({ watched: { reason_code: "missing_data", at: ENDED_AT } });
    await panel.openTrade(7);
    const line = panel.element.querySelector<HTMLElement>('[data-role="basis-body"] [data-role="discussion-end"]');
    expect(line?.textContent).toBe("지켜봄 — 자료가 모자라 지켜봄");
    expect(line?.dataset.ended).toBe("watched");
    expect(panel.element.querySelector('[data-role="basis-body"] [data-role="basis-card"]')).not.toBeNull();
  });

  it("오류로 끝났으면 '논의가 끝나지 못함', 아니면 그 줄이 없다", async () => {
    // 검증: V68 C23 M2
    const failed = panelWith({ failed: { reason_code: "discussion_error", at: ENDED_AT } });
    await failed.openSymbol("035420");
    const line = failed.element.querySelector<HTMLElement>('[data-role="discussion-end"]');
    expect(line?.textContent).toBe("논의가 끝나지 못함 — 논의 중 오류가 났음");
    expect(line?.dataset.ended).toBe("failed");
    const plain = panelWith({});
    await plain.openTrade(7);
    expect(plain.element.querySelector('[data-role="discussion-end"]')).toBeNull();
  });
});
