// 순찰(server 24차 1번 — 뼈대, red).

import type { Point } from "./nav";

export const PATROL_RULES = {
  idleMinMs: 0,
  idleMaxMs: 0,
  maxStops: 0,
  dwellMs: 0,
  speed: 0,
  maxWalkers: 0,
  giveUpMs: 0,
} as const;

export interface PatrolAgent {
  id: string;
  free: boolean;
}

export interface PatrolOrder {
  agent: string;
  to: Point | null;
  cancel?: boolean;
}

export interface PatrolMap {
  rooms: { id: string; x: number; y: number; w: number; h: number }[];
  furniture: { kind: string; x: number; y: number; w: number; h: number }[];
  agents: Record<string, { desk: Point; rest: Point }>;
  meeting_seats: Point[];
}

export function isFree(view: { place: string; work: string }, steered: boolean): boolean {
  void view;
  void steered;
  return false;
}

export function patrolSpots(map: PatrolMap, walkable: (x: number, y: number) => boolean): Point[] {
  void map;
  void walkable;
  return [];
}

export class Patrol {
  constructor(spots: readonly Point[], random: () => number = Math.random) {
    void spots;
    void random;
  }

  patrolling(agent: string): boolean {
    void agent;
    return false;
  }

  tick(agents: readonly PatrolAgent[], now: number): PatrolOrder[] {
    void agents;
    void now;
    return [];
  }

  arrived(agent: string, now: number): void {
    void agent;
    void now;
  }
}
