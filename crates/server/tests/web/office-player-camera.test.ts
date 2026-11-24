// 누르기와 끌기 · 카메라 따라가기(server 32차 5번 · 6번, V56): 5px 이상 움직이면 끌기(누르기로 치지 않는다),
// 따라가기는 기본 켜짐이고 끌기 · 돌리기 · 확대(와 각도 · 전체 보기)에 풀리며 "따라가기" 단추에 다시 켜진다.
// 따라가기는 매 프레임 보는 점을 캐릭터 쪽으로 0.08만큼 옮긴다. 켜져 있으면 회의 발언자 카메라가 끼어들지 않는다.
// 원본 DeskRPG 수치(docs/research/metaverse-office-refs.md B절). 장면 없이 상태와 셈만 돌린다.
import { describe, expect, it } from "vitest";
import { FOLLOW_LERP, FollowState, followStep, type CameraTouch } from "../../web/src/office/follow";
import { DRAG_THRESHOLD_PX, PointerGesture } from "../../web/src/office/gesture";
import { SpeakerCamera } from "../../web/src/office/speaker-camera";

const T0 = 1_000_000;

describe("누르기와 끌기", () => {
  it("5px 안에서 떼면 누르기다", () => {
    // 검증: V56
    expect(DRAG_THRESHOLD_PX).toBe(5);
    const g = new PointerGesture();
    g.down(100, 100);
    g.move(103, 102);
    expect(g.dragging()).toBe(false);
    expect(g.up(103, 102)).toBe("tap");
  });

  it("5px 이상 움직이면 끌기이고, 제자리로 돌아와 떼도 끌기다", () => {
    // 검증: V56
    const g = new PointerGesture();
    g.down(100, 100);
    g.move(103, 104);
    expect(g.dragging()).toBe(true);
    expect(g.up(103, 104)).toBe("drag");
    g.down(10, 10);
    g.move(30, 10);
    g.move(10, 10);
    expect(g.up(10, 10)).toBe("drag");
  });

  it("누르지 않고 떼면 아무것도 아니고, 다음 누르기는 새로 판단한다", () => {
    // 검증: V56
    const g = new PointerGesture();
    expect(g.up(5, 5)).toBeNull();
    g.down(0, 0);
    g.move(50, 0);
    expect(g.up(50, 0)).toBe("drag");
    g.down(0, 0);
    expect(g.up(1, 1)).toBe("tap");
  });
});

describe("따라가기 상태", () => {
  it("기본은 켜짐이다", () => {
    // 검증: V56
    expect(new FollowState().on()).toBe(true);
  });

  it("끌기 · 돌리기 · 확대(와 각도 · 전체 보기)에 풀리고 단추에 다시 켜진다", () => {
    // 검증: V56
    const kinds: CameraTouch[] = ["drag", "rotate", "zoom", "angle", "overview"];
    for (const kind of kinds) {
      const follow = new FollowState();
      follow.touch(kind);
      expect(follow.on(), kind).toBe(false);
      follow.resume();
      expect(follow.on(), kind).toBe(true);
    }
  });

  it("켜짐 · 꺼짐이 바뀔 때만 알리고, 끊으면 더 알리지 않는다", () => {
    // 검증: V56
    const follow = new FollowState();
    const seen: boolean[] = [];
    const stop = follow.onChange((on) => seen.push(on));
    follow.touch("drag");
    follow.touch("zoom");
    follow.resume();
    follow.resume();
    expect(seen).toEqual([false, true]);
    stop();
    follow.touch("rotate");
    expect(seen).toEqual([false, true]);
    expect(follow.on()).toBe(false);
  });

  it("한 프레임에 보는 점을 캐릭터 쪽으로 0.08만큼 옮기고 결국 캐릭터에 닿는다", () => {
    // 검증: V56
    expect(FOLLOW_LERP).toBe(0.08);
    const one = followStep({ x: 0, z: 0 }, { x: 10, z: 20 });
    expect(one.x).toBeCloseTo(0.8, 9);
    expect(one.z).toBeCloseTo(1.6, 9);
    let target = { x: 0, z: 0 };
    for (let i = 0; i < 300; i += 1) target = followStep(target, { x: 10, z: 20 });
    expect(target.x).toBeCloseTo(10, 3);
    expect(target.z).toBeCloseTo(20, 3);
    const half = followStep({ x: 0, z: 0 }, { x: 4, z: -2 }, 0.5);
    expect(half.x).toBeCloseTo(2, 9);
    expect(half.z).toBeCloseTo(-1, 9);
  });
});

describe("따라가기와 발언자 카메라", () => {
  it("따라가기는 기본 켜짐이고, 켜져 있으면 논의 글이 와도 발언자 카메라가 끼어들지 않는다", () => {
    // 검증: V56 V14
    const cam = new SpeakerCamera();
    expect(cam.following()).toBe(true);
    cam.gather(true, T0 - 1_000);
    cam.talk("risk", T0);
    expect(cam.goal(T0)).toBeNull();
    expect(cam.goal(T0 + 5_000)).toBeNull();
    expect(cam.enabled()).toBe(true);
  });

  it("따라가기가 꺼지면 다음 논의 글부터 지금처럼 돈다", () => {
    // 검증: V56 V14
    const cam = new SpeakerCamera();
    cam.gather(true, T0 - 1_000);
    cam.setFollowing(false, T0);
    expect(cam.following()).toBe(false);
    cam.talk("news", T0 + 100);
    expect(cam.goal(T0 + 100)).toMatchObject({ shot: "speaker", agent: "news", since: T0 + 100 });
  });

  it("발언자 샷 중에 따라가기가 켜지면 곧바로 손을 떼고, 다시 꺼지면 다음 글에 돈다", () => {
    // 검증: V56 V14
    const cam = new SpeakerCamera();
    cam.gather(true, T0 - 1_000);
    cam.setFollowing(false, T0);
    cam.talk("risk", T0 + 100);
    cam.setFollowing(true, T0 + 500);
    expect(cam.goal(T0 + 500)).toBeNull();
    cam.talk("leader", T0 + 600);
    expect(cam.goal(T0 + 600)).toBeNull();
    cam.setFollowing(false, T0 + 700);
    cam.talk("trader", T0 + 800);
    expect(cam.goal(T0 + 800)).toMatchObject({ shot: "speaker", agent: "trader" });
  });
});
