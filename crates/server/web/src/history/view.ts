// 히스토리 화면의 그리기(server 29차 2번): 탭 · 매매 목록 · 근거와 상태 흐름 · 하루 정리 · 승인 기록.
// 26차 공통 부품 · 토큰만 쓴다. 서버를 부르지 않는다(screen.ts가 부른다). 글은 모두 글자(`textContent`)로 넣는다.
//
// 화면 계약(시험이 고르는 표시):
// - 탭 줄 `[role="tablist"]` 안에 탭 셋 `button[role="tab"][data-tab="trades" | "summary" | "approvals"]`
//   (글 "매매" · "하루 정리" · "승인 기록", 고른 탭만 `aria-selected="true"`, `aria-controls`는 그 판의 id).
//   판 `[role="tabpanel"][data-tab=같은 이름]`은 고른 탭의 것만 보이고 나머지는 `hidden`.
// - 매매 판: 상태 거르기 `select[data-role="trade-state"]`(첫 칸 값 ""은 전체, 이어서 `TRADE_STATES`),
//   목록 `[data-role="trades"]`의 줄 `[data-trade=번호]`(`data-state`=상태 글자) 안에 종목 `[data-role="symbol"]` ·
//   상태 글 `[data-role="state"]` · 만든 시각 글 `[data-role="time"]` · 펼치기 단추 `button[data-action="toggle"]`
//   (`aria-expanded`). 펼친 줄 안에 `[data-role="detail"]`: 근거 요약 `[data-role="basis"]`의 줄 `[data-key=칸]`
//   (`basisLines` 차례, 글에 이름과 값), 상태 흐름 `[data-role="flow"]`의 줄 `[data-state=상태 글자]`(시각 순서,
//   글에 상태 글과 시각 글), 방향 · 수량 `[data-role="side"]`.
// - 하루 정리 판: 날짜 고르기 `input[type="date"][data-role="date"]`, 정리 `[data-role="summary"]` 안에 날짜
//   `[data-role="summary-date"]` · 한 일 `[data-part="done"]` · 손익 `[data-part="pnl"]` · 복기 `[data-part="review"]`.
// - 승인 기록 판: 상태 거르기 `select[data-role="approval-status"]`(첫 칸 ""은 전체, 이어서 `APPROVAL_STATUSES`),
//   목록 `[data-role="approvals"]`의 줄 `[data-approval=줄 번호]`(`data-status`) 안에 승인 번호 `[data-role="number"]` ·
//   결과 글 `[data-role="status"]` · 가린 요약 `[data-role="summary"]` · 만든 시각 글 `[data-role="time"]`.
// - 빈 목록은 그 판에 `[data-role="empty"]`(`role="status"`). "더 보기"는 그 판의 `button[data-action="more"]`.
//
// 매매 줄은 줄 전체가 펼치기 단추다(Enter · Space로도 열린다). 탭은 좌우 방향키 · Home · End로 오간다(screen.ts).
import { el } from "../ui/call";
import { badge, type BadgeTone } from "../ui/parts";
import {
  APPROVAL_STATUSES,
  TRADE_STATES,
  approvalStatusLabel,
  tradeStateLabel,
  type ApprovalRow,
  type BasisLine,
  type DailySummary,
  type FlowRow,
  type TradeRow,
} from "./model";

/** 탭 이름. */
export type HistoryTab = "trades" | "summary" | "approvals" | "memos";

/** 탭 차례와 글. */
export const TABS: readonly (readonly [HistoryTab, string])[] = [
  ["trades", "매매"],
  ["summary", "하루 정리"],
  ["approvals", "승인 기록"],
  // 23번: 결정 메모(어제 메모의 지난 목록).
  ["memos", "결정 메모"],
];

/** 탭 단추의 id. */
export function tabId(name: HistoryTab): string {
  return `history-tab-${name}`;
}

