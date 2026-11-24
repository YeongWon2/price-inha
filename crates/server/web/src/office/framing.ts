// 카메라 잡기(server 20차 고침 8번 · N3 검토 1 · 2번).
// - 처음 잡기: 여섯 담당의 업무 자리 · 팀장 자리 · 회의 자리 · 쉼 자리를 모두 담는 경계(여유 포함)를,
//   기울어진 원근 카메라로 실제로 투영해 화면 안(가장자리 6% 여유)에 들어오는 가장 가까운 거리로 잡는다.
// - 따라 비추기: 담당 다수가 미팅룸(3명 이상)이나 쉼 자리(4명 이상)로 옮기면 원본 회의 카메라처럼 그쪽을
//   가운데 두고 가깝게 잡는다. 사용자가 끌기 · 확대를 했으면 따라가지 않는다(장면 쪽이 정한다).

import * as THREE from "three";
import type { Bounds } from "./camera";
import type { OfficeMap } from "./scene3d";

/** 화면에 담을 높이(서 있는 캐릭터 머리까지). */
const TOP = 1.9;
/** 가장자리 여유(NDC). */
const EDGE = 0.94;
/** 경계 여유(칸). */
const MARGIN = 1.5;
/** 따라 비출 때의 경계 여유(칸). */
const FOCUS_MARGIN = 2.5;

/** 잡은 자리: 바라보는 점과 거리. */
export interface View { distance: number; tx: number; tz: number }

/** 어디를 비추는가: 모두 · 미팅룸 · 쉼 자리. */
export type Focus = "all" | "meeting" | "rest";

function around(points: { x: number; y: number }[], margin: number): Bounds {
  const xs = points.map((p) => p.x + 0.5);
  const zs = points.map((p) => p.y + 0.5);
  return {
    minX: Math.min(...xs) - margin, maxX: Math.max(...xs) + margin,
    minZ: Math.min(...zs) - margin, maxZ: Math.max(...zs) + margin,
  };
}

/** 비출 곳의 경계. */
export function focusBounds(map: OfficeMap, focus: Focus): Bounds {
  const agents = Object.values(map.agents);
  if (focus === "meeting" && map.meeting_seats.length > 0) { return around(map.meeting_seats, FOCUS_MARGIN); }
  if (focus === "rest") { return around(agents.map((a) => a.rest), FOCUS_MARGIN); }
  return around([...agents.flatMap((a) => [a.desk, a.rest]), ...map.meeting_seats], MARGIN);
}

/** 담당들의 자리로 비출 곳을 정한다. */
export function focusOf(places: readonly string[]): Focus {
  const count = (p: string) => places.filter((x) => x === p).length;
  if (count("meeting") >= 3) { return "meeting"; }
  if (count("bed") >= 4) { return "rest"; }
  return "all";
}

/**
 * 경계 `b`가 내려다보는 각 `polar`의 카메라 화면에 다 들어오는 가장 가까운 자리. `centered`면 경계 가운데를 바라보고
 * (따라 비추기 — 그쪽이 화면 가운데), 아니면 바라보는 점을 앞뒤로 옮겨 가장 가깝게 잡는다(처음 화면 — 캐릭터를 크게).
 */
export function fitView(b: Bounds, polar: number, aspect: number, fov: number, centered = false): View {
  const cam = new THREE.PerspectiveCamera(fov, aspect, 0.1, 400);
  const tx = (b.minX + b.maxX) / 2;
  const pts: Obj[] = [];
  for (const x of [b.minX, b.maxX]) { for (const z of [b.minZ, b.maxZ]) { for (const y of [0, TOP]) { pts.push(new THREE.Vector3(x, y, z)); } } }
  const q = new THREE.Vector3();
  const fits = (d: number, tz: number) => {
    cam.position.set(tx, d * Math.cos(polar), tz + d * Math.sin(polar));
    cam.lookAt(tx, 0, tz);
    cam.updateMatrixWorld();
    return pts.every((p: Obj) => { q.copy(p).project(cam); return Math.abs(q.x) <= EDGE && Math.abs(q.y) <= EDGE && q.z < 1; });
  };
  let best: View = { distance: 80, tx, tz: (b.minZ + b.maxZ) / 2 };
  const mid = (b.minZ + b.maxZ) / 2;
  for (let tz = centered ? mid : b.minZ; tz <= (centered ? mid : b.maxZ) + 1e-6; tz += 0.5) {
    let lo = 2;
    let hi = best.distance;
    if (!fits(hi, tz)) { continue; }
    for (let i = 0; i < 30; i++) { const m = (lo + hi) / 2; if (fits(m, tz)) { hi = m; } else { lo = m; } }
    if (hi < best.distance) { best = { distance: hi, tx, tz }; }
  }
  return best;
}

type Obj = any;
