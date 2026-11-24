// 알림음 켜기 · 끄기(server 31차 1번 (바)): 이 기기(브라우저)의 웹 저장소에 키 하나([`SOUND_PREF_KEY`])로만 남는다 —
// 서버에 보내지 않는다. 뒤 차수의 승인 알림음이 [`soundEnabled`]를 읽는다. 기본은 켜짐이다.
// 저장소를 쓸 수 없는 브라우저(막힘 · 사생활 모드 · 꽉 참)에서도 던지지 않는다: 읽기는 기본(켜짐)으로, 쓰기는
// `false`(남기지 못함)로 끝난다.

/** 웹 저장소의 키 하나. 값은 [`SOUND_ON`] · [`SOUND_OFF`]. */
export const SOUND_PREF_KEY = "price-inha.sound";
/** 켜짐 값. */
export const SOUND_ON = "on";
/** 꺼짐 값. */
export const SOUND_OFF = "off";

/** 알림음이 켜져 있나. 남긴 값이 [`SOUND_OFF`]일 때만 꺼짐이고, 없거나 읽지 못하면 켜짐이다. */
export function soundEnabled(): boolean {
  throw new Error("todo");
}

/** 알림음 켜기 · 끄기를 이 기기에 남긴다. 남겼으면 `true`, 저장소를 쓸 수 없으면 `false`(던지지 않는다). */
export function setSoundEnabled(on: boolean): boolean {
  throw new Error("todo");
}
