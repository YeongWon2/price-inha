// 계좌 · 운용 화면의 화면 모델(server 30차 · C31 · V85 · V86 · R3-1 · R3-3 · R14-1 · R16-5 · F1-6): 서버 응답을 화면이
// 그릴 글 · 값으로 바꾸는 순수 함수. DOM을 만지지 않는다(그리기는 view.ts, 서버 부르기는 screen.ts).
// 금액은 그 통화의 돈 모양(원 `100,000원` · 달러 `$1,234.50`), 퍼센트는 소수 둘째 자리까지다.

/** 팀 상태 글자(`GET /api/team`의 `state`, 서버 그대로). */
export type TeamState = "operating" | "resting" | "daily_limit_halt" | "anomaly_halt" | "full_stop";

/** 팀 상태 → 화면 글(F1-6). */
export const TEAM_STATE_TEXT: Readonly<Record<TeamState, string>> = {
  operating: "운용 중",
  resting: "쉼",
  daily_limit_halt: "하루 한도 멈춤",
  anomaly_halt: "이상 감지 멈춤",
  full_stop: "전체 정지",
};

/** 팀 상태 배지의 빛깔(공통 부품 `badge`의 tone). */
export type StateTone = "neutral" | "success" | "warning" | "danger";

/** 모의계좌 안내(AGENTS 6번 · 결정 17): 화면의 어느 상태에서도 늘 보인다. */
export const PAPER_NOTICE = "모의계좌입니다 — 실제 주문은 나가지 않습니다";

/** 전체 정지일 때 팀 카드의 까닭 글(R3-3 · R14-1: 사람만 다시 시작한다). */
export const FULL_STOP_TEXT = "최대 손실 한도에 닿아 팀이 모두 멈췄습니다. 사람이 다시 시작해야 다시 돕니다.";

/** "다시 시작" 단추의 확인 글(`window.confirm`에 그대로 준다). */
export const RESTART_CONFIRM = "팀을 다시 시작할까요? 다시 시작하면 팀이 쉬는 중으로 돌아가 다음 판단부터 운용합니다.";

/** 보유가 없을 때의 빈 상태 글. */
export const EMPTY_HOLDINGS = "보유 종목이 없습니다";

/** 값이 없을 때 보이는 글. */
export const NONE_TEXT = "없음";

/** 실시간 연결이 없어도 다시 읽는 간격(밀리초, 30차 3번 — 조립 상수). */
export const REFRESH_MS = 10_000;

/** `/ws`에서 이 종류의 이벤트를 받으면 팀 상태와 계좌를 다시 읽는다(하루 시작 · 마감 · 오늘 판단). */
export const REFRESH_KINDS: readonly string[] = ["day_start", "day_close", "day_plan"];

/** `GET /api/team`. `plan` · `reason`은 오늘 팀장이 정한 것(없으면 없음). */
export interface TeamReply {
  state: string;
  plan: "trade" | "rest" | null;
  reason: string | null;
  /** (server 40차) 전체 정지면 그 정지의 사유(`"owner_stop"` 내가 정지 · `"max_loss"` 최대 손실). 없으면 없음. */
  stop_cause?: string | null;
}

/** 통화 하나의 현금(십진수 글자). */
export interface CashLine {
  currency: string;
  available: string;
  held: string;
  unsettled: string;
}

/** 보유 종목 하나. 받은 시세가 없으면 `last_price` · `value` · `pnl`은 없음. */
export interface HoldingLine {
  symbol: string;
  market: string;
  currency: string;
  quantity: string;
  average_price: string;
  last_price: string | null;
  value: string | null;
  pnl: string | null;
}

/** 통화 하나의 평가액 · 최고 · 낙폭. 쓰는 템플릿이 없으면 `drawdown_limit_pct`는 없음. */
export interface EquityLine {
  currency: string;
  total: string;
  day_start: string;
  day_pnl: string;
  peak: string;
  drawdown_pct: string;
  drawdown_limit_pct: string | null;
}

/** `GET /api/account`(C31). 계좌는 모의계좌뿐이다. */
export interface AccountReply {
  account: "paper";
  cash: CashLine[];
  holdings: HoldingLine[];
  equity: EquityLine[];
}

/** 오름 · 내림 · 그대로. */
export type Trend = "up" | "down" | "flat";

