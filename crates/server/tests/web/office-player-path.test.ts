// 내 캐릭터 길 찾기(server 32차 5번 · 3번, F10 2a · 2b · M8 · E40): 막힌 칸을 피해 돌아가는 길, 길 없음, 담당 옆 빈 칸,
// 담당이 선 칸은 막힘, 처음 서는 칸(저장된 칸 → 막혔거나 없으면 입구 → 입구도 막혔으면 가장 가까운 빈 칸).
// 길은 지금의 nav.ts(원본 A*)를 쓴다. 장면 없이 칸 좌표만 돌린다.
import { describe, expect, it } from "vitest";
import { buildGrid, clearLine, type Grid, type NavMap, type Point } from "../../web/src/office/nav";
import {
  besideCell,
  cellOf,
  entranceCell,
  pathTo,
  startCell,
  withBlockers,
  type EntranceMap,
} from "../../web/src/office/pathfind";

const mapSource = import.meta.glob<string>("../../web/public/office/office-map.json", {
  query: "?raw",
  import: "default",
  eager: true,
});
const OFFICE = JSON.parse(Object.values(mapSource)[0]) as NavMap & EntranceMap;

const OPEN: Grid = buildGrid({ cols: 12, rows: 12 });
/** 가운데 세로벽(x 5, y 0~7) — 아래 두 줄(y 8 · 9)로만 지나간다. */
const WALLED: Grid = buildGrid({ cols: 10, rows: 10, walls: [{ x: 5, y: 0, w: 1, h: 8 }] });
/** (8, 8)을 벽으로 둘러쌌다. */
const BOXED: Grid = buildGrid({
  cols: 12,
  rows: 12,
  walls: [
    { x: 7, y: 7, w: 3, h: 1 },
    { x: 7, y: 9, w: 3, h: 1 },
    { x: 7, y: 8, w: 1, h: 1 },
    { x: 9, y: 8, w: 1, h: 1 },
  ],
});

const center = (c: Point): Point => ({ x: c.x + 0.5, y: c.y + 0.5 });

describe("바닥 누르기 길", () => {
  it("막힌 칸을 피해 돌아가는 길을 찾고 끝은 누른 칸 가운데다", () => {
    // 검증: V56 E40
    const path = pathTo(WALLED, { x: 2, y: 2 }, { x: 8, y: 2 });
    expect(path).not.toBeNull();
    const points = [center({ x: 2, y: 2 }), ...(path as Point[])];
    const last = points[points.length - 1];
    expect(last.x).toBeCloseTo(8.5, 9);
    expect(last.y).toBeCloseTo(2.5, 9);
    for (let i = 1; i < points.length; i += 1) {
      expect(clearLine(WALLED, points[i - 1], points[i]), `${i}번째 구간`).toBe(true);
    }
    expect(points.some((p) => p.y >= 8), "벽 아래로 돌아간다").toBe(true);
  });

  it("둘러싸인 칸 · 막힌 칸을 누르면 길이 없다", () => {
    // 검증: V56 E40
    expect(pathTo(BOXED, { x: 2, y: 2 }, { x: 8, y: 8 })).toBeNull();
    expect(pathTo(BOXED, { x: 2, y: 2 }, { x: 7, y: 7 })).toBeNull();
    expect(pathTo(OPEN, { x: 2, y: 2 }, { x: 20, y: 2 })).toBeNull();
  });

  it("지금 칸을 누르면 빈 길이다", () => {
    // 검증: V56
    expect(pathTo(OPEN, { x: 3, y: 4 }, { x: 3, y: 4 })).toEqual([]);
  });

  it("담당이 선 칸은 막힌 칸으로 친다(원래 표는 그대로)", () => {
    // 검증: E40
    const blocked = withBlockers(OPEN, [{ x: 7, y: 5 }, { x: 2, y: 9 }]);
    expect(blocked.walkable(7, 5)).toBe(false);
    expect(blocked.walkable(2, 9)).toBe(false);
    expect(blocked.walkable(6, 5)).toBe(true);
    expect(OPEN.walkable(7, 5)).toBe(true);
    expect(pathTo(blocked, { x: 2, y: 2 }, { x: 7, y: 5 })).toBeNull();
  });

  it("실수 자리는 내림한 칸이다", () => {
    // 검증: V56
    expect(cellOf({ x: 5.9, y: 2.1 })).toEqual({ x: 5, y: 2 });
    expect(cellOf({ x: 0.22, y: 11.99 })).toEqual({ x: 0, y: 11 });
  });
});

