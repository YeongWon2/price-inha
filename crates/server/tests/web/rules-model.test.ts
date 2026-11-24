// 규칙 화면의 화면 모델(server 28차 2번 · C29 · F5 · V82 · E28 · E35): 응답 → 규칙 표 줄(바뀜 · 냉각중 표시) ·
// 절대 상한 줄 · 개선안 줄(상태 · 실수 종류 · 앞 → 뒤 · 위험), 답하기 결과 문구. 순수 함수.
import { describe, expect, it } from "vitest";
import {
  answerMessage,
  capRows,
  mistakeLabel,
  proposalRows,
  proposalStateLabel,
  ruleRows,
} from "../../web/src/rules/model";
import { CAPS, noTemplate, proposal, proposalsOf, riskChange, rulesWith } from "./rules-fixture";

describe("규칙 표", () => {
  it("지금 규칙의 칸이 순서대로 줄이 되고 이름 · 값 · 상한이 글로 보인다", () => {
    // 검증: V82 C29 R7-3
    const rows = ruleRows(rulesWith("1"));
    expect(rows.map((row) => row.key)).toEqual(["risk_per_trade_pct", "daily_loss_limit_pct", "max_positions"]);
    expect(rows[0]).toEqual({
      key: "risk_per_trade_pct",
      label: "1회 위험 비율",
      value: "1%",
      cap: "상한 2%",
      changed: null,
      cooling: null,
    });
    expect(rows[1].cap).toBe("상한 5%");
    expect(rows[2].value).toBe("5");
    expect(rows[2].cap).toBeNull();
  });

  it("템플릿이 없으면 규칙 표 줄이 없다", () => {
    // 검증: V82 C29
    expect(ruleRows(noTemplate())).toEqual([]);
  });

  it("최근에 반영된 개선안이 바꾼 칸에는 '바뀜'과 앞 값이 붙는다", () => {
    // 검증: V82 F5
    const rows = ruleRows(
      rulesWith("0.5", { recent: [{ proposal: 3, applied_at: 1_700_000_000_000, changes: [riskChange("1", "0.5", false)] }] }),
    );
    expect(rows[0].changed).toEqual({ proposal: 3, before: "1%" });
    expect(rows[1].changed).toBeNull();
    expect(rows[2].changed).toBeNull();
  });

  it("그 뒤 값이 지금 값과 다르면(그 뒤에 또 바뀜) 바뀜 표시를 하지 않는다", () => {
    // 검증: V82 F5
    const rows = ruleRows(
      rulesWith("0.7", { recent: [{ proposal: 3, applied_at: 1, changes: [riskChange("1", "0.5", false)] }] }),
    );
    expect(rows[0].changed).toBeNull();
  });

  it("같은 칸을 여러 개선안이 바꿨으면 가장 늦게 반영된 것의 앞 값을 보인다", () => {
    // 검증: V82 F5
    const rows = ruleRows(
      rulesWith("0.5", {
        recent: [
          { proposal: 5, applied_at: 2, changes: [riskChange("0.8", "0.5", false)] },
          { proposal: 3, applied_at: 1, changes: [riskChange("1", "0.8", false)] },
        ],
      }),
    );
    expect(rows[0].changed).toEqual({ proposal: 5, before: "0.8%" });
  });

  it("냉각중 개선안이 걸린 칸에는 다음 거래일부터의 뒤 값이 붙는다", () => {
    // 검증: V82 E28 C29
    const rows = ruleRows(
      rulesWith("1", { cooling: [{ proposal: 4, changes: [riskChange("1", "1.5", true)], applies_on: "next_trading_day" }] }),
    );
    expect(rows[0].cooling).toEqual({ proposal: 4, after: "1.5%" });
    expect(rows[0].changed).toBeNull();
    expect(rows[1].cooling).toBeNull();
  });

  it("절대 상한 줄은 응답의 상한 차례로 이름과 상한 글을 보인다", () => {
    // 검증: V82 C29
    const rows = capRows(noTemplate());
    expect(rows.map((row) => row.key)).toEqual(Object.keys(CAPS));
    expect(rows[0]).toEqual({ key: "risk_per_trade_pct", label: "1회 위험 비율", cap: "상한 2%" });
    expect(rows.find((row) => row.key === "sector_weight_pct")?.cap).toBe("상한 50%");
  });
});

