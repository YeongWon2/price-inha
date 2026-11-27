// 3D 장면이 주 스레드를 막을 때 가볍게 하기(백로그 T 6번): 그리기 한 장이 아주 오래 걸리는 것이 몇 장 이어지면(소프트웨어
// 그리기 · 느린 기기) 곧바로 한 번 "가볍게"(그림자 끄기 · 해상도 최저)를 낸다 — 3초 창 두 개를 기다리는 해상도 낮춤보다
// 먼저. 그래야 탭 누르기 같은 입력이 그리기에 밀리지 않는다. 장면 없이 시험하는 순수 함수.
import { describe, expect, it } from "vitest";
import { SLOW_FRAME_MS, SLOW_FRAME_RUN, createFrameBudget } from "../../web/src/office/frame-budget";

describe("그리기 예산", () => {
  it("아주 느린 장이 이어지면 한 번만 가볍게 하라고 한다", () => {
    expect(SLOW_FRAME_MS).toBe(100);
    expect(SLOW_FRAME_RUN).toBe(5);
    const budget = createFrameBudget();
    const said: (string | null)[] = [];
    for (let n = 0; n < SLOW_FRAME_RUN; n += 1) { said.push(budget.note(160)); }
    expect(said.slice(0, SLOW_FRAME_RUN - 1).every((s) => s === null)).toBe(true);
    expect(said[SLOW_FRAME_RUN - 1]).toBe("lighten");
    for (let n = 0; n < SLOW_FRAME_RUN * 3; n += 1) { expect(budget.note(160)).toBeNull(); }
    expect(budget.lightened()).toBe(true);
  });

  it("빠른 장이 끼면 다시 센다", () => {
    const budget = createFrameBudget();
    for (let round = 0; round < 10; round += 1) {
      for (let n = 0; n < SLOW_FRAME_RUN - 1; n += 1) { expect(budget.note(150)).toBeNull(); }
      expect(budget.note(16)).toBeNull();
    }
    expect(budget.lightened()).toBe(false);
  });

  it("보통 기기의 장(60fps) · 한 번 긴 장(처음 그리기 준비)은 가볍게 하지 않는다", () => {
    const budget = createFrameBudget();
    expect(budget.note(900)).toBeNull();
    for (let n = 0; n < 600; n += 1) { expect(budget.note(16.7)).toBeNull(); }
    expect(budget.note(SLOW_FRAME_MS)).toBeNull();
    expect(budget.lightened()).toBe(false);
  });
});
