// 공통 부품(server 26차 2번): 카드 · 표 · 단추(기본 · 강조 · 위험) · 배지(상태) · 입력 칸 · 빈 상태 · 불러오는 중 · 오류 줄.
// 순수 DOM 함수(프레임워크 없음). 글은 모두 글자(`textContent`)로 넣는다 — HTML로 해석하지 않는다.
// 모양은 parts.css(토큰만 쓴다). 클래스 이름은 `ui-`로 시작해 화면 CSS와 겹치지 않는다.
import "./parts.css";

/** 단추 종류: 기본 · 강조 · 위험. 만든 단추의 `data-kind`에 그대로 적힌다. */
export type ButtonKind = "default" | "accent" | "danger";

/** 배지 상태: 보통 · 성공 · 경고 · 위험. 만든 배지의 `data-tone`에 그대로 적힌다. */
export type BadgeTone = "neutral" | "success" | "warning" | "danger";

/** 불러오는 중 줄의 기본 글. */
const LOADING_TEXT = "불러오는 중";

/** 태그 이름 · 클래스 · 글자로 요소 하나. */
function make<K extends keyof HTMLElementTagNameMap>(tag: K, className: string, text?: string): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

/** 단추 `<button type="button" data-kind=…>`(종류를 안 주면 `default`). */
export function button(label: string, kind: ButtonKind = "default"): HTMLButtonElement {
  const node = make("button", "ui-button", label);
  node.type = "button";
  node.dataset.kind = kind;
  return node;
}

/** 상태 배지 `<span data-tone=…>`(상태를 안 주면 `neutral`). */
export function badge(text: string, tone: BadgeTone = "neutral"): HTMLElement {
  const node = make("span", "ui-badge", text);
  node.dataset.tone = tone;
  return node;
}

/** 카드 `<section>`: 첫 자식이 제목(`h2`)이고 그 뒤에 `body`가 차례로 붙는다. */
export function card(title: string, ...body: Node[]): HTMLElement {
  const node = make("section", "ui-card");
  node.append(make("h2", "ui-card-title", title), ...body);
  return node;
}

/** 표: 머리 칸은 `thead th[scope=col]`, 줄은 `tbody tr td`. */
export function table(columns: readonly string[], rows: readonly (readonly string[])[]): HTMLTableElement {
  const node = make("table", "ui-table");
  const headRow = document.createElement("tr");
  for (const column of columns) {
    const th = document.createElement("th");
    th.scope = "col";
    th.textContent = column;
    headRow.append(th);
  }
  node.createTHead().append(headRow);
  const tbody = node.createTBody();
  for (const row of rows) {
    const tr = tbody.insertRow();
    for (const cell of row) tr.insertCell().textContent = cell;
  }
  return node;
}

/** 입력 칸: `label[for=id]`과 그 `input#id`(종류를 안 주면 `text`)를 담은 묶음. */
export function field(label: string, id: string, type = "text"): HTMLElement {
  const node = make("div", "ui-field");
  const caption = make("label", "ui-field-label", label);
  caption.htmlFor = id;
  const input = make("input", "ui-input");
  input.id = id;
  input.type = type;
  node.append(caption, input);
  return node;
}

/** 빈 상태 줄(`role="status"`, 예: "준비 중"). */
export function emptyState(text: string): HTMLElement {
  const node = make("p", "ui-empty", text);
  node.setAttribute("role", "status");
  return node;
}

/** 불러오는 중 줄(`role="status"`, `aria-busy="true"`). */
export function loading(text: string = LOADING_TEXT): HTMLElement {
  const node = make("p", "ui-loading", text);
  node.setAttribute("role", "status");
  node.setAttribute("aria-busy", "true");
  return node;
}

/** 오류 줄(`role="alert"`). */
export function errorLine(text: string): HTMLElement {
  const node = make("p", "ui-error", text);
  node.setAttribute("role", "alert");
  return node;
}
