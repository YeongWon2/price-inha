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
export const CHIME_NOTES: readonly ChimeNote[] = [
  { hz: 880, at: 0 },
  { hz: 659.25, at: 0.12 },
];
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

/** 음이 사라질 때까지 줄이는 음량(지수 램프는 0으로 갈 수 없다). */
const CHIME_FLOOR = 0.0001;
/** 음 시작의 짧은 올림(초) — 딸깍 소리를 막는다. */
const CHIME_ATTACK = 0.01;

/** 오디오 만들기(던질 수 있다 — 그러면 소리 없이 넘어간다). */
export type MakeAudio = () => ToneContext;

/** 승인 알림음. 오디오는 처음 필요할 때 한 번 만든다. */
export class Chime {
  private readonly makeAudio: MakeAudio;
  private readonly enabled: () => Promise<boolean>;
  private audio: ToneContext | null = null;
  /** 낸 번호 · 다시 읽어 채운 번호(다시 내지 않는다). */
  private readonly heard = new Set<string>();

  /** `makeAudio`: 오디오 만들기. `enabled`: 알림음 설정을 읽는다(화면은 `soundEnabled`). */
  constructor(makeAudio: MakeAudio, enabled: () => Promise<boolean>) {
    this.makeAudio = makeAudio;
    this.enabled = enabled;
  }

  /** 오디오(처음이면 만든다). 만들 수 없으면 `null`. */
  private context(): ToneContext | null {
    if (this.audio) return this.audio;
    try {
      this.audio = this.makeAudio();
    } catch {
      this.audio = null;
    }
    return this.audio;
  }

  /** 설정을 읽는다(읽지 못하면 끈 것으로 본다). */
  private async allowed(): Promise<boolean> {
    try {
      return (await this.enabled()) === true;
    } catch {
      return false;
    }
  }

  /** 오디오가 돌고 있게 한다(막혀 있으면 `resume`). 돌면 `true`. */
  private async running(audio: ToneContext): Promise<boolean> {
    if (audio.state === "running") return true;
    try {
      await audio.resume();
    } catch {
      return false;
    }
    return audio.state === "running";
  }

  /** 화면을 눌렀다(사용자 손짓): 막힌 오디오를 푼다(`resume`). 실패해도 던지지 않는다. */
  async unlock(): Promise<void> {
    if (!(await this.allowed())) return;
    const audio = this.context();
    if (!audio) return;
    await this.running(audio);
  }

  /** 다시 읽어 채운 번호들: 소리 없이 "이미 알린 것"으로 적는다. */
  quiet(numbers: Iterable<string>): void {
    for (const number of numbers) this.heard.add(number);
  }

  /**
   * 새 승인 요청 `number`에 알림음을 낸다. 낸 번호 · 채운 번호면 내지 않는다. 설정이 꺼져 있거나 소리가 막히면 내지
   * 않는다. 소리를 냈으면 `true`, 아니면 `false`. 던지지 않는다.
   */
  async ring(number: string): Promise<boolean> {
    if (this.heard.has(number)) return false;
    // 한 번 본 요청은 소리가 났든 안 났든 다시 "새 요청"이 아니다.
    this.heard.add(number);
    if (!(await this.allowed())) return false;
    const audio = this.context();
    if (!audio || !(await this.running(audio))) return false;
    try {
      this.play(audio);
      return true;
    } catch {
      return false;
    }
  }

  /** 사인파 두 음을 낸다(A5 → E5, 음마다 [`CHIME_NOTE_SECONDS`], 음량 [`CHIME_VOLUME`]). */
  private play(audio: ToneContext): void {
    const t0 = audio.currentTime;
    for (const note of CHIME_NOTES) {
      const start = t0 + note.at;
      const end = start + CHIME_NOTE_SECONDS;
      const osc = audio.createOscillator();
      osc.type = "sine";
      osc.frequency.setValueAtTime(note.hz, start);
      const gain = audio.createGain();
      gain.gain.setValueAtTime(CHIME_FLOOR, start);
      gain.gain.linearRampToValueAtTime(CHIME_VOLUME, start + CHIME_ATTACK);
      gain.gain.exponentialRampToValueAtTime(CHIME_FLOOR, end);
      osc.connect(gain);
      gain.connect(audio.destination);
      osc.start(start);
      osc.stop(end);
    }
  }
}
