// 익절 · 손절 연출 줄(server 24차 3번 — 뼈대, red).

import type { MarkId } from "./legend";

export const KIND_OFFICE_EFFECT = "office.effect";

export type EffectKind = "take_profit" | "stop_loss";

export const EFFECT_SHOW_MS: Readonly<Record<EffectKind, number>> = { take_profit: 0, stop_loss: 0 };

export const EFFECT_STALE_MS = 10_000;

export interface OfficeEffect {
  kind: EffectKind;
  agent: string;
  trade: number;
  symbol: string;
  at: number;
  returnPct: number | null;
}

export function readEffect(payload: unknown): OfficeEffect | null {
  void payload;
  return null;
}

export function effectText(effect: OfficeEffect): string {
  void effect;
  return "";
}

export interface EffectShown {
  agent: string;
  mark: MarkId;
  glyph: string;
  color: string;
  text: string;
  label: string;
}

export class EffectQueue {
  push(effect: OfficeEffect, now: number, hidden: boolean): boolean {
    void effect;
    void now;
    void hidden;
    return false;
  }

  current(now: number): EffectShown | undefined {
    void now;
    return undefined;
  }

  last(): { start: number; end: number } | undefined {
    return undefined;
  }

  clear(): void {}
}
