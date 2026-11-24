// 내 캐릭터 걷기(server 32차 4번 · 5번, 결정 17 · V56 · E40): 방향키 상태 → 이동 벡터(대각선 보정), 초당 3.75칸,
// 막힌 칸(벽 · 가구 · 담당) 앞에서 멈춤, 길 따라 걷기, 걷는 중 방향키를 누르면 길을 버림.
// 원본 DeskRPG 수치(docs/research/metaverse-office-refs.md B절). 장면 없이 칸 좌표만 돌린다.
import { describe, expect, it } from "vitest";
import { ACTOR_RADIUS, buildGrid, type Grid, type Point } from "../../web/src/office/nav";
import { withBlockers } from "../../web/src/office/pathfind";
import {
  ARROW_KEYS,
  ArrowState,
  NO_KEYS,
  PLAYER_SPEED,
  PlayerWalker,
  facingOf,
  moveVector,
  stepArrows,
  stepPath,
  type Held,
} from "../../web/src/office/player";

const OPEN: Grid = buildGrid({ cols: 12, rows: 12 });
const keys = (on: Partial<Held>): Held => ({ ...NO_KEYS, ...on });

/** 점이 (x, y)에 가깝다(+0 · -0을 가리지 않는다). */
function near(p: Point, x: number, y: number): void {
  expect(p.x).toBeCloseTo(x, 9);
  expect(p.y).toBeCloseTo(y, 9);
}

/** `frames`번 `dt`초씩 방향키로 걷는다. */
function walk(grid: Grid, from: Point, held: Held, frames: number, dt: number): Point {
  let pos = from;
  for (let i = 0; i < frames; i += 1) pos = stepArrows(grid, pos, held, dt);
  return pos;
}

describe("방향키 상태", () => {
  it("방향키 넷만 누름으로 친다(WASD는 아니다)", () => {
    // 검증: V56
    expect([...ARROW_KEYS]).toEqual(["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight"]);
    const state = new ArrowState();
    for (const key of ["w", "a", "s", "d", "W", "A", "S", "D", "KeyW", " "]) {
      expect(state.press(key), key).toBe(false);
    }
    expect(state.any()).toBe(false);
    expect(state.held()).toEqual(NO_KEYS);
    expect(state.press("ArrowUp")).toBe(true);
    expect(state.press("ArrowLeft")).toBe(true);
    expect(state.held()).toEqual(keys({ up: true, left: true }));
    expect(state.release("ArrowUp")).toBe(true);
    expect(state.held()).toEqual(keys({ left: true }));
    expect(state.release("d")).toBe(false);
  });

  it("창이 초점을 잃으면 눌린 키를 모두 놓은 것으로 본다", () => {
    // 검증: V56 E41
    const state = new ArrowState();
    state.press("ArrowRight");
    state.press("ArrowDown");
    expect(state.any()).toBe(true);
    state.clear();
    expect(state.any()).toBe(false);
    expect(state.held()).toEqual(NO_KEYS);
  });
});

describe("이동 벡터", () => {
  it("한 축이면 길이 1, 위는 y가 준다", () => {
    // 검증: V56
    near(moveVector(keys({ right: true })), 1, 0);
    near(moveVector(keys({ left: true })), -1, 0);
    near(moveVector(keys({ up: true })), 0, -1);
    near(moveVector(keys({ down: true })), 0, 1);
  });

  it("두 키를 같이 누르면 대각선이고 각 축에 √½을 곱해 빠르기가 같다", () => {
    // 검증: V56
    const v = moveVector(keys({ up: true, right: true }));
    expect(v.x).toBeCloseTo(Math.SQRT1_2, 9);
    expect(v.y).toBeCloseTo(-Math.SQRT1_2, 9);
    expect(Math.hypot(v.x, v.y)).toBeCloseTo(1, 9);
    const w = moveVector(keys({ down: true, left: true }));
    expect(w.x).toBeCloseTo(-Math.SQRT1_2, 9);
    expect(w.y).toBeCloseTo(Math.SQRT1_2, 9);
  });

  it("맞선 두 키는 그 축이 0이고, 아무것도 안 누르면 움직이지 않는다", () => {
    // 검증: V56
    near(moveVector(keys({ left: true, right: true })), 0, 0);
    near(moveVector(keys({ left: true, right: true, up: true })), 0, -1);
    const none = moveVector(NO_KEYS);
    expect(Math.hypot(none.x, none.y)).toBe(0);
  });

  it("바라보는 방향은 큰 축을 따르고 멈추면 그대로다", () => {
    // 검증: V56
    expect(facingOf({ x: 1, y: 0 }, "down")).toBe("right");
    expect(facingOf({ x: -1, y: 0 }, "down")).toBe("left");
    expect(facingOf({ x: 0, y: -1 }, "down")).toBe("up");
    expect(facingOf({ x: 0, y: 1 }, "up")).toBe("down");
    expect(facingOf({ x: 0, y: 0 }, "left")).toBe("left");
  });
});

