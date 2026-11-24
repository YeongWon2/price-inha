// 다가가 말 걸기(server 32차 7번 · 5번, F10 3 · 3a · 2b · 7, E42): 2칸 안(원본 64px, 엄격히 미만) 담당을 가까운 순으로 고르고
// 안내 글 "○○을(를) 클릭하거나 /를 눌러 대화"(조사는 받침에 맞춘다), `/`는 하나면 열기 · 둘 이상이면 "누구와 대화?" 목록 ·
// 없으면 아무 일 없음, 멀어지면 닫힘, 먼 담당을 누르면 옆 빈 칸까지 걸어감. 원본 DeskRPG(metaverse-office-refs.md C절).
import { describe, expect, it } from "vitest";
import { buildGrid } from "../../web/src/office/nav";
import {
  TALK_RADIUS,
  WHO_TO_TALK,
  agentClick,
  distance,
  nearbyAgents,
  objectParticle,
  slashAction,
  talkHint,
  tooFar,
  type AgentSpot,
} from "../../web/src/office/nearby";
import { withBlockers } from "../../web/src/office/pathfind";

const AGENTS: AgentSpot[] = [
  { agent: "news", x: 5.5, y: 5.5 },
  { agent: "risk", x: 6.5, y: 6.5 },
  { agent: "leader", x: 20.5, y: 20.5 },
];

describe("2칸 안 담당", () => {
  it("2칸보다 가까운 담당을 가까운 순으로 고른다", () => {
    // 검증: V56
    expect(TALK_RADIUS).toBe(2);
    expect(distance({ x: 0, y: 0 }, { x: 3, y: 4 })).toBeCloseTo(5, 9);
    expect(nearbyAgents({ x: 6.5, y: 5.5 }, AGENTS)).toEqual(["news", "risk"]);
    expect(nearbyAgents({ x: 7.5, y: 7.0 }, AGENTS)).toEqual(["risk"]);
    expect(nearbyAgents({ x: 6.0, y: 6.0 }, AGENTS)).toEqual(["news", "risk"]);
    expect(nearbyAgents({ x: 12.5, y: 12.5 }, AGENTS)).toEqual([]);
  });

  it("정확히 2칸은 안이 아니다(원본 엄격히 미만)", () => {
    // 검증: V56
    expect(nearbyAgents({ x: 7.5, y: 5.5 }, [{ agent: "news", x: 5.5, y: 5.5 }])).toEqual([]);
    expect(nearbyAgents({ x: 7.49, y: 5.5 }, [{ agent: "news", x: 5.5, y: 5.5 }])).toEqual(["news"]);
  });

  it("같은 거리면 id 글자 순이다", () => {
    // 검증: V56
    const pair: AgentSpot[] = [
      { agent: "trader", x: 4.5, y: 5.5 },
      { agent: "review", x: 6.5, y: 5.5 },
    ];
    expect(nearbyAgents({ x: 5.5, y: 5.5 }, pair)).toEqual(["review", "trader"]);
  });
});

describe("안내 글", () => {
  it("조사는 받침에 맞춘다(받침 있으면 을, 없으면 를, 한글이 아니면 을(를))", () => {
    // 검증: V56
    expect(objectParticle("전략")).toBe("을");
    expect(objectParticle("팀장")).toBe("을");
    expect(objectParticle("리스크")).toBe("를");
    expect(objectParticle("복기")).toBe("를");
    expect(objectParticle("뉴스 공시 시세")).toBe("를");
    expect(objectParticle("AI")).toBe("을(를)");
  });

  it("안내 글은 원본 문장 그대로다", () => {
    // 검증: V56
    expect(talkHint("전략")).toBe("전략을 클릭하거나 /를 눌러 대화");
    expect(talkHint("리스크")).toBe("리스크를 클릭하거나 /를 눌러 대화");
    expect(talkHint("AI")).toBe("AI을(를) 클릭하거나 /를 눌러 대화");
  });
});

describe("/ 누르기", () => {
  it("없으면 아무 일 없고, 하나면 열고, 둘 이상이면 가까운 순 목록이다", () => {
    // 검증: V56 E42
    expect(WHO_TO_TALK).toBe("누구와 대화?");
    expect(slashAction([])).toEqual({ kind: "none" });
    expect(slashAction(["risk"])).toEqual({ kind: "open", agent: "risk" });
    expect(slashAction(["risk", "news"])).toEqual({ kind: "choose", agents: ["risk", "news"] });
  });
});

describe("멀어지면 닫힘", () => {
  it("대화 중인 담당에게서 2칸 이상 멀어지면 닫는다", () => {
    // 검증: V56
    const agent = { x: 5.5, y: 5.5 };
    expect(tooFar({ x: 6.5, y: 5.5 }, agent)).toBe(false);
    expect(tooFar({ x: 6.9, y: 6.9 }, agent)).toBe(false);
    expect(tooFar({ x: 7.5, y: 5.5 }, agent)).toBe(true);
    expect(tooFar({ x: 9.5, y: 9.5 }, agent)).toBe(true);
  });
});

describe("담당 누르기", () => {
  it("2칸 안이면 곧바로 열고, 멀면 그 담당 옆 빈 칸까지 걸어간다", () => {
    // 검증: V56
    const grid = withBlockers(buildGrid({ cols: 20, rows: 20 }), [{ x: 10, y: 5 }]);
    expect(agentClick(grid, { x: 9.5, y: 5.5 }, { x: 10, y: 5 })).toEqual({ kind: "open" });
    expect(agentClick(grid, { x: 2.5, y: 5.5 }, { x: 10, y: 5 })).toEqual({ kind: "walk", to: { x: 9, y: 5 } });
    expect(agentClick(grid, { x: 17.5, y: 5.5 }, { x: 10, y: 5 })).toEqual({ kind: "walk", to: { x: 11, y: 5 } });
  });

  it("갈 수 있는 옆 칸이 없으면 아무 일 없다", () => {
    // 검증: V56 E40
    const grid = withBlockers(buildGrid({ cols: 20, rows: 20 }), [
      { x: 10, y: 5 },
      { x: 9, y: 5 },
      { x: 11, y: 5 },
      { x: 10, y: 4 },
      { x: 10, y: 6 },
    ]);
    expect(agentClick(grid, { x: 2.5, y: 5.5 }, { x: 10, y: 5 })).toEqual({ kind: "none" });
  });
});
