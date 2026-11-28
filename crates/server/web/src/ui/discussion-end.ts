// 69차: 끝난 논의의 글(M2 상태표는 그대로 — 표시만). 서버가 논의중 매매에 붙이는 칸
// `watched: {"reason_code", "at"}`(지켜보기로 끝남) · `failed: {"reason_code", "at"}`(오류로 끝남)을 그릴 글로 바꾼다.
// 히스토리 화면(매매 줄 · 상태 흐름)과 사무실 근거 패널이 같은 글을 쓴다. DOM을 만지지 않는다.

/** 끝난 논의 하나: 까닭 코드와 끝난 시각(유닉스 밀리초). */
export interface DiscussionEnd {
  reason_code: string;
  at: number;
}

/** 끝난 논의 칸을 가질 수 있는 것(매매 목록의 줄 · 매매 하나 · 근거). */
export interface EndFields {
  watched?: DiscussionEnd;
  failed?: DiscussionEnd;
}

/** 끝난 논의의 종류. */
export type EndKind = "watched" | "failed";

/** 끝난 논의의 종류. 칸이 없으면 없음. */
export function discussionEndKind(_fields: EndFields): EndKind | null {
  return null;
}

/** 끝난 논의의 글. 칸이 없으면 없음. */
export function discussionEndText(_fields: EndFields): string | null {
  return null;
}

/** 끝난 시각. 칸이 없으면 없음. */
export function discussionEndAt(_fields: EndFields): number | null {
  return null;
}
