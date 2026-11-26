// 순찰(server 24차 1번, additions.md R5-13 · N16 · V65 — 원본 동작: docs/research/agent-office-refs.md 3-C · F 3,
// PixelOffice의 한가할 때 순찰): 자리에서 한가한 지 20~45초가 지나면 사무실의 정해진 곳 최대 3곳을 다녀온다
// (한 곳에 3.0초 머묾, 산책 걷기 초당 55px ≈ 1.7칸). 진짜 일이 오면 바로 멈추고 자리로 간다.
// 화면만의 일이다: 서버 · 기록 · AI에 보내지 않는다(호출 0). 여러 화면에서 달라도 된다.
// 장면 없이 시험할 수 있게 시각은 부르는 쪽이 넘기는 밀리초(`now`), 난수는 넣어 주는 함수다. 걷기는 부르는 쪽이 한다
// (patrol-scene.ts가 장면의 `steerAgent`로 걷게 하고, 닿으면 [`Patrol.arrived`]를 부른다).
//
// 원본과 다른 점(까닭): 원본은 다른 사람의 책상을 들르지만, 우리는 지도에 있는 곳(라운지 · 응접 라운지 · 라운드 미팅 ·
// 대표실 · 탕비 · 복사기 · 화이트보드 곁)을 들른다(설계서 R5-13 "사무실을 돈다"). 동시에 순찰하는 담당은 둘까지다
// (원본은 상한이 없다 — 여섯이 한꺼번에 일어나지 않게, 그래도 둘이 마주쳐 잡담할 수 있게 우리가 정한 값).

import type { Point } from "./nav";

/** 순찰 수치(시간은 밀리초, 빠르기는 칸/초). */
export const PATROL_RULES = {
  /** 한가한 지 이 시간 뒤부터(원본 20초). */
  idleMinMs: 20_000,
  /** 이 시간 안에(원본 45초). */
  idleMaxMs: 45_000,
  /** 한 번에 들르는 곳은 이만큼까지(원본 3곳). */
  maxStops: 3,
  /** 한 곳에 머무는 시간(원본 3.0초). */
  dwellMs: 3_000,
  /** 산책 걷기 빠르기(원본 55px/s, 칸 32px). */
  speed: 55 / 32,
  /** 동시에 순찰하는 담당 수의 상한. */
  maxWalkers: 2,
  /** 이 시간 안에 닿지 못하면(길이 막힘) 그만두고 자리로. */
  giveUpMs: 30_000,
} as const;

/** 한 번 셀 때 담당 하나: 순찰해도 되는가(한가 · 회의 아님 · 부름 · 멘션으로 걷지 않음). */
export interface PatrolAgent {
  id: string;
  free: boolean;
}

/** 걷기 지시: `to`가 있으면 그 칸으로, `null`이면 자리로. `cancel`이면 순찰을 버리고 바로 보통 걸음으로 자리로. */
export interface PatrolOrder {
  agent: string;
  to: Point | null;
  cancel?: boolean;
}

/** 순찰할 곳을 고를 때 보는 지도 부분. */
export interface PatrolMap {
  rooms: { id: string; x: number; y: number; w: number; h: number }[];
  furniture: { kind: string; x: number; y: number; w: number; h: number }[];
  agents: Record<string, { desk: Point; rest: Point }>;
  meeting_seats: Point[];
}

/** 담당의 자리 · 일로 순찰해도 되는가: 일이 한가하고 회의 자리가 아니며 다른 걸음(부름 · 멘션)이 없다. */
export function isFree(view: { place: string; work: string }, steered: boolean): boolean {
  return view.work === "idle" && view.place !== "meeting" && !steered;
}

/** 들르는 방(논의하는 미팅룸 · 담당 책상이 있는 워크스테이션은 빼고). */
const SPOT_ROOMS = ["lounge", "entry-lounge", "round-meeting", "director"];
/** 곁에 들르는 가구(탕비 · 복사기 · 화이트보드). */
const SPOT_FURNITURE = ["studio_counter", "office_printer", "whiteboard"];
/** 논의하는 미팅룸(순찰로 들어가지 않는다). */
const MEETING_ROOM = "conference";
/** 걸을 수 있는 칸을 찾는 가장 먼 거리(칸). */
const SEARCH_RADIUS = 4;

function key(p: Point): string {
  return `${p.x},${p.y}`;
}

/** `from` 둘레에서 가까운 차례로 조건에 맞는 첫 칸(없으면 `null`). */
function nearestCell(from: Point, ok: (p: Point) => boolean): Point | null {
  for (let r = 0; r <= SEARCH_RADIUS; r++) {
    for (let dy = -r; dy <= r; dy++) {
      for (let dx = -r; dx <= r; dx++) {
        if (Math.max(Math.abs(dx), Math.abs(dy)) !== r) continue;
        const p = { x: from.x + dx, y: from.y + dy };
        if (ok(p)) return p;
      }
    }
  }
  return null;
}

