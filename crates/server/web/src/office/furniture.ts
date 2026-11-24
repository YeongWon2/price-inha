// 가구(server 20차 3번 · 결정 13): 원본 "종합상사" 배치의 가구를 원본 GLB 자산으로 놓는다.
// - 칸 (x, y)에서 시작하는 w×h 칸 가구는 가운데 (x + w/2, 0, y + h/2)에 놓고, 방향은 원본처럼
//   down 0 · up π · left -π/2 · right π/2 (자산의 앞은 +Z, 1칸 = 1m, 바닥이 원점).
// - 원본처럼 자산을 부르는 동안과 부르지 못했을 때는 간단한 도형(대신 그림)을 둔다.
// - 종류 · 변형 → 자산은 원본 자산 목록(scene-asset-definitions)의 이름을 따른다. 원본이 코드로 그리는 변형
//   (trading-files · trading-reception · trading-sample-table · 프린터)은 가장 가까운 원본 자산 또는 도형으로 둔다.

import * as THREE from "three";
import { instance, loadGltf } from "./gltf";

type Obj = any;

/** 원본 자산이 놓인 같은 출처 경로(앞부분). */
export const ASSET_DIR = "/office/deskrpg/";

/** 지도의 가구 하나(office-map.json `furniture`). */
export interface Furniture {
  kind: string;
  variant?: string;
  x: number;
  y: number;
  w: number;
  h: number;
  dir?: string;
  blocks?: boolean;
}

/** 방향 → y축 회전(원본과 같다). */
export const FACING: Record<string, number> = { down: 0, up: Math.PI, left: -Math.PI / 2, right: Math.PI / 2 };

/** 바라보는 쪽 글자 → y축 회전. */
export function facingYaw(facing: string | undefined): number {
  return FACING[facing ?? "down"] ?? 0;
}

const shared = (name: string) => `${ASSET_DIR}shared/furniture/${name}-v1.glb`;
const trading = (name: string) => `${ASSET_DIR}shared/trading/${name}-v1.glb`;
const landscape = (name: string) => `${ASSET_DIR}shared/landscape/${name}-v1.glb`;

/** 가구의 원본 자산 주소. 자산이 없는 것(프린터)은 null. */
export function furnitureAsset(f: Furniture): string | null {
  const v = f.variant ?? "";
  switch (f.kind) {
    case "chair": return shared("office-chair");
    case "plant": return landscape((f.x + f.y) % 2 ? "olive" : "ficus"); // 원본: 칸 합이 홀수면 올리브
    case "conference_table": return shared("conference-table");
    case "whiteboard": return shared("mobile-board");
    case "low_cabinet": return shared("low-shelf");
    case "studio_sofa": return shared("sofa");
    case "office_armchair": return shared("armchair");
    case "studio_counter": return trading("trade-packing-bench");
    case "studio_shelf": return trading("trade-sample-display");
    case "meeting_table": return trading(v.startsWith("trade-") ? v : "trade-square-table");
    case "reception_desk":
      if (v === "tech-workstation") { return `${ASSET_DIR}shared/tech/tech-workstation-v1.glb`; }
      if (v.startsWith("trade-")) { return trading(v); }
      return shared("credenza");
    default: return null;
  }
}

/** 대신 그림의 높이(m). */
const STAND_IN_HEIGHT: Record<string, number> = {
  chair: 0.5, plant: 1.2, conference_table: 0.78, whiteboard: 1.8, low_cabinet: 1.0, studio_sofa: 0.8,
  office_armchair: 0.8, studio_counter: 0.95, studio_shelf: 1.0, meeting_table: 0.76, reception_desk: 0.76,
  office_printer: 0.9,
};

function standIn(f: Furniture, mat: Obj): Obj {
  const turned = f.dir === "left" || f.dir === "right";
  const w = (turned ? f.h : f.w) * 0.86;
  const d = (turned ? f.w : f.h) * 0.86;
  const h = STAND_IN_HEIGHT[f.kind] ?? 0.8;
  const mesh = new THREE.Mesh(new THREE.BoxGeometry(w, h, d), mat);
  mesh.position.y = h / 2;
  mesh.castShadow = true;
  mesh.receiveShadow = true;
  return mesh;
}

/** 가구를 모두 놓는다. 원본 자산은 늦게 들어온다(실패하면 대신 그림이 남는다). */
export function placeFurniture(root: Obj, list: readonly Furniture[]): void {
  const mats: Record<string, Obj> = {
    wood: new THREE.MeshStandardMaterial({ color: 0xb79a74, roughness: 0.8 }),
    seat: new THREE.MeshStandardMaterial({ color: 0x263e59, roughness: 0.8 }),
    green: new THREE.MeshStandardMaterial({ color: 0x5f7f52, roughness: 0.9 }),
    grey: new THREE.MeshStandardMaterial({ color: 0xd8d6d0, roughness: 0.7 }),
  };
  const matOf = (kind: string) =>
    kind === "plant" ? mats.green : kind === "office_printer" ? mats.grey
      : ["chair", "studio_sofa", "office_armchair"].includes(kind) ? mats.seat : mats.wood;
  for (const f of list) {
    const holder = new THREE.Group();
    holder.name = `furniture:${f.kind}`;
    holder.position.set(f.x + f.w / 2, 0, f.y + f.h / 2);
    holder.rotation.y = FACING[f.dir ?? "down"] ?? 0;
    const temp = standIn(f, matOf(f.kind));
    holder.add(temp);
    root.add(holder);
    const url = furnitureAsset(f);
    if (!url) { continue; }
    loadGltf(url).then((gltf) => {
      if (!holder.parent) { return; }
      holder.remove(temp);
      temp.geometry.dispose();
      holder.add(instance(gltf.scene, false));
    }, () => { /* 원본처럼 대신 그림을 남긴다 */ });
  }
}
