// 3D 사무실 장면(server 20차 · 결정 13). 원본 DeskRPG의 동작을 따르고 코드는 새로 썼다
// (docs/research/office-deep.md 1 · 2 · 9 · 11절, 원본 trading-scene · gltf-actor).
// - 배경은 원본 "종합상사"(trading v3) 배치와 원본 가구 자산, 캐릭터는 원본 외형 GLB(실패하면 코드 인형).
// - 격자 1칸 = 3D 1단위(1m), 칸 (col, row)의 중심은 (col + 0.5, 0, row + 0.5).
// - 원근 카메라 38도, 처음에는 사무 공간이 화면 가로를 채운다(camera.ts). 끌어 돌리기 · 바퀴/두 손가락 확대 ·
//   오른쪽 끌기/Shift 끌기/두 손가락 끌기로 옮기기(범위 제한). 5px 미만 누름만 고르기.
// - 해 하나만 그림자, 바닥은 인스턴스 메시, 탭이 숨으면 그리지 않음, 자동 해상도 낮춤.
// - 담당 자리: desk(업무 자리) · meeting(미팅룸 탁자) · bed(쉼 자리 — 라운지 · 탕비실). 이름표 · 방 이름표는 DOM.
// - (23차 7번 (나)) 발언자 카메라: 담당 셋 이상이 미팅룸에 모이면 speaker-camera.ts의 목표(발언자 샷 · 탁자 샷)를
//   ease-in-out으로 따라간다. 목표가 없으면 평소의 따라 비추기 카메라로 돌아간다. 손으로 움직이면 그 논의 동안 끈다.
//   (25차 1번, 결정 18) 발언자 샷은 돌지 않는다: 탁자 샷의 방위각 · 고도 그대로, 중심만 발언자 쪽으로 절반 · 거리
//   0.8배(speakerOrbit). 여섯 담당이 계속 화면 안에 있다.
// - (32차) 내 캐릭터(player-scene.ts): 카메라는 기본으로 내 캐릭터를 따라간다(follow.ts — 원본 0.08/프레임).
//   끌기(화면 이동) · 오른쪽 끌기(돌리기) · 확대 · 도구 줄의 돌리기 · 각도 · 전체 보기에 풀리고 "따라가기"나 바닥
//   누르기에 다시 켜진다. 따라가는 동안 발언자 카메라는 끼어들지 않는다. 원본처럼 왼쪽 끌기는 화면 이동, 오른쪽 ·
//   Shift 끌기는 돌리기다(docs/research/metaverse-office-refs.md B절).

import * as THREE from "three";
import { buildFloor, type Floor } from "./architecture";
import { clampDistance } from "./camera";
import { fitView, focusBounds, focusOf, type Focus } from "./framing";
import { placeFurniture, type Furniture } from "./furniture";
import { disposeLoaded } from "./gltf";
import { buildGrid, type NavMap, type Point } from "./nav";
import { createActors } from "./actors3d";
import { createBubbleLayer } from "./bubbles3d";
import { actorPriority, layoutOffice } from "./layout";
import { easeInOut, speakerOrbit, type CameraGoal, type SpeakerCamera } from "./speaker-camera";
import { FOLLOW_LERP, followStep, type CameraTouch, type FollowState } from "./follow";
import { createPlayerScene, type PlayerScene, type PlayerSceneOptions } from "./player-scene";
import type { ToolId } from "./toolbar";
import { PointerGesture } from "./gesture";
import type { SpeechBoard } from "./speech";

/** three의 값(형 정의 없이 묶는다 — three.d.ts). */
type Obj = any;

/** 자리(칸)와 바라보는 쪽. */
export interface Seat extends Point { facing?: string }

/** 지도 정의(office-map.json)에서 장면이 쓰는 부분. */
export interface OfficeMap extends NavMap, Floor {
  walls: Floor["walls"];
  furniture: Furniture[];
  rooms: { id: string; label: string; x: number; y: number; w: number; h: number }[];
  agents: Record<string, { name?: string; desk: Point; desk_facing?: string; rest: Point; rest_facing?: string }>;
  meeting_seats: Seat[];
}

