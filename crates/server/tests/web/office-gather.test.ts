// 멘션하거나 말을 걸면 담당이 내 앞으로 모인다(결정 29, F10 2b1 · 4a2 · 4c4): 내 둘레의 빈 칸을 가까운 고리부터 하나씩
// 나눠 주고(거리 1 고리 → 2 고리 …, 같은 고리 안에서는 그 담당에게서 길이 짧은 칸, 다른 담당이 서 있거나 잡은 칸은
// 건너뜀), 말을 걸면(누르기 · 목록 · 탭 · `/`) 대화는 곧바로 열리고 그 담당이 온다(내 캐릭터는 걷지 않는다).
import { describe, expect, it } from "vitest";
import { buildGrid, type Grid, type Point } from "../../web/src/office/nav";
import { besideCell, withBlockers } from "../../web/src/office/pathfind";
import { GATHER_RINGS, gatherCell, ringCells, talkCalls } from "../../web/src/office/gather";
import { agentClick, agentsByDistance, slashAction, type AgentSpot } from "../../web/src/office/nearby";
import { opensConversation, summonDraw, SUMMON_SPEED } from "../../web/src/office/summon-view";

const OPEN: Grid = buildGrid({ cols: 20, rows: 20 });
const ME: Point = { x: 10, y: 10 };
const manhattan = (a: Point, b: Point) => Math.abs(a.x - b.x) + Math.abs(a.y - b.y);
const key = (p: Point) => `${p.x},${p.y}`;

describe("고리", () => {
  it("거리 1 고리는 네 이웃(위 · 아래 · 왼쪽 · 오른쪽)이고 거리 k 고리는 맨해튼 거리 k인 칸 모두다", () => {
    // 검증: V60
    expect(ringCells(ME, 1)).toEqual([
      { x: 10, y: 9 },
      { x: 10, y: 11 },
      { x: 9, y: 10 },
      { x: 11, y: 10 },
    ]);
    const two = ringCells(ME, 2);
    expect(two).toHaveLength(8);
    expect(new Set(two.map(key)).size).toBe(8);
    for (const c of two) expect(manhattan(c, ME)).toBe(2);
    expect(ringCells(ME, 3)).toHaveLength(12);
  });
});

describe("자리 나누기", () => {
  it("잡힌 칸이 없으면 지금처럼 네 이웃 가운데 그 담당에게서 길이 가장 짧은 칸이다", () => {
    // 검증: V60 N10
    for (const agent of [{ x: 1, y: 1 }, { x: 18, y: 10 }, { x: 10, y: 2 }, { x: 3, y: 17 }]) {
      expect(gatherCell(OPEN, agent, ME)).toEqual(besideCell(OPEN, agent, ME));
    }
  });

  it("다른 담당이 서 있거나 잡은 칸은 건너뛰고, 네 이웃이 차면 그 바깥 고리로 간다", () => {
    // 검증: V60
    const agent = { x: 2, y: 10 };
    expect(gatherCell(OPEN, agent, ME)).toEqual({ x: 9, y: 10 });
    // 같은 고리 안에서 다음으로 짧은 칸(같으면 위 · 아래 · 왼쪽 · 오른쪽 차례).
    expect(gatherCell(OPEN, agent, ME, [{ x: 9, y: 10 }])).toEqual({ x: 10, y: 9 });
    const ring1 = ringCells(ME, 1);
    const outer = gatherCell(OPEN, agent, ME, ring1);
    expect(outer).toEqual({ x: 8, y: 10 });
  });

  it("여섯이 와도 겹치지 않고 가까운 고리부터 채운다", () => {
    // 검증: V60
    const agents = [{ x: 1, y: 1 }, { x: 18, y: 1 }, { x: 1, y: 18 }, { x: 18, y: 18 }, { x: 10, y: 1 }, { x: 10, y: 18 }];
    const taken: Point[] = [];
    for (const agent of agents) {
      const cell = gatherCell(OPEN, agent, ME, taken);
      expect(cell, `${key(agent)}`).not.toBeNull();
      taken.push(cell as Point);
    }
    expect(new Set(taken.map(key)).size).toBe(6);
    expect(taken.some((c) => c.x === ME.x && c.y === ME.y)).toBe(false);
    expect(taken.slice(0, 4).map((c) => manhattan(c, ME))).toEqual([1, 1, 1, 1]);
    expect(taken.slice(4).map((c) => manhattan(c, ME))).toEqual([2, 2]);
  });

  it("이미 곁 빈 칸에 서 있으면 그 칸이다", () => {
    // 검증: V60
    expect(gatherCell(OPEN, { x: 11, y: 10 }, ME, [{ x: 10, y: 9 }])).toEqual({ x: 11, y: 10 });
  });

  it("네 이웃이 벽이면 바깥 고리로 가고, 정한 고리 안에 갈 칸이 없으면 null이다", () => {
    // 검증: V60 E40
    const boxed = buildGrid({
      cols: 12,
      rows: 12,
      walls: [
        { x: 5, y: 4, w: 1, h: 1 },
        { x: 5, y: 6, w: 1, h: 1 },
        { x: 4, y: 5, w: 1, h: 1 },
        { x: 6, y: 5, w: 1, h: 1 },
      ],
    });
    const cell = gatherCell(boxed, { x: 1, y: 1 }, { x: 5, y: 5 });
    expect(cell).toEqual({ x: 4, y: 4 });
    const all: Point[] = [];
    for (let k = 1; k <= GATHER_RINGS; k += 1) all.push(...ringCells(ME, k));
    expect(gatherCell(OPEN, { x: 1, y: 1 }, ME, all)).toBeNull();
  });

  it("오는 중의 그리기는 잡힌 칸을 건너뛴 칸까지 걷는다", () => {
    // 검증: V60 N10
    const agent = { x: 2, y: 10 };
    expect(summonDraw("called", OPEN, agent, ME, agent, [{ x: 9, y: 10 }])).toEqual({
      kind: "walk",
      to: { x: 10, y: 9 },
      speed: SUMMON_SPEED,
    });
  });
});

