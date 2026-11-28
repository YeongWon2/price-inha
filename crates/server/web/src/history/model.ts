// 히스토리 화면의 화면 모델(server 29차 · C30 · V83 · V84 · F7-7 · F7-9): 서버 응답을 화면이 그릴 줄로 바꾸는 순수 함수.
// DOM을 만지지 않는다(그리기는 view.ts, 서버 부르기는 screen.ts). 금액은 그 시장 돈 모양, 시각은 한국 시각 글이다.
import type { DiscussionEnd, EndKind } from "../ui/discussion-end";

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

/** (69차) 매매 거르기 글자: 논의중을 셋(논의중 · 지켜봄 · 끝나지 못함)으로 나눈 뒤 나머지 상태. */
export const TRADE_FILTERS: readonly string[] = [];

/** (69차) 매매 거르기 글자의 글. 모르는 글자는 그대로. */
export function tradeFilterLabel(word: string): string {
  return word;
}

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
  /** (69차) 논의중인데 마지막 논의가 지켜보기로 끝났으면 그 까닭 코드 · 시각. */
  watched?: DiscussionEnd;
  /** (69차) 논의중인데 마지막 논의가 오류로 끝났으면 그 까닭 코드 · 시각. */
  failed?: DiscussionEnd;
}

/** 매매 목록 응답.`next`는 다음 쪽의 `before` 값(더 없으면 없음). */
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
  /** 상태 글(스펙의 이름). 끝난 논의면 그 글(69차). */
  stateLabel: string;
  /** (69차) 끝난 논의의 종류. 아니면 없음. */
  ended: EndKind | null;
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

/** 매매 상태 글자 → 스펙의 이름. */
const TRADE_STATE_LABELS: Readonly<Record<string, string>> = {
  discussing: "논의중",
  judged: "판단됨",
  awaiting_approval: "승인대기",
  blocked: "막힘",
  ordered: "주문됨",
  holding: "보유중",
  sold: "팔림",
  reviewed: "복기됨",
};

/** 승인 요청 결과 글자 → 보이는 글. */
const APPROVAL_STATUS_LABELS: Readonly<Record<string, string>> = {
  pending: "기다림",
  approved: "승인",
  rejected: "거절",
  no_answer: "답 없음",
  expired: "만료",
};

/** 한국 시각(UTC+9)의 밀리초 차이. */
const KST_OFFSET_MS = 9 * 60 * 60 * 1000;

/** 근거 요약 줄의 칸과 이름(F7-7 차례). */
const BASIS_LABELS: readonly (readonly [BasisKey, string])[] = [
  ["hypothesis", "가설"],
  ["trigger", "계기"],
  ["entry", "진입"],
  ["stop", "손절"],
  ["target", "목표"],
  ["scores", "담당 점수"],
  ["facts", "자료"],
];

/** 표에서 그 글자의 글을 찾는다. 없으면 글자 그대로. */
function labelOf(table: Readonly<Record<string, string>>, word: string): string {
  return Object.prototype.hasOwnProperty.call(table, word) ? table[word] : word;
}

/**
 * 매매 상태 글(스펙의 매매 상태 이름): 논의중 · 판단됨 · 승인대기 · 막힘 · 주문됨 · 보유중 · 팔림 · 복기됨.
 * 모르는 글자는 그대로.
 */
export function tradeStateLabel(state: string): string {
  return labelOf(TRADE_STATE_LABELS, state);
}

/** 승인 요청 결과 글: 기다림 · 승인 · 거절 · 답 없음 · 만료. 모르는 글자는 그대로. */
export function approvalStatusLabel(status: string): string {
  return labelOf(APPROVAL_STATUS_LABELS, status);
}

/** 매매 방향 글: 사기 · 팔기, 없으면 [`NONE_TEXT`]. */
export function sideText(side: string | null): string {
  if (side === "buy") return "사기";
  if (side === "sell") return "팔기";
  return side === null ? NONE_TEXT : side;
}

function two(value: number): string {
  return String(value).padStart(2, "0");
}

/**
 * 유닉스 밀리초 → 한국 시각 글 `"YYYY-MM-DD HH:mm"`(24시간, 두 자리). 컴퓨터의 시간대와 상관없이 UTC+9로 본다.
 * 예: `Date.UTC(2026, 10, 15, 15, 0)` → `"2026-11-16 00:00"`.
 */
