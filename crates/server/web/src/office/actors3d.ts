// 담당 여섯의 3D 몸 · 걷기 · 자세 · 이름표(server 20차 2번 · 고침).
// - 몸: 원본 외형 GLB(actor.ts `makeActor` + gltf.ts). 부르는 동안 · 실패하면 코드 인형(doll.ts).
// - 클립: 걸으면 walk, 걷지 않고 자기 좌석(업무 · 쉼 · 논의 자리) 가까우면 sit, 아니면 idle. 바꿀 때 0.22초 교차.
//   원본처럼 달리기 클립은 없고, 빠르게 움직이면(논의 소집) 걷기 클립을 빨리 돌리고 앞으로 기울이고 튄다.
// - 길: A*(nav.ts), 걷기 150px/s · 소집 300px/s(칸 32px), 길을 못 찾으면 1초마다 다시.
// - 쉼 자리(`place: "bed"`)는 라운지 · 탕비실 의자다(💤, 누운 자세 없음).

import * as THREE from "three";
import { CROSSFADE_SECONDS, clipName, defaultLook, makeActor, type AgentId } from "./actor";
import { poseDoll, makeDoll, removeDoll, type Doll } from "./doll";
import { instance, loadLook } from "./gltf";
import { findPath, type Grid, type Point } from "./nav";
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
const HEAD_Y = 2.05;                   // 이름표 높이(서기)
const SIT_HEAD_Y = 1.6;
const GLYPH: Record<string, string> = { idle: "", queued: "⏳", thinking: "💭", failed: "❗" };

interface Anchor extends Point { facing: number }

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
  headY: number;
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
      target: null, path: null, speed: WALK_SPEED, retryAt: 0, walking: false, label: "", headY: HEAD_Y,
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
        return;
      }
      const model = instance(made.model.scene, true);
      const mixer = new THREE.AnimationMixer(model);
      const actions = new Map<string, Obj>();
      for (const clip of (made.model.animations ?? []) as Obj[]) { actions.set(String(clip.name).toLowerCase(), mixer.clipAction(clip)); }
      actor.root.add(model);
      if (actor.doll) { removeDoll(actor.doll); actor.doll = null; }
      actor.body = { look, model, mixer, actions, current: null };
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
    const a = map.agents[actor.id];
    if (actor.view.place === "desk") { return { x: a.desk.x, y: a.desk.y, facing: facingYaw(a.desk_facing) }; }
    if (actor.view.place === "meeting" && map.meeting_seats.length > 0) {
      const order = ids.filter((id) => actors.get(id)?.view.place === "meeting");
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
    actor.speed = view.place === "meeting" ? SUMMON_SPEED : WALK_SPEED;
    if (placeChanged || snap || !actor.target) {
      retarget(actor, snap);
      for (const other of actors.values()) {
        if (other !== actor && other.view.place === "meeting") { retarget(other, snap); }
      }
    }
  }

  function step(actor: Actor, dt: number, now: number): void {
    if (!actor.target) { return; }
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
    if (actor.path && actor.path.length === 0) { placeAt(actor, actor.target); }
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
    const clip = clipName(actor.walking, !actor.walking && seated(actor));
    if (actor.body) { animateBody(actor, actor.body, clip, dt); }
    if (actor.doll) { poseDoll(actor.doll, clip, actor.view.work, t + actor.phase); }
    actor.headY = clip === "sit" ? SIT_HEAD_Y : HEAD_Y;
    const work = actor.view.work;
    const glyph = actor.view.place === "bed" && work === "idle" ? "💤" : GLYPH[work] ?? "";
    const label = `${glyph ? glyph + " " : ""}${names[actor.id] ?? actor.id}`;
    if (label !== actor.label) {
      actor.tag.textContent = label;
      actor.label = label;
    }
    actor.tag.dataset.work = work;
    actor.tag.dataset.place = actor.view.place;
  }

  const projected = new THREE.Vector3();
  const raycaster = new THREE.Raycaster();
  const ndc = new THREE.Vector2();

  return {
    setAgent,
    setLook,
    update(dt: number, t: number, now: number): void {
      for (const actor of actors.values()) { step(actor, dt, now); animate(actor, dt, t); }
    },
    snapAll(): void {
      for (const actor of actors.values()) { if (actor.target) { placeAt(actor, actor.target); } }
    },
    placeLabels(camera: Obj, width: number, height: number): void {
      for (const actor of actors.values()) {
        projected.set(actor.x, actor.headY, actor.y).project(camera);
        const visible = projected.z < 1 && Math.abs(projected.x) <= 1.05 && Math.abs(projected.y) <= 1.05;
        if (!visible) { actor.tag.style.display = "none"; continue; }
        actor.tag.style.display = "";
        const sx = (projected.x + 1) / 2 * width;
        const sy = (1 - projected.y) / 2 * height;
        actor.tag.style.transform = `translate(${Math.round(sx)}px, ${Math.round(sy)}px) translate(-50%, -100%)`;
      }
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
