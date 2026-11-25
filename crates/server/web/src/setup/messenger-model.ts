// 설정 화면의 메신저 묶음(server 39차 7번, 결정 24)이 쓰는 값 · 글 · 순수 함수. 그리기는 `messenger-view.ts`가 한다.
// 서버 계약: `crates/server/src/messenger.rs` 모듈 문서(`GET /api/messenger` · `POST` · `DELETE /api/messenger/telegram` ·
// `POST /api/messenger/pairing/approve` · `POST /api/messenger/pairing/revoke`).

/** 텔레그램 받기 상태(서버 `MESSENGER_STATE_*`). */
export type MessengerState = "off" | "polling" | "unauthorized" | "error";

/** 텔레그램 칸(`GET /api/messenger`의 `telegram`). 보낸 사람 ID · 토큰 · 코드는 오지 않는다. */
export interface TelegramStatus {
  configured: boolean;
  bot: string | null;
  state: MessengerState;
  paired: boolean;
  pending: number;
  /** 페어링 승인이 잠겼으면 풀리는 시각(유닉스 ms), 아니면 `null`. */
  locked_until: number | null;
}

/** `GET /api/messenger`의 몸통. `editable`이 거짓이면(다른 기기) 보기만 한다. */
export interface MessengerStatus {
  editable: boolean;
  telegram: TelegramStatus;
}

/** 상태 → 화면 글. */
export const STATE_LABELS: Readonly<Record<MessengerState, string>> = {
  off: "꺼짐",
  polling: "받는 중",
  unauthorized: "토큰 거절",
  error: "오류",
};

/** 페어링 안내(토큰이 있고 아직 연결된 사람이 없을 때). */
export const PAIRING_HINT = "봇에게 아무 말이나 보내면 코드가 옵니다";
/** 연결된 사람이 있을 때. */
export const PAIRED_TEXT = "연결됨";
/** 다른 기기에서 볼 때(보기만). 403의 글이기도 하다. */
export const REMOTE_NOTE = "이 컴퓨터에서만 바꿀 수 있습니다";
/** 슬랙 줄. */
export const SLACK_SOON = "슬랙은 준비 중입니다";
/** 시연 모드에서 등록하려 할 때(서버 `MESSENGER_DEMO_REFUSED`와 같은 글). */
export const DEMO_REFUSED = "시연 모드에서는 메신저를 쓰지 않습니다";
/** 틀린 코드(400, 서버 `MESSENGER_WRONG_CODE`와 같은 글). */
export const WRONG_CODE = "코드가 맞지 않습니다";
/** 잠김(423)의 앞부분(서버 `MESSENGER_LOCKED`와 같은 글). 뒤에 풀리는 시각이 붙는다. */
export const LOCKED_TEXT = "틀린 코드가 많아 잠시 승인을 받지 않습니다";
/** 이미 연결된 사람이 있음(409 `owner_exists`). */
export const OWNER_EXISTS = "이미 연결된 사람이 있습니다 — 먼저 빼세요";
/** 토큰이 거절됨(422 `key_rejected`). */
export const TOKEN_REJECTED = "봇 토큰이 맞지 않습니다";
/** 텔레그램에 닿지 못함(502 `unreachable`). */
export const UNREACHABLE = "텔레그램에 닿지 못했습니다";
/** 시도가 너무 많음(429 `too_many_attempts`). */
export const TOO_MANY = "시도가 너무 많습니다. 잠시 뒤에 다시 해 주세요";

/**
 * `GET /api/messenger` 몸통을 읽는다. 모양이 틀리면 `null`(칸 이름 · 형은 [`MessengerStatus`] 그대로, 모르는 상태
 * 글자도 `null`).
 */
export function parseMessengerStatus(body: unknown): MessengerStatus | null {
  void body;
  throw new Error("todo");
}

/** 잠긴 동안의 글: [`LOCKED_TEXT`] 뒤에 풀리는 시각(이 기기의 시각 `HH:MM`). */
export function lockedText(until: number): string {
  void until;
  throw new Error("todo");
}

/**
 * 실패한 답 → 화면 글: 403 → [`REMOTE_NOTE`], 400 `wrong_code` → [`WRONG_CODE`], 423 → [`lockedText`]
 * (`locked_until`), 409 `demo_mode` → 서버가 준 `message`(없으면 [`DEMO_REFUSED`]), 409 `owner_exists` →
 * [`OWNER_EXISTS`], 422 `key_rejected` → [`TOKEN_REJECTED`], 502 `unreachable` → [`UNREACHABLE`],
 * 429 → [`TOO_MANY`]. 그 밖은 `ui/call.ts`의 `LOAD_FAILED`. 남은 횟수 같은 숫자를 만들어 넣지 않는다.
 */
export function messengerError(status: number, body: unknown): string {
  void status;
  void body;
  throw new Error("todo");
}

/** 토큰 등록 몸통: 앞뒤 공백을 뗀다. */
export function registerBody(token: string): { token: string } {
  void token;
  throw new Error("todo");
}

/** 페어링 승인 몸통: 앞뒤 공백을 떼고 대문자로. */
export function approveBody(code: string): { code: string } {
  void code;
  throw new Error("todo");
}
