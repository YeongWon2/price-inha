// 알림음 켜기 · 끄기(server 31차 1번 (바), 31차 고침 · 결정 22): 서버가 둔다. 화면은 브라우저 저장소 · 쿠키 ·
// 주소에 값을 남기지 않는다(V46 · D11 · N8). 뒤 차수의 승인 알림음이 [`soundEnabled`]를 읽는다. 기본은 켜짐이다.
//
// - 읽기: `GET API.setupSound` → `{"enabled": bool}`. 실패(연결 · 상태 · 모양)면 기본(켜짐)이고 던지지 않는다.
// - 쓰기: `POST API.setupSound` `{"enabled": bool}`(같은 출처 부르기 `ui/call.ts`의 `call`) → 답은 같은 모양.
//   실패면 던지지 않는다.

/** 저장한 값이 없거나 읽지 못했을 때의 값(켜짐). */
export const SOUND_DEFAULT = true;

/** 알림음이 켜져 있나(서버에서 읽는다). 읽지 못하거나 답의 모양이 맞지 않으면 [`SOUND_DEFAULT`]. 던지지 않는다. */
export async function soundEnabled(): Promise<boolean> {
  throw new Error("todo");
}

/**
 * 알림음 켜기 · 끄기를 서버에 보내고 서버가 답한 값(저장한 뒤의 `enabled`)을 돌려준다.
 * 보내지 못했거나(연결) 거절됐거나(상태) 답의 모양이 맞지 않으면 `null`. 던지지 않는다.
 */
export async function saveSound(on: boolean): Promise<boolean | null> {
  throw new Error("todo");
}

/** 알림음 켜기 · 끄기를 서버에 남긴다. 남겼으면 `true`, 남기지 못했으면 `false`(던지지 않는다). */
export async function setSoundEnabled(on: boolean): Promise<boolean> {
  throw new Error("todo");
}
