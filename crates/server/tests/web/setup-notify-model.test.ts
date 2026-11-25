// 설정 화면 알림 묶음의 화면 모델(server 40차 4 · 6번 · 결정 24 · V91): 서버 몸통 읽기(모양이 틀리면 없음) · 바꾸기
// 몸통 모양(통째, label · locked · editable 없음) · 오류 글(400 잠긴 종류 · 400 시각 · 403 · 그 밖).
import { describe, expect, it } from "vitest";
import {
  INVALID_QUIET_TEXT,
  LOCKED_OFF_TEXT,
  REMOTE_NOTE,
  SAVE_FAILED,
  notifyError,
  parseNotifySettings,
  saveBody,
} from "../../web/src/setup/notify-model";
import { notifySettings } from "./notify-fixture";

describe("알림 묶음 · 화면 모델", () => {
  it("서버 몸통을 그대로 읽는다", () => {
    // 검증: V91 C2
    const body = notifySettings();
    expect(parseNotifySettings(body)).toEqual(body);
    const remote = notifySettings({ editable: false });
    expect(parseNotifySettings(remote)?.editable).toBe(false);
  });

  it("모양이 틀리면 없음이다", () => {
    // 검증: V91 E4
    const good = notifySettings();
    const broken: unknown[] = [
      null,
      "글자",
      [],
      { ...good, kinds: {} },
      { ...good, kinds: [{ ...good.kinds[0], push: "true" }] },
      { ...good, kinds: [{ ...good.kinds[0], kind: 7 }] },
      { ...good, quiet: null },
      { ...good, quiet: { ...good.quiet, enabled: "yes" } },
      { ...good, quiet: { ...good.quiet, start: 2200 } },
      { ...good, editable: "true" },
    ];
    for (const body of broken) expect(parseNotifySettings(body), JSON.stringify(body)).toBeNull();
  });

  it("바꾸기 몸통은 모든 종류를 받은 차례로 통째 싣고 화면용 칸은 싣지 않는다", () => {
    // 검증: V91 C15
    const settings = notifySettings();
    settings.kinds[5] = { ...settings.kinds[5], push: false };
    settings.quiet = { enabled: true, start: "23:00", end: "06:30" };
    expect(saveBody(settings)).toEqual({
      kinds: settings.kinds.map((row) => ({ kind: row.kind, push: row.push, messenger: row.messenger })),
      quiet: { enabled: true, start: "23:00", end: "06:30" },
    });
  });

  it("오류 글: 403은 이 컴퓨터에서만, 400은 까닭별, 그 밖은 저장 실패", () => {
    // 검증: V91 E4 C15
    const kinds = notifySettings().kinds;
    expect(notifyError(403, { error: "local_only" })).toBe(REMOTE_NOTE);
    expect(notifyError(400, { error: "locked_kind_off", kind: "approval_request" }, kinds)).toBe(
      `${LOCKED_OFF_TEXT} — 승인 요청`,
    );
    expect(notifyError(400, { error: "locked_kind_off" })).toBe(LOCKED_OFF_TEXT);
    expect(notifyError(400, { error: "invalid_quiet_hours" })).toBe(INVALID_QUIET_TEXT);
    expect(notifyError(400, { error: "invalid_input" })).toBe(SAVE_FAILED);
    expect(notifyError(500, null)).toBe(SAVE_FAILED);
    expect(notifyError(0, null)).toBe(SAVE_FAILED);
  });
});
