// src/paths.ts
/** 화면이 열리는 주소(단추가 다른 화면으로 갈 때 쓴다). */
var SCREEN_ROOTS = {
	setup: "/",
	keys: "/keys",
	office: "/office/",
	phone: "/phone/"
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
];
/** 승인 카드 읽기 경로 `/api/approvals/{id}`(폰과 같다, 번호는 주소 글자로 감싼다). */
function approvalPath(id) {
	return API.approvals + encodeURIComponent(String(id));
}
/** 승인 · 거절 경로 /api/approvals/{id}/approve · reject(폰과 같다). */
function decidePath(id, verb) {
	return approvalPath(id) + "/" + verb;
}
export { decidePath as a, approvalPath as i, OFFICE_LOOK_PATHS as n, SCREEN_ROOTS as r, API as t };
