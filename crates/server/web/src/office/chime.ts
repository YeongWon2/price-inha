// 승인 알림음(server 34차 2번, additions.md N12 · E49 · E50 · V62 — 원본 동작: docs/research/agent-office-refs.md
// 1-B 알림음 · F 추가 후보, pixel-agents의 권한 알림음): 새 승인 요청에 짧은 알림음을 한 번 낸다. 소리 파일 없이
// Web Audio 사인파로 만든다(A5 880Hz → 0.12초 뒤 E5 659.25Hz, 음마다 0.15초, 음량 0.12).
//
// - 같은 승인 번호에는 두 번 내지 않는다. 다시 읽어 채운 번호([`Chime.quiet`])에는 내지 않는다(E49).
// - 알림음 설정(ui/sound-pref.ts `soundEnabled`)이 꺼져 있으면 내지 않는다.
// - 브라우저가 소리를 막으면(사용자 손짓 전, 오디오를 만들 수 없음, `resume` 거부) 조용히 넘어가고 던지지 않는다.
//   화면을 누르면 [`Chime.unlock`]으로 풀어 그 뒤부터 소리를 낸다(E50).
// - 시험이 가짜 오디오를 넣을 수 있게 오디오 만들기를 인자로 받는다(화면은 `() => new AudioContext()`).

/** 알림음의 한 음: 높이(Hz) · 시작(초, 첫 음 기준). */
export interface ChimeNote {
  hz: number;
  at: number;
}

/** 알림음의 음들(원본 권한 알림음: A5 → E5). */
export const CHIME_NOTES: readonly ChimeNote[] = [];
/** 음마다 길이(초). */
export const CHIME_NOTE_SECONDS = 0.15;
/** 음량(0~1). */
export const CHIME_VOLUME = 0.12;

/** 쓰는 만큼의 Web Audio 모양(`AudioParam`). */
export interface ToneParam {
  value: number;
  setValueAtTime(value: number, time: number): unknown;
  linearRampToValueAtTime(value: number, time: number): unknown;
  exponentialRampToValueAtTime(value: number, time: number): unknown;
}

/** 쓰는 만큼의 `OscillatorNode`. */
export interface ToneOscillator {
  type: string;
  frequency: ToneParam;
  connect(destination: unknown): unknown;
  start(when?: number): void;
  stop(when?: number): void;
}

/** 쓰는 만큼의 `GainNode`. */
export interface ToneGain {
  gain: ToneParam;
  connect(destination: unknown): unknown;
}

/** 쓰는 만큼의 `AudioContext`. */
export interface ToneContext {
  readonly currentTime: number;
  readonly state: string;
  readonly destination: unknown;
  resume(): Promise<void>;
  createOscillator(): ToneOscillator;
  createGain(): ToneGain;
}

/** 오디오 만들기(던질 수 있다 — 그러면 소리 없이 넘어간다). */
export type MakeAudio = () => ToneContext;

/** 승인 알림음. 오디오는 처음 필요할 때 한 번 만든다. */
export class Chime {
  /** `makeAudio`: 오디오 만들기. `enabled`: 알림음 설정을 읽는다(화면은 `soundEnabled`). */
  constructor(makeAudio: MakeAudio, enabled: () => Promise<boolean>) {
    void makeAudio;
    void enabled;
  }

  /** 화면을 눌렀다(사용자 손짓): 막힌 오디오를 푼다(`resume`). 실패해도 던지지 않는다. */
  async unlock(): Promise<void> {
    throw new Error("todo");
  }

  /** 다시 읽어 채운 번호들: 소리 없이 "이미 알린 것"으로 적는다. */
  quiet(numbers: Iterable<string>): void {
    void numbers;
    throw new Error("todo");
  }

  /**
   * 새 승인 요청 `number`에 알림음을 낸다. 낸 번호 · 채운 번호면 내지 않는다. 설정이 꺼져 있거나 소리가 막히면 내지
   * 않는다. 소리를 냈으면 `true`, 아니면 `false`. 던지지 않는다.
   */
  async ring(number: string): Promise<boolean> {
    void number;
    throw new Error("todo");
  }
}
