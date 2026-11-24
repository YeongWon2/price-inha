// 회의 발언자 카메라(server 23차 7번 (나), 결정 16 덧붙임 · 원본 DeskRPG `meeting-camera` 수치 —
// docs/research/deskrpg.md 11절): 미팅룸에 모여 있을 때 논의 글이 오면 발언자 샷(15°, 0.9초) → 최소 머무름 2초 →
// 마지막 말 뒤 1.5초 유지 → 탁자 샷(30°, 1.0초). 손으로 움직이면 그 논의 동안 꺼짐, 단추로 켜고 끔, 기본 켜짐.
// three.js 없이 상태 기계만 돌린다. 시각은 시험이 넘기는 밀리초다.
import { describe, expect, it } from "vitest";
import {
  HOLD_AFTER_TALK_MS,
  MIN_DWELL_MS,
  SPEAKER_ELEVATION_DEG,
  SPEAKER_TRANSITION_MS,
  SpeakerCamera,
  TABLE_ELEVATION_DEG,
  TABLE_TRANSITION_MS,
  easeInOut,
} from "../../web/src/office/speaker-camera";

const T0 = 1_000_000;

function gathered(now = T0 - 10_000): SpeakerCamera {
  const cam = new SpeakerCamera();
  cam.gather(true, now);
  return cam;
}

