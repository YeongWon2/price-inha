// 하루 AI 잔량 바 · 논의 진행 단계(server 51차 · 차수 표 21번, C21 · C22 · E51 · E52 · N14 · V63 · V64).
// 계산(사용률 · 모드 · 정렬 · 끝난 담당 수)은 서버가 해서 내려 준다. 화면은 받은 값을 글 · 모양으로 바꿔 그린다.
// 모델(`budget.ts` · `progress.ts`)은 DOM 없이, 뷰(`budget-view.ts` · `progress-view.ts`)는 jsdom으로 본다.
import { beforeEach, describe, expect, it } from "vitest";
import { API, SCREEN_PATHS } from "../../web/src/paths";
import {
  KIND_TEAM_AI_USED,
  budgetBar,
  budgetRows,
  readBudget,
  type Budget,
} from "../../web/src/office/budget";
import { mountBudget, type Fetcher } from "../../web/src/office/budget-view";
import {
  KIND_OFFICE_PROGRESS,
  RETRY_MAX,
  STAGE_MARK,
  elapsedText,
  progressRows,
  progressSummary,
  readProgress,
} from "../../web/src/office/progress";
import { mountProgress } from "../../web/src/office/progress-view";

const NAMES: Record<string, string> = {
  news: "뉴스 공시 시세",
  strategy: "전략",
  risk: "리스크",
  review: "복기",
  trader: "매매",
  leader: "팀장",
};
const nameOf = (id: string): string => NAMES[id] ?? id;

function member(agent: string, used: number, share: number | null, ratio: number | null, level: string | null) {
  return { agent, used, share, ratio, level };
}

/** 상한 10 · 6번 쓴(정리 모드) 잔량. 담당 목록은 서버가 많이 쓴 순서로 준다. */
const WRAP_UP = {
  limit: 10,
  used: 6,
  reserve: 2,
  remaining: 4,
  mode: "wrap_up",
  ratio: 0.6,
  level: "mid",
  members: [
    member("news", 4, 1.33, 3.0, "high"),
    member("strategy", 2, 1.33, 1.5, "high"),
    member("leader", 1, 3.33, 0.3, "low"),
    member("review", 0, 1.33, 0, "low"),
  ],
};

const UNLIMITED = {
  limit: null,
  used: 3,
  reserve: 0,
  remaining: null,
  mode: "normal",
  ratio: null,
  level: null,
  members: [member("news", 3, null, null, null)],
};

function progress(over: Record<string, unknown> = {}) {
  return {
    room: "trade-7",
    topic: "새 공시 · 종목 005930",
    members: [
      { agent: "news", state: "done", retry_max: 3 },
      { agent: "strategy", state: "running", retry: 1, retry_max: 3 },
      { agent: "risk", state: "error", retry_max: 3 },
      { agent: "review", state: "waiting", retry_max: 3 },
      { agent: "trader", state: "waiting", retry_max: 3 },
      { agent: "leader", state: "waiting", retry_max: 3 },
    ],
    calls: 5,
    done: 1,
    started_at: 1_000,
    at: 13_000,
    ...over,
  };
}

