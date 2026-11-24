// 팀 방 글 칸의 멘션 칩 에디터(server 33차 5번, C18 · N11 · V58). 원본 DeskRPG `MentionEditor`와 같은 동작
// (docs/research/metaverse-office-refs.md D절):
// - 글 칸은 `contenteditable` 요소 하나. 내용은 글 조각과 멘션 칩(`span.mention-chip`, `contenteditable="false"`,
//   보이는 글 `@이름`, `data-agent` · `data-name`)의 줄이다. 요소는 DOM 함수(createElement · textContent ·
//   append · replaceWith)로만 만든다(HTML 글자를 넣지 않는다).
// - `input`이 오면 커서(`document.getSelection()`, 글 칸 밖이면 끝)가 든 글 조각에서 `mentionQuery`로 `@`를 찾아
//   목록을 연다(거르기 `filterMembers`, 맞는 담당이 없으면 `MENTION_EMPTY` 한 줄). 찾는 말에 빈칸이 들어오면 닫는다.
// - 목록 줄(`.mention-option`, 글 `@이름`)은 명단 순서이고 처음 고른 줄은 0번이다. 열려 있으면: ↓/↑ 순환 이동,
//   Enter/Tab 고르기(후보가 없으면 아무 일도 없음, 기본 동작은 막는다), Esc 닫기. 목록 줄의 `mousedown`으로도 고른다
//   (글 칸이 포커스를 잃지 않게 기본 동작을 막는다). 고르면 `@찾는말`을 칩 + 빈칸 한 칸으로 바꾸고 커서를 빈칸 뒤로 둔다.
// - 목록이 닫혀 있으면: Enter(Shift 없이)는 보내기(`onSubmit`, 글이 비면 보내지 않는다), 칩 바로 뒤의 Backspace는
//   칩을 통째로 지운다(기본 동작을 막는다).
// - 한글 조합 중(`isComposing` 또는 keyCode 229, `compositionstart` ~ `compositionend`)에는 키를 처리하지 않는다.
//   조합이 끝나면 다시 찾는다.

import { MENTION_EMPTY, cycle, filterMembers, mentionQuery, serializePieces, type Member, type Piece } from "./mention-text";

/** 칩의 class. */
export const CHIP_CLASS = "mention-chip";
/** 목록의 class(닫히면 `hidden`). */
export const LIST_CLASS = "mention-list";
/** 목록 줄의 class(고른 줄은 `active`도 붙고 `aria-selected="true"`). */
export const OPTION_CLASS = "mention-option";

/** 에디터를 다는 값. */
export interface MentionEditorOptions {
  /** 지금 명단(목록을 열 때마다 읽는다). */
  members: () => readonly Member[];
  /** Enter로 보낼 때: 보낼 글(`@[이름]` 모양)과 조각. 보낸 뒤 비우는 것은 부르는 쪽이 `clear`로 한다. */
  onSubmit: (text: string, pieces: Piece[]) => void;
}

/** 달린 에디터. */
export interface MentionEditor {
  /** 글 칸(`contenteditable="true"`). */
  readonly input: HTMLElement;
  /** `@` 목록. */
  readonly list: HTMLElement;
  /** 목록이 열려 있는가. */
  isOpen(): boolean;
  /** 목록의 후보(열려 있지 않으면 빈 목록). */
  candidates(): Member[];
  /** 고른 줄의 번호(열려 있지 않으면 -1). */
  activeIndex(): number;
  /** 글 칸의 조각(글 · 칩, 보이는 순서). 이웃한 글 조각은 하나로 합치고 빈 글 조각은 뺀다. */
  pieces(): Piece[];
  /** 보낼 글(`serializePieces(pieces())`). */
  text(): string;
  /** 글 칸을 비우고 목록을 닫는다. */
  clear(): void;
}

/** 열린 `@` 자리: 그 글 조각과 `@`의 위치 · 커서 위치. */
interface Spot {
  node: Text;
  start: number;
  end: number;
}

/** 칩 하나를 만든다(보이는 글 `@이름`, 통째로 지워지게 편집 불가). */
function makeChip(member: Member): HTMLElement {
  const chip = document.createElement("span");
  chip.className = CHIP_CLASS;
  chip.setAttribute("contenteditable", "false");
  chip.dataset.agent = member.agent;
  chip.dataset.name = member.name;
  chip.textContent = `@${member.name}`;
  return chip;
}

function isChip(node: Node | null): node is HTMLElement {
  return node instanceof HTMLElement && node.classList.contains(CHIP_CLASS);
}

/** 글 칸의 조각(이웃한 글은 합치고 빈 글은 뺀다). 줄바꿈 요소는 `\n`이다. */
function readPieces(input: HTMLElement): Piece[] {
  const pieces: Piece[] = [];
  const pushText = (text: string): void => {
    if (text === "") return;
    const last = pieces[pieces.length - 1];
    if (last && last.kind === "text") last.text += text;
    else pieces.push({ kind: "text", text });
  };
  input.childNodes.forEach((node) => {
    if (isChip(node)) {
      pieces.push({ kind: "mention", agent: node.dataset.agent ?? "", name: node.dataset.name ?? "" });
    } else if (node instanceof HTMLBRElement) {
      pushText("\n");
    } else {
      pushText(node.textContent ?? "");
    }
  });
  return pieces;
}

