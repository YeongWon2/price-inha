// 카메라 도구 줄 · 조작 안내 · "누구와 대화?" 목록(server 32차 6번 · 7번)의 DOM. 글은 `textContent`로만 넣는다.
// 도구 줄 8개: 전체 보기 · 왼쪽 돌리기 · 오른쪽 돌리기 · 입체 · 위에서 · 따라가기 · + · −(이 순서).
// 오른쪽 아래 조작 안내: "방향키 이동 · / 대화 · 끌기 화면 이동".

import { WHO_TO_TALK } from "./nearby";

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

/** (다듬기 E 10번) 따라가기 중 자동 카메라 도움말. */
export const AUTO_CAMERA_REST_HINT = "내 캐릭터를 따라가는 중에는 자동 카메라가 쉽니다";

/**
 * (다듬기 E 10번) "따라가기"가 켜져 있으면 같은 문서의 "자동 카메라" 단추(`#auto-camera`)를 흐리게(`data-resting`) 하고
 * 도움말(title)을 단다. 꺼지면 걷는다. 동작 · `aria-pressed` · 글은 건드리지 않는다. 단추가 없으면 아무것도 하지 않는다.
 */
function restAutoCamera(doc: Document, following: boolean): void {
  const button = doc.getElementById("auto-camera");
  if (!button) return;
  button.dataset.resting = following ? "true" : "false";
  button.title = following ? AUTO_CAMERA_REST_HINT : "";
}

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
  const doc = root.ownerDocument;
  const bar = doc.createElement("div");
  bar.className = "toolbar";
  bar.setAttribute("role", "toolbar");
  bar.setAttribute("aria-label", "카메라");
  let follow: HTMLButtonElement | null = null;
  for (const tool of TOOLS) {
    const button = doc.createElement("button");
    button.type = "button";
    button.dataset.tool = tool.id;
    button.textContent = tool.label;
    button.addEventListener("click", () => onTool(tool.id));
    if (tool.id === "follow") follow = button;
    bar.appendChild(button);
  }
  const setFollowing = (on: boolean): void => {
    follow?.setAttribute("aria-pressed", on ? "true" : "false");
    restAutoCamera(doc, on);
  };
  setFollowing(following);
  root.appendChild(bar);
  return { element: bar, setFollowing };
}

/** `root` 안에 조작 안내(`id="controls-hint"`, 글 [`CONTROLS_HINT`])를 달고 그 요소를 돌려준다. */
export function mountControlsHint(root: HTMLElement): HTMLElement {
  const hint = root.ownerDocument.createElement("div");
  hint.id = "controls-hint";
  hint.textContent = CONTROLS_HINT;
  root.appendChild(hint);
  return hint;
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
  const doc = root.ownerDocument;
  for (const old of root.querySelectorAll(".chooser")) old.remove();
  const list = doc.createElement("div");
  list.className = "chooser";
  list.setAttribute("role", "dialog");
  list.setAttribute("aria-label", WHO_TO_TALK);
  const title = doc.createElement("div");
  title.className = "chooser-title";
  title.textContent = WHO_TO_TALK;
  list.appendChild(title);
  for (const agent of agents) {
    const button = doc.createElement("button");
    button.type = "button";
    button.dataset.agent = agent.id;
    button.textContent = agent.name;
    button.addEventListener("click", () => onPick(agent.id));
    list.appendChild(button);
  }
  root.appendChild(list);
  return list;
}

/**
 * 화면 아래 안내(말 걸기 안내 글)를 보이거나(`text`) 숨긴다(`null`). `el.hidden`과 `textContent`만 바꾼다.
 */
export function setHint(el: HTMLElement, text: string | null): void {
  if (text === null) {
    el.hidden = true;
    return;
  }
  if (el.textContent !== text) el.textContent = text;
  el.hidden = false;
}
