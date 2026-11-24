// 머리 위 말풍선(server 22차 3번 · 5번, 원본 DeskRPG 동작 — docs/research/deskrpg.md 10절): 글 다듬기 · 폭 ·
// 수명 12초 · 스트리밍 교체 · "···" 4초 · 상태 표시 · 우선순위 · 가벼운 화면의 최근 말. 장면 없이 `src/office/speech.ts`를
// 돌려 본다. 시각은 시험이 넘기는 밀리초(가짜 시계)다.
import { describe, expect, it } from "vitest";
import {
  LATEST_LIFE_MS,
  SPEECH_LIFE_MS,
  STATE_ICONS,
  SpeechBoard,
  TYPING_LIFE_MS,
  TYPING_TEXT,
  bubbleWhere,
  bubbleWidth,
  latestLine,
  trimSpeech,
} from "../../web/src/office/speech";

const T0 = 1_000_000;

describe("말풍선 글 다듬기", () => {
  it("빈칸 · 줄바꿈을 하나로 접고 앞뒤를 깎는다", () => {
    // 검증: V14
    expect(trimSpeech("  삼성전자   공시가\n\n 나왔습니다\t ")).toBe("삼성전자 공시가 나왔습니다");
  });

  it("240자까지는 그대로, 넘으면 237자 + ...", () => {
    // 검증: V14
    const exact = "가".repeat(240);
    expect(trimSpeech(exact)).toBe(exact);
    const over = "나".repeat(241);
    expect(trimSpeech(over)).toBe(`${"나".repeat(237)}...`);
    expect(trimSpeech("다".repeat(1000)).length).toBe(240);
  });

  it("접은 뒤의 길이로 자른다", () => {
    // 검증: V14
    const spaced = Array.from({ length: 200 }, () => "가  ").join("");
    // 접으면 "가 " 200번 - 끝 빈칸 = 399자 → 자른다
    const trimmed = trimSpeech(spaced);
    expect(trimmed.endsWith("...")).toBe(true);
    expect(trimmed.length).toBe(240);
    expect(trimmed.includes("  ")).toBe(false);
  });

  it("폭은 max(80, min(190, 화면폭 - 16))", () => {
    // 검증: V15
    expect(bubbleWidth(1280)).toBe(190);
    expect(bubbleWidth(206)).toBe(190);
    expect(bubbleWidth(150)).toBe(134);
    expect(bubbleWidth(96)).toBe(80);
    expect(bubbleWidth(40)).toBe(80);
  });
});