/** 담당 하나의 자리 · 일. */
export interface AgentLook { place: string; work: string }

/** 만든 장면: 담당 모습 바꾸기 · 외형 바꾸기 · 치우기. */
export interface Scene {
  setAgent(id: string, view: AgentLook, options?: { snap?: boolean }): void;
  setLook(id: string, look: string): void;
  /** (32차 6번) 카메라 도구 줄의 단추. */
  tool(id: ToolId): void;
  /** (32차) 내 캐릭터의 외형을 바꾼다. */
  setPlayerLook(look: string): void;
  dispose(): void;
}

/** 장면을 만들 때 받는 것. */
export interface SceneOptions {
  host: HTMLElement;
  labels: HTMLElement;
  map: OfficeMap;
  names: Record<string, string>;
  looks: Record<string, string>;
  onPick?: (id: string) => void;
  onLost?: () => void;
  /** (32차 6번) 카메라 따라가기 상태(있으면 내 캐릭터를 따라간다). */
  follow?: FollowState;
  /** (32차) 내 캐릭터(있으면 그린다): 사무실 층 · 서버에서 읽은 나 · 대화 여닫기. */
  player?: Pick<PlayerSceneOptions, "stage" | "me" | "hooks">;
  /** (22차 3번) 머리 위 말풍선 판(있으면 말풍선 층을 그린다 — 지나가며 잡담 포함). */
  speech?: SpeechBoard;
  /** (23차 7번 (나)) 회의 발언자 카메라(있으면 미팅룸에 모였을 때 목표를 따라간다). */
  speaker?: SpeakerCamera;
}

const FOV = 38;
const MIN_POLAR = 0.15;
const MAX_POLAR = 0.46 * Math.PI;
// 내려다보는 각(연직에서 약 54도): 쓰는 공간의 앞뒤(미팅룸 ~ 라운지)를 담으면서 캐릭터 키가 잘 보이게.
const START_POLAR = 0.95;
const MAX_PIXEL_RATIO = 1.75;
const MIN_PIXEL_RATIO = 0.75;
// 담당이 이만큼 이상 미팅룸에 있으면 "모였다"(framing.ts 따라 비추기와 같은 수).
const GATHERED = 3;
// (32차 6번) 도구 줄: 돌리기 ±45도 · 확대 ×0.8 / 축소 ×1.25(원본 수치), 위에서 보기의 각.
const ROTATE_STEP = Math.PI / 4;
const ZOOM_IN_STEP = 0.8;
const ZOOM_OUT_STEP = 1.25;
// 내 캐릭터를 따라갈 때의 처음 거리: 전체 보기 거리의 이만큼(사람 키가 잘 보이게).
const FOLLOW_DISTANCE = 0.5;

/** 카메라 궤도 값: 보는 점(tx, ty, tz)에서 거리 · 연직에서 잰 각 · 둘레 각. */
interface Orbit { radius: number; polar: number; theta: number; tx: number; ty: number; tz: number }

function angleTo(from: number, to: number): number {
  let d = to - from;
  while (d > Math.PI) { d -= Math.PI * 2; }
  while (d < -Math.PI) { d += Math.PI * 2; }
  return from + d;
}

/** WebGL을 쓸 수 있는가. */
export function webglAvailable(): boolean {
  try {
    const c = document.createElement("canvas");
    return Boolean(c.getContext("webgl2") || c.getContext("webgl"));
  } catch {
    return false;
  }
}

