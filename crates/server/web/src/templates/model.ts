// 템플릿 화면의 화면 모델(server 27차 · C28 · F6 · E5 · E28 · E58): 서버 응답을 화면이 그릴 줄로 바꾸는 순수 함수.
// DOM을 만지지 않는다(그리기는 view.ts, 서버 부르기는 screen.ts).

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

/** 판 이력의 바뀐 칸 하나(`diff_params`). */
export interface FieldChange {
  key: string;
  before: unknown;
  after: unknown;
  widened: boolean;
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
export const LIVE_NOTICE = "";

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

/** 판 이력의 바뀐 칸 줄. */
export interface ChangeRow {
  key: string;
  label: string;
  before: string;
  after: string;
  /** 넓힘인가(위험을 키운 칸). */
  widened: boolean;
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

/** 칸 이름 → 보이는 이름. 모르는 키는 키 그대로. */
export function fieldLabel(key: string): string {
  void key;
  throw new Error("todo");
}

/** 칸 값 → 글. 퍼센트 칸(`_pct`로 끝나는 키)의 수는 `%`를 붙이고, 없음은 "없음". */
export function formatValue(key: string, value: unknown): string {
  void key;
  void value;
  throw new Error("todo");
}

/** 상한 글자 → 안내 글("상한 2%"). 상한이 없으면 없음. */
export function capText(cap: string | null): string | null {
  void cap;
  throw new Error("todo");
}

/** 응답에서 쓰는 템플릿(`current`의 번호). 없으면 없음. */
export function currentTemplate(reply: TemplatesReply): TemplateEntry | null {
  void reply;
  throw new Error("todo");
}

/** 템플릿 하나 → 칸 표 줄(`fields` 순서 그대로). */
export function fieldRows(entry: TemplateEntry): FieldRow[] {
  void entry;
  throw new Error("todo");
}

/** 템플릿 하나 → 승격 기준 표 줄(`promotion` 순서 그대로). */
export function promotionRows(entry: TemplateEntry): PromotionRow[] {
  void entry;
  throw new Error("todo");
}

/** 템플릿 하나 → 판 이력 줄(`revisions` 순서 그대로). */
export function revisionRows(entry: TemplateEntry): RevisionRow[] {
  void entry;
  throw new Error("todo");
}

/** 상태 글자 → 배지 글(모의중 · 승격대기 · 실전중 · 멈춤). */
export function stateLabel(state: TemplateState): string {
  void state;
  throw new Error("todo");
}

/** "실전 운용 시작"을 누를 수 있는가: 승격대기이고 승격 승인 번호가 있을 때만. */
export function canPromote(entry: TemplateEntry | null): boolean {
  void entry;
  throw new Error("todo");
}

/** "멈추기" 단추가 보이는가: 실전중일 때만. */
export function canStop(entry: TemplateEntry | null): boolean {
  void entry;
  throw new Error("todo");
}

/** 값 저장 결과 → 문구. 상한 안내는 `rows`의 상한으로 만든다. */
export function saveMessage(reply: SaveReply, rows: readonly FieldRow[]): SaveMessage {
  void reply;
  void rows;
  throw new Error("todo");
}
