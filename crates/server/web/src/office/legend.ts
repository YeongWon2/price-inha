// 머리 위 표시의 정의 한 벌과 아이콘 범례(server 34차 4번, additions.md V62 · R5-10 — 원본 동작:
// docs/research/agent-office-refs.md F 2 "말풍선 아이콘 어휘 + 범례"). 머리 위 표시(speech.ts 상태 표시 ·
// approval-bubbles.ts ✋ · done-mark.ts ✓)와 범례는 모두 여기 [`OVERHEAD_MARKS`]의 글자 · 색을 쓴다(한곳에서 정의).
//
// 범례 모양(시험이 보는 약속):
// - [`renderLegend`]는 `root` 아래에 여닫는 단추 `button[data-legend-toggle]` 하나와 범례 `[data-legend]` 하나를 둔다.
//   처음에는 닫혀 있다(`[data-legend]`의 `hidden`이 참, 단추 `aria-expanded="false"`). 단추를 누르면 열리고 다시 누르면 닫힌다.
// - 범례 안에는 [`OVERHEAD_MARKS`] 차례대로 한 줄씩 `[data-mark="<id>"]`가 있고, 그 안에 표시 글자
//   `[data-part="glyph"]`(글자는 `mark.glyph`, 글자색은 `mark.color`)와 뜻 `[data-part="label"]`(`mark.label`)이 있다.
// - 글은 textContent로만 넣는다.

/** 머리 위 표시의 종류. */
export type MarkId = "thinking" | "writing" | "typing" | "waiting" | "failed" | "approval" | "done";

/** 머리 위 표시 하나: 글자 · 색(CSS 색 값) · 뜻(범례에 보일 쉬운 말). */
export interface OverheadMark {
  id: MarkId;
  glyph: string;
  color: string;
  label: string;
}

/**
 * 지금 쓰는 머리 위 표시 모두(범례 차례): 생각 중 💭 · 쓰는 중 💬 · 말하는 중 ··· · 기다림 ⏳ · 실패 ❗ ·
 * 승인 기다림 ✋ · 끝남 ✓. 글자는 지금 화면이 쓰는 것과 같다(speech.ts `STATE_ICONS` · `TYPING_TEXT`는 이것을 쓴다).
 */
export const OVERHEAD_MARKS: readonly OverheadMark[] = [];

/** 그 종류의 표시. */
export function markOf(id: MarkId): OverheadMark {
  void id;
  throw new Error("todo");
}

/** 범례를 `root` 아래에 그린다(위 모양, 처음에는 닫힘). 범례 요소(`[data-legend]`)를 돌려준다. */
export function renderLegend(root: HTMLElement): HTMLElement {
  void root;
  throw new Error("todo");
}
