// 알림음 켜기 · 끄기(server 31차 1번 (바) · V78): 이 기기의 `localStorage` 키 하나에 남고, 기본은 켜짐이며,
// 저장소를 쓸 수 없는 브라우저에서도 던지지 않는다(읽기는 켜짐, 쓰기는 `false`).
import { afterEach, describe, expect, it, vi } from "vitest";
import { SOUND_OFF, SOUND_ON, SOUND_PREF_KEY, setSoundEnabled, soundEnabled } from "../../web/src/ui/sound-pref";

/** 무엇을 해도 던지는 저장소(막힘 · 꽉 참). */
const brokenStorage = {
  getItem(): string | null {
    throw new DOMException("막힘", "SecurityError");
  },
  setItem(): void {
    throw new DOMException("꽉 참", "QuotaExceededError");
  },
  removeItem(): void {
    throw new DOMException("막힘", "SecurityError");
  },
  clear(): void {},
  key(): string | null {
    return null;
  },
  length: 0,
};

afterEach(() => {
  vi.unstubAllGlobals();
  try {
    localStorage.clear();
  } catch {
    // 저장소를 바꿔 둔 시험 뒤에는 되돌린 뒤라 여기 오지 않는다.
  }
});

describe("알림음 설정(기기에만 남음)", () => {
  it("남긴 값이 없으면 켜짐이다", () => {
    // 검증: V78 R18-4
    localStorage.clear();
    expect(soundEnabled()).toBe(true);
  });

  it("끄면 키 하나에 꺼짐이 남고, 다시 읽어도 꺼짐이며, 켜면 켜짐이 남는다", () => {
    // 검증: V78 R18-4
    expect(setSoundEnabled(false)).toBe(true);
    expect(localStorage.getItem(SOUND_PREF_KEY)).toBe(SOUND_OFF);
    expect(soundEnabled()).toBe(false);
    expect(setSoundEnabled(true)).toBe(true);
    expect(localStorage.getItem(SOUND_PREF_KEY)).toBe(SOUND_ON);
    expect(soundEnabled()).toBe(true);
    expect(localStorage.length, "키는 하나뿐이다").toBe(1);
  });

  it("알 수 없는 값이 남아 있으면 켜짐으로 본다", () => {
    // 검증: V78
    localStorage.setItem(SOUND_PREF_KEY, "엉뚱한 값");
    expect(soundEnabled()).toBe(true);
  });

  it("저장소가 던지면 읽기는 켜짐, 쓰기는 false이고 던지지 않는다", () => {
    // 검증: V78 E57
    vi.stubGlobal("localStorage", brokenStorage);
    expect(soundEnabled()).toBe(true);
    expect(setSoundEnabled(false)).toBe(false);
  });

  it("저장소 자체를 꺼내지 못해도 던지지 않는다", () => {
    // 검증: V78 E57
    const host = globalThis as Record<string, unknown>;
    const before = Object.getOwnPropertyDescriptor(host, "localStorage");
    Object.defineProperty(host, "localStorage", {
      configurable: true,
      get() {
        throw new DOMException("막힘", "SecurityError");
      },
    });
    try {
      expect(soundEnabled()).toBe(true);
      expect(setSoundEnabled(false)).toBe(false);
    } finally {
      if (before) Object.defineProperty(host, "localStorage", before);
      else delete host.localStorage;
    }
  });
});
