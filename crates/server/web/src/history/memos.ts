// 히스토리 화면의 "결정 메모" 탭(뼈대).

export type Fetcher = (url: string, init?: RequestInit) => Promise<Response>;

export const MEMO_PAGE_MAX = 0;

export interface MemoTab {
  readonly loaded: boolean;
  open(): Promise<void>;
  settled(): Promise<void>;
}

export function mountMemoTab(_panel: HTMLElement, _fetcher: Fetcher): MemoTab {
  return {
    loaded: false,
    open: () => Promise.resolve(),
    settled: () => Promise.resolve(),
  };
}
