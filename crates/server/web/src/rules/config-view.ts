// 규칙 화면의 금지 종목 · 업종표 묶음 그리기(server 35차 3번). 26차 공통 부품 · 토큰만 쓴다. 서버를 부르지 않는다
// (screen.ts가 부른다). 글은 모두 글자(`textContent`)로 넣는다.
//
// 화면 계약(시험이 고르는 표시) — 묶음 하나 `[data-role="rules-config"]`(카드 제목 "금지 종목 · 업종표") 안에:
// - 금지 종목 표 `[data-role="banned"]`: 줄 `tr[data-banned="<시장>:<종목>"]`(글에 시장 글 "국내" · "미국"과 종목).
//   비었으면 표 대신(또는 표 안에) `[data-role="no-banned"]`.
// - 업종표 `[data-role="sectors"]`: 줄 `tr[data-sector="<종목>"]`(글에 종목과 업종 이름). 비었으면 `[data-role="no-sectors"]`.
// - 업종 없는 종목 경고 `[data-role="unsectored"]`(글은 `unsectoredLine`): `unsectored`가 비면 없다.
// - 고칠 수 있으면(`editable`):
//   - 줄마다 지우기 단추 `button[data-action="remove-banned"]` · `button[data-action="remove-sector"]`.
//   - 금지 종목 더하기: `select[name="banned-market"]`(값 `kr` · `us`) · `input[name="banned-symbol"]` ·
//     `button[data-action="add-banned"]`. 업종표 더하기: `input[name="sector-symbol"]` · `input[name="sector-name"]` ·
//     `button[data-action="add-sector"]`. 더하지 못하면 까닭 글 `[data-role="config-problem"]`(요청은 나가지 않는다).
//   - 저장 단추 `button[data-action="save-config"]`. 저장하는 동안 막힌다.
// - 고칠 수 없으면(원격): 지우기 · 더하기 · 저장 단추와 입력이 없고 `[data-role="config-readonly"]`(글에
//   "이 컴퓨터에서만 바꿀 수 있습니다").
// - 저장 결과: 저장했으면 `[data-role="config-result"]`(글에 "저장했습니다"), 실패면 `[data-role="config-error"]`
//   (`role="alert"`, 글은 `saveMessage`) — 실패해도 고치던 줄은 그대로 남는다.
// - 이 묶음만 읽지 못하면 묶음 안에 `[data-role="config-load-failed"]`(글에 "불러오지 못"; `role="alert"`는 달지 않는다 —
//   다른 묶음(지금 규칙 · 절대 상한 · 개선안)은 그대로 그린다).
import { el } from "../ui/call";
import { button, card, emptyState, errorLine, table } from "../ui/parts";
import { READONLY_TEXT, unsectoredLine, type ConfigDraft, type RulesConfigReply, type SaveMessage } from "./config-model";

/** 묶음이 사용자 손짓을 알리는 곳(screen.ts가 단다). 차례는 초안의 줄 차례다. */
export interface ConfigHandlers {
  addBanned(market: string, symbol: string): void;
  removeBanned(index: number): void;
  addSector(symbol: string, sector: string): void;
  removeSector(index: number): void;
  save(): void;
}

/** 묶음의 지금 모습: 읽은 값(경고 · 고칠 수 있는가) · 초안 · 까닭 글 · 저장 결과 · 저장 중인가. */
export interface ConfigState {
  reply: RulesConfigReply;
  draft: ConfigDraft;
  problem: string | null;
  saved: SaveMessage | null;
  saving: boolean;
}

/** 카드 제목. */
const TITLE = "금지 종목 · 업종표";

/** 시장 글. */
const MARKET_TEXT: Readonly<Record<string, string>> = { kr: "국내", us: "미국" };

/** `data-role`을 단 요소. */
function marked<T extends HTMLElement>(node: T, role: string): T {
  node.dataset.role = role;
  return node;
}

/** 지우기 단추 칸. */
function removeCell(action: string, label: string, onClick: () => void): HTMLTableCellElement {
  const cell = el("td", "rul-cfg-act");
  const remove = button("지우기", "danger");
  remove.dataset.action = action;
  remove.setAttribute("aria-label", label);
  remove.addEventListener("click", onClick);
  cell.append(remove);
  return cell;
}

/** 금지 종목 표(비었으면 빈 상태 글). */
function bannedTable(state: ConfigState, handlers: ConfigHandlers): HTMLElement {
  const rows = state.draft.banned;
  if (rows.length === 0) return marked(emptyState("금지 종목이 없습니다."), "no-banned");
  const editable = state.reply.editable;
  const node = marked(table(editable ? ["시장", "종목", ""] : ["시장", "종목"], []), "banned");
  node.classList.add("rul-cfg-table");
  rows.forEach((row, index) => {
    const tr = node.tBodies[0].insertRow();
    tr.dataset.banned = `${row.market}:${row.symbol}`;
    tr.append(el("td", "rul-cfg-market", MARKET_TEXT[row.market] ?? row.market), el("td", "rul-cfg-symbol", row.symbol));
    if (editable) tr.append(removeCell("remove-banned", `${row.symbol} 금지 풀기`, () => handlers.removeBanned(index)));
  });
  return node;
}

