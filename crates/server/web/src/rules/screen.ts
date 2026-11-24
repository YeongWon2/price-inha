// 규칙 화면(server 28차 2번 · F5 · C29 · V82 · E28 · E35 · E57): 지금 규칙 표 · 절대 상한 · 개선안 목록과 승인 · 거절.
// 서버는 `paths.ts`의 경로로만 부른다. 3D 라이브러리에 닿지 않는다(N21).
//
// 화면 계약(시험이 고르는 표시, 표 · 목록은 view.ts):
// - 띄우면 세션을 얻고(`POST /api/session`) 규칙(`GET /api/rules`)과 개선안 목록(`GET /api/proposals`)을 읽어 그린다.
//   템플릿이 없으면 규칙 표 자리에 빈 상태 `[data-role="no-template"]`, 개선안이 없으면 `[data-role="no-proposals"]`.
// - 제안됨 줄의 승인 · 거절 단추 → `POST /api/proposals/answer {id, number, fingerprint, answer: "approve" | "reject"}`
//   (번호 · 지문은 목록의 것을 그대로). 답한 뒤 규칙 · 목록을 다시 읽어 그리고, 결과 문구는 그 개선안 줄의
//   `[data-role="answer-result"]`에 남는다. 답하는 동안 그 줄의 단추는 막힌다.
// - 읽기에 실패하면 본문에 "불러오지 못함 · 다시 시도" 오류 줄(`role="alert"`)과 `button[data-action="retry"]`(E57).
//   메뉴는 그대로다.

/**
 * 본문 자리(`main`)에 규칙 화면을 그린다: 세션을 얻고 규칙 · 개선안 목록을 읽어 그린다.
 * 첫 그림이 끝나면 풀린다. 자리가 없으면 아무것도 하지 않는다.
 */
export async function mountRules(main: HTMLElement | null): Promise<void> {
  void main;
  throw new Error("todo");
}
