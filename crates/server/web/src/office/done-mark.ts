// 끝남 표시(server 34차 3번, additions.md N12 · V62 — 원본 동작: docs/research/agent-office-refs.md 1-B ·
// F 1, pixel-agents의 완료 말풍선): 담당의 일이 끝나면 머리 위 ✓가 2.0초 보이고 0.5초 동안 흐려져 사라진다.
// 그 사이 새 일이 시작되면 바로 지운다. 장면 없이 시험할 수 있게 시각은 부르는 쪽이 넘기는 밀리초(`now`, 가짜 시계)다.
// 글자 · 색은 legend.ts의 `done` 표시를 쓴다.

import { markOf, type MarkId } from "./legend";

/** 끝남 표시가 온전히 보이는 시간(밀리초, 원본 2.0초). */
export const DONE_SHOW_MS = 2_000;
/** 그 뒤 흐려져 사라지는 시간(밀리초, 원본 0.5초). */
export const DONE_FADE_MS = 500;

/** 지금 보이는 끝남 표시. `opacity`는 1(온전히)에서 0 쪽으로 줄어든다(흐려지는 동안 0보다 크고 1보다 작다). */
export interface DoneShown {
  mark: MarkId;
  text: string;
  color: string;
  opacity: number;
}

/** 담당마다의 끝남 표시. */
export class DoneMarks {
  /** 담당 → 끝난 시각(밀리초). */
  private readonly finished = new Map<string, number>();

  /** 그 담당의 일이 `now`에 끝났다: ✓를 새로 띄운다(이미 있으면 `now`부터 다시 센다). */
  finish(agent: string, now: number): void {
    this.finished.set(agent, now);
  }

  /** 그 담당이 새 일을 시작했다: 끝남 표시를 바로 지운다. */
  start(agent: string, now: number): void {
    void now;
    this.finished.delete(agent);
  }

  /**
   * `now`에 그 담당에게 보이는 끝남 표시. 끝난 뒤 [`DONE_SHOW_MS`] 동안 `opacity` 1, 그 뒤 [`DONE_FADE_MS`] 동안
   * 1에서 0으로 고르게(직선으로) 줄고, 끝난 뒤 `DONE_SHOW_MS + DONE_FADE_MS`부터는 `undefined`.
   */
  shown(agent: string, now: number): DoneShown | undefined {
    const at = this.finished.get(agent);
    if (at === undefined) return undefined;
    const age = Math.max(0, now - at);
    if (age >= DONE_SHOW_MS + DONE_FADE_MS) {
      this.finished.delete(agent);
      return undefined;
    }
    const opacity = age < DONE_SHOW_MS ? 1 : 1 - (age - DONE_SHOW_MS) / DONE_FADE_MS;
    const mark = markOf("done");
    return { mark: mark.id, text: mark.glyph, color: mark.color, opacity };
  }

  /** 지금 끝남 표시가 걸린 담당들(다 사라진 것은 빠진다). */
  agents(now: number): string[] {
    return [...this.finished.keys()].filter((agent) => this.shown(agent, now) !== undefined);
  }
}