export function timeText(ms: number): string {
  const at = new Date(ms + KST_OFFSET_MS);
  const date = `${at.getUTCFullYear()}-${two(at.getUTCMonth() + 1)}-${two(at.getUTCDate())}`;
  return `${date} ${two(at.getUTCHours())}:${two(at.getUTCMinutes())}`;
}

/** 정수 부분 글자에 세 자리마다 쉼표. */
function grouped(digits: string): string {
  return digits.replace(/\B(?=(\d{3})+(?!\d))/g, ",");
}

/**
 * 십진수 금액 글자 → 그 시장 돈 모양. 원화(`KRW`)는 천 단위 쉼표와 "원"(`"100000"` → `"100,000원"`,
 * `"-5000"` → `"-5,000원"`), 달러(`USD`)는 "$"와 천 단위 쉼표 · 소수 둘째 자리(`"1234.5"` → `"$1,234.50"`,
 * `"-12"` → `"-$12.00"`). 없으면 [`NONE_TEXT`]. 모르는 통화는 수 글 뒤에 통화 글자.
 * 부동소수점으로 바꾸지 않고 글자로만 다룬다.
 */
export function moneyText(value: string | null, currency: string): string {
  if (value === null) return NONE_TEXT;
  const shape = /^([+-]?)(\d+)(?:\.(\d+))?$/.exec(value.trim());
  if (!shape) return value;
  const sign = shape[1] === "-" ? "-" : "";
  const whole = grouped(shape[2]);
  const fraction = shape[3] ?? "";
  if (currency === "USD") return `${sign}$${whole}.${(fraction + "00").slice(0, 2)}`;
  const number = fraction ? `${whole}.${fraction}` : whole;
  if (currency === "KRW") return `${sign}${number}원`;
  return `${sign}${number} ${currency}`;
}

/** 매매 목록의 줄(응답 차례). */
export function tradeRows(reply: TradesReply): TradeRow[] {
  return reply.trades.map((item) => ({
    id: item.id,
    symbol: item.symbol,
    state: item.state,
    stateLabel: tradeStateLabel(item.state),
    ended: null,
    created: timeText(item.created_at),
    updated: timeText(item.updated_at),
    decided: item.decided_at === null ? null : timeText(item.decided_at),
  }));
}

/** 상태 흐름의 줄(응답 차례 = 시각 순서). */
export function flowRows(detail: TradeDetail): FlowRow[] {
  return detail.states.map((change) => ({
    state: change.state,
    label: tradeStateLabel(change.state),
    at: timeText(change.at),
  }));
}

/** 글이 비었으면 [`NONE_TEXT`]. */
function orNone(text: string | null): string {
  return text === null || text.trim() === "" ? NONE_TEXT : text;
}

/**
 * 근거 요약 줄(F7-7): 가설 · 계기 · 진입 · 손절 · 목표 · 담당 점수 · 자료, 이 차례로 늘 일곱 줄.
 * - 진입 · 손절 · 목표는 [`moneyText`]로 그 매매의 통화 모양.
 * - 담당 점수는 담당마다 `"<담당> <점수>점 — <까닭>"`을 `" · "`로 잇는다.
 * - 자료는 붙은 자료 이름(`facts`)과 판단에 쓴 자료 이름(`basis.sources`, 겹치면 한 번)을 `" · "`로 잇는다.
 * - 값이 없으면 [`NONE_TEXT`].
 */
export function basisLines(detail: TradeDetail): BasisLine[] {
  const basis = detail.basis;
  const names = [...new Set([...detail.facts, ...basis.sources.map((source) => source.name)])];
  const text: Record<BasisKey, string> = {
    hypothesis: orNone(basis.hypothesis),
    trigger: orNone(basis.trigger),
    entry: moneyText(basis.entry, detail.currency),
    stop: moneyText(basis.stop, detail.currency),
    target: moneyText(basis.target, detail.currency),
    scores: orNone(basis.scores.map((s) => `${s.role} ${s.score}점 — ${s.reason}`).join(" · ")),
    facts: orNone(names.join(" · ")),
  };
  return BASIS_LABELS.map(([key, label]) => ({ key, label, text: text[key] }));
}

/** 승인 기록의 줄(응답 차례). */
export function approvalRows(reply: ApprovalsReply): ApprovalRow[] {
  return reply.approvals.map((item) => ({
    id: item.id,
    number: item.number,
    summary: item.summary,
    status: item.status,
    statusLabel: approvalStatusLabel(item.status),
    created: timeText(item.created_at),
    resolved: item.resolved_at === null ? null : timeText(item.resolved_at),
    consumed: item.consumed_at === null ? null : timeText(item.consumed_at),
  }));
}
