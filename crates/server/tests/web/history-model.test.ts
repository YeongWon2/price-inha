// 히스토리 화면의 화면 모델(server 29차 · C30 · V83 · V84 · F7-7): 상태 글(스펙의 이름) · 한국 시각 글 ·
// 그 시장 돈 모양 · 근거 요약 줄 · 상태 흐름 · 목록 줄.
import { describe, expect, it } from "vitest";
import {
  APPROVAL_STATUSES,
  NONE_TEXT,
  TRADE_STATES,
  approvalRows,
  approvalStatusLabel,
  basisLines,
  flowRows,
  moneyText,
  sideText,
  timeText,
  tradeRows,
  tradeStateLabel,
} from "../../web/src/history/model";
import { MONDAY_10, approval, detailOf, emptyDetailOf, trade } from "./history-fixture";

describe("히스토리 모델 · 글", () => {
  it("매매 상태 글은 스펙의 매매 상태 이름이다", () => {
    // 검증: V83 C30 F7
    expect(TRADE_STATES.map(tradeStateLabel)).toEqual([
      "논의중",
      "판단됨",
      "승인대기",
      "막힘",
      "주문됨",
      "보유중",
      "팔림",
      "복기됨",
    ]);
    expect(tradeStateLabel("someday")).toBe("someday");
  });

  it("승인 요청 결과 글은 기다림 · 승인 · 거절 · 답 없음 · 만료다", () => {
    // 검증: V84 C30
    expect(APPROVAL_STATUSES.map(approvalStatusLabel)).toEqual(["기다림", "승인", "거절", "답 없음", "만료"]);
    expect(approvalStatusLabel("odd")).toBe("odd");
  });

  it("매매 방향 글은 사기 · 팔기이고 없으면 없음이다", () => {
    // 검증: V83 F7
    expect(sideText("buy")).toBe("사기");
    expect(sideText("sell")).toBe("팔기");
    expect(sideText(null)).toBe(NONE_TEXT);
  });

  it("시각은 한국 시각 글이다(날짜가 넘어가는 자리 포함)", () => {
    // 검증: V83 V84 C30
    expect(timeText(MONDAY_10)).toBe("2026-11-16 10:00");
    expect(timeText(Date.UTC(2026, 10, 16, 1, 5, 30))).toBe("2026-11-16 10:05");
    expect(timeText(Date.UTC(2026, 10, 15, 15, 0))).toBe("2026-11-16 00:00");
    expect(timeText(Date.UTC(2026, 10, 15, 14, 59))).toBe("2026-11-15 23:59");
    expect(timeText(Date.UTC(2026, 0, 1, 0, 0))).toBe("2026-01-01 09:00");
  });

  it("금액은 그 시장의 돈 모양이다(원화 · 달러), 없으면 없음", () => {
    // 검증: V83 C30 F7
    expect(moneyText("100000", "KRW")).toBe("100,000원");
    expect(moneyText("-5000", "KRW")).toBe("-5,000원");
    expect(moneyText("950", "KRW")).toBe("950원");
    expect(moneyText("1234.5", "USD")).toBe("$1,234.50");
    expect(moneyText("-12", "USD")).toBe("-$12.00");
    expect(moneyText("0.07", "USD")).toBe("$0.07");
    expect(moneyText(null, "KRW")).toBe(NONE_TEXT);
    expect(moneyText(null, "USD")).toBe(NONE_TEXT);
  });
});

