// 승인 알림음(server 34차 2번, additions.md N12 · E49 · E50 · V62 — 원본 동작: docs/research/agent-office-refs.md 1-B 알림음):
// 새 요청에 한 번 · 같은 요청 두 번 아님 · 다시 읽어 채운 것에는 안 냄 · 설정 꺼짐이면 안 냄 · 소리가 막혀도 던지지 않음.
// 가짜 오디오(Web Audio 모양)를 넣어 `src/office/chime.ts`를 돌려 본다. 소리 파일은 없다.
import { describe, expect, it, vi } from "vitest";
import {
  CHIME_NOTES,
  CHIME_NOTE_SECONDS,
  CHIME_VOLUME,
  Chime,
  type ToneContext,
  type ToneGain,
  type ToneOscillator,
  type ToneParam,
} from "../../web/src/office/chime";

const N = "3f9a0c1e5b7d2468ace0f1b3d5c7e9a1";
const M = "b7c1d2e3f4a5b6c7d8e9f0a1b2c3d4e5";
const NOW = 10;

interface FakeParam extends ToneParam {
  /** 넣은 값 모두(`value =` · setValueAtTime · 램프). */
  values: number[];
}

function param(initial: number): FakeParam {
  let current = initial;
  const values: number[] = [];
  return {
    values,
    get value() {
      return current;
    },
    set value(v: number) {
      current = v;
      values.push(v);
    },
    setValueAtTime(v: number) {
      current = v;
      values.push(v);
    },
    linearRampToValueAtTime(v: number) {
      values.push(v);
    },
    exponentialRampToValueAtTime(v: number) {
      values.push(v);
    },
  };
}

interface FakeOsc extends ToneOscillator {
  frequency: FakeParam;
  starts: number[];
  stops: number[];
}

/** 부른 것을 적는 가짜 오디오. `state`가 "suspended"면 `resume`이 `resume()`대로 답한다. */
function fakeAudio(state = "running", resume: () => Promise<void> = async () => undefined) {
  const oscillators: FakeOsc[] = [];
  const gains: (ToneGain & { gain: FakeParam })[] = [];
  const context = {
    currentTime: NOW,
    state,
    destination: { name: "speaker" },
    resume: vi.fn(async () => {
      await resume();
      context.state = "running";
    }),
    createOscillator(): ToneOscillator {
      const osc: FakeOsc = {
        type: "",
        frequency: param(440),
        starts: [],
        stops: [],
        connect: () => undefined,
        start(when?: number) {
          osc.starts.push(when ?? context.currentTime);
        },
        stop(when?: number) {
          osc.stops.push(when ?? context.currentTime);
        },
      };
      oscillators.push(osc);
      return osc;
    },
    createGain(): ToneGain {
      const gain = { gain: param(1), connect: () => undefined };
      gains.push(gain);
      return gain;
    },
  };
  const make = vi.fn(() => context as ToneContext);
  return { context, make, oscillators, gains };
}

const on = async () => true;
const off = async () => false;

/** 그 오실레이터의 높이(Hz): 넣은 첫 값(없으면 지금 값). */
function hzOf(osc: FakeOsc): number {
  return osc.frequency.values[0] ?? osc.frequency.value;
}

describe("승인 알림음 — 소리 모양", () => {
  it("사인파 A5(880Hz) → 0.12초 뒤 E5(659.25Hz), 음마다 0.15초, 음량 0.12", async () => {
    // 검증: N12 V62
    expect(CHIME_NOTES).toEqual([
      { hz: 880, at: 0 },
      { hz: 659.25, at: 0.12 },
    ]);
    expect(CHIME_NOTE_SECONDS).toBe(0.15);
    expect(CHIME_VOLUME).toBe(0.12);
    const audio = fakeAudio();
    const chime = new Chime(audio.make, on);
    await expect(chime.ring(N)).resolves.toBe(true);
    expect(audio.oscillators.map((o) => o.type)).toEqual(["sine", "sine"]);
    expect(audio.oscillators.map(hzOf)).toEqual([880, 659.25]);
    expect(audio.oscillators[0].starts[0]).toBeCloseTo(NOW, 6);
    expect(audio.oscillators[1].starts[0]).toBeCloseTo(NOW + 0.12, 6);
    for (const osc of audio.oscillators) {
      expect(osc.stops[0] - osc.starts[0]).toBeCloseTo(CHIME_NOTE_SECONDS, 6);
    }
    const loudest = Math.max(...audio.gains.flatMap((g) => g.gain.values));
    expect(loudest).toBeCloseTo(CHIME_VOLUME, 6);
  });
});

