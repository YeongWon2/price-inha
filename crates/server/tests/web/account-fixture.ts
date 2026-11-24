// 계좌 · 운용 화면 시험의 응답 모양(server 30차 · C31): `GET /api/account` · `GET /api/team`.
import type { AccountReply, HoldingLine, TeamReply } from "../../web/src/account/model";

/** 팀 상태 응답. */
export function team(state: string, plan: TeamReply["plan"] = null, reason: string | null = null): TeamReply {
  return { state, plan, reason };
}

/** 보유 한 줄(원화, 시세가 있다). */
export function holding(symbol: string, overrides: Partial<HoldingLine> = {}): HoldingLine {
  return {
    symbol,
    market: "kr",
    currency: "KRW",
    quantity: "5",
    average_price: "100015",
    last_price: "95000",
    value: "475000",
    pnl: "-25075",
    ...overrides,
  };
}

/** 계좌 응답: 원화 · 달러 현금과 평가액, 보유 `holdings`. 원화 낙폭 · 한도는 고를 수 있다. */
export function account(
  holdings: HoldingLine[] = [],
  krw: { drawdown?: string; limit?: string | null; total?: string; dayPnl?: string } = {},
): AccountReply {
  return {
    account: "paper",
    cash: [
      { currency: "KRW", available: "9499925", held: "0", unsettled: "0" },
      { currency: "USD", available: "10000", held: "0", unsettled: "0" },
    ],
    holdings,
    equity: [
      {
        currency: "KRW",
        total: krw.total ?? "9974925",
        day_start: "10000000",
        day_pnl: krw.dayPnl ?? "-25075",
        peak: "10000000",
        drawdown_pct: krw.drawdown ?? "0.25",
        drawdown_limit_pct: krw.limit === undefined ? "20" : krw.limit,
      },
      {
        currency: "USD",
        total: "10000",
        day_start: "10000",
        day_pnl: "0",
        peak: "10000",
        drawdown_pct: "0",
        drawdown_limit_pct: krw.limit === undefined ? "20" : krw.limit,
      },
    ],
  };
}
