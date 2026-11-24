// 계좌 · 운용 화면의 화면 모델(server 30차 · C31 · V85 · V86 · R3-1 · R3-3 · R14-1 · R16-5 · F1-6): 팀 상태 글 · 오늘
// 매매/쉼과 쉬는 이유 · 다시 시작 단추를 보이는 상태 · 돈과 퍼센트 모양 · 손익 부호와 오름내림 · 낙폭 막대 비율 ·
// 보유 표 줄 · 다시 시작 결과 읽기.
import { describe, expect, it } from "vitest";
import {
  NONE_TEXT,
  PAPER_NOTICE,
  REFRESH_KINDS,
  REFRESH_MS,
  TEAM_STATE_TEXT,
  canRestart,
  drawdownRatio,
  holdingRows,
  moneyText,
  percentText,
  planText,
  restReason,
  restartResult,
  restartText,
  signedMoneyText,
  teamStateText,
  teamTone,
  trendOf,
} from "../../web/src/account/model";
import { account, holding, team } from "./account-fixture";

describe("팀 상태", () => {
  it("팀 상태 글자는 운용 중 · 쉼 · 하루 한도 멈춤 · 이상 감지 멈춤 · 전체 정지로 보이고 모르는 글자는 그대로다", () => {
    // 검증: V86 R3-1 F1-6
    expect(teamStateText("operating")).toBe("운용 중");
    expect(teamStateText("resting")).toBe("쉼");
    expect(teamStateText("daily_limit_halt")).toBe("하루 한도 멈춤");
    expect(teamStateText("anomaly_halt")).toBe("이상 감지 멈춤");
    expect(teamStateText("full_stop")).toBe("전체 정지");
    expect(teamStateText("flying")).toBe("flying");
    for (const [state, text] of Object.entries(TEAM_STATE_TEXT)) expect(teamStateText(state)).toBe(text);
  });

  it("상태 배지 빛깔: 운용 중 success · 쉼 neutral · 멈춤 warning · 전체 정지 danger", () => {
    // 검증: V86 R3-1
    expect(teamTone("operating")).toBe("success");
    expect(teamTone("resting")).toBe("neutral");
    expect(teamTone("daily_limit_halt")).toBe("warning");
    expect(teamTone("anomaly_halt")).toBe("warning");
    expect(teamTone("full_stop")).toBe("danger");
    expect(teamTone("flying")).toBe("neutral");
  });

  it("오늘 매매할지 쉴지와 쉬는 이유(F1-6)", () => {
    // 검증: V86 F1-6
    expect(planText(team("operating", "trade"))).toBe("오늘 매매합니다");
    expect(planText(team("resting", "rest", "변동성이 너무 큼"))).toBe("오늘 쉽니다");
    expect(planText(team("resting"))).toBe("오늘 할 일을 아직 정하지 않았습니다");
    expect(restReason(team("resting", "rest", "변동성이 너무 큼"))).toBe("변동성이 너무 큼");
    expect(restReason(team("operating", "trade"))).toBeNull();
    expect(restReason(team("resting"))).toBeNull();
    expect(restReason(team("resting", "rest", null))).toBeNull();
  });

  it("다시 시작 단추는 전체 정지일 때만 보인다(사람만 다시 시작한다)", () => {
    // 검증: V85 R3-3 R14-1
    expect(canRestart("full_stop")).toBe(true);
    for (const state of ["operating", "resting", "daily_limit_halt", "anomaly_halt", "flying"]) {
      expect(canRestart(state), state).toBe(false);
    }
  });
});

describe("금액 · 퍼센트 모양", () => {
  it("돈은 그 통화 모양(원은 쉼표 + 원, 달러는 $ + 소수 둘째 자리), 없음은 '없음'", () => {
    // 검증: V86 R16-5
    expect(moneyText("10000000", "KRW")).toBe("10,000,000원");
    expect(moneyText("950", "KRW")).toBe("950원");
    expect(moneyText("-25075", "KRW")).toBe("-25,075원");
    expect(moneyText("1234.5", "USD")).toBe("$1,234.50");
    expect(moneyText("-12", "USD")).toBe("-$12.00");
    expect(moneyText(null, "KRW")).toBe(NONE_TEXT);
  });

  it("손익은 부호를 붙인다: 양수 +, 음수 -, 0은 부호 없음", () => {
    // 검증: V86 R16-5
    expect(signedMoneyText("12000", "KRW")).toBe("+12,000원");
    expect(signedMoneyText("-25075", "KRW")).toBe("-25,075원");
    expect(signedMoneyText("0", "KRW")).toBe("0원");
    expect(signedMoneyText("3.5", "USD")).toBe("+$3.50");
    expect(signedMoneyText(null, "USD")).toBe(NONE_TEXT);
  });

  it("오름내림: 양수 up · 음수 down · 0 flat · 없음은 null", () => {
    // 검증: V86
    expect(trendOf("12000")).toBe("up");
    expect(trendOf("-1")).toBe("down");
    expect(trendOf("0")).toBe("flat");
    expect(trendOf("0.00")).toBe("flat");
    expect(trendOf(null)).toBeNull();
  });

  it("퍼센트는 소수 둘째 자리까지", () => {
    // 검증: V86 R16-5
    expect(percentText("1.5")).toBe("1.50%");
    expect(percentText("0")).toBe("0.00%");
    expect(percentText("12.345")).toBe("12.35%");
    expect(percentText(null)).toBe(NONE_TEXT);
  });
});

