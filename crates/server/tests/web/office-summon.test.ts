// 담당 부르기 · 멘션 걸어오기(server 33차 2번 · 4번 · 7번, C20 · E47 · N10 · V59 · V60): 부르기 단추 → 요청 ·
// 단계별 그리기 정보(걸어올 칸은 32차 `besideCell`) · 도착 알림 · 회의 중 안내 · 방 바꿈 알림.
// 원본 DeskRPG 동작(metaverse-office-refs.md F절): 300px/s로 와서 36px 안이면 도착, 나를 보고 서고, 150px/s로 돌아간다.
import { describe, expect, it } from "vitest";
import { API } from "../../web/src/paths";
import { buildGrid, type Grid } from "../../web/src/office/nav";
import { besideCell } from "../../web/src/office/pathfind";
import { reportArrived, reportView, requestSummon, type Fetcher } from "../../web/src/office/summon-client";
import {
  ARRIVE_RADIUS,
  RETURN_SPEED,
  SUMMON_BUTTON,
  SUMMON_BUTTON_CLASS,
  SUMMON_SPEED,
  arrivedNear,
  facingToward,
  meetingNotice,
  opensConversation,
  parseSummon,
  summonButton,
  summonDraw,
  viewChange,
} from "../../web/src/office/summon-view";

interface Call {
  url: string;
  init?: RequestInit;
}

function fakeFetch(answer: () => Promise<Response>): { fetcher: Fetcher; calls: Call[] } {
  const calls: Call[] = [];
  const fetcher: Fetcher = (url, init) => {
    calls.push({ url, init });
    return answer();
  };
  return { fetcher, calls };
}

const json = (status: number, body: unknown): Promise<Response> =>
  Promise.resolve(new Response(JSON.stringify(body), { status, headers: { "Content-Type": "application/json" } }));

const OPEN: Grid = buildGrid({ cols: 12, rows: 12 });
// 내 칸(5,5)의 네 이웃이 모두 막혔다.
const BOXED: Grid = buildGrid({
  cols: 12,
  rows: 12,
  walls: [
    { x: 5, y: 4, w: 1, h: 1 },
    { x: 5, y: 6, w: 1, h: 1 },
    { x: 4, y: 5, w: 1, h: 1 },
    { x: 6, y: 5, w: 1, h: 1 },
  ],
});

describe("빠르기와 도착 거리", () => {
  it("원본 수치를 칸으로 바꾼 값이다(칸 32px)", () => {
    // 검증: N10 V60
    expect(SUMMON_SPEED).toBe(9.4);
    expect(RETURN_SPEED).toBe(4.7);
    expect(ARRIVE_RADIUS).toBe(1.125);
    expect(arrivedNear({ x: 3, y: 3 }, { x: 4, y: 3 })).toBe(true);
    expect(arrivedNear({ x: 3, y: 3 }, { x: 4.125, y: 3 })).toBe(true);
    expect(arrivedNear({ x: 3, y: 3 }, { x: 4.2, y: 3 })).toBe(false);
    expect(arrivedNear({ x: 3, y: 3 }, { x: 4, y: 4 })).toBe(false);
  });
});

describe("단계 이벤트", () => {
  it("office.summon 내용을 읽고 틀린 모양은 버린다", () => {
    // 검증: V59 V60 C20
    expect(parseSummon({ agent: "risk", phase: "called", reason: "mention", at: 10 })).toEqual({
      agent: "risk",
      phase: "called",
      reason: "mention",
      at: 10,
    });
    expect(parseSummon({ agent: "risk", phase: "idle", reason: null, at: 11 })).toEqual({
      agent: "risk",
      phase: "idle",
      reason: null,
      at: 11,
    });
    expect(parseSummon({ agent: "risk", phase: "flying", reason: "call", at: 1 })).toBeNull();
    expect(parseSummon({ agent: "risk", phase: "called", reason: "boss", at: 1 })).toBeNull();
    expect(parseSummon({ phase: "called", reason: "call", at: 1 })).toBeNull();
    expect(parseSummon(null)).toBeNull();
  });
});

