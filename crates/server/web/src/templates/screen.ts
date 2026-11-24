// 템플릿 화면(server 27차 3번 · F6 · C28 · E5 · E28 · E57 · E58): 고르기 · 값 맞추기 · 상태 · 승격 기준 · 판 이력 ·
// 실전 운용 시작 · 멈추기. 서버는 `paths.ts`의 경로로만 부른다. 3D 라이브러리에 닿지 않는다(N21).
//
// 화면 계약(시험이 고르는 표시, 칸 · 표 · 이력은 view.ts):
// - 스타일 고르기 `[data-role="styles"] button[data-style=단타|스윙|중장기의 글자]` → `POST /api/templates {style}` 뒤
//   목록을 다시 읽는다.
// - 지금 템플릿 상태 배지 `[data-role="state"]`, 값 저장 `button[data-action="save"]` → `POST /api/templates/adjust`
//   `{id, changes}`, 결과 문구 `[data-role="save-result"]`(상한 넘음은 칸 옆 안내도).
// - "실전 운용 시작" `button[data-action="promote"]`: 승격대기일 때만 누를 수 있다. 누르면 승인 카드
//   `GET /api/approvals/{번호}`를 읽고 확인(`confirm`) 뒤에만 `POST /api/approvals/{번호}/approve {fingerprint}`.
//   그 옆에 실전 안내 `[data-role="live-notice"]`가 상태와 상관없이 늘 보인다.
// - "멈추기" `button[data-action="stop"]`: 실전중일 때만 있다. 확인(`confirm`) 뒤에만 `POST /api/templates/{id}/stop`,
//   그 뒤 목록을 다시 읽는다.
// - 읽기에 실패하면 본문에 "불러오지 못함 · 다시 시도" 오류 줄과 `button[data-action="retry"]`(E57). 메뉴는 그대로다.

/**
 * 본문 자리(`main`)에 템플릿 화면을 그린다: 세션을 얻고(`POST /api/session`) 목록을 읽어 그린다.
 * 첫 그림이 끝나면 풀린다. 자리가 없으면 아무것도 하지 않는다.
 */
export async function mountTemplates(main: HTMLElement | null): Promise<void> {
  void main;
  throw new Error("todo");
}
