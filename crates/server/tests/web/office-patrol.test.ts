// 순찰(server 24차 1 · 2번, additions.md R5-13 · N16 · V65 — 원본 동작: docs/research/agent-office-refs.md 3-C · F 3,
// PixelOffice의 한가할 때 순찰): 한가한 지 20~45초 뒤 최대 3곳을 다녀오고(한 곳 3.0초, 산책 걷기 초당 55px ≈ 1.7칸),
// 진짜 일이 오면 바로 멈추고 자리로 간다. 화면만의 일이다(서버 · AI를 부르지 않는다). 가짜 시계(밀리초)와 넣어 주는
// 난수로 `src/office/patrol.ts`를 돌려 본다. 순찰 중 마주친 둘은 지나가며 잡담(smalltalk.ts)으로 말풍선을 주고받는다.
import { describe, expect, it } from "vitest";
import { buildGrid, type NavMap, type Point } from "../../web/src/office/nav";
import { PATROL_RULES, Patrol, patrolSpots, type PatrolAgent, type PatrolMap } from "../../web/src/office/patrol";
import { SMALLTALK_LINES, Smalltalk, type Walker } from "../../web/src/office/smalltalk";
import { SOURCES, mainOf, reach } from "./source-graph";

const maps = import.meta.glob<string>("../../web/public/office/office-map.json", {
  query: "?raw",
  import: "default",
  eager: true,
});
const OFFICE = "../../web/src/office/";
const T0 = 9_000_000;
const SPOTS: Point[] = [{ x: 2, y: 26 }, { x: 36, y: 26 }, { x: 38, y: 11 }, { x: 4, y: 13 }, { x: 20, y: 4 }];

function officeMap(): PatrolMap & NavMap {
  const texts = Object.values(maps);
  expect(texts.length).toBe(1);
  return JSON.parse(texts[0]) as PatrolMap & NavMap;
}

/** 늘 같은 값을 주는 난수. */
function fixed(value: number): () => number {
  return () => value;
}

/** 차례로 값을 주는 난수(다 쓰면 처음부터). */
function sequence(values: number[]): () => number {
  let i = 0;
  return () => values[i++ % values.length];
}

function free(...ids: string[]): PatrolAgent[] {
  return ids.map((id) => ({ id, free: true }));
}

describe("순찰 수치", () => {
  it("한가한 지 20~45초 뒤 · 최대 3곳 · 한 곳 3.0초 · 초당 55px(1.7칸) · 동시에 둘까지", () => {
    // 검증: V65 N16
    expect(PATROL_RULES.idleMinMs).toBe(20_000);
    expect(PATROL_RULES.idleMaxMs).toBe(45_000);
    expect(PATROL_RULES.maxStops).toBe(3);
    expect(PATROL_RULES.dwellMs).toBe(3_000);
    expect(PATROL_RULES.speed).toBeCloseTo(55 / 32, 6);
    expect(PATROL_RULES.speed).toBeCloseTo(1.7, 1);
    expect(PATROL_RULES.maxWalkers).toBe(2);
  });
});

