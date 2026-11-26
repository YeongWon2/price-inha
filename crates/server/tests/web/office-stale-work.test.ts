// "일하는 중" 풀기 잇기(server 24차 4번, additions.md R5-15 · E55 · N15 · V67): 서버(office 크레이트 `Office::tick`)가
// 300초 동안 새 소식이 없는 "일하는 중"을 한가로 풀면 `office.agent` 이벤트(work: idle)를 낸다. 화면은 그 이벤트로
// 그 담당의 생각 중 표시를 지우고, 그 담당은 다시 한가한 담당으로 셈해져 순찰할 수 있다. 옛 이벤트는 번호로 버린다.
import { describe, expect, it } from "vitest";
import { Chatter } from "../../web/src/office/chatter";
import { isFree } from "../../web/src/office/patrol";
import { SOURCES } from "./source-graph";

const T0 = 3_000_000;

describe("오래된 일하는 중 풀기", () => {
  it("서버가 풀어 보낸 한가 이벤트로 생각 중 표시가 지워진다", () => {
    // 검증: V67 E55 N15 R5-15
    const chatter = new Chatter();
    chatter.work("risk", "thinking", T0);
    expect(chatter.board.shown("risk", T0)?.kind).toBe("status");
    chatter.work("risk", "idle", T0 + 300_000);
    expect(chatter.board.shown("risk", T0 + 300_000)).toBeUndefined();
    expect(chatter.board.busy("risk", T0 + 300_000)).toBe(false);
  });

  it("풀린 담당은 다시 한가한 담당으로 셈한다(회의 · 일 · 부름 중이면 아니다)", () => {
    // 검증: V67 V65 N16
    expect(isFree({ place: "desk", work: "thinking" }, false)).toBe(false);
    expect(isFree({ place: "desk", work: "queued" }, false)).toBe(false);
    expect(isFree({ place: "desk", work: "idle" }, false)).toBe(true);
    expect(isFree({ place: "bed", work: "idle" }, false)).toBe(true);
    expect(isFree({ place: "meeting", work: "idle" }, false)).toBe(false);
    expect(isFree({ place: "desk", work: "idle" }, true)).toBe(false);
  });

  it("사무실 화면은 번호가 이미 본 것 이하인 이벤트를 버리고 한가 이벤트를 말풍선 · 표시에 넘긴다", () => {
    // 검증: V67 E55
    const main = SOURCES["../../web/src/office/main.ts"];
    expect(main).toMatch(/frame\.seq <= state\.lastSeq/);
    expect(main).toMatch(/chatter\.work\(view\.agent, next\.work\)/);
  });
});
