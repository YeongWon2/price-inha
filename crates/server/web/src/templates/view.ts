// 템플릿 화면의 그리기(server 27차 3번): 칸 표 · 승격 기준 표 · 판 이력 · 실전 안내. 26차 공통 부품 · 토큰만 쓴다.
// 서버를 부르지 않는다(screen.ts가 부른다). 글은 모두 글자(`textContent`)로 넣는다.
//
// 화면 계약(시험이 고르는 표시):
// - 칸 표 `[data-role="fields"]`: 줄 `tr[data-key=키]`, 그 안에 지금 값 `[data-role="value"]` · 상한 `[data-role="cap"]` ·
//   입력 `input`(잠긴 칸 = 승격대기 · 멈춤은 `disabled`, 모의중 · 실전중은 입력할 수 있다) · 칸 옆 안내 `[data-role="field-note"]`.
// - 승인 안내 `[data-role="approval-notice"]`: 실전중 템플릿에만, 값 표 위에 "바꾸면 승인이 필요합니다" 뜻의 글.
// - 승격 기준 표 `[data-role="promotion"]`: 줄 `tr[data-criterion=기준]`, 모자란 줄은 `data-short="true"`.
// - 판 이력 `[data-role="revisions"]`: 판 `[data-revision=번호]`, 바뀐 칸 `[data-key=키]`, 넓힘은 `data-widened="true"`와 "넓힘" 글.
// - 실전 안내 `[data-role="live-notice"]`(글은 `LIVE_NOTICE`).
import { el } from "../ui/call";
import { badge, table } from "../ui/parts";
import { LIVE_NOTICE, isAdjustable, numberLabel, type CriterionName, type FieldRow, type PromotionRow, type RevisionRow } from "./model";

/** 글을 수로(쉼표 · `%` · 공백을 뺀다). 수가 아니면 NaN. */
function numberOf(text: string): number {
  const cleaned = text.replace(/[,%\s]/g, "");
  return cleaned === "" ? NaN : Number(cleaned);
}

/** 칸 줄 하나: 이름 · 지금 값 · 상한 · 입력과 칸 옆 안내. */
function fieldRow(row: FieldRow): HTMLTableRowElement {
  const tr = document.createElement("tr");
  tr.dataset.key = row.key;
  const name = el("th", "tpl-name", row.label);
  name.scope = "row";
  const value = el("td", "tpl-value", row.value);
  value.dataset.role = "value";
  const cap = el("td", "tpl-cap", row.cap ?? "—");
  cap.dataset.role = "cap";
  if (row.cap === null) cap.setAttribute("aria-label", "상한 없음");
  const entry = el("td", "tpl-entry");
  const input = el("input", "ui-input tpl-input");
  input.type = "text";
  input.inputMode = "decimal";
  input.autocomplete = "off";
  input.disabled = row.locked;
  const adjustable = isAdjustable(row.key);
  if (!adjustable) {
    // 값 맞추기 경로가 받지 않는 칸: 보기만 한다(입력 칸은 읽기 전용).
    input.readOnly = true;
    input.tabIndex = -1;
    input.placeholder = "보기만";
    input.setAttribute("aria-label", `${row.label} — 이 화면에서는 보기만 합니다`);
  } else {
    input.placeholder = row.locked ? "잠김" : "새 값";
    input.setAttribute("aria-label", `${row.label} 새 값`);
  }
  const note = el("span", "tpl-note");
  note.dataset.role = "field-note";
  note.setAttribute("aria-live", "polite");
  entry.append(input, note);
  tr.append(name, value, cap, entry);
  return tr;
}

/** 칸 표의 묶음 머리 줄. */
function groupRow(title: string, hint: string): HTMLTableRowElement {
  const tr = el("tr", "tpl-group");
  const th = el("th", "");
  th.colSpan = 4;
  th.scope = "colgroup";
  th.append(el("span", "tpl-group-title", title), el("span", "tpl-group-hint", hint));
  tr.append(th);
  return tr;
}

/** 칸 표. 잠긴 칸의 입력은 막는다. */
export function renderFields(rows: readonly FieldRow[]): HTMLElement {
  const node = table(["항목", "지금 값", "상한", "새 값"], []);
  node.classList.add("tpl-fields");
  node.dataset.role = "fields";
  const risk = node.tBodies[0];
  risk.dataset.group = "risk";
  const other = node.createTBody();
  other.dataset.group = "other";
  const riskRows = rows.filter((row) => isAdjustable(row.key));
  const otherRows = rows.filter((row) => !isAdjustable(row.key));
  if (riskRows.length > 0) {
    risk.append(groupRow("위험 한도", "절대 상한 안에서 바꿀 수 있습니다"));
    for (const row of riskRows) risk.append(fieldRow(row));
  }
  if (otherRows.length > 0) {
    other.append(groupRow("운용 방식", "이 화면에서는 보기만 합니다"));
    for (const row of otherRows) other.append(fieldRow(row));
  }
  return node;
}

