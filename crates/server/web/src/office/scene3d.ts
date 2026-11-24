// 3D 사무실 장면(server 20차 · 결정 13). 원본 DeskRPG의 동작을 따르고 코드는 새로 썼다
// (docs/research/office-deep.md 1 · 2 · 9 · 11절, 원본 trading-scene · gltf-actor).
// - 배경은 원본 "종합상사"(trading v3) 배치와 원본 가구 자산, 캐릭터는 원본 외형 GLB(실패하면 코드 인형).
// - 격자 1칸 = 3D 1단위(1m), 칸 (col, row)의 중심은 (col + 0.5, 0, row + 0.5).
// - 원근 카메라 38도, 처음에는 사무 공간이 화면 가로를 채운다(camera.ts). 끌어 돌리기 · 바퀴/두 손가락 확대 ·
//   오른쪽 끌기/Shift 끌기/두 손가락 끌기로 옮기기(범위 제한). 5px 미만 누름만 고르기.
// - 해 하나만 그림자, 바닥은 인스턴스 메시, 탭이 숨으면 그리지 않음, 자동 해상도 낮춤.
// - 담당 자리: desk(업무 자리) · meeting(미팅룸 탁자) · bed(쉼 자리 — 라운지 · 탕비실, 💤). 이름표 · 방 이름표는 DOM.

import * as THREE from "three";
import { buildFloor, type Floor } from "./architecture";
import { clampDistance, frameCamera, type Bounds } from "./camera";
import { placeFurniture, type Furniture } from "./furniture";
import { disposeLoaded } from "./gltf";
import { buildGrid, type NavMap, type Point } from "./nav";
import { createActors } from "./actors3d";

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
  frame?: Bounds;
}

/** 담당 하나의 자리 · 일. */
export interface AgentLook { place: string; work: string }

/** 만든 장면: 담당 모습 바꾸기 · 외형 바꾸기 · 치우기. */
export interface Scene {
  setAgent(id: string, view: AgentLook, options?: { snap?: boolean }): void;
  setLook(id: string, look: string): void;
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
}

const FOV = 38;
const MIN_POLAR = 0.15;
const MAX_POLAR = 0.46 * Math.PI;
const START_POLAR = 0.72;
const PRESS_SLOP_PX = 5;
const MAX_PIXEL_RATIO = 1.75;
const MIN_PIXEL_RATIO = 0.75;

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