describe("개선안 줄", () => {
  it("상태 글은 제안됨 · 냉각중 · 반영됨 · 거절됨이다", () => {
    // 검증: V82 F5
    expect(proposalStateLabel("proposed")).toBe("제안됨");
    expect(proposalStateLabel("cooling")).toBe("냉각중");
    expect(proposalStateLabel("applied")).toBe("반영됨");
    expect(proposalStateLabel("rejected")).toBe("거절됨");
  });

  it("실수 종류는 뜻 글로, 모르는 글은 그대로 보인다", () => {
    // 검증: V82 F5
    expect(mistakeLabel("chased_entry")).toBe("추격 매수");
    expect(mistakeLabel("late_stop")).toBe("손절이 늦음");
    expect(mistakeLabel("oversized_position")).toBe("비중 과다");
    expect(mistakeLabel("no_rationale")).toBe("근거 없이 진입");
    expect(mistakeLabel("panic_exit")).toBe("성급한 청산");
    expect(mistakeLabel("뉴스만 보고 샀다")).toBe("뉴스만 보고 샀다");
  });

  it("줄은 새 개선안부터이고 상태 · 실수 종류 · 앞 → 뒤 값 · 위험을 키우는가가 보인다", () => {
    // 검증: V82 F5 E28
    const rows = proposalRows(proposalsOf(proposal(1, "applied", "1", "0.5"), proposal(2, "proposed", "1", "1.5")));
    expect(rows.map((row) => row.id)).toEqual([2, 1]);
    const risky = rows[0];
    expect(risky.state).toBe("proposed");
    expect(risky.stateLabel).toBe("제안됨");
    expect(risky.mistake).toBe("손절이 늦음");
    expect(risky.risky).toBe(true);
    expect(risky.riskLabel).toBe("위험을 키움");
    expect(risky.changes).toEqual([
      { key: "risk_per_trade_pct", label: "1회 위험 비율", before: "1%", after: "1.5%", widened: true },
    ]);
    const safe = rows[1];
    expect(safe.stateLabel).toBe("반영됨");
    expect(safe.risky).toBe(false);
    expect(safe.riskLabel).toBe("위험을 키우지 않음");
    expect(safe.changes[0].widened).toBe(false);
  });

  it("승인 · 거절은 제안됨이고 승인 번호 · 지문이 있는 줄에서만 할 수 있다", () => {
    // 검증: V82 E35 V21
    const states = ["proposed", "cooling", "applied", "rejected"] as const;
    const rows = proposalRows(proposalsOf(...states.map((state, i) => proposal(i + 1, state, "1", "0.5"))));
    for (const row of rows) expect(row.canAnswer, row.state).toBe(row.state === "proposed");
    const proposed = rows.find((row) => row.state === "proposed");
    expect(proposed?.number).toBe("rule-1");
    expect(proposed?.fingerprint).toBe(proposal(1, "proposed", "1", "0.5").fingerprint);

    const unnumbered = { ...proposal(9, "proposed", "1", "0.5"), number: null, fingerprint: null };
    expect(proposalRows(proposalsOf(unnumbered))[0].canAnswer).toBe(false);
  });
});

describe("답하기 결과 문구", () => {
  it("반영 · 냉각중(다음 거래일부터) · 거절을 구분한다", () => {
    // 검증: V82 V21 E28
    expect(answerMessage({ status: 200, body: { result: "applied", reason: null } }).kind).toBe("applied");
    expect(answerMessage({ status: 200, body: { result: "applied", reason: null } }).text).toContain("반영");
    const cooling = answerMessage({ status: 200, body: { result: "cooling", reason: null } });
    expect(cooling.kind).toBe("cooling");
    expect(cooling.text).toContain("다음 거래일부터");
    const rejected = answerMessage({ status: 200, body: { result: "rejected", reason: "user_rejected" } });
    expect(rejected.kind).toBe("rejected");
    expect(rejected.text).toContain("거절");
  });

  it("거절의 까닭이 글로 보인다(이미 답함 · 템플릿 잠김 · 모르는 까닭은 글자 그대로)", () => {
    // 검증: V82 E35 V21
    const done = answerMessage({ status: 200, body: { result: "rejected", reason: "already_resolved" } });
    expect(done.kind).toBe("rejected");
    expect(done.text).toContain("이미");
    const locked = answerMessage({ status: 200, body: { result: "rejected", reason: "template_locked" } });
    expect(locked.text).toContain("잠겨");
    const other = answerMessage({ status: 200, body: { result: "rejected", reason: "fingerprint_mismatch_x" } });
    expect(other.text).toContain("fingerprint_mismatch_x");
  });

  it("200이 아닌 답은 실패이고 다시 시도하라는 글이다", () => {
    // 검증: V82 E57
    for (const status of [503, 500]) {
      const message = answerMessage({ status, body: { error: "unavailable" } });
      expect(message.kind, String(status)).toBe("failed");
      expect(message.text).toContain("다시 시도");
    }
  });
});