describe("잔량 모델", () => {
  it("이벤트 이름은 team.ai_used, 경로는 /api/team/budget이고 사무실이 부르는 경로에 든다", () => {
    // 검증: C22
    expect(KIND_TEAM_AI_USED).toBe("team.ai_used");
    expect(API.teamBudget).toBe("/api/team/budget");
    expect(SCREEN_PATHS.office).toContain("/api/team/budget");
  });

  it("모양이 틀린 내용은 읽지 않는다", () => {
    // 검증: C22
    expect(readBudget(null)).toBeNull();
    expect(readBudget({ used: "3" })).toBeNull();
    expect(readBudget({ ...WRAP_UP, mode: "nope" })).toBeNull();
    expect(readBudget(WRAP_UP)?.members.map((m) => m.agent)).toEqual(["news", "strategy", "leader", "review"]);
  });

  it("상한이 있으면 쓴 수 / 상한과 예비분 구간을 그리고, 정리 모드면 글이 바뀐다", () => {
    // 검증: V64 E52 N14
    const bar = budgetBar(readBudget(WRAP_UP) as Budget);
    expect(bar.unlimited).toBe(false);
    expect(bar.text).toBe("AI 6/10");
    expect(bar.fill).toBeCloseTo(0.6);
    expect(bar.reserveFrom).toBeCloseTo(0.8);
    expect(bar.mode).toBe("wrap_up");
    expect(bar.modeText).toBe("정리 모드");
    expect(bar.level).toBe("mid");
    const leader = budgetBar(readBudget({ ...WRAP_UP, used: 8, mode: "leader_only", ratio: 0.8, level: "high" }) as Budget);
    expect(leader.modeText).toBe("팀장 몫만");
    const normal = budgetBar(readBudget({ ...WRAP_UP, used: 1, mode: "normal", ratio: 0.1, level: "low" }) as Budget);
    expect(normal.modeText).toBe("");
  });

  it("상한이 없으면 바 대신 상한 없음과 쓴 수만", () => {
    // 검증: V64 C22
    const bar = budgetBar(readBudget(UNLIMITED) as Budget);
    expect(bar.unlimited).toBe(true);
    expect(bar.fill).toBeNull();
    expect(bar.text).toBe("AI 상한 없음 · 오늘 3번");
  });

  it("담당별 표는 서버가 준 순서 그대로, 몫이 없으면 줄표다", () => {
    // 검증: V64 R5-12
    const rows = budgetRows(readBudget(WRAP_UP) as Budget, nameOf);
    expect(rows.map((r) => r.name)).toEqual(["뉴스 공시 시세", "전략", "팀장", "복기"]);
    expect(rows[0]).toMatchObject({ used: "4번", share: "1.33", percent: "300%", level: "high" });
    expect(rows[0].fill).toBe(1);
    expect(rows[2]).toMatchObject({ percent: "30%", level: "low" });
    const none = budgetRows(readBudget(UNLIMITED) as Budget, nameOf);
    expect(none[0]).toMatchObject({ used: "3번", share: "—", percent: "—", level: null, fill: null });
  });
});

describe("진행 단계 모델", () => {
  it("이벤트 이름은 office.progress, 다시 시도 상한은 3이다", () => {
    // 검증: C21 E51
    expect(KIND_OFFICE_PROGRESS).toBe("office.progress");
    expect(RETRY_MAX).toBe(3);
    expect(STAGE_MARK).toEqual({ waiting: "○", running: "●", done: "✓", error: "!" });
  });

  it("담당마다 대기 ○ · 진행 ● · 완료 ✓ · 오류 !, 다시 시도 중이면 다시 시도 n/3", () => {
    // 검증: V63 E51 R5-11
    const read = readProgress(progress());
    expect(read).not.toBeNull();
    const rows = progressRows(read!, nameOf);
    expect(rows.map((r) => [r.name, r.mark, r.label])).toEqual([
      ["뉴스 공시 시세", "✓", "완료"],
      ["전략", "●", "다시 시도 1/3"],
      ["리스크", "!", "오류"],
      ["복기", "○", "대기"],
      ["매매", "○", "대기"],
      ["팀장", "○", "대기"],
    ]);
  });

  it("아래 한 줄은 담당 n/6 · 호출 n · 경과 m:ss이고, 경과는 받은 뒤로 흐른다", () => {
    // 검증: V63
    const read = readProgress(progress())!;
    expect(progressSummary(read, 0)).toBe("담당 1/6 · 호출 5 · 경과 0:12");
    expect(progressSummary(read, 61_000)).toBe("담당 1/6 · 호출 5 · 경과 1:13");
    expect(elapsedText(-5)).toBe("0:00");
    expect(elapsedText(3_599_000)).toBe("59:59");
  });

  it("모양이 틀린 내용은 읽지 않는다", () => {
    // 검증: C21
    expect(readProgress(null)).toBeNull();
    expect(readProgress({ ...progress(), members: "x" })).toBeNull();
    expect(readProgress({ ...progress(), calls: "5" })).toBeNull();
    expect(readProgress({ ...progress(), members: [{ agent: "news", state: "sleeping", retry_max: 3 }] })).toBeNull();
  });
});