describe("방향키로 걷기", () => {
  it("초당 3.75칸으로 걷는다", () => {
    // 검증: V56
    expect(PLAYER_SPEED).toBe(3.75);
    const pos = stepArrows(OPEN, { x: 5.5, y: 5.5 }, keys({ right: true }), 0.1);
    expect(pos.x).toBeCloseTo(5.5 + 0.375, 9);
    expect(pos.y).toBeCloseTo(5.5, 9);
    const up = stepArrows(OPEN, { x: 5.5, y: 5.5 }, keys({ up: true }), 0.05);
    expect(up.y).toBeCloseTo(5.5 - 0.1875, 9);
  });

  it("대각선도 같은 빠르기다(더 빠르지 않다)", () => {
    // 검증: V56
    const from = { x: 5.5, y: 5.5 };
    const pos = stepArrows(OPEN, from, keys({ up: true, right: true }), 0.1);
    expect(Math.hypot(pos.x - from.x, pos.y - from.y)).toBeCloseTo(0.375, 9);
    expect(pos.x).toBeGreaterThan(from.x);
    expect(pos.y).toBeLessThan(from.y);
  });

  it("벽 앞에서 멈추고 벽 칸으로 들어가지 않는다", () => {
    // 검증: V56 E40
    const grid = buildGrid({ cols: 12, rows: 12, walls: [{ x: 7, y: 0, w: 1, h: 12 }] });
    const pos = walk(grid, { x: 5.5, y: 5.5 }, keys({ right: true }), 40, 0.05);
    expect(pos.x).toBeGreaterThan(6);
    expect(pos.x).toBeLessThanOrEqual(7 - ACTOR_RADIUS);
    expect(pos.y).toBeCloseTo(5.5, 9);
    // 반대쪽으로는 다시 걷는다.
    const back = stepArrows(grid, pos, keys({ left: true }), 0.1);
    expect(back.x).toBeCloseTo(pos.x - 0.375, 9);
  });

  it("가구 · 담당이 선 칸 앞에서도 멈춘다", () => {
    // 검증: V56 E40
    const furniture = buildGrid({ cols: 12, rows: 12, furniture: [{ x: 7, y: 5, w: 1, h: 1, blocks: true }] });
    const a = walk(furniture, { x: 5.5, y: 5.5 }, keys({ right: true }), 40, 0.05);
    expect(a.x).toBeLessThanOrEqual(7 - ACTOR_RADIUS);
    expect(a.x).toBeGreaterThan(6);
    const agents = withBlockers(OPEN, [{ x: 7, y: 5 }]);
    const b = walk(agents, { x: 5.5, y: 5.5 }, keys({ right: true }), 40, 0.05);
    expect(b.x).toBeLessThanOrEqual(7 - ACTOR_RADIUS);
    expect(b.x).toBeGreaterThan(6);
    // 막히지 않는 가구(의자)는 지나간다.
    const chair = buildGrid({ cols: 12, rows: 12, furniture: [{ x: 7, y: 5, w: 1, h: 1, blocks: false }] });
    const c = walk(chair, { x: 5.5, y: 5.5 }, keys({ right: true }), 10, 0.1);
    expect(c.x).toBeGreaterThan(8);
  });

  it("지도 밖으로 나가지 않는다", () => {
    // 검증: V56 E40
    const pos = walk(OPEN, { x: 0.5, y: 5.5 }, keys({ left: true }), 20, 0.1);
    expect(pos.x).toBeGreaterThanOrEqual(ACTOR_RADIUS - 1e-9);
    expect(pos.x).toBeLessThanOrEqual(0.5);
    const top = walk(OPEN, { x: 5.5, y: 0.5 }, keys({ up: true }), 20, 0.1);
    expect(top.y).toBeGreaterThanOrEqual(ACTOR_RADIUS - 1e-9);
  });
});

