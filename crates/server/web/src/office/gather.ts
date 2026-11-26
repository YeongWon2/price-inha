// 내 앞으로 모이기(결정 29): 자리 나누기 · 말 걸기에서 부를지. 뼈대(red).

import type { Grid, Point } from "./nav";
import type { SummonPhase, SummonReason } from "./summon-view";

/** 곁 칸을 찾는 가장 바깥 고리. */
export const GATHER_RINGS = 3;

/** 칸 `center`에서 맨해튼 거리 `k`인 칸들. */
export function ringCells(center: Point, k: number): Point[] {
  void center;
  void k;
  return [];
}

/** 담당(칸 `from`)이 내 칸 `player` 둘레에서 설 칸. */
export function gatherCell(grid: Grid, from: Point, player: Point, taken: readonly Point[] = []): Point | null {
  void grid;
  void from;
  void player;
  void taken;
  return null;
}

/** 말을 걸 때 그 담당을 부르는가. */
export function talkCalls(phase: SummonPhase | null, reason: SummonReason | null, near: boolean): boolean {
  void phase;
  void reason;
  void near;
  return false;
}