/** 커서가 든 글 칸 안의 자리(글 칸 밖이면 끝). */
function caretOf(input: HTMLElement): { node: Node; offset: number } {
  const selection = document.getSelection();
  const node = selection?.anchorNode ?? null;
  if (selection && node && input.contains(node)) return { node, offset: selection.anchorOffset };
  const last = input.lastChild;
  if (last instanceof Text) return { node: last, offset: last.data.length };
  return { node: input, offset: input.childNodes.length };
}

/** 커서를 그 자리에 둔다. */
function placeCaret(node: Node, offset: number): void {
  const selection = document.getSelection();
  if (!selection) return;
  const range = document.createRange();
  range.setStart(node, offset);
  range.collapse(true);
  selection.removeAllRanges();
  selection.addRange(range);
}

/** 커서 바로 앞이 칩이면 그 칩. */
function chipBeforeCaret(input: HTMLElement): HTMLElement | null {
  const { node, offset } = caretOf(input);
  if (node instanceof Text) return offset === 0 && isChip(node.previousSibling) ? node.previousSibling : null;
  if (node === input && offset > 0) {
    const before = input.childNodes[offset - 1];
    return isChip(before) ? before : null;
  }
  return null;
}

/** 조합 중인 키인가(한글 입력기). */
function composingKey(event: KeyboardEvent, composing: boolean): boolean {
  return composing || event.isComposing || event.keyCode === 229;
}

/** `host` 안에 글 칸과 목록을 만들어 단다. */
export function mountMentionEditor(host: HTMLElement, options: MentionEditorOptions): MentionEditor {
  const input = document.createElement("div");
  input.className = "mention-input";
  input.setAttribute("contenteditable", "true");
  input.setAttribute("role", "textbox");
  const list = document.createElement("div");
  list.className = LIST_CLASS;
  list.setAttribute("role", "listbox");
  list.hidden = true;
  host.append(list, input);

  let spot: Spot | null = null;
  let found: Member[] = [];
  let active = -1;
  let composing = false;

  const close = (): void => {
    spot = null;
    found = [];
    active = -1;
    list.hidden = true;
    list.replaceChildren();
  };

  const paint = (): void => {
    list.replaceChildren();
    if (found.length === 0) {
      const empty = document.createElement("div");
      empty.className = "mention-empty";
      empty.textContent = MENTION_EMPTY;
      list.append(empty);
    }
    found.forEach((member, i) => {
      const option = document.createElement("div");
      option.className = OPTION_CLASS;
      option.setAttribute("role", "option");
      option.textContent = `@${member.name}`;
      const on = i === active;
      option.classList.toggle("active", on);
      option.setAttribute("aria-selected", on ? "true" : "false");
      option.addEventListener("mousedown", (event) => {
        event.preventDefault();
        choose(i);
      });
      list.append(option);
    });
    list.hidden = false;
  };

  const look = (): void => {
    const { node, offset } = caretOf(input);
    const query = node instanceof Text ? mentionQuery(node.data, offset) : null;
    if (!query || !(node instanceof Text)) {
      close();
      return;
    }
    spot = { node, start: query.start, end: offset };
    found = filterMembers(options.members(), query.query);
    active = found.length > 0 ? 0 : -1;
    paint();
  };

  const choose = (index: number): void => {
    const member = found[index];
    if (!spot || !member) return;
    const { node, start, end } = spot;
    const before = node.data.slice(0, start);
    const after = document.createTextNode(` ${node.data.slice(end)}`);
    const chip = makeChip(member);
    if (before === "") node.replaceWith(chip, after);
    else node.replaceWith(document.createTextNode(before), chip, after);
    placeCaret(after, 1);
    close();
  };

  const submit = (): void => {
    const pieces = readPieces(input);
    const text = serializePieces(pieces);
    if (text.trim() === "") return;
    options.onSubmit(text, pieces);
  };

  const openKey = (event: KeyboardEvent): void => {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      active = cycle(Math.max(active, 0), found.length, event.key === "ArrowDown" ? 1 : -1);
      paint();
    } else if (event.key === "Enter" || event.key === "Tab") {
      event.preventDefault();
      if (active >= 0) choose(active);
    } else if (event.key === "Escape") {
      event.preventDefault();
      close();
    }
  };

  input.addEventListener("keydown", (event) => {
    if (composingKey(event, composing)) return;
    if (spot) {
      openKey(event);
      return;
    }
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      submit();
    } else if (event.key === "Backspace") {
      const chip = chipBeforeCaret(input);
      if (!chip) return;
      event.preventDefault();
      const next = chip.nextSibling;
      chip.remove();
      if (next) placeCaret(next, 0);
    }
  });
  input.addEventListener("input", () => {
    if (!composing) look();
  });
  input.addEventListener("compositionstart", () => {
    composing = true;
  });
  input.addEventListener("compositionend", () => {
    composing = false;
    look();
  });
  input.addEventListener("blur", close);

  return {
    input,
    list,
    isOpen: () => spot !== null,
    candidates: () => (spot ? [...found] : []),
    activeIndex: () => (spot ? active : -1),
    pieces: () => readPieces(input),
    text: () => serializePieces(readPieces(input)),
    clear: () => {
      input.replaceChildren();
      close();
    },
  };
}