/** 탭 줄과 판 셋. 처음 고른 탭은 `selected`. */
export function renderTabs(selected: HistoryTab): { tabs: HTMLElement; panels: Record<HistoryTab, HTMLElement> } {
  const tabs = el("div", "his-tabs");
  tabs.setAttribute("role", "tablist");
  tabs.setAttribute("aria-label", "히스토리");
  const panels = {} as Record<HistoryTab, HTMLElement>;
  for (const [name, label] of TABS) {
    const tab = el("button", "his-tab", label);
    tab.type = "button";
    tab.id = tabId(name);
    tab.dataset.tab = name;
    tab.setAttribute("role", "tab");
    tab.setAttribute("aria-controls", `history-panel-${name}`);
    const panel = el("div", "his-panel");
    panel.id = `history-panel-${name}`;
    panel.dataset.tab = name;
    panel.setAttribute("role", "tabpanel");
    panel.setAttribute("aria-labelledby", tab.id);
    panel.tabIndex = 0;
    tabs.append(tab);
    panels[name] = panel;
  }
  selectTab(tabs, panels, selected);
  return { tabs, panels };
}

/** 그 탭을 고른다: 고른 탭만 `aria-selected="true"` · 포커스 차례(tabindex 0), 그 판만 보인다. */
export function selectTab(tabs: HTMLElement, panels: Record<HistoryTab, HTMLElement>, selected: HistoryTab): void {
  for (const tab of tabs.querySelectorAll<HTMLElement>('[role="tab"]')) {
    const on = tab.dataset.tab === selected;
    tab.setAttribute("aria-selected", String(on));
    tab.tabIndex = on ? 0 : -1;
  }
  for (const [name] of TABS) panels[name].hidden = name !== selected;
}

/** 상태 거르기 칸: 첫 칸 ""은 전체, 이어서 `words`(글은 `label`). */
export function renderFilter(
  role: string,
  caption: string,
  words: readonly string[],
  label: (word: string) => string,
): HTMLElement {
  const box = el("label", "his-filter");
  const select = el("select", "ui-input his-select");
  select.dataset.role = role;
  const all = el("option", "", "전체");
  all.value = "";
  select.append(all);
  for (const word of words) {
    const option = el("option", "", label(word));
    option.value = word;
    select.append(option);
  }
  box.append(el("span", "his-filter-label", caption), select);
  return box;
}

/** 매매 상태 거르기. */
export function renderTradeFilter(): HTMLElement {
  return renderFilter("trade-state", "상태", TRADE_STATES, tradeStateLabel);
}

/** 승인 결과 거르기. */
export function renderApprovalFilter(): HTMLElement {
  return renderFilter("approval-status", "결과", APPROVAL_STATUSES, approvalStatusLabel);
}

/** 매매 상태의 배지 색. */
function tradeTone(state: string): BadgeTone {
  if (state === "blocked") return "danger";
  if (state === "awaiting_approval") return "warning";
  if (state === "sold" || state === "reviewed" || state === "holding") return "success";
  return "neutral";
}

/** 승인 결과의 배지 색. */
function approvalTone(status: string): BadgeTone {
  if (status === "approved") return "success";
  if (status === "rejected") return "danger";
  if (status === "pending") return "warning";
  return "neutral";
}

/** 칸 하나(`data-role`과 글). */
function cell(className: string, role: string, text: string): HTMLElement {
  const node = el("span", className, text);
  node.dataset.role = role;
  return node;
}

/** 목록 머리 줄(칸 이름, 화면 읽기 프로그램은 줄마다 이름을 따로 읽는다). */
export function renderHead(className: string, columns: readonly string[]): HTMLElement {
  const head = el("div", `his-head ${className}`);
  head.setAttribute("aria-hidden", "true");
  for (const column of columns) head.append(el("span", "", column));
  return head;
}

/** 매매 목록의 줄들(목록 `[data-role="trades"]`에 붙일 것). */
export function renderTradeRows(rows: readonly TradeRow[]): HTMLElement[] {
  return rows.map((row) => {
    const item = el("li", "his-trade");
    item.dataset.trade = String(row.id);
    item.dataset.state = row.state;
    const toggle = el("button", "his-row his-trade-cols");
    toggle.type = "button";
    toggle.dataset.action = "toggle";
    toggle.setAttribute("aria-expanded", "false");
    toggle.setAttribute("aria-controls", `history-trade-${row.id}`);
    toggle.setAttribute("aria-label", `매매 ${row.id} · ${row.symbol} · ${row.stateLabel} · 근거 펼치기`);
    const state = badge(row.stateLabel, tradeTone(row.state));
    state.dataset.role = "state";
    const stateCell = el("span", "his-cell-state");
    stateCell.append(state);
    toggle.append(
      cell("his-num", "id", `#${row.id}`),
      cell("his-symbol", "symbol", row.symbol),
      stateCell,
      cell("his-time", "time", row.created),
      cell("his-time his-decided", "decided", row.decided ?? "—"),
      el("span", "his-chevron"),
    );
    item.append(toggle);
    return item;
  });
}

