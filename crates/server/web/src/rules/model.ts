// 규칙 화면의 화면 모델(server 28차 · C29 · F5 · V82 · E28 · E35): 서버 응답을 화면이 그릴 줄로 바꾸는 순수 함수.
// DOM을 만지지 않는다(그리기는 view.ts, 서버 부르기는 screen.ts). 칸 이름 · 값 글 · 상한 글은 템플릿 화면의 것
// (`fieldLabel` · `formatValue` · `capText`)을 그대로 쓴다 — 두 화면이 함께 쓰는 `../ui/fields`에 있다.
import { capText, changeRow, fieldLabel, formatValue, isObject, type ChangeRow, type FieldChange } from "../ui/fields";

/** 읽기 응답의 바뀐 칸 하나(`pi_template::diff_params`, 템플릿 판 이력과 같은 모양) · 그 줄. */
export type { ChangeRow, FieldChange };

/** 규칙 읽기 응답의 칸 하나(`template_view`의 키 · 값 · 상한). `cap`은 십진수 글자 또는 없음. */
export interface RuleField {
  key: string;
  value: unknown;
  cap: string | null;
}

/** 냉각중인 개선안 하나: 다음 거래일 시작에 반영된다(E28). */
export interface CoolingView {
  proposal: number;
  changes: FieldChange[];
  applies_on: "next_trading_day";
}

/** 최근에 반영된 개선안 하나. `applied_at`은 유닉스 밀리초. */
export interface RecentView {
  proposal: number;
  applied_at: number;
  changes: FieldChange[];
}

/**
 * 규칙 읽기 응답 `GET /api/rules`(C29). 템플릿이 없으면 `template`은 없음이고 `fields`는 빈 목록이다.
 * `caps`는 원칙의 절대 상한(키 → 십진수 글자)이고 늘 있다. `recent`는 반영 시각이 늦은 것부터다.
 */
export interface RulesReply {
  template: number | null;
  fields: RuleField[];
  caps: Record<string, string>;
  cooling: CoolingView[];
  recent: RecentView[];
}

/** 개선안 상태 글자(서버 그대로). */
export type ProposalState = "proposed" | "cooling" | "applied" | "rejected";

/** 개선안 목록 `GET /api/proposals`의 개선안 하나. `number` · `fingerprint`는 그 승인 요청의 번호 · 지문. */
export interface ProposalView {
  id: number;
  state: ProposalState;
  mistake: string;
  increases_risk: boolean;
  evidence: number[];
  number: string | null;
  fingerprint: string | null;
  changes: FieldChange[];
}

/** 개선안 목록 응답. */
export interface ProposalsReply {
  proposals: ProposalView[];
}

/** 규칙 표의 줄 하나. */
export interface RuleRow {
  /** 칸 이름(서버 키). */
  key: string;
  /** 보이는 이름(템플릿 화면과 같다). */
  label: string;
  /** 지금 값 글(퍼센트 칸은 `%`). */
  value: string;
  /** 상한 안내 글("상한 2%"). 상한이 없으면 없음. */
  cap: string | null;
  /**
   * "바뀜" 표시(F5-8): 최근에 반영된 개선안이 이 칸을 바꿨고 그 뒤 값이 지금 값과 같으면 그 개선안 번호와 앞 값 글.
   * 여러 개선안이 같은 칸을 바꿨으면 가장 늦게 반영된 것(`recent`의 앞쪽). 아니면 없음.
   */
  changed: { proposal: number; before: string } | null;
  /** 냉각중인 개선안이 이 칸을 바꿀 예정이면 그 개선안 번호와 뒤 값 글(다음 거래일부터). 아니면 없음. */
  cooling: { proposal: number; after: string } | null;
}

/** 개선안 목록의 줄 하나. */
export interface ProposalRow {
  id: number;
  state: ProposalState;
  /** 상태 글: 제안됨 · 냉각중 · 반영됨 · 거절됨. */
  stateLabel: string;
  /** 실수 종류 글(모르는 글자는 그대로). */
  mistake: string;
  /** 위험을 키우는가(`increases_risk`). */
  risky: boolean;
  /** 위험 표시 글: "위험을 키움" · "위험을 키우지 않음". */
  riskLabel: string;
  /** 앞 → 뒤 값. */
  changes: ChangeRow[];
  /** 승인 · 거절 단추를 보이는가: 제안됨이고 승인 번호 · 지문이 있을 때만. */
  canAnswer: boolean;
  number: string | null;
  fingerprint: string | null;
}

/** 답하기 결과(`POST /api/proposals/answer`의 상태 · 몸통). */
export interface AnswerReply {
  status: number;
  body: unknown;
}

/**
 * 답하기 결과 문구(그 개선안 줄에 보인다).
 * - `applied`: 반영됨(다음 매매부터).
 * - `cooling`: 냉각중 — 글에 "다음 거래일부터"가 든다(E28).
 * - `rejected`: 거절 — 사용자가 거절했으면 거절했다는 글, 그 밖은 까닭 글(이미 답함 · 템플릿 잠김 · 번호 · 지문 등, E35).
 * - `failed`: 200이 아닌 답(503 등) — 다시 시도하라는 글.
 */
export type AnswerMessage =
  | { kind: "applied"; text: string }
  | { kind: "cooling"; text: string }
  | { kind: "rejected"; text: string }
  | { kind: "failed"; text: string };

