// 설정 화면의 하루 AI 호출 상한(server 22차 6번). 지금 값은 설정 상태(`GET /api/setup`)의 `ai_daily_limit` 칸
// (`null` = 없음)이고, 바꾸는 길은 AI 설정 저장(`POST /api/setup/ai`의 `daily_limit` 칸 하나)이다.
import { API } from "../paths";

/** 설정 상태에서 하루 AI 호출 상한을 읽는 칸 이름. */
export const AI_LIMIT_FIELD = "ai_daily_limit";
/** 바꾸는 요청의 칸 이름. */
export const AI_LIMIT_KEY = "daily_limit";

/** 지금 값 글자: 없으면 "없음", 있으면 "N번". */
export function formatAiLimit(limit: number | null): string {
  return limit === null ? "없음" : `${limit}번`;
}

/** 설정 상태 본문에서 지금 값을 읽는다(칸이 없거나 수가 아니면 `null`). */
export function readAiLimit(status: unknown): number | null {
  if (typeof status !== "object" || status === null) return null;
  const value = (status as Record<string, unknown>)[AI_LIMIT_FIELD];
  return typeof value === "number" && Number.isInteger(value) && value >= 1 ? value : null;
}

/** 입력 칸 글자를 값으로: 비었으면 `null`(없음), 1 이상 정수면 그 수, 그 밖은 잘못(`ok: false`). */
export function parseAiLimit(input: string): { ok: true; limit: number | null } | { ok: false } {
  const text = input.trim();
  if (text === "") return { ok: true, limit: null };
  if (!/^[0-9]+$/.test(text)) return { ok: false };
  const limit = Number(text);
  if (!Number.isSafeInteger(limit) || limit < 1 || limit > 4_294_967_295) return { ok: false };
  return { ok: true, limit };
}

/** 바꾸는 요청: 경로는 AI 설정 저장 길, 본문은 `{"daily_limit": 값}` 하나. */
export function aiLimitRequest(limit: number | null): { path: string; body: Record<string, number | null> } {
  return { path: API.setupAi, body: { [AI_LIMIT_KEY]: limit } };
}