/** 펼친 줄 안의 근거 요약과 상태 흐름. `side`는 방향 · 수량 글. */
export function renderDetail(basis: readonly BasisLine[], flow: readonly FlowRow[], side: string): HTMLElement {
  const detail = el("div", "his-detail");
  detail.dataset.role = "detail";
  const sideLine = el("p", "his-side", side);
  sideLine.dataset.role = "side";

  const list = el("dl", "his-basis");
  list.dataset.role = "basis";
  for (const line of basis) {
    const pair = el("div", "his-basis-line");
    pair.dataset.key = line.key;
    const value = el("dd", "");
    if ((line.key === "scores" || line.key === "facts") && line.text.includes(" · ")) {
      // 여러 담당 · 자료는 하나씩 떼어 보인다(담당 점수는 줄마다, 자료는 꼬리표로).
      value.className = `his-items his-items-${line.key}`;
      for (const part of line.text.split(" · ")) value.append(el("span", "his-item", part));
    } else {
      value.textContent = line.text;
    }
    pair.append(el("dt", "", line.label), value);
    list.append(pair);
  }

  const steps = el("ol", "his-flow");
  steps.dataset.role = "flow";
  for (const step of flow) {
    const node = el("li", "his-step");
    node.dataset.state = step.state;
    node.append(el("span", "his-step-label", step.label), el("time", "his-step-at", step.at));
    steps.append(node);
  }

  const basisBox = el("section", "his-detail-part");
  basisBox.append(el("h3", "his-detail-title", "근거"), list);
  const flowBox = el("section", "his-detail-part");
  flowBox.append(el("h3", "his-detail-title", "상태 흐름"), steps);
  const parts = el("div", "his-detail-parts");
  parts.append(basisBox, flowBox);
  detail.append(sideLine, parts);
  return detail;
}

/** 하루 정리의 한 덩이(제목과 글). */
function summaryPart(part: string, title: string, text: string): HTMLElement {
  const node = el("section", "his-summary-part");
  node.dataset.part = part;
  node.append(el("h3", "his-summary-title", title), el("p", "his-summary-text", text));
  return node;
}

/** 하루 정리. */
export function renderSummary(summary: DailySummary): HTMLElement {
  const box = el("div", "his-summary");
  box.dataset.role = "summary";
  const date = el("p", "his-summary-date", `${summary.date} 하루 정리`);
  date.dataset.role = "summary-date";
  box.append(
    date,
    summaryPart("done", "한 일", summary.done),
    summaryPart("pnl", "손익", summary.pnl),
    summaryPart("review", "복기", summary.review),
  );
  return box;
}

/** 승인 기록의 줄들(목록 `[data-role="approvals"]`에 붙일 것). */
export function renderApprovalRows(rows: readonly ApprovalRow[]): HTMLElement[] {
  return rows.map((row) => {
    const item = el("li", "his-row his-approval-cols his-approval");
    item.dataset.approval = String(row.id);
    item.dataset.status = row.status;
    const status = badge(row.statusLabel, approvalTone(row.status));
    status.dataset.role = "status";
    const statusCell = el("span", "his-cell-state");
    statusCell.append(status);
    const closed = row.resolved === null ? "닫히지 않음" : `닫힘 ${row.resolved}`;
    const times = el("span", "his-times");
    times.append(cell("his-time", "time", row.created), el("span", "his-time his-closed", closed));
    item.append(
      cell("his-num his-number", "number", row.number),
      statusCell,
      cell("his-approval-summary", "summary", row.summary),
      times,
    );
    return item;
  });
}
