import { a as errorLine, i as emptyState } from "./parts-c9wvtg59.js";
import { n as API } from "./menu-myat6qpc.js";
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
