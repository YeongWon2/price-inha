// 히스토리 화면의 그리기(server 29차 2번): 탭 · 매매 목록 · 근거와 상태 흐름 · 하루 정리 · 승인 기록.
// 26차 공통 부품 · 토큰만 쓴다. 서버를 부르지 않는다(screen.ts가 부른다). 글은 모두 글자(`textContent`)로 넣는다.
//
// 화면 계약(시험이 고르는 표시):
// - 탭 줄 `[role="tablist"]` 안에 탭 셋 `button[role="tab"][data-tab="trades" | "summary" | "approvals"]`
//   (글 "매매" · "하루 정리" · "승인 기록", 고른 탭만 `aria-selected="true"`, `aria-controls`는 그 판의 id).
//   판 `[role="tabpanel"][data-tab=같은 이름]`은 고른 탭의 것만 보이고 나머지는 `hidden`.
// - 매매 판: 상태 거르기 `select[data-role="trade-state"]`(첫 칸 값 ""은 전체, 이어서 `TRADE_STATES`),
//   목록 `[data-role="trades"]`의 줄 `[data-trade=번호]`(`data-state`=상태 글자) 안에 종목 `[data-role="symbol"]` ·
//   상태 글 `[data-role="state"]` · 만든 시각 글 `[data-role="time"]` · 펼치기 단추 `button[data-action="toggle"]`
//   (`aria-expanded`). 펼친 줄 안에 `[data-role="detail"]`: 근거 요약 `[data-role="basis"]`의 줄 `[data-key=칸]`
//   (`basisLines` 차례, 글에 이름과 값), 상태 흐름 `[data-role="flow"]`의 줄 `[data-state=상태 글자]`(시각 순서,
//   글에 상태 글과 시각 글), 방향 · 수량 `[data-role="side"]`.
// - 하루 정리 판: 날짜 고르기 `input[type="date"][data-role="date"]`, 정리 `[data-role="summary"]` 안에 날짜
//   `[data-role="summary-date"]` · 한 일 `[data-part="done"]` · 손익 `[data-part="pnl"]` · 복기 `[data-part="review"]`.
// - 승인 기록 판: 상태 거르기 `select[data-role="approval-status"]`(첫 칸 ""은 전체, 이어서 `APPROVAL_STATUSES`),
//   목록 `[data-role="approvals"]`의 줄 `[data-approval=줄 번호]`(`data-status`) 안에 승인 번호 `[data-role="number"]` ·
//   결과 글 `[data-role="status"]` · 가린 요약 `[data-role="summary"]` · 만든 시각 글 `[data-role="time"]`.
// - 빈 목록은 그 판에 `[data-role="empty"]`(`role="status"`). "더 보기"는 그 판의 `button[data-action="more"]`.
import type { ApprovalRow, BasisLine, DailySummary, FlowRow, TradeRow } from "./model";

/** 탭 이름. */
export type HistoryTab = "trades" | "summary" | "approvals";

/** 탭 줄과 판 셋. 처음 고른 탭은 `selected`. */
export function renderTabs(selected: HistoryTab): { tabs: HTMLElement; panels: Record<HistoryTab, HTMLElement> } {
  void selected;
  throw new Error("todo");
}

/** 매매 목록의 줄들(목록 `[data-role="trades"]`에 붙일 것). */
export function renderTradeRows(rows: readonly TradeRow[]): HTMLElement[] {
  void rows;
  throw new Error("todo");
}

/** 펼친 줄 안의 근거 요약과 상태 흐름. `side`는 방향 · 수량 글. */
export function renderDetail(basis: readonly BasisLine[], flow: readonly FlowRow[], side: string): HTMLElement {
  void basis;
  void flow;
  void side;
  throw new Error("todo");
}

/** 하루 정리. */
export function renderSummary(summary: DailySummary): HTMLElement {
  void summary;
  throw new Error("todo");
}

/** 승인 기록의 줄들(목록 `[data-role="approvals"]`에 붙일 것). */
export function renderApprovalRows(rows: readonly ApprovalRow[]): HTMLElement[] {
  void rows;
  throw new Error("todo");
}