describe("말 걸기", () => {
  it("말을 걸면 멀리 있는 담당을 부르고, 이미 부름으로 오는 중 · 곁이면 부르지 않는다", () => {
    // 검증: V56 V60 C20
    expect(talkCalls("idle", null, false)).toBe(true);
    expect(talkCalls(null, null, false)).toBe(true);
    expect(talkCalls("idle", null, true)).toBe(false);
    expect(talkCalls("called", "call", false)).toBe(false);
    expect(talkCalls("waiting", "call", false)).toBe(false);
    expect(talkCalls("returning", "call", true)).toBe(true);
    // 멘션으로 왔던 담당(1:1로 가면 돌아간다)은 말을 걸면 다시 부른다.
    expect(talkCalls("waiting", "mention", true)).toBe(true);
    expect(talkCalls("called", "mention", false)).toBe(true);
  });

  it("말 걸기로 불러 도착하면 1:1을 다시 열지 않는다(이미 열려 있다)", () => {
    // 검증: V60 C20
    expect(opensConversation("call", true)).toBe(false);
    expect(opensConversation("call", false)).toBe(true);
    expect(opensConversation("mention", false)).toBe(false);
  });

  it("담당을 누르면 거리 상관없이 곧바로 열고, 멀면 그 담당이 내 곁 빈 칸으로 온다", () => {
    // 검증: V56 V60
    const grid = withBlockers(buildGrid({ cols: 20, rows: 20 }), [{ x: 10, y: 5 }]);
    expect(agentClick(grid, { x: 9.5, y: 5.5 }, { x: 10, y: 5 })).toEqual({ kind: "open" });
    expect(agentClick(grid, { x: 2.5, y: 5.5 }, { x: 10, y: 5 })).toEqual({ kind: "come", to: { x: 3, y: 5 } });
    const walled = withBlockers(grid, [
      { x: 9, y: 5 },
      { x: 11, y: 5 },
      { x: 10, y: 4 },
      { x: 10, y: 6 },
      { x: 9, y: 4 },
      { x: 11, y: 4 },
      { x: 9, y: 6 },
      { x: 11, y: 6 },
    ]);
    // 갇혀서 올 수 없어도 대화는 연다.
    expect(agentClick(walled, { x: 2.5, y: 5.5 }, { x: 10, y: 5 })).toEqual({ kind: "open" });
  });

  it("/의 후보는 2칸 제한 없이 모든 담당이고 가까운 순이다", () => {
    // 검증: V56 E42
    const spots: AgentSpot[] = [
      { agent: "news", x: 15.5, y: 10.5 },
      { agent: "risk", x: 11.5, y: 10.5 },
      { agent: "leader", x: 3.5, y: 3.5 },
    ];
    const all = agentsByDistance({ x: 10.5, y: 10.5 }, spots);
    expect(all).toEqual(["risk", "news", "leader"]);
    expect(slashAction(all)).toEqual({ kind: "choose", agents: ["risk", "news", "leader"] });
    expect(slashAction(agentsByDistance({ x: 1.5, y: 1.5 }, [spots[0]]))).toEqual({ kind: "open", agent: "news" });
    expect(agentsByDistance({ x: 1.5, y: 1.5 }, [])).toEqual([]);
  });
});
