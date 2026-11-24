// 사무실 층(server 20차 고침 7번): 원본 "종합상사"(trading v3)의 바닥 · 바깥벽(창) · 출입구 · 방 칸막이 · 뒷벽 벽화.
// 원본 동작(trading-scene · office-perimeter · room-architecture)을 보고 새로 썼다:
// - 바닥은 돌 타일(칸마다 조금씩 다른 밝기) 아래 받침판, 가운데 아래쪽 빈 곳(void)은 정말 비어 있다.
// - 바깥벽은 바닥 윤곽에서 반 칸 안쪽 선을 따라 낮은 벽 + 유리 창 + 금속 기둥 · 띠. 뒷벽은 높이 2.85, 나머지는 1.25
//   (안이 보이게 낮다). 아래쪽 출입구(3칸)는 비운다.
// - 방 칸막이는 유리 칸막이(높이 1.8, 바닥 띠 · 윗 띠 · 양 끝 기둥), 문 칸은 비운다.
// - 뒷벽 가운데 세계 지도 벽화(shared/trading/world-mural.webp).

import * as THREE from "three";
import { ASSET_DIR } from "./furniture";
import type { Rect } from "./nav";

type Obj = any;

/** 층 정의(office-map.json에서 쓰는 부분). */
export interface Floor {
  cols: number;
  rows: number;
  void?: Rect;
  entrance?: { fromCol: number; toCol: number; row: number };
  walls: (Rect & { kind?: string; dir?: string })[];
}

interface Run { x1: number; z1: number; x2: number; z2: number; height: number }

const METAL = 0x35434b;

/** 칸이 바닥인가(void가 아닌가). */
export function isFloor(floor: Floor, x: number, y: number): boolean {
  const v = floor.void;
  return !(v && x >= v.x && x < v.x + v.w && y >= v.y && y < v.y + v.h);
}

/** 바깥벽 선: 바닥 윤곽(빈 곳이 아래쪽을 파고든 모양)을 반 칸 안쪽으로. 출입구는 끊는다. */
export function perimeterRuns(floor: Floor): Run[] {
  const L = 0.5, R = floor.cols - 0.5, B = 0.5, F = floor.rows - 0.5;
  const v = floor.void;
  const points = v
    ? [[L, B], [R, B], [R, F], [v.x + v.w + 0.5, F], [v.x + v.w + 0.5, v.y - 0.5], [v.x - 0.5, v.y - 0.5], [v.x - 0.5, F], [L, F]]
    : [[L, B], [R, B], [R, F], [L, F]];
  const runs: Run[] = [];
  const e = floor.entrance;
  points.forEach(([x1, z1], i) => {
    const [x2, z2] = points[(i + 1) % points.length];
    const height = z1 === B && z2 === B ? 2.85 : 1.25;
    const lo = Math.min(x1, x2), hi = Math.max(x1, x2);
    if (e && z1 === F && z2 === F && lo < e.fromCol && hi > e.toCol) {
      runs.push({ x1: lo, z1, x2: e.fromCol, z2, height }, { x1: e.toCol + 1, z1, x2: hi, z2, height });
    } else {
      runs.push({ x1, z1, x2, z2, height });
    }
  });
  return runs;
}

function box(parent: Obj, w: number, h: number, d: number, mat: Obj, x: number, y: number, z: number): Obj {
  const mesh = new THREE.Mesh(new THREE.BoxGeometry(w, h, d), mat);
  mesh.position.set(x, y, z);
  mesh.castShadow = true;
  mesh.receiveShadow = true;
  parent.add(mesh);
  return mesh;
}

function addFrameRun(root: Obj, run: Run, mats: Record<string, Obj>): void {
  const length = Math.hypot(run.x2 - run.x1, run.z2 - run.z1);
  const g = new THREE.Group();
  g.position.set((run.x1 + run.x2) / 2, 0, (run.z1 + run.z2) / 2);
  if (run.x1 === run.x2) { g.rotation.y = Math.PI / 2; }
  root.add(g);
  box(g, length, 0.24, 0.18, mats.wall, 0, 0.12, 0);
  box(g, length, 0.055, 0.12, mats.metal, 0, run.height, 0);
  const count = Math.max(1, Math.ceil(length));
  const width = length / count;
  const glass = run.height > 2 ? mats.glassTall : mats.glass;
  for (let i = 0; i < count; i++) {
    const pane = box(g, width - 0.035, run.height - 0.3, 0.035, glass, -length / 2 + (i + 0.5) * width, (run.height + 0.24) / 2, 0);
    pane.castShadow = false;
  }
  for (let i = 0; i <= count; i++) {
    box(g, 0.065, run.height + 0.025, 0.14, mats.metal, -length / 2 + i * width, (run.height + 0.025) / 2, 0);
  }
  if (run.height > 2) {
    box(g, length, 0.045, 0.09, mats.metal, 0, 1.45, 0);
    box(g, length, 0.065, 0.28, mats.wood, 0, 0.3, 0);
  }
}

