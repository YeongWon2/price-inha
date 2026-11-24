// 설정 화면에 하루 AI 호출 상한(server 22차 6번): 지금 값(없음 / N번)을 보이고, 바꾸는 칸의 글자를 값으로 읽고,
// 바꾸기는 AI 설정 저장 길(`POST /api/setup/ai`의 `daily_limit` 칸 하나)로 보낸다 — 새 경로를 더하지 않는다.
import { describe, expect, it } from "vitest";
import { API, SCREEN_PATHS } from "../../web/src/paths";
import {
  AI_LIMIT_FIELD,
  AI_LIMIT_KEY,
  aiLimitRequest,
  formatAiLimit,
  parseAiLimit,
  readAiLimit,
} from "../../web/src/setup/ai-limit";

describe("하루 AI 호출 상한 칸", () => {
  it("지금 값은 없음 또는 N번으로 보인다", () => {
    // 검증: E30 N5
    expect(formatAiLimit(null)).toBe("없음");
    expect(formatAiLimit(1)).toBe("1번");
    expect(formatAiLimit(200)).toBe("200번");
  });

  it("설정 상태의 ai_daily_limit 칸에서 지금 값을 읽는다", () => {
    // 검증: E30
    expect(AI_LIMIT_FIELD).toBe("ai_daily_limit");
    expect(readAiLimit({ ai_daily_limit: 5, demo: true })).toBe(5);
    expect(readAiLimit({ ai_daily_limit: null })).toBeNull();
    expect(readAiLimit({ demo: true })).toBeNull();
    expect(readAiLimit({ ai_daily_limit: "5" })).toBeNull();
    expect(readAiLimit(null)).toBeNull();
  });

  it("바꾸는 칸: 비우면 없음, 1 이상 정수는 그 수, 그 밖은 잘못이다", () => {
    // 검증: E30 C3
    expect(parseAiLimit("")).toEqual({ ok: true, limit: null });
    expect(parseAiLimit("  ")).toEqual({ ok: true, limit: null });
    expect(parseAiLimit("3")).toEqual({ ok: true, limit: 3 });
    expect(parseAiLimit(" 40 ")).toEqual({ ok: true, limit: 40 });
    for (const bad of ["0", "-1", "1.5", "abc", "3번", "1e3"]) {
      expect(parseAiLimit(bad), bad).toEqual({ ok: false });
    }
  });

  it("바꾸기는 AI 설정 저장 길로 daily_limit 칸 하나만 보낸다", () => {
    // 검증: E30 D11
    expect(AI_LIMIT_KEY).toBe("daily_limit");
    expect(aiLimitRequest(3)).toEqual({ path: API.setupAi, body: { daily_limit: 3 } });
    expect(aiLimitRequest(null)).toEqual({ path: API.setupAi, body: { daily_limit: null } });
    expect(SCREEN_PATHS.setup).toContain(aiLimitRequest(3).path);
  });
});
