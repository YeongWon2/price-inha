// 익절 · 손절 연출(server 24차 3번, additions.md R5-14 · C26 · E60 · N12 · V66 — 원본 동작: docs/research/agent-office-refs.md
// 3-E · F 2, PixelOffice의 성공 1.5초 · 실패 2.0초): 체결 이벤트 `office.effect`가 익절이면 매매 담당 머리 위에 기쁨 표시와
// "+n%"가 1.5초, 손절이면 차분한 표시와 "-n%"가 2.0초 뜬다. 겹치면 줄 세워 하나씩, 화면이 숨어 있으면 쌓지 않고 버린다.
// 수익률은 서버가 세서 싣는다(화면은 그린다). 가짜 시계(밀리초)로 `src/office/effects.ts`를 돌려 본다.
import { describe, expect, it } from "vitest";
import {
  EFFECT_SHOW_MS,
  EFFECT_STALE_MS,
  EffectQueue,
  KIND_OFFICE_EFFECT,
  effectText,
  readEffect,
  type OfficeEffect,
} from "../../web/src/office/effects";
import { effectElement } from "../../web/src/office/effect-scene";
import { markOf } from "../../web/src/office/legend";
import { SOURCES, mainOf, reach } from "./source-graph";

const OFFICE = "../../web/src/office/";
const T0 = 7_000_000;

function effect(kind: OfficeEffect["kind"], trade: number, at: number, returnPct: number | null): OfficeEffect {
  return { kind, agent: "trader", trade, symbol: "000660", at, returnPct };
}

describe("연출 이벤트 읽기", () => {
  it("서버의 `office.effect` 모양을 읽고, 수익률 글자를 수로 바꾼다", () => {
    // 검증: V66 C26
    expect(KIND_OFFICE_EFFECT).toBe("office.effect");
    const payload = { kind: "take_profit", agent: "trader", trade: 7, symbol: "000660", at: T0, return_pct: "4.00" };
    expect(readEffect(payload)).toEqual(effect("take_profit", 7, T0, 4));
    const loss = { kind: "stop_loss", agent: "trader", trade: 8, symbol: "000660", at: T0, return_pct: "-6.09" };
    expect(readEffect(loss)?.returnPct).toBeCloseTo(-6.09, 6);
    const bare = { kind: "stop_loss", agent: "trader", trade: 9, symbol: "000660", at: T0 };
    expect(readEffect(bare)?.returnPct).toBeNull();
  });

  it("모양이 틀리면 읽지 않는다", () => {
    // 검증: V66 C26
    for (const bad of [
      null, "x", 3, {},
      { kind: "liquidation", agent: "trader", trade: 1, symbol: "000660", at: T0 },
      { kind: "take_profit", trade: 1, symbol: "000660", at: T0 },
      { kind: "take_profit", agent: "trader", trade: "1", symbol: "000660", at: T0 },
      { kind: "take_profit", agent: "trader", trade: 1, symbol: "000660" },
    ]) {
      expect(readEffect(bad), JSON.stringify(bad)).toBeNull();
    }
  });

  it("수익률 글은 부호와 소수 한 자리, 없으면 빈 글이다", () => {
    // 검증: V66
    expect(effectText(effect("take_profit", 1, T0, 4))).toBe("+4.0%");
    expect(effectText(effect("take_profit", 1, T0, 12.345))).toBe("+12.3%");
    expect(effectText(effect("stop_loss", 1, T0, -6.09))).toBe("-6.1%");
    expect(effectText(effect("stop_loss", 1, T0, null))).toBe("");
  });
});