/** 보유 표의 줄 하나(그릴 글). `trend`는 평가 손익의 오름내림(모르면 없음). */
export interface HoldingRow {
  symbol: string;
  quantity: string;
  average: string;
  last: string;
  value: string;
  pnl: string;
  trend: Trend | null;
}

/**
 * 전체 정지 확인 글(server 40차 2번, 결정 25): 누르면 `window.confirm`으로 한 번 묻는다.
 */
export const STOP_CONFIRM = "새 주문이 모두 막힙니다. 보유의 손절은 계속 돕니다. 다시 시작은 이 화면에서만 됩니다.";

/**
 * (server 40차 · 41차 7번) 내가 내린 전체 정지(`stop_cause` `"owner_stop"`)일 때의 정지 안내. 41차부터 팀 카드는 까닭에
 * 맞는 글 **하나만** 보인다: 내가 정지면 이 글만, 그 밖(최대 손실 · 까닭 모름)이면 [`FULL_STOP_TEXT`]만.
 */
export const OWNER_STOP_TEXT = "내가 전체 정지를 눌렀습니다. 새 주문이 모두 막혀 있습니다. 사람이 다시 시작해야 다시 돕니다.";

/** 전체 정지 단추 글. */
export const STOP_LABEL = "전체 정지";

/** 전체 정지(`POST /api/team/stop`)의 결과(server 40차 2번). */
export type StopResult = { kind: "stopped"; state: string } | { kind: "refused"; reason: string };

/** 다시 시작(`POST /api/team/restart`)의 결과. */
export type RestartResult =
  | { kind: "restarted"; state: string; notice: string | null }
  | { kind: "needs_approval"; number: string }
  | { kind: "refused"; reason: string };

/** 십진수 글자를 부호 · 정수 자리 · 소수 자리로 나눈 것. */
interface Digits {
  negative: boolean;
  whole: string;
  fraction: string;
}

/** 십진수 글자(`-12.5`, `+3`, `0.00`)를 읽는다. 십진수가 아니면 없음. 부동소수점으로 바꾸지 않는다. */
function digitsOf(value: string): Digits | null {
  const found = /^([+-]?)(\d+)(?:\.(\d+))?$/.exec(value.trim());
  if (!found) return null;
  return { negative: found[1] === "-", whole: found[2], fraction: found[3] ?? "" };
}

/** 소수 `places`자리로 반올림(절반은 올림)한 크기 — `places`자리 정수(BigInt). */
function scaled(digits: Digits, places: number): bigint {
  const kept = digits.fraction.padEnd(places, "0").slice(0, places);
  let amount = BigInt(digits.whole + kept);
  if ((digits.fraction[places] ?? "0") >= "5") amount += BigInt(1);
  return amount;
}

/** 세 자리마다 쉼표. */
function grouped(whole: string): string {
  return whole.replace(/\B(?=(\d{3})+(?!\d))/g, ",");
}

/** 반올림한 크기를 (부호, 쉼표 붙은 정수 자리, 소수 자리)로. 0이 되면 부호가 없다. */
function rounded(value: string, places: number): { negative: boolean; whole: string; fraction: string } | null {
  const digits = digitsOf(value);
  if (!digits) return null;
  const amount = scaled(digits, places);
  const text = amount.toString().padStart(places + 1, "0");
  const whole = text.slice(0, text.length - places);
  const fraction = text.slice(text.length - places);
  return { negative: digits.negative && amount !== BigInt(0), whole: grouped(whole), fraction };
}

/** 팀 상태 글. 모르는 글자는 그대로 보인다. */
export function teamStateText(state: string): string {
  return Object.hasOwn(TEAM_STATE_TEXT, state) ? TEAM_STATE_TEXT[state as TeamState] : state;
}

/** 팀 상태 배지 빛깔: 운용 중 success · 쉼 neutral · 하루 한도 · 이상 감지 멈춤 warning · 전체 정지 danger(모르면 neutral). */
export function teamTone(state: string): StateTone {
  switch (state) {
    case "operating":
      return "success";
    case "daily_limit_halt":
    case "anomaly_halt":
      return "warning";
    case "full_stop":
      return "danger";
    default:
      return "neutral";
  }
}

