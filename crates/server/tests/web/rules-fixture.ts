// 규칙 화면 시험(server 28차)의 응답 본: 규칙 읽기(`GET /api/rules`)와 개선안 목록(`GET /api/proposals`)의 모양.
import type { FieldChange, ProposalState, ProposalView, ProposalsReply, RulesReply } from "../../web/src/rules/model";

/** 원칙의 절대 상한(서버 `AbsoluteCaps::FIXED`, 상한이 있는 여섯 칸). */
export const CAPS: Record<string, string> = {
  risk_per_trade_pct: "2",
  daily_loss_limit_pct: "5",
  max_drawdown_pct: "20",
  symbol_weight_pct: "25",
  conviction_weight_pct: "25",
  sector_weight_pct: "50",
};

/** 1회 위험 비율 하나가 바뀐 칸. */
export function riskChange(before: string, after: string, widened: boolean): FieldChange {
  return { key: "risk_per_trade_pct", before, after, widened };
}

/** 지금 규칙(스윙 템플릿 7번, 칸 셋). `risk`는 1회 위험 비율. */
export function rulesWith(risk: string, more: Partial<RulesReply> = {}): RulesReply {
  return {
    template: 7,
    fields: [
      { key: "risk_per_trade_pct", value: risk, cap: "2" },
      { key: "daily_loss_limit_pct", value: "3", cap: "5" },
      { key: "max_positions", value: 5, cap: null },
    ],
    caps: { ...CAPS },
    cooling: [],
    recent: [],
    ...more,
  };
}

/** 템플릿이 없을 때의 규칙 읽기. */
export function noTemplate(): RulesReply {
  return { template: null, fields: [], caps: { ...CAPS }, cooling: [], recent: [] };
}

/** 개선안 하나(늦은 손절, 1회 위험 비율을 바꾼다). 제안됨이면 승인 번호 · 지문이 있다. */
export function proposal(id: number, state: ProposalState, before: string, after: string): ProposalView {
  const widened = Number(after) > Number(before);
  return {
    id,
    state,
    mistake: "late_stop",
    increases_risk: widened,
    evidence: [11, 12],
    number: `rule-${id}`,
    fingerprint: `${"ab".repeat(31)}${String(id).padStart(2, "0")}`,
    changes: [riskChange(before, after, widened)],
  };
}

/** 개선안 목록 응답. */
export function proposalsOf(...list: ProposalView[]): ProposalsReply {
  return { proposals: list };
}
