// 순찰을 3D 사무실에 잇는다(server 24차 1번, additions.md R5-13 · N16 · N17 · V65). 일정은 순수 모듈(patrol.ts)이
// 갖고, 여기서는 250ms마다 담당들의 자리 · 일을 넘기고 지시대로 장면의 `steerAgent`로 걷게 한다(화면만, 서버에 보내지
// 않는다). 부르기 · 멘션 걸음(summon-scene)이 건 담당은 순찰하지 않는다: 장면의 `steerAgent`를 감싸 다른 쪽이 건
// 걸음을 알아챈다(자리로 돌아와 닿거나 풀면 다시 한가로 센다).
// 가벼운 화면(장면 없음) · 움직임 줄이기(`prefers-reduced-motion`) · 숨은 탭이면 순찰하지 않는다.

import { buildGrid, type NavMap } from "./nav";
import { PATROL_RULES, Patrol, isFree, patrolSpots, type PatrolMap, type PatrolOrder } from "./patrol";
import type { Scene } from "./scene3d";

/** 순찰을 굴리는 간격(밀리초). */
const TICK_MS = 250;

/** 장면의 걷기 지시 한 벌(actors3d `Steer`와 같은 모양). */
type SteerOf = Parameters<Scene["steerAgent"]>[1];

/** 순찰에 넘기는 것. */
export interface PatrolOptions {
  map: PatrolMap & NavMap;
  /** 지금의 3D 장면(가벼운 화면이면 `null`). */
  view: () => Scene | null;
  /** 담당마다 지금의 자리 · 일(서버가 정본). */
  agents: () => ReadonlyMap<string, { place: string; work: string }>;
  /** 가벼운 화면이 보이는가. */
  light: () => boolean;
  random?: () => number;
}

/** 순찰을 시작한다. 멈추는 함수를 돌려준다. */
export function startPatrol(options: PatrolOptions): () => void {
  const grid = buildGrid(options.map);
  const spots = patrolSpots(options.map, (x, y) => grid.walkable(x, y));
  const reduced = typeof window.matchMedia === "function" ? window.matchMedia("(prefers-reduced-motion: reduce)") : null;
  let patrol = new Patrol(spots, options.random);
  /** 다른 쪽(부르기 · 멘션)이 걸음을 건 담당. */
  const external = new Set<string>();
  let wrapped: Scene | null = null;
  let base: Scene["steerAgent"] | null = null;
  let mine = false;

  /** 새 장면이면 걸음 걸기를 감싸고 순찰을 처음부터 센다. */
  function wrap(view: Scene): void {
    if (wrapped === view) return;
    wrapped = view;
    external.clear();
    patrol = new Patrol(spots, options.random);
    const inner = view.steerAgent.bind(view);
    base = inner;
    view.steerAgent = (id: string, steer: SteerOf) => {
      if (!mine) inner(id, noteExternal(id, steer));
      else inner(id, steer);
    };
  }

  /** 다른 쪽이 건 걸음을 적는다: 풀면 지우고, 자리로 돌아가는 걸음이면 닿을 때 지운다. */
  function noteExternal(id: string, steer: SteerOf): SteerOf {
    if (steer === null) { external.delete(id); return steer; }
    external.add(id);
    if (steer.to !== null) return steer;
    const done = steer.onArrive;
    return { ...steer, onArrive: () => { external.delete(id); done?.(); } };
  }

  function steer(id: string, value: SteerOf): void {
    if (!base) return;
    mine = true;
    try { base(id, value); } finally { mine = false; }
  }

  function send(order: PatrolOrder): void {
    if (order.cancel) {
      if (!external.has(order.agent)) steer(order.agent, null);
      return;
    }
    const agent = order.agent;
    steer(agent, { to: order.to, speed: PATROL_RULES.speed, onArrive: () => patrol.arrived(agent, Date.now()) });
  }

  function tick(): void {
    const view = options.view();
    if (!view) { wrapped = null; base = null; return; }
    wrap(view);
    if (document.hidden) return;
    const off = options.light() || Boolean(reduced?.matches);
    const agents = [...options.agents()].map(([id, v]) => ({ id, free: !off && isFree(v, external.has(id)) }));
    for (const order of patrol.tick(agents, Date.now())) send(order);
  }

  const timer = window.setInterval(tick, TICK_MS);
  return () => window.clearInterval(timer);
}
