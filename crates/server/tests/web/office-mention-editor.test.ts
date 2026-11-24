// 팀 방 글 칸의 멘션 칩 에디터(server 33차 5번, C18 · N11 · V58): `@` 목록 · ↑↓ · Enter/Tab · Esc · 누르기로 고르기 ·
// 칩 · 칩 통째 지우기 · 한글 조합 중 · 보낼 글 모양과 가림. 원본 DeskRPG `MentionEditor`(metaverse-office-refs.md D절).
import { beforeEach, describe, expect, it } from "vitest";
import {
  CHIP_CLASS,
  LIST_CLASS,
  OPTION_CLASS,
  mountMentionEditor,
  type MentionEditor,
} from "../../web/src/office/mention-editor";
import { MENTION_EMPTY, type Member, type Piece } from "../../web/src/office/mention-text";

const ROSTER: Member[] = [
  { agent: "news", name: "뉴스 공시 시세" },
  { agent: "strategy", name: "전략" },
  { agent: "risk", name: "리스크" },
  { agent: "review", name: "복기" },
  { agent: "trader", name: "매매" },
  { agent: "leader", name: "팀장" },
];

let host: HTMLElement;
let sent: { text: string; pieces: Piece[] }[];
let editor: MentionEditor;

function mount(members: readonly Member[] = ROSTER): MentionEditor {
  return mountMentionEditor(host, {
    members: () => members,
    onSubmit: (text, pieces) => sent.push({ text, pieces }),
  });
}

beforeEach(() => {
  document.body.replaceChildren();
  host = document.createElement("div");
  document.body.appendChild(host);
  sent = [];
  editor = mount();
});

/** 커서를 그 글 조각의 `offset`에 둔다. */
function caretAt(node: Node, offset: number): void {
  const selection = document.getSelection()!;
  const range = document.createRange();
  range.setStart(node, offset);
  range.collapse(true);
  selection.removeAllRanges();
  selection.addRange(range);
}

/** 글 칸 끝에 `text`를 친 것처럼: 마지막 글 조각에 더하고(없으면 만든다) 커서를 끝에 두고 `input`을 낸다. */
function type(text: string): void {
  const last = editor.input.lastChild;
  let node: Text;
  if (last instanceof Text) {
    last.data += text;
    node = last;
  } else {
    node = document.createTextNode(text);
    editor.input.append(node);
  }
  caretAt(node, node.data.length);
  editor.input.dispatchEvent(new InputEvent("input", { bubbles: true, data: text }));
}

function key(name: string, init: KeyboardEventInit = {}): KeyboardEvent {
  const event = new KeyboardEvent("keydown", { key: name, bubbles: true, cancelable: true, ...init });
  editor.input.dispatchEvent(event);
  return event;
}

function chips(): HTMLElement[] {
  return Array.from(editor.input.querySelectorAll<HTMLElement>(`.${CHIP_CLASS}`));
}

function options(): HTMLElement[] {
  return Array.from(editor.list.querySelectorAll<HTMLElement>(`.${OPTION_CLASS}`));
}

describe("@ 목록", () => {
  it("글 칸은 contenteditable이고 목록은 처음에 닫혀 있다", () => {
    // 검증: V58
    expect(editor.input.getAttribute("contenteditable")).toBe("true");
    expect(host.contains(editor.input)).toBe(true);
    expect(editor.list.classList.contains(LIST_CLASS)).toBe(true);
    expect(editor.isOpen()).toBe(false);
    expect(editor.list.hidden).toBe(true);
  });

  it("@를 치면 담당 목록이 뜨고 이어 친 글로 거른다", () => {
    // 검증: V58 N11
    type("@");
    expect(editor.isOpen()).toBe(true);
    expect(editor.list.hidden).toBe(false);
    expect(editor.candidates().map((m) => m.agent)).toEqual(ROSTER.map((m) => m.agent));
    expect(options().map((o) => o.textContent)).toEqual(ROSTER.map((m) => `@${m.name}`));
    expect(editor.activeIndex()).toBe(0);
    type("리");
    expect(editor.candidates().map((m) => m.agent)).toEqual(["risk"]);
  });

  it("빈칸 없이 글 뒤에 붙은 @는 목록을 열지 않는다", () => {
    // 검증: V58 N11
    type("mail@");
    expect(editor.isOpen()).toBe(false);
  });

  it("맞는 담당이 없으면 안내 줄을 보이고 Enter로는 아무것도 고르지 않는다", () => {
    // 검증: V58
    type("@없는");
    expect(editor.isOpen()).toBe(true);
    expect(editor.candidates()).toEqual([]);
    expect(editor.list.textContent).toContain(MENTION_EMPTY);
    key("Enter");
    expect(chips()).toHaveLength(0);
    expect(sent).toHaveLength(0);
  });

  it("↓ ↑는 돌며 움직이고 고른 줄이 표시된다", () => {
    // 검증: V58
    type("@");
    key("ArrowDown");
    expect(editor.activeIndex()).toBe(1);
    expect(options()[1].classList.contains("active")).toBe(true);
    expect(options()[1].getAttribute("aria-selected")).toBe("true");
    key("ArrowUp");
    key("ArrowUp");
    expect(editor.activeIndex()).toBe(ROSTER.length - 1);
  });

  it("Esc는 목록을 닫고 글은 그대로 둔다", () => {
    // 검증: V58
    type("안녕 @리");
    key("Escape");
    expect(editor.isOpen()).toBe(false);
    expect(chips()).toHaveLength(0);
    expect(editor.text()).toBe("안녕 @리");
  });

  it("찾는 말에 빈칸이 들어오면 닫힌다", () => {
    // 검증: V58 N11
    type("@리");
    expect(editor.isOpen()).toBe(true);
    type(" ");
    expect(editor.isOpen()).toBe(false);
  });
});

