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
  /** 자동 카메라 단추가 켜져 있는가. */
  enabled(): boolean {
    throw new Error("todo");
  }

  /** 단추: 켜고 끈다. */
  setEnabled(on: boolean, now: number): void {
    void on;
    void now;
    throw new Error("todo");
  }

  /** 담당들이 미팅룸에 모여 있는가. `false`면 논의가 끝난 것이다(손 조작으로 꺼진 것도 풀린다). */
  gather(gathered: boolean, now: number): void {
    void gathered;
    void now;
    throw new Error("todo");
  }

  /** 논의 글이 왔다. */
  talk(agent: string, now: number): void {
    void agent;
    void now;
    throw new Error("todo");
  }

  /** 사용자가 카메라를 손으로 움직였다. */
  manual(now: number): void {
    void now;
    throw new Error("todo");
  }

  /** `now`의 목표(없으면 `null`). */
  goal(now: number): CameraGoal | null {
    void now;
    throw new Error("todo");
  }
}

/** ease-in-out 보간(0 → 0, 1 → 1, 가운데 0.5, 양 끝이 느리다). 0~1 밖은 끝값으로 자른다. */
export function easeInOut(t: number): number {
  void t;
  throw new Error("todo");
}