describe("순찰 일정", () => {
  it("한가한 지 20초(난수 0) 전에는 나서지 않고, 되면 정한 곳 하나로 걷는다", () => {
    // 검증: V65 N16 R5-13
    const patrol = new Patrol(SPOTS, fixed(0));
    expect(patrol.tick(free("risk"), T0)).toEqual([]);
    expect(patrol.tick(free("risk"), T0 + 19_999)).toEqual([]);
    const orders = patrol.tick(free("risk"), T0 + 20_000);
    expect(orders).toHaveLength(1);
    expect(orders[0].agent).toBe("risk");
    expect(orders[0].cancel ?? false).toBe(false);
    expect(SPOTS).toContainEqual(orders[0].to);
    expect(patrol.patrolling("risk")).toBe(true);
  });

  it("난수가 1에 가까우면 45초 가까이 기다린다", () => {
    // 검증: V65 N16
    const patrol = new Patrol(SPOTS, fixed(0.999));
    patrol.tick(free("news"), T0);
    expect(patrol.tick(free("news"), T0 + 44_900)).toEqual([]);
    expect(patrol.tick(free("news"), T0 + 45_000)).toHaveLength(1);
  });

  it("최대 3곳을 서로 다르게 들르고 한 곳에 3.0초 머문 뒤 자리로 돌아간다", () => {
    // 검증: V65 N16 R5-13
    const patrol = new Patrol(SPOTS, fixed(0.99));
    patrol.tick(free("trader"), T0);
    let now = T0 + 45_000;
    const visited: Point[] = [];
    let order = patrol.tick(free("trader"), now)[0];
    for (let guard = 0; guard < 10 && order.to; guard++) {
      visited.push(order.to);
      now += 4_000;
      patrol.arrived("trader", now);
      expect(patrol.tick(free("trader"), now + 2_999), "3.0초 동안은 머문다").toEqual([]);
      now += 3_000;
      order = patrol.tick(free("trader"), now)[0];
      expect(order, "머문 뒤 다음 곳 또는 자리로").toBeDefined();
    }
    expect(order.to).toBeNull();
    expect(order.cancel ?? false).toBe(false);
    expect(visited.length).toBeGreaterThanOrEqual(1);
    expect(visited.length).toBeLessThanOrEqual(PATROL_RULES.maxStops);
    expect(new Set(visited.map((p) => `${p.x},${p.y}`)).size).toBe(visited.length);
    for (const p of visited) expect(SPOTS).toContainEqual(p);
    // 자리에 닿으면 다시 한가를 센다.
    patrol.arrived("trader", now + 5_000);
    expect(patrol.patrolling("trader")).toBe(false);
    expect(patrol.tick(free("trader"), now + 5_000 + 19_000)).toEqual([]);
  });

  it("순찰 중 일이 오면(회의 · 부름 · 멘션 · 일하는 중) 바로 멈추고 자리로 돌아가게 한다", () => {
    // 검증: V65 N16 R5-13
    const patrol = new Patrol(SPOTS, fixed(0));
    patrol.tick(free("risk"), T0);
    expect(patrol.tick(free("risk"), T0 + 20_000)).toHaveLength(1);
    const busy = [{ id: "risk", free: false }];
    expect(patrol.tick(busy, T0 + 21_000)).toEqual([{ agent: "risk", to: null, cancel: true }]);
    expect(patrol.patrolling("risk")).toBe(false);
    // 일하는 동안은 나서지 않고, 다시 한가해지면 그때부터 20초를 센다.
    expect(patrol.tick(busy, T0 + 90_000)).toEqual([]);
    expect(patrol.tick(free("risk"), T0 + 100_000)).toEqual([]);
    expect(patrol.tick(free("risk"), T0 + 119_999)).toEqual([]);
    expect(patrol.tick(free("risk"), T0 + 120_000)).toHaveLength(1);
  });

  it("머무는 중이나 돌아가는 중에 일이 와도 바로 멈춘다", () => {
    // 검증: V65 N16
    const patrol = new Patrol(SPOTS, fixed(0.99));
    patrol.tick(free("review"), T0);
    patrol.tick(free("review"), T0 + 45_000);
    patrol.arrived("review", T0 + 47_000);
    expect(patrol.tick([{ id: "review", free: false }], T0 + 48_000)).toEqual([
      { agent: "review", to: null, cancel: true },
    ]);
    expect(patrol.patrolling("review")).toBe(false);
  });

  it("동시에 순찰하는 담당은 둘까지다", () => {
    // 검증: V65 N16
    const patrol = new Patrol(SPOTS, fixed(0));
    const all = free("news", "strategy", "risk", "review", "trader", "leader");
    patrol.tick(all, T0);
    const orders = patrol.tick(all, T0 + 20_000);
    expect(orders).toHaveLength(2);
    expect(all.filter((a) => patrol.patrolling(a.id))).toHaveLength(2);
    expect(patrol.tick(all, T0 + 25_000)).toEqual([]);
  });

  it("오래 닿지 못하면(길이 막힘) 그만두고 자리로 돌아간다", () => {
    // 검증: V65
    const patrol = new Patrol(SPOTS, fixed(0));
    patrol.tick(free("leader"), T0);
    patrol.tick(free("leader"), T0 + 20_000);
    expect(patrol.tick(free("leader"), T0 + 20_000 + PATROL_RULES.giveUpMs)).toEqual([
      { agent: "leader", to: null },
    ]);
  });

  it("갈 곳이 없으면 나서지 않는다", () => {
    // 검증: V65
    const patrol = new Patrol([], fixed(0));
    patrol.tick(free("risk"), T0);
    expect(patrol.tick(free("risk"), T0 + 60_000)).toEqual([]);
    expect(patrol.patrolling("risk")).toBe(false);
  });

  it("들를 곳은 넣어 준 난수로 고른다(다른 난수면 다른 곳)", () => {
    // 검증: V65
    // 난수를 쓰는 차례: 기다릴 시간 → 들를 곳 수 → 들를 곳마다 하나.
    const a = new Patrol(SPOTS, sequence([0, 0, 0]));
    const b = new Patrol(SPOTS, sequence([0, 0, 0.9]));
    a.tick(free("risk"), T0);
    b.tick(free("risk"), T0);
    const one = a.tick(free("risk"), T0 + 20_000)[0].to;
    const two = b.tick(free("risk"), T0 + 20_000)[0].to;
    expect(one).not.toEqual(two);
  });
});

