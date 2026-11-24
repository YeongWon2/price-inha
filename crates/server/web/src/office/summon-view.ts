// 담당 부르기 · 멘션 걸어오기의 그리기 정보(server 33차 2번 · 7번, C20 · E47 · N10 · V59 · V60). 원본 DeskRPG 동작
// (docs/research/metaverse-office-refs.md F절 · C절):
// - 단계 `office.summon` `{"agent", "phase", "reason", "at"}`: 자리(idle) → 오는 중(called) → 곁에서 기다림(waiting) →
//   돌아가는 중(returning) → 자리. 이벤트로 단계가 바뀌면 그대로 그린다(다른 화면에서 불러도 보인다).
// - 오는 중: 이미 1.125칸(원본 36px) 안이면 걷지 않고 바로 도착. 아니면 내 곁 빈 칸(32차 `besideCell` — 내 칸의 네 이웃
//   가운데 담당 칸에서 길이 가장 짧은 칸)까지 초당 9.4칸(원본 300px/s)으로 걷는다. 도착하면 도착 알림을 보낸다.
// - 곁에서 기다림: 나를 보고 선다(가로 · 세로 가운데 큰 축 쪽 네 방향, 같으면 세로).
// - 돌아가는 중: 자기 자리까지 초당 4.7칸(원본 150px/s)으로 걷고, 닿으면 도착 알림을 보낸다.
// - 부름(`call`)으로 와서 도착하면 1:1 대화를 연다. 멘션(`mention`)으로 온 담당은 열지 않는다(답은 채팅 패널).
// - 회의 중(409 `meeting_reserved`)이면 "○○은(는) 지금 회의 중이에요" 안내(E47)를 보이고 걸어오지 않는다.
// three.js 없이 칸 좌표와 DOM 단추만 다룬다.

import type { Grid, Point } from "./nav";
import type { Facing } from "./player";
import type { ViewRoom } from "./summon-client";

/** 부르기 단계. */
export type SummonPhase = "idle" | "called" | "waiting" | "returning";
/** 왜 왔는가. */
export type SummonReason = "call" | "mention";

/** `office.summon` 이벤트 하나. */
export interface SummonEvent {
  agent: string;
  phase: SummonPhase;
  reason: SummonReason | null;
  at: number;
}

/** 부를 때 걷기 빠르기(칸/초, 원본 300px/s ÷ 32px). */
export const SUMMON_SPEED = 9.4;
/** 돌아갈 때 걷기 빠르기(칸/초, 원본 150px/s). */
export const RETURN_SPEED = 4.7;
/** 도착으로 보는 거리(칸, 원본 36px). */
export const ARRIVE_RADIUS = 1.125;
/** 담당 목록의 부르기 단추 글. */
export const SUMMON_BUTTON = "부르기";
/** 부르기 단추의 class. */
export const SUMMON_BUTTON_CLASS = "summon";

/** `office.summon` 내용 → 이벤트. 모양이 틀리거나 모르는 단계 · 까닭이면 `null`. */
export function parseSummon(payload: unknown): SummonEvent | null {
  void payload;
  throw new Error("todo");
}

/** 단계별로 그릴 것. */
export type SummonDraw =
  /** 그 칸까지 그 빠르기로 걷는다. */
  | { kind: "walk"; to: Point; speed: number }
  /** 걷지 않고 바로 도착이다(도착 알림을 보낸다). */
  | { kind: "arrived" }
  /** 그 자리에서 그쪽을 보고 선다. */
  | { kind: "face"; facing: Facing }
  /** 걸어갈 곁 칸이 없다: 그 자리에 선다. */
  | { kind: "stay" }
  /** 자리에 있다(지금의 자리 그리기를 따른다). */
  | { kind: "home" };

/**
 * 단계 `phase`에서 칸 `agent`의 담당을 어떻게 그릴지. `player`는 내 칸, `home`은 그 담당의 자리 칸.
 * - `called`: `agent`가 `player`에서 [`ARRIVE_RADIUS`] 안이면 `arrived`, 아니면 `besideCell(grid, agent, player)`로
 *   `walk`(빠르기 [`SUMMON_SPEED`]), 곁 칸이 없으면 `stay`.
 * - `waiting`: `face`(나를 보는 쪽, [`facingToward`]).
 * - `returning`: `home`으로 `walk`(빠르기 [`RETURN_SPEED`]).
 * - `idle`: `home`.
 */
export function summonDraw(phase: SummonPhase, grid: Grid, agent: Point, player: Point, home: Point): SummonDraw {
  void phase;
  void grid;
  void agent;
  void player;
  void home;
  throw new Error("todo");
}

/** `from`에서 `to`를 보는 네 방향: 가로 거리가 세로 거리보다 크면 left/right, 아니면 up/down(y가 크면 down). */
export function facingToward(from: Point, to: Point): Facing {
  void from;
  void to;
  throw new Error("todo");
}

/** 걷는 담당 `pos`가 목표 `target`에 닿았는가(거리 [`ARRIVE_RADIUS`] 이하). */
export function arrivedNear(pos: Point, target: Point): boolean {
  void pos;
  void target;
  throw new Error("todo");
}

/** 도착했을 때 1:1 대화를 여는가: 부름(`call`)이면 참, 멘션으로 왔으면 거짓(4a3 · 4c4). */
export function opensConversation(reason: SummonReason | null): boolean {
  void reason;
  throw new Error("todo");
}

/** 회의 중 안내(E47): `{이름}{은/는} 지금 회의 중이에요`(받침 있으면 "은", 없으면 "는", 한글이 아니면 "은(는)"). */
export function meetingNotice(name: string): string {
  void name;
  throw new Error("todo");
}

/**
 * 대화 바꾸기 `prev` → `next`(대화 키: 팀 방 `"team"` 또는 담당 id)에서 서버에 알릴 방. 팀 방 ↔ 1:1 방이 바뀔 때만
 * `"team"` · `"direct"`, 같은 쪽 안에서 바뀌면(1:1 → 다른 1:1 포함) `null`.
 */
export function viewChange(prev: string, next: string): ViewRoom | null {
  void prev;
  void next;
  throw new Error("todo");
}

/** 담당 목록 줄에 다는 부르기 단추(글 [`SUMMON_BUTTON`], class [`SUMMON_BUTTON_CLASS`], `data-agent`). 누르면 `onSummon(agent)`. */
export function summonButton(agent: string, onSummon: (agent: string) => void): HTMLButtonElement {
  void agent;
  void onSummon;
  throw new Error("todo");
}
