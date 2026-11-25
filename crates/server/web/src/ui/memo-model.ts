// 결정 메모(어제 메모) 순수 모델(additions.md 23번 · R7-4 · M11 · C25 · E54 · V70): DOM에 닿지 않는다.
// 서버(`GET /api/memos`)가 정한 순서 · 상태 · 결과 판정 · 수익률을 글로만 바꾼다(계산하지 않는다). 사무실의 어제 메모
// 카드와 히스토리의 결정 메모 탭이 함께 쓴다.

/** 메모 상태(서버 글자). */
export type MemoState = "pending" | "settled" | "unsettled";
/** 결과 판정(서버 글자): 맞음 · 틀림 · 판단 못 함. */
export type MemoVerdict = "right" | "wrong" | "unknown";

/** 서버가 준 메모 하나(C25). 없는 칸은 빠지거나 `null`이다. */
export interface Memo {
  id?: number;
  trade: number;
  date: string;
  symbol: string;
  decision: "buy" | "sell";
  state: MemoState;
  return_pct?: number | null;
  excess_pct?: number | null;
  hold_days?: number | null;
  lesson?: string | null;
  reason?: string | null;
  verdict?: MemoVerdict | null;
  unsettled_reason?: string | null;
  settled_at?: number | null;
}

/** 히스토리 탭이 읽는 쪽. */
export interface MemoPage {
  memos: Memo[];
  next: number | null;
}

/** 화면에 그릴 줄 하나(글자만). */
export interface MemoRow {
  id: number;
  trade: number;
  date: string;
  symbol: string;
  decision: string;
  stateWord: MemoState;
  state: string;
  verdict: MemoVerdict | "";
  result: string;
  returnText: string;
  excessText: string;
  holdText: string;
  lesson: string;
  reason: string;
  why: string;
}

/** 어제 메모 카드에 보이는 정산된 메모의 가장 많은 수. */
export const CARD_MAX = 3;

const STATE_TEXT: Record<MemoState, string> = { pending: "정산 대기", settled: "정산됨", unsettled: "정산 못 함" };
const VERDICT_TEXT: Record<MemoVerdict, string> = { right: "맞음", wrong: "틀림", unknown: "판단 못 함" };
const DECISION_TEXT: Record<Memo["decision"], string> = { buy: "사기", sell: "팔기" };
const WHY_TEXT: Record<string, string> = { no_price: "시세 없음" };

/** 백분율 수 → `+1.23%`(0은 부호 없음). 없으면 빈 글. */
export function pctText(value: number | null | undefined): string {
  if (typeof value !== "number" || !Number.isFinite(value)) return "";
  const sign = value > 0 ? "+" : value < 0 ? "-" : "";
  return `${sign}${Math.abs(value).toFixed(2)}%`;
}

/** 메모 하나 → 줄. */
export function memoRow(memo: Memo): MemoRow {
  const verdict = memo.verdict ?? "";
  const why = memo.unsettled_reason ? (WHY_TEXT[memo.unsettled_reason] ?? memo.unsettled_reason) : "";
  return {
    id: memo.id ?? memo.trade,
    trade: memo.trade,
    date: memo.date,
    symbol: memo.symbol,
    decision: DECISION_TEXT[memo.decision] ?? memo.decision,
    stateWord: memo.state,
    state: STATE_TEXT[memo.state] ?? memo.state,
    verdict,
    result: verdict ? VERDICT_TEXT[verdict] : "",
    returnText: pctText(memo.return_pct),
    excessText: pctText(memo.excess_pct),
    holdText: typeof memo.hold_days === "number" ? `${memo.hold_days}거래일` : "",
    lesson: memo.lesson ?? "",
    reason: memo.reason ?? "",
    why,
  };
}

/** 쪽의 줄들(서버 순서). */
export function memoRows(page: MemoPage): MemoRow[] {
  return page.memos.map(memoRow);
}

/** 카드에 보일 줄: 정산된 메모만 서버 순서대로 `max`개까지. */
export function cardMemos(list: readonly Memo[], max: number = CARD_MAX): MemoRow[] {
  return list
    .filter((memo) => memo.state === "settled")
    .slice(0, max)
    .map(memoRow);
}

/** 정산 대기 메모 수. */
export function pendingCount(list: readonly Memo[]): number {
  return list.filter((memo) => memo.state === "pending").length;
}

function isMemo(value: unknown): value is Memo {
  if (typeof value !== "object" || value === null) return false;
  const memo = value as Record<string, unknown>;
  return (
    typeof memo.trade === "number" &&
    typeof memo.date === "string" &&
    typeof memo.symbol === "string" &&
    typeof memo.state === "string"
  );
}

/** `?days=` 응답(배열)을 읽는다. 모양이 틀리면 던진다. */
export function readMemoList(body: unknown): Memo[] {
  if (!Array.isArray(body) || !body.every(isMemo)) throw new Error("결정 메모 목록 모양이 아니다");
  return body;
}

/** `?before=&max=` 응답(쪽)을 읽는다. 모양이 틀리면 던진다. */
export function readMemoPage(body: unknown): MemoPage {
  if (typeof body !== "object" || body === null || Array.isArray(body)) throw new Error("결정 메모 쪽 모양이 아니다");
  const page = body as Record<string, unknown>;
  const next = page.next;
  if (!Array.isArray(page.memos) || !page.memos.every(isMemo)) throw new Error("결정 메모 쪽 모양이 아니다");
  if (next !== null && next !== undefined && typeof next !== "number") throw new Error("결정 메모 쪽 모양이 아니다");
  return { memos: page.memos, next: typeof next === "number" ? next : null };
}
