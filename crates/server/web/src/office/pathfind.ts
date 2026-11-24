// 내 캐릭터 길 찾기(server 32차 5번 · 3번, 9번): 바닥 누르기 · 담당 옆 빈 칸 · 처음 서는 자리.
// 길은 지금의 `nav.ts`(원본 DeskRPG A* 8방향 · 4096회 한도 · 꺾이는 점만)를 그대로 쓰고, 여기서는 담당이 선 칸을 막힌
// 칸으로 더하고 목표 칸을 고른다. 원본: 담당을 누르면 그 담당의 네 이웃 칸 가운데 걸을 수 있고 비어 있는 칸으로 걸어간다
// (docs/research/metaverse-office-refs.md C절). 장면 없이 칸 좌표만 다룬다.

import type { Grid, Point } from "./nav";

/** 지도에서 입구를 읽는 부분(`office-map.json`의 `entrance`: 아래쪽 바깥벽의 빈 칸들). */
export interface EntranceMap {
  cols: number;
  rows: number;
  entrance?: { fromCol: number; toCol: number; row: number };
}

/** `grid`에 `cells`(담당이 선 칸)를 막힌 칸으로 더한 새 표. `grid`는 바꾸지 않는다. */
export function withBlockers(grid: Grid, cells: readonly Point[]): Grid {
  void grid;
  void cells;
  throw new Error("todo");
}

/** 실수 자리 → 그 칸(정수, 내림). */
export function cellOf(pos: Point): Point {
  void pos;
  throw new Error("todo");
}

/**
 * 칸 `from`에서 칸 `to`까지의 길(칸 가운데 점, 마지막은 `to`의 가운데). 막힌 칸을 피해 돌아가고, 길이 없거나
 * `to`가 막혔으면 `null`(움직이지 않는다 — E40). 같은 칸이면 빈 길.
 */
export function pathTo(grid: Grid, from: Point, to: Point): Point[] | null {
  void grid;
  void from;
  void to;
  throw new Error("todo");
}

/**
 * 담당(칸 `agent`) 옆 빈 칸: 네 이웃(위 · 아래 · 왼쪽 · 오른쪽) 가운데 걸을 수 있고 `from`에서 길이 있는 칸 중
 * 길이 가장 짧은 칸. 없으면 `null`.
 */
export function besideCell(grid: Grid, from: Point, agent: Point): Point | null {
  void grid;
  void from;
  void agent;
  throw new Error("todo");
}

/** 입구 칸: 입구 가운데 열, 입구 줄 바로 안쪽(한 줄 위). 지도에 입구가 없으면 지도 아래 가운데의 한 줄 위. */
export function entranceCell(map: EntranceMap): Point {
  void map;
  throw new Error("todo");
}

/**
 * 처음 서는 칸(M8): 저장된 칸이 걸을 수 있으면 그 칸, 아니면(없거나 막혔으면) 입구, 입구도 막혔으면 입구에서
 * 가장 가까운 걸을 수 있는 칸.
 */
export function startCell(grid: Grid, saved: Point | null, entrance: Point): Point {
  void grid;
  void saved;
  void entrance;
  throw new Error("todo");
}
