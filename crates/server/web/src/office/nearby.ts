// 다가가 말 걸기(server 32차 7번 · 5번): 2칸 안 담당 고르기 · 안내 글 · `/` · 담당 누르기 · 멀어지면 닫힘.
// 원본 DeskRPG와 같게(docs/research/metaverse-office-refs.md C절): 거리는 칸 단위 곧은 거리이고 2칸 **미만**이 안이다
// (원본 64px, 엄격히 `<`). 가까운 순으로 고른다. 안내 글 "○○을(를) 클릭하거나 /를 눌러 대화"(조사는 받침에 맞춘다).
// `/`: 후보 하나면 바로 열고, 둘 이상이면 "누구와 대화?" 목록, 없으면 아무 일 없다(E42). 장면 없이 시험하는 순수 모듈.

import type { Grid, Point } from "./nav";

/** 말 걸 수 있는 거리(칸). 이보다 **가까우면**(엄격히 작으면) 안이다. */
export const TALK_RADIUS = 2;

/** "누구와 대화?" 목록의 제목. */
export const WHO_TO_TALK = "누구와 대화?";

/** 담당 하나의 자리(칸 단위 실수, 칸 가운데는 +0.5). */
export interface AgentSpot {
  agent: string;
  x: number;
  y: number;
}

/** 두 자리의 곧은 거리(칸). */
export function distance(a: Point, b: Point): number {
  void a;
  void b;
  throw new Error("todo");
}

/** `player`에서 [`TALK_RADIUS`] 안의 담당 id, 가까운 순(같으면 id 글자 순). */
export function nearbyAgents(player: Point, agents: readonly AgentSpot[]): string[] {
  void player;
  void agents;
  throw new Error("todo");
}

/** 목적격 조사: 마지막 글자가 받침 있는 한글이면 "을", 받침 없는 한글이면 "를", 한글이 아니면 "을(를)". */
export function objectParticle(name: string): string {
  void name;
  throw new Error("todo");
}

/** 안내 글: `{이름}{을/를} 클릭하거나 /를 눌러 대화`. */
export function talkHint(name: string): string {
  void name;
  throw new Error("todo");
}

/** `/`를 눌렀을 때 할 일. */
export type SlashAction = { kind: "none" } | { kind: "open"; agent: string } | { kind: "choose"; agents: string[] };

/** 가까운 담당(가까운 순) → `/`의 할 일: 없으면 none, 하나면 open, 둘 이상이면 choose(가까운 순 그대로). */
export function slashAction(near: readonly string[]): SlashAction {
  void near;
  throw new Error("todo");
}

/** 대화 중인 담당에게서 멀어졌는가(거리가 [`TALK_RADIUS`] 이상이면 닫는다 — 기본 흐름 7). */
export function tooFar(player: Point, agent: Point): boolean {
  void player;
  void agent;
  throw new Error("todo");
}

/** 담당을 눌렀을 때 할 일(2b): 2칸 안이면 open, 멀면 그 담당 옆 빈 칸으로 walk(도착하면 연다), 갈 수 없으면 none. */
export type AgentClick = { kind: "open" } | { kind: "walk"; to: Point } | { kind: "none" };

/**
 * 담당 누르기. `grid`는 담당이 선 칸이 막힌 표(`withBlockers`), `player`는 실수 자리, `agent`는 그 담당이 선 칸(정수).
 * 거리는 `player`에서 그 칸 가운데(+0.5)까지다. 걸어갈 칸은 `besideCell`(내 칸에서 길이 가장 짧은 이웃 칸).
 */
export function agentClick(grid: Grid, player: Point, agent: Point): AgentClick {
  void grid;
  void player;
  void agent;
  throw new Error("todo");
}