describe("발언자 카메라", () => {
  it("기본 켜짐이고, 논의 글 전에는 목표가 없다(원본 수치)", () => {
    // 검증: V14
    // 원본 수치: 발언자 15° · 0.9초, 탁자 30° · 1.0초, 최소 머무름 2초, 말 뒤 1.5초 유지.
    expect(SPEAKER_ELEVATION_DEG).toBe(15);
    expect(SPEAKER_TRANSITION_MS).toBe(900);
    expect(TABLE_ELEVATION_DEG).toBe(30);
    expect(TABLE_TRANSITION_MS).toBe(1_000);
    expect(MIN_DWELL_MS).toBe(2_000);
    expect(HOLD_AFTER_TALK_MS).toBe(1_500);
    const cam = gathered();
    expect(cam.enabled()).toBe(true);
    expect(cam.goal(T0)).toBeNull();
  });

  it("모여 있을 때 논의 글이 오면 말한 담당의 발언자 샷", () => {
    // 검증: V14
    const cam = gathered();
    cam.talk("risk", T0);
    expect(cam.goal(T0)).toEqual({
      shot: "speaker",
      agent: "risk",
      elevationDeg: 15,
      transitionMs: 900,
      since: T0,
    });
  });

  it("미팅룸에 모여 있지 않으면 논의 글이 와도 움직이지 않는다", () => {
    // 검증: V14
    const cam = new SpeakerCamera();
    cam.talk("risk", T0);
    expect(cam.goal(T0 + 100)).toBeNull();
    cam.gather(true, T0 + 200);
    cam.gather(false, T0 + 300);
    cam.talk("news", T0 + 400);
    expect(cam.goal(T0 + 500)).toBeNull();
  });

  it("다른 담당이 말해도 지금 샷이 2초 머문 뒤에 바꾼다", () => {
    // 검증: V14
    const cam = gathered();
    cam.talk("risk", T0);
    cam.talk("news", T0 + 500);
    expect(cam.goal(T0 + 1_999)).toMatchObject({ shot: "speaker", agent: "risk", since: T0 });
    expect(cam.goal(T0 + 2_000)).toMatchObject({ shot: "speaker", agent: "news", since: T0 + 2_000 });
  });

  it("2초가 지난 뒤 다른 담당이 말하면 곧바로 바꾼다", () => {
    // 검증: V14
    const cam = gathered();
    cam.talk("risk", T0);
    cam.talk("risk", T0 + 1_200);
    cam.talk("news", T0 + 2_500);
    expect(cam.goal(T0 + 2_500)).toMatchObject({ shot: "speaker", agent: "news", since: T0 + 2_500 });
  });

  it("말이 하나뿐이면 최소 머무름 2초 뒤 탁자 샷", () => {
    // 검증: V14
    const cam = gathered();
    cam.talk("risk", T0);
    expect(cam.goal(T0 + 1_999)).toMatchObject({ shot: "speaker", agent: "risk" });
    expect(cam.goal(T0 + 2_000)).toEqual({
      shot: "table",
      agent: null,
      elevationDeg: 30,
      transitionMs: 1_000,
      since: T0 + 2_000,
    });
  });

  it("마지막 말 뒤 1.5초 유지한 뒤 탁자 샷", () => {
    // 검증: V14
    const cam = gathered();
    cam.talk("risk", T0);
    cam.talk("risk", T0 + 1_000); // 같은 담당: 샷은 그대로(since 그대로)
    expect(cam.goal(T0 + 2_499)).toMatchObject({ shot: "speaker", agent: "risk", since: T0 });
    expect(cam.goal(T0 + 2_500)).toMatchObject({ shot: "table", agent: null, since: T0 + 2_500 });
  });

  it("탁자 샷 뒤 다시 말하면 그 담당에게 다가간다", () => {
    // 검증: V14
    const cam = gathered();
    cam.talk("risk", T0);
    expect(cam.goal(T0 + 5_000)).toMatchObject({ shot: "table" });
    cam.talk("leader", T0 + 8_000);
    expect(cam.goal(T0 + 8_000)).toMatchObject({ shot: "speaker", agent: "leader", since: T0 + 8_000 });
  });

  it("사용자가 손으로 움직이면 그 논의 동안 꺼지고, 다음 논의에는 다시 켜진다", () => {
    // 검증: V14
    const cam = gathered();
    cam.talk("risk", T0);
    cam.manual(T0 + 500);
    expect(cam.goal(T0 + 500)).toBeNull();
    cam.talk("news", T0 + 3_000);
    expect(cam.goal(T0 + 3_000)).toBeNull();
    expect(cam.enabled()).toBe(true);
    cam.gather(false, T0 + 10_000);
    cam.gather(true, T0 + 20_000);
    cam.talk("leader", T0 + 21_000);
    expect(cam.goal(T0 + 21_000)).toMatchObject({ shot: "speaker", agent: "leader" });
  });

  it("단추로 끄면 목표를 내지 않고, 다시 켜면 다음 논의 글에 움직인다", () => {
    // 검증: V14
    const cam = gathered();
    cam.setEnabled(false, T0);
    expect(cam.enabled()).toBe(false);
    cam.talk("risk", T0 + 100);
    expect(cam.goal(T0 + 100)).toBeNull();
    cam.setEnabled(true, T0 + 5_000);
    expect(cam.enabled()).toBe(true);
    cam.talk("news", T0 + 6_000);
    expect(cam.goal(T0 + 6_000)).toMatchObject({ shot: "speaker", agent: "news" });
  });

  it("말하는 중에 끄면 곧바로 손을 뗀다", () => {
    // 검증: V14
    const cam = gathered();
    cam.talk("risk", T0);
    cam.setEnabled(false, T0 + 300);
    expect(cam.goal(T0 + 300)).toBeNull();
  });
});

describe("ease-in-out", () => {
  it("0 → 0, 1 → 1, 가운데 0.5, 양 끝이 느리고 대칭이다", () => {
    // 검증: V14
    expect(easeInOut(0)).toBe(0);
    expect(easeInOut(1)).toBe(1);
    expect(easeInOut(0.5)).toBeCloseTo(0.5);
    expect(easeInOut(0.1)).toBeLessThan(0.1);
    expect(easeInOut(0.9)).toBeGreaterThan(0.9);
    for (const t of [0.1, 0.25, 0.4]) expect(easeInOut(t) + easeInOut(1 - t)).toBeCloseTo(1);
    let prev = -1;
    for (let i = 0; i <= 20; i += 1) {
      const v = easeInOut(i / 20);
      expect(v).toBeGreaterThanOrEqual(prev);
      prev = v;
    }
  });

  it("0~1 밖은 끝값으로 자른다", () => {
    // 검증: V14
    expect(easeInOut(-0.5)).toBe(0);
    expect(easeInOut(1.5)).toBe(1);
  });
});
