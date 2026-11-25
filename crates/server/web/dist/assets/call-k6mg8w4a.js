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
export { call as n, el as r, LOAD_FAILED as t };