/** 칸 옆 안내를 모두 지우고 `notes`의 칸 옆에 글을 단다(상한 넘음 E5). */
export function showFieldNotes(table: HTMLElement, notes: readonly { key: string; text: string }[]): void {
  for (const tr of table.querySelectorAll<HTMLTableRowElement>("tr[data-key]")) {
    const note = tr.querySelector<HTMLElement>('[data-role="field-note"]');
    const input = tr.querySelector<HTMLInputElement>("input");
    const found = notes.find((item) => item.key === tr.dataset.key);
    if (note) note.textContent = found ? found.text : "";
    if (input) {
      if (found) input.setAttribute("aria-invalid", "true");
      else input.removeAttribute("aria-invalid");
    }
    if (found) tr.dataset.invalid = "true";
    else delete tr.dataset.invalid;
  }
}

/** 칸 표의 입력에서 바꾼 값만 모은다(빈 칸 · 지금 값과 같은 칸은 뺀다). 키 → 십진수 글자. */
export function changedValues(table: HTMLElement, rows: readonly FieldRow[]): Record<string, string> {
  const changes: Record<string, string> = {};
  const lines = [...table.querySelectorAll<HTMLTableRowElement>("tr[data-key]")];
  for (const row of rows) {
    if (row.locked) continue;
    const input = lines.find((line) => line.dataset.key === row.key)?.querySelector<HTMLInputElement>("input");
    if (!input || input.disabled || input.readOnly) continue;
    const typed = input.value.trim().replace(/,/g, "").replace(/%$/, "").trim();
    if (typed === "") continue;
    const now = numberOf(row.value);
    const next = Number(typed);
    if (Number.isFinite(now) && Number.isFinite(next) && now === next) continue;
    changes[row.key] = typed;
  }
  return changes;
}

/** 승격 기준의 필요한 값 글(단위 · 비교). */
function needText(criterion: CriterionName, need: string): string {
  const value = numberLabel(need);
  switch (criterion) {
    case "trades":
      return `${value}건 이상`;
    case "trading_days":
      return `${value}거래일 이상`;
    case "max_drawdown":
      return `${value}% 이하`;
    default:
      return `${value} 초과`;
  }
}

/** 승격 기준의 지금 값 글. */
function haveText(criterion: CriterionName, have: string): string {
  if (!/^-?\d/.test(have)) return have;
  const value = numberLabel(have);
  switch (criterion) {
    case "trades":
      return `${value}건`;
    case "trading_days":
      return `${value}거래일`;
    case "max_drawdown":
      return `${value}%`;
    default:
      return value;
  }
}

/** 승격 기준 표. 모자란 줄은 `data-short="true"`. */
export function renderPromotion(rows: readonly PromotionRow[]): HTMLElement {
  const node = table(["기준", "필요", "지금", "판정"], []);
  node.classList.add("tpl-promotion");
  node.dataset.role = "promotion";
  const body = node.tBodies[0];
  for (const row of rows) {
    const tr = document.createElement("tr");
    tr.dataset.criterion = row.criterion;
    tr.dataset.short = row.short ? "true" : "false";
    const name = el("th", "tpl-name", row.label);
    name.scope = "row";
    const verdict = el("td", "tpl-verdict");
    verdict.append(badge(row.met ? "채움" : "모자람", row.met ? "success" : "warning"));
    tr.append(name, el("td", "tpl-num", needText(row.criterion, row.need)), el("td", "tpl-num", haveText(row.criterion, row.have)), verdict);
    body.append(tr);
  }
  return node;
}

/** 유닉스 밀리초 → "2025. 11. 17. 09:40". */
function timeText(at: number): string {
  return new Date(at).toLocaleString("ko-KR", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
  });
}

/** 판 이력. 판마다 바뀐 칸과 넓힘 표시. */
export function renderRevisions(rows: readonly RevisionRow[]): HTMLElement {
  const list = el("ol", "tpl-revisions");
  list.dataset.role = "revisions";
  for (const row of rows) {
    const item = el("li", "tpl-revision");
    item.dataset.revision = String(row.revision);
    const head = el("div", "tpl-revision-head");
    head.append(el("strong", "tpl-revision-no", `판 ${row.revision}`));
    const time = el("time", "tpl-revision-at", timeText(row.at));
    time.dateTime = new Date(row.at).toISOString();
    head.append(time);
    if (row.proposal !== null) head.append(badge(`개선안 ${row.proposal}번`));
    item.append(head);
    if (row.changes.length === 0) {
      item.append(el("p", "tpl-revision-empty", row.revision === 1 ? "처음 값(고른 스타일의 기본값)" : "바뀐 칸 없음"));
    } else {
      const changes = el("ul", "tpl-changes");
      for (const change of row.changes) {
        const line = el("li", "tpl-change");
        line.dataset.key = change.key;
        if (change.widened) line.dataset.widened = "true";
        const values = el("span", "tpl-change-values");
        values.append(el("span", "tpl-before", change.before), el("span", "tpl-arrow", " → "), el("span", "tpl-after", change.after));
        line.append(el("span", "tpl-change-name", change.label), values);
        if (change.widened) line.append(badge("넓힘", "warning"));
        changes.append(line);
      }
      item.append(changes);
    }
    list.append(item);
  }
  return list;
}

/** 실전 안내 줄(`LIVE_NOTICE`). */
export function liveNotice(): HTMLElement {
  const node = el("p", "tpl-live-notice", LIVE_NOTICE);
  node.dataset.role = "live-notice";
  node.setAttribute("role", "note");
  return node;
}