export function createScene({ host, labels, map, names, looks, onPick, onLost }: SceneOptions): Scene {
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

  // 카메라: 원하는 값으로 부드럽게 따라간다(감쇠). 처음 잡기는 사무 공간(map.frame)이 가로를 채운다.
  const frame: Bounds = map.frame ?? { minX: 0, maxX: map.cols, minZ: 0, maxZ: map.rows };
  const want = { radius: 30, polar: START_POLAR, theta: 0, tx: 0, tz: 0 };
  const cur = { ...want };
  let framed = 30;
  let userMoved = false;
  const target = new THREE.Vector3();
  function reframe(): void {
    const f = frameCamera(frame, { width: host.clientWidth || 1, height: host.clientHeight || 1 }, FOV);
    framed = f.distance;
    Object.assign(want, { radius: f.distance, polar: START_POLAR, theta: 0, tx: f.target.x, tz: f.target.z });
  }
  function clampWant(): void {
    want.radius = clampDistance(want.radius, framed);
    want.polar = THREE.MathUtils.clamp(want.polar, MIN_POLAR, MAX_POLAR);
    want.tx = THREE.MathUtils.clamp(want.tx, 0, map.cols);
    want.tz = THREE.MathUtils.clamp(want.tz, 0, map.rows);
  }
  function updateCamera(dt: number): void {
    const a = 1 - Math.exp(-12 * dt);
    for (const key of Object.keys(want) as (keyof typeof want)[]) { cur[key] += (want[key] - cur[key]) * a; }
    target.set(cur.tx, 0, cur.tz);
    const s = Math.sin(cur.polar);
    camera.position.set(target.x + cur.radius * s * Math.sin(cur.theta), cur.radius * Math.cos(cur.polar),
      target.z + cur.radius * s * Math.cos(cur.theta));
    camera.lookAt(target);
  }
  function resize(): void {
    const w = host.clientWidth || 1;
    const h = host.clientHeight || 1;
    renderer.setSize(w, h, false);
    camera.aspect = w / h;
    camera.updateProjectionMatrix();
    if (!userMoved) { reframe(); } else { framed = frameCamera(frame, { width: w, height: h }, FOV).distance; }
  }
  const observer = new ResizeObserver(resize);
  observer.observe(host);
  resize();
  Object.assign(cur, want);

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
    moved() { userMoved = true; clampWant(); },
    pick(x: number, y: number) { const id = actors.pick(x, y, camera, canvas); if (id) { onPick?.(id); } },
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
    actors.update(dt, (now - clock0) / 1000, now);
    updateCamera(dt);
    renderer.render(scene, camera);
    actors.placeLabels(camera, host.clientWidth, host.clientHeight);
    placeRoomTags(host.clientWidth, host.clientHeight);
    measure(now, frameMs);
  }
  raf = requestAnimationFrame(frameStep);

  function dispose(): void {
    cancelAnimationFrame(raf);
    observer.disconnect();
    controls.dispose();
    document.removeEventListener("visibilitychange", onVisibility);
    canvas.removeEventListener("webglcontextlost", onContextLost);
    actors.dispose();
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

  return { setAgent: actors.setAgent, setLook: actors.setLook, dispose };
}

/** 누르기 · 끌기 · 바퀴 · 두 손가락. */
interface ControlHandlers {
  pan(dx: number, dy: number): void;
  rotate(dx: number, dy: number): void;
  zoom(factor: number): void;
  twist(angle: number): void;
  moved(): void;
  pick(clientX: number, clientY: number): void;
}

function wireControls(canvas: HTMLElement, host: HTMLElement, h: ControlHandlers): { dispose(): void } {
  const pointers = new Map<number, Point>();
  let press: { x: number; y: number; pan: boolean; dragging: boolean } | null = null;
  let pinch: { dist: number; angle: number; mx: number; my: number } | null = null;
  const pair = () => {
    const [p1, p2] = [...pointers.values()];
    return { dist: Math.hypot(p1.x - p2.x, p1.y - p2.y), angle: Math.atan2(p2.y - p1.y, p2.x - p1.x), mx: (p1.x + p2.x) / 2, my: (p1.y + p2.y) / 2 };
  };
  function onDown(e: PointerEvent): void {
    try { canvas.setPointerCapture?.(e.pointerId); } catch { /* 이미 끝난 누름 */ }
    pointers.set(e.pointerId, { x: e.clientX, y: e.clientY });
    if (pointers.size === 1) {
      press = { x: e.clientX, y: e.clientY, pan: e.button === 2 || e.shiftKey, dragging: false };
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
      h.moved();
      return;
    }
    if (!press) { return; }
    if (!press.dragging && Math.hypot(e.clientX - press.x, e.clientY - press.y) < PRESS_SLOP_PX) { return; }
    press.dragging = true;
    if (press.pan) { h.pan(dx, dy); } else { h.rotate(dx, dy); }
    h.moved();
  }
  function onUp(e: PointerEvent): void {
    pointers.delete(e.pointerId);
    if (pointers.size < 2) { pinch = null; }
    if (press && !press.dragging && pointers.size === 0 && !press.pan && e.button === 0) { h.pick(e.clientX, e.clientY); }
    if (pointers.size === 0) { press = null; }
  }
  function onWheel(e: WheelEvent): void {
    e.preventDefault();
    h.zoom(Math.exp(e.deltaY * 0.001));
    h.moved();
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
