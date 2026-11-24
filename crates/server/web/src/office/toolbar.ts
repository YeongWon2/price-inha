// 카메라 도구 줄 · 조작 안내 · "누구와 대화?" 목록(server 32차 6번 · 7번)의 DOM. 글은 `textContent`로만 넣는다.
// 도구 줄 8개: 전체 보기 · 왼쪽 돌리기 · 오른쪽 돌리기 · 입체 · 위에서 · 따라가기 · + · −(이 순서).
// 오른쪽 아래 조작 안내: "방향키 이동 · / 대화 · 끌기 화면 이동".

/** 도구 줄 단추 id. */
export type ToolId = "overview" | "rotate-left" | "rotate-right" | "perspective" | "top" | "follow" | "zoom-in" | "zoom-out";

/** 도구 줄(순서대로): id와 단추 글. */
export const TOOLS: readonly { id: ToolId; label: string }[] = [
  { id: "overview", label: "전체 보기" },
  { id: "rotate-left", label: "왼쪽 돌리기" },
  { id: "rotate-right", label: "오른쪽 돌리기" },
  { id: "perspective", label: "입체" },
  { id: "top", label: "위에서" },
  { id: "follow", label: "따라가기" },
  { id: "zoom-in", label: "+" },
  { id: "zoom-out", label: "−" },
];

/** 조작 안내 글. */
export const CONTROLS_HINT = "방향키 이동 · / 대화 · 끌기 화면 이동";

/** 달아 놓은 도구 줄. */
export interface Toolbar {
  /** 도구 줄 요소(`role="toolbar"`). */
  element: HTMLElement;
  /** "따라가기" 단추의 눌림(`aria-pressed`)을 맞춘다. */
  setFollowing(on: boolean): void;
}

/**
 * `root` 안에 도구 줄을 단다: [`TOOLS`] 순서의 `<button type="button" data-tool="{id}">{label}</button>` 8개.
 * 단추를 누르면 `onTool(id)`. "따라가기" 단추는 `aria-pressed`로 켜짐을 보인다(처음 `following`).
 */
export function mountToolbar(root: HTMLElement, onTool: (id: ToolId) => void, following = true): Toolbar {
  void root;
  void onTool;
  void following;
  throw new Error("todo");
}

/** `root` 안에 조작 안내(`id="controls-hint"`, 글 [`CONTROLS_HINT`])를 달고 그 요소를 돌려준다. */
export function mountControlsHint(root: HTMLElement): HTMLElement {
  void root;
  throw new Error("todo");
}

/**
 * "누구와 대화?" 목록(3a)을 `root` 안에 그린다(있던 목록은 바꾼다): 제목 "누구와 대화?"와 담당마다 단추 하나
 * (넘긴 순서 = 가까운 순, `data-agent="{id}"`, 글은 이름). 단추를 누르면 `onPick(id)`. 목록 요소를 돌려준다.
 */
export function showChooser(
  root: HTMLElement,
  agents: readonly { id: string; name: string }[],
  onPick: (id: string) => void,
): HTMLElement {
  void root;
  void agents;
  void onPick;
  throw new Error("todo");
}

/**
 * 화면 아래 안내(말 걸기 안내 글)를 보이거나(`text`) 숨긴다(`null`). `el.hidden`과 `textContent`만 바꾼다.
 */
export function setHint(el: HTMLElement, text: string | null): void {
  void el;
  void text;
  throw new Error("todo");
}