describe("낙폭 막대", () => {
  it("비율은 낙폭 ÷ 한도이고 0~1로 자른다, 한도를 모르거나 0이면 막대가 없다", () => {
    // 검증: V86 R16-5
    expect(drawdownRatio("5", "20")).toBeCloseTo(0.25, 10);
    expect(drawdownRatio("0", "20")).toBe(0);
    expect(drawdownRatio("20", "20")).toBe(1);
    expect(drawdownRatio("30", "20")).toBe(1);
    expect(drawdownRatio("-1", "20")).toBe(0);
    expect(drawdownRatio("5", null)).toBeNull();
    expect(drawdownRatio("5", "0")).toBeNull();
  });
});

describe("보유 표", () => {
  it("줄마다 수량 · 평균 단가 · 지금 값 · 평가 · 부호 붙은 평가 손익 · 오름내림(응답 차례)", () => {
    // 검증: V86 R3-1 R16-5
    const rows = holdingRows(
      account([
        holding("000660"),
        holding("035420", { quantity: "2", average_price: "200000", last_price: "210000", value: "420000", pnl: "20000" }),
      ]),
    );
    expect(rows.map((row) => row.symbol)).toEqual(["000660", "035420"]);
    expect(rows[0]).toEqual({
      symbol: "000660",
      quantity: "5",
      average: "100,015원",
      last: "95,000원",
      value: "475,000원",
      pnl: "-25,075원",
      trend: "down",
    });
    expect(rows[1].pnl).toBe("+20,000원");
    expect(rows[1].trend).toBe("up");
  });

  it("받은 시세가 없으면 지금 값 · 평가 · 손익은 '없음'이고 오름내림이 없다", () => {
    // 검증: V86
    const rows = holdingRows(account([holding("000660", { last_price: null, value: null, pnl: null })]));
    expect(rows[0].last).toBe(NONE_TEXT);
    expect(rows[0].value).toBe(NONE_TEXT);
    expect(rows[0].pnl).toBe(NONE_TEXT);
    expect(rows[0].trend).toBeNull();
    expect(holdingRows(account([]))).toEqual([]);
  });
});

describe("다시 시작 결과", () => {
  it("200 상태는 다시 시작됨(함께 온 안내까지), 승인 대기는 승인 번호, 409는 거절 이유", () => {
    // 검증: V85 R3-3 R14-1 E9
    const done = restartResult(200, { state: "resting" });
    expect(done).toEqual({ kind: "restarted", state: "resting", notice: null });
    expect(restartText(done)).toBe("다시 시작했습니다 — 지금 상태: 쉼");

    const noticed = restartResult(200, { state: "resting", notice: "템플릿을 먼저 고르세요" });
    expect(noticed).toEqual({ kind: "restarted", state: "resting", notice: "템플릿을 먼저 고르세요" });
    expect(restartText(noticed)).toBe("다시 시작했습니다 — 지금 상태: 쉼 · 템플릿을 먼저 고르세요");

    const waiting = restartResult(200, { state: "needs_approval", number: "A-0042" });
    expect(waiting).toEqual({ kind: "needs_approval", number: "A-0042" });
    expect(restartText(waiting)).toBe("승인이 필요합니다 — 승인 번호 A-0042");

    const keys = restartResult(409, { error: "keys_required", message: "증권사 키와 AI 키를 먼저 다시 등록해야 합니다" });
    expect(keys).toEqual({ kind: "refused", reason: "증권사 키와 AI 키를 먼저 다시 등록해야 합니다" });
    expect(restartText(keys)).toBe("다시 시작하지 못했습니다 — 증권사 키와 AI 키를 먼저 다시 등록해야 합니다");

    expect(restartResult(409, { error: "not_stopped" })).toEqual({ kind: "refused", reason: "팀이 멈춰 있지 않습니다" });
    expect(restartResult(500, null)).toEqual({ kind: "refused", reason: "서버가 받지 않았습니다" });
  });
});

describe("화면 상수", () => {
  it("모의계좌 안내 · 다시 읽는 간격 · 다시 읽게 하는 이벤트 종류", () => {
    // 검증: V86 R3-1
    expect(PAPER_NOTICE).toContain("모의계좌");
    expect(PAPER_NOTICE).toContain("실제 주문은 나가지 않습니다");
    expect(REFRESH_MS).toBe(10_000);
    expect(REFRESH_KINDS.length).toBeGreaterThan(0);
  });
});
