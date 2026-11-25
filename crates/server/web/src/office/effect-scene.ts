// 익절 · 손절 연출을 사무실 화면에 잇는다(server 24차 3번 — 뼈대, red).

import type { EffectShown } from "./effects";
import type { OfficeMarks } from "./marks-scene";

export function effectElement(shown: EffectShown, name: string): HTMLElement {
  void shown;
  void name;
  return document.createElement("span");
}

export interface OfficeEffects {
  onEffect(payload: unknown): void;
  wrap(marks: OfficeMarks): OfficeMarks;
}

export function createEffects(names: Record<string, string>): OfficeEffects {
  void names;
  return { onEffect: () => undefined, wrap: (marks) => marks };
}
