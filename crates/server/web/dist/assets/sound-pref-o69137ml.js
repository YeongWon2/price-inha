import { n as API } from "./menu-n8p14elz.js";
import { n as call } from "./call-enmqzayl.js";
/** 서버 답에서 `enabled`(참 · 거짓)를 꺼낸다. 모양이 다르면 `null`. */
function enabledOf(body) {
	if (body === null || typeof body !== "object" || Array.isArray(body)) return null;
	const value = body.enabled;
	return typeof value === "boolean" ? value : null;
}
/** 알림음이 켜져 있나(서버에서 읽는다). 읽지 못하거나 답의 모양이 맞지 않으면 [`SOUND_DEFAULT`]. 던지지 않는다. */
async function soundEnabled() {
	try {
		const reply = await call("GET", API.setupSound);
		if (!reply.ok) return true;
		return enabledOf(reply.body) ?? true;
	} catch {
		return true;
	}
}
/**
* 알림음 켜기 · 끄기를 서버에 보내고 서버가 답한 값(저장한 뒤의 `enabled`)을 돌려준다.
* 보내지 못했거나(연결) 거절됐거나(상태) 답의 모양이 맞지 않으면 `null`. 던지지 않는다.
*/
async function saveSound(on) {
	try {
		const reply = await call("POST", API.setupSound, { enabled: on });
		if (!reply.ok) return null;
		return enabledOf(reply.body);
	} catch {
		return null;
	}
}
export { soundEnabled as n, saveSound as t };