describe("순찰할 곳(지도)", () => {
  it("지도의 방(라운지 · 응접 라운지 · 라운드 미팅 · 대표실)과 탕비 · 복사기 · 화이트보드 곁에서 걸을 수 있는 칸을 고른다", () => {
    // 검증: V65 R5-13
    const map = officeMap();
    const grid = buildGrid(map);
    const spots = patrolSpots(map, (x, y) => grid.walkable(x, y));
    expect(spots.length).toBeGreaterThanOrEqual(4);
    const keys = spots.map((p) => `${p.x},${p.y}`);
    expect(new Set(keys).size).toBe(keys.length);
    const reserved = new Set<string>();
    for (const a of Object.values(map.agents)) {
      reserved.add(`${a.desk.x},${a.desk.y}`);
      reserved.add(`${a.rest.x},${a.rest.y}`);
    }
    for (const s of map.meeting_seats) reserved.add(`${s.x},${s.y}`);
    for (const p of spots) {
      expect(Number.isInteger(p.x) && Number.isInteger(p.y), JSON.stringify(p)).toBe(true);
      expect(grid.walkable(p.x, p.y), `걸을 수 있는 칸: ${JSON.stringify(p)}`).toBe(true);
      expect(reserved.has(`${p.x},${p.y}`), `담당 자리가 아니다: ${JSON.stringify(p)}`).toBe(false);
    }
    const inside = (id: string) => {
      const room = map.rooms.find((r) => r.id === id);
      expect(room, id).toBeDefined();
      const r = room as { x: number; y: number; w: number; h: number };
      return spots.some((p) => p.x >= r.x && p.x < r.x + r.w && p.y >= r.y && p.y < r.y + r.h);
    };
    expect(inside("lounge"), "라운지").toBe(true);
    expect(inside("entry-lounge"), "응접 라운지").toBe(true);
    expect(inside("conference"), "논의하는 미팅룸에는 가지 않는다").toBe(false);
  });
});

describe("순찰 중 잡담", () => {
  it("순찰로 걷다 마주친 두 담당은 미리 정한 글로 말풍선을 주고받고, 같은 글이 잇달아 나오지 않는다", () => {
    // 검증: V65 N16 R5-13
    const talk = new Smalltalk(fixed(0));
    const meet = (now: number): Walker[] => [
      { id: "risk", x: 3, z: 26, idle: true, walking: true, busy: false },
      { id: "news", x: 3.5, z: 26.5, idle: true, walking: true, busy: false },
    ];
    const first = talk.tick(meet(T0), () => true, T0);
    expect(first).not.toBeNull();
    const said = talk.speaking(T0 + 2_500).map((s) => s.text);
    const line = SMALLTALK_LINES[first?.line ?? 0];
    expect(said).toEqual([line.ask, line.answer]);
    const second = talk.tick(meet(T0 + 100_000), () => true, T0 + 100_000);
    expect(second).not.toBeNull();
    expect(second?.line).not.toBe(first?.line);
  });
});

describe("순찰 모듈의 자리", () => {
  it("순찰은 장면 · 바깥 이름 없이 돌고 서버 · AI를 부르지 않으며 브라우저 저장소 글자가 없다", () => {
    // 검증: V65 N16
    for (const file of ["patrol.ts", "patrol-scene.ts"].map((f) => OFFICE + f)) {
      expect(file in SOURCES, file).toBe(true);
      for (const banned of ["fetch(", "WebSocket", "/api/", "Storage", "indexedDB", "cookie", "innerHTML"]) {
        expect(SOURCES[file].includes(banned), `${file} ${banned}`).toBe(false);
      }
    }
    expect([...reach(OFFICE + "patrol.ts").bare]).toEqual([]);
  });

  it("사무실 화면이 순찰을 잇고, 움직임 줄이기 · 가벼운 화면 · 숨은 탭이면 순찰하지 않는다", () => {
    // 검증: V65 N16 N17
    const { files } = reach(mainOf("office"));
    expect(files.has(OFFICE + "patrol-scene.ts")).toBe(true);
    const scene = SOURCES[OFFICE + "patrol-scene.ts"];
    expect(scene).toMatch(/prefers-reduced-motion/);
    expect(scene).toMatch(/document\.hidden/);
  });
});
