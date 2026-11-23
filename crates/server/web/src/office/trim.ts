// 응답 목록 정리(server 12차 3번 · 옛 office `web_prune`, C5): 대화 줄이 넘치면 오래된 줄을 버리고,
// 버린 줄의 응답이 끝난 것(complete · failed)이면 응답 표에서도 지운다. 오는 중인 응답은 남긴다.

/** 한 대화에 남기는 줄 수(사무실 화면과 같다). */
export const KEEP_LINES = 200;

/** 채팅 줄: 내 말 또는 응답 자리. */
export type Line =
  | { kind: "me"; id: string; room: string; agent: string; text: string; failed?: boolean }
  | { kind: "resp"; requestId: string };

/** 응답 표의 값에서 정리가 보는 부분(상태). */
export interface ResponseStatus {
  status: string;
}

/**
 * `lines`가 `keep`줄을 넘으면 앞(오래된 쪽)에서 넘친 만큼 버린다(`lines`를 그 자리에서 고친다).
 * 버린 응답 줄의 응답이 끝난 것(`complete` · `failed`)이면 `responses`에서도 지운다.
 */
export function trimLines(
  lines: Line[],
  responses: Map<string, ResponseStatus>,
  keep: number = KEEP_LINES,
): void {
  if (lines.length <= keep) { return; }
  for (const line of lines.splice(0, lines.length - keep)) {
    if (line.kind !== "resp") { continue; }
    const status = responses.get(line.requestId)?.status;
    if (status === "complete" || status === "failed") { responses.delete(line.requestId); }
  }
}
