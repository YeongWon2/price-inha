// 담당 여섯의 3D 몸 · 걷기 · 자세 · 이름표(server 20차 2번 · 고침).
// - 몸: 원본 외형 GLB(actor.ts `makeActor` + gltf.ts). 부르는 동안 · 실패하면 코드 인형(doll.ts).
// - 클립: 걸으면 walk, 걷지 않고 자기 좌석(업무 · 쉼 · 논의 자리) 가까우면 sit, 아니면 idle. 바꿀 때 0.22초 교차.
//   원본처럼 달리기 클립은 없고, 빠르게 움직이면(논의 소집) 걷기 클립을 빨리 돌리고 앞으로 기울이고 튄다.
// - 길: A*(nav.ts), 걷기 150px/s · 소집 300px/s(칸 32px), 길을 못 찾으면 1초마다 다시.
// - 쉼 자리(`place: "bed"`)는 라운지 · 탕비실 의자다(누운 자세 없음).
// - (23차 1 · 2번) 이름표는 발 아래(안은 이름뿐), 말풍선 기준점은 머리 위. 자리는 bubbles3d가 layout.ts로 정한다.
//   머리 기준점은 원본(키 2.7에 서 있음 2.7 · 앉음 2.0)을 우리 캐릭터의 실제 키(불러온 모델의 높이)에 비례해 맞춘다.
// - (33차) 부르기 · 멘션 걸어오기: `steer`로 자리 그리기 대신 그 칸까지 그 빠르기로 걷고(닿으면 `onArrive`),
//   `to`가 `null`이면 자기 자리로 돌아가 닿은 뒤 자리 그리기로 돌아간다.

import * as THREE from "three";
import { CROSSFADE_SECONDS, clipName, defaultLook, makeActor, type AgentId } from "./actor";
import { poseDoll, makeDoll, removeDoll, type Doll } from "./doll";
import { instance, loadLook } from "./gltf";
import { clearLine, findPath, type Grid, type Point } from "./nav";
import type { ActorSpot } from "./bubbles3d";
import { nameTagText } from "./layout";
import type { Rect } from "./speech";
import type { Walker } from "./smalltalk";
import { facingYaw } from "./furniture";
import type { AgentLook, OfficeMap } from "./scene3d";

type Obj = any;

const TILE_PX = 32;
const WALK_SPEED = 150 / TILE_PX;      // 칸/초 (원본 walk 150px/s — 걷기 클립이 미끄러지지 않는 빠르기)
const SUMMON_SPEED = 300 / TILE_PX;    // 논의 소집(원본 meetingSummon 300px/s)
const ARRIVE = 2 / TILE_PX;            // 경유점 도착 판정 2px
const SEAT_RADIUS = 0.22;              // 좌석에서 이만큼 안이면 앉은 모습
const RETRY_MS = 1000;
const RUN_LEAN = 0.18;                 // 원본 달리기 기울기 · 튐
const RUN_BOUNCE = 0.05;
// 원본 캐릭터 키 2.7에서 말풍선 기준점: 서 있음 2.7 · 앉음 2.0(docs/research/deskrpg.md 11절). 우리 키에 비례해 쓴다.
const ORIGIN_HEIGHT = 2.7;
const ORIGIN_HEAD_STAND = 2.7;
const ORIGIN_HEAD_SIT = 2.0;
// 키를 아직 못 쟀을 때(원본 외형 · 코드 인형 모두 약 1.85 — framing.ts의 머리 높이 1.9와 같은 크기).
const DEFAULT_HEIGHT = 1.85;
const MIN_HEIGHT = 1.0;
const MAX_HEIGHT = 4.0;

/** 몸의 키(세계 단위)를 잰다. 못 재면 `null`. */
function measureHeight(obj: Obj): number | null {
  obj.updateMatrixWorld(true);
  const box = new THREE.Box3().setFromObject(obj);
  const h = box.max.y - Math.max(0, box.min.y);
  return Number.isFinite(h) && h >= MIN_HEIGHT && h <= MAX_HEIGHT ? h : null;
}

/** 머리 기준점의 높이: 원본 비율(서 있음 2.7/2.7 · 앉음 2.0/2.7) × 우리 키. */
function headHeight(height: number, sitting: boolean): number {
  return (height * (sitting ? ORIGIN_HEAD_SIT : ORIGIN_HEAD_STAND)) / ORIGIN_HEIGHT;
}

