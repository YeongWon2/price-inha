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

/** 지켜보기 까닭 코드(`pi_agent::WatchReason::code`) → 쉬운 말. */
const WATCH_REASON_TEXT: Readonly<Record<string, string>> = {
  missing_data: "자료가 모자라 지켜봄",
  role_unavailable: "담당이 답하지 못해 지켜봄",
  discussion_bounded: "논의가 길어져 지켜봄",
  consensus_below: "합의 점수가 기준에 못 미쳐 지켜봄",
  rationale_incomplete: "근거가 비어 지켜봄",
  leader_watch: "팀장이 지켜보기로 정함",
};

/** 오류 끝 까닭 코드(서버가 남김) → 쉬운 말. */
const FAILED_REASON_TEXT: Readonly<Record<string, string>> = {
  discussion_error: "논의 중 오류가 났음",
  gather_failed: "자료를 모으다 오류가 났음",
};

/** 끝난 논의의 앞 글. */
const HEAD: Readonly<Record<EndKind, string>> = { watched: "지켜봄", failed: "논의가 끝나지 못함" };

function isEnd(value: unknown): value is DiscussionEnd {
  if (typeof value !== "object" || value === null) return false;
  const end = value as { reason_code?: unknown; at?: unknown };
  return typeof end.reason_code === "string" && typeof end.at === "number";
}

/** 끝난 논의의 종류와 그 칸. 칸이 없거나 모양이 틀리면 없음. */
function endOf(fields: EndFields): [EndKind, DiscussionEnd] | null {
  if (isEnd(fields.watched)) return ["watched", fields.watched];
  if (isEnd(fields.failed)) return ["failed", fields.failed];
  return null;
}

/** 끝난 논의의 종류. 칸이 없으면 없음. */
export function discussionEndKind(fields: EndFields): EndKind | null {
  return endOf(fields)?.[0] ?? null;
}

/**
 * 끝난 논의의 글: `"지켜봄 — <까닭 글>"` · `"논의가 끝나지 못함 — <까닭 글>"`. 모르는 까닭 코드면 앞 글만.
 * 칸이 없으면 없음.
 */
export function discussionEndText(fields: EndFields): string | null {
  const found = endOf(fields);
  if (found === null) return null;
  const [kind, end] = found;
  const table = kind === "watched" ? WATCH_REASON_TEXT : FAILED_REASON_TEXT;
  const reason = Object.prototype.hasOwnProperty.call(table, end.reason_code) ? table[end.reason_code] : null;
  return reason === null ? HEAD[kind] : `${HEAD[kind]} — ${reason}`;
}

/** 끝난 시각. 칸이 없으면 없음. */
export function discussionEndAt(fields: EndFields): number | null {
  return endOf(fields)?.[1].at ?? null;
}
