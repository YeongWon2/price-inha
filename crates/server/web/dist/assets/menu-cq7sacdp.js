// src/paths.ts
/** 화면이 열리는 주소(단추가 다른 화면으로 갈 때 쓴다). */
var SCREEN_ROOTS = {
	setup: "/",
	keys: "/keys",
	office: "/office/",
	phone: "/phone/",
	account: "/account/",
	templates: "/templates/",
	rules: "/rules/",
	history: "/history/"
};
/** 화면들이 부르는 경로 이름 → 경로. */
var API = {
	session: "/api/session",
	socket: "/ws",
	setup: "/api/setup",
	setupKeys: "/api/setup/keys",
	setupSymbols: "/api/setup/symbols",
	setupAi: "/api/setup/ai",
	setupBackup: "/api/setup/backup",
	setupSlots: "/api/setup/slots",
	setupProviders: "/api/setup/providers",
	accessKey: "/api/access/key",
	accessUnlock: "/api/access/unlock",
	accessLog: "/api/access/log",
	remoteKeys: "/api/remote/keys",
	office: "/api/office",
	officeChat: "/api/office/chat",
	officeLooks: "/api/office/looks",
	officeLook: "/api/office/look",
	pushKey: "/api/push/key",
	pushSubscribe: "/api/push/subscribe",
	notices: "/api/notices",
	dailySummary: "/api/daily-summary",
	approvals: "/api/approvals/"
};
/**
* 사무실 담당 외형(server 20차 4번): 읽기 `GET` · 바꾸기 `POST`. 사무실 경로([`API.office`]) 아래에 둔다.
* (21차 6번) 정식 상수([`API.officeLooks`] · [`API.officeLook`])이고 `SCREEN_PATHS.office`에 들어 있다.
*/
var OFFICE_LOOK_PATHS = {
	list: API.officeLooks,
	choose: API.officeLook
};
var APPROVALS = [
	"/api/approvals/{id}",
	"/api/approvals/{id}/approve",
	"/api/approvals/{id}/reject"
];
API.session, API.socket, API.setup, API.setupKeys, API.setupSymbols, API.setupAi, API.setupBackup, API.setupSlots, API.setupProviders, API.accessKey, API.accessUnlock, API.accessLog, API.remoteKeys, API.setupSlots, API.setupProviders, [
	API.session,
	API.office,
	API.officeChat,
	API.officeLooks,
	API.officeLook,
	API.socket,
	...APPROVALS
], [
	API.pushKey,
	API.pushSubscribe,
	API.notices,
	API.dailySummary,
	...APPROVALS
], API.session, API.session, API.session, API.session;
/** 승인 카드 읽기 경로 `/api/approvals/{id}`(폰과 같다, 번호는 주소 글자로 감싼다). */
function approvalPath(id) {
	return API.approvals + encodeURIComponent(String(id));
}
/** 승인 · 거절 경로 /api/approvals/{id}/approve · reject(폰과 같다). */
function decidePath(id, verb) {
	return approvalPath(id) + "/" + verb;
}
/** 메뉴 항목 여섯(사무실 · 계좌 · 템플릿 · 규칙 · 히스토리 · 설정 차례). */
var MENU_ITEMS = [
	["office", "사무실"],
	["account", "계좌"],
	["templates", "템플릿"],
	["rules", "규칙"],
	["history", "히스토리"],
	["setup", "설정"]
].map(([screen, label]) => ({
	screen,
	label,
	href: SCREEN_ROOTS[screen]
}));
/** 메뉴 줄의 접근 이름(화면마다 같다). */
var MENU_LABEL = "주 메뉴";
/** 메뉴 줄 클래스(모양은 menu.css). */
var MENU_CLASS = "app-menu";
/**
* 메뉴 줄 `nav`(접근 이름 `aria-label`이 있다)를 만든다. 항목은 링크(`a[href]`)이고,
* 지금 화면(`current`)의 항목에만 `aria-current="page"`가 붙는다. 키 · 폰처럼 메뉴에 없는 화면이면 붙는 항목이 없다.
*/
function menu(current) {
	const nav = document.createElement("nav");
	nav.className = MENU_CLASS;
	nav.setAttribute("aria-label", MENU_LABEL);
	for (const item of MENU_ITEMS) {
		const link = document.createElement("a");
		link.href = item.href;
		link.textContent = item.label;
		if (item.screen === current) link.setAttribute("aria-current", "page");
		nav.append(link);
	}
	return nav;
}
/** 화면 HTML의 메뉴 자리(`#menu`)에 메뉴 줄을 넣고 돌려준다. 자리가 없으면 아무것도 하지 않고 `null`. */
function mountMenu(slot, current) {
	if (!slot) return null;
	const nav = menu(current);
	slot.replaceChildren(nav);
	const here = nav.querySelector("[aria-current=\"page\"]");
	if (here && nav.scrollWidth > nav.clientWidth) nav.scrollLeft = Math.max(0, here.offsetLeft - nav.clientWidth / 2 + here.offsetWidth / 2);
	return nav;
}
export { approvalPath as a, SCREEN_ROOTS as i, API as n, decidePath as o, OFFICE_LOOK_PATHS as r, mountMenu as t };
