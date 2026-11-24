// 지나가며 잡담(server 22차 4번, 원본 DeskRPG `npc-smalltalk` 동작 — docs/research/deskrpg.md 10절): 조건(둘 다 한가 ·
// 한쪽 이상 걷는 중 · 거리 1.8 이하 · 서로 보임 · 대화 · 상태 말풍선 없음), 말하는 시간(먼저 0~4.5초 · 답 2~6.5초) ·
// 멈춤 7.5초, 쉬는 시간(담당 45초 · 같은 쌍 90초 · 전체 12초), 바로 앞 대사 피하기, 우리 대사 16쌍.
// 화면만의 일이라 서버를 부르지 않는다. 시각은 시험이 넘기는 밀리초(가짜 시계)다.
import { describe, expect, it } from "vitest";
import { SMALLTALK_LINES, SMALLTALK_RULES, Smalltalk, type Walker } from "../../web/src/office/smalltalk";

const sources = import.meta.glob<string>("../../web/src/office/smalltalk.ts", {
  query: "?raw",
  import: "default",
  eager: true,
});

const T0 = 5_000_000;
const SEE = () => true;
const fixed = () => 0;

/** 한가하고 걷는 중인 담당(바쁘지 않다). */
function walker(id: string, x: number, z = 0, more: Partial<Walker> = {}): Walker {
  return { id, x, z, idle: true, walking: true, busy: false, ...more };
}

/** 그 두 담당끼리의 잡담인가. */
function between(exchange: { first: string; second: string } | null, a: string, b: string): boolean {
  if (!exchange) return false;
  return [exchange.first, exchange.second].sort().join() === [a, b].sort().join();
}

describe("잡담 대사", () => {
  it("우리 대사 16쌍이 있고 질문 · 답 모두 한국어이며 질문이 겹치지 않는다", () => {
    // 검증: V15
    expect(SMALLTALK_LINES.length).toBe(16);
    for (const line of SMALLTALK_LINES) {
      expect(/[가-힣]/.test(line.ask), line.ask).toBe(true);
      expect(/[가-힣]/.test(line.answer), line.answer).toBe(true);
      expect(line.ask.trim().length).toBeGreaterThan(0);
      expect(line.answer.trim().length).toBeGreaterThan(0);
    }
    expect(new Set(SMALLTALK_LINES.map((l) => l.ask)).size).toBe(16);
  });

  it("서버 · 기록 · AI에 보내지 않는다(화면만)", () => {
    // 검증: D11 D1
    const text = Object.values(sources).join("\n");
    expect(text.length).toBeGreaterThan(0);
    for (const word of ["fetch(", "XMLHttpRequest", "WebSocket", "/api/", "sendBeacon"]) {
      expect(text.includes(word), word).toBe(false);
    }
  });

  it("수치는 원본과 같다", () => {
    // 검증: V15
    expect(SMALLTALK_RULES).toEqual({
      range: 1.8,
      firstUntilMs: 4_500,
      replyFromMs: 2_000,
      replyUntilMs: 6_500,
      pauseMs: 7_500,
      personalMs: 45_000,
      pairMs: 90_000,
      globalMs: 12_000,
    });
  });
});

describe("잡담 조건", () => {
  it("둘 다 한가하고 한쪽이 걷는 중이며 거리 1.8 이하이고 서로 보이면 시작한다", () => {
    // 검증: V15
    const talk = new Smalltalk(fixed);
    const started = talk.tick([walker("news", 0), walker("risk", 1.8, 0, { walking: false })], SEE, T0);
    expect(between(started, "news", "risk")).toBe(true);
    expect(started?.start).toBe(T0);
  });

  it("거리가 1.8을 넘으면 하지 않는다", () => {
    // 검증: V15
    const talk = new Smalltalk(fixed);
    expect(talk.tick([walker("news", 0), walker("risk", 1.3, 1.3)], SEE, T0)).toBeNull();
  });

  it("둘 다 서 있으면 하지 않는다", () => {
    // 검증: V15
    const talk = new Smalltalk(fixed);
    const still = { walking: false };
    expect(talk.tick([walker("news", 0, 0, still), walker("risk", 1, 0, still)], SEE, T0)).toBeNull();
  });

  it("한쪽이라도 일하는 중이면 하지 않는다", () => {
    // 검증: V15
    const talk = new Smalltalk(fixed);
    expect(talk.tick([walker("news", 0), walker("risk", 1, 0, { idle: false })], SEE, T0)).toBeNull();
  });

  it("서로 보이지 않으면 하지 않는다", () => {
    // 검증: V15
    const talk = new Smalltalk(fixed);
    expect(talk.tick([walker("news", 0), walker("risk", 1)], () => false, T0)).toBeNull();
  });

  it("대화 · 상태 말풍선이 있으면 하지 않는다", () => {
    // 검증: V14 V15
    const talk = new Smalltalk(fixed);
    expect(talk.tick([walker("news", 0, 0, { busy: true }), walker("risk", 1)], SEE, T0)).toBeNull();
  });
});

