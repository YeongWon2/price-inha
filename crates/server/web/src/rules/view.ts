// 규칙 화면의 그리기(server 28차 2번): 규칙 표 · 절대 상한 · 개선안 목록. 26차 공통 부품 · 토큰만 쓴다.
// 서버를 부르지 않는다(screen.ts가 부른다). 글은 모두 글자(`textContent`)로 넣는다.
//
// 화면 계약(시험이 고르는 표시):
// - 규칙 표 `[data-role="rules"]`: 줄 `tr[data-key=키]`, 그 안에 지금 값 `[data-role="value"]` · 상한 `[data-role="cap"]`.
//   최근에 바뀐 칸은 줄에 `data-changed="true"`와 `[data-role="changed"]`(글에 "바뀜"과 앞 값),
//   냉각중 변경이 걸린 칸은 `[data-role="cooling"]`(글에 "다음 거래일부터"와 뒤 값).
// - 절대 상한 `[data-role="caps"]`: 줄 `[data-key=키]`, 글에 이름과 상한.
// - 개선안 목록 `[data-role="proposals"]`: 줄 `[data-proposal=번호]`(`data-state`=상태 글자), 그 안에
//   상태 `[data-role="state"]` · 실수 종류 `[data-role="mistake"]` · 위험 `[data-role="risk"]`(`data-risky`=true|false) ·
//   바뀐 칸 `[data-role="change"][data-key=키]`(글에 앞 값 "→" 뒤 값, 넓힘은 `data-widened="true"`) ·
//   답 결과 `[data-role="answer-result"]`(`data-tone`=문구 종류). 승인 · 거절 단추
//   `button[data-action="approve"]` · `button[data-action="reject"]`는 답할 수 있는 줄(`canAnswer`)에만 있다.
import { el } from "../ui/call";
import { badge, button, table, type BadgeTone } from "../ui/parts";
import type { AnswerMessage, ProposalRow, ProposalState, RuleRow } from "./model";

/** 개선안 상태 배지 색. */
const STATE_TONES: Readonly<Record<ProposalState, BadgeTone>> = {
  proposed: "warning",
  cooling: "neutral",
  applied: "success",
  rejected: "danger",
};

/** 표시 줄 하나(`data-role`과 글). */
function mark(role: string, className: string, text: string): HTMLElement {
  const node = el("span", className, text);
  node.dataset.role = role;
  return node;
}

/** 규칙 표의 줄 하나: 이름 · 지금 값(바뀜 · 냉각중 표시) · 상한. */
function ruleRow(row: RuleRow): HTMLTableRowElement {
  const tr = document.createElement("tr");
  tr.dataset.key = row.key;
  const name = el("th", "rul-name", row.label);
  name.scope = "row";
  const value = el("td", "rul-value");
  value.append(mark("value", "rul-now", row.value));
  if (row.changed) {
    tr.dataset.changed = "true";
    value.append(mark("changed", "rul-changed", `바뀜 · 앞 값 ${row.changed.before} (개선안 ${row.changed.proposal}번)`));
  }
  if (row.cooling) {
    value.append(mark("cooling", "rul-cooling", `다음 거래일부터 ${row.cooling.after} (개선안 ${row.cooling.proposal}번)`));
  }
  const cap = el("td", "rul-cap", row.cap ?? "—");
  cap.dataset.role = "cap";
  if (row.cap === null) cap.setAttribute("aria-label", "상한 없음");
  tr.append(name, value, cap);
  return tr;
}

/** 규칙 표. */
export function renderRules(rows: readonly RuleRow[]): HTMLElement {
  const node = table(["항목", "지금 값", "상한"], []);
  node.classList.add("rul-rules");
  node.dataset.role = "rules";
  const body = node.tBodies[0];
  for (const row of rows) body.append(ruleRow(row));
  return node;
}

/** 원칙의 절대 상한 목록. */
export function renderCaps(rows: readonly { key: string; label: string; cap: string }[]): HTMLElement {
  const list = el("ul", "rul-caps");
  list.dataset.role = "caps";
  for (const row of rows) {
    const item = el("li", "rul-cap-item");
    item.dataset.key = row.key;
    item.append(el("span", "rul-cap-name", row.label), el("span", "rul-cap-value", row.cap));
    list.append(item);
  }
  return list;
}

/** 개선안 줄의 앞 → 뒤 값 목록. */
function changeList(row: ProposalRow): HTMLElement {
  const list = el("ul", "rul-changes");
  for (const change of row.changes) {
    const line = el("li", "rul-change");
    line.dataset.role = "change";
    line.dataset.key = change.key;
    if (change.widened) line.dataset.widened = "true";
    const values = el("span", "rul-change-values");
    values.append(el("span", "rul-before", change.before), el("span", "rul-arrow", " → "), el("span", "rul-after", change.after));
    line.append(el("span", "rul-change-name", change.label), values);
    if (change.widened) line.append(badge("넓힘", "warning"));
    list.append(line);
  }
  if (row.changes.length === 0) list.append(el("li", "rul-change-empty", "바뀐 칸을 읽을 수 없습니다"));
  return list;
}

/** 개선안 줄 하나. */
function proposalRow(row: ProposalRow, note: AnswerMessage | undefined): HTMLElement {
  const item = el("li", "rul-proposal");
  item.dataset.proposal = String(row.id);
  item.dataset.state = row.state;
  const head = el("div", "rul-proposal-head");
  const state = badge(row.stateLabel, STATE_TONES[row.state] ?? "neutral");
  state.dataset.role = "state";
  const risk = mark("risk", "rul-risk", row.riskLabel);
  risk.dataset.risky = String(row.risky);
  head.append(el("strong", "rul-proposal-no", `개선안 ${row.id}번`), state, mark("mistake", "rul-mistake", row.mistake), risk);
  item.append(head, changeList(row));
  const foot = el("div", "rul-proposal-foot");
  if (row.canAnswer) {
    const approve = button("승인", "accent");
    approve.dataset.action = "approve";
    const reject = button("거절", "danger");
    reject.dataset.action = "reject";
    foot.append(approve, reject);
  }
  const result = el("p", "rul-result");
  result.dataset.role = "answer-result";
  result.setAttribute("aria-live", "polite");
  if (note) {
    result.textContent = note.text;
    result.dataset.tone = note.kind;
  }
  foot.append(result);
  item.append(foot);
  return item;
}

/** 개선안 목록. `notes`는 개선안 번호 → 그 줄에 보일 답 결과 문구. */
export function renderProposals(rows: readonly ProposalRow[], notes: ReadonlyMap<number, AnswerMessage>): HTMLElement {
  const list = el("ol", "rul-proposals");
  list.dataset.role = "proposals";
  for (const row of rows) list.append(proposalRow(row, notes.get(row.id)));
  return list;
}