/**
 * 오늘 할 일 글(F1-6): `plan`이 `"trade"`면 "오늘 매매합니다", `"rest"`면 "오늘 쉽니다", 없으면
 * "오늘 할 일을 아직 정하지 않았습니다".
 */
export function planText(team: TeamReply): string {
  if (team.plan === "trade") return "오늘 매매합니다";
  if (team.plan === "rest") return "오늘 쉽니다";
  return "오늘 할 일을 아직 정하지 않았습니다";
}

/** 쉬는 이유: 오늘 쉬기로 했고 이유가 있으면 그 글, 아니면 없음. */
export function restReason(team: TeamReply): string | null {
  if (team.plan !== "rest" || typeof team.reason !== "string") return null;
  const reason = team.reason.trim();
  return reason === "" ? null : reason;
}

/** "다시 시작" 단추를 보이는가: 전체 정지일 때만(R3-3 · R14-1). */
export function canRestart(state: string): boolean {
  return state === "full_stop";
}

/**
 * 돈 모양: 원은 세 자리마다 쉼표 + "원"(`"100000"` → `"100,000원"`), 달러는 "$" + 세 자리 쉼표 + 소수 둘째 자리
 * (`"1234.5"` → `"$1,234.50"`). 음수는 앞에 "-"(`"-5000"` → `"-5,000원"`, `"-12"` → `"-$12.00"`). 없음은 [`NONE_TEXT`].
 */
export function moneyText(value: string | null, currency: string): string {
  if (value === null) return NONE_TEXT;
  const usd = currency === "USD";
  // 원은 원 단위(소수 없음)로 반올림한다. 그 밖의 모르는 통화는 소수 둘째 자리 + 통화 글자.
  const parts = rounded(value, currency === "KRW" ? 0 : 2);
  if (!parts) return value;
  const sign = parts.negative ? "-" : "";
  if (currency === "KRW") return `${sign}${parts.whole}원`;
  if (usd) return `${sign}$${parts.whole}.${parts.fraction}`;
  return `${sign}${parts.whole}.${parts.fraction} ${currency}`;
}

/** 부호를 붙인 돈 모양(손익): 양수는 앞에 "+", 음수는 "-", 0은 부호 없음. 없음은 [`NONE_TEXT`]. */
export function signedMoneyText(value: string | null, currency: string): string {
  if (value === null) return NONE_TEXT;
  const text = moneyText(value, currency);
  return trendOf(value) === "up" ? `+${text}` : text;
}

/** 퍼센트 글: 소수 둘째 자리까지 + "%"(`"1.5"` → `"1.50%"`, `"0"` → `"0.00%"`). 없음은 [`NONE_TEXT`]. */
export function percentText(value: string | null): string {
  if (value === null) return NONE_TEXT;
  const parts = rounded(value, 2);
  if (!parts) return value;
  return `${parts.negative ? "-" : ""}${parts.whole}.${parts.fraction}%`;
}

/** 손익의 오름내림: 양수 up · 음수 down · 0 flat · 없음은 없음. */
export function trendOf(value: string | null): Trend | null {
  if (value === null) return null;
  const digits = digitsOf(value);
  if (!digits) return null;
  if (/^0*$/.test(digits.whole + digits.fraction)) return "flat";
  return digits.negative ? "down" : "up";
}

/**
 * 낙폭 막대의 비율 = 낙폭 ÷ 한도, 0~1로 자른다(한도를 넘으면 1, 음수면 0). 한도가 없거나 0 이하면 없음.
 */
export function drawdownRatio(drawdownPct: string, limitPct: string | null): number | null {
  if (limitPct === null) return null;
  const limit = Number(limitPct);
  const drawdown = Number(drawdownPct);
  if (!Number.isFinite(limit) || limit <= 0 || !Number.isFinite(drawdown)) return null;
  return Math.min(1, Math.max(0, drawdown / limit));
}

/**
 * 보유 표의 줄(응답 차례): 수량은 글자 그대로, 평균 단가 · 지금 값 · 평가는 [`moneyText`], 평가 손익은
 * [`signedMoneyText`](모두 그 종목의 통화), `trend`는 평가 손익의 [`trendOf`].
 */