function makeRenderer(pixelRatio: number): Obj {
  const renderer = new THREE.WebGLRenderer({ antialias: true, powerPreference: "high-performance" });
  renderer.setPixelRatio(pixelRatio);
  renderer.shadowMap.enabled = true;
  // npm three(0.187)에는 PCFSoftShadowMap이 없어져 그 자리에서 PCFShadowMap으로 대신한다.
  renderer.shadowMap.type = THREE.PCFShadowMap;
  renderer.outputColorSpace = THREE.SRGBColorSpace;
  renderer.toneMapping = THREE.ACESFilmicToneMapping;
  renderer.toneMappingExposure = 1.08;
  renderer.debug.checkShaderErrors = false;
  return renderer;
}

function addLights(scene: Obj, map: OfficeMap): void {
  const cx = map.cols / 2;
  const cz = map.rows / 2;
  const sun = new THREE.DirectionalLight(0xffffff, 2.4);
  sun.position.set(cx - 14, 30, cz - 20);
  sun.target.position.set(cx, 0, cz);
  sun.castShadow = true;
  sun.shadow.mapSize.set(2048, 2048);
  const half = Math.hypot(map.cols, map.rows) / 2 + 2;
  Object.assign(sun.shadow.camera, { left: -half, right: half, top: half, bottom: -half, near: 0.5, far: 140 });
  sun.shadow.bias = -0.0005;
  scene.add(sun, sun.target);
  const fill = new THREE.DirectionalLight(0xffffff, 0.7);
  fill.position.set(cx + 12, 16, cz + 18);
  scene.add(fill);
  scene.add(new THREE.HemisphereLight(0xfffaf0, 0x8a8170, 1.5));
}

/** 방 이름표(DOM). */
function roomTags(labels: HTMLElement, map: OfficeMap): { el: HTMLElement; x: number; z: number }[] {
  return map.rooms.map((room) => {
    const el = document.createElement("div");
    el.className = "room-tag";
    el.textContent = room.label;
    labels.appendChild(el);
    return { el, x: room.x + room.w / 2, z: room.y + 0.6 };
  });
}

