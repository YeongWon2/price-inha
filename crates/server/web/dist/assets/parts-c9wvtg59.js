// src/ui/parts.ts
/** 불러오는 중 줄의 기본 글. */
var LOADING_TEXT = "불러오는 중";
/** 태그 이름 · 클래스 · 글자로 요소 하나. */
function make(tag, className, text) {
	const node = document.createElement(tag);
	node.className = className;
	if (text !== void 0) node.textContent = text;
	return node;
}
/** 단추 `<button type="button" data-kind=…>`(종류를 안 주면 `default`). */
function button(label, kind = "default") {
	const node = make("button", "ui-button", label);
	node.type = "button";
	node.dataset.kind = kind;
	return node;
}
/** 상태 배지 `<span data-tone=…>`(상태를 안 주면 `neutral`). */
function badge(text, tone = "neutral") {
	const node = make("span", "ui-badge", text);
	node.dataset.tone = tone;
	return node;
}
/** 카드 `<section>`: 첫 자식이 제목(`h2`)이고 그 뒤에 `body`가 차례로 붙는다. */
function card(title, ...body) {
	const node = make("section", "ui-card");
	node.append(make("h2", "ui-card-title", title), ...body);
	return node;
}
/** 표: 머리 칸은 `thead th[scope=col]`, 줄은 `tbody tr td`. */
function table(columns, rows) {
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
/** 빈 상태 줄(`role="status"`, 예: "준비 중"). */
function emptyState(text) {
	const node = make("p", "ui-empty", text);
	node.setAttribute("role", "status");
	return node;
}
/** 불러오는 중 줄(`role="status"`, `aria-busy="true"`). */
function loading(text = LOADING_TEXT) {
	const node = make("p", "ui-loading", text);
	node.setAttribute("role", "status");
	node.setAttribute("aria-busy", "true");
	return node;
}
/** 오류 줄(`role="alert"`). */
function errorLine(text) {
	const node = make("p", "ui-error", text);
	node.setAttribute("role", "alert");
	return node;
}
export { errorLine as a, emptyState as i, button as n, loading as o, card as r, table as s, badge as t };
