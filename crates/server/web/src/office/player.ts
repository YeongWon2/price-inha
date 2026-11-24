// 내 캐릭터 걷기(server 32차 4번 · 5번, 결정 17): 방향키 상태 → 이동 벡터 · 한 프레임 걷기 · 막힘 · 길 따라 걷기.
// 원본 DeskRPG와 같게(docs/research/metaverse-office-refs.md B절): 방향키만(WASD 없음), 초당 3.75칸(120px/s ÷ 32px),
// 대각선은 각 축에 √½을 곱해 빠르기가 같고, 진행 방향 앞(0.375칸)과 몸통 반지름(0.22칸)을 보고 막히면 그 프레임
// 이동은 0이다(E40). 키를 누르면 길 따라가기를 버린다. 장면(three.js) 없이 칸 좌표만 다루는 순수 모듈이다.
// 좌표는 칸 단위 실수(칸 (x, y)의 가운데는 (x + 0.5, y + 0.5)), y가 늘면 아래(화면 앞쪽, 원본 지도와 같다).

import type { Grid, Point } from "./nav";

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

/**
 * 눌린 방향키의 상태. `press` · `release`는 방향키일 때만 바꾸고 바꿨는지(방향키였는지)를 돌려준다.
 * `clear`는 모두 놓은 것으로 본다(창이 초점을 잃을 때 — E41).
 */
export class ArrowState {
  press(key: string): boolean {
    void key;
    throw new Error("todo");
  }

  release(key: string): boolean {
    void key;
    throw new Error("todo");
  }

  clear(): void {
    throw new Error("todo");
  }

  /** 지금 눌린 방향(새 객체). */
  held(): Held {
    throw new Error("todo");
  }

  /** 하나라도 눌려 있는가. */
  any(): boolean {
    throw new Error("todo");
  }
}

/**
 * 눌린 방향 → 이동 방향(길이 1 또는 0). 한 축만이면 그 축 ±1, 두 축이면 각 축 ±√½(대각선도 같은 빠르기).
 * 맞선 두 키(왼쪽 + 오른쪽)는 그 축이 0이다.
 */
export function moveVector(held: Held): Point {
  void held;
  throw new Error("todo");
}

/** 이동 방향 → 바라보는 방향(큰 축 기준). 움직이지 않으면 `current` 그대로. */
export function facingOf(move: Point, current: Facing): Facing {
  void move;
  void current;
  throw new Error("todo");
}

/**
 * 방향키로 한 프레임 걷는다: `pos`에서 `moveVector(held) × PLAYER_SPEED × dtSeconds`만큼. 가려는 곳(진행 방향 앞
 * [`PROBE_AHEAD`] · 몸통 반지름)이 `grid`에서 걸을 수 없으면 그 프레임은 움직이지 않는다(E40). 새 자리를 돌려준다.
 */
export function stepArrows(grid: Grid, pos: Point, held: Held, dtSeconds: number): Point {
  void grid;
  void pos;
  void held;
  void dtSeconds;
  throw new Error("todo");
}

/**
 * 길(칸 가운데 점 목록)을 따라 `PLAYER_SPEED × dtSeconds`만큼 걷는다. 지난 점은 빼고 남은 길을 돌려준다
 * (끝까지 왔으면 `rest`가 비고 `pos`는 마지막 점).
 */
export function stepPath(pos: Point, path: readonly Point[], dtSeconds: number): { pos: Point; rest: Point[] } {
  void pos;
  void path;
  void dtSeconds;
  throw new Error("todo");
}

/**
 * 내 캐릭터의 걷기 상태: 자리 · 바라보는 방향 · 따라가는 길.
 * - `walkTo(cell)`: 그 칸까지 길을 찾아(막힌 칸을 피해) 따라가기 시작한다. 길이 없으면 `false`이고 움직이지 않는다.
 * - `step(held, dt)`: 방향키가 하나라도 눌려 있으면 길을 버리고 방향키로 걷는다. 아니면 남은 길을 따라 걷는다.
 */
export class PlayerWalker {
  constructor(grid: Grid, start: Point, facing: Facing = "down") {
    void grid;
    void start;
    void facing;
    throw new Error("todo");
  }

  pos(): Point {
    throw new Error("todo");
  }

  facing(): Facing {
    throw new Error("todo");
  }

  /** 지금 있는 칸(정수). */
  cell(): Point {
    throw new Error("todo");
  }

  /** 길을 따라가는 중인가. */
  following(): boolean {
    throw new Error("todo");
  }

  walkTo(cell: Point): boolean {
    void cell;
    throw new Error("todo");
  }

  /** 걷지 않는 칸 표를 바꾼다(담당이 움직여 막힌 칸이 바뀔 때). */
  setGrid(grid: Grid): void {
    void grid;
    throw new Error("todo");
  }

  step(held: Held, dtSeconds: number): void {
    void held;
    void dtSeconds;
    throw new Error("todo");
  }
}
