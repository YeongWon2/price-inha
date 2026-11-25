// 하루 AI 잔량 바 그리기(server 51차, C22 · V64). 뼈대.

/** 서버 부르기(시험이 바꿔 끼운다). */
export type Fetcher = (url: string, init?: RequestInit) => Promise<Response>;

/** 위 줄 잔량 바. */
export interface BudgetView {
  load(): Promise<void>;
  apply(payload: unknown): void;
}

export function mountBudget(bar: HTMLElement, options: { fetcher: Fetcher; nameOf: (id: string) => string }): BudgetView {
  void bar;
  void options;
  return { load: () => Promise.resolve(), apply: () => undefined };
}
