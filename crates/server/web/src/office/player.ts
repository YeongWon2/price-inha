// 내 캐릭터 걷기(server 32차 4번 · 5번, 결정 17): 방향키 상태 → 이동 벡터 · 한 프레임 걷기 · 막힘 · 길 따라 걷기.
// 원본 DeskRPG와 같게(docs/research/metaverse-office-refs.md B절): 방향키만(WASD 없음), 초당 3.75칸(120px/s ÷ 32px),
// 대각선은 각 축에 √½을 곱해 빠르기가 같고, 진행 방향 앞(0.375칸)과 몸통 반지름(0.22칸)을 보고 막히면 그 프레임
// 이동은 0이다(E40). 키를 누르면 길 따라가기를 버린다. 장면(three.js) 없이 칸 좌표만 다루는 순수 모듈이다.
// 좌표는 칸 단위 실수(칸 (x, y)의 가운데는 (x + 0.5, y + 0.5)), y가 늘면 아래(화면 앞쪽, 원본 지도와 같다).

import { ACTOR_RADIUS, type Grid, type Point } from "./nav";
import { cellOf, pathTo } from "./pathfind";

/** 바라보는 방향(서버 `dir`와 같은 글자). */
export type Facing = "up" | "down" | "left" | "right";

/** 캐릭터를 움직이는 키: 방향키 넷뿐이다(WASD는 움직이지 않는다 — 결정 17, 원본 그대로). */
export const ARROW_KEYS = ["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight"] as const;
export type ArrowKey = (typeof ARROW_KEYS)[number];

/** 걷는 빠르기(칸/초). 원본 `PLAYER_SPEED` 120px/s ÷ 칸 32px. */
export const PLAYER_SPEED = 3.75;
/** 방향키 이동이 진행 방향으로 미리 보는 거리(칸, 원본 12px ÷ 32px). */
export const PROBE_AHEAD = 0.375;

/** 지금 눌린 방향. */
export interface Held {
  up: boolean;
  down: boolean;
  left: boolean;
  right: boolean;
}

/** 아무것도 눌리지 않음. */
export const NO_KEYS: Readonly<Held> = { up: false, down: false, left: false, right: false };

/** 방향키 → 방향 이름. */
const KEY_DIR: Readonly<Record<ArrowKey, keyof Held>> = {
  ArrowUp: "up",
  ArrowDown: "down",
  ArrowLeft: "left",
  ArrowRight: "right",
};

function isArrow(key: string): key is ArrowKey {
  return (ARROW_KEYS as readonly string[]).includes(key);
}

/**
 * 눌린 방향키의 상태. `press` · `release`는 방향키일 때만 바꾸고 바꿨는지(방향키였는지)를 돌려준다.
 * `clear`는 모두 놓은 것으로 본다(창이 초점을 잃을 때 — E41).
 */
export class ArrowState {
  private state: Held = { ...NO_KEYS };

  press(key: string): boolean {
    if (!isArrow(key)) return false;
    this.state[KEY_DIR[key]] = true;
    return true;
  }

  release(key: string): boolean {
    if (!isArrow(key)) return false;
    this.state[KEY_DIR[key]] = false;
    return true;
  }

  clear(): void {
    this.state = { ...NO_KEYS };
  }

  /** 지금 눌린 방향(새 객체). */
  held(): Held {
    return { ...this.state };
  }

  /** 하나라도 눌려 있는가. */
  any(): boolean {
    return this.state.up || this.state.down || this.state.left || this.state.right;
  }
}

/**
 * 눌린 방향 → 이동 방향(길이 1 또는 0). 한 축만이면 그 축 ±1, 두 축이면 각 축 ±√½(대각선도 같은 빠르기).
 * 맞선 두 키(왼쪽 + 오른쪽)는 그 축이 0이다.
 */
export function moveVector(held: Held): Point {
  const x = (held.right ? 1 : 0) - (held.left ? 1 : 0);
  const y = (held.down ? 1 : 0) - (held.up ? 1 : 0);
  if (x !== 0 && y !== 0) return { x: x * Math.SQRT1_2, y: y * Math.SQRT1_2 };
  return { x, y };
}

/** 이동 방향 → 바라보는 방향(큰 축 기준). 움직이지 않으면 `current` 그대로. */
export function facingOf(move: Point, current: Facing): Facing {
  if (move.x === 0 && move.y === 0) return current;
  if (Math.abs(move.x) > Math.abs(move.y)) return move.x > 0 ? "right" : "left";
  return move.y > 0 ? "down" : "up";
}

/** 몸통(반지름 [`ACTOR_RADIUS`])의 네 귀퉁이가 모두 걸을 수 있는 칸에 있는가. */
function bodyFits(grid: Grid, x: number, y: number): boolean {
  const r = ACTOR_RADIUS;
  for (const [ox, oy] of [[-r, -r], [r, -r], [-r, r], [r, r]]) {
    if (!grid.walkable(Math.floor(x + ox), Math.floor(y + oy))) return false;
  }
  return true;
}

