// 결정 메모(어제 메모) 순수 모델(additions.md 23번 · R7-4 · M11 · C25 · E54 · V70): 서버가 내려 준 상태 · 판정 ·
// 수익률을 글로 바꾸기, 카드에 보일 정산된 메모 고르기(서버 순서 그대로), 정산 대기 수 세기, 쪽 응답 읽기.
import { describe, expect, it } from "vitest";
import {
  CARD_MAX,
  cardMemos,
  memoRow,
  memoRows,
  pctText,
  pendingCount,
  readMemoList,
  readMemoPage,
  type Memo,
} from "../../web/src/ui/memo-model";

function settled(trade: number, over: Partial<Memo> = {}): Memo {
  return {
    id: trade,
    trade,
    date: "2026-11-21",
    symbol: "000660",
    decision: "buy",
    state: "settled",
    return_pct: 10,
    hold_days: 5,
    lesson: `교훈 ${trade}. 둘째 문장.`,
    reason: "실적 개선",
    verdict: "right",
    ...over,
  };
}

function pending(trade: number): Memo {
  return { id: trade, trade, date: "2026-11-25", symbol: "005930", decision: "sell", state: "pending" };
}

describe("결정 메모 모델 · 글", () => {
  it("수익률은 부호와 소수 둘째 자리까지, 없으면 빈 글이다", () => {
    // 검증: V70
    expect(pctText(10)).toBe("+10.00%");
    expect(pctText(-5)).toBe("-5.00%");
    expect(pctText(0)).toBe("0.00%");
    expect(pctText(1.23)).toBe("+1.23%");
    expect(pctText(null)).toBe("");
    expect(pctText(undefined)).toBe("");
  });

  it("정산된 메모 줄은 결정 · 상태 · 결과 · 수익률 · 보유일 · 교훈을 글로 보인다", () => {
    // 검증: V70 R7-4
    const row = memoRow(settled(3, { excess_pct: 2.5 }));
    expect(row).toMatchObject({
      trade: 3,
      date: "2026-11-21",
      symbol: "000660",
      decision: "사기",
      state: "정산됨",
      result: "맞음",
      returnText: "+10.00%",
      excessText: "+2.50%",
      holdText: "5거래일",
      lesson: "교훈 3. 둘째 문장.",
      reason: "실적 개선",
      why: "",
    });
    expect(memoRow(settled(4, { verdict: "wrong", decision: "sell", return_pct: -5 }))).toMatchObject({
      decision: "팔기",
      result: "틀림",
      returnText: "-5.00%",
    });
  });

  it("정산 대기는 결과 · 수익률이 비고 \"정산 대기\"로, 정산 못 함은 까닭과 \"판단 못 함\"으로 보인다", () => {
    // 검증: V70 E54
    expect(memoRow(pending(5))).toMatchObject({
      state: "정산 대기",
      result: "",
      returnText: "",
      holdText: "",
      lesson: "",
    });
    const lost: Memo = { ...pending(6), state: "unsettled", verdict: "unknown", unsettled_reason: "no_price" };
    expect(memoRow(lost)).toMatchObject({ state: "정산 못 함", result: "판단 못 함", why: "시세 없음" });
  });
});

describe("결정 메모 모델 · 카드 · 쪽", () => {
  it("카드는 정산된 메모만 서버 순서대로 많아야 셋을 고르고 정산 대기 수를 센다", () => {
    // 검증: V70 C25
    const list = [pending(9), settled(8), settled(7), pending(6), settled(5), settled(4)];
    expect(CARD_MAX).toBe(3);
    expect(cardMemos(list).map((row) => row.trade)).toEqual([8, 7, 5]);
    expect(pendingCount(list)).toBe(2);
    expect(cardMemos([pending(1)])).toEqual([]);
  });

  it("목록 · 쪽 응답을 읽고 모양이 틀리면 던진다", () => {
    // 검증: C25
    expect(readMemoList([settled(1)]).map((memo) => memo.trade)).toEqual([1]);
    expect(() => readMemoList({ memos: [] })).toThrow();
    const page = readMemoPage({ memos: [settled(2), settled(1)], next: 1 });
    expect(page.next).toBe(1);
    expect(memoRows(page).map((row) => row.trade)).toEqual([2, 1]);
    expect(readMemoPage({ memos: [], next: null }).next).toBeNull();
    expect(() => readMemoPage([settled(1)])).toThrow();
  });
});
