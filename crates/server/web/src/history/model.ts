// 히스토리 화면의 화면 모델(server 29차 · C30 · V83 · V84 · F7-7 · F7-9): 서버 응답을 화면이 그릴 줄로 바꾸는 순수 함수.
// DOM을 만지지 않는다(그리기는 view.ts, 서버 부르기는 screen.ts). 금액은 그 시장 돈 모양, 시각은 한국 시각 글이다.

/** 매매 상태 글자(서버 그대로, 스펙의 매매 상태표 차례). */
export type TradeState =
  | "discussing"
  | "judged"
  | "awaiting_approval"
  | "blocked"
  | "ordered"
  | "holding"
  | "sold"
  | "reviewed";

/** 매매 상태 글자 전부(상태 거르기의 고르기 차례). */
export const TRADE_STATES: readonly TradeState[] = [
  "discussing",
  "judged",
  "awaiting_approval",
  "blocked",
  "ordered",
  "holding",
  "sold",
  "reviewed",
];

/** 승인 요청 결과 글자(서버 그대로). */
export type ApprovalStatus = "pending" | "approved" | "rejected" | "no_answer" | "expired";

/** 승인 요청 결과 글자 전부(상태 거르기의 고르기 차례). */
export const APPROVAL_STATUSES: readonly ApprovalStatus[] = ["pending", "approved", "rejected", "no_answer", "expired"];

/** 값이 없을 때 보이는 글. */
export const NONE_TEXT = "없음";

/** 매매 목록 `GET /api/trades`의 줄 하나. 시각은 유닉스 밀리초. */
export interface TradeItem {
  id: number;
  symbol: string;
  account: string;
  state: string;
  created_at: number;
  updated_at: number;
  decided_at: number | null;
}

/** 매매 목록 응답. `next`는 다음 쪽의 `before` 값(더 없으면 없음). */
export interface TradesReply {
  trades: TradeItem[];
  next: number | null;
}

/** 상태 바뀜 하나(시각 순서). 만든 줄의 `from`은 없음. */
export interface StateChange {
  state: string;
  from: string | null;
  at: number;
}

/** 매매 근거. 금액은 그 시장 통화의 십진수 글자. 판단 전이면 빈 칸은 없음 · 빈 목록. */
export interface Basis {
  hypothesis: string | null;
  trigger: string | null;
  entry: string | null;
  stop: string | null;
  target: string | null;
  expected_value: string | null;
  scores: { role: string; score: number; reason: string }[];
  consensus_score: number | null;
  sources: { name: string; known_at: number }[];
}

/** 매매 하나 `GET /api/trades/{id}`. */
export interface TradeDetail extends TradeItem {
  states: StateChange[];
  side: "buy" | "sell" | null;
  quantity: number | null;
  facts: string[];
  currency: string;
  basis: Basis;
  summary: string;
}

/** 승인 요청 기록 `GET /api/approvals`의 줄 하나. `summary`는 가린 사유 글. */
export interface ApprovalItem {
  id: number;
  number: string;
  summary: string;
  status: string;
  created_at: number;
  resolved_at: number | null;
  consumed_at: number | null;
}

/** 승인 요청 기록 응답. */
export interface ApprovalsReply {
  approvals: ApprovalItem[];
  next: number | null;
}

/** 하루 정리 `GET /api/daily-summary?date=`. 다른 날짜면 `pnl_krw`는 없음. */
export interface DailySummary {
  date: string;
  done: string;
  pnl: string;
  review: string;
  orders: number;
  pnl_krw: string | null;
  reviews: number;
  backup_at: number | null;
}

/** 매매 목록의 줄 하나(그릴 글). */
export interface TradeRow {
  id: number;
  symbol: string;
  state: string;
  /** 상태 글(스펙의 이름). */
  stateLabel: string;
  /** 만든 시각 글(한국 시각). */
  created: string;
  /** 바뀐 시각 글. */
  updated: string;
  /** 판단 시각 글. 판단 전이면 없음. */
  decided: string | null;
}