/** 업종표(비었으면 빈 상태 글). */
function sectorTable(state: ConfigState, handlers: ConfigHandlers): HTMLElement {
  const rows = state.draft.sectors;
  if (rows.length === 0) return marked(emptyState("업종표가 비어 있습니다. 업종을 모르는 종목은 사지 않습니다."), "no-sectors");
  const editable = state.reply.editable;
  const node = marked(table(editable ? ["종목", "업종", ""] : ["종목", "업종"], []), "sectors");
  node.classList.add("rul-cfg-table");
  rows.forEach((row, index) => {
    const tr = node.tBodies[0].insertRow();
    tr.dataset.sector = row.symbol;
    tr.append(el("td", "rul-cfg-symbol", row.symbol), el("td", "rul-cfg-sector", row.sector));
    if (editable) tr.append(removeCell("remove-sector", `${row.symbol} 업종 지우기`, () => handlers.removeSector(index)));
  });
  return node;
}

/** 이름 붙은 입력 칸(라벨과 함께). */
function labeled(text: string, control: HTMLInputElement | HTMLSelectElement, name: string): HTMLElement {
  control.name = name;
  control.id = `rul-cfg-${name}`;
  control.classList.add("ui-input");
  const label = el("label", "ui-field-label", text);
  label.htmlFor = control.id;
  const box = el("div", "ui-field rul-cfg-field");
  box.append(label, control);
  return box;
}

function textInput(placeholder: string): HTMLInputElement {
  const input = el("input", "");
  input.type = "text";
  input.autocomplete = "off";
  input.placeholder = placeholder;
  return input;
}

/** 금지 종목 더하기 줄. */
function bannedForm(handlers: ConfigHandlers): HTMLElement {
  const market = el("select", "");
  for (const [value, text] of Object.entries(MARKET_TEXT)) {
    const option = el("option", "", text);
    option.value = value;
    market.append(option);
  }
  const symbol = textInput("예: 005930");
  const add = button("금지 종목 더하기");
  add.dataset.action = "add-banned";
  add.addEventListener("click", () => handlers.addBanned(market.value, symbol.value));
  const form = el("div", "rul-cfg-form");
  form.append(labeled("시장", market, "banned-market"), labeled("종목", symbol, "banned-symbol"), add);
  return form;
}

/** 업종표 더하기 줄. */
function sectorForm(handlers: ConfigHandlers): HTMLElement {
  const symbol = textInput("예: 000660");
  const name = textInput("예: 반도체");
  const add = button("업종 더하기");
  add.dataset.action = "add-sector";
  add.addEventListener("click", () => handlers.addSector(symbol.value, name.value));
  const form = el("div", "rul-cfg-form");
  form.append(labeled("종목", symbol, "sector-symbol"), labeled("업종", name, "sector-name"), add);
  return form;
}

/** 저장 단추와 결과 줄(고칠 수 있을 때). */
function saveRow(state: ConfigState, handlers: ConfigHandlers): HTMLElement {
  const save = button(state.saving ? "저장하는 중" : "저장", "accent");
  save.dataset.action = "save-config";
  save.disabled = state.saving;
  save.addEventListener("click", () => handlers.save());
  const row = el("div", "rul-cfg-save");
  row.append(save);
  if (state.saved?.kind === "saved") row.append(marked(el("p", "rul-cfg-result", state.saved.text), "config-result"));
  if (state.saved?.kind === "failed") row.append(marked(errorLine(state.saved.text), "config-error"));
  return row;
}

/** 소제목. */
function heading(text: string): HTMLElement {
  return el("h3", "rul-cfg-heading", text);
}

/** 금지 종목 · 업종표 묶음(카드 하나)을 그린다. */
export function renderConfig(state: ConfigState, handlers: ConfigHandlers): HTMLElement {
  const editable = state.reply.editable;
  const parts: Node[] = [
    el("p", "rul-hint", "금지 종목은 사지 않고, 업종표에 없는 종목도 사지 않습니다. 저장하면 다음 주문 전 점검부터 씁니다."),
  ];
  const warning = unsectoredLine(state.reply);
  if (warning !== null) parts.push(marked(el("p", "rul-cfg-warn", warning), "unsectored"));
  parts.push(heading("금지 종목"), bannedTable(state, handlers));
  if (editable) parts.push(bannedForm(handlers));
  parts.push(heading("업종표"), sectorTable(state, handlers));
  if (editable) parts.push(sectorForm(handlers));
  if (editable && state.problem !== null) {
    const problem = marked(el("p", "rul-cfg-problem", state.problem), "config-problem");
    problem.setAttribute("role", "status");
    parts.push(problem);
  }
  parts.push(editable ? saveRow(state, handlers) : marked(el("p", "rul-cfg-readonly", READONLY_TEXT), "config-readonly"));
  return marked(card(TITLE, ...parts), "rules-config");
}

/** 이 묶음만 읽지 못했을 때의 묶음(카드 하나, `[data-role="config-load-failed"]`). */
export function renderConfigFailed(): HTMLElement {
  const note = marked(
    el("p", "rul-cfg-failed", "금지 종목 · 업종표를 불러오지 못했습니다. 잠시 뒤 화면을 다시 열어 주세요."),
    "config-load-failed",
  );
  return marked(card(TITLE, note), "rules-config");
}
