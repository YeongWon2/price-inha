// 내 캐릭터 길 찾기(server 32차 5번 · 3번, 9번): 바닥 누르기 · 담당 옆 빈 칸 · 처음 서는 자리.
// 길은 지금의 `nav.ts`(원본 DeskRPG A* 8방향 · 4096회 한도 · 꺾이는 점만)를 그대로 쓰고, 여기서는 담당이 선 칸을 막힌
// 칸으로 더하고 목표 칸을 고른다. 원본: 담당을 누르면 그 담당의 네 이웃 칸 가운데 걸을 수 있고 비어 있는 칸으로 걸어간다
// (docs/research/metaverse-office-refs.md C절). 장면 없이 칸 좌표만 다룬다.

import { findPath, type Grid, type Point } from "./nav";

/** 지도에서 입구를 읽는 부분(`office-map.json`의 `entrance`: 아래쪽 바깥벽의 빈 칸들). */
export interface EntranceMap {
  cols: number;
  rows: number;
  entrance?: { fromCol: number; toCol: number; row: number };
}

/** `grid`에 `cells`(담당이 선 칸)를 막힌 칸으로 더한 새 표. `grid`는 바꾸지 않는다. */
export function withBlockers(grid: Grid, cells: readonly Point[]): Grid {
  const taken = new Set(cells.map((c) => `${c.x},${c.y}`));
  return {
    cols: grid.cols,
    rows: grid.rows,
    walkable(x: number, y: number) {
      return grid.walkable(x, y) && !taken.has(`${x},${y}`);
    },
  };
}

/** 실수 자리 → 그 칸(정수, 내림). */
export function cellOf(pos: Point): Point {
  return { x: Math.floor(pos.x), y: Math.floor(pos.y) };
}

/**
 * 칸 `from`에서 칸 `to`까지의 길(칸 가운데 점, 마지막은 `to`의 가운데). 막힌 칸을 피해 돌아가고, 길이 없거나
 * `to`가 막혔으면 `null`(움직이지 않는다 — E40). 같은 칸이면 빈 길.
 */
export function pathTo(grid: Grid, from: Point, to: Point): Point[] | null {
  if (!grid.walkable(to.x, to.y)) return null;
  if (from.x === to.x && from.y === to.y) return [];
  const points = findPath(grid, from, to);
  if (points === null) return null;
  // 첫 점은 출발 칸 가운데다(이미 그 칸에 서 있다).
  return points.slice(1);
}

/** 길의 길이(칸): 출발 칸 가운데에서 점들을 이은 거리. */
function lengthOf(from: Point, path: readonly Point[]): number {
  let total = 0;
  let here = { x: from.x + 0.5, y: from.y + 0.5 };
  for (const p of path) {
    total += Math.hypot(p.x - here.x, p.y - here.y);
    here = p;
  }
  return total;
}

/** 네 이웃(위 · 아래 · 왼쪽 · 오른쪽). */
const NEIGHBORS: readonly Point[] = [
  { x: 0, y: -1 },
  { x: 0, y: 1 },
  { x: -1, y: 0 },
  { x: 1, y: 0 },
];

/**
 * 담당(칸 `agent`) 옆 빈 칸: 네 이웃(위 · 아래 · 왼쪽 · 오른쪽) 가운데 걸을 수 있고 `from`에서 길이 있는 칸 중
 * 길이 가장 짧은 칸. 없으면 `null`.
 */
export function besideCell(grid: Grid, from: Point, agent: Point): Point | null {
  let best: { cell: Point; length: number } | null = null;
  for (const d of NEIGHBORS) {
    const cell = { x: agent.x + d.x, y: agent.y + d.y };
    if (!grid.walkable(cell.x, cell.y)) continue;
    const path = pathTo(grid, from, cell);
    if (path === null) continue;
    const length = lengthOf(from, path);
    if (best === null || length < best.length) best = { cell, length };
  }
  return best === null ? null : best.cell;
}

/** 입구 칸: 입구 가운데 열, 입구 줄 바로 안쪽(한 줄 위). 지도에 입구가 없으면 지도 아래 가운데의 한 줄 위. */
export function entranceCell(map: EntranceMap): Point {
  const e = map.entrance;
  if (e) return { x: Math.floor((e.fromCol + e.toCol) / 2), y: e.row - 1 };
  return { x: Math.floor(map.cols / 2), y: map.rows - 2 };
}

/**
 * 처음 서는 칸(M8): 저장된 칸이 걸을 수 있으면 그 칸, 아니면(없거나 막혔으면) 입구, 입구도 막혔으면 입구에서
 * 가장 가까운 걸을 수 있는 칸.
 */
export function startCell(grid: Grid, saved: Point | null, entrance: Point): Point {
  if (saved && grid.walkable(saved.x, saved.y)) return { x: saved.x, y: saved.y };
  if (grid.walkable(entrance.x, entrance.y)) return { x: entrance.x, y: entrance.y };
  // 입구에서 맨해튼 거리 1, 2, …의 고리를 차례로 본다(같은 고리에서는 곧은 거리가 짧은 칸 먼저).
  const reach = grid.cols + grid.rows;
  for (let ring = 1; ring <= reach; ring += 1) {
    let best: Point | null = null;
    let bestDist = Infinity;
    for (let dx = -ring; dx <= ring; dx += 1) {
      const rest = ring - Math.abs(dx);
      for (const dy of rest === 0 ? [0] : [-rest, rest]) {
        const x = entrance.x + dx;
        const y = entrance.y + dy;
        if (!grid.walkable(x, y)) continue;
        const dist = Math.hypot(dx, dy);
        if (dist < bestDist) {
          best = { x, y };
          bestDist = dist;
        }
      }
    }
    if (best) return best;
  }
  return { x: entrance.x, y: entrance.y };
}
