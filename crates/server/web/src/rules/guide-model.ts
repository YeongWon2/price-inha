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
  void body;
  return null;
}

/** 익절 비율 글. 아직 끝난 매매가 없으면 그렇다고 쓴다. */
export function rateText(row: GuidelineView): string {
  void row;
  return "";
}

/** 쓰임 글. */
export function usesText(row: GuidelineView): string {
  void row;
  return "";
}

/** 내린 까닭 글. */
export function whyText(why: string | null): string {
  void why;
  return "";
}

/** 끄기 결과 글. */
export interface RetireMessage {
  kind: "ok" | "failed";
  text: string;
}

/** 끄기 응답(상태 · 본문)을 결과 글로 바꾼다. 상태 0은 연결 실패다. */
export function retireMessage(status: number, body: unknown): RetireMessage {
  void status;
  void body;
  return { kind: "failed", text: "" };
}
