// 익절 · 손절 연출 줄(server 24차 3번, additions.md R5-14 · C26 · E60 · N12 · V66 — 원본 동작:
// docs/research/agent-office-refs.md 3-E · F 2, PixelOffice의 성공 1.5초 · 실패 2.0초): 체결 이벤트 `office.effect`가
// 익절이면 매매 담당 머리 위에 기쁨 표시와 "+n%"가 1.5초, 손절이면 차분한 표시와 "-n%"가 2.0초 뜬다.
// - 여러 연출이 겹치면 줄 세워 하나씩 보인다(E60).
// - 화면이 숨어 있으면 쌓지 않고 버린다. 숨기면 줄을 비운다(E60 — 채팅 · 히스토리에는 남는다, 연출은 화면만의 일).
// - 다시 이을 때 다시 받은 옛 이벤트(받은 때보다 [`EFFECT_STALE_MS`] 넘게 앞선 것)와 같은 매매의 같은 연출은 다시 보이지 않는다.
// 수익률은 서버가 세서 글자로 싣는다(화면은 그린다). 장면 없이 시험할 수 있게 시각은 부르는 쪽이 넘기는 밀리초다.
// 글자 · 색은 legend.ts의 `profit` · `loss` 표시를 쓴다.

import { markOf, type MarkId } from "./legend";

/** 서버 이벤트 이름. */
export const KIND_OFFICE_EFFECT = "office.effect";

/** 연출 종류(서버 `kind`). */
export type EffectKind = "take_profit" | "stop_loss";

/** 연출이 보이는 시간(밀리초, N12: 익절 1.5초 · 손절 2.0초). */
export const EFFECT_SHOW_MS: Readonly<Record<EffectKind, number>> = { take_profit: 1_500, stop_loss: 2_000 };

/** 받은 때보다 이만큼 넘게 앞선 연출은 옛 것으로 버린다(다시 이을 때 다시 받은 것). */
export const EFFECT_STALE_MS = 10_000;

/** 연출 종류 → 머리 위 표시. */
const MARK_OF: Readonly<Record<EffectKind, MarkId>> = { take_profit: "profit", stop_loss: "loss" };

/** 읽은 연출 이벤트. `returnPct`는 퍼센트 수(서버가 실은 글자를 바꾼 것, 없으면 `null`). */
export interface OfficeEffect {
  kind: EffectKind;
  agent: string;
  trade: number;
  symbol: string;
  at: number;
  returnPct: number | null;
}

/** `office.effect` 내용을 읽는다(모양이 틀리면 `null`). */
export function readEffect(payload: unknown): OfficeEffect | null {
  if (typeof payload !== "object" || payload === null) return null;
  const p = payload as Record<string, unknown>;
  if (p.kind !== "take_profit" && p.kind !== "stop_loss") return null;
  if (typeof p.agent !== "string" || typeof p.trade !== "number" || typeof p.at !== "number") return null;
  const pct = typeof p.return_pct === "string" ? Number(p.return_pct) : Number.NaN;
  return {
    kind: p.kind,
    agent: p.agent,
    trade: p.trade,
    symbol: typeof p.symbol === "string" ? p.symbol : "",
    at: p.at,
    returnPct: Number.isFinite(pct) ? pct : null,
  };
}

/** 수익률 글: 부호와 소수 한 자리(`+4.0%`, `-6.1%`). 없으면 빈 글. */
export function effectText(effect: OfficeEffect): string {
  if (effect.returnPct === null) return "";
  const fixed = effect.returnPct.toFixed(1);
  return effect.returnPct > 0 ? `+${fixed}%` : `${fixed}%`;
}

/** 지금 보이는 연출. */
export interface EffectShown {
  agent: string;
  mark: MarkId;
  glyph: string;
  color: string;
  /** 수익률 글(없으면 빈 글). */
  text: string;
  /** 쉬운 뜻(범례와 같은 글). */
  label: string;
}

interface Slot {
  effect: OfficeEffect;
  start: number;
  end: number;
}

/** 연출 줄: 한 번에 하나씩 차례로 보인다. */
export class EffectQueue {
  private slots: Slot[] = [];
  /** 이미 넣은 연출(매매 · 종류). */
  private readonly seen = new Set<string>();

  /** 연출 하나를 `now`에 받는다. 숨은 화면 · 옛 것 · 이미 받은 것이면 버리고 거짓. */
  push(effect: OfficeEffect, now: number, hidden: boolean): boolean {
    const key = `${effect.trade}|${effect.kind}`;
    if (hidden || now - effect.at > EFFECT_STALE_MS || this.seen.has(key)) return false;
    this.seen.add(key);
    this.drop(now);
    const last = this.slots[this.slots.length - 1];
    const start = Math.max(now, last ? last.end : now);
    this.slots.push({ effect, start, end: start + EFFECT_SHOW_MS[effect.kind] });
    return true;
  }

  private drop(now: number): void {
    this.slots = this.slots.filter((s) => s.end > now);
  }

  /** `now`에 보이는 연출(없으면 `undefined`). */
  current(now: number): EffectShown | undefined {
    this.drop(now);
    const slot = this.slots.find((s) => s.start <= now && now < s.end);
    if (!slot) return undefined;
    const mark = markOf(MARK_OF[slot.effect.kind]);
    return {
      agent: slot.effect.agent,
      mark: mark.id,
      glyph: mark.glyph,
      color: mark.color,
      text: effectText(slot.effect),
      label: mark.label,
    };
  }

  /** 마지막으로 넣은 연출이 보이기 시작하는 · 끝나는 시각(줄이 비었으면 `undefined`). */
  last(): { start: number; end: number } | undefined {
    const slot = this.slots[this.slots.length - 1];
    return slot ? { start: slot.start, end: slot.end } : undefined;
  }

  /** 줄을 비운다(탭이 숨었을 때). */
  clear(): void {
    this.slots = [];
  }
}
