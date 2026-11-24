// 키보드 거르기(server 32차 4번 · 7번, 결정 17 · E41 · V56): 방향키만 움직이고(WASD 무시), 글 칸(INPUT · TEXTAREA ·
// SELECT · 글 고치기 칸)에 초점이 있거나 한글 조합 중이면 방향키 · `/`는 글 칸 몫이다. 방향키 · `/`의 기본 동작(스크롤)은
// 사무실 층에 초점이 있을 때만 막는다. 원본 DeskRPG `isTypingTarget`(docs/research/metaverse-office-refs.md B절).
import { describe, expect, it } from "vitest";
import { arrowOf, isTalkKey, isTypingTarget, preventsDefault, type KeyLike } from "../../web/src/office/player-keys";

function typingFields(): HTMLElement[] {
  const input = document.createElement("input");
  const area = document.createElement("textarea");
  const select = document.createElement("select");
  const editor = document.createElement("div");
  editor.setAttribute("contenteditable", "true");
  for (const el of [input, area, select, editor]) document.body.appendChild(el);
  return [input, area, select, editor];
}

const key = (k: string, more: Partial<KeyLike> = {}): KeyLike => ({ key: k, target: document.body, ...more });

describe("글 칸 가르기", () => {
  it("INPUT · TEXTAREA · SELECT · 글 고치기 칸은 글 칸이고, 바탕 · 단추는 아니다", () => {
    // 검증: E41
    for (const el of typingFields()) expect(isTypingTarget(el), el.tagName).toBe(true);
    const button = document.createElement("button");
    const plain = document.createElement("div");
    const off = document.createElement("div");
    off.setAttribute("contenteditable", "false");
    for (const el of [button, plain, off, document.body]) expect(isTypingTarget(el), el.tagName).toBe(false);
    expect(isTypingTarget(null)).toBe(false);
  });
});

describe("방향키", () => {
  it("방향키 넷은 움직이는 키다", () => {
    // 검증: V56
    for (const k of ["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight"]) expect(arrowOf(key(k))).toBe(k);
    expect(arrowOf(key("ArrowUp", { target: document.createElement("button") }))).toBe("ArrowUp");
  });

  it("WASD는 움직이지 않는다", () => {
    // 검증: V56
    for (const k of ["w", "a", "s", "d", "W", "A", "S", "D"]) {
      expect(arrowOf(key(k, { code: `Key${k.toUpperCase()}` })), k).toBeNull();
    }
    expect(arrowOf(key(" ", { code: "Space" }))).toBeNull();
    expect(arrowOf(key("Enter"))).toBeNull();
  });

  it("글 칸에 초점이 있으면 방향키는 캐릭터를 움직이지 않는다", () => {
    // 검증: V56 E41
    for (const el of typingFields()) expect(arrowOf(key("ArrowLeft", { target: el })), el.tagName).toBeNull();
  });

  it("한글 조합 중이면 방향키를 무시한다", () => {
    // 검증: V56 E41
    expect(arrowOf(key("ArrowDown", { isComposing: true }))).toBeNull();
    expect(arrowOf(key("ArrowDown", { keyCode: 229 }))).toBeNull();
  });
});

describe("/ 말 걸기 키", () => {
  it("바탕에서 /는 말 걸기, 글 칸 · 조합 중이면 글자다", () => {
    // 검증: E41
    expect(isTalkKey(key("/", { code: "Slash" }))).toBe(true);
    for (const el of typingFields()) expect(isTalkKey(key("/", { code: "Slash", target: el })), el.tagName).toBe(false);
    expect(isTalkKey(key("/", { code: "Slash", isComposing: true }))).toBe(false);
    expect(isTalkKey(key("?", { code: "Slash" }))).toBe(false);
    expect(isTalkKey(key("a"))).toBe(false);
  });
});

describe("스크롤 막기", () => {
  it("사무실 층에 초점이 있을 때만 방향키 · /의 기본 동작을 막는다", () => {
    // 검증: V56 E41
    expect(preventsDefault(key("ArrowUp"), true)).toBe(true);
    expect(preventsDefault(key("/", { code: "Slash" }), true)).toBe(true);
    expect(preventsDefault(key("ArrowUp"), false)).toBe(false);
    expect(preventsDefault(key("w"), true)).toBe(false);
    expect(preventsDefault(key("Enter"), true)).toBe(false);
    for (const el of typingFields()) expect(preventsDefault(key("ArrowUp", { target: el }), true), el.tagName).toBe(false);
    expect(preventsDefault(key("ArrowUp", { isComposing: true }), true)).toBe(false);
  });
});