describe("말하는 시간과 멈춤", () => {
  it("먼저 말한 쪽은 질문을 0~4.5초, 답하는 쪽은 답을 2~6.5초, 둘 다 7.5초 멈춘다", () => {
    // 검증: V15
    const talk = new Smalltalk(fixed);
    const ex = talk.tick([walker("news", 0), walker("risk", 1)], SEE, T0);
    expect(ex).not.toBeNull();
    const { first, second, line } = ex as NonNullable<typeof ex>;
    const { ask, answer } = SMALLTALK_LINES[line];
    const at = (ms: number) => talk.speaking(T0 + ms);

    expect(at(0)).toEqual([{ agent: first, text: ask }]);
    expect(at(1_900)).toEqual([{ agent: first, text: ask }]);
    expect(at(2_100)).toEqual(
      expect.arrayContaining([
        { agent: first, text: ask },
        { agent: second, text: answer },
      ]),
    );
    expect(at(4_400).some((s) => s.agent === first)).toBe(true);
    expect(at(4_600)).toEqual([{ agent: second, text: answer }]);
    expect(at(6_400)).toEqual([{ agent: second, text: answer }]);
    expect(at(6_600)).toEqual([]);

    for (const id of [first, second]) {
      expect(talk.paused(id, T0 + 7_400), id).toBe(true);
      expect(talk.paused(id, T0 + 7_600), id).toBe(false);
    }
  });
});

describe("쉬는 시간", () => {
  it("전체는 12초 쉰다", () => {
    // 검증: V15
    const talk = new Smalltalk(fixed);
    expect(talk.tick([walker("news", 0), walker("risk", 1)], SEE, T0)).not.toBeNull();
    const others = [walker("news", 50), walker("risk", 60), walker("review", 0), walker("trader", 1)];
    expect(talk.tick(others, SEE, T0 + 11_900)).toBeNull();
    expect(between(talk.tick(others, SEE, T0 + 12_100), "review", "trader")).toBe(true);
  });

  it("담당마다 45초 쉰다", () => {
    // 검증: V15
    const talk = new Smalltalk(fixed);
    expect(talk.tick([walker("news", 0), walker("risk", 1)], SEE, T0)).not.toBeNull();
    const withNews = [walker("news", 0), walker("risk", 60), walker("review", 1)];
    expect(talk.tick(withNews, SEE, T0 + 13_000)).toBeNull();
    expect(talk.tick(withNews, SEE, T0 + 44_900)).toBeNull();
    expect(between(talk.tick(withNews, SEE, T0 + 45_100), "news", "review")).toBe(true);
  });

  it("같은 쌍은 90초 쉰다", () => {
    // 검증: V15
    const talk = new Smalltalk(fixed);
    const pair = [walker("news", 0), walker("risk", 1)];
    expect(talk.tick(pair, SEE, T0)).not.toBeNull();
    expect(talk.tick(pair, SEE, T0 + 46_000)).toBeNull();
    expect(talk.tick(pair, SEE, T0 + 89_900)).toBeNull();
    expect(between(talk.tick(pair, SEE, T0 + 90_100), "news", "risk")).toBe(true);
  });
});

describe("바로 앞 대사 피하기", () => {
  it("다음 잡담은 바로 앞과 다른 대사를 고른다", () => {
    // 검증: V15
    const talk = new Smalltalk(fixed);
    const first = talk.tick([walker("news", 0), walker("risk", 1)], SEE, T0);
    const second = talk.tick([walker("review", 0), walker("trader", 1)], SEE, T0 + 12_100);
    expect(first).not.toBeNull();
    expect(second).not.toBeNull();
    expect(second?.line).not.toBe(first?.line);
    expect(second?.line).toBeGreaterThanOrEqual(0);
    expect(second?.line).toBeLessThan(16);
  });
});
