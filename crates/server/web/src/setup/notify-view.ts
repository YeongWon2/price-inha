// 설정 화면의 "알림" 묶음(server 40차 6번, 결정 24). 설정 보강(`extras.ts`의 `mountSetupExtras`)이 자리 하나
// (`data-role="notify"`)에 [`mountNotify`]로 단다(N2가 단다). 공통 부품(`ui/parts.ts`) · 같은 출처 부르기(`ui/call.ts`)만
// 쓰고 서버가 준 글자는 글자로만 넣는다(브라우저 저장소 · 쿠키에 남기지 않는다). 계산 · 검사는 서버가 한다.
//
// 그리는 자리(`data-role`, 모두 `[data-role="notify"]` 안):
// - `notify-kinds`: 표. 종류마다 `tr[data-kind="<종류 글자>"]` 한 줄(서버가 준 차례) — 종류 글(`label`) · 폰 칸
//   `input[type=checkbox][name="push"]` · 메신저 칸 `input[type=checkbox][name="messenger"]`. 잠긴 줄은
//   `data-locked="true"`이고 [`LOCKED_TEXT`]가 보인다(칸은 그대로 켜고 끌 수 있다 — 둘 다 끄면 서버가 400).
// - `notify-quiet`: 조용한 시간 — 켜기 `input#notify-quiet-on`(checkbox) · 시작 `input#notify-quiet-start` · 끝
//   `input#notify-quiet-end`(값은 `HH:MM` 글자) · [`QUIET_NOTE`].
// - 이 컴퓨터(`editable`)면 저장 `button[data-action="notify-save"]` → `POST API.notifySettings`(`saveBody`의 모양,
//   지금 칸 값으로). 기다리는 동안 단추는 잠긴다. 성공이면 받은 몸통으로 묶음을 다시 그리고 `notify-message`에
//   [`SAVED_TEXT`](`role="status"`). 실패(400 · 403 · 그 밖)면 `notify-message`에 오류 줄(`role="alert"`,
//   `notifyError`의 글)이고 칸 값은 그대로 둔다.
// - 다른 기기(`editable` 거짓)면 저장 단추가 없고 모든 칸이 잠기며(`disabled`) `notify-remote`([`REMOTE_NOTE`])가 보인다.
// 읽기 실패: 이 묶음에 오류 줄(`LOAD_FAILED`)만 보이고 던지지 않는다.
import type { NotifySettings } from "./notify-model";

/** 설정 하나로 알림 묶음(`data-role="notify"`)을 그린다. 단추는 위 계약대로 서버를 부르고 다시 그린다. */
export function notifyBlock(settings: NotifySettings): HTMLElement {
  void settings;
  throw new Error("todo");
}

/** 자리 하나에 알림 묶음을 읽어 그린다(`GET API.notifySettings`). 읽기가 실패해도 던지지 않는다. */
export function mountNotify(root: HTMLElement): Promise<void> {
  void root;
  throw new Error("todo");
}
