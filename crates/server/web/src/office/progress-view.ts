// 논의 진행 단계 그리기(server 51차, C21 · E51 · V63). 옆 패널(채팅) 맨 위에 칸 하나를 둔다.
// - 논의가 돌 때: 주제 · 담당 여섯의 표시(대기 ○ · 진행 ● · 완료 ✓ · 오류 ! · 다시 시도 n/3) · 아래 한 줄
//   ("담당 n/6 · 호출 n · 경과 m:ss", 1초마다 경과가 흐른다).
// - 끝난 논의(`closed`)는 마지막 상태를 잠시 보인 뒤 숨긴다. 내용이 null(스냅샷에 도는 논의 없음)이면 바로 숨긴다.

import { progressRows, progressSummary, readProgress, type Progress } from "./progress";

/** 끝난 논의의 마지막 상태를 보여 두는 시간. */
export const CLOSED_LINGER_MS = 5_000;

/** 옆 패널의 진행 단계. */
export interface ProgressView {
  /** 받은 내용(이벤트 · 스냅샷)으로 그린다. `now`는 받은 시각(화면 시계). */
  apply(payload: unknown, now: number): void;
  /** 경과를 흘리고, 끝난 논의는 때가 되면 숨긴다. */
  tick(now: number): void;
}

function el<K extends keyof HTMLElementTagNameMap>(tag: K, className?: string, text?: string): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  if (className) { node.className = className; }
  if (text !== undefined) { node.textContent = text; }
  return node;
}

/** 옆 패널 맨 위에 진행 단계 칸을 단다. */
export function mountProgress(host: HTMLElement, nameOf: (id: string) => string): ProgressView {
  const panel = el("section", "progress");
  panel.setAttribute("aria-label", "논의 진행 단계");
  panel.setAttribute("role", "status");
  panel.hidden = true;
  const title = el("div", "progress-topic");
  const list = el("ul", "progress-list");
  const summary = el("div", "progress-summary");
  panel.append(title, list, summary);
  host.prepend(panel);

  let shown: Progress | null = null;
  let receivedAt = 0;

  const hide = (): void => {
    shown = null;
    panel.hidden = true;
  };

  const apply = (payload: unknown, now: number): void => {
    if (payload === null || payload === undefined) { hide(); return; }
    const progress = readProgress(payload);
    if (!progress) { return; }
    shown = progress;
    receivedAt = now;
    title.textContent = progress.topic;
    list.replaceChildren(...progressRows(progress, nameOf).map((row) => {
      const item = el("li");
      item.dataset.agent = row.agent;
      item.dataset.state = row.state;
      item.append(el("span", "progress-mark", row.mark), el("span", "progress-name", row.name),
        el("span", "progress-label", row.label));
      return item;
    }));
    panel.dataset.closed = String(progress.closed);
    summary.textContent = progressSummary(progress, 0);
    panel.hidden = false;
  };

  const tick = (now: number): void => {
    if (!shown) { return; }
    if (shown.closed && now - receivedAt >= CLOSED_LINGER_MS) { hide(); return; }
    summary.textContent = progressSummary(shown, now - receivedAt);
  };

  return { apply, tick };
}
