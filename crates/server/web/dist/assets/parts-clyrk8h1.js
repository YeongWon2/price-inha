// src/ui/call.ts
/** 읽기 실패 글(E57). */
var LOAD_FAILED = "불러오지 못함 · 다시 시도해 주세요. 서버와 연결되지 않았거나 잠시 응답하지 않습니다.";
/** 같은 출처 경로를 부른다. 몸통을 JSON으로 읽고, 읽을 수 없으면 `null`. */
async function call(method, url, body) {
	const init = {
		method,
		credentials: "same-origin",
		cache: "no-store"
	};
	if (body !== void 0) {
		init.headers = { "Content-Type": "application/json" };
		init.body = JSON.stringify(body);
	}
	const reply = await fetch(url, init);
	let parsed = null;
	try {
		parsed = await reply.json();
	} catch {
		parsed = null;
	}
	return {
		status: reply.status,
		ok: reply.ok,
		body: parsed
	};
}
/** 태그 · 클래스 · 글로 요소 하나. 글은 글자(`textContent`)로 넣는다. */
function el(tag, className, text) {
	const node = document.createElement(tag);
	if (className) node.className = className;
	if (text !== void 0) node.textContent = text;
	return node;
}
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
/** 입력 칸: `label[for=id]`과 그 `input#id`(종류를 안 주면 `text`)를 담은 묶음. */
function field(label, id, type = "text") {
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
export { errorLine as a, table as c, el as d, emptyState as i, LOAD_FAILED as l, button as n, field as o, card as r, loading as s, badge as t, call as u };
