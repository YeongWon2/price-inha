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
//
// 색은 3D 층의 말풍선처럼 원본 고정 색(크림 말풍선 위의 녹색 계열)이다 — 다크 모드에서도 같은 말풍선 위에 보인다.

/** 머리 위 표시의 종류. */
export type MarkId = "thinking" | "writing" | "typing" | "waiting" | "failed" | "approval" | "done"
  | "profit" | "loss";

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
export const OVERHEAD_MARKS: readonly OverheadMark[] = [
  { id: "thinking", glyph: "💭", color: "#5a6b5e", label: "생각 중" },
  { id: "writing", glyph: "💬", color: "#4e745b", label: "답을 쓰는 중" },
  { id: "typing", glyph: "···", color: "#3e513f", label: "말하는 중" },
  { id: "waiting", glyph: "⏳", color: "#8a5a00", label: "차례를 기다리는 중" },
  { id: "failed", glyph: "❗", color: "#b3412f", label: "일이 막힘(실패)" },
  { id: "approval", glyph: "✋", color: "#a4561d", label: "내 승인을 기다림 — 누르면 승인 카드가 열려요" },
  { id: "done", glyph: "✓", color: "#2f6b47", label: "일을 마침(잠깐 보였다 사라져요)" },
];

let legendCount = 0;
const BY_ID = new Map<MarkId, OverheadMark>(OVERHEAD_MARKS.map((mark) => [mark.id, mark]));

/** 그 종류의 표시. */
export function markOf(id: MarkId): OverheadMark {
  const mark = BY_ID.get(id);
  if (!mark) throw new Error(`모르는 표시: ${id}`);
  return mark;
}

/** 범례를 `root` 아래에 그린다(위 모양, 처음에는 닫힘). 범례 요소(`[data-legend]`)를 돌려준다. */
export function renderLegend(root: HTMLElement): HTMLElement {
  const toggle = document.createElement("button");
  toggle.type = "button";
  toggle.className = "legend-toggle";
  toggle.dataset.legendToggle = "";
  toggle.textContent = "표시 뜻";
  toggle.title = "머리 위 표시의 뜻";
  toggle.setAttribute("aria-expanded", "false");

  const legend = document.createElement("div");
  legend.className = "legend";
  legend.dataset.legend = "";
  legendCount += 1;
  legend.id = `overhead-legend-${legendCount}`;
  legend.setAttribute("role", "region");
  legend.setAttribute("aria-label", "머리 위 표시의 뜻");
  legend.hidden = true;
  toggle.setAttribute("aria-controls", legend.id);

  const list = document.createElement("ul");
  for (const mark of OVERHEAD_MARKS) {
    const row = document.createElement("li");
    row.dataset.mark = mark.id;
    const glyph = document.createElement("span");
    glyph.dataset.part = "glyph";
    glyph.className = "legend-glyph";
    glyph.textContent = mark.glyph;
    glyph.style.color = mark.color;
    glyph.setAttribute("aria-hidden", "true");
    const label = document.createElement("span");
    label.dataset.part = "label";
    label.textContent = mark.label;
    row.append(glyph, label);
    list.append(row);
  }
  legend.append(list);

  toggle.addEventListener("click", () => {
    legend.hidden = !legend.hidden;
    toggle.setAttribute("aria-expanded", String(!legend.hidden));
  });
  root.append(toggle, legend);
  return legend;
}
