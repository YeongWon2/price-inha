// 논의 진행 단계 그리기(server 51차, C21 · V63). 뼈대.

/** 옆 패널의 진행 단계. */
export interface ProgressView {
  apply(payload: unknown, now: number): void;
  tick(now: number): void;
}

export function mountProgress(host: HTMLElement, nameOf: (id: string) => string): ProgressView {
  void host;
  void nameOf;
  return { apply: () => undefined, tick: () => undefined };
}
