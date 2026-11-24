// 계좌 · 운용 화면(server 30차 2번 · 3번 · C31 · V85 · V86 · R3-1 · R3-3 · R14-1 · R16-5 · F1-6 · E57): 팀 상태 카드 ·
// 다시 시작(전체 정지일 때만) · 계좌 카드 · 보유 표 · 늘 보이는 모의계좌 안내. 3D 라이브러리에 닿지 않는다(N21).
// 서버는 `paths.ts`의 경로로만 부른다.
//
// 화면 계약(시험이 고르는 표시, 카드 · 표는 view.ts):
// - 띄우면 세션을 얻고(`POST /api/session`) 팀 상태(`GET /api/team`)와 계좌(`GET /api/account`)를 읽어 그린다.
// - 모의계좌 안내(`data-role="paper-notice"`, `PAPER_NOTICE`)는 읽기 전 · 읽은 뒤 · 읽기 실패 · 어느 팀 상태에서도 보인다.
// - 전체 정지일 때만 `button[data-action="restart"]`가 있다. 누르면 `window.confirm(RESTART_CONFIRM)`을 묻고, 예일 때만
//   `POST /api/team/restart`(몸통 없음)를 보낸다. 결과(`restartText`)는 `data-role="restart-result"`(`role="status"`)에
//   보이고, 그 뒤 팀 상태와 계좌를 다시 읽는다. 아니오면 아무것도 보내지 않는다.
// - 읽기(팀 상태 · 계좌 가운데 하나라도)에 실패하면 `role="alert"` 오류 줄(LOAD_FAILED)과 `button[data-action="retry"]`,
//   다시 시도하면 둘을 다시 읽는다.
// - 실시간: 같은 출처의 `/ws`(`ws:` · `wss:`)를 열고 `REFRESH_KINDS`의 이벤트를 받으면 둘을 다시 읽는다. 이와 별개로
//   `REFRESH_MS`마다 둘을 다시 읽는다.

/**
 * 본문 자리(`main`)에 계좌 · 운용 화면을 그린다: 세션을 얻고 팀 상태 · 계좌를 읽어 그린다.
 * 첫 그림이 끝나면 풀린다. 자리가 없으면 아무것도 하지 않는다.
 */
export async function mountAccount(main: HTMLElement | null): Promise<void> {
  void main;
  throw new Error("todo");
}
