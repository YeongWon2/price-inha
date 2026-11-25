// 하루 AI 잔량 바 그리기(server 51차, C22 · E52 · V64). 위 줄(#bar)에 단추 하나를 단다.
// - 단추 안: 글("AI 6/10" 또는 "AI 상한 없음 · 오늘 3번") · 모드 글("정리 모드" · "팀장 몫만") · 막대(채움 + 예비분 구간).
//   모양은 data-mode · data-level · data-unlimited로 CSS가 고른다(색은 토큰).
// - 누르면 담당별 사용 표(담당 · 쓴 수 · 몫 · 사용률 · 작은 막대)를 열고, 다시 누르면 닫는다.
// - 처음에 `GET /api/team/budget`을 읽고, 그 뒤로는 이벤트 `team.ai_used`가 오면 다시 그린다. 계산은 서버가 한다.

import { API } from "../paths";
import { budgetBar, budgetRows, readBudget, type Budget } from "./budget";

/** 서버 부르기(시험이 바꿔 끼운다). */
export type Fetcher = (url: string, init?: RequestInit) => Promise<Response>;

/** 위 줄 잔량 바. */
export interface BudgetView {
  /** 잔량을 읽어 그린다(실패하면 지금 표시를 그대로 둔다). */
  load(): Promise<void>;
  /** 받은 내용(이벤트 · 읽기)으로 다시 그린다. 모양이 틀리면 무시한다. */
  apply(payload: unknown): void;
}

function el<K extends keyof HTMLElementTagNameMap>(tag: K, className?: string, text?: string): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  if (className) { node.className = className; }
  if (text !== undefined) { node.textContent = text; }
  return node;
}

function percent(value: number): string {
  return `${Math.round(value * 1000) / 10}%`;
}

function tableHead(): HTMLTableSectionElement {
  const head = el("thead");
  const row = el("tr");
  for (const title of ["담당", "쓴 수", "몫", "사용률"]) { row.appendChild(el("th", undefined, title)); }
  head.appendChild(row);
  return head;
}

function renderRows(body: HTMLTableSectionElement, budget: Budget, nameOf: (id: string) => string): void {
  body.replaceChildren(...budgetRows(budget, nameOf).map((row) => {
    const line = el("tr");
    line.dataset.agent = row.agent;
    if (row.level) { line.dataset.level = row.level; }
    const name = el("th", undefined, row.name);
    name.scope = "row";
    const meter = el("span", "budget-mini");
    const fill = el("span", "budget-mini-fill");
    fill.style.width = row.fill === null ? "0%" : percent(row.fill);
    meter.appendChild(fill);
    meter.hidden = row.fill === null;
    const ratio = el("td", undefined, row.percent);
    ratio.prepend(meter);
    line.append(name, el("td", undefined, row.used), el("td", undefined, row.share), ratio);
    return line;
  }));
}

/** 위 줄에 잔량 바를 단다. */
export function mountBudget(bar: HTMLElement, options: { fetcher: Fetcher; nameOf: (id: string) => string }): BudgetView {
  const wrap = el("div", "budget-wrap");
  const button = el("button", "budget");
  button.type = "button";
  button.setAttribute("aria-expanded", "false");
  button.title = "오늘 AI 호출 — 누르면 담당별 사용";
  const text = el("span", "budget-text", "AI …");
  const mode = el("span", "budget-mode");
  const track = el("span", "budget-track");
  const fill = el("span", "budget-fill");
  const reserve = el("span", "budget-reserve");
  track.append(fill, reserve);
  button.append(text, track, mode);
  const table = el("table", "budget-table");
  table.hidden = true;
  table.appendChild(el("caption", undefined, "담당별 오늘 AI 사용"));
  table.appendChild(tableHead());
  const body = el("tbody");
  table.appendChild(body);
  wrap.append(button, table);
  bar.appendChild(wrap);
  button.addEventListener("click", () => {
    table.hidden = !table.hidden;
    button.setAttribute("aria-expanded", String(!table.hidden));
  });

  const apply = (payload: unknown): void => {
    const budget = readBudget(payload);
    if (!budget) { return; }
    const shown = budgetBar(budget);
    text.textContent = shown.text;
    mode.textContent = shown.modeText;
    mode.hidden = shown.modeText === "";
    button.dataset.mode = shown.mode;
    button.dataset.unlimited = String(shown.unlimited);
    if (shown.level) { button.dataset.level = shown.level; } else { delete button.dataset.level; }
    track.hidden = shown.unlimited;
    fill.style.width = shown.fill === null ? "0%" : percent(shown.fill);
    reserve.style.left = shown.reserveFrom === null ? "100%" : percent(shown.reserveFrom);
    renderRows(body, budget, options.nameOf);
  };

  const load = async (): Promise<void> => {
    try {
      const reply = await options.fetcher(API.teamBudget, { credentials: "same-origin", cache: "no-store" });
      if (!reply.ok) { return; }
      apply(await reply.json());
    } catch {
      // 못 읽으면 지금 표시를 그대로 둔다(이벤트가 오면 다시 그린다).
    }
  };

  return { load, apply };
}
