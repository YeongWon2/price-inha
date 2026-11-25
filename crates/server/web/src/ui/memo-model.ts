// 결정 메모(어제 메모) 순수 모델(뼈대).

export type MemoState = "pending" | "settled" | "unsettled";
export type MemoVerdict = "right" | "wrong" | "unknown";

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

export interface MemoPage {
  memos: Memo[];
  next: number | null;
}

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

export const CARD_MAX = 0;

export function pctText(_value: number | null | undefined): string {
  throw new Error("구현 전");
}

export function memoRow(_memo: Memo): MemoRow {
  throw new Error("구현 전");
}

export function memoRows(_page: MemoPage): MemoRow[] {
  throw new Error("구현 전");
}

export function cardMemos(_list: readonly Memo[], _max: number = CARD_MAX): MemoRow[] {
  throw new Error("구현 전");
}

export function pendingCount(_list: readonly Memo[]): number {
  throw new Error("구현 전");
}

export function readMemoList(_body: unknown): Memo[] {
  throw new Error("구현 전");
}

export function readMemoPage(_body: unknown): MemoPage {
  throw new Error("구현 전");
}
