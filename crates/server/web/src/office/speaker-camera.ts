// 회의 발언자 카메라(server 23차 7번 (나), 결정 16 덧붙임 · 원본 DeskRPG `meeting-camera` 수치 —
// docs/research/deskrpg.md 11절). three.js 없이 시험할 수 있게 "지금 누구를 · 어떤 샷으로" 보는지만 내는 상태 기계로
// 둔다. 카메라를 실제로 옮기는 일(상반신 맞추기 · 보간)은 장면이 한다. 시각은 부르는 쪽이 넘기는 밀리초(`now`)다.

/** 발언자 샷의 고도(도). */
export const SPEAKER_ELEVATION_DEG = 15;
/** 탁자 샷의 고도(도). */
export const TABLE_ELEVATION_DEG = 30;
/** 발언자 샷으로 가는 전환 시간(밀리초, ease-in-out). */
export const SPEAKER_TRANSITION_MS = 900;
/** 탁자 샷으로 가는 전환 시간(밀리초, ease-in-out). */
export const TABLE_TRANSITION_MS = 1_000;
/** 한 발언자 샷의 최소 머무름(밀리초). */
export const MIN_DWELL_MS = 2_000;
/** 마지막 말 뒤 발언자 샷을 유지하는 시간(밀리초). */
export const HOLD_AFTER_TALK_MS = 1_500;

/** (25차 1번, 결정 18) 발언자 샷의 보는 중심: 탁자 샷의 중심에서 발언자 쪽으로 이 몫만 옮긴다. */
export const SPEAKER_CENTER_SHARE = 0.5;
/** (25차 1번, 결정 18) 발언자 샷의 거리: 탁자 샷 거리의 이 배수. */
export const SPEAKER_DISTANCE_SCALE = 0.8;

/** 카메라 궤도 값: 보는 점(tx, ty, tz)에서 거리 · 연직에서 잰 각(polar) · 둘레 각(theta, 방위각). */
export interface Orbit {
  radius: number;
  polar: number;
  theta: number;
  tx: number;
  ty: number;
  tz: number;
}

/** 발언자가 선 자리(바닥 좌표). */
export interface Spot {
  x: number;
  z: number;
}

/**
 * (25차 1번, 결정 18) 탁자 샷 궤도에서 발언자 샷 궤도를 낸다. 돌지 않는다: 방위각(theta) · 고도(polar)는
 * 탁자 샷 그대로다. 보는 중심의 바닥 좌표(tx, tz)만 발언자 자리 쪽으로 [`SPEAKER_CENTER_SHARE`]만큼 옮기고
 * (높이 ty는 그대로), 거리는 탁자 샷의 [`SPEAKER_DISTANCE_SCALE`]배다. 넘긴 값은 바꾸지 않는다.
 */
export function speakerOrbit(table: Orbit, spot: Spot): Orbit {
  void table;
  void spot;
  throw new Error("todo");
}

/** 자동 카메라의 목표. */
export interface CameraGoal {
  shot: "speaker" | "table";
  /** 발언자 샷이면 그 담당 id, 탁자 샷이면 `null`. */
  agent: string | null;
  elevationDeg: number;
  transitionMs: number;
  /** 이 샷이 시작된 시각(밀리초). 전환은 여기서 `transitionMs` 동안이다. */
  since: number;
}

/**
 * 발언자 카메라 상태 기계. 기본 켜짐.
 * - 담당들이 미팅룸에 모여 있을 때(`gather(true)`) 논의 글이 오면(`talk`) 그 담당의 발언자 샷(고도 15°, 0.9초).
 * - 다른 담당이 말해도 지금 샷이 시작된 지 2초가 지나기 전에는 바꾸지 않는다(2초가 되는 때 바꾼다).
 * - 마지막 말 뒤 1.5초(그리고 그 샷의 최소 머무름 2초)가 지나면 탁자 샷(고도 30°, 1.0초).
 * - 사용자가 카메라를 손으로 움직이면(`manual`) 그 논의 동안(`gather(false)`가 올 때까지) 꺼진다.
 * - 단추(`setEnabled`)로 켜고 끈다. 꺼져 있으면 목표를 내지 않는다.
 * - 목표가 없으면(논의 글 전 · 모이지 않음 · 꺼짐) [`goal`]은 `null`이고 장면은 평소 카메라를 쓴다.
 */