/**
 * 방향키로 한 프레임 걷는다: `pos`에서 `moveVector(held) × PLAYER_SPEED × dtSeconds`만큼. 가려는 곳(진행 방향 앞
 * [`PROBE_AHEAD`] · 몸통 반지름)이 `grid`에서 걸을 수 없으면 그 프레임은 움직이지 않는다(E40). 새 자리를 돌려준다.
 */
export function stepArrows(grid: Grid, pos: Point, held: Held, dtSeconds: number): Point {
  const move = moveVector(held);
  if (move.x === 0 && move.y === 0) return pos;
  const dist = PLAYER_SPEED * dtSeconds;
  const next = { x: pos.x + move.x * dist, y: pos.y + move.y * dist };
  // 진행 방향 앞(원본 12px)의 칸이 막혔으면 그 프레임은 서고, 몸통이 새 자리에서 벽에 닿아도 선다.
  const probe = { x: pos.x + move.x * PROBE_AHEAD, y: pos.y + move.y * PROBE_AHEAD };
  if (!grid.walkable(Math.floor(probe.x), Math.floor(probe.y))) return pos;
  if (!bodyFits(grid, next.x, next.y)) return pos;
  return next;
}

/**
 * 길(칸 가운데 점 목록)을 따라 `PLAYER_SPEED × dtSeconds`만큼 걷는다. 지난 점은 빼고 남은 길을 돌려준다
 * (끝까지 왔으면 `rest`가 비고 `pos`는 마지막 점).
 */
export function stepPath(pos: Point, path: readonly Point[], dtSeconds: number): { pos: Point; rest: Point[] } {
  let here = pos;
  let budget = PLAYER_SPEED * dtSeconds;
  const rest = [...path];
  while (rest.length > 0 && budget > 0) {
    const goal = rest[0];
    const gap = Math.hypot(goal.x - here.x, goal.y - here.y);
    if (gap <= budget) {
      here = { x: goal.x, y: goal.y };
      budget -= gap;
      rest.shift();
    } else {
      const k = budget / gap;
      here = { x: here.x + (goal.x - here.x) * k, y: here.y + (goal.y - here.y) * k };
      budget = 0;
    }
  }
  return { pos: here, rest };
}

/**
 * 내 캐릭터의 걷기 상태: 자리 · 바라보는 방향 · 따라가는 길.
 * - `walkTo(cell)`: 그 칸까지 길을 찾아(막힌 칸을 피해) 따라가기 시작한다. 길이 없으면 `false`이고 움직이지 않는다.
 * - `step(held, dt)`: 방향키가 하나라도 눌려 있으면 길을 버리고 방향키로 걷는다. 아니면 남은 길을 따라 걷는다.
 */
export class PlayerWalker {
  private grid: Grid;
  private here: Point;
  private face: Facing;
  private path: Point[] = [];

  constructor(grid: Grid, start: Point, facing: Facing = "down") {
    this.grid = grid;
    this.here = { x: start.x, y: start.y };
    this.face = facing;
  }

  pos(): Point {
    return { x: this.here.x, y: this.here.y };
  }

  facing(): Facing {
    return this.face;
  }

  /** 지금 있는 칸(정수). */
  cell(): Point {
    return cellOf(this.here);
  }

  /** 길을 따라가는 중인가. */
  following(): boolean {
    return this.path.length > 0;
  }

  walkTo(cell: Point): boolean {
    const from = this.cell();
    const path = pathTo(this.grid, from, cell);
    if (path === null) return false;
    // 칸 가운데에서 벗어나 서 있으면 먼저 내 칸 가운데로 간다(첫 구간이 벽 모서리에 걸리지 않게).
    const middle = { x: from.x + 0.5, y: from.y + 0.5 };
    const off = Math.hypot(this.here.x - middle.x, this.here.y - middle.y) > 1e-6;
    this.path = off ? [middle, ...path] : path;
    return this.path.length > 0 || (from.x === cell.x && from.y === cell.y);
  }

  /** 걷지 않는 칸 표를 바꾼다(담당이 움직여 막힌 칸이 바뀔 때). */
  setGrid(grid: Grid): void {
    this.grid = grid;
  }

  step(held: Held, dtSeconds: number): void {
    const move = moveVector(held);
    if (move.x !== 0 || move.y !== 0) {
      this.path = [];
      this.face = facingOf(move, this.face);
      this.here = stepArrows(this.grid, this.here, held, dtSeconds);
      return;
    }
    if (this.path.length === 0) return;
    const before = this.here;
    const next = stepPath(this.here, this.path, dtSeconds);
    this.here = next.pos;
    this.path = next.rest;
    this.face = facingOf({ x: next.pos.x - before.x, y: next.pos.y - before.y }, this.face);
  }
}
