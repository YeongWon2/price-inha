// 사무실 카메라(server 20차 고침 8번): 처음 잡기(사무 공간이 화면을 채운다) · 확대 범위. 장면 없이 시험하는 순수 함수.

/** 장면 경계(세계 좌표, x · z 평면). */
export interface Bounds {
  minX: number;
  maxX: number;
  minZ: number;
  maxZ: number;
}

export interface Viewport {
  width: number;
  height: number;
}

export interface Framing {
  /** 카메라가 보는 점에서 카메라까지의 거리. */
  distance: number;
  /** 보는 점(경계의 가운데). */
  target: { x: number; z: number };
}

/** 처음 잡기에서 경계 가로 둘레에 남기는 여백(가로의 4%). 15% 안이다. */
export const FRAME_MARGIN = 1.04;
/** 가장 가까이: 처음 거리의 이만큼. */
export const ZOOM_IN_LIMIT = 0.3;
/** 가장 멀리: 처음 거리의 이만큼. */
export const ZOOM_OUT_LIMIT = 2.2;

function tanHalf(fovDeg: number): number {
  return Math.tan(((fovDeg / 2) * Math.PI) / 180);
}

/** 세로 시야각 `fovDeg`, 거리 `distance`에서 보이는 세로 길이(세계 단위, 보는 점 높이). */
export function visibleHeight(distance: number, fovDeg: number): number {
  return 2 * distance * tanHalf(fovDeg);
}

/** 처음 잡기: 경계의 가로가 화면 가로를 꽉 채우는(넘치지 않고 15% 안으로 남는) 거리와 가운데 점. */
export function frameCamera(bounds: Bounds, viewport: Viewport, fovDeg: number): Framing {
  const aspect = Math.max(viewport.width, 1) / Math.max(viewport.height, 1);
  const width = Math.max(bounds.maxX - bounds.minX, 1e-3) * FRAME_MARGIN;
  const distance = width / aspect / (2 * tanHalf(fovDeg));
  return {
    distance,
    target: { x: (bounds.minX + bounds.maxX) / 2, z: (bounds.minZ + bounds.maxZ) / 2 },
  };
}

/** 키 `actorHeight`(세계 단위)인 캐릭터가 보는 점 근처에서 화면에 몇 픽셀 높이로 보이는가. */
export function actorScreenHeight(actorHeight: number, distance: number, fovDeg: number, viewportHeight: number): number {
  return (actorHeight / visibleHeight(distance, fovDeg)) * viewportHeight;
}

/** 확대 · 축소 범위 제한: 처음 잡은 거리 `framed`를 기준으로 가까이 · 멀리 모두 막는다. */
export function clampDistance(distance: number, framed: number): number {
  const lo = framed * ZOOM_IN_LIMIT;
  const hi = framed * ZOOM_OUT_LIMIT;
  if (!Number.isFinite(distance)) { return hi; }
  return Math.min(hi, Math.max(lo, distance));
}
