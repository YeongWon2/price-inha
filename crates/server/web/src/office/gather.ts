// 내 앞으로 모이기(server 59차 · 결정 29, F10 2b1 · 4a2 · 4c4): 멘션하거나 말을 걸면 담당이 내 곁으로 온다.
// - 자리 나누기: 여럿이 와도 겹치지 않게 내 둘레의 빈 칸을 가까운 고리부터 하나씩 나눠 준다 — 내 칸에서 맨해튼 거리 1
//   고리(네 이웃: 위 · 아래 · 왼쪽 · 오른쪽 — 32차 `besideCell`과 같은 차례) → 2 고리 → … [`GATHER_RINGS`] 고리까지.
//   같은 고리 안에서는 그 담당에게서 길이 가장 짧은 칸(같으면 고리 차례가 앞인 칸). 다른 담당이 서 있거나 이미 다른
//   담당에게 잡힌 칸(`taken`)은 건너뛴다. 원본 DeskRPG는 네 이웃 하나만 본다(metaverse-office-refs.md C절) — 바깥 고리는
//   결정 29의 확장(고리 차례는 조정자 해석).
// - 말 걸기에서 부를지: 이미 부름으로 오는 중 · 곁에서 기다림이면 부르지 않고, 자리에 있는데 이미 곁(2칸 안)이어도
//   부르지 않는다. 돌아가는 중이거나 멘션으로 왔던 담당(1:1로 가면 돌아간다)은 다시 부른다.
// 장면 없이 칸 좌표만 다루는 순수 모듈.

import type { Grid, Point } from "./nav";
import { lengthOf, pathTo } from "./pathfind";
import type { SummonPhase, SummonReason } from "./summon-view";

/** 곁 칸을 찾는 가장 바깥 고리(여섯 담당이 모두 와도 네 이웃 + 2 고리 여덟 칸으로 넉넉하다). */
export const GATHER_RINGS = 3;

/** 네 이웃(위 · 아래 · 왼쪽 · 오른쪽 — `besideCell`과 같은 차례). */
const NEIGHBORS: readonly Point[] = [
  { x: 0, y: -1 },
  { x: 0, y: 1 },
  { x: -1, y: 0 },
  { x: 1, y: 0 },
];

/** 칸 `center`에서 맨해튼 거리 `k`(1 이상)인 칸들. 1이면 네 이웃 차례, 그 밖은 위 줄부터 왼쪽에서 오른쪽으로. */
export function ringCells(center: Point, k: number): Point[] {
  if (k === 1) return NEIGHBORS.map((d) => ({ x: center.x + d.x, y: center.y + d.y }));
  const cells: Point[] = [];
  for (let dy = -k; dy <= k; dy += 1) {
    const dx = k - Math.abs(dy);
    cells.push({ x: center.x - dx, y: center.y + dy });
    if (dx !== 0) cells.push({ x: center.x + dx, y: center.y + dy });
  }
  return cells;
}

/**
 * 담당(칸 `from`)이 내 칸 `player` 둘레에서 설 칸: 가까운 고리부터, 같은 고리에서는 `from`에서 길이 가장 짧은 칸.
 * 걸을 수 없거나 `taken`에 든 칸 · 길이 없는 칸은 건너뛴다. [`GATHER_RINGS`] 고리 안에 없으면 `null`.
 */
export function gatherCell(grid: Grid, from: Point, player: Point, taken: readonly Point[] = []): Point | null {
  const held = new Set(taken.map((c) => `${c.x},${c.y}`));
  for (let k = 1; k <= GATHER_RINGS; k += 1) {
    let best: { cell: Point; length: number } | null = null;
    for (const cell of ringCells(player, k)) {
      if (held.has(`${cell.x},${cell.y}`) || !grid.walkable(cell.x, cell.y)) continue;
      const path = pathTo(grid, from, cell);
      if (path === null) continue;
      const length = lengthOf(from, path);
      if (best === null || length < best.length) best = { cell, length };
    }
    if (best !== null) return best.cell;
  }
  return null;
}

/**
 * 말을 걸 때(누르기 · 목록 · 탭 · `/`로 그 담당의 1:1을 열 때) 그 담당을 부르는가. `near`는 이미 곁(말 걸 거리 안)인가.
 * - 부름(`call`)으로 오는 중 · 곁에서 기다림이면 부르지 않는다.
 * - 멘션으로 왔던 담당 · 돌아가는 중이면 부른다.
 * - 자리에 있으면(단계 없음 포함) 곁이 아닐 때만 부른다.
 */
export function talkCalls(phase: SummonPhase | null, reason: SummonReason | null, near: boolean): boolean {
  if (phase === "called" || phase === "waiting") return reason !== "call";
  if (phase === "returning") return true;
  return !near;
}