describe("말풍선 판", () => {
  it("글 말풍선은 마지막으로 바뀐 뒤 12초 동안 보인다", () => {
    // 검증: V14
    expect(SPEECH_LIFE_MS).toBe(12_000);
    const board = new SpeechBoard();
    board.say("risk", "손절선을 지킵니다", T0);
    expect(board.shown("risk", T0)?.text).toBe("손절선을 지킵니다");
    expect(board.shown("risk", T0 + 11_900)?.text).toBe("손절선을 지킵니다");
    expect(board.shown("risk", T0 + 12_100)).toBeUndefined();
  });

  it("스트리밍 중에는 지금까지의 글 전체로 통째 바꾸고 수명은 마지막으로 바뀐 때부터다", () => {
    // 검증: V14 C5
    const board = new SpeechBoard();
    board.say("strategy", "지금", T0);
    board.say("strategy", "지금 사도", T0 + 5_000);
    board.say("strategy", "지금 사도 됩니다", T0 + 10_000);
    expect(board.shown("strategy", T0 + 10_000)?.text).toBe("지금 사도 됩니다");
    expect(board.shown("strategy", T0 + 21_000)?.text).toBe("지금 사도 됩니다");
    expect(board.shown("strategy", T0 + 22_100)).toBeUndefined();
  });

  it("말풍선 글도 다듬는다", () => {
    // 검증: V14
    const board = new SpeechBoard();
    board.say("news", `  ${"가".repeat(300)}  `, T0);
    expect(board.shown("news", T0)?.text).toBe(`${"가".repeat(237)}...`);
  });

  it("글 없이 말하는 중이면 ··· 를 4초", () => {
    // 검증: V14
    expect(TYPING_TEXT).toBe("···");
    expect(TYPING_LIFE_MS).toBe(4_000);
    const board = new SpeechBoard();
    board.typing("leader", T0);
    expect(board.shown("leader", T0 + 3_900)?.text).toBe("···");
    expect(board.shown("leader", T0 + 4_100)).toBeUndefined();
  });

  it("말하는 중 뒤에 글이 오면 글로 바뀌고 12초를 산다", () => {
    // 검증: V14
    const board = new SpeechBoard();
    board.typing("leader", T0);
    board.say("leader", "오늘은 지켜봅니다", T0 + 1_000);
    expect(board.shown("leader", T0 + 12_000)?.text).toBe("오늘은 지켜봅니다");
  });

  it("상태 표시는 생각 중 💭 · 쓰는 중 💬 · 기다림 ⏳ · 실패 ❗ 이고 글 앞에 붙는다", () => {
    // 검증: V14 E6
    expect(STATE_ICONS).toEqual({ thinking: "💭", writing: "💬", waiting: "⏳", failed: "❗" });
    const board = new SpeechBoard();
    board.say("risk", "손절선은", T0, "writing");
    const text = board.shown("risk", T0)?.text ?? "";
    expect(text.startsWith("💬")).toBe(true);
    expect(text.endsWith("손절선은")).toBe(true);
    board.status("trader", "failed", T0);
    expect(board.shown("trader", T0 + 60_000)?.text).toContain("❗");
    // 23차 3번: 글 없는 상태 말풍선은 머리 위가 아니라 말풍선 줄에 뜬다.
    expect(bubbleWhere(board.shown("trader", T0 + 60_000))).toBe("rail");
    expect(bubbleWhere(board.shown("risk", T0))).toBe("head");
  });

  it("한 담당에 여러 말풍선이면 대화 > 상태 > 잡담 순서로 하나만 보인다", () => {
    // 검증: V14
    const board = new SpeechBoard();
    board.smalltalk("review", "점심 뭐 드셨어요?", T0, T0 + 4_500);
    expect(board.shown("review", T0)?.kind).toBe("smalltalk");
    board.status("review", "thinking", T0);
    expect(board.shown("review", T0)?.kind).toBe("status");
    board.say("review", "복기 글입니다", T0);
    expect(board.shown("review", T0)?.kind).toBe("talk");
    expect(board.shown("review", T0 + 12_100)?.kind).toBe("status");
    board.clear("review", "status");
    expect(board.shown("review", T0 + 1_000)?.kind).toBe("talk");
    board.clear("review");
    expect(board.shown("review", T0 + 1_000)).toBeUndefined();
  });

  it("대화 · 상태 말풍선이 있으면 바쁘다(잡담만 있으면 바쁘지 않다)", () => {
    // 검증: V14
    const board = new SpeechBoard();
    expect(board.busy("news", T0)).toBe(false);
    board.smalltalk("news", "날씨 좋네요", T0, T0 + 4_500);
    expect(board.busy("news", T0)).toBe(false);
    board.say("news", "공시가 나왔습니다", T0);
    expect(board.busy("news", T0)).toBe(true);
    expect(board.busy("news", T0 + 12_100)).toBe(false);
    board.status("news", "waiting", T0);
    expect(board.busy("news", T0 + 12_100)).toBe(true);
  });

  it("담당마다 따로다", () => {
    // 검증: V14
    const board = new SpeechBoard();
    board.say("risk", "리스크", T0);
    board.say("leader", "팀장", T0);
    expect(board.shown("risk", T0)?.text).toBe("리스크");
    expect(board.shown("leader", T0)?.text).toBe("팀장");
    expect(board.shown("news", T0)).toBeUndefined();
  });
});

describe("가벼운 화면의 최근 말", () => {
  const said = [
    { agent: "risk", text: "첫 말", at: T0 },
    { agent: "leader", text: "팀장 말", at: T0 + 1_000 },
    { agent: "risk", text: "  둘째   말 ", at: T0 + 2_000 },
  ];

  it("그 담당의 가장 최근 말을 다듬어 한 줄로 보인다", () => {
    // 검증: V14 V15
    expect(latestLine(said, "risk", T0 + 3_000)).toBe("둘째 말");
    expect(latestLine(said, "leader", T0 + 3_000)).toBe("팀장 말");
    expect(latestLine(said, "news", T0 + 3_000)).toBeNull();
  });

  it("12초가 지나면 보이지 않는다", () => {
    // 검증: V14
    expect(LATEST_LIFE_MS).toBe(12_000);
    expect(latestLine(said, "risk", T0 + 2_000 + 11_900)).toBe("둘째 말");
    expect(latestLine(said, "risk", T0 + 2_000 + 12_100)).toBeNull();
  });
});