describe("연출 줄", () => {
  it("익절은 기쁨 표시와 +n%가 1.5초, 손절은 차분한 표시와 -n%가 2.0초 보인다", () => {
    // 검증: V66 N12 C26 R5-14
    expect(EFFECT_SHOW_MS.take_profit).toBe(1_500);
    expect(EFFECT_SHOW_MS.stop_loss).toBe(2_000);
    const queue = new EffectQueue();
    expect(queue.push(effect("take_profit", 1, T0, 4), T0, false)).toBe(true);
    const shown = queue.current(T0);
    expect(shown?.agent).toBe("trader");
    expect(shown?.mark).toBe("profit");
    expect(shown?.glyph).toBe(markOf("profit").glyph);
    expect(shown?.color).toBe(markOf("profit").color);
    expect(shown?.text).toBe("+4.0%");
    expect(queue.current(T0 + 1_499)?.mark).toBe("profit");
    expect(queue.current(T0 + 1_500)).toBeUndefined();

    expect(queue.push(effect("stop_loss", 2, T0 + 5_000, -6.09), T0 + 5_000, false)).toBe(true);
    expect(queue.current(T0 + 5_000)?.mark).toBe("loss");
    expect(queue.current(T0 + 5_000)?.glyph).toBe(markOf("loss").glyph);
    expect(queue.current(T0 + 5_000)?.text).toBe("-6.1%");
    expect(queue.current(T0 + 6_999)?.mark).toBe("loss");
    expect(queue.current(T0 + 7_000)).toBeUndefined();
  });

  it("겹치면 줄 세워 하나씩 보인다", () => {
    // 검증: E60 V66
    const queue = new EffectQueue();
    queue.push(effect("take_profit", 1, T0, 4), T0, false);
    queue.push(effect("stop_loss", 2, T0 + 100, -5), T0 + 100, false);
    queue.push(effect("take_profit", 3, T0 + 200, 2), T0 + 200, false);
    expect(queue.current(T0 + 1_000)?.text).toBe("+4.0%");
    expect(queue.current(T0 + 1_500)?.text).toBe("-5.0%");
    expect(queue.current(T0 + 3_499)?.text).toBe("-5.0%");
    expect(queue.current(T0 + 3_500)?.text).toBe("+2.0%");
    expect(queue.current(T0 + 4_999)?.text).toBe("+2.0%");
    expect(queue.current(T0 + 5_000)).toBeUndefined();
  });

  it("화면이 숨어 있으면 쌓지 않고 버리고, 숨기면 줄을 비운다", () => {
    // 검증: E60 N17
    const queue = new EffectQueue();
    expect(queue.push(effect("take_profit", 1, T0, 4), T0, true)).toBe(false);
    expect(queue.current(T0)).toBeUndefined();
    queue.push(effect("take_profit", 2, T0, 4), T0, false);
    queue.push(effect("stop_loss", 3, T0, -5), T0, false);
    queue.clear();
    expect(queue.current(T0)).toBeUndefined();
    expect(queue.current(T0 + 2_000)).toBeUndefined();
    expect(queue.push(effect("stop_loss", 4, T0 + 10, -5), T0 + 10, false)).toBe(true);
    expect(queue.current(T0 + 10)?.text).toBe("-5.0%");
  });

  it("오래된 것(다시 이을 때 다시 받은 것)과 같은 매매의 같은 연출은 다시 보이지 않는다", () => {
    // 검증: E60 V66
    const queue = new EffectQueue();
    expect(queue.push(effect("take_profit", 1, T0 - EFFECT_STALE_MS - 1, 4), T0, false)).toBe(false);
    expect(queue.current(T0)).toBeUndefined();
    expect(queue.push(effect("take_profit", 2, T0, 4), T0, false)).toBe(true);
    expect(queue.push(effect("take_profit", 2, T0, 4), T0 + 10, false)).toBe(false);
    expect(queue.current(T0 + 1_600)).toBeUndefined();
  });
});

describe("연출 표시의 모양", () => {
  it("범례에 익절 · 손절 표시가 있고, 표시 요소는 글자 · 수익률 · 쉬운 뜻을 가진다", () => {
    // 검증: V66 R5-10
    for (const id of ["profit", "loss"] as const) {
      const mark = markOf(id);
      expect(mark.glyph.length, id).toBeGreaterThan(0);
      expect(mark.color.length, id).toBeGreaterThan(0);
      expect(mark.label.length, id).toBeGreaterThan(0);
    }
    expect(markOf("profit").glyph).not.toBe(markOf("loss").glyph);
    const queue = new EffectQueue();
    queue.push(effect("take_profit", 1, T0, 4), T0, false);
    const shown = queue.current(T0);
    expect(shown).toBeDefined();
    const el = effectElement(shown as NonNullable<typeof shown>, "매매");
    expect(el.dataset.effect).toBe("profit");
    expect(el.textContent).toContain(markOf("profit").glyph);
    expect(el.textContent).toContain("+4.0%");
    expect(el.getAttribute("aria-label")).toContain("매매");
    expect(el.getAttribute("aria-label")).toContain("+4.0%");
  });
});

describe("연출 모듈의 자리", () => {
  it("연출 모듈은 장면 · 바깥 이름 없이 돌고 서버를 따로 부르지 않으며 브라우저 저장소 글자가 없다", () => {
    // 검증: V66 E60
    for (const file of ["effects.ts", "effect-scene.ts"].map((f) => OFFICE + f)) {
      expect(file in SOURCES, file).toBe(true);
      expect([...reach(file).bare], file).toEqual([]);
      for (const banned of ["fetch(", "WebSocket", "Storage", "indexedDB", "cookie", "innerHTML"]) {
        expect(SOURCES[file].includes(banned), `${file} ${banned}`).toBe(false);
      }
    }
  });

  it("사무실 화면이 `office.effect` 이벤트를 연출 줄에 넣고, 탭이 숨으면 줄을 비운다", () => {
    // 검증: V66 E60
    const { files } = reach(mainOf("office"));
    expect(files.has(OFFICE + "effect-scene.ts")).toBe(true);
    expect(SOURCES[OFFICE + "main.ts"]).toMatch(/KIND_OFFICE_EFFECT/);
    const scene = SOURCES[OFFICE + "effect-scene.ts"];
    expect(scene).toMatch(/visibilitychange/);
    expect(scene).toMatch(/document\.hidden/);
  });
});
