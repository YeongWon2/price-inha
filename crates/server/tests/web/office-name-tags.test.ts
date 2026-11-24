// 이름표는 발 아래(server 23차 1번, 원본 DeskRPG `label-layout` 수치 — docs/research/deskrpg.md 11절): 발 투영점에서
// 10px 아래가 윗변 · 가로 가운데 · 화면 여백 8px 안 · 겹치면 아래로 `높이 + 5`씩 3단 × 좌우 0 · ±0.4폭 9후보(간격 4px) ·
// 우선순위 내림차순(같으면 id 순) · 안은 이름뿐. 장면 없이 `placeNameTags`를 돌려 본다.
import { describe, expect, it } from "vitest";
import {
  TAG_FOOT_GAP,
  TAG_MAX_WIDTH,
  TAG_PADDING,
  TAG_ROWS,
  TAG_SCREEN_MARGIN,
  TAG_SIDE_SHIFT,
  TAG_STACK_GAP,
  nameTagText,
  placeNameTags,
  type FootSpot,
} from "../../web/src/office/layout";
import type { Rect, Size } from "../../web/src/office/speech";

const SCREEN: Size = { width: 1280, height: 800 };
const TAG = { width: 60, height: 20 };

function foot(agent: string, x: number, y: number, priority = 0): FootSpot {
  return { agent, x, y, width: TAG.width, height: TAG.height, priority };
}

/** 발 바로 아래 첫 자리(여백 안으로 밀기 전). */
function below(f: FootSpot): Rect {
  return { x: f.x - f.width / 2, y: f.y + 10, width: f.width, height: f.height };
}

function rectOf(placed: { agent: string; rect: Rect }[], agent: string): Rect {
  const found = placed.find((p) => p.agent === agent);
  expect(found).toBeDefined();
  return (found as { rect: Rect }).rect;
}

/** 두 사각형 사이가 `gap`px 이상 떨어져 있는가(가로나 세로 어느 한쪽으로). */
function apart(a: Rect, b: Rect, gap: number): boolean {
  return (
    a.x + a.width + gap <= b.x ||
    b.x + b.width + gap <= a.x ||
    a.y + a.height + gap <= b.y ||
    b.y + b.height + gap <= a.y
  );
}