describe("승인 알림음 — 언제 내나", () => {
  it("새 요청에 한 번 내고 같은 요청에는 두 번 내지 않는다", async () => {
    // 검증: V62 E49
    const audio = fakeAudio();
    const chime = new Chime(audio.make, on);
    await expect(chime.ring(N)).resolves.toBe(true);
    await expect(chime.ring(N)).resolves.toBe(false);
    expect(audio.oscillators.length).toBe(CHIME_NOTES.length);
    await expect(chime.ring(M)).resolves.toBe(true);
    expect(audio.oscillators.length).toBe(CHIME_NOTES.length * 2);
    expect(audio.make).toHaveBeenCalledTimes(1);
  });

  it("다시 읽어 채운 요청에는 내지 않는다", async () => {
    // 검증: V62 E49
    const audio = fakeAudio();
    const chime = new Chime(audio.make, on);
    chime.quiet([N]);
    await expect(chime.ring(N)).resolves.toBe(false);
    expect(audio.oscillators).toEqual([]);
    await expect(chime.ring(M)).resolves.toBe(true);
  });

  it("알림음 설정이 꺼져 있으면 내지 않고 오디오도 만들지 않으며, 켜면 다음 새 요청부터 낸다", async () => {
    // 검증: V62 E50
    const audio = fakeAudio();
    let enabled = false;
    const chime = new Chime(audio.make, async () => enabled);
    await expect(chime.ring(N)).resolves.toBe(false);
    expect(audio.make).not.toHaveBeenCalled();
    expect(audio.oscillators).toEqual([]);
    enabled = true;
    await expect(chime.ring(M)).resolves.toBe(true);
    const silent = fakeAudio();
    await expect(new Chime(silent.make, off).ring(N)).resolves.toBe(false);
    expect(silent.make).not.toHaveBeenCalled();
  });
});

describe("승인 알림음 — 소리가 막힘", () => {
  it("브라우저가 소리를 막으면(resume 거부) 던지지 않고 소리 없이 넘어간다", async () => {
    // 검증: E50 V62
    const audio = fakeAudio("suspended", async () => {
      throw new Error("NotAllowedError");
    });
    const chime = new Chime(audio.make, on);
    await expect(chime.ring(N)).resolves.toBe(false);
    expect(audio.oscillators).toEqual([]);
    await expect(chime.unlock()).resolves.toBeUndefined();
  });

  it("오디오를 만들 수 없어도 던지지 않는다", async () => {
    // 검증: E50
    const make = vi.fn((): ToneContext => {
      throw new Error("AudioContext 없음");
    });
    const chime = new Chime(make, on);
    await expect(chime.ring(N)).resolves.toBe(false);
    await expect(chime.unlock()).resolves.toBeUndefined();
  });

  it("화면을 누르면(unlock) 막힌 오디오를 풀고 그 뒤 새 요청부터 소리를 낸다", async () => {
    // 검증: E50 V62
    const audio = fakeAudio("suspended");
    const chime = new Chime(audio.make, on);
    await chime.unlock();
    expect(audio.context.resume).toHaveBeenCalled();
    expect(audio.context.state).toBe("running");
    await expect(chime.ring(N)).resolves.toBe(true);
    expect(audio.oscillators.length).toBe(CHIME_NOTES.length);
    expect(audio.make).toHaveBeenCalledTimes(1);
  });
});
