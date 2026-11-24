// 템플릿 화면의 화면 모델(server 27차 3번 · C28 · F6 · E5 · E28 · E58 · V79~V81): 응답 → 칸 표 · 승격 기준 표 ·
// 판 이력 줄, 저장 결과 문구, 실전 안내 문구. 순수 함수.
import { describe, expect, it } from "vitest";
import {
  LIVE_NOTICE,
  STYLES,
  canPromote,
  canStop,
  capText,
  currentTemplate,
  fieldLabel,
  fieldRows,
  formatValue,
  promotionRows,
  revisionRows,
  saveMessage,
  stateLabel,
} from "../../web/src/templates/model";
import { replyWith, swingEntry } from "./template-fixture";

describe("칸 표", () => {
  it("응답의 칸이 순서대로 줄이 되고 이름 · 값 · 상한이 글로 보인다", () => {
    // 검증: V79 R10-1 C28
    const rows = fieldRows(swingEntry(1, "simulating"));
    expect(rows.map((row) => row.key)).toEqual(["risk_per_trade_pct", "daily_loss_limit_pct", "max_positions"]);
    expect(rows[0]).toEqual({ key: "risk_per_trade_pct", label: "1회 위험 비율", value: "1%", cap: "상한 2%", locked: false });
    expect(rows[1].cap).toBe("상한 5%");
    expect(rows[2].value).toBe("5");
    expect(rows[2].cap).toBeNull();
  });

  it("바꿀 수 없는 칸(승격대기 · 실전중 · 멈춤)은 잠김이다", () => {
    // 검증: V79 E28 R10-2
    for (const state of ["promotion_pending", "live", "stopped"] as const) {
      for (const row of fieldRows(swingEntry(1, state))) expect(row.locked, `${state}: ${row.key}`).toBe(true);
    }
    for (const row of fieldRows(swingEntry(1, "simulating"))) expect(row.locked).toBe(false);
  });

  it("값 · 상한 글 만들기: 퍼센트 칸은 %, 없음은 '없음', 모르는 키는 키 그대로", () => {
    // 검증: V80 E5
    expect(formatValue("risk_per_trade_pct", "1.5")).toBe("1.5%");
    expect(formatValue("max_positions", 5)).toBe("5");
    expect(formatValue("conviction_weight_pct", null)).toBe("없음");
    expect(capText("2")).toBe("상한 2%");
    expect(capText(null)).toBeNull();
    expect(fieldLabel("risk_per_trade_pct")).toBe("1회 위험 비율");
    expect(fieldLabel("no_such_field")).toBe("no_such_field");
  });

  it("쓰는 템플릿은 current의 번호이고, 없으면 없음", () => {
    // 검증: V79 C28
    expect(currentTemplate(replyWith(null))).toBeNull();
    const entry = swingEntry(4, "simulating");
    expect(currentTemplate({ current: 4, templates: [swingEntry(2, "stopped"), entry] })?.id).toBe(4);
  });

  it("상태 배지 글과 스타일 단추(단타 · 스윙 · 중장기)", () => {
    // 검증: V79 R10-1 F6
    expect(stateLabel("simulating")).toBe("모의중");
    expect(stateLabel("promotion_pending")).toBe("승격대기");
    expect(stateLabel("live")).toBe("실전중");
    expect(stateLabel("stopped")).toBe("멈춤");
    expect(STYLES.map(([style, label]) => [style, label])).toEqual([
      ["day_trade", "단타"],
      ["swing", "스윙"],
      ["long_term", "중장기"],
    ]);
  });
});

describe("승격 기준 표", () => {
  it("채우지 못한 기준은 모자란 줄이고 아직 없는 값은 '아직 없음'", () => {
    // 검증: V81 E58 R11-2
    const rows = promotionRows(swingEntry(1, "simulating"));
    expect(rows.map((row) => row.criterion)).toEqual(["trades", "trading_days", "net_pnl", "max_drawdown", "out_of_sample"]);
    expect(rows.filter((row) => row.short).map((row) => row.criterion)).toEqual(["trades", "net_pnl", "out_of_sample"]);
    for (const row of rows) {
      expect(row.short).toBe(!row.met);
      expect(row.label.length, row.criterion).toBeGreaterThan(0);
      expect(row.label, row.criterion).not.toBe(row.criterion);
    }
    expect(rows[0].need).toBe("30");
    expect(rows[0].have).toBe("12");
    expect(rows[4].have).toBe("아직 없음");
  });
});

