// 3D 사무실 장면. 원본 DeskRPG의 동작을 따르되 코드와 자산은 새로 만들었다
// (docs/research/office-deep.md 1 · 2 · 9 · 11절).
// - 격자 1칸(32px) = 3D 1단위, 칸 (col, row)의 중심은 (col + 0.5, 0, row + 0.5).
// - 원근 카메라 38도, 궤도 조작(거리 8~85, 내려다보는 각 0.15rad~0.46π), 5px 미만 누름만 고르기.
// - 해 하나만 그림자, 바닥 · 벽은 인스턴스 메시, 탭이 숨으면 그리지 않음, 자동 해상도 낮춤.
// - 캐릭터는 절차적 모델, 이름표는 DOM. 앉음 · 누움은 위치에서 유도한다(앵커 0.22칸 안).

import * as THREE from "./vendor/three.module.min.js";
import { buildGrid, findPath } from "./nav.js";

const TILE_PX = 32;
const WALK_SPEED = 150 / TILE_PX;      // 칸/초 (원본 walk 150px/s)
const SUMMON_SPEED = 300 / TILE_PX;    // 논의 소집(원본 meetingSummon 300px/s)
const ARRIVE = 2 / TILE_PX;            // 경유점 도착 판정 2px
const POSE_RADIUS = 0.22;              // 앵커에서 이만큼 안이면 앉음 · 누움
const RETRY_MS = 1000;                 // 길을 못 찾으면 1초마다 다시
const FOV = 38;
const MIN_DIST = 8;
const MAX_DIST = 85;
const MIN_POLAR = 0.15;
const MAX_POLAR = 0.46 * Math.PI;
const PRESS_SLOP_PX = 5;
const MAX_PIXEL_RATIO = 1.75;
const MIN_PIXEL_RATIO = 0.75;

const COLORS = {
  news: 0xd9822b, strategy: 0x3f7cc4, risk: 0xc0453a,
  review: 0x7b5bb5, trader: 0x2f9a6b, leader: 0x37474f,
};
const GLYPH = { idle: "", queued: "⏳", thinking: "💭", failed: "❗" };

/** WebGL을 쓸 수 있는가. */
export function webglAvailable() {
  try {
    const c = document.createElement("canvas");
    return Boolean(c.getContext("webgl2") || c.getContext("webgl"));
  } catch (error) {
    return false;
  }
}

function makeCharacter(color) {
  const root = new THREE.Group();
  const body = new THREE.Group();
  root.add(body);
  const cloth = new THREE.MeshStandardMaterial({ color, roughness: 0.7 });
  const skin = new THREE.MeshStandardMaterial({ color: 0xf1c9a5, roughness: 0.8 });
  const dark = new THREE.MeshStandardMaterial({ color: 0x2d2a26, roughness: 0.9 });

  const torso = new THREE.Mesh(new THREE.CapsuleGeometry(0.17, 0.3, 4, 10), cloth);
  torso.position.y = 0.72;
  body.add(torso);

  const head = new THREE.Group();
  head.position.y = 1.08;
  const skull = new THREE.Mesh(new THREE.SphereGeometry(0.15, 16, 12), skin);
  const hair = new THREE.Mesh(new THREE.SphereGeometry(0.155, 16, 8, 0, Math.PI * 2, 0, Math.PI / 2), dark);
  hair.position.y = 0.02;
  head.add(skull, hair);
  body.add(head);

  const limb = (w, h, mat, x, y) => {
    const pivot = new THREE.Group();
    pivot.position.set(x, y, 0);
    const mesh = new THREE.Mesh(new THREE.BoxGeometry(w, h, w), mat);
    mesh.position.y = -h / 2;
    pivot.add(mesh);
    body.add(pivot);
    return pivot;
  };
  const armL = limb(0.08, 0.36, cloth, -0.23, 0.9);
  const armR = limb(0.08, 0.36, cloth, 0.23, 0.9);
  const legL = limb(0.1, 0.44, dark, -0.09, 0.46);
  const legR = limb(0.1, 0.44, dark, 0.09, 0.46);

  root.traverse((o) => { if (o.isMesh) { o.castShadow = true; } });
  return { root, body, head, armL, armR, legL, legR };
}

