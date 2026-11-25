// 아이콘 범례(server 34차 4번, additions.md R5-10 · V62 — 원본 동작: docs/research/agent-office-refs.md F 2):
// 머리 위 표시의 정의 한 벌 · 범례 DOM(단추로 열고 닫음, 기본 닫힘) · 범례와 머리 위 표시가 같은 정의를 씀.
// 그리고 34차 네 모듈이 장면(three) 없이 돌고 사무실 화면에 이어졌는지 소스로 본다.
import { describe, expect, it } from "vitest";
import { OVERHEAD_MARKS, markOf, renderLegend, type MarkId } from "../../web/src/office/legend";
import { STATE_ICONS, TYPING_TEXT, type SpeechState } from "../../web/src/office/speech";
import { SOURCES, mainOf, reach } from "./source-graph";

const OFFICE = "../../web/src/office/";
const NEW_MODULES = ["legend.ts", "approval-bubbles.ts", "chime.ts", "done-mark.ts"].map((f) => OFFICE + f);
const ALL: MarkId[] = ["thinking", "writing", "typing", "waiting", "failed", "approval", "done"];

/** 그 색을 브라우저가 읽은 모양으로(글자색 비교용). */
function asColor(color: string): string {
  const probe = document.createElement("span");
  probe.style.color = color;
  return probe.style.color;
}

describe("머리 위 표시의 정의", () => {
  it("지금 쓰는 표시 모두가 한 벌에 있고 이름이 겹치지 않으며 글자 · 색 · 뜻이 있다", () => {
    // 검증: V62 R5-10
    const ids = OVERHEAD_MARKS.map((m) => m.id);
    expect([...ids].sort()).toEqual([...ALL].sort());
    expect(new Set(ids).size).toBe(ids.length);
    for (const mark of OVERHEAD_MARKS) {
      expect(mark.glyph.length, mark.id).toBeGreaterThan(0);
      expect(mark.color.length, mark.id).toBeGreaterThan(0);
      expect(mark.label.length, mark.id).toBeGreaterThan(0);
      expect(markOf(mark.id)).toEqual(mark);
    }
    expect(markOf("approval").glyph).toBe("✋");
    expect(markOf("done").glyph).toBe("✓");
  });

  it("말풍선의 상태 표시 · 말하는 중 글자는 범례와 같은 정의다", () => {
    // 검증: V62 V14
    const states: SpeechState[] = ["thinking", "writing", "waiting", "failed"];
    for (const state of states) expect(STATE_ICONS[state], state).toBe(markOf(state).glyph);
    expect(TYPING_TEXT).toBe(markOf("typing").glyph);
    expect(STATE_ICONS).toEqual({ thinking: "💭", writing: "💬", waiting: "⏳", failed: "❗" });
  });

  it("말풍선 · ✋ · ✓ 모듈은 legend.ts의 정의를 가져다 쓴다(한곳에서 정의)", () => {
    // 검증: V62
    for (const file of ["speech.ts", "approval-bubbles.ts", "done-mark.ts"]) {
      expect(SOURCES[OFFICE + file], file).toMatch(/from\s+["']\.\/legend["']/);
    }
  });
});

describe("범례 DOM", () => {
  it("단추로 열고 닫으며 처음에는 닫혀 있다", () => {
    // 검증: V62 R5-10
    const root = document.createElement("div");
    const legend = renderLegend(root);
    const toggle = root.querySelector<HTMLButtonElement>("button[data-legend-toggle]");
    expect(toggle).not.toBeNull();
    expect(root.querySelectorAll("[data-legend]").length).toBe(1);
    expect(root.querySelector("[data-legend]")).toBe(legend);
    expect(legend.hidden).toBe(true);
    expect(toggle?.getAttribute("aria-expanded")).toBe("false");
    toggle?.click();
    expect(legend.hidden).toBe(false);
    expect(toggle?.getAttribute("aria-expanded")).toBe("true");
    toggle?.click();
    expect(legend.hidden).toBe(true);
    expect(toggle?.getAttribute("aria-expanded")).toBe("false");
  });

  it("표시마다 한 줄씩 정의 차례대로, 머리 위와 같은 글자 · 색과 뜻을 보인다", () => {
    // 검증: V62 R5-10
    const root = document.createElement("div");
    const legend = renderLegend(root);
    const rows = Array.from(legend.querySelectorAll<HTMLElement>("[data-mark]"));
    expect(rows.map((r) => r.dataset.mark)).toEqual(OVERHEAD_MARKS.map((m) => m.id));
    for (const [i, rowEl] of rows.entries()) {
      const mark = OVERHEAD_MARKS[i];
      const glyph = rowEl.querySelector<HTMLElement>('[data-part="glyph"]');
      const label = rowEl.querySelector<HTMLElement>('[data-part="label"]');
      expect(glyph?.textContent, mark.id).toBe(mark.glyph);
      expect(glyph?.style.color, mark.id).toBe(asColor(mark.color));
      expect(label?.textContent, mark.id).toBe(mark.label);
    }
  });
});

describe("34차 모듈의 자리", () => {
  it("네 모듈은 장면(three) · 바깥 이름 없이 돌고 브라우저 저장소 글자가 없다", () => {
    // 검증: V62 N8 D11
    for (const file of NEW_MODULES) {
      expect(file in SOURCES, file).toBe(true);
      expect([...reach(file).bare], file).toEqual([]);
      for (const banned of ["Storage", "indexedDB", "cookie", "innerHTML"]) {
        expect(SOURCES[file].includes(banned), `${file} ${banned}`).toBe(false);
      }
    }
  });

  it("사무실 화면이 네 모듈과 알림음 설정을 잇는다", () => {
    // 검증: V62 E50
    const { files } = reach(mainOf("office"));
    for (const file of NEW_MODULES) expect(files.has(file), file).toBe(true);
    expect(files.has("../../web/src/ui/sound-pref.ts")).toBe(true);
  });
});
