// 말풍선 자리 고르기(server 22차 3번, 원본 DeskRPG `label-layout` 동작 — docs/research/deskrpg.md 10절): 머리 바로 위부터
// 위아래 · 좌우(폭의 0.55배) 3×3 후보 가운데 화면 안이고 다른 말풍선 · 이름표와 안 겹치는 첫 자리, 막히거나 넓이가
// 화면의 24%를 넘으면 말풍선 줄로. 화면 밖 담당은 숨긴다. 장면 없이 `placeBubbles`를 돌려 본다.
// 23차: 이름표가 발 아래로 간다(시험 담당의 이름표 자리를 고침). 위아래 간격 `높이 + 6` · 첫 자리 `max(8, …)` ·
// 줄 자리 · 자리 잡는 순서는 office-layout.test.ts.
import { describe, expect, it } from "vitest";
import {
  BUBBLE_HEAD_GAP,
  BUBBLE_SIDE_SHIFT,
  LANE_AREA_RATIO,
  placeBubbles,
  type HeadSpot,
  type Rect,
  type Size,
} from "../../web/src/office/speech";

const SCREEN: Size = { width: 1280, height: 800 };
const BUBBLE: Size = { width: 120, height: 40 };

/** 발 아래에 이름표가 있는 담당 하나(23차 1번: 이름표는 머리 아래가 아니라 발 투영점 10px 아래). */
function spot(agent: string, x: number, y: number, bubble: Size = BUBBLE, label?: Rect): HeadSpot {
  return { agent, x, y, bubble, label: label ?? { x: x - 30, y: y + 130, width: 60, height: 16 } };
}

/** 머리 바로 위 첫 자리. */
function above(s: HeadSpot): Rect {
  return {
    x: s.x - s.bubble.width / 2,
    y: s.y - s.bubble.height - BUBBLE_HEAD_GAP,
    width: s.bubble.width,
    height: s.bubble.height,
  };
}

function overlaps(a: Rect, b: Rect): boolean {
  return a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height;
}

function inside(r: Rect, screen: Size): boolean {
  return r.x >= 0 && r.y >= 0 && r.x + r.width <= screen.width && r.y + r.height <= screen.height;
}

/** 첫 자리에서 좌우로 비킨 양이 0 또는 ±폭×0.55인가. */
function sideShiftOk(rect: Rect, s: HeadSpot): boolean {
  const dx = rect.x - above(s).x;
  const shift = s.bubble.width * BUBBLE_SIDE_SHIFT;
  return [0, shift, -shift].some((v) => Math.abs(dx - v) < 1e-6);
}

describe("말풍선 자리 고르기", () => {
  it("막힌 것이 없으면 머리 바로 위(가운데 맞춤, 높이 + 8px 위)에 띄운다", () => {
    // 검증: V14 V15
    expect(BUBBLE_HEAD_GAP).toBe(8);
    const one = spot("risk", 640, 400);
    const result = placeBubbles([one], SCREEN);
    expect(result.placed).toEqual([{ agent: "risk", rect: above(one) }]);
    expect(result.lane).toEqual([]);
    expect(result.hidden).toEqual([]);
  });

  it("앞서 놓은 말풍선과 겹치면 3×3 후보 가운데 안 겹치는 다른 자리에 띄운다", () => {
    // 검증: V14 V15
    expect(BUBBLE_SIDE_SHIFT).toBe(0.55);
    const first = spot("risk", 640, 400);
    // 같은 줄의 오른쪽 후보(+66px)는 비어 있다 — 위아래 후보의 간격과 상관없이 자리가 있다.
    const second = spot("strategy", 700, 400);
    const result = placeBubbles([first, second], SCREEN);
    expect(result.lane).toEqual([]);
    expect(result.placed.map((p) => p.agent)).toEqual(["risk", "strategy"]);
    const [a, b] = result.placed.map((p) => p.rect);
    expect(a).toEqual(above(first));
    expect(b).not.toEqual(above(second));
    expect(overlaps(a, b)).toBe(false);
    expect(inside(b, SCREEN)).toBe(true);
    expect(sideShiftOk(b, second)).toBe(true);
  });

  it("다른 담당의 이름표를 덮지 않는다", () => {
    // 검증: V14 V15
    const me = spot("news", 640, 400);
    // 가운데 후보(580~700)에만 걸리고 좌우 후보(514~634 · 646~766)에는 걸리지 않는 이름표.
    const blocker = spot("leader", 900, 600, BUBBLE, { x: 635, y: 360, width: 10, height: 10 });
    const result = placeBubbles([me, blocker], SCREEN);
    const mine = result.placed.find((p) => p.agent === "news")?.rect;
    expect(mine).toBeDefined();
    expect(mine).not.toEqual(above(me));
    expect(overlaps(mine as Rect, blocker.label)).toBe(false);
    expect(overlaps(mine as Rect, me.label)).toBe(false);
    expect(sideShiftOk(mine as Rect, me)).toBe(true);
  });

  it("가운데 자리가 화면 밖이면 화면 안의 옆 자리로 비킨다", () => {
    // 검증: V15
    const edge = spot("trader", 30, 400);
    const result = placeBubbles([edge], SCREEN);
    const rect = result.placed[0]?.rect;
    expect(rect).toBeDefined();
    expect(inside(rect as Rect, SCREEN)).toBe(true);
    expect((rect as Rect).x).toBeCloseTo(above(edge).x + BUBBLE.width * BUBBLE_SIDE_SHIFT);
  });

  it("모든 후보가 막히면 말풍선 줄로 보낸다", () => {
    // 검증: V15
    const me = spot("review", 640, 400);
    const wall = spot("leader", 100, 700, { width: 20, height: 10 }, { x: 0, y: 0, width: 1280, height: 560 });
    const result = placeBubbles([me, wall], SCREEN);
    expect(result.lane).toContain("review");
    expect(result.placed.some((p) => p.agent === "review")).toBe(false);
  });

  it("말풍선 넓이 합이 화면의 24%를 넘으면 모두 말풍선 줄로 보낸다", () => {
    // 검증: V15
    expect(LANE_AREA_RATIO).toBe(0.24);
    const small: Size = { width: 400, height: 300 }; // 넓이 120000, 24% = 28800
    const big: Size = { width: 190, height: 80 }; // 15200
    const one = placeBubbles([spot("risk", 100, 200, big)], small);
    expect(one.lane).toEqual([]);
    expect(one.placed.length).toBe(1);
    const two = placeBubbles([spot("risk", 100, 200, big), spot("news", 300, 200, big)], small);
    expect(two.lane).toEqual(["risk", "news"]);
    expect(two.placed).toEqual([]);
  });

  it("머리가 화면 밖인 담당의 말풍선은 숨긴다", () => {
    // 검증: V15
    const result = placeBubbles([spot("risk", -50, 400), spot("news", 640, 900), spot("leader", 640, 400)], SCREEN);
    expect(result.hidden.sort()).toEqual(["news", "risk"]);
    expect(result.placed.map((p) => p.agent)).toEqual(["leader"]);
    expect(result.lane).toEqual([]);
  });
});