export function createScene({ host, labels, map, names, onPick, onLost }) {
  const grid = buildGrid(map);
  const renderer = new THREE.WebGLRenderer({ antialias: true, powerPreference: "high-performance" });
  let pixelRatio = Math.min(window.devicePixelRatio || 1, MAX_PIXEL_RATIO);
  renderer.setPixelRatio(pixelRatio);
  renderer.shadowMap.enabled = true;
  renderer.shadowMap.type = THREE.PCFSoftShadowMap;
  renderer.outputColorSpace = THREE.SRGBColorSpace;
  renderer.toneMapping = THREE.ACESFilmicToneMapping;
  renderer.toneMappingExposure = 1.08;
  renderer.debug.checkShaderErrors = false;
  host.appendChild(renderer.domElement);
  const canvas = renderer.domElement;

  const scene = new THREE.Scene();
  scene.background = new THREE.Color(0xe9e4da);
  const camera = new THREE.PerspectiveCamera(FOV, 1, 0.1, 250);

  const cx = map.cols / 2;
  const cz = map.rows / 2;
  const diag = Math.hypot(map.cols, map.rows);

  // 조명: 해(그림자) + 보조 방향광 + 반구광.
  const sun = new THREE.DirectionalLight(0xffffff, 2.5);
  sun.position.set(cx - 12, 26, cz - 18);
  sun.target.position.set(cx, 0, cz);
  sun.castShadow = true;
  sun.shadow.mapSize.set(2048, 2048);
  const half = diag / 2 + 4;
  Object.assign(sun.shadow.camera, { left: -half, right: half, top: half, bottom: -half, near: 0.5, far: 120 });
  scene.add(sun, sun.target);
  const fill = new THREE.DirectionalLight(0xffffff, 0.75);
  fill.position.set(cx + 10, 14, cz + 16);
  scene.add(fill);
  scene.add(new THREE.HemisphereLight(0xfffaf0, 0x8a8170, 1.45));

  // 바닥 · 벽: 인스턴스 메시 하나씩.
  const wallSet = new Set();
  for (const w of map.walls) {
    for (let y = w.y; y < w.y + w.h; y++) { for (let x = w.x; x < w.x + w.w; x++) { wallSet.add(`${x},${y}`); } }
  }
  const floorTiles = [];
  const wallTiles = [];
  for (let y = 0; y < map.rows; y++) {
    for (let x = 0; x < map.cols; x++) { (wallSet.has(`${x},${y}`) ? wallTiles : floorTiles).push([x, y]); }
  }
  const tmp = new THREE.Object3D();
  const color = new THREE.Color();
  const floor = new THREE.InstancedMesh(new THREE.BoxGeometry(1, 0.1, 1),
    new THREE.MeshStandardMaterial({ roughness: 0.9 }), floorTiles.length);
  floorTiles.forEach(([x, y], i) => {
    tmp.position.set(x + 0.5, -0.05, y + 0.5);
    tmp.updateMatrix();
    floor.setMatrixAt(i, tmp.matrix);
    const office = y <= 9;
    const hall = y === 10 || y === 11;
    const shade = (x + y) % 2 === 0 ? 0 : 0.03;
    if (office) { color.setRGB(0.80 - shade, 0.70 - shade, 0.56 - shade); }
    else if (hall) { color.setRGB(0.72 - shade, 0.72 - shade, 0.70 - shade); }
    else { color.setRGB(0.74 - shade, 0.79 - shade, 0.84 - shade); }
    floor.setColorAt(i, color);
  });
  floor.receiveShadow = true;
  scene.add(floor);
  const walls = new THREE.InstancedMesh(new THREE.BoxGeometry(1, 0.9, 1),
    new THREE.MeshStandardMaterial({ color: 0xf6f2ea, roughness: 0.95 }), wallTiles.length);
  wallTiles.forEach(([x, y], i) => {
    tmp.position.set(x + 0.5, 0.45, y + 0.5);
    tmp.updateMatrix();
    walls.setMatrixAt(i, tmp.matrix);
  });
  walls.castShadow = true;
  walls.receiveShadow = true;
  scene.add(walls);

  // 가구.
  const furnitureGroup = new THREE.Group();
  const wood = new THREE.MeshStandardMaterial({ color: 0x9b7653, roughness: 0.8 });
  const screen = new THREE.MeshStandardMaterial({ color: 0x1f2a33, roughness: 0.4 });
  const sheet = new THREE.MeshStandardMaterial({ color: 0xf3efe6, roughness: 0.9 });
  const pillowMat = new THREE.MeshStandardMaterial({ color: 0xffffff, roughness: 0.9 });
  const box = (w, h, d, mat, x, y, z) => {
    const m = new THREE.Mesh(new THREE.BoxGeometry(w, h, d), mat);
    m.position.set(x, y, z);
    m.castShadow = true;
    m.receiveShadow = true;
    furnitureGroup.add(m);
    return m;
  };
  for (const f of map.furniture) {
    const fx = f.x + f.w / 2;
    const fz = f.y + f.h / 2;
    if (f.kind === "desk") {
      box(f.w - 0.1, 0.06, f.h - 0.15, wood, fx, 0.74, fz);
      box(0.08, 0.7, 0.6, wood, f.x + 0.1, 0.36, fz);
      box(0.08, 0.7, 0.6, wood, f.x + f.w - 0.1, 0.36, fz);
      box(0.7, 0.42, 0.05, screen, fx, 1.0, fz - 0.15);
    } else if (f.kind === "bed") {
      box(f.w - 0.1, 0.3, f.h - 0.1, wood, fx, 0.15, fz);
      box(f.w - 0.16, 0.12, f.h - 0.2, sheet, fx, 0.36, fz);
      box(f.w - 0.3, 0.1, 0.35, pillowMat, fx, 0.46, f.y + 0.35);
    } else if (f.kind === "table") {
      box(f.w - 0.2, 0.08, f.h - 0.2, wood, fx, 0.74, fz);
      box(0.2, 0.7, 0.2, wood, fx, 0.36, fz);
    }
  }
  // 의자: 책상 자리와 논의 자리.
  const chairMat = new THREE.MeshStandardMaterial({ color: 0x55606b, roughness: 0.8 });
  for (const id of Object.keys(map.agents)) {
    const d = map.agents[id].desk;
    box(0.5, 0.08, 0.5, chairMat, d.x + 0.5, 0.42, d.y + 0.5);
  }
  for (const s of map.meeting_seats) { box(0.45, 0.08, 0.45, chairMat, s.x + 0.5, 0.42, s.y + 0.5); }
  scene.add(furnitureGroup);

  const table = map.furniture.find((f) => f.kind === "table");
  const tableCenter = table ? { x: table.x + table.w / 2, y: table.y + table.h / 2 } : { x: cx, y: cz };

  // 캐릭터와 이름표.
  const actors = new Map();
  const ids = Object.keys(map.agents);
  for (const [i, id] of ids.entries()) {
    const parts = makeCharacter(COLORS[id] ?? 0x888888);
    parts.root.userData.agentId = id;
    parts.root.traverse((o) => { o.userData.agentId = id; });
    scene.add(parts.root);
    const tag = document.createElement("div");
    tag.className = "tag";
    labels.appendChild(tag);
    const bed = map.agents[id].bed;
    actors.set(id, {
      id, parts, tag, phase: i * 1.7,
      x: bed.x + 0.5, y: bed.y + 0.5, yaw: 0,
      view: { place: "bed", work: "idle" },
      target: null, path: null, speed: WALK_SPEED, retryAt: 0, walking: false,
      label: "",
    });
  }

  function anchorOf(actor) {
    const a = map.agents[actor.id];
    if (actor.view.place === "desk") { return { x: a.desk.x, y: a.desk.y, facing: Math.PI }; }
    if (actor.view.place === "meeting") {
      const order = ids.filter((id) => actors.get(id)?.view.place === "meeting");
      const seat = map.meeting_seats[Math.max(0, order.indexOf(actor.id)) % map.meeting_seats.length];
      const yaw = Math.atan2(tableCenter.x - (seat.x + 0.5), tableCenter.y - (seat.y + 0.5));
      return { x: seat.x, y: seat.y, facing: yaw };
    }
    return { x: a.bed.x, y: a.bed.y, facing: 0 };
  }

  function placeAt(actor, anchor) {
    actor.x = anchor.x + 0.5;
    actor.y = anchor.y + 0.5;
    actor.yaw = anchor.facing;
    actor.path = null;
    actor.walking = false;
  }

  function retarget(actor, snap) {
    const anchor = anchorOf(actor);
    actor.target = anchor;
    if (snap) { placeAt(actor, anchor); return; }
    actor.path = findPath(grid, { x: Math.floor(actor.x), y: Math.floor(actor.y) }, anchor);
    actor.retryAt = actor.path ? 0 : performance.now() + RETRY_MS;
    actor.walking = Boolean(actor.path);
  }

  /** 담당 모습을 바꾼다. snap이면 걷지 않고 바로 놓는다(접속 스냅샷). */
  function setAgent(id, view, { snap = false } = {}) {
    const actor = actors.get(id);
    if (!actor) { return; }
    const placeChanged = actor.view.place !== view.place;
    actor.view = { place: view.place, work: view.work };
    actor.speed = view.place === "meeting" ? SUMMON_SPEED : WALK_SPEED;
    if (placeChanged || snap || !actor.target) {
      retarget(actor, snap);
      // 논의 자리는 참가 순서로 정하므로 같이 앉은 담당의 자리도 다시 본다.
      for (const other of actors.values()) {
        if (other !== actor && other.view.place === "meeting") { retarget(other, snap); }
      }
    }
  }

  function snapAll() {
    for (const actor of actors.values()) { if (actor.target) { placeAt(actor, actor.target); } }
  }

  function step(actor, dt, now) {
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
      const want = Math.atan2(dx, dy);
      actor.yaw += angleDiff(want, actor.yaw) * (1 - Math.exp(-14 * Math.min(dt, 0.1)));
    }
    if (actor.path && actor.path.length === 0) { placeAt(actor, actor.target); }
  }

  function angleDiff(a, b) {
    let d = a - b;
    while (d > Math.PI) { d -= Math.PI * 2; }
    while (d < -Math.PI) { d += Math.PI * 2; }
    return d;
  }

  function near(actor, p) { return Math.hypot(actor.x - (p.x + 0.5), actor.y - (p.y + 0.5)) <= POSE_RADIUS; }

  // 위치에서 포즈를 유도한다.
  function poseOf(actor) {
    if (actor.walking) { return "walk"; }
    const a = map.agents[actor.id];
    if (near(actor, a.bed)) { return "lie"; }
    if (near(actor, a.desk)) { return "sit"; }
    if (map.meeting_seats.some((s) => near(actor, s))) { return "sit"; }
    return "stand";
  }

  function animate(actor, t) {
    const p = actor.parts;
    const pose = poseOf(actor);
    const work = actor.view.work;
    const k = t + actor.phase;
    p.root.position.set(actor.x, 0, actor.y);
    p.root.rotation.set(0, actor.yaw, 0);
    p.body.position.set(0, 0, 0);
    p.body.rotation.set(0, 0, 0);
    p.head.rotation.set(0, 0, 0);
    for (const limb of [p.armL, p.armR, p.legL, p.legR]) { limb.rotation.set(0, 0, 0); }
    let headY = 1.4;
    if (pose === "walk") {
      const s = Math.sin(k * 9);
      p.armL.rotation.x = s * 0.55;
      p.armR.rotation.x = -s * 0.55;
      p.legL.rotation.x = -s * 0.5;
      p.legR.rotation.x = s * 0.5;
    } else if (pose === "sit") {
      p.body.position.y = -0.04;
      p.legL.rotation.x = -Math.PI / 2;
      p.legR.rotation.x = -Math.PI / 2;
      p.armL.rotation.x = -0.5;
      p.armR.rotation.x = -0.5;
      headY = 1.3;
    } else if (pose === "lie") {
      p.root.rotation.set(0, 0, 0);
      p.body.rotation.x = -Math.PI / 2;
      p.body.position.set(0, 0.6, 0.1);
      headY = 0.95;
    }
    if (pose !== "walk" && pose !== "lie") {
      if (work === "thinking") {
        p.armR.rotation.x = -2.5;
        p.head.rotation.x = 0.25;
      } else if (work === "failed") {
        p.head.rotation.x = 0.45;
      } else {
        p.head.rotation.y = Math.sin(k * (Math.PI * 2 / 13)) * 0.18;
        p.head.rotation.x = Math.sin(k * (Math.PI * 2 / 17)) * 0.05;
      }
    }
    const breathe = pose === "lie" ? Math.sin(k * 1.2) * 0.02 : Math.sin(k * 2.3) * 0.012;
    p.body.scale.set(1, 1 + breathe, 1);
    actor.headY = headY;

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
  function placeLabels(width, height) {
    for (const actor of actors.values()) {
      projected.set(actor.x, actor.headY ?? 1.4, actor.y).project(camera);
      const visible = projected.z < 1 && Math.abs(projected.x) <= 1.05 && Math.abs(projected.y) <= 1.05;
      if (!visible) { actor.tag.style.display = "none"; continue; }
      actor.tag.style.display = "";
      const sx = (projected.x + 1) / 2 * width;
      const sy = (1 - projected.y) / 2 * height;
      actor.tag.style.transform = `translate(${Math.round(sx)}px, ${Math.round(sy)}px) translate(-50%, -100%)`;
    }
  }

  // 카메라: 원근 38도, 궤도 조작. 원하는 값으로 부드럽게 따라간다(감쇠).
  const target = new THREE.Vector3(cx, 0, cz);
  const want = { radius: 30, polar: 0.85, theta: 0, tx: cx, tz: cz };
  const cur = { ...want };
  let userMoved = false;
  function fitDistance() {
    const vHalf = THREE.MathUtils.degToRad(FOV / 2);
    const hHalf = Math.atan(Math.tan(vHalf) * camera.aspect);
    return THREE.MathUtils.clamp((diag / 2) / Math.sin(Math.min(vHalf, hHalf)) * 1.1, MIN_DIST, MAX_DIST);
  }
  function clampWant() {
    want.radius = THREE.MathUtils.clamp(want.radius, MIN_DIST, MAX_DIST);
    want.polar = THREE.MathUtils.clamp(want.polar, MIN_POLAR, MAX_POLAR);
    want.tx = THREE.MathUtils.clamp(want.tx, 0, map.cols);
    want.tz = THREE.MathUtils.clamp(want.tz, 0, map.rows);
  }
  function updateCamera(dt) {
    const a = 1 - Math.exp(-12 * dt);
    for (const key of Object.keys(want)) { cur[key] += (want[key] - cur[key]) * a; }
    target.set(cur.tx, 0, cur.tz);
    const s = Math.sin(cur.polar);
    camera.position.set(
      target.x + cur.radius * s * Math.sin(cur.theta),
      cur.radius * Math.cos(cur.polar),
      target.z + cur.radius * s * Math.cos(cur.theta));
    camera.lookAt(target);
  }

  function resize() {
    const w = host.clientWidth || 1;
    const h = host.clientHeight || 1;
    renderer.setSize(w, h, false);
    camera.aspect = w / h;
    camera.updateProjectionMatrix();
    if (!userMoved) { want.radius = fitDistance(); }
  }
  const observer = new ResizeObserver(resize);
  observer.observe(host);
  resize();
  Object.assign(cur, want);

  // 누르기 · 끌기. 5px 미만으로 움직인 누름만 고르기로 친다.
  const pointers = new Map();
  let press = null;
  let pinch = null;
  function pan(dx, dy) {
    const scale = cur.radius / (host.clientHeight || 1) * 1.2;
    const sinT = Math.sin(cur.theta);
    const cosT = Math.cos(cur.theta);
    want.tx += (-dx * cosT - dy * sinT) * scale;
    want.tz += (dx * sinT - dy * cosT) * scale;
  }
  function rotate(dx, dy) {
    want.theta -= dx * 0.005;
    want.polar -= dy * 0.005;
  }
  function onDown(e) {
    canvas.setPointerCapture?.(e.pointerId);
    pointers.set(e.pointerId, { x: e.clientX, y: e.clientY });
    if (pointers.size === 1) {
      press = { x: e.clientX, y: e.clientY, button: e.button, dragging: false };
    } else {
      press = null;
      const [p1, p2] = [...pointers.values()];
      pinch = { dist: Math.hypot(p1.x - p2.x, p1.y - p2.y), angle: Math.atan2(p2.y - p1.y, p2.x - p1.x) };
    }
  }
  function onMove(e) {
    const prev = pointers.get(e.pointerId);
    if (!prev) { return; }
    const dx = e.clientX - prev.x;
    const dy = e.clientY - prev.y;
    prev.x = e.clientX;
    prev.y = e.clientY;
    if (pointers.size >= 2) {
      const [p1, p2] = [...pointers.values()];
      const dist = Math.hypot(p1.x - p2.x, p1.y - p2.y);
      const angle = Math.atan2(p2.y - p1.y, p2.x - p1.x);
      if (pinch && dist > 0) {
        want.radius *= pinch.dist / dist;
        want.theta -= angle - pinch.angle;
      }
      pinch = { dist, angle };
      userMoved = true;
      clampWant();
      return;
    }
    if (!press) { return; }
    if (!press.dragging && Math.hypot(e.clientX - press.x, e.clientY - press.y) < PRESS_SLOP_PX) { return; }
    press.dragging = true;
    userMoved = true;
    if (press.button === 2) { rotate(dx, dy); } else { pan(dx, dy); }
    clampWant();
  }
  const raycaster = new THREE.Raycaster();
  const ndc = new THREE.Vector2();
  function pick(clientX, clientY) {
    const rect = canvas.getBoundingClientRect();
    ndc.set(((clientX - rect.left) / rect.width) * 2 - 1, -((clientY - rect.top) / rect.height) * 2 + 1);
    raycaster.setFromCamera(ndc, camera);
    const hits = raycaster.intersectObjects([...actors.values()].map((a) => a.parts.root), true);
    const id = hits.find((h) => h.object.userData.agentId)?.object.userData.agentId;
    if (id) { onPick?.(id); }
  }
  function onUp(e) {
    pointers.delete(e.pointerId);
    if (pointers.size < 2) { pinch = null; }
    if (press && !press.dragging && pointers.size === 0 && press.button === 0) { pick(e.clientX, e.clientY); }
    if (pointers.size === 0) { press = null; }
  }
  function onWheel(e) {
    e.preventDefault();
    want.radius *= Math.exp(e.deltaY * 0.001);
    userMoved = true;
    clampWant();
  }
  const onContext = (e) => e.preventDefault();
  canvas.addEventListener("pointerdown", onDown);
  canvas.addEventListener("pointermove", onMove);
  canvas.addEventListener("pointerup", onUp);
  canvas.addEventListener("pointercancel", onUp);
  canvas.addEventListener("wheel", onWheel, { passive: false });
  canvas.addEventListener("contextmenu", onContext);

  // 문맥을 잃으면 가벼운 화면으로 넘긴다.
  const onContextLost = (e) => { e.preventDefault(); onLost?.(); };
  canvas.addEventListener("webglcontextlost", onContextLost);

  // 탭이 숨었다 돌아오면 걷던 담당을 목적지에 바로 놓는다.
  const onVisibility = () => { if (!document.hidden) { snapAll(); last = performance.now(); } };
  document.addEventListener("visibilitychange", onVisibility);

  // 자동 해상도 낮춤: 3초 창마다 fps · p95를 재고, fps < 55 또는 p95 > 25ms가 두 창 연속이면 0.25 내린다.
  let windowStart = performance.now();
  let frameTimes = [];
  let badWindows = 0;
  function measure(now, frameMs) {
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

  let raf = 0;
  let last = performance.now();
  const clock0 = performance.now();
  function frame(now) {
    raf = requestAnimationFrame(frame);
    if (document.hidden) { last = now; return; }
    const frameMs = now - last;
    const dt = Math.min(frameMs / 1000, 0.2);
    last = now;
    const t = (now - clock0) / 1000;
    for (const actor of actors.values()) { step(actor, dt, now); animate(actor, t); }
    updateCamera(dt);
    renderer.render(scene, camera);
    placeLabels(host.clientWidth, host.clientHeight);
    measure(now, frameMs);
  }
  raf = requestAnimationFrame(frame);

  function dispose() {
    cancelAnimationFrame(raf);
    observer.disconnect();
    document.removeEventListener("visibilitychange", onVisibility);
    canvas.removeEventListener("pointerdown", onDown);
    canvas.removeEventListener("pointermove", onMove);
    canvas.removeEventListener("pointerup", onUp);
    canvas.removeEventListener("pointercancel", onUp);
    canvas.removeEventListener("wheel", onWheel);
    canvas.removeEventListener("contextmenu", onContext);
    canvas.removeEventListener("webglcontextlost", onContextLost);
    scene.traverse((o) => {
      if (o.geometry) { o.geometry.dispose(); }
      const mats = Array.isArray(o.material) ? o.material : o.material ? [o.material] : [];
      for (const m of mats) { m.dispose(); }
    });
    renderer.dispose();
    renderer.forceContextLoss();
    canvas.remove();
    for (const actor of actors.values()) { actor.tag.remove(); }
  }

  return { setAgent, dispose };
}