describe("잔량 바 화면", () => {
  let bar: HTMLElement;
  beforeEach(() => {
    document.body.replaceChildren();
    bar = document.createElement("header");
    document.body.appendChild(bar);
  });

  function answering(body: unknown): { fetcher: Fetcher; urls: string[] } {
    const urls: string[] = [];
    const fetcher: Fetcher = (url) => {
      urls.push(url);
      return Promise.resolve(new Response(JSON.stringify(body), { status: 200 }));
    };
    return { fetcher, urls };
  }

  it("처음에 잔량을 읽어 위 줄에 바를 그리고, 정리 모드면 모양 · 글이 바뀐다", async () => {
    // 검증: V64 E52 C22
    const { fetcher, urls } = answering(WRAP_UP);
    const view = mountBudget(bar, { fetcher, nameOf });
    await view.load();
    expect(urls).toEqual(["/api/team/budget"]);
    const button = bar.querySelector<HTMLButtonElement>("button.budget");
    expect(button).not.toBeNull();
    expect(button!.dataset.mode).toBe("wrap_up");
    expect(button!.dataset.level).toBe("mid");
    expect(button!.textContent).toContain("AI 6/10");
    expect(button!.textContent).toContain("정리 모드");
    expect(bar.querySelector<HTMLElement>(".budget-fill")!.style.width).toBe("60%");
    expect(bar.querySelector<HTMLElement>(".budget-reserve")!.style.left).toBe("80%");
  });

  it("상한이 없으면 바 대신 상한 없음, 이벤트가 오면 다시 그린다", async () => {
    // 검증: V64
    const { fetcher } = answering(WRAP_UP);
    const view = mountBudget(bar, { fetcher, nameOf });
    await view.load();
    view.apply(UNLIMITED);
    const button = bar.querySelector<HTMLButtonElement>("button.budget")!;
    expect(button.dataset.unlimited).toBe("true");
    expect(button.textContent).toContain("AI 상한 없음 · 오늘 3번");
    expect(bar.querySelector<HTMLElement>(".budget-track")!.hidden).toBe(true);
    view.apply({ nonsense: true });
    expect(button.textContent).toContain("AI 상한 없음 · 오늘 3번");
  });

  it("바를 누르면 담당별 사용 표가 열리고 다시 누르면 닫힌다", async () => {
    // 검증: V64 R5-12
    const { fetcher } = answering(WRAP_UP);
    const view = mountBudget(bar, { fetcher, nameOf });
    await view.load();
    const button = bar.querySelector<HTMLButtonElement>("button.budget")!;
    const table = bar.querySelector<HTMLElement>(".budget-table")!;
    expect(table.hidden).toBe(true);
    button.click();
    expect(table.hidden).toBe(false);
    expect(button.getAttribute("aria-expanded")).toBe("true");
    const rows = [...table.querySelectorAll("tbody tr")];
    expect(rows.map((r) => r.querySelector("th")?.textContent)).toEqual(["뉴스 공시 시세", "전략", "팀장", "복기"]);
    expect(rows[0].getAttribute("data-level")).toBe("high");
    expect(rows[0].textContent).toContain("4번");
    button.click();
    expect(table.hidden).toBe(true);
  });
});

describe("진행 단계 화면", () => {
  let host: HTMLElement;
  beforeEach(() => {
    document.body.replaceChildren();
    host = document.createElement("aside");
    document.body.appendChild(host);
  });

  it("논의가 없으면 숨기고, 진행이 오면 담당 여섯과 아래 한 줄을 그린다", () => {
    // 검증: V63 R5-11 C21
    const view = mountProgress(host, nameOf);
    const panel = host.querySelector<HTMLElement>(".progress")!;
    expect(panel.hidden).toBe(true);
    view.apply(progress(), 0);
    expect(panel.hidden).toBe(false);
    const items = [...panel.querySelectorAll("li")];
    expect(items).toHaveLength(6);
    expect(items[1].getAttribute("data-state")).toBe("running");
    expect(items[1].textContent).toContain("다시 시도 1/3");
    expect(items[2].textContent).toContain("!");
    expect(panel.querySelector(".progress-summary")!.textContent).toBe("담당 1/6 · 호출 5 · 경과 0:12");
    view.tick(2_000);
    expect(panel.querySelector(".progress-summary")!.textContent).toBe("담당 1/6 · 호출 5 · 경과 0:14");
  });

  it("끝난 논의(closed)는 마지막 상태를 보인 뒤 잠시 뒤에 숨기고, null이면 바로 숨긴다", () => {
    // 검증: C21 V63
    const view = mountProgress(host, nameOf);
    const panel = host.querySelector<HTMLElement>(".progress")!;
    view.apply(progress({ closed: true, done: 4 }), 0);
    expect(panel.hidden).toBe(false);
    view.tick(1_000);
    expect(panel.hidden).toBe(false);
    view.tick(60_000);
    expect(panel.hidden).toBe(true);
    view.apply(progress(), 0);
    expect(panel.hidden).toBe(false);
    view.apply(null, 0);
    expect(panel.hidden).toBe(true);
  });
});
