// 설정 화면 메신저 묶음의 값 · 글(server 39차 7번 · 결정 24 · V88): 상태 몸통 읽기, 오류 답 → 화면 글(400 · 403 · 423 ·
// 409 · 422 · 502), 등록 · 승인 몸통 모양.
import { describe, expect, it } from "vitest";
import { LOAD_FAILED } from "../../web/src/ui/call";
import {
  DEMO_REFUSED,
  LOCKED_TEXT,
  OWNER_EXISTS,
  REMOTE_NOTE,
  STATE_LABELS,
  TOKEN_REJECTED,
  TOO_MANY,
  UNREACHABLE,
  WRONG_CODE,
  approveBody,
  lockedText,
  messengerError,
  parseMessengerStatus,
  registerBody,
} from "../../web/src/setup/messenger-model";
import type { MessengerStatus } from "../../web/src/setup/messenger-model";

const STATUS: MessengerStatus = {
  editable: true,
  telegram: { configured: true, bot: "siheom_web_bot", state: "polling", paired: false, pending: 1, locked_until: null },
};

describe("메신저 상태 몸통", () => {
  it("서버 몸통을 그대로 읽는다", () => {
    // 검증: V88 C2
    expect(parseMessengerStatus(JSON.parse(JSON.stringify(STATUS)))).toEqual(STATUS);
    const locked = { ...STATUS, telegram: { ...STATUS.telegram, locked_until: 1_790_000_000_000 } };
    expect(parseMessengerStatus(locked)?.telegram.locked_until).toBe(1_790_000_000_000);
  });

  it("모양이 틀리거나 모르는 상태 글자면 null이다", () => {
    // 검증: V88 E57
    expect(parseMessengerStatus(null)).toBeNull();
    expect(parseMessengerStatus({ editable: true })).toBeNull();
    expect(parseMessengerStatus({ ...STATUS, telegram: { ...STATUS.telegram, state: "zz_unknown" } })).toBeNull();
    expect(parseMessengerStatus({ ...STATUS, telegram: { ...STATUS.telegram, paired: "yes" } })).toBeNull();
  });

  it("상태 글은 꺼짐 · 받는 중 · 토큰 거절 · 오류다", () => {
    // 검증: V88
    expect(STATE_LABELS).toEqual({ off: "꺼짐", polling: "받는 중", unauthorized: "토큰 거절", error: "오류" });
  });
});

describe("메신저 오류 글", () => {
  it("400 틀린 코드 · 403 다른 기기 · 409 시연 · 이미 연결 · 422 · 502 · 429를 화면 글로 바꾼다", () => {
    // 검증: V88 D2 V87
    expect(messengerError(400, { error: "wrong_code", message: WRONG_CODE })).toBe(WRONG_CODE);
    expect(messengerError(403, null)).toBe(REMOTE_NOTE);
    expect(messengerError(409, { error: "demo_mode", message: DEMO_REFUSED })).toBe(DEMO_REFUSED);
    expect(messengerError(409, { error: "demo_mode" })).toBe(DEMO_REFUSED);
    expect(messengerError(409, { error: "owner_exists", message: OWNER_EXISTS })).toBe(OWNER_EXISTS);
    expect(messengerError(422, { error: "key_rejected" })).toBe(TOKEN_REJECTED);
    expect(messengerError(502, { error: "unreachable" })).toBe(UNREACHABLE);
    expect(messengerError(429, { error: "too_many_attempts" })).toBe(TOO_MANY);
    expect(messengerError(500, { error: "internal" })).toBe(LOAD_FAILED);
  });

  it("423이면 잠김 글과 풀리는 시각을 보이고 남은 횟수는 만들지 않는다", () => {
    // 검증: V88
    const until = new Date(2026, 10, 16, 11, 5).getTime();
    const text = messengerError(423, { error: "locked", message: LOCKED_TEXT, locked_until: until });
    expect(text).toBe(lockedText(until));
    expect(text).toContain(LOCKED_TEXT);
    expect(text).toContain("11:05");
    expect(text).not.toContain("남은");
    expect(text).not.toContain("NaN");
  });
});

describe("메신저 요청 몸통", () => {
  it("토큰은 앞뒤 공백만 떼고, 코드는 앞뒤 공백을 떼어 대문자로 보낸다", () => {
    // 검증: V87 V88
    expect(registerBody("  SIHEOM-WEB-TOKEN-0001 \n")).toEqual({ token: "SIHEOM-WEB-TOKEN-0001" });
    expect(approveBody(" abcd2345 ")).toEqual({ code: "ABCD2345" });
  });
});
