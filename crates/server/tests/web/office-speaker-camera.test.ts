// 회의 발언자 카메라(server 23차 7번 (나), 25차 1번 · 결정 18로 고침): 미팅룸에 모여 있을 때 논의 글이 오면
// 발언자 샷 → 최소 머무름 2초 → 마지막 말 뒤 4초 유지 → 탁자 샷. 발언자 샷도 방위각 · 고도는 탁자 샷 그대로(돌지 않는다),
// 보는 중심만 발언자 쪽으로 절반 · 거리는 탁자 샷의 0.8배, 전환 1.2초(ease-in-out). 말이 이어지는 동안에는 탁자 샷을
// 거치지 않고 다음 발언자 쪽으로 옮긴다. 손으로 움직이면 그 논의 동안 꺼짐, 단추로 켜고 끔, 기본 켜짐.
// three.js 없이 상태 기계와 궤도 셈만 돌린다. 시각은 시험이 넘기는 밀리초다.
import { describe, expect, it } from "vitest";
import {
  HOLD_AFTER_TALK_MS,
  MIN_DWELL_MS,
  type Orbit,
  SPEAKER_CENTER_SHARE,
  SPEAKER_DISTANCE_SCALE,
  SPEAKER_ELEVATION_DEG,
  SPEAKER_TRANSITION_MS,
  SpeakerCamera,
  TABLE_ELEVATION_DEG,
  TABLE_TRANSITION_MS,
  easeInOut,
  speakerOrbit,
} from "../../web/src/office/speaker-camera";

const T0 = 1_000_000;

function gathered(now = T0 - 10_000): SpeakerCamera {
  const cam = new SpeakerCamera();
  // 32차 6번: 따라가기가 켜져 있으면 발언자 카메라가 끼어들지 않는다 — 발언자 카메라는 따라가기를 끈 뒤에 본다.
  cam.setFollowing(false, now);
  cam.gather(true, now);
  return cam;
}