export function createScene({ host, labels, map, names, looks, onPick, onLost, follow, player: playerOptions, speech, speaker }: SceneOptions): Scene {
  let pixelRatio = Math.min(window.devicePixelRatio || 1, MAX_PIXEL_RATIO);
  const renderer = makeRenderer(pixelRatio);
  host.appendChild(renderer.domElement);
  const canvas = renderer.domElement;
  const scene = new THREE.Scene();
  scene.background = new THREE.Color(0xe9e4da);
  const camera = new THREE.PerspectiveCamera(FOV, 1, 0.1, 250);
  addLights(scene, map);
  scene.add(buildFloor(map));
  const furnitureRoot = new THREE.Group();
  placeFurniture(furnitureRoot, map.furniture);
  scene.add(furnitureRoot);
  const rooms = roomTags(labels, map);
  const actors = createActors({ scene, labels, map, names, looks, grid: buildGrid(map) });
  const bubbles = speech ? createBubbleLayer(labels, speech, names) : null;
  const player: PlayerScene | null = playerOptions
    ? createPlayerScene({
      ...playerOptions, scene, labels, map, names,
      agents: () => actors.walkers().map((w) => ({ id: w.id, x: w.x, z: w.z })),
    })
    : null;
  /** 내 캐릭터를 따라가는 중인가(내 캐릭터가 있고 따라가기가 켜져 있음). */
  const following = () => Boolean(player && follow?.on());

  // 카메라: 원하는 값으로 부드럽게 따라간다(감쇠). 처음에는 쓰는 자리 모두(framing.ts), 다수가 미팅룸 · 쉼 자리로
  // 옮기면 그쪽을 비춘다. 사용자가 끌기 · 확대를 하면 따라가지 않는다("전체 보기"로 다시 켠다).
  let focus: Focus = "all";
  const want: Orbit = { radius: 30, polar: START_POLAR, theta: 0, tx: 0, ty: 0, tz: 0 };
  const cur = { ...want };
  let framed = 30;
  let userMoved = false;
  const target = new THREE.Vector3();
  function aspect(): number { return (host.clientWidth || 1) / (host.clientHeight || 1); }
  function reframe(): void {
    framed = fitView(focusBounds(map, "all"), START_POLAR, aspect(), FOV).distance;
    const v = focus === "all" ? null : fitView(focusBounds(map, focus), START_POLAR, aspect(), FOV, true);
    const view = v ?? fitView(focusBounds(map, "all"), START_POLAR, aspect(), FOV);
    Object.assign(want, { radius: view.distance, polar: START_POLAR, theta: 0, tx: view.tx, ty: 0, tz: view.tz });
  }
  /** 손으로 움직였는지(자동 비추기를 멈춘다). */
  function setFollowing(on: boolean): void {
    userMoved = !on;
  }
  /** 내 캐릭터를 따라갈 때 원하는 값: 보는 점은 캐릭터, 거리는 처음엔 전체 보기의 절반. */
  function followView(snap: boolean): void {
    if (!player) { return; }
    const p = player.pos();
    if (snap) { Object.assign(want, { radius: framed * FOLLOW_DISTANCE, polar: START_POLAR, theta: 0 }); }
    want.tx = p.x;
    want.tz = p.y;
  }
  /** 사용자가 카메라를 손으로 움직였다: 따라가기를 풀고 발언자 카메라는 그 논의 동안 끈다. */
  function touched(kind: CameraTouch): void {
    tween = null;
    speaker?.manual(Date.now());
    follow?.touch(kind);
    setFollowing(false);
    clampWant();
  }
  function clampWant(): void {
    want.radius = clampDistance(want.radius, framed);
    want.polar = THREE.MathUtils.clamp(want.polar, MIN_POLAR, MAX_POLAR);
    want.tx = THREE.MathUtils.clamp(want.tx, 0, map.cols);
    want.tz = THREE.MathUtils.clamp(want.tz, 0, map.rows);
  }
  // 발언자 카메라가 카메라를 쥐고 있는 동안의 전환(시작 값 · 시작 시각 · 길이). 쥐는 동안 want는 cur를 따라간다.
  let tween: { key: string; from: Orbit; start: number; ms: number; to: Orbit | null } | null = null;
  let gathered = false;
  function goalOrbit(goal: CameraGoal): Orbit | null {
    const polar = (90 - goal.elevationDeg) * Math.PI / 180;
    // 탁자 샷: 미팅룸 전체. 몸이 없는 발언자(사무실 담당이 아닌 id)도 카메라를 놓지 않고 탁자를 본다.
    const v = fitView(focusBounds(map, "meeting"), polar, aspect(), FOV, true);
    const table: Orbit = { radius: v.distance, polar, theta: 0, tx: v.tx, ty: 0, tz: v.tz };
    const pose = goal.shot === "speaker" && goal.agent ? actors.pose(goal.agent) : null;
    // 발언자 샷: 탁자 샷에서 돌지 않고 발언자 쪽으로 조금만 다가간다(결정 18).
    return pose ? speakerOrbit(table, { x: pose.x, z: pose.z }) : table;
  }
  /**
   * 미팅룸에 모였는지를 발언자 카메라에 알린다. 프레임마다만 알리면 소집(`office.agent`)과 첫 논의 글(`office.talk`)이
   * 같은 틈에 오거나 탭이 숨어 있을 때 첫 글을 "아직 안 모임"으로 버려, 발언자 샷 없이 미팅룸 전체 보기(거리 약 18 —
   * 상반신이 화면 높이의 8%쯤)에 머문다. 그래서 담당 자리가 바뀌는 그 자리에서도 바로 알린다.
   */
  function syncGather(wall: number): void {
    if (!speaker) { return; }
    const meeting = actors.places().filter((p) => p === "meeting").length >= GATHERED;
    if (meeting !== gathered) { gathered = meeting; speaker.gather(meeting, wall); }
  }
  /** 이번 프레임 발언자 카메라를 따른다. 쥐고 있으면 참. */
  function driveSpeaker(wall: number): boolean {
    if (!speaker) { return false; }
    syncGather(wall);
    const goal = speaker.goal(wall);
    if (!goal) {
      if (tween) {
        tween = null;
        Object.assign(want, cur);
        if (!userMoved) { reframe(); }
      }
      return false;
    }
    const key = `${goal.shot}:${goal.agent ?? ""}:${goal.since}`;
    if (!tween || tween.key !== key) {
      tween = { key, from: { ...cur }, start: wall, ms: goal.transitionMs, to: goal.shot === "table" ? goalOrbit(goal) : null };
    }
    const to = tween.to ?? goalOrbit(goal);
    if (!to) { return false; }
    const k = easeInOut((wall - tween.start) / tween.ms);
    const theta = angleTo(tween.from.theta, to.theta);
    for (const name of Object.keys(cur) as (keyof Orbit)[]) {
      const end = name === "theta" ? theta : to[name];
      cur[name] = tween.from[name] + (end - tween.from[name]) * k;
    }
    Object.assign(want, cur);
    return true;
  }
  function updateCamera(dt: number, wall: number): void {
    if (following() && player) {
      // 원본: 매 프레임 (캐릭터 - 보는 점) × 0.08. 60fps 기준 같은 몫이 되게 프레임 시간으로 맞춘다.
      const p = player.pos();
      const k = 1 - Math.pow(1 - FOLLOW_LERP, Math.min(dt, 0.2) * 60);
      const next = followStep({ x: cur.tx, z: cur.tz }, { x: p.x, z: p.y }, k);
      cur.tx = want.tx = next.x;
      cur.tz = want.tz = next.z;
      tween = null;
    }
    if (following() || !driveSpeaker(wall)) {
      const a = 1 - Math.exp(-12 * dt);
      for (const key of Object.keys(want) as (keyof Orbit)[]) { cur[key] += (want[key] - cur[key]) * a; }
    }
    target.set(cur.tx, cur.ty, cur.tz);
    const s = Math.sin(cur.polar);
    camera.position.set(target.x + cur.radius * s * Math.sin(cur.theta), target.y + cur.radius * Math.cos(cur.polar),
      target.z + cur.radius * s * Math.cos(cur.theta));
    camera.lookAt(target);
  }
  function resize(): void {
    const w = host.clientWidth || 1;
    const h = host.clientHeight || 1;
    renderer.setSize(w, h, false);
    camera.aspect = w / h;
    camera.updateProjectionMatrix();
    if (!userMoved && !following()) { reframe(); } else { framed = fitView(focusBounds(map, "all"), START_POLAR, w / h, FOV).distance; }
  }
  const observer = new ResizeObserver(resize);
  observer.observe(host);
  resize();
  if (following()) { followView(true); }
  Object.assign(cur, want);
  // 따라가기가 다시 켜지면 그 자리에서 캐릭터 쪽으로 다가간다. 꺼지면(전체 보기 등) 지금 보이는 자리에 둔다.
  const stopFollow = follow?.onChange((on) => { if (on) { setFollowing(true); followView(false); } });

  const controls = wireControls(canvas, host, {
    pan(dx: number, dy: number) {
      const scale = cur.radius / (host.clientHeight || 1) * 0.75;
      const sinT = Math.sin(cur.theta);
      const cosT = Math.cos(cur.theta);
      want.tx += (-dx * cosT - dy * sinT) * scale;
      want.tz += (dx * sinT - dy * cosT) * scale;
    },
    rotate(dx: number, dy: number) { want.theta -= dx * 0.006; want.polar -= dy * 0.005; },
    zoom(factor: number) { want.radius *= factor; },
    twist(angle: number) { want.theta -= angle; },
    moved(kind: CameraTouch) {
      // 손으로 움직이면 따라가기가 풀리고 발언자 카메라는 그 논의 동안 꺼지며, 지금 보이는 자리에서 사용자 조작을 잇는다.
      touched(kind);
    },
    pick(x: number, y: number) {
      const id = actors.pick(x, y, camera, canvas);
      if (id) {
        if (player) { player.tapAgent(id); } else { onPick?.(id); }
        return;
      }
      const cell = player ? floorCell(x, y) : null;
      // 원본: 바닥을 누르면 그리로 걷고 카메라가 다시 따라간다.
      if (cell && player?.tapFloor(cell)) { follow?.resume(); }
    },
  });

  const onContextLost = (e: Event) => { e.preventDefault(); onLost?.(); };
  canvas.addEventListener("webglcontextlost", onContextLost);
  const onVisibility = () => { if (!document.hidden) { actors.snapAll(); last = performance.now(); } };
  document.addEventListener("visibilitychange", onVisibility);

  // 자동 해상도 낮춤: 3초 창마다 fps · p95를 재고, fps < 55 또는 p95 > 25ms가 두 창 연속이면 0.25 내린다.
  let windowStart = performance.now();
  let frameTimes: number[] = [];
  let badWindows = 0;
  function measure(now: number, frameMs: number): void {
    frameTimes.push(frameMs);
    if (now - windowStart < 3000) { return; }
    const fps = frameTimes.length / ((now - windowStart) / 1000);
    const sorted = frameTimes.slice().sort((a, b) => a - b);
    const p95 = sorted[Math.min(sorted.length - 1, Math.floor(sorted.length * 0.95))] ?? 0;
    badWindows = fps < 55 || p95 > 25 ? badWindows + 1 : 0;
    if (badWindows >= 2 && pixelRatio > MIN_PIXEL_RATIO) {
      pixelRatio = Math.max(MIN_PIXEL_RATIO, pixelRatio - 0.25);
      renderer.setPixelRatio(pixelRatio);
      resize();
      badWindows = 0;
    }
    windowStart = now;
    frameTimes = [];
  }

  const projected = new THREE.Vector3();
  function placeRoomTags(width: number, height: number): void {
    for (const tag of rooms) {
      projected.set(tag.x, 0.1, tag.z).project(camera);
      const visible = projected.z < 1 && Math.abs(projected.x) <= 1.05 && Math.abs(projected.y) <= 1.05;
      tag.el.style.display = visible ? "" : "none";
      if (visible) {
        tag.el.style.transform = `translate(${Math.round((projected.x + 1) / 2 * width)}px, ${Math.round((1 - projected.y) / 2 * height)}px) translate(-50%, -50%)`;
      }
    }
  }

  let raf = 0;
  let last = performance.now();
  const clock0 = performance.now();
  function frameStep(now: number): void {
    raf = requestAnimationFrame(frameStep);
    if (document.hidden) { last = now; return; }
    const frameMs = now - last;
    const dt = Math.min(frameMs / 1000, 0.2);
    last = now;
    // 말풍선 판과 잡담은 벽시계 밀리초를 쓴다(서버 이벤트를 받은 때와 같은 시계).
    const wall = Date.now();
    if (bubbles) { actors.pause((id) => bubbles.paused(id, wall)); }
    actors.update(dt, (now - clock0) / 1000, now);
    player?.step(dt, (now - clock0) / 1000, wall);
    updateCamera(dt, wall);
    renderer.render(scene, camera);
    const width = host.clientWidth;
    const height = host.clientHeight;
    placeRoomTags(width, height);
    player?.placeTag(camera, width, height);
    // (23차) 이름표 · 말풍선 · 말풍선 줄을 layout.ts로 한 번에 놓는다.
    const spots = actors.spots(camera, width, height);
    if (bubbles) {
      bubbles.update(wall, spots, actors.walkers(), actors.canSee, width, height, actors.placeTag);
    } else {
      const layout = layoutOffice(spots.map((s) => ({
        agent: s.id, visible: s.visible, foot: s.foot, head: s.head, tag: s.tag, bubble: null, railed: false,
        priority: actorPriority(false, s.work),
      })), { width, height });
      const tags = new Map(layout.tags.map((t) => [t.agent, t.rect]));
      for (const s of spots) { actors.placeTag(s.id, tags.get(s.id) ?? null); }
    }
    measure(now, frameMs);
  }
  raf = requestAnimationFrame(frameStep);

  function dispose(): void {
    cancelAnimationFrame(raf);
    observer.disconnect();
    controls.dispose();
    stopFollow?.();
    player?.dispose();
    document.removeEventListener("visibilitychange", onVisibility);
    canvas.removeEventListener("webglcontextlost", onContextLost);
    actors.dispose();
    bubbles?.dispose();
    scene.traverse((o: Obj) => {
      if (o.geometry) { o.geometry.dispose(); }
      const mats = Array.isArray(o.material) ? o.material : o.material ? [o.material] : [];
      for (const m of mats) { m.map?.dispose?.(); m.dispose(); }
    });
    disposeLoaded();
    renderer.dispose();
    renderer.forceContextLoss();
    canvas.remove();
    for (const tag of rooms) { tag.el.remove(); }
  }

  /** 담당 자리가 바뀌면(걸어서 옮길 때) 비출 곳을 다시 정한다. 접속 스냅샷(snap)은 처음 화면 그대로. */
  function setAgent(id: string, view: AgentLook, options?: { snap?: boolean }): void {
    const before = actors.places().join();
    actors.setAgent(id, view, options);
    syncGather(Date.now());
    if (options?.snap || actors.places().join() === before) { return; }
    const next = focusOf(actors.places());
    if (next === focus) { return; }
    focus = next;
    if (!userMoved && !following()) { reframe(); }
  }
  /** 화면 점 → 바닥(y 0)의 칸. 바닥에 닿지 않거나 지도 밖이면 `null`. */
  const floorRay = new THREE.Raycaster();
  const floorNdc = new THREE.Vector2();
  const floorPlane = new THREE.Plane(new THREE.Vector3(0, 1, 0), 0);
  const floorHit = new THREE.Vector3();
  function floorCell(clientX: number, clientY: number): Point | null {
    const rect = canvas.getBoundingClientRect();
    floorNdc.set(((clientX - rect.left) / rect.width) * 2 - 1, -((clientY - rect.top) / rect.height) * 2 + 1);
    floorRay.setFromCamera(floorNdc, camera);
    if (!floorRay.ray.intersectPlane(floorPlane, floorHit)) { return null; }
    const cell = { x: Math.floor(floorHit.x), y: Math.floor(floorHit.z) };
    return cell.x >= 0 && cell.y >= 0 && cell.x < map.cols && cell.y < map.rows ? cell : null;
  }
  /** (32차 6번) 도구 줄: 전체 보기 · 돌리기 · 각도 · 따라가기 · 확대 · 축소. */
  function tool(id: ToolId): void {
    switch (id) {
      case "overview":
        follow?.touch("overview");
        tween = null;
        setFollowing(true);
        focus = "all";
        reframe();
        return;
      case "rotate-left": want.theta += ROTATE_STEP; touched("rotate"); return;
      case "rotate-right": want.theta -= ROTATE_STEP; touched("rotate"); return;
      case "perspective": want.polar = START_POLAR; touched("angle"); return;
      case "top": want.polar = MIN_POLAR; touched("angle"); return;
      case "zoom-in": want.radius *= ZOOM_IN_STEP; touched("zoom"); return;
      case "zoom-out": want.radius *= ZOOM_OUT_STEP; touched("zoom"); return;
      case "follow": follow?.resume(); return;
    }
  }

  return {
    setAgent, setLook: actors.setLook, tool, dispose,
    setPlayerLook: (look: string) => player?.setLook(look),
  };
}

