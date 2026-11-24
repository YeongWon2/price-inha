// 키보드 거르기(server 32차 4번 · 7번, E41): 방향키 · `/`가 캐릭터 몫인지 글 칸 몫인지 가른다.
// 원본 DeskRPG `isTypingTarget`과 같게 INPUT · TEXTAREA · SELECT · 글 고치기 칸(contentEditable)에 초점이 있으면 무시하고,
// 한글 조합 중(`isComposing`, 또는 keyCode 229)이면 무시한다. 방향키만 움직인다(WASD 없음 — 결정 17).

import { ARROW_KEYS, type ArrowKey } from "./player";

/** 키 이벤트에서 쓰는 부분(시험이 가짜로 만들 수 있게). */
export interface KeyLike {
  key: string;
  code?: string;
  isComposing?: boolean;
  keyCode?: number;
  target?: EventTarget | null;
}

const TYPING_TAGS = new Set(["INPUT", "TEXTAREA", "SELECT"]);

/**
 * 글을 쓰는 칸(INPUT · TEXTAREA · SELECT · 글 고치기 칸)인가. 글 고치기 칸은 `contenteditable` 속성이 있고
 * `"false"`가 아닌 요소다(`isContentEditable`이 없는 환경도 있어 속성으로도 본다). 요소가 아니면 `false`.
 */
export function isTypingTarget(target: EventTarget | null | undefined): boolean {
  if (!target || typeof (target as Element).tagName !== "string") return false;
  const el = target as HTMLElement;
  if (TYPING_TAGS.has(el.tagName.toUpperCase())) return true;
  if (el.isContentEditable) return true;
  const attr = typeof el.getAttribute === "function" ? el.getAttribute("contenteditable") : null;
  return attr !== null && attr.toLowerCase() !== "false";
}

/** 글 칸 몫인가(글 칸 초점 · 한글 조합 중). */
function forText(event: KeyLike): boolean {
  return event.isComposing === true || event.keyCode === 229 || isTypingTarget(event.target);
}

/** 캐릭터를 움직일 방향키. 방향키가 아니거나(WASD 포함) 글 칸 초점 · 한글 조합 중이면 `null`. */
export function arrowOf(event: KeyLike): ArrowKey | null {
  if (!(ARROW_KEYS as readonly string[]).includes(event.key)) return null;
  if (forText(event)) return null;
  return event.key as ArrowKey;
}

/** `/`가 "말 걸기"인가(글 칸 초점 · 조합 중이면 글자다). */
export function isTalkKey(event: KeyLike): boolean {
  return event.key === "/" && !forText(event);
}

/**
 * 기본 동작(화면 스크롤)을 막을 키인가: 사무실 층에 초점이 있을 때(`floorFocused`)의 방향키 · `/`만.
 * 글 칸 초점 · 조합 중이면 막지 않는다.
 */
export function preventsDefault(event: KeyLike, floorFocused: boolean): boolean {
  if (!floorFocused) return false;
  return arrowOf(event) !== null || isTalkKey(event);
}