describe("판 이력", () => {
  it("판마다 바뀐 칸 · 앞뒤 값 · 넓힘 표시와 개선안 번호", () => {
    // 검증: V81 R11-2 E28
    const rows = revisionRows(swingEntry(1, "live"));
    expect(rows.map((row) => row.revision)).toEqual([1, 2, 3]);
    expect(rows[0].changes).toEqual([]);
    expect(rows[1].changes).toEqual([
      { key: "risk_per_trade_pct", label: "1회 위험 비율", before: "1.5%", after: "1%", widened: false },
    ]);
    expect(rows[2].changes[0].widened).toBe(true);
    expect(rows[2].changes[0].before).toBe("3%");
    expect(rows[2].proposal).toBe(9);
    expect(rows[2].at).toBe(1_763_340_120_000);
  });
});

describe("단추를 누를 수 있는가", () => {
  it("실전 운용 시작은 승격대기(승인 번호 있음)일 때만, 멈추기는 실전중일 때만", () => {
    // 검증: E58 V81 F6
    expect(canPromote(null)).toBe(false);
    expect(canPromote(swingEntry(1, "promotion_pending"))).toBe(true);
    expect(canPromote({ ...swingEntry(1, "promotion_pending"), approval: null })).toBe(false);
    for (const state of ["simulating", "live", "stopped"] as const) expect(canPromote(swingEntry(1, state)), state).toBe(false);
    expect(canStop(null)).toBe(false);
    expect(canStop(swingEntry(1, "live"))).toBe(true);
    for (const state of ["simulating", "promotion_pending", "stopped"] as const) expect(canStop(swingEntry(1, state)), state).toBe(false);
  });
});

describe("값 저장 결과 문구", () => {
  const rows = () => fieldRows(swingEntry(1, "simulating"));

  it("저장되면 새 판 번호를 알린다", () => {
    // 검증: V79 C28
    const message = saveMessage({ status: 200, body: { id: 1, revision: 4, state: "simulating" } }, rows());
    expect(message.kind).toBe("saved");
    expect(message.text).toContain("4");
  });

  it("상한을 넘으면 저장되지 않았다고 하고 그 칸 옆에 상한 안내를 단다", () => {
    // 검증: V80 E5 R10-2
    const message = saveMessage({ status: 400, body: { error: "over_cap", fields: ["risk_per_trade_pct"] } }, rows());
    expect(message.kind).toBe("over_cap");
    if (message.kind !== "over_cap") return;
    expect(message.fields).toEqual([{ key: "risk_per_trade_pct", text: "상한 2%" }]);
    expect(message.text).toContain("저장");
    expect(message.text).toContain("상한");
  });

  it("승인 대기면 '승인 뒤 다음 거래일부터'와 승인 번호를 보인다", () => {
    // 검증: E28 V81 R11-2
    const body = { state: "needs_approval", proposal: 3, widened: ["risk_per_trade_pct"], number: "AP-31" };
    const message = saveMessage({ status: 200, body }, rows());
    expect(message.kind).toBe("needs_approval");
    if (message.kind !== "needs_approval") return;
    expect(message.number).toBe("AP-31");
    expect(message.text).toContain("승인 뒤 다음 거래일부터");
    expect(message.text).toContain("AP-31");
  });

  it("잠긴 템플릿(409)과 그 밖의 실패는 따로 알린다", () => {
    // 검증: E28 E57
    expect(saveMessage({ status: 409, body: { error: "locked" } }, rows()).kind).toBe("locked");
    const failed = saveMessage({ status: 500, body: { error: "internal" } }, rows());
    expect(failed.kind).toBe("failed");
    expect(failed.text.length).toBeGreaterThan(0);
  });
});

describe("실전 안내", () => {
  it("실전 운용을 시작해도 실제 주문은 나가지 않는다는 뜻이다", () => {
    // 검증: E58 V81
    expect(LIVE_NOTICE).toContain("실제");
    expect(LIVE_NOTICE).toContain("주문");
    expect(LIVE_NOTICE).toContain("나가지 않");
  });
});
