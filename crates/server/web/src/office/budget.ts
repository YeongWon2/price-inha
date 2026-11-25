// 하루 AI 잔량 모델(server 51차, C22 · M13 · N14 · E52 · V64). DOM 없이 받은 내용을 그리기 값으로 바꾼다.
// - 내용은 `GET /api/team/budget`과 이벤트 `team.ai_used`(같은 모양). 사용률 · 색 단계 · 모드 · 담당 정렬은 서버가 정한다.
// - 상한이 있으면 위 줄 바 = 쓴 수 / 상한, 바 끝의 예비분 구간(팀장 몫)을 따로 칠한다. 모드가 정리 · 팀장만이면 글이 붙는다.
// - 상한이 없으면 바 대신 "AI 상한 없음 · 오늘 n번"(E52 · V64).

/** 잔량이 바뀔 때 오는 이벤트. */
export const KIND_TEAM_AI_USED = "team.ai_used";

/** 사용률 색 단계(서버가 정한다: 0.33 · 0.66을 넘으면 바뀐다). */
export type BudgetLevel = "low" | "mid" | "high";
/** 모드(서버가 정한다). */
export type BudgetMode = "normal" | "wrap_up" | "leader_only";

/** 담당 하나의 그날 사용. */
export interface BudgetMember {
  agent: string;
  used: number;
  share: number | null;
  ratio: number | null;
  level: BudgetLevel | null;
}

/** `GET /api/team/budget` · `team.ai_used`의 내용. */
export interface Budget {
  limit: number | null;
  used: number;
  reserve: number;
  mode: BudgetMode;
  ratio: number | null;
  level: BudgetLevel | null;
  members: BudgetMember[];
}

/** 위 줄 잔량 바의 그리기 값. */
export interface BudgetBar {
  unlimited: boolean;
  text: string;
  mode: BudgetMode;
  modeText: string;
  level: BudgetLevel | null;
  /** 채움(0~1). 상한이 없으면 null. */
  fill: number | null;
  /** 예비분 구간이 시작하는 자리(0~1). 상한이 없으면 null. */
  reserveFrom: number | null;
}

/** 담당별 사용 표의 줄 하나. */
export interface BudgetRow {
  agent: string;
  name: string;
  used: string;
  share: string;
  percent: string;
  level: BudgetLevel | null;
  fill: number | null;
}

const MODES: readonly BudgetMode[] = ["normal", "wrap_up", "leader_only"];
const LEVELS: readonly BudgetLevel[] = ["low", "mid", "high"];
const MODE_TEXT: Readonly<Record<BudgetMode, string>> = { normal: "", wrap_up: "정리 모드", leader_only: "팀장 몫만" };
/** 몫이 없을 때의 칸 글. */
const NONE = "—";

type Obj = Record<string, unknown>;

function isObj(value: unknown): value is Obj {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function count(value: unknown): number | null {
  return typeof value === "number" && Number.isFinite(value) && value >= 0 ? value : null;
}

/** 수이거나 null(없음). 둘 다 아니면 undefined(틀림). */
function maybe(value: unknown): number | null | undefined {
  if (value === null) { return null; }
  return count(value) ?? undefined;
}

function levelOf(value: unknown): BudgetLevel | null | undefined {
  if (value === null) { return null; }
  return LEVELS.find((one) => one === value);
}

function readMember(value: unknown): BudgetMember | null {
  if (!isObj(value) || typeof value.agent !== "string") { return null; }
  const used = count(value.used);
  const share = maybe(value.share);
  const ratio = maybe(value.ratio);
  const level = levelOf(value.level);
  if (used === null || share === undefined || ratio === undefined || level === undefined) { return null; }
  return { agent: value.agent, used, share, ratio, level };
}

/** 받은 내용을 읽는다. 모양이 틀리면 null. */
export function readBudget(payload: unknown): Budget | null {
  if (!isObj(payload) || !Array.isArray(payload.members)) { return null; }
  const limit = maybe(payload.limit);
  const used = count(payload.used);
  const reserve = count(payload.reserve);
  const mode = MODES.find((one) => one === payload.mode);
  const ratio = maybe(payload.ratio);
  const level = levelOf(payload.level);
  if (limit === undefined || used === null || reserve === null || !mode || ratio === undefined || level === undefined) {
    return null;
  }
  const members = payload.members.map(readMember);
  if (members.some((one) => one === null)) { return null; }
  return { limit, used, reserve, mode, ratio, level, members: members as BudgetMember[] };
}

function clamp(value: number): number {
  return Math.min(1, Math.max(0, value));
}

/** 위 줄 바의 그리기 값. */
export function budgetBar(budget: Budget): BudgetBar {
  const base = { mode: budget.mode, modeText: MODE_TEXT[budget.mode], level: budget.level };
  if (budget.limit === null || budget.limit <= 0) {
    return { ...base, unlimited: true, text: `AI 상한 없음 · 오늘 ${budget.used}번`, fill: null, reserveFrom: null };
  }
  return {
    ...base,
    unlimited: false,
    text: `AI ${budget.used}/${budget.limit}`,
    fill: clamp(budget.ratio ?? budget.used / budget.limit),
    reserveFrom: clamp((budget.limit - budget.reserve) / budget.limit),
  };
}

/** 몫 글: 소수 둘째 자리까지(끝의 0은 뺀다). */
function shareText(share: number): string {
  return String(Math.round(share * 100) / 100);
}

/** 담당별 사용 표(서버가 준 순서 그대로). */
export function budgetRows(budget: Budget, nameOf: (id: string) => string): BudgetRow[] {
  return budget.members.map((member) => ({
    agent: member.agent,
    name: nameOf(member.agent),
    used: `${member.used}번`,
    share: member.share === null ? NONE : shareText(member.share),
    percent: member.ratio === null ? NONE : `${Math.round(member.ratio * 100)}%`,
    level: member.level,
    fill: member.ratio === null ? null : clamp(member.ratio),
  }));
}