interface Anchor extends Point { facing: number }

/** (33차) 부르기 걸음: 그 칸(`null`이면 자기 자리)까지 그 빠르기로 걷고 닿으면 그쪽(yaw)을 보고 `onArrive`. */
export interface Steer {
  to: Point | null;
  speed: number;
  yaw?: number;
  onArrive?: () => void;
}

interface Body {
  look: string;
  model: Obj;
  mixer: Obj;
  actions: Map<string, Obj>;
  current: Obj;
}

interface Actor {
  id: string;
  root: Obj;
  doll: Doll | null;
  body: Body | null;
  wanted: string;
  tag: HTMLElement;
  phase: number;
  x: number;
  y: number;
  yaw: number;
  view: AgentLook;
  target: Anchor | null;
  path: Point[] | null;
  speed: number;
  retryAt: number;
  walking: boolean;
  label: string;
  /** 몸의 키(세계 단위, 몸이 바뀔 때 잰다). */
  height: number;
  headY: number;
  /** 이름표 크기(글이 바뀔 때 잰다). */
  tagSize: { width: number; height: number } | null;
  /** (22차 4번) 지나가며 잡담으로 멈춰 있다. */
  paused: boolean;
  /** (33차) 부르기 걸음(없으면 자리 그리기). */
  steer: Steer | null;
}

export interface ActorsOptions {
  scene: Obj;
  labels: HTMLElement;
  map: OfficeMap;
  names: Record<string, string>;
  looks: Record<string, string>;
  grid: Grid;
}

function angleDiff(a: number, b: number): number {
  let d = a - b;
  while (d > Math.PI) { d -= Math.PI * 2; }
  while (d < -Math.PI) { d += Math.PI * 2; }
  return d;
}

function disposeBody(body: Body): void {
  body.mixer.stopAllAction();
  body.mixer.uncacheRoot(body.model);
  body.model.parent?.remove(body.model);
}

