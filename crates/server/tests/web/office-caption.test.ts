// 화면 아래 자막(server 23차 6번) · 회의 "○○ 발언 중…"(23차 7번 (가)), 원본 DeskRPG 화면 —
// docs/research/deskrpg.md 11절. 시각은 시험이 넘기는 밀리초(가짜 시계)다.
import { describe, expect, it } from "vitest";
import { CAPTION_LIFE_MS, SPEAKING_LIFE_MS, caption, speakingLine } from "../../web/src/office/caption";

const T0 = 1_000_000;
const NAMES: Record<string, string> = { risk: "리스크 담당", leader: "팀장" };

describe("화면 아래 자막", () => {
  const said = [
    { agent: "risk", text: "손절선을 지킵니다", at: T0 },
    { agent: "leader", text: "  오늘은   지켜봅니다 ", at: T0 + 2_000 },
  ];

  it("가장 최근 말을 `이름: 글`로(글은 다듬은 것)", () => {
    // 검증: V14
    expect(caption(said, NAMES, T0 + 3_000)).toBe("팀장: 오늘은 지켜봅니다");
  });

  it("넘긴 순서가 아니라 시각으로 가장 최근 것을 고른다", () => {
    // 검증: V14
    expect(caption([...said].reverse(), NAMES, T0 + 3_000)).toBe("팀장: 오늘은 지켜봅니다");
  });

  it("12초가 지나면 숨긴다(null)", () => {
    // 검증: V14
    expect(CAPTION_LIFE_MS).toBe(12_000);
    expect(caption(said, NAMES, T0 + 2_000 + 11_900)).toBe("팀장: 오늘은 지켜봅니다");
    expect(caption(said, NAMES, T0 + 2_000 + 12_000)).toBeNull();
  });

  it("말이 없으면 숨긴다", () => {
    // 검증: V14
    expect(caption([], NAMES, T0)).toBeNull();
  });

  it("이름을 모르면 id를 쓰고, 긴 글은 240자로 다듬는다", () => {
    // 검증: V14
    const line = caption([{ agent: "scout", text: "가".repeat(300), at: T0 }], NAMES, T0);
    expect(line).toBe(`scout: ${"가".repeat(237)}...`);
  });
});

describe("회의 발언 중 표시", () => {
  const talks = [
    { agent: "risk", at: T0 },
    { agent: "leader", at: T0 + 1_000 },
  ];

  it("마지막 논의 글의 담당이 `○○ 발언 중…`", () => {
    // 검증: V14
    expect(speakingLine(talks, NAMES, T0 + 1_500)).toBe("팀장 발언 중…");
    expect(speakingLine([...talks].reverse(), NAMES, T0 + 1_500)).toBe("팀장 발언 중…");
  });

  it("마지막 논의 글 뒤 4초 동안만 보인다", () => {
    // 검증: V14
    expect(SPEAKING_LIFE_MS).toBe(4_000);
    expect(speakingLine(talks, NAMES, T0 + 1_000 + 3_900)).toBe("팀장 발언 중…");
    expect(speakingLine(talks, NAMES, T0 + 1_000 + 4_000)).toBeNull();
    expect(speakingLine([], NAMES, T0)).toBeNull();
  });
});
