// 내 캐릭터를 사무실 장면에 붙이기(server 32차 3 · 4 · 5 · 7 · 2번): 몸(player3d.ts) · 방향키(player-keys.ts) ·
// 걷기(player.ts) · 바닥 · 담당 누르기(pathfind.ts · nearby.ts) · 2칸 안내 · `/` · "누구와 대화?" · 멀어지면 닫힘 ·
// 자리 띄엄띄엄 남기기(player-sync.ts). 장면(scene3d.ts)이 프레임마다 `step`을 부르고, 누르기를 `tapAgent` ·
// `tapFloor`로 넘긴다. 대화 탭을 여닫는 일은 사무실 입구(main.ts)가 `hooks`로 한다.

import { buildGrid, type Grid, type NavMap, type Point } from "./nav";
import { cellOf, entranceCell, startCell, withBlockers, type EntranceMap } from "./pathfind";
import { ArrowState, PlayerWalker } from "./player";
import { createPlayerBody } from "./player3d";
import { arrowOf, isTalkKey, isTypingTarget, preventsDefault } from "./player-keys";
import { SpotSender, postSpot, type Fetcher, type Me, type Spot } from "./player-sync";
import { agentClick, distance, nearbyAgents, slashAction, talkHint, tooFar, TALK_RADIUS } from "./nearby";
import { setHint, showChooser } from "./toolbar";

type Obj = any;

/** 한 프레임 걷기의 가장 긴 시간(초, 원본 키 이동 `min(delta, 100ms)`). */
const MAX_STEP_SECONDS = 0.1;
/** 이만큼 넘게 움직였으면 걷는 모습. */
const MOVED_EPSILON = 1e-4;

/** 장면 밖(사무실 입구)이 하는 일. */
export interface PlayerHooks {
  /** 그 담당과 1:1 대화를 연다. */
  openChat(agent: string): void;
  /** 그 담당과의 대화가 열려 있으면 닫는다. */
  closeChat(agent: string): void;
  /** 지금 열린 대화(팀이면 "team", 아니면 담당 id). */
  conversation(): string;
  /** 같은 출처 fetch(자리 남기기). */
  fetcher: Fetcher;
  /** 내 자리가 바뀌었다(장면을 다시 만들 때 그 자리에서 시작하게 기억한다). */
  onSpot?(spot: Spot): void;
}

/** 담당 하나의 지금 자리(세계 좌표 — x · z, 칸 단위). */
export interface AgentPos {
  id: string;
  x: number;
  z: number;
}

export interface PlayerSceneOptions {
  scene: Obj;
  labels: HTMLElement;
  /** 안내 글 · 목록을 다는 사무실 층. 여기에 초점이 있을 때만 방향키 · `/`의 스크롤을 막는다. */
  stage: HTMLElement;
  map: NavMap & EntranceMap;
  names: Record<string, string>;
  me: Me | null;
  hooks: PlayerHooks;
  /** 담당들의 지금 자리. */
  agents(): AgentPos[];
}

/** 장면에 붙은 내 캐릭터. */
export interface PlayerScene {
  /** 지금 자리(칸 단위 실수 — 세계 x · z). */
  pos(): Point;
  step(dt: number, t: number, wall: number): void;
  placeTag(camera: Obj, width: number, height: number): void;
  /** 담당을 눌렀다. */
  tapAgent(id: string): void;
  /** 바닥 칸을 눌렀다. 걸어가기 시작했으면 참. */
  tapFloor(cell: Point): boolean;
  setLook(look: string): void;
  dispose(): void;
}

const DEFAULT_LOOK = "office-jun";
const DEFAULT_NAME = "나";