describe("담당 옆 빈 칸", () => {
  it("담당의 네 이웃 가운데 길이 가장 짧은 칸이다", () => {
    // 검증: V56
    const grid = withBlockers(OPEN, [{ x: 5, y: 5 }]);
    expect(besideCell(grid, { x: 1, y: 5 }, { x: 5, y: 5 })).toEqual({ x: 4, y: 5 });
    expect(besideCell(grid, { x: 10, y: 5 }, { x: 5, y: 5 })).toEqual({ x: 6, y: 5 });
  });

  it("가까운 이웃이 막혔으면 다른 빈 이웃으로 간다", () => {
    // 검증: V56 E40
    const base = buildGrid({ cols: 12, rows: 12, furniture: [{ x: 4, y: 5, w: 1, h: 1, blocks: true }] });
    const grid = withBlockers(base, [{ x: 5, y: 5 }]);
    const cell = besideCell(grid, { x: 1, y: 5 }, { x: 5, y: 5 });
    expect(cell).not.toBeNull();
    expect([JSON.stringify({ x: 5, y: 4 }), JSON.stringify({ x: 5, y: 6 })]).toContain(JSON.stringify(cell));
  });

  it("네 이웃이 모두 막혔거나 갈 수 없으면 없다", () => {
    // 검증: V56 E40
    const grid = withBlockers(OPEN, [
      { x: 5, y: 5 },
      { x: 4, y: 5 },
      { x: 6, y: 5 },
      { x: 5, y: 4 },
      { x: 5, y: 6 },
    ]);
    expect(besideCell(grid, { x: 1, y: 1 }, { x: 5, y: 5 })).toBeNull();
    // (8, 8) 담당은 둘러싸여 갈 수 없다.
    expect(besideCell(withBlockers(BOXED, [{ x: 8, y: 8 }]), { x: 1, y: 1 }, { x: 8, y: 8 })).toBeNull();
  });
});

describe("처음 서는 칸", () => {
  it("입구 칸은 입구 가운데 열의 한 줄 안쪽이다", () => {
    // 검증: V56 C17
    expect(entranceCell({ cols: 10, rows: 10, entrance: { fromCol: 2, toCol: 4, row: 9 } })).toEqual({ x: 3, y: 8 });
    expect(entranceCell({ cols: 10, rows: 10 })).toEqual({ x: 5, y: 8 });
  });

  it("사무실 지도의 입구 칸은 (37, 28)이고 걸을 수 있다(서버의 기본 자리와 같다)", () => {
    // 검증: V56 C17
    const cell = entranceCell(OFFICE);
    expect(cell).toEqual({ x: 37, y: 28 });
    expect(buildGrid(OFFICE).walkable(cell.x, cell.y)).toBe(true);
  });

  it("저장된 칸이 걸을 수 있으면 그 칸, 없거나 막혔으면 입구다", () => {
    // 검증: V56 C17
    const grid = buildGrid({ cols: 10, rows: 10, walls: [{ x: 2, y: 2, w: 1, h: 1 }] });
    const entrance = { x: 5, y: 8 };
    expect(startCell(grid, { x: 3, y: 3 }, entrance)).toEqual({ x: 3, y: 3 });
    expect(startCell(grid, null, entrance)).toEqual(entrance);
    expect(startCell(grid, { x: 2, y: 2 }, entrance)).toEqual(entrance);
    expect(startCell(grid, { x: 40, y: 3 }, entrance)).toEqual(entrance);
  });

  it("입구도 막혔으면 입구에서 가장 가까운 빈 칸이다", () => {
    // 검증: V56 C17
    const grid = buildGrid({ cols: 10, rows: 10, walls: [{ x: 5, y: 8, w: 1, h: 1 }] });
    const cell = startCell(grid, null, { x: 5, y: 8 });
    expect(grid.walkable(cell.x, cell.y)).toBe(true);
    expect(Math.abs(cell.x - 5) + Math.abs(cell.y - 8)).toBe(1);
  });
});