/** 칸막이 칸들을 가로 · 세로로 이어진 줄로 묶는다. */
function partitionRuns(cells: (Rect & { dir?: string })[]): { x: number; z: number; length: number; vertical: boolean }[] {
  const out: { x: number; z: number; length: number; vertical: boolean }[] = [];
  for (const vertical of [false, true]) {
    const set = new Set(cells.filter((c) => (c.dir === "v") === vertical).map((c) => `${c.x},${c.y}`));
    const done = new Set<string>();
    for (const key of [...set].sort()) {
      if (done.has(key)) { continue; }
      const [x, y] = key.split(",").map(Number);
      const prev = vertical ? `${x},${y - 1}` : `${x - 1},${y}`;
      if (set.has(prev)) { continue; }
      let n = 0;
      while (set.has(vertical ? `${x},${y + n}` : `${x + n},${y}`)) { done.add(vertical ? `${x},${y + n}` : `${x + n},${y}`); n++; }
      out.push(vertical ? { x: x + 0.5, z: y + n / 2, length: n, vertical } : { x: x + n / 2, z: y + 0.5, length: n, vertical });
    }
  }
  return out;
}

function addPartition(root: Obj, p: { x: number; z: number; length: number; vertical: boolean }, mats: Record<string, Obj>): void {
  const w = p.vertical ? 0.14 : p.length;
  const d = p.vertical ? p.length : 0.14;
  box(root, w, 0.055, d, mats.frame, p.x, 0.0275, p.z);
  const pane = box(root, p.vertical ? 0.035 : p.length - 0.05, 1.69, p.vertical ? p.length - 0.05 : 0.035, mats.partition, p.x, 0.925, p.z);
  pane.castShadow = false;
  box(root, w, 0.055, d, mats.frame, p.x, 1.8, p.z);
  for (const s of [-1, 1]) {
    const off = s * (p.length / 2 - 0.023);
    box(root, p.vertical ? 0.15 : 0.045, 1.75, p.vertical ? 0.045 : 0.15, mats.frame, p.x + (p.vertical ? 0 : off), 0.925, p.z + (p.vertical ? off : 0));
  }
}

function addFloor(root: Obj, floor: Floor): void {
  const cells: [number, number][] = [];
  for (let y = 0; y < floor.rows; y++) { for (let x = 0; x < floor.cols; x++) { if (isFloor(floor, x, y)) { cells.push([x, y]); } } }
  const stone = new THREE.MeshStandardMaterial({ color: 0xc4bfb3, roughness: 0.68 });
  const tiles = new THREE.InstancedMesh(new THREE.BoxGeometry(0.986, 0.04, 0.986), stone, cells.length);
  const m = new THREE.Matrix4();
  const c = new THREE.Color();
  cells.forEach(([x, y], i) => {
    m.makeTranslation(x + 0.5, -0.025, y + 0.5);
    tiles.setMatrixAt(i, m);
    const shade = 1 + Math.sin(x * 31.7 + y * 13.1) * 0.018;
    tiles.setColorAt(i, c.setRGB(shade, shade, shade));
  });
  tiles.receiveShadow = true;
  root.add(tiles);
  const slabMat = new THREE.MeshStandardMaterial({ color: 0xceb48a, roughness: 0.76 });
  const v = floor.void;
  const slabs: [number, number, number, number][] = v
    ? [[0, 0, floor.cols, v.y], [0, v.y, v.x, floor.rows - v.y], [v.x + v.w, v.y, floor.cols - v.x - v.w, floor.rows - v.y]]
    : [[0, 0, floor.cols, floor.rows]];
  for (const [x, z, w, d] of slabs) {
    const slab = box(root, w, 0.3, d, slabMat, x + w / 2, -0.21, z + d / 2);
    slab.castShadow = false;
  }
}

function addMural(root: Obj, cols: number): void {
  const mural = new THREE.Group();
  mural.position.set(cols / 2, 0, 0.61);
  root.add(mural);
  box(mural, 9.8, 2.9, 0.14, new THREE.MeshStandardMaterial({ color: 0xede7da, roughness: 0.9 }), 0, 1.55, 0);
  new THREE.TextureLoader().load(`${ASSET_DIR}shared/trading/world-mural.webp`, (texture: Obj) => {
    if (!root.parent) { texture.dispose(); return; }
    texture.colorSpace = THREE.SRGBColorSpace;
    const panel = new THREE.Mesh(new THREE.PlaneGeometry(9.5, 2.7), new THREE.MeshStandardMaterial({ map: texture, roughness: 0.94 }));
    panel.position.set(0, 1.55, 0.08);
    mural.add(panel);
  }, undefined, () => { /* 벽화 없이 둔다 */ });
}

/** 층 전체를 만든다. */
export function buildFloor(floor: Floor): Obj {
  const root = new THREE.Group();
  root.name = "trading-architecture";
  const glassBase = { color: 0xb5d3dd, transparent: true, roughness: 0.12, depthWrite: false };
  const mats = {
    wall: new THREE.MeshStandardMaterial({ color: 0xe6e0d2, roughness: 0.85 }),
    wood: new THREE.MeshStandardMaterial({ color: 0xc3aa82, roughness: 0.7 }),
    metal: new THREE.MeshStandardMaterial({ color: METAL, roughness: 0.5, metalness: 0.4 }),
    glass: new THREE.MeshStandardMaterial({ ...glassBase, opacity: 0.18 }),
    glassTall: new THREE.MeshStandardMaterial({ ...glassBase, opacity: 0.3 }),
    frame: new THREE.MeshStandardMaterial({ color: 0x435454, roughness: 0.5 }),
    partition: new THREE.MeshStandardMaterial({ color: 0xb9d2ce, roughness: 0.25, metalness: 0.05, transparent: true, opacity: 0.14, depthWrite: false }),
  };
  addFloor(root, floor);
  for (const run of perimeterRuns(floor)) { addFrameRun(root, run, mats); }
  for (const p of partitionRuns(floor.walls.filter((w) => w.kind === "partition"))) { addPartition(root, p, mats); }
  addMural(root, floor.cols);
  return root;
}