/** 상태 글. */
const STATE_LABELS: Readonly<Record<ProposalState, string>> = {
  proposed: "제안됨",
  cooling: "냉각중",
  applied: "반영됨",
  rejected: "거절됨",
};

/** 실수 종류 글(journal `Mistake`의 뜻). */
const MISTAKES: Readonly<Record<string, string>> = {
  chased_entry: "추격 매수",
  late_stop: "손절이 늦음",
  oversized_position: "비중 과다",
  no_rationale: "근거 없이 진입",
  panic_exit: "성급한 청산",
};

/** 거절 까닭 글(E35 · V21). 모르는 까닭은 글자 그대로 보인다. */
const REASONS: Readonly<Record<string, string>> = {
  user_rejected: "거절했습니다. 규칙은 그대로입니다.",
  already_resolved: "이미 답한 개선안입니다. 목록을 새로 읽었습니다.",
  template_locked: "쓰는 템플릿의 값이 잠겨 있어(승격대기 · 실전중) 반영하지 않았습니다. 승인 번호는 그대로 기다립니다.",
  fingerprint_mismatch: "승인 내용이 달라(지문 다름) 반영하지 않았습니다.",
  not_found: "모르는 승인 번호입니다.",
  expired: "승인 기한이 지났습니다.",
};

/** 상태 글. */
export function proposalStateLabel(state: ProposalState): string {
  return STATE_LABELS[state] ?? state;
}

/**
 * 실수 종류 글: `chased_entry` "추격 매수" · `late_stop` "손절이 늦음" · `oversized_position` "비중 과다" ·
 * `no_rationale` "근거 없이 진입" · `panic_exit` "성급한 청산"(journal `Mistake`의 뜻). 그 밖의 글은 그대로.
 */
export function mistakeLabel(word: string): string {
  return MISTAKES[word] ?? word;
}

/** 그 칸의 바뀐 값(없으면 없음). */
function changeOf(changes: readonly FieldChange[], key: string): FieldChange | undefined {
  return changes.find((change) => change.key === key);
}

/** 규칙 표의 줄(응답의 칸 순서). 템플릿이 없으면 빈 목록. */
export function ruleRows(reply: RulesReply): RuleRow[] {
  return reply.fields.map((field) => {
    const value = formatValue(field.key, field.value);
    // 가장 늦게 반영된 것부터 보고, 그 칸을 바꾼 첫 개선안의 뒤 값이 지금 값과 같을 때만 "바뀜".
    let changed: RuleRow["changed"] = null;
    for (const recent of reply.recent) {
      const change = changeOf(recent.changes, field.key);
      if (!change) continue;
      if (formatValue(field.key, change.after) === value) {
        changed = { proposal: recent.proposal, before: formatValue(field.key, change.before) };
      }
      break;
    }
    let cooling: RuleRow["cooling"] = null;
    for (const pending of reply.cooling) {
      const change = changeOf(pending.changes, field.key);
      if (change) cooling = { proposal: pending.proposal, after: formatValue(field.key, change.after) };
    }
    return { key: field.key, label: fieldLabel(field.key), value, cap: capText(field.cap), changed, cooling };
  });
}

/** 원칙의 절대 상한 줄: 키 → 보이는 이름과 상한 글("상한 2%"), 응답의 `caps` 차례. */
export function capRows(reply: RulesReply): { key: string; label: string; cap: string }[] {
  return Object.entries(reply.caps).map(([key, cap]) => ({ key, label: fieldLabel(key), cap: capText(cap) ?? "" }));
}

/** 개선안 목록의 줄. 번호가 큰 것(새것)부터. */
export function proposalRows(reply: ProposalsReply): ProposalRow[] {
  return [...reply.proposals]
    .sort((a, b) => b.id - a.id)
    .map((proposal) => ({
      id: proposal.id,
      state: proposal.state,
      stateLabel: proposalStateLabel(proposal.state),
      mistake: mistakeLabel(proposal.mistake),
      risky: proposal.increases_risk,
      riskLabel: proposal.increases_risk ? "위험을 키움" : "위험을 키우지 않음",
      changes: (proposal.changes ?? []).map(changeRow),
      canAnswer:
        proposal.state === "proposed" &&
        typeof proposal.number === "string" &&
        proposal.number.length > 0 &&
        typeof proposal.fingerprint === "string" &&
        proposal.fingerprint.length > 0,
      number: proposal.number,
      fingerprint: proposal.fingerprint,
    }));
}

/** 답하기 결과 → 문구. */
export function answerMessage(reply: AnswerReply): AnswerMessage {
  const body = isObject(reply.body) ? reply.body : {};
  if (reply.status !== 200) {
    return { kind: "failed", text: "지금 답을 처리하지 못했습니다. 잠시 뒤 다시 시도해 주세요." };
  }
  if (body.result === "applied") return { kind: "applied", text: "반영했습니다. 다음 매매부터 새 값을 씁니다." };
  if (body.result === "cooling") {
    return { kind: "cooling", text: "승인했습니다. 위험을 키우는 변경이라 냉각중이며 다음 거래일부터 반영됩니다." };
  }
  const reason = typeof body.reason === "string" ? body.reason : "";
  const text = REASONS[reason] ?? `반영하지 않았습니다(${reason || "까닭 모름"}).`;
  return { kind: "rejected", text };
}
