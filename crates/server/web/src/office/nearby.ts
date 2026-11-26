// 다가가 말 걸기(server 32차 7번 · 5번): 2칸 안 담당 고르기 · 안내 글 · `/` · 담당 누르기 · 멀어지면 닫힘.
// 원본 DeskRPG와 같게(docs/research/metaverse-office-refs.md C절): 거리는 칸 단위 곧은 거리이고 2칸 **미만**이 안이다
// (원본 64px, 엄격히 `<`). 가까운 순으로 고른다. 안내 글 "○○을(를) 클릭하거나 /를 눌러 대화"(조사는 받침에 맞춘다).
// `/`: 후보 하나면 바로 열고, 둘 이상이면 "누구와 대화?" 목록, 없으면 아무 일 없다(E42). 장면 없이 시험하는 순수 모듈.
// (59차 · 결정 29) 말을 걸면 대화는 곧바로 열리고 그 담당이 내 곁으로 온다 — 내 캐릭터는 걷지 않는다. `/`의 후보는
// 2칸 제한 없이 모든 담당(가까운 순)이고, 2칸 안내 글은 그대로다.

import type { Grid, Point } from "./nav";
import { gatherCell } from "./gather";
import { cellOf } from "./pathfind";

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
  return Math.hypot(a.x - b.x, a.y - b.y);
}

/** `player`에서 [`TALK_RADIUS`] 안의 담당 id, 가까운 순(같으면 id 글자 순). */
export function nearbyAgents(player: Point, agents: readonly AgentSpot[]): string[] {
  return byDistance(player, agents, TALK_RADIUS);
}

/** 담당 id를 가까운 순으로 늘어놓는다(같으면 id 글자 순). */
function byDistance(player: Point, agents: readonly AgentSpot[], within: number): string[] {
  return agents
    .map((a) => ({ agent: a.agent, d: distance(player, a) }))
    .filter((a) => a.d < within)
    .sort((a, b) => a.d - b.d || (a.agent < b.agent ? -1 : a.agent > b.agent ? 1 : 0))
    .map((a) => a.agent);
}

/** (결정 29) `/`의 후보: 거리 제한 없이 모든 담당 id, 가까운 순(같으면 id 글자 순). */
export function agentsByDistance(player: Point, agents: readonly AgentSpot[]): string[] {
  return byDistance(player, agents, Infinity);
}

/** 목적격 조사: 마지막 글자가 받침 있는 한글이면 "을", 받침 없는 한글이면 "를", 한글이 아니면 "을(를)". */
export function objectParticle(name: string): string {
  const last = name.trim().slice(-1);
  const code = last.charCodeAt(0);
  if (!(code >= 0xac00 && code <= 0xd7a3)) return "을(를)";
  return (code - 0xac00) % 28 === 0 ? "를" : "을";
}

/** 안내 글: `{이름}{을/를} 클릭하거나 /를 눌러 대화`. */
export function talkHint(name: string): string {
  return `${name}${objectParticle(name)} 클릭하거나 /를 눌러 대화`;
}

/** `/`를 눌렀을 때 할 일. */
export type SlashAction = { kind: "none" } | { kind: "open"; agent: string } | { kind: "choose"; agents: string[] };

/** 가까운 담당(가까운 순) → `/`의 할 일: 없으면 none, 하나면 open, 둘 이상이면 choose(가까운 순 그대로). */
export function slashAction(near: readonly string[]): SlashAction {
  if (near.length === 0) return { kind: "none" };
  if (near.length === 1) return { kind: "open", agent: near[0] };
  return { kind: "choose", agents: [...near] };
}

/** 대화 중인 담당에게서 멀어졌는가(거리가 [`TALK_RADIUS`] 이상이면 닫는다 — 기본 흐름 7). */
export function tooFar(player: Point, agent: Point): boolean {
  return distance(player, agent) >= TALK_RADIUS;
}

/**
 * 담당을 눌렀을 때 할 일(2b · 결정 29): 대화는 거리 상관없이 곧바로 연다. 2칸 안이면 `open`(이미 곁이라 부르지 않는다),
 * 멀면 그 담당이 내 곁 빈 칸 `to`로 `come`(내 캐릭터는 걷지 않는다), 담당이 올 길이 없으면 `open`.
 */
export type AgentClick = { kind: "open" } | { kind: "come"; to: Point };

/**
 * 담당 누르기. `grid`는 걷기 표, `player`는 실수 자리, `agent`는 그 담당이 선 칸(정수). 거리는 `player`에서 그 칸
 * 가운데(+0.5)까지다. 올 칸은 `gatherCell`(내 칸 둘레에서 그 담당에게서 길이 가장 짧은 빈 칸 — `taken`은 잡힌 칸).
 */
export function agentClick(grid: Grid, player: Point, agent: Point, taken: readonly Point[] = []): AgentClick {
  if (distance(player, { x: agent.x + 0.5, y: agent.y + 0.5 }) < TALK_RADIUS) return { kind: "open" };
  const to = gatherCell(grid, agent, cellOf(player), taken);
  return to === null ? { kind: "open" } : { kind: "come", to };
}