/** 상태 흐름의 줄 하나. */
export interface FlowRow {
  state: string;
  /** 상태 글. */
  label: string;
  /** 바뀐 시각 글. */
  at: string;
}

/** 근거 요약 줄의 칸(F7-7 차례). */
export type BasisKey = "hypothesis" | "trigger" | "entry" | "stop" | "target" | "scores" | "facts";

/** 근거 요약 줄 하나. */
export interface BasisLine {
  key: BasisKey;
  /** 보이는 이름: 가설 · 계기 · 진입 · 손절 · 목표 · 담당 점수 · 자료. */
  label: string;
  /** 보이는 글. 값이 없으면 [`NONE_TEXT`]. */
  text: string;
}

/** 승인 기록의 줄 하나(그릴 글). */
export interface ApprovalRow {
  id: number;
  number: string;
  summary: string;
  status: string;
  /** 결과 글. */
  statusLabel: string;
  created: string;
  resolved: string | null;
  consumed: string | null;
}

/**
 * 매매 상태 글(스펙의 매매 상태 이름): 논의중 · 판단됨 · 승인대기 · 막힘 · 주문됨 · 보유중 · 팔림 · 복기됨.
 * 모르는 글자는 그대로.
 */
export function tradeStateLabel(state: string): string {
  void state;
  throw new Error("todo");
}

/** 승인 요청 결과 글: 기다림 · 승인 · 거절 · 답 없음 · 만료. 모르는 글자는 그대로. */
export function approvalStatusLabel(status: string): string {
  void status;
  throw new Error("todo");
}

/** 매매 방향 글: 사기 · 팔기, 없으면 [`NONE_TEXT`]. */
export function sideText(side: string | null): string {
  void side;
  throw new Error("todo");
}

/**
 * 유닉스 밀리초 → 한국 시각 글 `"YYYY-MM-DD HH:mm"`(24시간, 두 자리). 컴퓨터의 시간대와 상관없이 UTC+9로 본다.
 * 예: `Date.UTC(2026, 10, 15, 15, 0)` → `"2026-11-16 00:00"`.
 */
export function timeText(ms: number): string {
  void ms;
  throw new Error("todo");
}

/**
 * 십진수 금액 글자 → 그 시장 돈 모양. 원화(`KRW`)는 천 단위 쉼표와 "원"(`"100000"` → `"100,000원"`,
 * `"-5000"` → `"-5,000원"`), 달러(`USD`)는 "$"와 천 단위 쉼표 · 소수 둘째 자리(`"1234.5"` → `"$1,234.50"`,
 * `"-12"` → `"-$12.00"`). 없으면 [`NONE_TEXT`]. 모르는 통화는 수 글 뒤에 통화 글자.
 */
export function moneyText(value: string | null, currency: string): string {
  void value;
  void currency;
  throw new Error("todo");
}

/** 매매 목록의 줄(응답 차례). */
export function tradeRows(reply: TradesReply): TradeRow[] {
  void reply;
  throw new Error("todo");
}

/** 상태 흐름의 줄(응답 차례 = 시각 순서). */
export function flowRows(detail: TradeDetail): FlowRow[] {
  void detail;
  throw new Error("todo");
}

/**
 * 근거 요약 줄(F7-7): 가설 · 계기 · 진입 · 손절 · 목표 · 담당 점수 · 자료, 이 차례로 늘 일곱 줄.
 * - 진입 · 손절 · 목표는 [`moneyText`]로 그 매매의 통화 모양.
 * - 담당 점수는 담당마다 `"<담당> <점수>점 — <까닭>"`을 `" · "`로 잇는다.
 * - 자료는 붙은 자료 이름(`facts`)과 판단에 쓴 자료 이름(`basis.sources`, 겹치면 한 번)을 `" · "`로 잇는다.
 * - 값이 없으면 [`NONE_TEXT`].
 */
export function basisLines(detail: TradeDetail): BasisLine[] {
  void detail;
  throw new Error("todo");
}

/** 승인 기록의 줄(응답 차례). */
export function approvalRows(reply: ApprovalsReply): ApprovalRow[] {
  void reply;
  throw new Error("todo");
}
