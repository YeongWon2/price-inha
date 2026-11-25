// 사무실 옆 패널의 "어제 메모" 카드(뼈대).

export type Fetcher = (url: string, init?: RequestInit) => Promise<Response>;

export const KIND_OFFICE_MEMO = "";
export const MEMO_CARD_DAYS = 0;

export interface MemoCard {
  refresh(): Promise<void>;
  onEvent(kind: unknown): Promise<void>;
}

export function mountMemoCard(_root: HTMLElement, _fetcher: Fetcher): MemoCard {
  return {
    refresh: () => Promise.resolve(),
    onEvent: () => Promise.resolve(),
  };
}
