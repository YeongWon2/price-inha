// 순찰을 3D 사무실에 잇는다(server 24차 1번 — 뼈대, red).

import type { NavMap } from "./nav";
import type { PatrolMap } from "./patrol";
import type { Scene } from "./scene3d";

export interface PatrolOptions {
  map: PatrolMap & NavMap;
  view: () => Scene | null;
  agents: () => ReadonlyMap<string, { place: string; work: string }>;
  light: () => boolean;
  random?: () => number;
}

export function startPatrol(options: PatrolOptions): () => void {
  void options;
  return () => undefined;
}