describe("발언자 카메라", () => {
  it("기본 켜짐이고, 논의 글 전에는 목표가 없다(결정 18 수치)", () => {
    // 검증: V14
    // 결정 18: 발언자 샷의 고도는 탁자 샷 그대로, 전환 1.2초, 최소 머무름 2초, 말 뒤 4초 유지.
    expect(SPEAKER_ELEVATION_DEG).toBe(TABLE_ELEVATION_DEG);
    expect(SPEAKER_TRANSITION_MS).toBe(1_200);
    expect(TABLE_ELEVATION_DEG).toBe(30);
    expect(TABLE_TRANSITION_MS).toBe(1_200);
    expect(MIN_DWELL_MS).toBe(2_000);
    expect(HOLD_AFTER_TALK_MS).toBe(4_000);
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
      elevationDeg: TABLE_ELEVATION_DEG,
      transitionMs: 1_200,
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

  it("말이 하나뿐이면 말 뒤 4초 유지한 뒤 탁자 샷", () => {
    // 검증: V14
    const cam = gathered();
    cam.talk("risk", T0);
    expect(cam.goal(T0 + 3_999)).toMatchObject({ shot: "speaker", agent: "risk" });
    expect(cam.goal(T0 + 4_000)).toEqual({
      shot: "table",
      agent: null,
      elevationDeg: 30,
      transitionMs: 1_200,
      since: T0 + 4_000,
    });
  });

  it("마지막 말 뒤 4초 유지한 뒤 탁자 샷", () => {
    // 검증: V14
    const cam = gathered();
    cam.talk("risk", T0);
    cam.talk("risk", T0 + 1_000); // 같은 담당: 샷은 그대로(since 그대로)
    expect(cam.goal(T0 + 4_999)).toMatchObject({ shot: "speaker", agent: "risk", since: T0 });
    expect(cam.goal(T0 + 5_000)).toMatchObject({ shot: "table", agent: null, since: T0 + 5_000 });
  });

  it("기다리던 발언자로 넘어간 뒤에도 마지막 말 뒤 4초와 최소 머무름 2초가 다 지나야 탁자 샷", () => {
    // 검증: V14
    const cam = gathered();
    cam.talk("risk", T0);
    cam.talk("news", T0 + 500); // 2초 머문 뒤(T0 + 2초) news로 넘어간다
    expect(cam.goal(T0 + 4_499)).toMatchObject({ shot: "speaker", agent: "news", since: T0 + 2_000 });
    expect(cam.goal(T0 + 4_500)).toMatchObject({ shot: "table", agent: null, since: T0 + 4_500 });
  });

  it("말이 이어지는 동안에는 탁자 샷을 거치지 않고 다음 발언자 쪽으로 옮긴다", () => {
    // 검증: V14
    const cam = gathered();
    const order = ["risk", "news", "strategy", "leader"];
    const seen: string[] = [];
    for (let i = 0; i < order.length; i += 1) {
      const at = T0 + i * 3_000; // 말 사이 3초: 최소 머무름은 지났고 말 뒤 4초 유지 안이다
      for (let t = at - 3_000 + 50; i > 0 && t < at; t += 50) {
        const goal = cam.goal(t);
        expect(goal?.shot, `말 사이(${t - T0}ms)에 탁자 샷으로 돌아가지 않는다`).toBe("speaker");
      }
      cam.talk(order[i], at);
      const goal = cam.goal(at);
      expect(goal).toMatchObject({ shot: "speaker", agent: order[i], since: at });
      seen.push(goal?.agent ?? "");
    }
    expect(seen).toEqual(order);
    const last = T0 + (order.length - 1) * 3_000;
    expect(cam.goal(last + 3_999)).toMatchObject({ shot: "speaker", agent: "leader" });
    expect(cam.goal(last + 4_000)).toMatchObject({ shot: "table", since: last + 4_000 });
  });

  it("말 뒤 4초 안에 다른 담당이 말하면 탁자 샷이 끼지 않는다(옛 1.5초였다면 탁자 샷이 끼던 간격)", () => {
    // 검증: V14
    const cam = gathered();
    cam.talk("risk", T0);
    expect(cam.goal(T0 + 3_500)).toMatchObject({ shot: "speaker", agent: "risk", since: T0 });
    cam.talk("news", T0 + 3_500);
    expect(cam.goal(T0 + 3_500)).toMatchObject({ shot: "speaker", agent: "news", since: T0 + 3_500 });
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

describe("발언자 샷 궤도(결정 18)", () => {
  const table: Orbit = { radius: 20, polar: 1.0, theta: 0.3, tx: 10, ty: 0.5, tz: 8 };

  it("중심 절반 · 거리 0.8배 상수", () => {
    // 검증: V14
    expect(SPEAKER_CENTER_SHARE).toBe(0.5);
    expect(SPEAKER_DISTANCE_SCALE).toBe(0.8);
  });

  it("방위각 · 고도는 탁자 샷 그대로이고, 중심은 발언자 쪽으로 절반, 거리는 0.8배", () => {
    // 검증: V14
    const orbit = speakerOrbit(table, { x: 14, z: 6 });
    expect(orbit.theta).toBe(table.theta);
    expect(orbit.polar).toBe(table.polar);
    expect(orbit.ty).toBe(table.ty);
    expect(orbit.radius).toBeCloseTo(16);
    expect(orbit.tx).toBeCloseTo(12);
    expect(orbit.tz).toBeCloseTo(7);
  });

  it("발언자가 어디에 있어도 돌지 않는다(방위각 · 고도가 같다)", () => {
    // 검증: V14
    for (const spot of [
      { x: 0, z: 0 },
      { x: 20, z: 8 },
      { x: 10, z: 20 },
      { x: -5, z: 3 },
    ]) {
      const orbit = speakerOrbit(table, spot);
      expect(orbit.theta).toBe(table.theta);
      expect(orbit.polar).toBe(table.polar);
      expect(orbit.radius).toBeCloseTo(table.radius * 0.8);
      expect(orbit.tx).toBeCloseTo((table.tx + spot.x) / 2);
      expect(orbit.tz).toBeCloseTo((table.tz + spot.z) / 2);
    }
  });

  it("발언자가 탁자 가운데에 있으면 중심은 그대로이고 거리만 0.8배", () => {
    // 검증: V14
    const orbit = speakerOrbit(table, { x: table.tx, z: table.tz });
    expect(orbit).toEqual({ ...table, radius: table.radius * 0.8 });
  });

  it("넘긴 탁자 샷 궤도를 바꾸지 않는다", () => {
    // 검증: V14
    const before = { ...table };
    speakerOrbit(table, { x: 3, z: 4 });
    expect(table).toEqual(before);
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
