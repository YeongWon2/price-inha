// 히스토리 화면 시험(server 29차)의 응답 본: 매매 목록 · 매매 하나 · 승인 요청 기록 · 하루 정리의 모양.
import type { ApprovalItem, DailySummary, TradeDetail, TradeItem } from "../../web/src/history/model";

/** 2026-11-16 10:00 한국 시각(01:00 UTC)의 유닉스 밀리초. */
export const MONDAY_10 = Date.UTC(2026, 10, 16, 1, 0);

/** 매매 목록의 줄 하나(번호가 클수록 늦게 만든 것). */
export function trade(id: number, state = "discussing"): TradeItem {
  const at = MONDAY_10 + id * 60_000;
  return {
    id,
    symbol: id % 2 === 0 ? "000660" : "035420",
    account: "paper",
    state,
    created_at: at,
    updated_at: at + 1_000,
    decided_at: state === "discussing" ? null : at + 1_000,
  };
}

/** 매매 `count`개(1번부터), 상태는 `stateOf`로 정한다. 번호 순서. */
export function trades(count: number, stateOf: (id: number) => string = () => "judged"): TradeItem[] {
  return Array.from({ length: count }, (_, i) => trade(i + 1, stateOf(i + 1)));
}

/** 매매 하나: 논의중 → 판단됨 → 승인대기, 근거 6칸 · 담당 점수 · 자료가 다 있다(원화). */
export function detailOf(item: TradeItem): TradeDetail {
  const at = item.created_at;
  return {
    ...item,
    state: "awaiting_approval",
    states: [
      { state: "discussing", from: null, at },
      { state: "judged", from: "discussing", at: at + 1_000 },
      { state: "awaiting_approval", from: "judged", at: at + 2_000 },
    ],
    side: "buy",
    quantity: 7,
    facts: ["주요사항보고서(자기주식취득결정)"],
    currency: "KRW",
    basis: {
      hypothesis: "실적 개선",
      trigger: "거래량 증가",
      entry: "100000",
      stop: "95000",
      target: "110000",
      expected_value: "5000",
      scores: [{ role: "risk", score: 70, reason: "근거 충분" }],
      consensus_score: 70,
      sources: [{ name: "시세", known_at: at + 1_000 }],
    },
    summary: "종목 " + item.symbol + "\n가설: 실적 개선",
  };
}

/** 판단 전 매매 하나: 근거 칸이 비어 있다. */
export function emptyDetailOf(item: TradeItem): TradeDetail {
  return {
    ...item,
    state: "discussing",
    states: [{ state: "discussing", from: null, at: item.created_at }],
    side: null,
    quantity: null,
    facts: [],
    currency: "KRW",
    basis: {
      hypothesis: null,
      trigger: null,
      entry: null,
      stop: null,
      target: null,
      expected_value: null,
      scores: [],
      consensus_score: null,
      sources: [],
    },
    summary: "종목 " + item.symbol,
  };
}

/** 승인 요청 기록의 줄 하나(번호가 클수록 늦게 만든 것). */
export function approval(id: number, status = "pending"): ApprovalItem {
  const at = MONDAY_10 + id * 60_000;
  return {
    id,
    number: "A-" + String(id).padStart(4, "0"),
    summary: "개선안 " + id + " 반영 요청 키 [가림]",
    status,
    created_at: at,
    resolved_at: status === "pending" ? null : at + 5_000,
    consumed_at: null,
  };
}

/** 그 날짜의 하루 정리. 오늘이 아니면 손익 수가 없다. */
export function daySummary(date: string, today = "2026-11-16"): DailySummary {
  const isToday = date === today;
  return {
    date,
    done: isToday ? "체결 2건 · 복기 1건" : "체결 0건 · 복기 0건 (" + date + ")",
    pnl: isToday ? "오늘 손익 -50000원" : "그날 손익 기록 없음 (" + date + ")",
    review: isToday ? "매매 3: 손절이 늦었다" : "남긴 복기가 없습니다 (" + date + ")",
    orders: isToday ? 2 : 0,
    pnl_krw: isToday ? "-50000" : null,
    reviews: isToday ? 1 : 0,
    backup_at: null,
  };
}