export function holdingRows(account: AccountReply): HoldingRow[] {
  return (account.holdings ?? []).map((line) => ({
    symbol: line.symbol,
    quantity: line.quantity,
    average: moneyText(line.average_price, line.currency),
    last: moneyText(line.last_price, line.currency),
    value: moneyText(line.value, line.currency),
    pnl: signedMoneyText(line.pnl, line.currency),
    trend: trendOf(line.pnl),
  }));
}

/** 몸통의 글자 칸 하나(글자가 아니면 없음). */
function textField(body: unknown, name: string): string | null {
  if (typeof body !== "object" || body === null) return null;
  const value = (body as Record<string, unknown>)[name];
  return typeof value === "string" ? value : null;
}

/**
 * 다시 시작 응답을 결과로 읽는다:
 * - 200 `{"state": "needs_approval", "number"}` → 승인이 필요함(승인 번호).
 * - 그 밖의 200 `{"state", "notice"?}` → 다시 시작됨(바뀐 상태 · 함께 온 안내).
 * - 그 밖(409 `{"error", "message"?}` 등) → 거절: 이유는 `message`가 있으면 그 글, `error`가
 *   `"not_stopped"`면 "팀이 멈춰 있지 않습니다", 그 밖은 "서버가 받지 않았습니다".
 */
export function restartResult(status: number, body: unknown): RestartResult {
  const state = textField(body, "state");
  if (status === 200 && state !== null) {
    if (state === "needs_approval") return { kind: "needs_approval", number: textField(body, "number") ?? "" };
    return { kind: "restarted", state, notice: textField(body, "notice") };
  }
  const message = textField(body, "message");
  if (message !== null) return { kind: "refused", reason: message };
  if (textField(body, "error") === "not_stopped") return { kind: "refused", reason: "팀이 멈춰 있지 않습니다" };
  return { kind: "refused", reason: "서버가 받지 않았습니다" };
}

/**
 * 다시 시작 결과 글: 다시 시작됨은 "다시 시작했습니다 — 지금 상태: <상태 글>"(안내가 있으면 " · <안내>"를 잇는다),
 * 승인이 필요함은 "승인이 필요합니다 — 승인 번호 <번호>", 거절은 "다시 시작하지 못했습니다 — <이유>".
 */
export function restartText(result: RestartResult): string {
  switch (result.kind) {
    case "restarted": {
      const base = `다시 시작했습니다 — 지금 상태: ${teamStateText(result.state)}`;
      return result.notice ? `${base} · ${result.notice}` : base;
    }
    case "needs_approval":
      return `승인이 필요합니다 — 승인 번호 ${result.number}`;
    case "refused":
      return `다시 시작하지 못했습니다 — ${result.reason}`;
  }
}

/**
 * "전체 정지" 단추를 보이는가(server 40차 2번): 전체 정지가 아닌 모든 상태(모르는 글자 포함)에서 보이고, 이미 전체
 * 정지면 보이지 않는다(그때는 지금의 "다시 시작"만 보인다 — [`canRestart`]).
 */
export function canStop(state: string): boolean {
  return state !== "full_stop";
}

/**
 * 전체 정지 응답을 결과로 읽는다(server 40차 1 · 2번):
 * - 200 `{"state"}` → 정지됨(바뀐 상태 글자).
 * - 409 `{"error": "already_stopped"}` → 거절, 이유 "이미 전체 정지입니다".
 * - 그 밖(`message`가 있으면 그 글, 없으면 "서버가 받지 않았습니다") → 거절.
 */
export function stopResult(status: number, body: unknown): StopResult {
  const state = textField(body, "state");
  if (status === 200 && state !== null) return { kind: "stopped", state };
  if (status === 409 && textField(body, "error") === "already_stopped") {
    return { kind: "refused", reason: "이미 전체 정지입니다" };
  }
  const message = textField(body, "message");
  if (message !== null) return { kind: "refused", reason: message };
  return { kind: "refused", reason: "서버가 받지 않았습니다" };
}

/**
 * 전체 정지 결과 글: 정지됨은 "전체 정지했습니다 — 지금 상태: <상태 글>", 거절은 "전체 정지하지 못했습니다 — <이유>".
 */
export function stopText(result: StopResult): string {
  return result.kind === "stopped"
    ? `전체 정지했습니다 — 지금 상태: ${teamStateText(result.state)}`
    : `전체 정지하지 못했습니다 — ${result.reason}`;
}
