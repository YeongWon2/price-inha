// 규칙 화면의 "팀 지침(AI가 배운 것)" 묶음(기능 L · 결정 30)의 순수 계산: 응답 읽기 · 보이는 글.
// 계산 · 판단(성적 · 고르기 · 내림)은 서버가 한다. 여기서는 받은 값을 글로 바꾸기만 한다.

/** 지침 종류(서버 기계 글자). */
export type GuidelineKind = "avoid" | "enter" | "manage";

/** 지침 하나(`GET /api/guidelines`의 줄). */
export interface GuidelineView {
  id: string;
  kind: GuidelineKind;
  text: string;
  when: string;
  symbol: string | null;
  basis: string[];
  uses: number;
  wins: number;
  losses: number;
  win_rate_pct: number | null;
  created_at: number;
  updated_at: number;
  state: "active" | "retired";
  retired_why: string | null;
}

/** `GET /api/guidelines` 응답. */
export interface GuidelinesReply {
  active: GuidelineView[];
  retired: GuidelineView[];
  editable: boolean;
}

/** 묶음 제목. */
export const GUIDE_TITLE = "팀 지침(AI가 배운 것)";

/** 1층 규칙과 다르다는 한 줄. */
export const LAYER_NOTE =
  "AI가 복기 · 실수 · 결정 메모에서 스스로 배운 참고입니다. 위의 규칙과 달리 승인 없이 바뀌고, 주문은 언제나 위의 규칙 검사를 그대로 거칩니다.";

/** 종류의 쉬운 말. */
export const KIND_LABEL: Readonly<Record<GuidelineKind, string>> = {
  avoid: "피하기",
  enter: "들어가기",
  manage: "운용",
};

/** 응답을 읽는다. 모양이 다르면 `null`. */
export function readGuidelines(body: unknown): GuidelinesReply | null {
  if (typeof body !== "object" || body === null) return null;
  const value = body as Record<string, unknown>;
  const { active, retired, editable } = value;
  if (!Array.isArray(active) || !Array.isArray(retired) || typeof editable !== "boolean") return null;
  if (![...active, ...retired].every(isRow)) return null;
  return { active: active as GuidelineView[], retired: retired as GuidelineView[], editable };
}

/** 지침 줄 모양인가(화면이 쓰는 칸만 본다). */
function isRow(item: unknown): boolean {
  if (typeof item !== "object" || item === null) return false;
  const row = item as Record<string, unknown>;
  return (
    typeof row.id === "string" &&
    (row.kind === "avoid" || row.kind === "enter" || row.kind === "manage") &&
    typeof row.text === "string" &&
    typeof row.when === "string" &&
    typeof row.uses === "number" &&
    typeof row.wins === "number" &&
    (row.win_rate_pct === null || typeof row.win_rate_pct === "number")
  );
}

/** 익절 비율 글. 아직 끝난 매매가 없으면 그렇다고 쓴다. */
export function rateText(row: GuidelineView): string {
  if (row.win_rate_pct === null || row.uses === 0) return "아직 끝난 매매 없음";
  return `익절 ${row.win_rate_pct}% (${row.uses}번 중 ${row.wins}번)`;
}

/** 쓰임 글. */
export function usesText(row: GuidelineView): string {
  return `쓰임 ${row.uses}번`;
}

/** 내린 까닭(서버 기계 글자) → 쉬운 말. */
const RETIRED_WHY: Readonly<Record<string, string>> = {
  merged: "겹치는 지침과 합쳤습니다",
  poor_win_rate: "익절 비율이 낮아 내렸습니다",
  unused: "오래 쓰이지 않아 내렸습니다",
  blocking_trades: "매매를 너무 막아 내렸습니다",
  person: "사람이 끔",
};

/** 내린 까닭 글. */
export function whyText(why: string | null): string {
  if (why === null) return "";
  return RETIRED_WHY[why] ?? why;
}

/** 끄기 결과 글. */
export interface RetireMessage {
  kind: "ok" | "failed";
  text: string;
}

/** 끄기 응답(상태 · 본문)을 결과 글로 바꾼다. 상태 0은 연결 실패다. */
export function retireMessage(status: number, body: unknown): RetireMessage {
  void body;
  if (status === 200) return { kind: "ok", text: "지침을 껐습니다." };
  if (status === 0) return { kind: "failed", text: "서버와 연결하지 못했습니다. 잠시 뒤 다시 시도해 주세요." };
  if (status === 403) return { kind: "failed", text: "지침은 이 컴퓨터에서만 끌 수 있습니다." };
  if (status === 404) return { kind: "failed", text: "그 지침이 없습니다. 목록을 다시 읽었습니다." };
  return { kind: "failed", text: "지침을 끄지 못했습니다. 잠시 뒤 다시 시도해 주세요." };
}
