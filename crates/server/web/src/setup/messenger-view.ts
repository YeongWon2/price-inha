// 설정 화면의 "메신저" 묶음(server 39차 7번, 결정 24). 설정 보강(`extras.ts`의 `mountSetupExtras`)이 자리 하나
// (`data-role="messenger"`)에 [`mountMessenger`]로 단다. 공통 부품(`ui/parts.ts`) · 같은 출처 부르기(`ui/call.ts`)만 쓰고
// 서버가 준 글자는 글자로만 넣는다(브라우저 저장소 · 쿠키에 남기지 않는다).
//
// 그리는 자리(`data-role`, 모두 `[data-role="messenger"]` 안):
// - `messenger-state`: [`STATE_LABELS`]의 글, `data-state`에 상태 글자.
// - `messenger-bot`: 봇 이름이 있으면 `@봇이름`.
// - 이 컴퓨터(`editable`)면: 토큰 칸 `input#messenger-token`(`type=password` · `autocomplete=new-password`) ·
//   등록 `button[data-action="messenger-register"]`(→ `POST API.messengerTelegram` `{"token"}`), 토큰이 있으면 지우기
//   `button[data-action="messenger-remove"]`(→ `DELETE API.messengerTelegram`). 보낸 뒤에는 토큰 칸을 비운다.
//   - 토큰이 있고 연결된 사람이 없으면 `messenger-pairing`: [`PAIRING_HINT`] · 코드 칸 `input#messenger-code` ·
//     승인 `button[data-action="messenger-approve"]`(→ `POST API.messengerApprove` `{"code"}`).
//   - 연결된 사람이 있으면 `messenger-paired`: [`PAIRED_TEXT`] · 빼기 `button[data-action="messenger-revoke"]`
//     (→ `POST API.messengerRevoke`, 몸통 없음).
// - 다른 기기(`editable` 거짓)면: 입력 · 단추 없이 `messenger-remote`([`REMOTE_NOTE`]). 연결됐으면 [`PAIRED_TEXT`]는 보인다.
// - `messenger-slack`: [`SLACK_SOON`] 한 줄(늘).
// - `messenger-message`: 결과 자리. 실패면 오류 줄(`role="alert"`, [`messengerError`]의 글). 성공이면 상태를 다시
//   읽어(`GET API.messenger`) 묶음을 새로 그린다. 기다리는 동안 그 단추는 잠긴다.
// 읽기 실패: 이 묶음에 오류 줄(`LOAD_FAILED`)만 보이고 던지지 않는다.
import type { MessengerStatus } from "./messenger-model";

/** 상태 하나로 메신저 묶음(`data-role="messenger"`)을 그린다. 단추는 위 계약대로 서버를 부르고 다시 그린다. */
export function messengerBlock(status: MessengerStatus): HTMLElement {
  void status;
  throw new Error("todo");
}

/** 자리 하나에 메신저 묶음을 읽어 그린다(`GET API.messenger`). 읽기가 실패해도 던지지 않는다. */
export function mountMessenger(root: HTMLElement): Promise<void> {
  void root;
  throw new Error("todo");
}
