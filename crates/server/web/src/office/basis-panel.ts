// 사무실 근거 패널(server 50차 · F7-7 · C23 · V68 · V69): 논의 줄의 종목 표시를 누르면 그 종목의 가장 최근 매매의
// 근거 카드를, 매매 번호로 열면 그 매매의 근거 카드를 채팅 옆 패널에 보인다(히스토리 화면과 같은 카드).
// 서버는 `paths.ts`의 경로로만 부른다: 매매 목록 `GET /api/trades?max=` · 근거 `GET /api/trades/{id}/basis`.
//
// 모양(시험이 고르는 표시): 패널 `section[data-role="basis-panel"]`(처음엔 숨김), 제목 `[data-part="title"]`,
// 닫기 `button[data-action="close-basis"]`, 본문 `[data-role="basis-body"]`(카드 · 안내 글 · 오류 줄 `role="alert"`).

/** `fetch`와 같은 모양(시험에서 바꿔 끼운다). */
export type FetchLike = (input: string, init?: RequestInit) => Promise<Response>;

/** 근거 패널. */
export interface BasisPanel {
  readonly element: HTMLElement;
  /** 그 매매의 근거 카드를 연다. */
  openTrade(id: number): Promise<void>;
  /** 그 종목의 가장 최근 매매의 근거 카드를 연다. 매매가 없으면 안내 글. */
  openSymbol(symbol: string): Promise<void>;
  /** 닫는다(숨긴다). */
  close(): void;
}

/** 자리(`host`) 끝에 근거 패널을 붙인다. */
export function mountBasisPanel(host: HTMLElement, fetcher: FetchLike): BasisPanel {
  void fetcher;
  const element = document.createElement("section");
  host.append(element);
  return {
    element,
    openTrade: async () => {},
    openSymbol: async () => {},
    close: () => {},
  };
}

/** 채팅 줄(`log`)의 종목 표시(`.topic[data-symbol]`)를 누르거나 Enter를 치면 패널로 그 종목을 연다. */
export function bindTopicClicks(log: HTMLElement, panel: BasisPanel): void {
  void log;
  void panel;
}