describe("길 따라 걷기", () => {
  it("길을 초당 3.75칸으로 따라가고 끝에 닿으면 남은 길이 빈다", () => {
    // 검증: V56
    const first = stepPath({ x: 5.5, y: 5.5 }, [{ x: 8.5, y: 5.5 }], 0.4);
    expect(first.pos.x).toBeCloseTo(7.0, 9);
    expect(first.pos.y).toBeCloseTo(5.5, 9);
    expect(first.rest).toEqual([{ x: 8.5, y: 5.5 }]);
    const done = stepPath(first.pos, first.rest, 1);
    near(done.pos, 8.5, 5.5);
    expect(done.rest).toEqual([]);
  });

  it("꺾이는 점을 지나면 그 점을 빼고 다음 점으로 이어 걷는다", () => {
    // 검증: V56
    const step = stepPath({ x: 5.5, y: 5.5 }, [{ x: 6.5, y: 5.5 }, { x: 6.5, y: 8.5 }], 1);
    expect(step.pos.x).toBeCloseTo(6.5, 9);
    expect(step.pos.y).toBeCloseTo(5.5 + 2.75, 9);
    expect(step.rest).toEqual([{ x: 6.5, y: 8.5 }]);
  });

  it("바닥을 누른 칸까지 막힌 칸을 피해 걸어가 그 칸에 선다", () => {
    // 검증: V56 E40
    const grid = buildGrid({ cols: 10, rows: 10, walls: [{ x: 5, y: 0, w: 1, h: 8 }] });
    const walker = new PlayerWalker(grid, { x: 2.5, y: 2.5 });
    expect(walker.walkTo({ x: 8, y: 2 })).toBe(true);
    expect(walker.following()).toBe(true);
    let frames = 0;
    while (walker.following() && frames < 400) {
      walker.step(NO_KEYS, 0.05);
      const here = walker.pos();
      expect(grid.walkable(Math.floor(here.x), Math.floor(here.y)), `${here.x},${here.y}`).toBe(true);
      frames += 1;
    }
    expect(walker.following()).toBe(false);
    expect(walker.cell()).toEqual({ x: 8, y: 2 });
  });

  it("길이 없는 곳을 누르면 움직이지 않는다", () => {
    // 검증: V56 E40
    const grid = buildGrid({
      cols: 12,
      rows: 12,
      walls: [
        { x: 7, y: 7, w: 3, h: 1 },
        { x: 7, y: 9, w: 3, h: 1 },
        { x: 7, y: 8, w: 1, h: 1 },
        { x: 9, y: 8, w: 1, h: 1 },
      ],
    });
    const walker = new PlayerWalker(grid, { x: 2.5, y: 2.5 });
    expect(walker.walkTo({ x: 8, y: 8 })).toBe(false);
    expect(walker.walkTo({ x: 7, y: 7 })).toBe(false);
    expect(walker.following()).toBe(false);
    walker.step(NO_KEYS, 0.5);
    near(walker.pos(), 2.5, 2.5);
  });

  it("걷는 중 방향키를 누르면 길을 버리고 방향키를 따른다", () => {
    // 검증: V56
    const walker = new PlayerWalker(OPEN, { x: 2.5, y: 2.5 });
    expect(walker.walkTo({ x: 10, y: 2 })).toBe(true);
    walker.step(NO_KEYS, 0.1);
    const mid = walker.pos();
    expect(mid.x).toBeGreaterThan(2.5);
    walker.step(keys({ down: true }), 0.1);
    expect(walker.following()).toBe(false);
    expect(walker.pos().x).toBeCloseTo(mid.x, 9);
    expect(walker.pos().y).toBeCloseTo(mid.y + 0.375, 9);
    expect(walker.facing()).toBe("down");
    // 키를 놓아도 버린 길로 돌아가지 않는다.
    const after = walker.pos();
    walker.step(NO_KEYS, 0.5);
    near(walker.pos(), after.x, after.y);
  });

  it("방향키로 걸으면 바라보는 방향이 바뀐다", () => {
    // 검증: V56
    const walker = new PlayerWalker(OPEN, { x: 5.5, y: 5.5 }, "down");
    expect(walker.facing()).toBe("down");
    walker.step(keys({ right: true }), 0.1);
    expect(walker.facing()).toBe("right");
    walker.step(keys({ up: true }), 0.1);
    expect(walker.facing()).toBe("up");
    walker.step(NO_KEYS, 0.1);
    expect(walker.facing()).toBe("up");
  });
});
