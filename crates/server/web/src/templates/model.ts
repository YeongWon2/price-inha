// 템플릿 화면의 화면 모델(server 27차 · C28 · F6 · E5 · E28 · E58): 서버 응답을 화면이 그릴 줄로 바꾸는 순수 함수.
// DOM을 만지지 않는다(그리기는 view.ts, 서버 부르기는 screen.ts).

import { capText, changeRow, fieldLabel, formatValue, isObject, type ChangeRow, type FieldChange } from "../ui/fields";

// 칸 이름 · 값 글 · 상한 글 · 바뀐 칸 줄은 규칙 화면과 함께 쓴다(`../ui/fields`).
export { capText, fieldLabel, formatValue, numberLabel, type ChangeRow, type FieldChange } from "../ui/fields";

/** 템플릿 상태 글자(서버 그대로). */
export type TemplateState = "simulating" | "promotion_pending" | "live" | "stopped";

/** 스타일 글자(서버 그대로). */
export type StyleName = "day_trade" | "swing" | "long_term";

/** 승격 기준 글자(서버 그대로, `promotion_lines` 순서). */
export type CriterionName = "trades" | "trading_days" | "net_pnl" | "max_drawdown" | "out_of_sample";

/** 읽기 응답의 칸 하나(`template_view`). `cap`은 십진수 글자 또는 없음. */
export interface FieldView {
  key: string;
  value: unknown;
  cap: string | null;
  editable: boolean;
}

/** 판 이력의 판 하나. `at`은 유닉스 밀리초, `proposal`은 사용자 변경 개선안 번호. */
export interface RevisionView {
  revision: number;
  at: number;
  changes: FieldChange[];
  proposal: number | null;
}

/** 승격 기준 줄 하나(`promotion_lines`). `need` · `have`는 십진수 글자. */
export interface CriterionView {
  criterion: CriterionName;
  need: string;
  have: string | null;
  met: boolean;
}

/** 읽기 응답의 템플릿 하나. `approval`은 승격대기일 때 기다리는 실전 승격 승인 번호. */
export interface TemplateEntry {
  id: number;
  style: StyleName;
  state: TemplateState;
  revision: number;
  fields: FieldView[];
  revisions: RevisionView[];
  promotion: CriterionView[];
  approval: string | null;
}

/** 읽기 응답 `GET /api/templates/current`. */
export interface TemplatesReply {
  current: number | null;
  templates: TemplateEntry[];
}

/** 스타일 고르기 단추 차례와 보이는 이름: 단타 · 스윙 · 중장기. */
export const STYLES: ReadonlyArray<readonly [StyleName, string]> = [
  ["day_trade", "단타"],
  ["swing", "스윙"],
  ["long_term", "중장기"],
];

/**
 * 실전 안내(AGENTS 6번 · 결정 17 · E58): "실전 운용 시작" 단추 옆에 늘 보인다. 실전 운용을 시작해도
 * 실제 증권사로 주문은 나가지 않는다는 뜻이다.
 */
export const LIVE_NOTICE = "실전 운용을 시작해도 실제 증권사로 주문은 나가지 않습니다. 이 시스템의 주문은 언제나 직접 만든 모의계좌로만 갑니다.";

/** 화면 칸 표의 줄 하나. */
export interface FieldRow {
  /** 칸 이름(서버 키). */
  key: string;
  /** 보이는 이름(예: `risk_per_trade_pct` → "1회 위험 비율"). 모르는 키는 키 그대로. */
  label: string;
  /** 지금 값 글(퍼센트 칸은 `1%`처럼 `%`를 붙인다). */
  value: string;
  /** 상한 안내 글(예: "상한 2%"). 상한이 없으면 없음. */
  cap: string | null;
  /** 잠겼는가(`editable`의 반대). 잠긴 칸의 입력은 막힌다. */
  locked: boolean;
}

/** 승격 기준 표의 줄 하나. */
export interface PromotionRow {
  criterion: CriterionName;
  /** 보이는 이름(예: "매매 건수"). */
  label: string;
  /** 필요한 값 글. */
  need: string;
  /** 지금 값 글. 아직 없으면 "아직 없음". */
  have: string;
  /** 채웠는가. */
  met: boolean;
  /** 모자란 기준인가(`met`의 반대, F6-5a1). */
  short: boolean;
}

/** 판 이력의 판 하나. */
export interface RevisionRow {
  revision: number;
  at: number;
  changes: ChangeRow[];
  proposal: number | null;
}

/** 값 저장 결과(`POST /api/templates/adjust`의 상태 · 몸통). */
export interface SaveReply {
  status: number;
  body: unknown;
}

/** 상한을 넘은 칸 하나와 그 옆에 보일 상한 안내(예: "상한 2%"). */
export interface CapNote {
  key: string;
  text: string;
}

/**
 * 저장 결과 문구.
 * - `saved`: 저장됨(새 판 번호를 글에 담는다).
 * - `over_cap`: 상한 넘음(E5) — 값은 저장되지 않았고 칸마다 상한 안내.
 * - `needs_approval`: 실전중 위험을 키우는 변경(E28) — "승인 뒤 다음 거래일부터"와 승인 번호.
 * - `locked`: 잠긴 템플릿(409) — 실전중에는 위험을 키우는 변경만 승인을 거쳐 바꿀 수 있다는 뜻의 글.
 * - `failed`: 그 밖의 실패.
 */
export type SaveMessage =
  | { kind: "saved"; text: string }
  | { kind: "over_cap"; text: string; fields: CapNote[] }
  | { kind: "needs_approval"; text: string; number: string }
  | { kind: "locked"; text: string }
  | { kind: "failed"; text: string };