describe("고르기와 칩", () => {
  it("Enter로 고르면 @찾는말이 칩과 빈칸으로 바뀌고 목록이 닫힌다", () => {
    // 검증: V58 C18
    type("안녕 @리");
    const event = key("Enter");
    expect(event.defaultPrevented).toBe(true);
    expect(sent).toHaveLength(0);
    expect(editor.isOpen()).toBe(false);
    const [chip] = chips();
    expect(chip.textContent).toBe("@리스크");
    expect(chip.getAttribute("contenteditable")).toBe("false");
    expect(chip.dataset.agent).toBe("risk");
    expect(editor.pieces()).toEqual([
      { kind: "text", text: "안녕 " },
      { kind: "mention", agent: "risk", name: "리스크" },
      { kind: "text", text: " " },
    ]);
    expect(editor.text()).toBe("안녕 @[리스크] ");
  });

  it("Tab으로도 고르고, 고른 줄은 방향키로 옮긴 줄이다", () => {
    // 검증: V58
    type("@");
    key("ArrowDown");
    key("Tab");
    expect(chips().map((c) => c.dataset.agent)).toEqual(["strategy"]);
  });

  it("목록 줄을 누르면 그 담당을 고른다", () => {
    // 검증: V58
    type("@");
    const pressed = new MouseEvent("mousedown", { bubbles: true, cancelable: true });
    options()[5].dispatchEvent(pressed);
    expect(pressed.defaultPrevented).toBe(true);
    expect(chips().map((c) => c.dataset.agent)).toEqual(["leader"]);
    expect(editor.isOpen()).toBe(false);
  });

  it("칩 바로 뒤의 Backspace는 칩을 통째로 지운다", () => {
    // 검증: V58
    type("@리");
    key("Enter");
    const after = chips()[0].nextSibling as Text;
    caretAt(after, 0);
    const event = key("Backspace");
    expect(event.defaultPrevented).toBe(true);
    expect(chips()).toHaveLength(0);
    expect(editor.pieces().some((p) => p.kind === "mention")).toBe(false);
  });

  it("여러 칩과 가림이 보낼 글에 그대로 실린다", () => {
    // 검증: V58 C18
    sent = [];
    editor = mount([...ROSTER, { agent: "odd", name: "a[b]" }]);
    type("@리");
    key("Enter");
    type("@a[");
    key("Enter");
    type("어때요?");
    key("Enter");
    expect(sent).toHaveLength(1);
    expect(sent[0].text).toBe("@[리스크] @[a\\[b\\]] 어때요?");
    expect(sent[0].pieces.filter((p) => p.kind === "mention")).toHaveLength(2);
  });
});

describe("보내기와 한글 조합", () => {
  it("목록이 닫혀 있으면 Enter가 보내고, 빈 글은 보내지 않는다", () => {
    // 검증: V58
    key("Enter");
    expect(sent).toHaveLength(0);
    type("공지 아닌 글");
    const event = key("Enter");
    expect(event.defaultPrevented).toBe(true);
    expect(sent.map((s) => s.text)).toEqual(["공지 아닌 글"]);
  });

  it("조합 중의 Enter · 방향키는 처리하지 않는다", () => {
    // 검증: V58 N11
    type("@");
    key("ArrowDown", { isComposing: true });
    expect(editor.activeIndex()).toBe(0);
    key("Enter", { isComposing: true });
    expect(chips()).toHaveLength(0);
    expect(editor.isOpen()).toBe(true);
    key("Enter", { keyCode: 229 } as KeyboardEventInit);
    expect(chips()).toHaveLength(0);
    // 조합이 끝나면 다시 찾고, 그 뒤의 Enter는 고른다.
    editor.input.dispatchEvent(new CompositionEvent("compositionstart", { bubbles: true }));
    type("리");
    editor.input.dispatchEvent(new CompositionEvent("compositionend", { bubbles: true, data: "리" }));
    expect(editor.candidates().map((m) => m.agent)).toEqual(["risk"]);
    key("Enter");
    expect(chips().map((c) => c.dataset.agent)).toEqual(["risk"]);
  });

  it("비우면 글 칸이 비고 목록이 닫힌다", () => {
    // 검증: V58
    type("@리");
    editor.clear();
    expect(editor.text()).toBe("");
    expect(editor.pieces()).toEqual([]);
    expect(editor.isOpen()).toBe(false);
  });
});