/** 지도에서 순찰할 곳(걸을 수 있고 담당 자리가 아닌 칸, 겹치지 않음): 방 가운데 · 가구 곁. */
export function patrolSpots(map: PatrolMap, walkable: (x: number, y: number) => boolean): Point[] {
  const reserved = new Set<string>();
  for (const a of Object.values(map.agents)) { reserved.add(key(a.desk)); reserved.add(key(a.rest)); }
  for (const s of map.meeting_seats) reserved.add(key(s));
  const meeting = map.rooms.filter((r) => r.id === MEETING_ROOM);
  const inMeeting = (p: Point) => meeting.some((r) => p.x >= r.x && p.x < r.x + r.w && p.y >= r.y && p.y < r.y + r.h);
  const out: Point[] = [];
  const ok = (p: Point) =>
    walkable(p.x, p.y) && !reserved.has(key(p)) && !inMeeting(p) && !out.some((o) => key(o) === key(p));
  const add = (p: Point | null) => { if (p) out.push(p); };
  for (const id of SPOT_ROOMS) {
    const room = map.rooms.find((r) => r.id === id);
    if (!room) continue;
    const inRoom = (p: Point) => p.x >= room.x && p.x < room.x + room.w && p.y >= room.y && p.y < room.y + room.h;
    add(nearestCell({ x: room.x + Math.floor(room.w / 2), y: room.y + Math.floor(room.h / 2) }, (p) => inRoom(p) && ok(p)));
  }
  for (const f of map.furniture) {
    if (!SPOT_FURNITURE.includes(f.kind)) continue;
    add(nearestCell({ x: f.x + Math.floor(f.w / 2), y: f.y + Math.floor(f.h / 2) }, ok));
  }
  return out;
}

/** 담당 하나의 순찰 상태. */
type State =
  | { phase: "seated"; idleSince: number | null; wait: number }
  | { phase: "going"; stops: Point[]; index: number; since: number }
  | { phase: "dwelling"; stops: Point[]; index: number; until: number }
  | { phase: "returning"; since: number };

const SEATED: Extract<State, { phase: "seated" }> = { phase: "seated", idleSince: null, wait: 0 };

/** 순찰 일정 판. */
export class Patrol {
  private readonly spots: readonly Point[];
  private readonly random: () => number;
  private readonly states = new Map<string, State>();

  /** `spots`: 순찰할 곳. `random`은 [0, 1)을 주는 함수(시험은 고정 값을 넘긴다). */
  constructor(spots: readonly Point[], random: () => number = Math.random) {
    this.spots = spots;
    this.random = random;
  }

  private state(agent: string): State {
    return this.states.get(agent) ?? SEATED;
  }

  /** 그 담당이 지금 순찰 중인가(자리를 떠나 있음). */
  patrolling(agent: string): boolean {
    return this.state(agent).phase !== "seated";
  }

  private walkers(): number {
    return [...this.states.values()].filter((s) => s.phase !== "seated").length;
  }

  /** 들를 곳: 1 ~ [`PATROL_RULES.maxStops`]곳을 서로 다르게(넣어 준 난수로). */
  private pickStops(): Point[] {
    const count = Math.min(this.spots.length, 1 + Math.floor(this.random() * PATROL_RULES.maxStops));
    const left = [...this.spots];
    const stops: Point[] = [];
    while (stops.length < count && left.length > 0) {
      const i = Math.min(left.length - 1, Math.floor(this.random() * left.length));
      stops.push(left.splice(i, 1)[0]);
    }
    return stops;
  }

  /** 자리에 앉아 한가한 담당: 기다린 시간이 차고 자리가 있으면 순찰을 시작한다. */
  private fromSeat(agent: string, s: Extract<State, { phase: "seated" }>, now: number): PatrolOrder | null {
    if (s.idleSince === null) {
      const wait = PATROL_RULES.idleMinMs + this.random() * (PATROL_RULES.idleMaxMs - PATROL_RULES.idleMinMs);
      this.states.set(agent, { phase: "seated", idleSince: now, wait });
      return null;
    }
    if (now - s.idleSince < s.wait || this.spots.length === 0 || this.walkers() >= PATROL_RULES.maxWalkers) return null;
    const stops = this.pickStops();
    if (stops.length === 0) return null;
    this.states.set(agent, { phase: "going", stops, index: 0, since: now });
    return { agent, to: stops[0] };
  }

  private step(agent: string, free: boolean, now: number): PatrolOrder | null {
    const s = this.state(agent);
    if (!free) {
      this.states.set(agent, SEATED);
      return s.phase === "seated" ? null : { agent, to: null, cancel: true };
    }
    switch (s.phase) {
      case "seated":
        return this.fromSeat(agent, s, now);
      case "going":
        if (now - s.since < PATROL_RULES.giveUpMs) return null;
        this.states.set(agent, { phase: "returning", since: now });
        return { agent, to: null };
      case "dwelling": {
        if (now < s.until) return null;
        const next = s.index + 1;
        if (next < s.stops.length) {
          this.states.set(agent, { phase: "going", stops: s.stops, index: next, since: now });
          return { agent, to: s.stops[next] };
        }
        this.states.set(agent, { phase: "returning", since: now });
        return { agent, to: null };
      }
      case "returning":
        if (now - s.since < PATROL_RULES.giveUpMs) return null;
        this.states.set(agent, SEATED);
        return { agent, to: null, cancel: true };
    }
  }

  /** 지금(`now`) 담당들을 보고 걷기 지시들을 돌려준다(없으면 빈 목록). */
  tick(agents: readonly PatrolAgent[], now: number): PatrolOrder[] {
    const out: PatrolOrder[] = [];
    for (const a of agents) {
      const order = this.step(a.id, a.free, now);
      if (order) out.push(order);
    }
    return out;
  }

  /** 걷던 담당이 `now`에 닿았다: 들를 곳이면 머물기, 자리면 다시 한가를 센다. */
  arrived(agent: string, now: number): void {
    const s = this.state(agent);
    if (s.phase === "going") {
      this.states.set(agent, { phase: "dwelling", stops: s.stops, index: s.index, until: now + PATROL_RULES.dwellMs });
    } else if (s.phase === "returning") {
      this.states.set(agent, SEATED);
      this.fromSeat(agent, SEATED, now);
    }
  }
}