describe("단계별 그리기 정보", () => {
  const home = { x: 1, y: 1 };

  it("오는 중이면 내 곁 빈 칸(32차 besideCell)까지 부르기 빠르기로 걷는다", () => {
    // 검증: V60 C20 N10
    const agent = { x: 1, y: 1 };
    const player = { x: 8, y: 5 };
    expect(summonDraw("called", OPEN, agent, player, home)).toEqual({
      kind: "walk",
      to: besideCell(OPEN, agent, player),
      speed: SUMMON_SPEED,
    });
  });

  it("이미 도착 거리 안이면 걷지 않고 바로 도착이다", () => {
    // 검증: V60 N10
    expect(summonDraw("called", OPEN, { x: 5, y: 5 }, { x: 6, y: 5 }, home)).toEqual({ kind: "arrived" });
    expect(summonDraw("called", OPEN, { x: 5, y: 5 }, { x: 6, y: 6 }, home).kind).toBe("walk");
  });

  it("곁에 빈 칸이 없으면 그 자리에 선다", () => {
    // 검증: V60
    // 대조(결정 29): 네 이웃이 막히면 그 바깥 고리(맨해튼 2)의 빈 칸으로 온다.
    expect(summonDraw("called", BOXED, { x: 1, y: 1 }, { x: 5, y: 5 }, home)).toEqual({ kind: "walk", to: { x: 4, y: 4 }, speed: SUMMON_SPEED });
  });

  it("곁에서 기다리면 나를 보고 선다", () => {
    // 검증: V60 C20
    expect(summonDraw("waiting", OPEN, { x: 7, y: 5 }, { x: 8, y: 5 }, home)).toEqual({ kind: "face", facing: "right" });
    expect(summonDraw("waiting", OPEN, { x: 8, y: 6 }, { x: 8, y: 5 }, home)).toEqual({ kind: "face", facing: "up" });
  });

  it("돌아가는 중이면 자기 자리로 돌아가는 빠르기로 걷고, 자리면 지금 자리 그리기를 따른다", () => {
    // 검증: V59 V60 N10
    expect(summonDraw("returning", OPEN, { x: 7, y: 5 }, { x: 8, y: 5 }, home)).toEqual({
      kind: "walk",
      to: home,
      speed: RETURN_SPEED,
    });
    expect(summonDraw("idle", OPEN, home, { x: 8, y: 5 }, home)).toEqual({ kind: "home" });
  });

  it("보는 쪽은 큰 축의 네 방향이고 같으면 세로다", () => {
    // 검증: V60
    const o = { x: 5, y: 5 };
    expect(facingToward(o, { x: 8, y: 6 })).toBe("right");
    expect(facingToward(o, { x: 2, y: 4 })).toBe("left");
    expect(facingToward(o, { x: 6, y: 9 })).toBe("down");
    expect(facingToward(o, { x: 4, y: 1 })).toBe("up");
    expect(facingToward(o, { x: 7, y: 7 })).toBe("down");
  });

  it("부름으로 와서 도착하면 1:1 대화를 열고, 멘션으로 왔으면 열지 않는다", () => {
    // 검증: V59 V60 C20
    expect(opensConversation("call")).toBe(true);
    expect(opensConversation("mention")).toBe(false);
    expect(opensConversation(null)).toBe(false);
  });
});

describe("부르기 단추와 요청", () => {
  it("담당 목록의 부르기 단추를 누르면 그 담당을 부른다", () => {
    // 검증: V60 C20
    const called: string[] = [];
    const button = summonButton("risk", (agent) => called.push(agent));
    expect(button.textContent).toBe(SUMMON_BUTTON);
    expect(button.classList.contains(SUMMON_BUTTON_CLASS)).toBe(true);
    expect(button.dataset.agent).toBe("risk");
    expect(button.type).toBe("button");
    button.click();
    expect(called).toEqual(["risk"]);
  });

  it("부르기 요청은 부르기 경로로 담당을 보낸다", async () => {
    // 검증: V60 C20
    const { fetcher, calls } = fakeFetch(() => json(200, { agent: "risk", phase: "called" }));
    expect(await requestSummon(fetcher, "risk")).toEqual({ kind: "called" });
    expect(calls).toHaveLength(1);
    expect(calls[0].url).toBe(API.officeSummon);
    expect(calls[0].init?.method).toBe("POST");
    expect(JSON.parse(String(calls[0].init?.body))).toEqual({ agent: "risk" });
  });

  it("회의 중이면 안내를 보이고 걸어오지 않는다", async () => {
    // 검증: V60 E47
    const meeting = fakeFetch(() => json(409, { error: "meeting_reserved" }));
    expect(await requestSummon(meeting.fetcher, "risk")).toEqual({ kind: "meeting" });
    expect(meetingNotice("리스크")).toBe("리스크는 지금 회의 중이에요");
    expect(meetingNotice("전략")).toBe("전략은 지금 회의 중이에요");
    expect(meetingNotice("AI")).toBe("AI은(는) 지금 회의 중이에요");
  });

  it("모르는 담당 · 그 밖의 실패는 던지지 않고 결과로 돌려준다", async () => {
    // 검증: C20
    expect(await requestSummon(fakeFetch(() => json(400, { error: "unknown_agent" })).fetcher, "x")).toEqual({
      kind: "unknown",
    });
    expect(await requestSummon(fakeFetch(() => json(500, { error: "x" })).fetcher, "risk")).toEqual({ kind: "failed" });
    expect(await requestSummon(fakeFetch(() => Promise.reject(new Error("끊김"))).fetcher, "risk")).toEqual({
      kind: "failed",
    });
  });

  it("도착하면 도착 경로로 그 담당을 알린다", async () => {
    // 검증: V60 C20 E48
    const { fetcher, calls } = fakeFetch(() => json(200, { agent: "risk", phase: "waiting" }));
    expect(await reportArrived(fetcher, "risk")).toBe(true);
    expect(calls[0].url).toBe(API.officeSummonArrived);
    expect(calls[0].init?.method).toBe("POST");
    expect(JSON.parse(String(calls[0].init?.body))).toEqual({ agent: "risk" });
    expect(await reportArrived(fakeFetch(() => Promise.reject(new Error("끊김"))).fetcher, "risk")).toBe(false);
  });
});

describe("방 바꿈 알림", () => {
  it("팀 방 ↔ 1:1 방이 바뀔 때만 알린다", () => {
    // 검증: V59 C20
    expect(viewChange("team", "risk")).toBe("direct");
    expect(viewChange("risk", "team")).toBe("team");
    expect(viewChange("risk", "strategy")).toBeNull();
    expect(viewChange("team", "team")).toBeNull();
  });

  it("보는 방 알림은 보는 방 경로로 방을 보낸다", async () => {
    // 검증: V59 C20
    const { fetcher, calls } = fakeFetch(() => json(200, { room: "direct" }));
    expect(await reportView(fetcher, "direct")).toBe(true);
    expect(calls[0].url).toBe(API.officeSummonView);
    expect(calls[0].init?.method).toBe("POST");
    expect(JSON.parse(String(calls[0].init?.body))).toEqual({ room: "direct" });
    expect(await reportView(fakeFetch(() => json(400, { error: "invalid_input" })).fetcher, "team")).toBe(false);
  });
});