export function createPlayerScene(options: PlayerSceneOptions): PlayerScene {
  const { scene, labels, stage, map, names, me, hooks } = options;
  const base: Grid = buildGrid(map);
  const doc = stage.ownerDocument;
  const saved = me ? { x: me.x, y: me.y } : null;
  const start = startCell(base, saved, entranceCell(map));
  const walker = new PlayerWalker(base, { x: start.x + 0.5, y: start.y + 0.5 }, me?.dir ?? "up");
  const body = createPlayerBody(scene, labels, me?.name ?? DEFAULT_NAME, me?.look ?? DEFAULT_LOOK);
  const arrows = new ArrowState();
  const sender = new SpotSender((spot) => postSpot(hooks.fetcher, spot), me ? { x: me.x, y: me.y, dir: me.dir } : null);

  const hint = doc.createElement("div");
  hint.className = "talk-hint";
  hint.setAttribute("role", "status");
  hint.setAttribute("aria-live", "polite");
  hint.hidden = true;
  stage.appendChild(hint);
  let chooser: HTMLElement | null = null;
  let chooserFor = "";
  /** 걸어가서 닿으면 대화를 열 담당. */
  let openOnArrival: string | null = null;
  /** 다가가 연 대화(멀어지면 닫는다). */
  let talkingTo: string | null = null;
  let near: string[] = [];
  let lastSpot = "";

  const nameOf = (id: string) => names[id] ?? id;

  function agentCells(): Point[] {
    const mine = walker.cell();
    // 내가 선 칸은 막지 않는다(담당이 지나가며 겹쳐도 갇히지 않게).
    return options.agents().map((a) => cellOf({ x: a.x, y: a.z })).filter((c) => c.x !== mine.x || c.y !== mine.y);
  }

  function agentAt(id: string): AgentPos | undefined {
    return options.agents().find((a) => a.id === id);
  }

  function open(id: string): void {
    closeChooser();
    openOnArrival = null;
    talkingTo = id;
    hooks.openChat(id);
  }

  function closeChooser(): void {
    chooser?.remove();
    chooser = null;
    chooserFor = "";
  }

  function slash(): void {
    const action = slashAction(near);
    if (action.kind === "open") { open(action.agent); return; }
    if (action.kind === "choose") {
      chooser = showChooser(stage, action.agents.map((id) => ({ id, name: nameOf(id) })), (id) => open(id));
      chooserFor = action.agents.join();
      chooser.querySelector<HTMLButtonElement>("button[data-agent]")?.focus();
    }
  }

  function floorFocused(): boolean {
    const active = doc.activeElement;
    return !active || active === doc.body || stage.contains(active);
  }

  const onKeyDown = (event: KeyboardEvent) => {
    if (event.key === "Escape" && chooser) { closeChooser(); return; }
    const arrow = arrowOf(event);
    if (arrow) { arrows.press(arrow); }
    if (preventsDefault(event, floorFocused())) { event.preventDefault(); }
    if (isTalkKey(event) && !event.repeat) { slash(); }
  };
  const onKeyUp = (event: KeyboardEvent) => { arrows.release(event.key); };
  const onBlur = () => { arrows.clear(); };
  const onFocusIn = (event: FocusEvent) => { if (isTypingTarget(event.target)) { arrows.clear(); } };
  const onLeave = () => { sender.flush(); };
  const onHidden = () => { if (doc.visibilityState === "hidden") { sender.flush(); } };
  const view = doc.defaultView ?? window;
  view.addEventListener("keydown", onKeyDown);
  view.addEventListener("keyup", onKeyUp);
  view.addEventListener("blur", onBlur);
  view.addEventListener("pagehide", onLeave);
  doc.addEventListener("focusin", onFocusIn);
  doc.addEventListener("visibilitychange", onHidden);

  function step(dt: number, t: number, wall: number): void {
    const blocked = withBlockers(base, agentCells());
    walker.setGrid(blocked);
    if (arrows.any()) { openOnArrival = null; }
    const before = walker.pos();
    const wasFollowing = walker.following();
    walker.step(arrows.held(), Math.min(dt, MAX_STEP_SECONDS));
    const pos = walker.pos();
    const walking = Math.hypot(pos.x - before.x, pos.y - before.y) > MOVED_EPSILON;
    body.update(pos, walker.facing(), walking, dt, t);

    const agents = options.agents();
    near = nearbyAgents(pos, agents.map((a) => ({ agent: a.id, x: a.x, y: a.z })));
    // 걸어가서 닿았으면 연다(그사이 담당이 떠났으면 열지 않는다).
    if (openOnArrival && wasFollowing && !walker.following()) {
      const target = agentAt(openOnArrival);
      const id = openOnArrival;
      openOnArrival = null;
      if (target && distance(pos, { x: target.x, y: target.z }) < TALK_RADIUS) { open(id); }
    }
    // 다가가 연 대화는 멀어지면 닫는다(사용자가 다른 탭으로 옮겼으면 놓아 준다).
    if (talkingTo && hooks.conversation() !== talkingTo) { talkingTo = null; }
    if (talkingTo) {
      const target = agentAt(talkingTo);
      if (!target || tooFar(pos, { x: target.x, y: target.z })) {
        hooks.closeChat(talkingTo);
        talkingTo = null;
      }
    }
    setHint(hint, near.length > 0 ? talkHint(nameOf(near[0])) : null);
    if (chooser && near.join() !== chooserFor) { closeChooser(); }

    const cell = walker.cell();
    const spot: Spot = { x: cell.x, y: cell.y, dir: walker.facing() };
    const key = `${spot.x},${spot.y},${spot.dir}`;
    if (key !== lastSpot) {
      lastSpot = key;
      hooks.onSpot?.(spot);
    }
    sender.moved(spot, wall);
  }

  return {
    pos: () => walker.pos(),
    step,
    placeTag: (camera, width, height) => body.placeTag(camera, width, height),
    tapAgent(id: string): void {
      const target = agentAt(id);
      if (!target) { return; }
      const cell = cellOf({ x: target.x, y: target.z });
      const others = agentCells();
      const blocked = withBlockers(base, others.some((c) => c.x === cell.x && c.y === cell.y) ? others : [...others, cell]);
      const click = agentClick(blocked, walker.pos(), cell);
      if (click.kind === "open") { open(id); return; }
      if (click.kind === "walk") {
        walker.setGrid(blocked);
        if (walker.walkTo(click.to)) { openOnArrival = id; }
      }
    },
    tapFloor(cell: Point): boolean {
      walker.setGrid(withBlockers(base, agentCells()));
      openOnArrival = null;
      return walker.walkTo(cell);
    },
    setLook: (look: string) => body.setLook(look),
    dispose(): void {
      sender.flush();
      view.removeEventListener("keydown", onKeyDown);
      view.removeEventListener("keyup", onKeyUp);
      view.removeEventListener("blur", onBlur);
      view.removeEventListener("pagehide", onLeave);
      doc.removeEventListener("focusin", onFocusIn);
      doc.removeEventListener("visibilitychange", onHidden);
      closeChooser();
      hint.remove();
      body.dispose();
    },
  };
}