export class SpeakerCamera {
  private on = true;
  private gathered = false;
  /** 사용자가 손으로 움직여 이 논의 동안 꺼졌다. */
  private handOff = false;
  /** 지금 샷(없으면 `null`). */
  private shot: { kind: "speaker" | "table"; agent: string | null; since: number } | null = null;
  /** 최소 머무름이 끝나면 넘어갈 다음 발언자. */
  private next: string | null = null;
  private lastTalkAt = 0;

  /** 자동 카메라 단추가 켜져 있는가. */
  enabled(): boolean {
    return this.on;
  }

  /** 단추: 켜고 끈다. */
  setEnabled(on: boolean, now: number): void {
    void now;
    this.on = on;
    if (!on) this.reset();
  }

  /** 담당들이 미팅룸에 모여 있는가. `false`면 논의가 끝난 것이다(손 조작으로 꺼진 것도 풀린다). */
  gather(gathered: boolean, now: number): void {
    void now;
    if (gathered === this.gathered) return;
    this.gathered = gathered;
    if (!gathered) {
      this.handOff = false;
      this.reset();
    }
  }

  /** 논의 글이 왔다. */
  talk(agent: string, now: number): void {
    if (!this.active()) return;
    this.advance(now);
    this.lastTalkAt = now;
    const shot = this.shot;
    if (!shot || shot.kind === "table") {
      this.shot = { kind: "speaker", agent, since: now };
      this.next = null;
    } else if (shot.agent === agent) {
      this.next = null;
    } else if (now >= shot.since + MIN_DWELL_MS) {
      this.shot = { kind: "speaker", agent, since: now };
      this.next = null;
    } else {
      this.next = agent;
    }
  }

  /** 사용자가 카메라를 손으로 움직였다. */
  manual(now: number): void {
    void now;
    if (!this.gathered) return;
    this.handOff = true;
    this.reset();
  }

  /** `now`의 목표(없으면 `null`). */
  goal(now: number): CameraGoal | null {
    if (!this.active()) return null;
    this.advance(now);
    const shot = this.shot;
    if (!shot) return null;
    return shot.kind === "speaker"
      ? { shot: "speaker", agent: shot.agent, elevationDeg: SPEAKER_ELEVATION_DEG, transitionMs: SPEAKER_TRANSITION_MS, since: shot.since }
      : { shot: "table", agent: null, elevationDeg: TABLE_ELEVATION_DEG, transitionMs: TABLE_TRANSITION_MS, since: shot.since };
  }

  private active(): boolean {
    return this.on && this.gathered && !this.handOff;
  }

  private reset(): void {
    this.shot = null;
    this.next = null;
  }

  /** `now`까지 시간이 흐른 만큼 샷을 넘긴다: 기다리던 발언자(최소 머무름 뒤) → 탁자 샷(말 뒤 1.5초 · 최소 머무름 뒤). */
  private advance(now: number): void {
    const shot = this.shot;
    if (!shot || shot.kind !== "speaker") return;
    if (this.next !== null) {
      const at = shot.since + MIN_DWELL_MS;
      if (now < at) return;
      this.shot = { kind: "speaker", agent: this.next, since: at };
      this.next = null;
      this.advance(now);
      return;
    }
    const tableAt = Math.max(this.lastTalkAt + HOLD_AFTER_TALK_MS, shot.since + MIN_DWELL_MS);
    if (now >= tableAt) this.shot = { kind: "table", agent: null, since: tableAt };
  }
}

/** ease-in-out 보간(0 → 0, 1 → 1, 가운데 0.5, 양 끝이 느리다). 0~1 밖은 끝값으로 자른다. */
export function easeInOut(t: number): number {
  const x = Math.max(0, Math.min(1, t));
  return x < 0.5 ? 2 * x * x : 1 - (-2 * x + 2) ** 2 / 2;
}
