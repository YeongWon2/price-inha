// 하루 AI 잔량 모델(server 51차, C22 · M13 · N14 · E52 · V64). 뼈대.

/** 잔량이 바뀔 때 오는 이벤트. */
export const KIND_TEAM_AI_USED = "team.ai_used";

/** 사용률 색 단계(서버가 정한다). */
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
  fill: number | null;
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

export function readBudget(payload: unknown): Budget | null {
  void payload;
  return null;
}

export function budgetBar(budget: Budget): BudgetBar {
  void budget;
  return { unlimited: false, text: "", mode: "normal", modeText: "", level: null, fill: null, reserveFrom: null };
}

export function budgetRows(budget: Budget, nameOf: (id: string) => string): BudgetRow[] {
  void budget;
  void nameOf;
  return [];
}