/** 누르기 · 끌기 · 바퀴 · 두 손가락. */
interface ControlHandlers {
  pan(dx: number, dy: number): void;
  rotate(dx: number, dy: number): void;
  zoom(factor: number): void;
  twist(angle: number): void;
  moved(kind: CameraTouch): void;
  pick(clientX: number, clientY: number): void;
}

function wireControls(canvas: HTMLElement, host: HTMLElement, h: ControlHandlers): { dispose(): void } {
  const pointers = new Map<number, Point>();
  // 누르기 · 끌기는 gesture.ts(원본 5px 문턱)로 가른다.
  let press: { turn: boolean } | null = null;
  const gesture = new PointerGesture();
  let pinch: { dist: number; angle: number; mx: number; my: number } | null = null;
  const pair = () => {
    const [p1, p2] = [...pointers.values()];
    return { dist: Math.hypot(p1.x - p2.x, p1.y - p2.y), angle: Math.atan2(p2.y - p1.y, p2.x - p1.x), mx: (p1.x + p2.x) / 2, my: (p1.y + p2.y) / 2 };
  };
  function onDown(e: PointerEvent): void {
    try { canvas.setPointerCapture?.(e.pointerId); } catch { /* 이미 끝난 누름 */ }
    pointers.set(e.pointerId, { x: e.clientX, y: e.clientY });
    if (pointers.size === 1) {
      press = { turn: e.button === 2 || e.shiftKey };
      gesture.down(e.clientX, e.clientY);
    } else {
      press = null;
      pinch = pair();
    }
  }
  function onMove(e: PointerEvent): void {
    const prev = pointers.get(e.pointerId);
    if (!prev) { return; }
    const dx = e.clientX - prev.x;
    const dy = e.clientY - prev.y;
    prev.x = e.clientX;
    prev.y = e.clientY;
    if (pointers.size >= 2) {
      const now = pair();
      if (pinch && now.dist > 0) {
        h.zoom(pinch.dist / now.dist);
        h.twist(now.angle - pinch.angle);
        h.pan(now.mx - pinch.mx, now.my - pinch.my);
      }
      pinch = now;
      h.moved("zoom");
      return;
    }
    if (!press) { return; }
    gesture.move(e.clientX, e.clientY);
    if (!gesture.dragging()) { return; }
    // 원본: 왼쪽 끌기 = 화면 이동, 오른쪽 · Shift 끌기 = 돌리기.
    if (press.turn) { h.rotate(dx, dy); } else { h.pan(dx, dy); }
    h.moved(press.turn ? "rotate" : "drag");
  }
  function onUp(e: PointerEvent): void {
    pointers.delete(e.pointerId);
    if (pointers.size < 2) { pinch = null; }
    if (press && pointers.size === 0) {
      const kind = gesture.up(e.clientX, e.clientY);
      if (kind === "tap" && !press.turn && e.button === 0) { h.pick(e.clientX, e.clientY); }
    }
    if (pointers.size === 0) { press = null; }
  }
  function onWheel(e: WheelEvent): void {
    e.preventDefault();
    h.zoom(Math.exp(e.deltaY * 0.001));
    h.moved("zoom");
  }
  const onContext = (e: Event) => e.preventDefault();
  canvas.addEventListener("pointerdown", onDown);
  canvas.addEventListener("pointermove", onMove);
  canvas.addEventListener("pointerup", onUp);
  canvas.addEventListener("pointercancel", onUp);
  canvas.addEventListener("wheel", onWheel, { passive: false });
  canvas.addEventListener("contextmenu", onContext);
  host.style.touchAction = "none";
  return {
    dispose() {
      canvas.removeEventListener("pointerdown", onDown);
      canvas.removeEventListener("pointermove", onMove);
      canvas.removeEventListener("pointerup", onUp);
      canvas.removeEventListener("pointercancel", onUp);
      canvas.removeEventListener("wheel", onWheel);
      canvas.removeEventListener("contextmenu", onContext);
    },
  };
}
