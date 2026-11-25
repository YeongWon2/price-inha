// 끝남 표시(server 34차 3번, additions.md N12 · V62 — 원본 동작: docs/research/agent-office-refs.md 1-B · F 1):
// 일이 끝나면 ✓가 2.0초 보이고 0.5초 동안 흐려져 사라짐 · 그 사이 새 일이 시작되면 바로 지움. 가짜 시계(밀리초)로
// `src/office/done-mark.ts`를 돌려 본다.
import { describe, expect, it } from "vitest";
import { DONE_FADE_MS, DONE_SHOW_MS, DoneMarks } from "../../web/src/office/done-mark";
import { markOf } from "../../web/src/office/legend";

const T0 = 5_000_000;

describe("끝남 표시", () => {
  it("끝나면 ✓가 2.0초 온전히 보이고 0.5초 동안 흐려져 사라진다", () => {
    // 검증: V62 N12
    expect(DONE_SHOW_MS).toBe(2_000);
    expect(DONE_FADE_MS).toBe(500);
    const marks = new DoneMarks();
    marks.finish("risk", T0);
    const first = marks.shown("risk", T0);
    expect(first?.mark).toBe("done");
    expect(first?.text).toBe(markOf("done").glyph);
    expect(markOf("done").glyph).toBe("✓");
    expect(first?.color).toBe(markOf("done").color);
    expect(first?.opacity).toBe(1);
    expect(marks.shown("risk", T0 + 1_999)?.opacity).toBe(1);
    expect(marks.shown("risk", T0 + 2_000)?.opacity).toBeCloseTo(1, 6);
    expect(marks.shown("risk", T0 + 2_250)?.opacity).toBeCloseTo(0.5, 6);
    const last = marks.shown("risk", T0 + 2_499)?.opacity ?? 0;
    expect(last).toBeGreaterThan(0);
    expect(last).toBeLessThan(0.01);
    expect(marks.shown("risk", T0 + 2_500)).toBeUndefined();
    expect(marks.shown("risk", T0 + 60_000)).toBeUndefined();
  });

  it("보이는 사이 새 일이 시작되면 바로 지운다", () => {
    // 검증: V62 N12
    const marks = new DoneMarks();
    marks.finish("news", T0);
    marks.start("news", T0 + 800);
    expect(marks.shown("news", T0 + 800)).toBeUndefined();
    expect(marks.shown("news", T0 + 1_000)).toBeUndefined();
    marks.finish("news", T0 + 2_200);
    marks.start("news", T0 + 4_300);
    expect(marks.shown("news", T0 + 4_300)).toBeUndefined();
  });

  it("다시 끝나면 그때부터 다시 세고, 담당마다 따로다", () => {
    // 검증: V62 N12
    const marks = new DoneMarks();
    marks.finish("trader", T0);
    marks.finish("trader", T0 + 1_500);
    expect(marks.shown("trader", T0 + 3_000)?.opacity).toBe(1);
    expect(marks.shown("trader", T0 + 4_000)).toBeUndefined();
    marks.finish("review", T0);
    expect(marks.shown("leader", T0)).toBeUndefined();
    marks.start("leader", T0 + 10);
    expect(marks.shown("review", T0 + 10)?.opacity).toBe(1);
  });
});