export function createActors({ scene, labels, map, names, looks, grid }: ActorsOptions) {
  const actors = new Map<string, Actor>();
  const ids = Object.keys(map.agents);
  const fixedSeats = map.meeting_seats.length >= ids.length;
  const meetingCenter = (() => {
    const s = map.meeting_seats;
    const n = Math.max(1, s.length);
    return { x: s.reduce((a, p) => a + p.x + 0.5, 0) / n, y: s.reduce((a, p) => a + p.y + 0.5, 0) / n };
  })();
  let disposed = false;

  for (const [i, id] of ids.entries()) {
    const root = new THREE.Group();
    root.userData.agentId = id;
    scene.add(root);
    const tag = document.createElement("div");
    tag.className = "tag";
    labels.appendChild(tag);
    const rest = map.agents[id].rest;
    const actor: Actor = {
      id, root, doll: makeDoll(root, id), body: null, wanted: "", tag, phase: i * 1.7,
      x: rest.x + 0.5, y: rest.y + 0.5, yaw: facingYaw(map.agents[id].rest_facing),
      view: { place: "bed", work: "idle" },
      target: null, path: null, speed: WALK_SPEED, retryAt: 0, walking: false, label: "", height: DEFAULT_HEIGHT,
      headY: DEFAULT_HEIGHT, tagSize: null, paused: false, steer: null,
    };
    tagTree(actor);
    actors.set(id, actor);
    setLook(id, looks[id] ?? defaultLook(id as AgentId));
  }

  function tagTree(actor: Actor): void { actor.root.traverse((o: Obj) => { o.userData.agentId = actor.id; }); }

  /** 외형을 바꾼다: 원본 GLB를 부르고, 되면 인형을 치운다. 실패하면 인형으로 남는다. */
  function setLook(id: string, look: string): void {
    const actor = actors.get(id);
    if (!actor || actor.wanted === look) { return; }
    actor.wanted = look;
    void makeActor(look, loadLook).then((made) => {
      if (disposed || actor.wanted !== look) { return; }
      if (actor.body) { disposeBody(actor.body); actor.body = null; }
      if (made.kind === "doll") {
        if (!actor.doll) { actor.doll = makeDoll(actor.root, id); tagTree(actor); }
        actor.height = measureHeight(actor.root) ?? DEFAULT_HEIGHT;
        return;
      }
      const model = instance(made.model.scene, true);
      const mixer = new THREE.AnimationMixer(model);
      const actions = new Map<string, Obj>();
      for (const clip of (made.model.animations ?? []) as Obj[]) { actions.set(String(clip.name).toLowerCase(), mixer.clipAction(clip)); }
      actor.root.add(model);
      if (actor.doll) { removeDoll(actor.doll); actor.doll = null; }
      actor.body = { look, model, mixer, actions, current: null };
      actor.height = measureHeight(model) ?? DEFAULT_HEIGHT;
      tagTree(actor);
    });
  }

  // 좌석 칸(의자 · 소파 · 안락의자). 쉼 자리가 좌석이 아니면(라운지 탁자 둘레) 서 있는다.
  const seatCells = new Set<string>();
  for (const f of map.furniture) {
    if (!["chair", "studio_sofa", "office_armchair"].includes(f.kind)) { continue; }
    for (let dy = 0; dy < f.h; dy++) { for (let dx = 0; dx < f.w; dx++) { seatCells.add(`${f.x + dx},${f.y + dy}`); } }
  }
  function seatsOf(actor: Actor): Point[] {
    const a = map.agents[actor.id];
    return [a.desk, a.rest, ...map.meeting_seats].filter((p) => seatCells.has(`${p.x},${p.y}`));
  }

  function anchorOf(actor: Actor): Anchor {
    const to = actor.steer?.to;
    if (to) { return { x: to.x, y: to.y, facing: actor.steer?.yaw ?? actor.yaw }; }
    const a = map.agents[actor.id];
    if (actor.view.place === "desk") { return { x: a.desk.x, y: a.desk.y, facing: facingYaw(a.desk_facing) }; }
    if (actor.view.place === "meeting" && map.meeting_seats.length > 0) {
      // 자리가 담당 수만큼 있으면 담당마다 제 자리(지도 순서 = 담당 순서, 팀장은 탁자 머리)에 앉는다 — 누가 오가도
      // 다른 담당이 자리를 바꿔 걷지 않는다(발언자 샷 도중 발언자가 옮겨 가지 않게). 모자라면 온 순서로 앉는다.
      const order = fixedSeats ? ids : ids.filter((id) => actors.get(id)?.view.place === "meeting");
      const seat = map.meeting_seats[Math.max(0, order.indexOf(actor.id)) % map.meeting_seats.length];
      const yaw = seat.facing ? facingYaw(seat.facing)
        : Math.atan2(meetingCenter.x - (seat.x + 0.5), meetingCenter.y - (seat.y + 0.5));
      return { x: seat.x, y: seat.y, facing: yaw };
    }
    return { x: a.rest.x, y: a.rest.y, facing: facingYaw(a.rest_facing) };
  }

  function placeAt(actor: Actor, anchor: Anchor): void {
    actor.x = anchor.x + 0.5;
    actor.y = anchor.y + 0.5;
    actor.yaw = anchor.facing;
    actor.path = null;
    actor.walking = false;
  }

  function retarget(actor: Actor, snap: boolean): void {
    const anchor = anchorOf(actor);
    actor.target = anchor;
    if (snap) { placeAt(actor, anchor); return; }
    actor.path = findPath(grid, { x: Math.floor(actor.x), y: Math.floor(actor.y) }, anchor);
    actor.retryAt = actor.path ? 0 : performance.now() + RETRY_MS;
    actor.walking = Boolean(actor.path);
  }

  /** 담당 모습을 바꾼다. snap이면 걷지 않고 바로 놓는다(접속 스냅샷). */
  function setAgent(id: string, view: AgentLook, { snap = false }: { snap?: boolean } = {}): void {
    const actor = actors.get(id);
    if (!actor) { return; }
    const placeChanged = actor.view.place !== view.place;
    actor.view = { place: view.place, work: view.work };
    actor.speed = actor.steer?.speed ?? (view.place === "meeting" ? SUMMON_SPEED : WALK_SPEED);
    if (placeChanged || snap || !actor.target) {
      retarget(actor, snap);
      for (const other of actors.values()) {
        if (!fixedSeats && other !== actor && other.view.place === "meeting") { retarget(other, snap); }
      }
    }
  }

  function step(actor: Actor, dt: number, now: number): void {
    if (!actor.target || actor.paused) { return; }
    if (!actor.path) {
      const there = Math.hypot(actor.x - (actor.target.x + 0.5), actor.y - (actor.target.y + 0.5)) < ARRIVE;
      if (!there && actor.retryAt && now >= actor.retryAt) { retarget(actor, false); }
      return;
    }
    let budget = actor.speed * Math.min(dt, 0.1);
    while (budget > 0 && actor.path && actor.path.length > 0) {
      const wp = actor.path[0];
      const dx = wp.x - actor.x;
      const dy = wp.y - actor.y;
      const dist = Math.hypot(dx, dy);
      if (dist < ARRIVE) { actor.path.shift(); continue; }
      const move = Math.min(dist, budget);
      actor.x += (dx / dist) * move;
      actor.y += (dy / dist) * move;
      budget -= move;
      actor.yaw += angleDiff(Math.atan2(dx, dy), actor.yaw) * (1 - Math.exp(-14 * Math.min(dt, 0.1)));
    }
    if (actor.path && actor.path.length === 0) { placeAt(actor, actor.target); arrive(actor); }
  }

  /** (33차) 부르기 걸음이 닿았다: 알리고, 자기 자리로 돌아온 것이면 자리 그리기로 돌아간다. */
  function arrive(actor: Actor): void {
    const steer = actor.steer;
    if (!steer) { return; }
    const done = steer.onArrive;
    steer.onArrive = undefined;
    if (!steer.to) {
      actor.steer = null;
      actor.speed = actor.view.place === "meeting" ? SUMMON_SPEED : WALK_SPEED;
    }
    done?.();
  }

  /** (33차) 부르기 걸음을 건다(`null`이면 바로 자리 그리기로). */
  function steerAgent(id: string, steer: Steer | null): void {
    const actor = actors.get(id);
    if (!actor) { return; }
    actor.steer = steer;
    actor.speed = steer?.speed ?? (actor.view.place === "meeting" ? SUMMON_SPEED : WALK_SPEED);
    retarget(actor, false);
  }

  function seated(actor: Actor): boolean {
    return seatsOf(actor).some((p) => Math.hypot(actor.x - (p.x + 0.5), actor.y - (p.y + 0.5)) <= SEAT_RADIUS);
  }

  function animateBody(actor: Actor, body: Body, clip: string, dt: number): void {
    const next = body.actions.get(clip) ?? body.actions.get("idle");
    if (next && next !== body.current) {
      next.reset().setEffectiveTimeScale(1).setEffectiveWeight(1).play();
      if (body.current) { next.crossFadeFrom(body.current, CROSSFADE_SECONDS, false); }
      body.current = next;
    }
    const running = clip === "walk" && actor.speed > WALK_SPEED * 1.2;
    if (body.current && clip === "walk") {
      body.current.setEffectiveTimeScale(running ? Math.min(2.6, Math.max(1.2, actor.speed / WALK_SPEED)) : 1);
    }
    body.mixer.update(dt);
    body.model.rotation.x = running ? RUN_LEAN : 0;
    const cur = body.current;
    const cycle = cur ? (cur.time / Math.max(cur.getClip().duration, 1e-3)) * Math.PI * 2 : 0;
    body.model.position.y = running ? Math.abs(Math.sin(cycle)) * RUN_BOUNCE : 0;
  }

  function animate(actor: Actor, dt: number, t: number): void {
    actor.root.position.set(actor.x, 0, actor.y);
    actor.root.rotation.set(0, actor.yaw, 0);
    const moving = actor.walking && !actor.paused;
    const clip = clipName(moving, !moving && seated(actor));
    if (actor.body) { animateBody(actor, actor.body, clip, dt); }
    if (actor.doll) { poseDoll(actor.doll, clip, actor.view.work, t + actor.phase); }
    actor.headY = headHeight(actor.height, clip === "sit");
    // (23차 1번) 이름표 안은 이름뿐(💭 · 💤 같은 아이콘은 붙이지 않는다).
    const label = nameTagText(names[actor.id] ?? actor.id, actor.view.work, actor.view.place);
    if (label !== actor.label) {
      actor.tag.textContent = label;
      actor.tag.title = label;
      actor.label = label;
      actor.tagSize = null;
    }
  }

  /** 세계 점을 화면 점(px)으로. 카메라 뒤이거나 화면 밖이면 `inside`가 거짓. */
  function screenPoint(camera: Obj, x: number, y: number, z: number, width: number, height: number) {
    projected.set(x, y, z).project(camera);
    const inside = projected.z < 1 && Math.abs(projected.x) <= 1.05 && Math.abs(projected.y) <= 1.05;
    return { x: (projected.x + 1) / 2 * width, y: (1 - projected.y) / 2 * height, inside };
  }

  const projected = new THREE.Vector3();
  const raycaster = new THREE.Raycaster();
  const ndc = new THREE.Vector2();

  return {
    setAgent,
    setLook,
    steer: steerAgent,
    /** (33차) 담당의 지금 자리(칸 단위 실수). */
    at(id: string): Point | null {
      const actor = actors.get(id);
      return actor ? { x: actor.x, y: actor.y } : null;
    },
    update(dt: number, t: number, now: number): void {
      for (const actor of actors.values()) { step(actor, dt, now); animate(actor, dt, t); }
    },
    /** 담당들의 지금 자리(desk · meeting · bed). */
    places(): string[] { return [...actors.values()].map((a) => a.view.place); },
    snapAll(): void {
      for (const actor of actors.values()) { if (actor.target) { placeAt(actor, actor.target); } }
    },
    /**
     * (23차) 이번 프레임의 발 투영점(월드 y 0, 앉았으면 좌석 자리) · 머리 투영점(원본 비율 × 우리 키) · 이름표 크기.
     * 발이나 머리 어느 하나라도 화면 안이면 보인다(발언자 샷처럼 발이 화면 아래로 나가도 말풍선은 보인다).
     */
    spots(camera: Obj, width: number, height: number): ActorSpot[] {
      return [...actors.values()].map((actor) => {
        const foot = screenPoint(camera, actor.x, 0, actor.y, width, height);
        const head = screenPoint(camera, actor.x, actor.headY, actor.y, width, height);
        if (!actor.tagSize) {
          actor.tag.style.display = "";
          actor.tagSize = { width: actor.tag.offsetWidth, height: actor.tag.offsetHeight };
        }
        return {
          id: actor.id, visible: foot.inside || head.inside, work: actor.view.work,
          foot: { x: foot.x, y: foot.y }, head: { x: head.x, y: head.y }, tag: actor.tagSize,
        };
      });
    },
    /** 이름표를 그 자리에 놓는다(`null`이면 숨긴다). */
    placeTag(id: string, rect: Rect | null): void {
      const actor = actors.get(id);
      if (!actor) { return; }
      if (!rect) { actor.tag.style.display = "none"; return; }
      actor.tag.style.display = "";
      actor.tag.style.transform = `translate(${Math.round(rect.x)}px, ${Math.round(rect.y)}px)`;
    },
    /** 발언자 카메라가 볼 담당의 자세: 세계 자리 · 보는 쪽 · 키 · 머리 높이. */
    pose(id: string): { x: number; z: number; yaw: number; height: number; headY: number } | null {
      const actor = actors.get(id);
      return actor ? { x: actor.x, z: actor.y, yaw: actor.yaw, height: actor.height, headY: actor.headY } : null;
    },
    /** (22차 4번) 잡담을 볼 담당들(세계 좌표 · 한가 · 걷는 중). 바쁨은 말풍선 층이 채운다. */
    walkers(): Walker[] {
      return [...actors.values()].map((a) => ({
        id: a.id, x: a.x, z: a.y, idle: a.view.work === "idle" && a.view.place !== "meeting",
        walking: a.walking && !a.paused, busy: false,
      }));
    },
    /** 두 담당 사이가 트여 있는가(벽 · 가구에 막히지 않음). */
    canSee(a: string, b: string): boolean {
      const one = actors.get(a);
      const two = actors.get(b);
      return Boolean(one && two && clearLine(grid, { x: one.x, y: one.y }, { x: two.x, y: two.y }));
    },
    /** 잡담으로 멈출 담당을 적는다. */
    pause(isPaused: (id: string) => boolean): void {
      for (const actor of actors.values()) { actor.paused = isPaused(actor.id); }
    },
    pick(clientX: number, clientY: number, camera: Obj, canvas: HTMLElement): string | undefined {
      const rect = canvas.getBoundingClientRect();
      ndc.set(((clientX - rect.left) / rect.width) * 2 - 1, -((clientY - rect.top) / rect.height) * 2 + 1);
      raycaster.setFromCamera(ndc, camera);
      const hits = raycaster.intersectObjects([...actors.values()].map((a) => a.root), true);
      return hits.find((h: Obj) => h.object.userData.agentId)?.object.userData.agentId;
    },
    dispose(): void {
      disposed = true;
      for (const actor of actors.values()) {
        if (actor.body) { disposeBody(actor.body); }
        actor.tag.remove();
      }
    },
  };
}
