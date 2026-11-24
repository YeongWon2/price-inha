import { n as API } from "./menu-cq7sacdp.js";
// src/ui/parts.ts
/** 태그 이름 · 클래스 · 글자로 요소 하나. */
function make(tag, className, text) {
	const node = document.createElement(tag);
	node.className = className;
	if (text !== void 0) node.textContent = text;
	return node;
}
/** 빈 상태 줄(`role="status"`, 예: "준비 중"). */
function emptyState(text) {
	const node = make("p", "ui-empty", text);
	node.setAttribute("role", "status");
	return node;
}
/** 오류 줄(`role="alert"`). */
function errorLine(text) {
	const node = make("p", "ui-error", text);
	node.setAttribute("role", "alert");
	return node;
}
// src/ui/admin.ts
/** 빈 상태 글. */
var PREPARING = "준비 중 — 이 화면의 내용은 다음 단계에서 채워집니다.";
/** 세션을 얻지 못했을 때의 글. */
var SESSION_FAILED = "서버와 연결하지 못했습니다. 잠시 뒤 새로 고침해 주세요.";
/**
* 본문 자리(`main`)에 "준비 중" 빈 상태를 붙이고 세션을 얻는다. 자리가 없으면 아무것도 하지 않는다.
* 세션을 얻지 못하면 오류 줄을 하나 더한다.
*/
async function mountAdminFrame(main) {
	if (!main) return;
	main.append(emptyState(PREPARING));
	try {
		const reply = await fetch(API.session, {
			method: "POST",
			credentials: "same-origin",
			cache: "no-store"
		});
		if (!reply.ok) throw new Error(`session ${reply.status}`);
	} catch {
		main.append(errorLine(SESSION_FAILED));
	}
}
export { mountAdminFrame as t };
