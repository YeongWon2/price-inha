// 사무실 카메라 처음 잡기(server 20차 고침 8번): 사무 공간이 화면을 채우고 1280x800에서 캐릭터 키가 60px 이상이며,
// 확대 · 축소는 범위 안에서만 된다. 장면 없이 시험하는 순수 함수.
import { describe, expect, it } from "vitest";
import {
  actorScreenHeight,
  clampDistance,
  frameCamera,
  visibleHeight,
  type Bounds,
} from "../../web/src/office/camera";

const FOV = 45;
/** 사무 공간 30 x 18 세계 단위(가로로 긴 층). */
const FLOOR: Bounds = { minX: -15, maxX: 15, minZ: -9, maxZ: 9 };
const ACTOR = 1.7;

describe("사무실 카메라", () => {
  it("처음 잡기는 가운데를 보고 사무 공간 가로가 화면을 채운다(넘치지 않고 15% 안으로 남는다)", () => {
    // 검증: V15
    const view = { width: 1280, height: 800 };
    const framing = frameCamera(FLOOR, view, FOV);
    expect(framing.target.x).toBeCloseTo(0);
    expect(framing.target.z).toBeCloseTo(0);
    const width = visibleHeight(framing.distance, FOV) * (view.width / view.height);
    expect(width).toBeGreaterThanOrEqual(30 - 1e-6);
    expect(width).toBeLessThanOrEqual(30 * 1.15);
  });

  it("1280x800 처음 화면에서 캐릭터 키가 60px 이상이다", () => {
    // 검증: V15
    const view = { width: 1280, height: 800 };
    const { distance } = frameCamera(FLOOR, view, FOV);
    expect(actorScreenHeight(ACTOR, distance, FOV, view.height)).toBeGreaterThanOrEqual(60);
  });

  it("세로로 좁은 화면은 더 멀리서 잡는다", () => {
    // 검증: V15
    const wide = frameCamera(FLOOR, { width: 1280, height: 800 }, FOV).distance;
    const narrow = frameCamera(FLOOR, { width: 390, height: 800 }, FOV).distance;
    expect(narrow).toBeGreaterThan(wide);
  });

  it("확대 · 축소는 처음 잡은 거리를 기준으로 범위 안에 막힌다", () => {
    // 검증: V15
    const framed = frameCamera(FLOOR, { width: 1280, height: 800 }, FOV).distance;
    expect(clampDistance(framed, framed)).toBeCloseTo(framed);
    const closest = clampDistance(0, framed);
    const farthest = clampDistance(framed * 1e6, framed);
    expect(closest).toBeGreaterThan(0);
    expect(closest).toBeLessThan(framed);
    expect(farthest).toBeGreaterThan(framed);
    expect(farthest).toBeLessThan(framed * 1e6);
    expect(clampDistance(framed * 0.99, framed)).toBeLessThanOrEqual(framed);
    expect(clampDistance(framed * 1.01, framed)).toBeGreaterThanOrEqual(framed);
  });
});