describe("히스토리 모델 · 줄", () => {
  it("매매 목록의 줄은 응답 차례이고 상태 글 · 시각 글이 있다", () => {
    // 검증: V83 C30
    const rows = tradeRows({ trades: [trade(3, "judged"), trade(2, "discussing")], next: 2 });
    expect(rows.map((row) => row.id)).toEqual([3, 2]);
    expect(rows[0]).toMatchObject({ symbol: "035420", state: "judged", stateLabel: "판단됨" });
    expect(rows[0].created).toBe(timeText(trade(3).created_at));
    expect(rows[0].updated).toBe(timeText(trade(3).updated_at));
    expect(rows[0].decided).toBe(timeText(trade(3, "judged").decided_at as number));
    expect(rows[1]).toMatchObject({ stateLabel: "논의중", decided: null });
  });

  it("상태 흐름은 시각 순서의 상태 글과 시각 글이다", () => {
    // 검증: V83 C30 F7
    const detail = detailOf(trade(4));
    expect(flowRows(detail)).toEqual([
      { state: "discussing", label: "논의중", at: timeText(detail.created_at) },
      { state: "judged", label: "판단됨", at: timeText(detail.created_at + 1_000) },
      { state: "awaiting_approval", label: "승인대기", at: timeText(detail.created_at + 2_000) },
    ]);
  });

  it("근거 요약 줄은 가설 · 계기 · 진입 · 손절 · 목표 · 담당 점수 · 자료 차례이고 금액은 돈 모양이다", () => {
    // 검증: V83 R6-3 F7 C30
    const lines = basisLines(detailOf(trade(4)));
    expect(lines.map((line) => line.key)).toEqual(["hypothesis", "trigger", "entry", "stop", "target", "scores", "facts"]);
    expect(lines.map((line) => line.label)).toEqual(["가설", "계기", "진입", "손절", "목표", "담당 점수", "자료"]);
    const text = Object.fromEntries(lines.map((line) => [line.key, line.text]));
    expect(text.hypothesis).toBe("실적 개선");
    expect(text.trigger).toBe("거래량 증가");
    expect(text.entry).toBe("100,000원");
    expect(text.stop).toBe("95,000원");
    expect(text.target).toBe("110,000원");
    expect(text.scores).toBe("risk 70점 — 근거 충분");
    expect(text.facts).toBe("주요사항보고서(자기주식취득결정) · 시세");
  });

  it("근거 요약 줄: 담당이 여럿이면 잇고, 자료 이름은 겹치면 한 번, 달러 매매는 달러 모양", () => {
    // 검증: V83 F7
    const base = detailOf(trade(6));
    const detail = {
      ...base,
      currency: "USD",
      facts: ["실적 발표", "시세"],
      basis: {
        ...base.basis,
        entry: "123.4",
        stop: "120",
        target: "130.25",
        scores: [
          { role: "risk", score: 70, reason: "근거 충분" },
          { role: "analyst", score: 60, reason: "실적 좋음" },
        ],
        sources: [
          { name: "시세", known_at: base.created_at },
          { name: "환율", known_at: base.created_at },
        ],
      },
    };
    const text = Object.fromEntries(basisLines(detail).map((line) => [line.key, line.text]));
    expect(text.entry).toBe("$123.40");
    expect(text.stop).toBe("$120.00");
    expect(text.target).toBe("$130.25");
    expect(text.scores).toBe("risk 70점 — 근거 충분 · analyst 60점 — 실적 좋음");
    expect(text.facts).toBe("실적 발표 · 시세 · 환율");
  });

  it("판단 전 매매의 근거 요약 줄은 일곱 줄 모두 없음이다", () => {
    // 검증: V83 F7
    const lines = basisLines(emptyDetailOf(trade(5)));
    expect(lines.length).toBe(7);
    for (const line of lines) expect(line.text, line.key).toBe(NONE_TEXT);
  });

  it("승인 기록의 줄은 응답 차례이고 결과 글 · 시각 글 · 가린 요약 그대로다", () => {
    // 검증: V84 C30
    const rows = approvalRows({ approvals: [approval(9, "rejected"), approval(8)], next: null });
    expect(rows.map((row) => row.id)).toEqual([9, 8]);
    expect(rows[0]).toMatchObject({
      number: "A-0009",
      status: "rejected",
      statusLabel: "거절",
      summary: approval(9).summary,
      created: timeText(approval(9).created_at),
      resolved: timeText(approval(9, "rejected").resolved_at as number),
      consumed: null,
    });
    expect(rows[1]).toMatchObject({ statusLabel: "기다림", resolved: null });
  });
});
