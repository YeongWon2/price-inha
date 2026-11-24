// 담당 부르기 · 멘션 걸어오기 붙이기(server 33차 2번 · 7번, C20 · E47 · N10 · V59 · V60). 원본 DeskRPG 동작
// (docs/research/metaverse-office-refs.md F절): `office.summon` 단계를 받아 3D 장면의 담당을 걷게 한다.
// - 오는 중: 내 곁 빈 칸(32차 `besideCell`)까지 걸어와 나를 보고 선다 → 도착 알림. 부름으로 왔으면 1:1 대화를 연다.
// - 곁에서 기다림: 나를 본다(다시 들어와 자리에 있으면 다시 곁으로 걸어온다 — 알림은 보내지 않는다).
// - 돌아가는 중: 자기 자리로 걸어가 닿으면 도착 알림(→ 자리).
// - 3D가 없으면(가벼운 화면) 걷는 그림 없이 바로 도착 알림만 보낸다(서버 단계가 멈추지 않게).
// 판단(어디로 · 얼마나 빨리 · 어느 쪽)은 `summon-view.ts`의 순수 함수가 한다.

import type { Steer } from "./actors3d";
import { buildGrid, type NavMap, type Point } from "./nav";
import { cellOf } from "./pathfind";
import type { Facing } from "./player";
import type { Scene } from "./scene3d";
import { reportArrived, reportView, requestSummon, type Fetcher } from "./summon-client";
import {
  arrivedNear,
  facingToward,
  meetingNotice,
  opensConversation,
  parseSummon,
  summonDraw,
  viewChange,
  SUMMON_SPEED,
  type SummonEvent,
  type SummonReason,
} from "./summon-view";

/** 보는 쪽 → y축 회전(가구 · 담당과 같은 값). */
const YAW: Record<Facing, number> = { down: 0, up: Math.PI, left: -Math.PI / 2, right: Math.PI / 2 };
/** 다시 들어왔을 때 곁에 있다고 보는 거리(칸). */
const NEAR = 2;

export interface SummonSceneOptions {
  map: NavMap;
  /** 지금 3D 장면(없으면 가벼운 화면). */
  view: () => Scene | null;
  fetcher: Fetcher;
  nameOf: (id: string) => string;
  /** 1:1 대화를 연다. */
  openChat: (id: string) => void;
  /** 채팅 칸 안내. */
  say: (text: string) => void;
}

export interface SummonScene {
  /** `office.summon` 이벤트. */
  onEvent(payload: unknown): void;
  /** 접속 스냅샷의 담당 단계. */
  fromSnapshot(agent: string, phase: unknown, reason: unknown): void;
  /** 장면을 새로 만들었다: 지금 단계대로 다시 그린다. */
  redraw(): void;
  /** 부르기 단추. */
  summon(agent: string): Promise<void>;
  /** 대화 바꿈(`"team"` 또는 담당 id). */
  conversation(prev: string, next: string): void;
}

export function createSummonScene(options: SummonSceneOptions): SummonScene {
  const grid = buildGrid(options.map);
  const phases = new Map<string, SummonEvent>();

  const arrived = (agent: string, reason: SummonReason | null, opens: boolean): void => {
    void reportArrived(options.fetcher, agent);
    if (opens && opensConversation(reason)) { options.openChat(agent); }
  };

  function steerTo(view: Scene, agent: string, to: Point, yaw: number, onArrive?: () => void): void {
    const steer: Steer = { to, speed: SUMMON_SPEED, yaw, onArrive };
    view.steerAgent(agent, steer);
  }

  function draw(agent: string): void {
    const event = phases.get(agent);
    if (!event) { return; }
    const view = options.view();
    const here = view?.agentAt(agent) ?? null;
    const me = view?.playerAt() ?? null;
    if (!view || !here || !me) {
      // 걷는 그림이 없다: 오는 중 · 돌아가는 중은 바로 닿은 것으로 알린다.
      if (event.phase === "called" || event.phase === "returning") { arrived(agent, event.reason, event.phase === "called"); }
      return;
    }
    const draw = summonDraw(event.phase, grid, cellOf(here), cellOf(me), cellOf(here));
    const toward = YAW[facingToward(here, me)];
    switch (draw.kind) {
      case "walk":
        if (event.phase === "returning") {
          view.steerAgent(agent, { to: null, speed: draw.speed, onArrive: () => arrived(agent, event.reason, false) });
        } else {
          steerTo(view, agent, draw.to, YAW[facingToward(draw.to, cellOf(me))], () => arrived(agent, event.reason, true));
        }
        return;
      case "arrived":
      case "stay":
        steerTo(view, agent, cellOf(here), toward);
        arrived(agent, event.reason, true);
        return;
      case "face":
        faceOrCome(view, agent, here, me);
        return;
      default:
        view.steerAgent(agent, null);
    }
  }

  /** 곁에서 기다림: 곁이면 나를 보고, 멀면(다시 들어옴) 곁으로 걸어온다. */
  function faceOrCome(view: Scene, agent: string, here: Point, me: Point): void {
    if (Math.hypot(here.x - me.x, here.y - me.y) <= NEAR || arrivedNear(here, me)) {
      steerTo(view, agent, cellOf(here), YAW[facingToward(here, me)]);
      return;
    }
    const come = summonDraw("called", grid, cellOf(here), cellOf(me), cellOf(here));
    if (come.kind === "walk") { steerTo(view, agent, come.to, YAW[facingToward(come.to, cellOf(me))]); }
  }

  return {
    onEvent(payload: unknown): void {
      const event = parseSummon(payload);
      if (!event) { return; }
      phases.set(event.agent, event);
      draw(event.agent);
    },
    fromSnapshot(agent: string, phase: unknown, reason: unknown): void {
      const event = parseSummon({ agent, phase, reason: reason ?? null, at: 0 });
      if (!event || event.phase === "idle") { return; }
      phases.set(agent, event);
    },
    redraw(): void {
      for (const agent of phases.keys()) { draw(agent); }
    },
    async summon(agent: string): Promise<void> {
      const result = await requestSummon(options.fetcher, agent);
      if (result.kind === "meeting") { options.say(meetingNotice(options.nameOf(agent))); }
      else if (result.kind !== "called") { options.say(`${options.nameOf(agent)}을(를) 부르지 못했습니다`); }
    },
    conversation(prev: string, next: string): void {
      const room = viewChange(prev, next);
      if (room) { void reportView(options.fetcher, room); }
    },
  };
}