/**
 * 값 맞추기 경로(`POST /api/templates/adjust`)가 받는 칸(서버 `change_of`와 같은 일곱 칸). 그 밖의 칸은 이 화면에서
 * 보기만 한다(입력 칸은 있으나 읽기 전용).
 */
export const ADJUSTABLE_KEYS: readonly string[] = [
  "risk_per_trade_pct",
  "daily_loss_limit_pct",
  "max_drawdown_pct",
  "symbol_weight_pct",
  "sector_weight_pct",
  "min_cash_pct",
  "auto_order_limit_pct",
];

/** 이 화면에서 값을 보낼 수 있는 칸인가. */
export function isAdjustable(key: string): boolean {
  return ADJUSTABLE_KEYS.includes(key);
}

/** 상태 배지 글. */
const STATE_LABELS: Readonly<Record<TemplateState, string>> = {
  simulating: "모의중",
  promotion_pending: "승격대기",
  live: "실전중",
  stopped: "멈춤",
};

/** 승격 기준 이름. */
const CRITERION_LABELS: Readonly<Record<CriterionName, string>> = {
  trades: "매매 건수",
  trading_days: "거래일 수",
  net_pnl: "비용 뺀 손익",
  max_drawdown: "최대 낙폭",
  out_of_sample: "표본 밖 성과",
};

/** 응답에서 쓰는 템플릿(`current`의 번호). 없으면 없음. */
export function currentTemplate(reply: TemplatesReply): TemplateEntry | null {
  if (reply.current === null) return null;
  return reply.templates.find((entry) => entry.id === reply.current) ?? null;
}

/** 템플릿 하나 → 칸 표 줄(`fields` 순서 그대로). */
export function fieldRows(entry: TemplateEntry): FieldRow[] {
  return entry.fields.map((field) => ({
    key: field.key,
    label: fieldLabel(field.key),
    value: formatValue(field.key, field.value),
    cap: capText(field.cap),
    locked: !field.editable,
  }));
}

/** 템플릿 하나 → 승격 기준 표 줄(`promotion` 순서 그대로). */
export function promotionRows(entry: TemplateEntry): PromotionRow[] {
  return entry.promotion.map((line) => ({
    criterion: line.criterion,
    label: CRITERION_LABELS[line.criterion] ?? line.criterion,
    need: line.need,
    have: line.have ?? "아직 없음",
    met: line.met,
    short: !line.met,
  }));
}

/** 템플릿 하나 → 판 이력 줄(`revisions` 순서 그대로). */
export function revisionRows(entry: TemplateEntry): RevisionRow[] {
  return entry.revisions.map((revision) => ({
    revision: revision.revision,
    at: revision.at,
    proposal: revision.proposal,
    changes: revision.changes.map(changeRow),
  }));
}

/** 상태 글자 → 배지 글(모의중 · 승격대기 · 실전중 · 멈춤). */
export function stateLabel(state: TemplateState): string {
  return STATE_LABELS[state] ?? state;
}

/** 스타일 글자 → 보이는 이름. */
export function styleLabel(style: StyleName): string {
  return STYLES.find(([name]) => name === style)?.[1] ?? style;
}

/** "실전 운용 시작"을 누를 수 있는가: 승격대기이고 승격 승인 번호가 있을 때만. */
export function canPromote(entry: TemplateEntry | null): boolean {
  return entry !== null && entry.state === "promotion_pending" && typeof entry.approval === "string" && entry.approval.length > 0;
}

/** "멈추기" 단추가 보이는가: 실전중일 때만. */
export function canStop(entry: TemplateEntry | null): boolean {
  return entry !== null && entry.state === "live";
}

/** 값 저장 결과 → 문구. 상한 안내는 `rows`의 상한으로 만든다. */
export function saveMessage(reply: SaveReply, rows: readonly FieldRow[]): SaveMessage {
  const body = isObject(reply.body) ? reply.body : {};
  if (reply.status === 200 && body.state === "needs_approval") {
    const number = typeof body.number === "string" ? body.number : "";
    return {
      kind: "needs_approval",
      number,
      text: `위험을 키우는 변경이라 승인을 기다립니다(승인 번호 ${number}). 승인 뒤 다음 거래일부터 적용됩니다.`,
    };
  }
  if (reply.status === 200) {
    const revision = typeof body.revision === "number" ? body.revision : null;
    return { kind: "saved", text: revision === null ? "저장했습니다." : `저장했습니다 — 판 ${revision}.` };
  }
  if (reply.status === 400 && body.error === "over_cap") {
    const keys = Array.isArray(body.fields) ? body.fields.filter((key): key is string => typeof key === "string") : [];
    const fields: CapNote[] = keys.map((key) => {
      const cap = rows.find((row) => row.key === key)?.cap;
      return { key, text: cap ?? "허용 범위를 벗어났습니다" };
    });
    return { kind: "over_cap", fields, text: "상한을 넘거나 허용 범위를 벗어난 값이 있어 저장하지 않았습니다. 표시된 칸의 상한 안으로 맞춰 주세요." };
  }
  if (reply.status === 409) {
    return { kind: "locked", text: "실전중에는 위험을 키우는 변경만 승인을 거쳐 바꿀 수 있습니다. 이 변경은 저장하지 않았습니다." };
  }
  if (reply.status === 400) {
    return { kind: "failed", text: "값을 읽을 수 없어 저장하지 않았습니다. 수를 넣어 주세요." };
  }
  return { kind: "failed", text: "저장하지 못했습니다. 잠시 뒤 다시 시도해 주세요." };
}
