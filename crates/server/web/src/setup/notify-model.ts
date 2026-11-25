// 설정 화면의 "알림" 묶음(server 40차 6번, 결정 24)이 쓰는 값 · 글 · 순수 함수. 그리기는 `notify-view.ts`가 한다.
// 서버 계약: `crates/server/src/notify_settings.rs` 모듈 문서(`GET` · `POST /api/notify/settings`). 계산 · 검사는
// 서버가 하고 화면은 그린다(잠긴 종류 · 시각 글자 검사도 서버가 한다 — 화면은 서버의 400 · 403을 오류 줄로 보인다).

/**
 * 종류 한 줄(`GET`의 `kinds[]`). `kind`는 서버의 종류 글자(`approval_request` 등), `label`은 화면 글.
 * (server 41차 2번) `push`가 `null`이면 폰 칸이 없는 종류다(지금은 [`TEAM_TALK_KIND`] "팀 논의" 하나 — 메신저 칸만).
 */
export interface NotifyKindRow {
  kind: string;
  label: string;
  push: boolean | null;
  messenger: boolean;
  /** 끌 수 없는 종류(두 전달처를 모두 끌 수 없다). */
  locked: boolean;
}

/** 조용한 시간(`GET`의 `quiet`). 시각은 시장 시간대의 `HH:MM`. */
export interface QuietSetting {
  enabled: boolean;
  start: string;
  end: string;
}

/** `GET /api/notify/settings`의 몸통. `editable`이 거짓이면(다른 기기) 보기만 한다. */
export interface NotifySettings {
  kinds: NotifyKindRow[];
  quiet: QuietSetting;
  editable: boolean;
}

/** 묶음 제목. */
export const NOTIFY_TITLE = "알림";
/**
 * (server 41차 2번 · 결정 26) 팀 논의 종류 글자(서버 `NOTIFY_TEAM_TALK_KIND`). `GET`의 `kinds` 맨 끝에 `push: null`로 온다.
 * 표에는 알림 종류 줄(`tr[data-kind]`)과 따로 `tr[data-role="notify-talk"]` 한 줄로 그린다(폰 칸 자리에
 * [`NO_PUSH_TEXT`], 메신저 칸 `input[type=checkbox][name="messenger"]`만). 저장 몸통에는 받은 차례 그대로
 * `{"kind": "team_talk", "push": null, "messenger"}`로 싣는다.
 */
export const TEAM_TALK_KIND = "team_talk";
/** 폰 칸이 없는 줄의 폰 칸 자리 글. */
export const NO_PUSH_TEXT = "폰 없음";
/** 잠긴 줄의 표시 글. */
export const LOCKED_TEXT = "끌 수 없음";
/** 잠긴 종류 안내(표 아래 한 줄). */
export const LOCKED_NOTE = "승인 요청 · 멈춤 · 이상 알림은 적어도 한 곳으로는 갑니다";
/** 조용한 시간 안내. */
export const QUIET_NOTE = "조용한 시간에는 급하지 않은 알림을 미룹니다. 승인 요청 · 멈춤 같은 급한 알림은 그대로 갑니다";
/** 저장 성공. */
export const SAVED_TEXT = "저장했습니다";
/** 다른 기기에서 볼 때(보기만). 403의 글이기도 하다. */
export const REMOTE_NOTE = "이 컴퓨터에서만 바꿀 수 있습니다";
/** 잠긴 종류를 둘 다 끄려 함(400 `locked_kind_off`). 뒤에 " — <종류 글>"이 붙을 수 있다. */
export const LOCKED_OFF_TEXT = "이 알림은 폰과 메신저를 모두 끌 수 없습니다";
/** 조용한 시간의 시각 글자가 틀림(400 `invalid_quiet_hours`). */
export const INVALID_QUIET_TEXT = "조용한 시간의 시작과 끝을 HH:MM으로, 서로 다르게 적어 주세요";
/** 그 밖의 거부. */
export const SAVE_FAILED = "저장하지 못했습니다";

/** `GET` 몸통을 읽는다. 모양이 틀리면 `null`(종류 줄 하나라도 틀리면 `null`). */
export function parseNotifySettings(body: unknown): NotifySettings | null {
  if (!isObject(body) || !Array.isArray(body.kinds) || typeof body.editable !== "boolean") return null;
  const kinds: NotifyKindRow[] = [];
  for (const row of body.kinds) {
    const parsed = parseKind(row);
    if (parsed === null) return null;
    kinds.push(parsed);
  }
  const quiet = parseQuiet(body.quiet);
  if (quiet === null) return null;
  return { kinds, quiet, editable: body.editable };
}

/** 몸통이 JSON 객체(배열 아님)인가. */
function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

/** 종류 한 줄. 칸 하나라도 틀리면 `null`. */
function parseKind(row: unknown): NotifyKindRow | null {
  if (!isObject(row)) return null;
  const { kind, label, push, messenger, locked } = row;
  if (typeof kind !== "string" || typeof label !== "string") return null;
  if (typeof push !== "boolean" || typeof messenger !== "boolean" || typeof locked !== "boolean") return null;
  return { kind, label, push, messenger, locked };
}

/** 조용한 시간. 칸 하나라도 틀리면 `null`. */
function parseQuiet(quiet: unknown): QuietSetting | null {
  if (!isObject(quiet)) return null;
  const { enabled, start, end } = quiet;
  if (typeof enabled !== "boolean" || typeof start !== "string" || typeof end !== "string") return null;
  return { enabled, start, end };
}

/**
 * 바꾸기(`POST`) 몸통: `{"kinds": [{"kind", "push", "messenger"}], "quiet": {"enabled", "start", "end"}}` — 모든 종류를
 * 받은 차례 그대로 통째로 보낸다(`label` · `locked` · `editable`은 싣지 않는다).
 */
export function saveBody(settings: NotifySettings): unknown {
  return {
    kinds: settings.kinds.map(({ kind, push, messenger }) => ({ kind, push, messenger })),
    quiet: { enabled: settings.quiet.enabled, start: settings.quiet.start, end: settings.quiet.end },
  };
}

/**
 * 바꾸기 실패의 글: 403은 [`REMOTE_NOTE`], 400 `locked_kind_off`는 [`LOCKED_OFF_TEXT`](몸통의 `kind`가 `labels`에
 * 있으면 " — <종류 글>"을 잇는다), 400 `invalid_quiet_hours`는 [`INVALID_QUIET_TEXT`], 그 밖은 [`SAVE_FAILED`].
 */
export function notifyError(status: number, body: unknown, labels: readonly NotifyKindRow[] = []): string {
  if (status === 403) return REMOTE_NOTE;
  if (status !== 400 || !isObject(body)) return SAVE_FAILED;
  if (body.error === "locked_kind_off") {
    const label = labels.find((row) => row.kind === body.kind)?.label;
    return label === undefined ? LOCKED_OFF_TEXT : `${LOCKED_OFF_TEXT} — ${label}`;
  }
  if (body.error === "invalid_quiet_hours") return INVALID_QUIET_TEXT;
  return SAVE_FAILED;
}
