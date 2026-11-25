// 사무실 근거 패널(server 50차 · F7-7 · C23 · V68 · V69): 논의 줄의 종목 표시를 누르면 그 종목의 가장 최근 매매의
// 근거 카드가 채팅 옆 패널에 보인다(히스토리와 같은 카드). 매매 번호로도 연다. 부르는 경로는 표에 있다.
import { afterEach, describe, expect, it, vi } from "vitest";
import { chatLineElement } from "../../web/src/office/chat-view";
import { bindTopicClicks, mountBasisPanel, type FetchLike } from "../../web/src/office/basis-panel";
import { SCREEN_PATHS, tradeBasisPath } from "../../web/src/paths";
import { oldBasis, recordedBasis } from "./basis-card-fixture";

interface Seen {
  url: string;
  path: string;
}

/** 매매 목록(최근 것부터): 12번 000660 · 11번 035420 · 10번 000660(옛 매매). */
const LIST = [
  { id: 12, symbol: "000660", state: "judged" },
  { id: 11, symbol: "035420", state: "judged" },
  { id: 10, symbol: "000660", state: "sold" },
];

function fake(options: { failBasis?: boolean } = {}): { fetcher: FetchLike; seen: Seen[] } {
  const seen: Seen[] = [];
  const fetcher = vi.fn(async (input: string) => {
    const parsed = new URL(input, "http://local.test");
    seen.push({ url: input, path: parsed.pathname });
    const json = (status: number, value: unknown) =>
      new Response(JSON.stringify(value), { status, headers: { "Content-Type": "application/json" } });
    if (parsed.pathname === "/api/trades") return json(200, { trades: LIST, next: null });
    const one = /^\/api\/trades\/(\d+)\/basis$/.exec(parsed.pathname);
    if (one) {
      if (options.failBasis) return json(500, { error: "internal" });
      const id = Number(one[1]);
      if (id === 12) return json(200, { id, summary: "요약", ...recordedBasis() });
      if (id === 10 || id === 11) return json(200, { id, summary: "요약", ...oldBasis() });
      return json(404, { error: "not_found" });
    }
    return json(404, { error: "not_found" });
  });
  return { fetcher, seen };
}

async function settle(): Promise<void> {
  for (let i = 0; i < 20; i++) await new Promise((resolve) => setTimeout(resolve, 0));
}

function setup(options: { failBasis?: boolean } = {}) {
  document.body.replaceChildren();
  const host = document.createElement("aside");
  const log = document.createElement("div");
  host.append(log);
  document.body.append(host);
  const { fetcher, seen } = fake(options);
  const panel = mountBasisPanel(host, fetcher);
  bindTopicClicks(log, panel);
  return { host, log, panel, seen };
}

function talk(log: HTMLElement, topic: string): HTMLElement {
  const line = chatLineElement({ key: "t-" + topic, side: "left", name: "전략", text: "사 볼 만하다", tag: topic, pending: false });
  log.append(line);
  return line;
}

afterEach(() => {
  vi.restoreAllMocks();
});

describe("사무실 근거 패널", () => {
  it("처음에는 숨어 있고, 매매 번호로 열면 그 매매의 근거 카드가 보인다", async () => {
    // 검증: V68 V69 C23
    const { panel, seen } = setup();
    expect(panel.element.dataset.role).toBe("basis-panel");
    expect(panel.element.hidden).toBe(true);
    await panel.openTrade(12);
    expect(panel.element.hidden).toBe(false);
    expect(seen.map((s) => s.url)).toEqual([tradeBasisPath(12)]);
    expect(tradeBasisPath(12)).toBe("/api/trades/12/basis");
    const card = panel.element.querySelector<HTMLElement>('[data-role="basis-body"] [data-role="basis-card"]');
    expect(card).not.toBeNull();
    expect(card?.querySelectorAll('[data-role="opinions"] > li').length).toBe(4);
    expect(panel.element.querySelector('[data-part="title"]')?.textContent).toContain("12");
  });

  it("논의 줄의 종목 표시를 누르면 그 종목의 가장 최근 매매의 근거 카드가 열린다", async () => {
    // 검증: V68 V69 F7-7
    const { log, panel, seen } = setup();
    const line = talk(log, "000660");
    const topic = line.querySelector<HTMLElement>(".topic");
    expect(topic?.dataset.symbol).toBe("000660");
    topic?.click();
    await settle();
    expect(seen.map((s) => s.path)).toEqual(["/api/trades", "/api/trades/12/basis"]);
    expect(panel.element.hidden).toBe(false);
    expect(panel.element.querySelector('[data-agent="scout"] [data-part="direction"]')?.textContent).toBe("기권");
  });

  it("Enter로도 열리고, 옛 매매는 '기록 없음', 닫기를 누르면 숨는다", async () => {
    // 검증: V69 E53
    const { log, panel } = setup();
    const topic = talk(log, "035420").querySelector<HTMLElement>(".topic");
    topic?.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    await settle();
    expect(panel.element.querySelector('[data-role="no-record"]')?.textContent).toBe("기록 없음");
    panel.element.querySelector<HTMLButtonElement>('button[data-action="close-basis"]')?.click();
    expect(panel.element.hidden).toBe(true);
  });

  it("하루 판단 줄의 표시는 종목이 아니라 누를 거리가 아니다", () => {
    // 검증: F7-7
    const { log } = setup();
    const topic = talk(log, "day").querySelector<HTMLElement>(".topic");
    expect(topic?.textContent).toBe("하루 판단");
    expect(topic?.dataset.symbol).toBeUndefined();
  });

  it("그 종목의 매매가 없으면 안내 글, 읽기에 실패하면 오류 줄이 보인다", async () => {
    // 검증: E53 C23
    const { panel } = setup();
    await panel.openSymbol("005930");
    expect(panel.element.hidden).toBe(false);
    expect(panel.element.querySelector('[data-role="basis-body"]')?.textContent).toContain("매매가 아직 없어요");
    const failing = setup({ failBasis: true });
    await failing.panel.openTrade(12);
    expect(failing.panel.element.querySelector('[role="alert"]')?.textContent).toContain("근거를 불러오지 못했어요");
  });

  it("사무실이 부르는 경로 표에 매매 목록과 근거 경로가 있다", () => {
    // 검증: C23
    expect(SCREEN_PATHS.office).toContain("/api/trades");
    expect(SCREEN_PATHS.office).toContain("/api/trades/{id}/basis");
  });
});
