// 사무실 승인 카드(server 10c 14번 · 11차 4번). `office.approval` 이벤트
// `{number, summary(가린 요약), short_code(지문 앞 6자), status}`로 카드를 그리고,
// 승인 · 거절은 폰과 같은 경로(`/api/approvals/{id}/approve|reject`) · 같은 지문 대조로 보낸다.
// N1 뼈대: 몸통은 N2가 채운다.
//
// 카드 모양(시험이 보는 약속):
// - 카드: `root` 아래 `[data-approval="<번호>"]` 하나.
// - 요약: `[data-part="summary"]`(글로만 넣는다 — textContent), 지문 앞 6자: `[data-part="code"]`,
//   결과 글: `[data-part="result"]`, 단추: `button[data-verb="approve"]` · `button[data-verb="reject"]`.

import type { Verb } from "../paths";

/** 사무실 승인 이벤트 종류. */
export const KIND_OFFICE_APPROVAL = "office.approval";

/** 이벤트 몸통. */
export interface OfficeApproval {
  number: number;
  summary: string;
  short_code: string;
  status: string;
}

/** `fetch`와 같은 모양(시험에서 바꿔 끼운다). */
export type FetchLike = (input: string, init?: RequestInit) => Promise<Response>;

/** 승인 · 거절의 결과: 보냄 · 지문이 카드와 다름(보내지 않음) · 실패. */
export type DecideOutcome = "sent" | "mismatch" | "failed";

/**
 * 이벤트 한 건을 카드에 반영한다. `pending`이면 그 번호의 카드를 그리거나 고치고(같은 번호는 한 장),
 * 아니면 그 카드를 닫는다(지운다). 모양이 맞지 않는 몸통은 무시한다.
 */
export function applyApproval(root: HTMLElement, payload: unknown, fetcher: FetchLike): void {
  throw new Error(`아직 없음(N2): applyApproval(${root.id}, ${typeof payload}, ${typeof fetcher})`);
}

/**
 * 승인 · 거절을 보낸다. 폰과 같이 `GET /api/approvals/{id}`로 전체 지문을 받아, 그 앞 6자가 카드에 보인
 * 지문 앞 6자와 같을 때만 `POST /api/approvals/{id}/{verb}` `{"fingerprint": 전체 지문}`을 보낸다.
 */
export async function decideApproval(
  root: HTMLElement,
  number: number,
  verb: Verb,
  fetcher: FetchLike,
): Promise<DecideOutcome> {
  throw new Error(`아직 없음(N2): decideApproval(${root.id}, ${number}, ${verb}, ${typeof fetcher})`);
}