describe("이름표 자리", () => {
  it("막힌 것이 없으면 발 투영점에서 10px 아래가 윗변, 가로 가운데(원본 수치)", () => {
    // 검증: V15
    // 원본 수치: 발 아래 10px · 여백 8 · 한 단 높이 + 5 · 3단 · 좌우 0.4폭 · 간격 4 · 최대 폭 150.
    expect(TAG_FOOT_GAP).toBe(10);
    expect(TAG_SCREEN_MARGIN).toBe(8);
    expect(TAG_STACK_GAP).toBe(5);
    expect(TAG_ROWS).toBe(3);
    expect(TAG_SIDE_SHIFT).toBe(0.4);
    expect(TAG_PADDING).toBe(4);
    expect(TAG_MAX_WIDTH).toBe(150);
    const one = foot("risk", 640, 400);
    expect(placeNameTags([one], SCREEN)).toEqual([{ agent: "risk", rect: below(one) }]);
  });

  it("가로는 화면 여백 8px 안으로 민다(왼쪽 · 오른쪽)", () => {
    // 검증: V15
    const left = rectOf(placeNameTags([foot("risk", 10, 400)], SCREEN), "risk");
    expect(left.x).toBe(8);
    expect(left.y).toBe(410);
    const right = rectOf(placeNameTags([foot("news", 1275, 400)], SCREEN), "news");
    expect(right.x).toBe(1280 - 8 - TAG.width);
    expect(right.y).toBe(410);
  });

  it("같은 자리에 겹치면 아래로 높이 + 5씩 내려 3단까지 쌓는다", () => {
    // 검증: V15
    const step = TAG.height + 5;
    const placed = placeNameTags([foot("a", 640, 400), foot("b", 640, 400), foot("c", 640, 400)], SCREEN);
    const base = below(foot("a", 640, 400));
    expect(rectOf(placed, "a")).toEqual(base);
    expect(rectOf(placed, "b")).toEqual({ ...base, y: base.y + step });
    expect(rectOf(placed, "c")).toEqual({ ...base, y: base.y + 2 * step });
  });

  it("가운데 줄이 막히면 좌우로 0.4폭 비킨 후보를 쓴다", () => {
    // 검증: V15
    const me = foot("risk", 640, 400); // 첫 자리 x 610~670, y 410~430
    // 세 단의 가운데 · 왼쪽 후보(x 586~646)에 모두 걸리고 오른쪽 후보(x 634~694)에서는 4px 넘게 떨어진 좁은 기둥.
    const pillar: Rect = { x: 600, y: 405, width: 20, height: 100 };
    const rect = rectOf(placeNameTags([me], SCREEN, [pillar]), "risk");
    expect(rect).toEqual({ x: 610 + TAG.width * 0.4, y: 410, width: TAG.width, height: TAG.height });
  });

  it("4px보다 가까우면 겹친 것으로 본다", () => {
    // 검증: V15
    const me = foot("risk", 640, 400); // 첫 자리 x 610~670
    // 첫 자리 왼쪽 끝에서 2px 떨어진 것(겹치지는 않지만 간격 4보다 가깝다).
    const near: Rect = { x: 590, y: 410, width: 18, height: 20 };
    const rect = rectOf(placeNameTags([me], SCREEN, [near]), "risk");
    expect(rect).not.toEqual(below(me));
    expect(apart(rect, near, 4)).toBe(true);
    // 5px 떨어져 있으면 첫 자리 그대로.
    const far: Rect = { x: 587, y: 410, width: 18, height: 20 };
    expect(rectOf(placeNameTags([me], SCREEN, [far]), "risk")).toEqual(below(me));
  });

  it("놓은 이름표끼리는 4px 이상 떨어진다", () => {
    // 검증: V15
    const feet = [foot("a", 600, 400), foot("b", 630, 405), foot("c", 660, 395), foot("d", 900, 300)];
    const placed = placeNameTags(feet, SCREEN);
    expect(placed.map((p) => p.agent).sort()).toEqual(["a", "b", "c", "d"]);
    for (let i = 0; i < placed.length; i += 1) {
      for (let j = i + 1; j < placed.length; j += 1) {
        expect(apart(placed[i].rect, placed[j].rect, 4)).toBe(true);
      }
    }
  });

  it("우선순위가 높은 이름표가 먼저 자리를 잡는다(넘긴 순서와 상관없이)", () => {
    // 검증: V15
    const low = foot("alpha", 640, 400, 0);
    const high = foot("zeta", 640, 400, 80);
    const placed = placeNameTags([low, high], SCREEN);
    expect(placed.map((p) => p.agent)).toEqual(["zeta", "alpha"]);
    expect(rectOf(placed, "zeta")).toEqual(below(high));
    expect(rectOf(placed, "alpha")).not.toEqual(below(low));
  });

  it("우선순위가 같으면 id 순서로 놓는다", () => {
    // 검증: V15
    const placed = placeNameTags([foot("news", 640, 400, 50), foot("leader", 640, 400, 50)], SCREEN);
    expect(placed.map((p) => p.agent)).toEqual(["leader", "news"]);
    expect(rectOf(placed, "leader")).toEqual(below(foot("leader", 640, 400)));
  });

  it("이름표 안은 이름 글자뿐이다(생각 중 · 쉬는 중 아이콘을 붙이지 않는다)", () => {
    // 검증: V15
    expect(nameTagText("리스크 담당", "thinking", "desk")).toBe("리스크 담당");
    expect(nameTagText("리스크 담당", "idle", "bed")).toBe("리스크 담당");
    expect(nameTagText("팀장", "failed", "meeting")).toBe("팀장");
    expect(nameTagText("팀장", "queued", "desk")).toBe("팀장");
  });
});
