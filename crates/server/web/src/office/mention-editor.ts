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

import type { Member, Piece } from "./mention-text";

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

/** `host` 안에 글 칸과 목록을 만들어 단다. */
export function mountMentionEditor(host: HTMLElement, options: MentionEditorOptions): MentionEditor {
  void host;
  void options;
  throw new Error("todo");
}
