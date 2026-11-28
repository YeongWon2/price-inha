import { n as API, t as mountMenu } from "./menu-kc5ybrux.js";
import { n as submitKeyFields, t as applyKeyFieldRules } from "./field-e6l8inco.js";
import { n as call, r as el, t as LOAD_FAILED } from "./call-enmqzayl.js";
import { a as errorLine, c as table, i as emptyState, n as button, o as field, r as card, s as loading, t as badge } from "./parts-ma3d1o7o.js";
import { n as soundEnabled, t as saveSound } from "./sound-pref-ctkxfhgf.js";
// src/setup/ai-limit.ts
/** 설정 상태에서 하루 AI 호출 상한을 읽는 칸 이름. */
var AI_LIMIT_FIELD = "ai_daily_limit";
/** 바꾸는 요청의 칸 이름. */
var AI_LIMIT_KEY = "daily_limit";
/** 지금 값 글자: 없으면 "없음", 있으면 "N번". */
function formatAiLimit(limit) {
	return limit === null ? "없음" : `${limit}번`;
}
/** 설정 상태 본문에서 지금 값을 읽는다(칸이 없거나 수가 아니면 `null`). */
function readAiLimit(status) {
	if (typeof status !== "object" || status === null) return null;
	const value = status[AI_LIMIT_FIELD];
	return typeof value === "number" && Number.isInteger(value) && value >= 1 ? value : null;
}
/** 입력 칸 글자를 값으로: 비었으면 `null`(없음), 1 이상 정수면 그 수, 그 밖은 잘못(`ok: false`). */
function parseAiLimit(input) {
	const text = input.trim();
	if (text === "") return {
		ok: true,
		limit: null
	};
	if (!/^[0-9]+$/.test(text)) return { ok: false };
	const limit = Number(text);
	if (!Number.isSafeInteger(limit) || limit < 1 || limit > 4294967295) return { ok: false };
	return {
		ok: true,
		limit
	};
}
/** 바꾸는 요청: 경로는 AI 설정 저장 길, 본문은 `{"daily_limit": 값}` 하나. */
function aiLimitRequest(limit) {
	return {
		path: API.setupAi,
		body: { [AI_LIMIT_KEY]: limit }
	};
}
// src/setup/messenger-model.ts
/** 상태 → 화면 글. */
var STATE_LABELS = {
	off: "꺼짐",
	polling: "받는 중",
	unauthorized: "토큰 거절",
	error: "오류"
};
/** 페어링 안내(토큰이 있고 아직 연결된 사람이 없을 때). */
var PAIRING_HINT = "봇에게 아무 말이나 보내면 코드가 옵니다";
/** 연결된 사람이 있을 때. */
var PAIRED_TEXT = "연결됨";
/** 다른 기기에서 볼 때(보기만). 403의 글이기도 하다. */
var REMOTE_NOTE$1 = "이 컴퓨터에서만 바꿀 수 있습니다";
/** 틀린 코드(400, 서버 `MESSENGER_WRONG_CODE`와 같은 글). */
var WRONG_CODE = "코드가 맞지 않습니다";
/** 잠김(423)의 앞부분(서버 `MESSENGER_LOCKED`와 같은 글). 뒤에 풀리는 시각이 붙는다. */
var LOCKED_TEXT$1 = "틀린 코드가 많아 잠시 승인을 받지 않습니다";
/** 이미 연결된 사람이 있음(409 `owner_exists`). */
var OWNER_EXISTS = "이미 연결된 사람이 있습니다 — 먼저 빼세요";
/** 토큰이 거절됨(422 `key_rejected`). */
var TOKEN_REJECTED = "봇 토큰이 맞지 않습니다";
/** 텔레그램에 닿지 못함(502 `unreachable`). */
var UNREACHABLE = "텔레그램에 닿지 못했습니다";
/** 시도가 너무 많음(429 `too_many_attempts`). */
var TOO_MANY = "시도가 너무 많습니다. 잠시 뒤에 다시 해 주세요";
/** 상태 글자 목록(모르는 글자를 걸러 낸다). */
var STATES = [
	"off",
	"polling",
	"unauthorized",
	"error"
];
function isRecord(value) {
	return typeof value === "object" && value !== null && !Array.isArray(value);
}
function isState(value) {
	return typeof value === "string" && STATES.includes(value);
}
/** 텔레그램 칸을 읽는다. 모양이 틀리면 `null`. */
function parseTelegram(value) {
	if (!isRecord(value)) return null;
	const { configured, bot, state, paired, pending, locked_until: lockedUntil } = value;
	if (typeof configured !== "boolean" || typeof paired !== "boolean") return null;
	if (bot !== null && typeof bot !== "string") return null;
	if (!isState(state)) return null;
	if (typeof pending !== "number" || !Number.isFinite(pending)) return null;
	if (lockedUntil !== null && (typeof lockedUntil !== "number" || !Number.isFinite(lockedUntil))) return null;
	return {
		configured,
		bot,
		state,
		paired,
		pending,
		locked_until: lockedUntil
	};
}
/**
* `GET /api/messenger` 몸통을 읽는다. 모양이 틀리면 `null`(칸 이름 · 형은 [`MessengerStatus`] 그대로, 모르는 상태
* 글자도 `null`).
*/
function parseMessengerStatus(body) {
	if (!isRecord(body) || typeof body.editable !== "boolean") return null;
	const telegram = parseTelegram(body.telegram);
	return telegram === null ? null : {
		editable: body.editable,
		telegram
	};
}
/** 두 자리 숫자. */
function pad(value) {
	return String(value).padStart(2, "0");
}
/** 잠긴 동안의 글: [`LOCKED_TEXT`] 뒤에 풀리는 시각(이 기기의 시각 `HH:MM`). */
function lockedText(until) {
	const at = new Date(until);
	if (!Number.isFinite(until) || Number.isNaN(at.getTime())) return LOCKED_TEXT$1;
	return `${LOCKED_TEXT$1} (${pad(at.getHours())}:${pad(at.getMinutes())}에 풀립니다)`;
}
/** 몸통의 `error` 글자(없으면 빈 글). */
function errorOf(body) {
	return isRecord(body) && typeof body.error === "string" ? body.error : "";
}
/** 몸통의 `message` 글자(없으면 `null`). */
function messageOf(body) {
	return isRecord(body) && typeof body.message === "string" && body.message !== "" ? body.message : null;
}
/**
* 실패한 답 → 화면 글: 403 → [`REMOTE_NOTE`], 400 `wrong_code` → [`WRONG_CODE`], 423 → [`lockedText`]
* (`locked_until`), 409 `demo_mode` → 서버가 준 `message`(없으면 [`DEMO_REFUSED`]), 409 `owner_exists` →
* [`OWNER_EXISTS`], 422 `key_rejected` → [`TOKEN_REJECTED`], 502 `unreachable` → [`UNREACHABLE`],
* 429 → [`TOO_MANY`]. 그 밖은 `ui/call.ts`의 `LOAD_FAILED`. 남은 횟수 같은 숫자를 만들어 넣지 않는다.
*/
function messengerError(status, body) {
	const error = errorOf(body);
	if (status === 403) return REMOTE_NOTE$1;
	if (status === 400 && error === "wrong_code") return WRONG_CODE;
	if (status === 423) return lockedText(isRecord(body) && typeof body.locked_until === "number" ? body.locked_until : NaN);
	if (status === 409 && error === "demo_mode") return messageOf(body) ?? "시연 모드에서는 메신저를 쓰지 않습니다";
	if (status === 409 && error === "owner_exists") return OWNER_EXISTS;
	if (status === 422 && error === "key_rejected") return TOKEN_REJECTED;
	if (status === 502 && error === "unreachable") return UNREACHABLE;
	if (status === 429) return TOO_MANY;
	return LOAD_FAILED;
}
/** 토큰 등록 몸통: 앞뒤 공백을 뗀다. */
function registerBody(token) {
	return { token: token.trim() };
}
/** 페어링 승인 몸통: 앞뒤 공백을 떼고 대문자로. */
function approveBody(code) {
	return { code: code.trim().toUpperCase() };
}
// src/setup/messenger-view.ts
/** 묶음 제목. */
var TITLE = "메신저";
/** 상태 배지의 빛깔(공통 부품의 상태 이름만 쓴다). */
var STATE_TONES = {
	off: "neutral",
	polling: "success",
	unauthorized: "danger",
	error: "warning"
};
/** 이 묶음의 겉(`data-role="messenger"`). */
function shell$1(...body) {
	const node = card(TITLE, ...body);
	node.dataset.role = "messenger";
	return node;
}
/** 상태를 읽는다. 실패(상태 · 연결 · 모양)면 `null`. */
async function readStatus() {
	try {
		const reply = await call("GET", API.messenger);
		return reply.ok ? parseMessengerStatus(reply.body) : null;
	} catch {
		return null;
	}
}
/** 읽지 못했을 때의 묶음. */
function failedBlock$2() {
	const message = el("div", "");
	message.dataset.role = "messenger-message";
	message.append(errorLine(LOAD_FAILED));
	return shell$1(message);
}
/** 단추 하나를 만든다(`data-action`). */
function actionButton(label, action, kind = "default") {
	const node = button(label, kind);
	node.dataset.action = action;
	return node;
}
/** (다듬기 E 7번) 성공 답의 `notice`(토큰 지우기의 BotFather 폐기 안내 등). 글이 아니면 `null`. */
function noticeOf(body) {
	const notice = typeof body === "object" && body !== null ? body.notice : null;
	return typeof notice === "string" && notice !== "" ? notice : null;
}
/** 새로 그린 묶음의 결과 자리에 안내 한 줄(`messenger-notice`)을 넣는다(없으면 그대로). */
function withNotice(box, notice) {
	const message = box.querySelector("[data-role=\"messenger-message\"]");
	if (notice !== null && message) {
		const line = el("p", "hint", notice);
		line.dataset.role = "messenger-notice";
		message.append(line);
	}
	return box;
}
/** 단추를 누르면 `ask()`가 만든 요청을 보내고, 됐으면 다시 읽어 묶음을 새로 그린다. 실패면 결과 자리에 오류 줄. */
function wire(box, message, node, ask) {
	node.addEventListener("click", () => {
		node.disabled = true;
		message.replaceChildren();
		(async () => {
			const { method, url, body } = ask();
			let text = null;
			let notice = null;
			try {
				const reply = await call(method, url, body);
				if (!reply.ok) text = messengerError(reply.status, reply.body);
				else notice = noticeOf(reply.body);
			} catch {
				text = LOAD_FAILED;
			}
			if (text === null) {
				const next = await readStatus();
				if (next !== null) {
					box.replaceWith(withNotice(messengerBlock(next), notice));
					return;
				}
				text = LOAD_FAILED;
			}
			message.replaceChildren(errorLine(text));
			node.disabled = false;
		})();
	});
}
/** 칸 하나의 입력 요소(`field`가 만든 것). */
function inputOf$2(holder) {
	return holder.querySelector("input");
}
/** 상태 줄: 배지(`messenger-state`)와 봇 이름(`messenger-bot`, 있으면). */
function stateLine(status) {
	const line = el("div", "");
	const state = status.telegram.state;
	const mark = badge(STATE_LABELS[state], STATE_TONES[state]);
	mark.dataset.role = "messenger-state";
	mark.dataset.state = state;
	line.append(el("span", "", "텔레그램 "), mark);
	if (status.telegram.bot !== null) {
		const bot = el("span", "", ` @${status.telegram.bot}`);
		bot.dataset.role = "messenger-bot";
		line.append(bot);
	}
	return line;
}
/** 이 컴퓨터에서 바꾸는 칸 · 단추들. */
function editParts(box, message, status) {
	const parts = [];
	const tokenField = field("봇 토큰", "messenger-token", "password");
	const token = inputOf$2(tokenField);
	token.autocomplete = "new-password";
	const register = actionButton("등록", "messenger-register", "accent");
	wire(box, message, register, () => {
		const body = registerBody(token.value);
		token.value = "";
		return {
			method: "POST",
			url: API.messengerTelegram,
			body
		};
	});
	parts.push(tokenField, register);
	const telegram = status.telegram;
	if (telegram.configured) {
		const remove = actionButton("지우기", "messenger-remove", "danger");
		wire(box, message, remove, () => ({
			method: "DELETE",
			url: API.messengerTelegram
		}));
		parts.push(remove);
	}
	if (telegram.configured && !telegram.paired) {
		const pairing = el("div", "");
		pairing.dataset.role = "messenger-pairing";
		const codeField = field("페어링 코드", "messenger-code");
		const code = inputOf$2(codeField);
		code.autocomplete = "off";
		const approve = actionButton("승인", "messenger-approve", "accent");
		wire(box, message, approve, () => ({
			method: "POST",
			url: API.messengerApprove,
			body: approveBody(code.value)
		}));
		pairing.append(el("p", "", PAIRING_HINT), codeField, approve);
		parts.push(pairing);
	}
	if (telegram.paired) {
		const paired = el("div", "");
		paired.dataset.role = "messenger-paired";
		const revoke = actionButton("빼기", "messenger-revoke", "danger");
		wire(box, message, revoke, () => ({
			method: "POST",
			url: API.messengerRevoke
		}));
		paired.append(el("span", "", `${PAIRED_TEXT} `), revoke);
		parts.push(paired);
	}
	return parts;
}
/** 다른 기기에서 보는 줄들(입력 · 단추 없음). */
function remoteParts(status) {
	const note = el("p", "", REMOTE_NOTE$1);
	note.dataset.role = "messenger-remote";
	const parts = [note];
	if (status.telegram.paired) {
		const paired = el("p", "", PAIRED_TEXT);
		paired.dataset.role = "messenger-paired";
		parts.push(paired);
	}
	return parts;
}
/** 상태 하나로 메신저 묶음(`data-role="messenger"`)을 그린다. 단추는 위 계약대로 서버를 부르고 다시 그린다. */
function messengerBlock(status) {
	const message = el("div", "");
	message.dataset.role = "messenger-message";
	const box = shell$1();
	const middle = status.editable ? editParts(box, message, status) : remoteParts(status);
	box.append(stateLine(status), ...middle, message);
	return box;
}
/** 자리 하나에 메신저 묶음을 읽어 그린다(`GET API.messenger`). 읽기가 실패해도 던지지 않는다. */
async function mountMessenger(root) {
	root.replaceChildren(shell$1(loading()));
	const status = await readStatus();
	root.replaceChildren(status === null ? failedBlock$2() : messengerBlock(status));
}
// src/setup/notify-model.ts
/** 묶음 제목. */
var NOTIFY_TITLE = "알림";
/** 폰 칸이 없는 줄의 폰 칸 자리 글. */
var NO_PUSH_TEXT = "폰 없음";
/** 잠긴 줄의 표시 글. */
var LOCKED_TEXT = "끌 수 없음";
/** 잠긴 종류 안내(표 아래 한 줄). */
var LOCKED_NOTE = "승인 요청 · 멈춤 · 이상 알림은 적어도 한 곳으로는 갑니다";
/** 조용한 시간 안내. */
var QUIET_NOTE = "조용한 시간에는 급하지 않은 알림을 미룹니다. 승인 요청 · 멈춤 같은 급한 알림은 그대로 갑니다";
/** 저장 성공. */
var SAVED_TEXT = "저장했습니다";
/** 다른 기기에서 볼 때(보기만). 403의 글이기도 하다. */
var REMOTE_NOTE = "이 컴퓨터에서만 바꿀 수 있습니다";
/** 잠긴 종류를 둘 다 끄려 함(400 `locked_kind_off`). 뒤에 " — <종류 글>"이 붙을 수 있다. */
var LOCKED_OFF_TEXT = "이 알림은 폰과 메신저를 모두 끌 수 없습니다";
/** 조용한 시간의 시각 글자가 틀림(400 `invalid_quiet_hours`). */
var INVALID_QUIET_TEXT = "조용한 시간의 시작과 끝을 HH:MM으로, 서로 다르게 적어 주세요";
/** 그 밖의 거부. */
var SAVE_FAILED = "저장하지 못했습니다";
/** `GET` 몸통을 읽는다. 모양이 틀리면 `null`(종류 줄 하나라도 틀리면 `null`). */
function parseNotifySettings(body) {
	if (!isObject(body) || !Array.isArray(body.kinds) || typeof body.editable !== "boolean") return null;
	const kinds = [];
	for (const row of body.kinds) {
		const parsed = parseKind(row);
		if (parsed === null) return null;
		kinds.push(parsed);
	}
	const quiet = parseQuiet(body.quiet);
	if (quiet === null) return null;
	return {
		kinds,
		quiet,
		editable: body.editable
	};
}
/** 몸통이 JSON 객체(배열 아님)인가. */
function isObject(value) {
	return typeof value === "object" && value !== null && !Array.isArray(value);
}
/** 종류 한 줄. 칸 하나라도 틀리면 `null`. */
function parseKind(row) {
	if (!isObject(row)) return null;
	const { kind, label, push, messenger, locked } = row;
	if (typeof kind !== "string" || typeof label !== "string") return null;
	if (typeof push !== "boolean" && push !== null) return null;
	if (typeof messenger !== "boolean" || typeof locked !== "boolean") return null;
	return {
		kind,
		label,
		push,
		messenger,
		locked
	};
}
/** 조용한 시간. 칸 하나라도 틀리면 `null`. */
function parseQuiet(quiet) {
	if (!isObject(quiet)) return null;
	const { enabled, start, end } = quiet;
	if (typeof enabled !== "boolean" || typeof start !== "string" || typeof end !== "string") return null;
	return {
		enabled,
		start,
		end
	};
}
/**
* 바꾸기(`POST`) 몸통: `{"kinds": [{"kind", "push", "messenger"}], "quiet": {"enabled", "start", "end"}}` — 모든 종류를
* 받은 차례 그대로 통째로 보낸다(`label` · `locked` · `editable`은 싣지 않는다).
*/
function saveBody(settings) {
	return {
		kinds: settings.kinds.map(({ kind, push, messenger }) => ({
			kind,
			push,
			messenger
		})),
		quiet: {
			enabled: settings.quiet.enabled,
			start: settings.quiet.start,
			end: settings.quiet.end
		}
	};
}
/**
* 바꾸기 실패의 글: 403은 [`REMOTE_NOTE`], 400 `locked_kind_off`는 [`LOCKED_OFF_TEXT`](몸통의 `kind`가 `labels`에
* 있으면 " — <종류 글>"을 잇는다), 400 `invalid_quiet_hours`는 [`INVALID_QUIET_TEXT`], 그 밖은 [`SAVE_FAILED`].
*/
function notifyError(status, body, labels = []) {
	if (status === 403) return REMOTE_NOTE;
	if (status !== 400 || !isObject(body)) return SAVE_FAILED;
	if (body.error === "locked_kind_off") {
		const label = labels.find((row) => row.kind === body.kind)?.label;
		return label === void 0 ? LOCKED_OFF_TEXT : `${LOCKED_OFF_TEXT} — ${label}`;
	}
	if (body.error === "invalid_quiet_hours") return INVALID_QUIET_TEXT;
	return SAVE_FAILED;
}
// src/setup/notify-view.ts
/** 표의 머리 칸. */
var COLUMNS = [
	"종류",
	"폰",
	"메신저"
];
/** 이 묶음의 겉(`data-role="notify"`). */
function shell(...body) {
	const node = card(NOTIFY_TITLE, ...body);
	node.dataset.role = "notify";
	return node;
}
/** 설정을 읽는다. 실패(상태 · 연결 · 모양)면 `null`. */
async function readSettings() {
	try {
		const reply = await call("GET", API.notifySettings);
		return reply.ok ? parseNotifySettings(reply.body) : null;
	} catch {
		return null;
	}
}
/** 켜고 끄는 칸 하나(`input[type=checkbox][name]`). */
function toggle(name, checked, label, editable) {
	const input = el("input", "");
	input.type = "checkbox";
	input.name = name;
	input.checked = checked;
	input.disabled = !editable;
	input.setAttribute("aria-label", label);
	return input;
}
/**
* (server 41차 2번) 폰 칸이 없는 종류 한 줄(`tr[data-role="notify-talk"]`, 종류 글자는 `data-talk`): 종류 글 · 폰 칸
* 자리 글 [`NO_PUSH_TEXT`] · 메신저 칸.
*/
function talkRow(tbody, row, editable) {
	const tr = tbody.insertRow();
	tr.dataset.role = "notify-talk";
	tr.dataset.talk = row.kind;
	tr.insertCell().append(el("span", "", row.label));
	tr.insertCell().append(el("span", "", NO_PUSH_TEXT));
	tr.insertCell().append(toggle("messenger", row.messenger, `${row.label} 메신저 알림`, editable));
}
/** 종류 한 줄(`tr[data-kind]`, 잠긴 줄은 `data-locked="true"`와 [`LOCKED_TEXT`]). 폰 칸이 없으면 [`talkRow`]. */
function kindRow(tbody, row, editable) {
	if (row.push === null) {
		talkRow(tbody, row, editable);
		return;
	}
	const tr = tbody.insertRow();
	tr.dataset.kind = row.kind;
	if (row.locked) tr.dataset.locked = "true";
	const name = tr.insertCell();
	name.append(el("span", "", row.label));
	if (row.locked) name.append(" ", badge(LOCKED_TEXT, "warning"));
	tr.insertCell().append(toggle("push", row.push, `${row.label} 폰 알림`, editable));
	tr.insertCell().append(toggle("messenger", row.messenger, `${row.label} 메신저 알림`, editable));
}
/** 종류별 표(`data-role="notify-kinds"`). */
function kindsTable(settings) {
	const grid = table(COLUMNS, []);
	const tbody = grid.tBodies[0] ?? grid.createTBody();
	for (const row of settings.kinds) kindRow(tbody, row, settings.editable);
	const holder = el("div", "");
	holder.dataset.role = "notify-kinds";
	holder.append(grid, el("p", "", LOCKED_NOTE));
	return holder;
}
/** 칸 묶음(`field`)의 입력 요소. */
function inputOf$1(holder) {
	return holder.querySelector("input");
}
/** 조용한 시간 칸(`data-role="notify-quiet"`): 켜기 · 시작 · 끝(`HH:MM`). */
function quietPart(settings) {
	const on = field("조용한 시간 켜기", "notify-quiet-on", "checkbox");
	on.classList.add("setup-extra-check");
	const start = field("시작", "notify-quiet-start", "time");
	const end = field("끝", "notify-quiet-end", "time");
	inputOf$1(on).checked = settings.quiet.enabled;
	inputOf$1(start).value = settings.quiet.start;
	inputOf$1(end).value = settings.quiet.end;
	for (const holder of [
		on,
		start,
		end
	]) inputOf$1(holder).disabled = !settings.editable;
	const part = el("div", "");
	part.dataset.role = "notify-quiet";
	part.append(on, start, end, el("p", "", QUIET_NOTE));
	return part;
}
/** 지금 칸 값으로 바꾼 설정(서버가 준 차례 그대로). */
function currentValues(box, settings) {
	const checked = (selector, fallback) => box.querySelector(selector)?.checked ?? fallback;
	const value = (selector, fallback) => box.querySelector(selector)?.value ?? fallback;
	const lines = [...box.querySelectorAll("tr[data-kind], tr[data-role=\"notify-talk\"]")];
	const kinds = settings.kinds.map((row) => {
		const line = lines.find((tr) => (row.push === null ? tr.dataset.talk : tr.dataset.kind) === row.kind);
		const flag = (name) => line?.querySelector(`input[name="${name}"]`)?.checked;
		const push = row.push === null ? null : flag("push") ?? row.push;
		return {
			...row,
			push,
			messenger: flag("messenger") ?? row.messenger
		};
	});
	const quiet = {
		enabled: checked("#notify-quiet-on", settings.quiet.enabled),
		start: value("#notify-quiet-start", settings.quiet.start),
		end: value("#notify-quiet-end", settings.quiet.end)
	};
	return {
		...settings,
		kinds,
		quiet
	};
}
/** 저장 단추: 지금 칸 값으로 통째 보낸다. 성공이면 받은 값으로 다시 그리고, 실패면 오류 줄(칸 값은 그대로). */
function saveButton(box, message, settings) {
	const save = button("저장", "accent");
	save.dataset.action = "notify-save";
	save.addEventListener("click", () => {
		save.disabled = true;
		message.replaceChildren();
		(async () => {
			let text;
			try {
				const reply = await call("POST", API.notifySettings, saveBody(currentValues(box, settings)));
				const next = reply.ok ? parseNotifySettings(reply.body) : null;
				if (next !== null) {
					const fresh = notifyBlock(next);
					const done = fresh.querySelector("[data-role=\"notify-message\"]");
					if (done) done.replaceChildren(savedLine());
					box.replaceWith(fresh);
					return;
				}
				text = notifyError(reply.status, reply.body, settings.kinds);
			} catch {
				text = notifyError(0, null);
			}
			message.replaceChildren(errorLine(text));
			save.disabled = false;
		})();
	});
	return save;
}
/** 저장 성공 줄(`role="status"`). */
function savedLine() {
	const line = el("p", "", SAVED_TEXT);
	line.setAttribute("role", "status");
	return line;
}
/** 설정 하나로 알림 묶음(`data-role="notify"`)을 그린다. 단추는 위 계약대로 서버를 부르고 다시 그린다. */
function notifyBlock(settings) {
	const message = el("div", "");
	message.dataset.role = "notify-message";
	const box = shell(kindsTable(settings), quietPart(settings));
	if (settings.editable) box.append(saveButton(box, message, settings));
	else {
		const remote = el("p", "", REMOTE_NOTE);
		remote.dataset.role = "notify-remote";
		box.append(remote);
	}
	box.append(message);
	return box;
}
/** 읽지 못했을 때의 묶음. */
function failedBlock$1() {
	const message = el("div", "");
	message.dataset.role = "notify-message";
	message.append(errorLine(LOAD_FAILED));
	return shell(message);
}
/** 자리 하나에 알림 묶음을 읽어 그린다(`GET API.notifySettings`). 읽기가 실패해도 던지지 않는다. */
async function mountNotify(root) {
	root.replaceChildren(shell(loading()));
	const settings = await readSettings();
	root.replaceChildren(settings === null ? failedBlock$1() : notifyBlock(settings));
}
// src/setup/semantic-lan.ts
/** 켜기 칸의 글. */
var SEMANTIC_LAN_LABEL = "같은 집 다른 기기(사설망)의 Ollama 허용";
/** 켜기 칸과 "밖으로 보냄" 줄을 만든다. */
function semanticLan(privateNetwork, outside) {
	const holder = field(SEMANTIC_LAN_LABEL, "semantic-lan", "checkbox");
	const box = holder.querySelector("input");
	const line = el("div", "setup-extra-outside");
	const paint = (on, text) => {
		box.checked = on === true;
		line.replaceChildren(...typeof text === "string" && text !== "" ? [outsideLine(text)] : []);
	};
	paint(privateNetwork, outside);
	return {
		nodes: [holder, line],
		checked: () => box.checked,
		paint
	};
}
function outsideLine(text) {
	const node = el("p", "hint", text);
	node.dataset.role = "semantic-outside";
	return node;
}
// src/setup/models-view.ts
/** "직접 입력" 고르기의 값. */
var MODEL_CUSTOM = "__custom__";
/** "직접 입력" 고르기의 글. */
var MODEL_CUSTOM_LABEL = "직접 입력";
/** 비용 안내. */
var MODELS_COST_NOTE = "비싼 모델은 하루 AI 호출 상한과 비용에 영향을 줍니다.";
/** 언제 쓰이나 안내. */
var MODELS_APPLY_NOTE = "바꾼 모델은 다시 켜지 않아도 다음 AI 턴부터 쓰입니다. 도는 논의는 시작할 때의 모델로 끝납니다.";
/** 다른 기기 안내. */
var MODELS_REMOTE_NOTE = "담당 모델은 이 컴퓨터에서만 바꿀 수 있습니다.";
/** 저장 안내. */
var MODELS_SAVED = "저장했습니다. 다음 AI 턴부터 쓰입니다.";
/** 바꾸기 실패 까닭 → 화면 글. */
var MODELS_ERRORS = {
	invalid_input: "모델 이름은 영문 소문자 · 숫자 · '-' · '.'로 쓰고, 첫 글자는 영문이나 숫자, 64자까지입니다.",
	local_only: MODELS_REMOTE_NOTE
};
/** 객체면 그대로, 아니면 빈 객체. */
function record$1(value) {
	return value !== null && typeof value === "object" ? value : {};
}
/** 칸 넷이 모두 글자인 줄들만. */
function strings(list, keys) {
	if (!Array.isArray(list)) return [];
	return list.map(record$1).filter((row) => keys.every((key) => typeof row[key] === "string"));
}
/** 답이 고르기 표 모양이면 그 값. */
function modelsReplyOf(value) {
	const body = record$1(value);
	if (!Array.isArray(body.roles) || typeof body.editable !== "boolean") return null;
	const roles = strings(body.roles, [
		"agent",
		"name",
		"model",
		"default"
	]);
	if (roles.length === 0) return null;
	return {
		roles,
		choices: strings(body.choices, [
			"id",
			"label",
			"note"
		]),
		editable: body.editable
	};
}
/** 바꾸기 실패 까닭 → 화면 글. */
function errorText(status, body) {
	const reason = record$1(body).error;
	if (status === 403) return MODELS_REMOTE_NOTE;
	if (typeof reason === "string" && MODELS_ERRORS[reason]) return MODELS_ERRORS[reason];
	return LOAD_FAILED;
}
/** 고르기 · 직접 입력 칸을 그 모델로 맞춘다(목록에 없으면 직접 입력). */
function show(inputs, model, choices) {
	const listed = choices.some((choice) => choice.id === model);
	inputs.pick.value = listed ? model : MODEL_CUSTOM;
	inputs.typed.value = listed ? "" : model;
	inputs.typed.hidden = listed;
}
/** 한 담당의 고르기 · 직접 입력 칸을 만든다. */
function rowInputs(row, choices, editable) {
	const pick = el("select", "ui-input");
	pick.name = `model-${row.agent}`;
	pick.setAttribute("aria-label", `${row.name} 모델`);
	for (const choice of choices) {
		const option = el("option", "", `${choice.label} — ${choice.note}`);
		option.value = choice.id;
		pick.append(option);
	}
	const custom = el("option", "", MODEL_CUSTOM_LABEL);
	custom.value = MODEL_CUSTOM;
	pick.append(custom);
	const typed = el("input", "ui-input");
	typed.name = `model-custom-${row.agent}`;
	typed.setAttribute("aria-label", `${row.name} 모델 직접 입력`);
	pick.disabled = !editable;
	typed.disabled = !editable;
	const inputs = {
		agent: row.agent,
		pick,
		typed
	};
	show(inputs, row.model, choices);
	pick.addEventListener("change", () => {
		typed.hidden = pick.value !== MODEL_CUSTOM;
		if (!typed.hidden) typed.focus();
	});
	return inputs;
}
/** 고른 값(직접 입력이면 앞뒤 공백을 뗀 칸 값). */
function picked(inputs) {
	return inputs.pick.value === "__custom__" ? inputs.typed.value.trim() : inputs.pick.value;
}
/** 담당 줄 표와 줄마다의 입력 칸. */
function rowsTable(reply) {
	const grid = table([
		"담당",
		"모델",
		"직접 입력",
		"기본값"
	], reply.roles.map((row) => [
		row.name,
		"",
		"",
		row.default
	]));
	const lines = grid.tBodies[0]?.rows ?? [];
	return {
		grid,
		inputs: reply.roles.map((row, index) => {
			const made = rowInputs(row, reply.choices, reply.editable);
			const tr = lines[index];
			if (tr) {
				tr.dataset.agent = row.agent;
				tr.cells[1]?.append(made.pick);
				tr.cells[2]?.append(made.typed);
			}
			return made;
		})
	};
}
/** 고르기 표를 그린다(`data-role="models"`). 저장 · 되돌리기는 이 자리 안에서 다시 그린다. */
function modelsTable(initial) {
	let current = initial;
	const wrap = el("div", "setup-extra-wrap");
	const result = el("div", "setup-extra-message");
	result.dataset.role = "models-result";
	result.setAttribute("aria-live", "polite");
	let inputs = [];
	const paint = () => {
		const made = rowsTable(current);
		inputs = made.inputs;
		wrap.replaceChildren(made.grid);
	};
	const notes = [el("p", "hint", MODELS_COST_NOTE), el("p", "hint", MODELS_APPLY_NOTE)];
	const node = card("담당 모델", wrap, ...notes);
	node.dataset.role = "models";
	node.classList.add("setup-extra");
	paint();
	if (!current.editable) {
		node.append(el("p", "hint", MODELS_REMOTE_NOTE));
		return node;
	}
	const save = button("저장", "accent");
	save.dataset.action = "save-models";
	const reset = button("기본값으로 되돌리기");
	reset.dataset.action = "reset-models";
	const send = (roles) => {
		save.disabled = true;
		reset.disabled = true;
		result.replaceChildren();
		call("POST", API.setupModels, { roles }).then((reply) => {
			const next = reply.ok ? modelsReplyOf(reply.body) : null;
			if (!next) return result.replaceChildren(errorLine(errorText(reply.status, reply.body)));
			current = next;
			paint();
			result.replaceChildren(el("p", "hint", MODELS_SAVED));
		}).catch(() => result.replaceChildren(errorLine(LOAD_FAILED))).finally(() => {
			save.disabled = false;
			reset.disabled = false;
		});
	};
	save.addEventListener("click", () => send(inputs.map((row) => ({
		agent: row.agent,
		model: picked(row)
	}))));
	reset.addEventListener("click", () => send(current.roles.map((row) => ({
		agent: row.agent,
		model: row.default
	}))));
	const actions = el("div", "setup-extra-actions");
	actions.append(save, reset);
	node.append(actions, result);
	return node;
}
/**
* 고르기 표를 읽어 `root` 안의 보기 줄(`[data-role="models"]`)을 표로 바꾼다. 읽지 못하거나 모양이 다르면 보기 줄
* 그대로다. 던지지 않는다.
*/
async function upgradeModels(root) {
	try {
		const reply = await call("GET", API.setupModels);
		const models = reply.ok ? modelsReplyOf(reply.body) : null;
		const line = root.querySelector("[data-role=\"models\"]");
		if (models && line) line.replaceWith(modelsTable(models));
	} catch {}
}
// src/setup/extras.ts
/** 시장 글자 → 화면 이름. */
var MARKET_LABELS = {
	kr: "국내",
	us: "미국"
};
/** 시장 줄의 안내. */
var MARKET_RESTART_NOTE = "시장은 켤 때 정합니다. 바꾸려면 --market 인자를 바꿔 서버를 다시 켜야 합니다.";
/** 담당 id → 화면 이름. */
var AGENT_LABELS = {
	scout: "뉴스 · 공시 · 시세",
	strategy: "전략",
	risk: "리스크",
	review: "복기",
	trader: "매매",
	leader: "팀장"
};
/** 모델 줄의 안내(고르기 표를 읽기 전 · 읽지 못했을 때의 보기 줄). */
var MODELS_NOTE = "지금 쓰는 담당 모델입니다. 고르기 표를 읽으면 이 자리에서 바꿀 수 있습니다.";
/** 모델 목록이 비었을 때(팀을 잇지 않은 서버). */
var MODELS_NONE = "팀이 이어져 있지 않아 쓰는 모델이 없습니다.";
/** 뜻 검색 주소 안내. */
var SEMANTIC_LOOPBACK_NOTE = "Ollama 주소는 같은 컴퓨터(127.0.0.1 · localhost)만 됩니다.";
/** 뜻 검색을 바꾸는 동안의 줄. */
var SEMANTIC_SAVING = "저장하는 중";
/** 뜻 검색을 켠 뒤의 안내(다시 만들기는 한가할 때 서버가 한다). */
var SEMANTIC_REBUILD_NOTE = "켜졌습니다. 기존 기록의 뜻 색인은 한가할 때 조금씩 다시 만듭니다.";
/** 뜻 검색 바꾸기 실패 까닭 → 화면 글. */
var SEMANTIC_ERRORS = {
	loopback_only: "Ollama 주소는 같은 컴퓨터(127.0.0.1 · localhost)만 됩니다. 주소를 확인하세요.",
	invalid_input: "주소와 모델 이름을 모두 적어 주세요.",
	local_only: "뜻 검색 설정은 이 컴퓨터에서만 바꿀 수 있습니다.",
	private_only: "같은 집 다른 기기는 사설망 주소(192.168.x.x · 10.x.x.x · 172.16~31.x.x · 이름.local)만 됩니다. 주소를 확인하세요."
};
/** 진단 상태 → 배지 글. */
var DIAG_LABELS = {
	ok: "정상",
	warn: "살펴볼 것",
	fail: "문제",
	info: "알림"
};
/** 진단 상태 → 배지 색. */
var DIAG_TONES = {
	ok: "success",
	warn: "warning",
	fail: "danger",
	info: "neutral"
};
/** 되살리기 명령(앞부분). */
var RESTORE_COMMAND = "price-inha restore";
/** 되살리기 안내. */
var RESTORE_NOTE = "되살리기는 이 화면에서 하지 않습니다. 서버를 끈 뒤 터미널에서 아래 명령으로 빈 폴더에 되살리고, 그 폴더로 다시 켭니다.";
/** 알림음 칸 이름. */
var SOUND_LABEL = "승인 알림음";
/** 알림음 바꾸기에 실패했을 때의 글. */
var SOUND_SAVE_FAILED = "알림음 설정을 바꾸지 못했습니다. 서버와 연결되지 않았거나 잠시 응답하지 않습니다. 다시 시도해 주세요.";
/** 되살린 폴더로 다시 켜는 명령. */
var RESTART_COMMAND = "price-inha run --home <되살린 폴더>";
/** 되살리기 명령 전체(파일 · 폴더 자리는 사람이 채운다). */
var RESTORE_USAGE = `${RESTORE_COMMAND} <백업 파일> --identity <age 비밀키 파일> --into <빈 폴더>`;
/** 객체면 그대로, 아니면 빈 객체. */
function record(value) {
	return value !== null && typeof value === "object" ? value : {};
}
/** 자리 하나(공통 카드)에 `data-role`을 단다. */
function block(role, title, ...body) {
	const node = card(title, ...body);
	node.dataset.role = role;
	node.classList.add("setup-extra");
	return node;
}
/** 읽기에 실패한 자리. */
function failedBlock(role, title) {
	return block(role, title, errorLine(LOAD_FAILED));
}
/** 설정 상태 몸통으로 시장 줄을 그린다(`data-role="market"`). */
function marketLine(status) {
	const market = String(record(status).market ?? "");
	const name = MARKET_LABELS[market] ?? market;
	return block("market", "시장", el("p", "setup-extra-value", `${name} (${market})`), el("p", "hint", MARKET_RESTART_NOTE));
}
/** 설정 상태의 `models`에서 담당 · 모델 글자가 있는 줄만. */
function modelRows(status) {
	const models = record(status).models;
	if (!Array.isArray(models)) return [];
	return models.map(record).filter((row) => typeof row.agent === "string" && typeof row.model === "string").map((row) => ({
		agent: row.agent,
		model: row.model
	}));
}
/** 설정 상태 몸통으로 담당 모델 줄을 그린다(`data-role="models"`). */
function modelsLine(status) {
	const rows = modelRows(status);
	if (rows.length === 0) return block("models", "담당 모델", emptyState(MODELS_NONE), el("p", "hint", MODELS_NOTE));
	const list = el("ul", "setup-extra-list");
	for (const row of rows) {
		const item = el("li", "setup-extra-row");
		item.dataset.agent = row.agent;
		item.append(el("span", "setup-extra-name", AGENT_LABELS[row.agent] ?? row.agent), el("code", "setup-extra-code", row.model));
		list.append(item);
	}
	return block("models", "담당 모델", list, el("p", "hint", MODELS_NOTE));
}
/** 진단 상태 글자면 그대로, 아니면 알림. */
function diagStatus(value) {
	return value === "ok" || value === "warn" || value === "fail" ? value : "info";
}
/** 진단 답의 항목들(모양이 맞지 않는 칸은 글자로 바꾼다). */
function diagnosisItems(reply) {
	const items = record(reply).items;
	if (!Array.isArray(items)) return [];
	return items.map(record).map((item) => ({
		key: String(item.key ?? ""),
		label: String(item.label ?? ""),
		status: diagStatus(item.status),
		detail: String(item.detail ?? ""),
		todo: typeof item.todo === "string" ? item.todo : null
	}));
}
/** 진단 답으로 진단 표를 그린다(`data-role="diagnosis"`). */
function diagnosisTable(reply) {
	const items = diagnosisItems(reply);
	const rows = items.map((item) => [
		item.label,
		"",
		item.detail,
		item.todo ?? "할 일 없음"
	]);
	const grid = table([
		"항목",
		"상태",
		"본 것",
		"할 일"
	], rows);
	const lines = grid.tBodies[0]?.rows ?? [];
	items.forEach((item, index) => {
		const tr = lines[index];
		if (!tr) return;
		tr.dataset.key = item.key;
		tr.cells[1]?.append(badge(DIAG_LABELS[item.status], DIAG_TONES[item.status]));
		tr.cells[3]?.setAttribute("data-role", "todo");
		tr.cells[2]?.classList.add("setup-diag-text");
		tr.cells[3]?.classList.add("setup-diag-text");
	});
	const wrap = el("div", "setup-extra-wrap");
	wrap.append(grid);
	return block("diagnosis", "진단", wrap, el("p", "hint", "실행 중인 서버가 아는 것만 봅니다. 더 자세히는 서버를 끈 뒤 price-inha doctor 를 실행합니다."));
}
/** 백업 설정 답(`{"repo", "recipient", "configured"}`)으로 되살리기 안내를 그린다(`data-role="restore"`). */
function restoreGuide(backup) {
	const on = record(backup).configured === true;
	const state = el("p", "setup-extra-value", on ? "백업 켜짐" : "백업 꺼짐");
	state.dataset.role = "backup-state";
	const usage = el("pre", "setup-extra-command");
	usage.append(el("code", "", RESTORE_USAGE));
	const restart = el("pre", "setup-extra-command");
	restart.append(el("code", "", RESTART_COMMAND));
	return block("restore", "되살리기", state, el("p", "hint", RESTORE_NOTE), usage, restart);
}
/** 답이 뜻 검색 설정 모양이면 그 값. */
function semanticOf(value) {
	const body = record(value);
	if (typeof body.enabled !== "boolean" || typeof body.base_url !== "string" || typeof body.model !== "string") return null;
	const lan = {
		private_network: body.private_network === true,
		outside: typeof body.outside === "string" ? body.outside : null
	};
	return {
		enabled: body.enabled,
		base_url: body.base_url,
		model: body.model,
		...lan
	};
}
/** 뜻 검색 바꾸기 실패 까닭 → 화면 글. */
function semanticError(body) {
	const reason = record(body).error;
	if (typeof reason !== "string" || reason === "") return LOAD_FAILED;
	return SEMANTIC_ERRORS[reason] ?? reason;
}
/** 칸 묶음 안의 입력 칸. */
function inputOf(holder) {
	return holder.querySelector("input");
}
/** 뜻 검색 카드를 그린다(`data-role="semantic"`). 단추를 누르면 바꾸기를 보낸다. */
function semanticCard(initial) {
	let current = initial;
	let busy = false;
	const state = el("p", "setup-extra-value");
	state.dataset.role = "semantic-state";
	const urlField = field("Ollama 주소", "semantic-url", "url");
	const modelField = field("임베딩 모델", "semantic-model");
	inputOf(urlField).value = current.base_url;
	inputOf(modelField).value = current.model;
	const toggle = button("", "accent");
	toggle.dataset.action = "semantic-toggle";
	const message = el("div", "setup-extra-message");
	message.setAttribute("aria-live", "polite");
	const lan = semanticLan(current.private_network, current.outside);
	const paint = () => {
		state.textContent = current.enabled ? "켜짐" : "꺼짐";
		toggle.textContent = current.enabled ? "끄기" : "켜기";
		lan.paint(current.private_network, current.outside);
	};
	toggle.addEventListener("click", () => {
		if (busy) return;
		busy = true;
		toggle.disabled = true;
		message.replaceChildren(loading(SEMANTIC_SAVING));
		const body = {
			enabled: !current.enabled,
			base_url: inputOf(urlField).value,
			model: inputOf(modelField).value
		};
		if (lan.checked()) Object.assign(body, { private_network: true });
		call("POST", API.setupSemantic, body).then((reply) => {
			const next = reply.ok ? semanticOf(reply.body) : null;
			if (!next) return message.replaceChildren(errorLine(semanticError(reply.body)));
			current = next;
			inputOf(urlField).value = next.base_url;
			inputOf(modelField).value = next.model;
			paint();
			message.replaceChildren(...next.enabled ? [el("p", "hint", SEMANTIC_REBUILD_NOTE)] : []);
		}).catch(() => message.replaceChildren(errorLine(LOAD_FAILED))).finally(() => {
			busy = false;
			toggle.disabled = false;
		});
	});
	paint();
	return block("semantic", "뜻 검색", state, urlField, modelField, el("p", "hint", SEMANTIC_LOOPBACK_NOTE), ...lan.nodes, toggle, message);
}
/** [`soundSetting`]의 자리와, 서버에서 읽은 값을 칸에 보이고 나면 끝나는 약속(던지지 않는다). */
function soundBlock() {
	const holder = field(SOUND_LABEL, "sound-on", "checkbox");
	holder.classList.add("setup-extra-check");
	const box = inputOf(holder);
	box.checked = true;
	box.disabled = true;
	const message = el("div", "setup-extra-message");
	message.setAttribute("aria-live", "polite");
	let current = true;
	box.addEventListener("change", () => {
		const wanted = box.checked;
		box.disabled = true;
		message.replaceChildren();
		saveSound(wanted).then((saved) => {
			if (saved === null) message.replaceChildren(errorLine(SOUND_SAVE_FAILED));
			else current = saved;
			box.checked = current;
			box.disabled = false;
		});
	});
	const ready = soundEnabled().then((on) => {
		current = on;
		box.checked = on;
		box.disabled = false;
	});
	return {
		node: block("sound", "알림음", holder, message),
		ready
	};
}
/** 경로 하나를 읽는다. 실패(상태 · 연결)면 `null`. */
async function read(url) {
	try {
		const reply = await call("GET", url);
		return reply.ok ? reply.body : null;
	} catch {
		return null;
	}
}
/** 자리를 새것으로 바꾼다. */
function swap(slot, next) {
	slot.replaceWith(next);
	return next;
}
/** 자리 하나에 보강 묶음 모두를 읽어 그린다. 읽기가 실패해도 던지지 않는다. */
async function mountSetupExtras(root) {
	const titles = {
		market: "시장",
		models: "담당 모델",
		semantic: "뜻 검색",
		diagnosis: "진단",
		restore: "되살리기"
	};
	const slots = {};
	for (const [role, title] of Object.entries(titles)) slots[role] = block(role, title, loading());
	const sound = soundBlock();
	const messengerSlot = el("div", "");
	const notifySlot = el("div", "");
	root.replaceChildren(...Object.values(slots), sound.node, messengerSlot, notifySlot);
	const messenger = mountMessenger(messengerSlot);
	const notify = mountNotify(notifySlot);
	const status = read(API.setup).then((body) => {
		swap(slots.market, body === null ? failedBlock("market", titles.market) : marketLine(body));
		swap(slots.models, body === null ? failedBlock("models", titles.models) : modelsLine(body));
	});
	const models = status.then(() => upgradeModels(root));
	const semantic = read(API.setupSemantic).then((body) => {
		const settings = semanticOf(body);
		swap(slots.semantic, settings === null ? failedBlock("semantic", titles.semantic) : semanticCard(settings));
	});
	const doctor = read(API.setupDoctor).then((body) => {
		swap(slots.diagnosis, body === null ? failedBlock("diagnosis", titles.diagnosis) : diagnosisTable(body));
	});
	const backup = read(API.setupBackup).then((body) => {
		const state = swap(slots.restore, restoreGuide(body ?? {})).querySelector("[data-role=\"backup-state\"]");
		if (body === null && state) state.textContent = "백업 상태를 읽지 못함";
	});
	await Promise.allSettled([
		status,
		semantic,
		doctor,
		backup,
		sound.ready,
		messenger,
		notify
	]);
	await models;
}
// src/setup/main.ts
mountMenu(document.getElementById("menu"), "setup");
/** 이 화면의 요소. 칸 · 단추 · 글 자리를 같은 모양(value · checked · disabled · hidden · textContent)으로 다룬다. */
function byId(id) {
	return document.getElementById(id);
}
var SESSION_PATH = API.session;
var WS_PATH = API.socket;
var SETUP_PATH = API.setup;
var SETUP_KEYS_PATH = API.setupKeys;
var SETUP_SYMBOLS_PATH = API.setupSymbols;
var SETUP_AI_PATH = API.setupAi;
var SETUP_BACKUP_PATH = API.setupBackup;
var ACCESS_KEY_PATH = API.accessKey;
var ACCESS_UNLOCK_PATH = API.accessUnlock;
var ACCESS_LOG_PATH = API.accessLog;
var SETUP_SLOTS_PATH = API.setupSlots;
var SLOT_FIELDS = {
	dart: "api_key",
	ecos: "api_key",
	fred: "api_key",
	backup_github_token: "token"
};
var SETUP_PROVIDERS_PATH = API.setupProviders;
var PROVIDER_LABELS = { naver_news: "네이버 뉴스 키 (뉴스 검색)" };
var ACCESS_KEY_SHOW_MS = 6e4;
var RETRY_MIN_MS = 1e3;
var RETRY_MAX_MS = 1e4;
var MAX_EVENTS = 20;
var $conn = byId("st-conn");
var $feed = byId("st-feed");
var $late = byId("st-late");
var $quotes = byId("quotes");
var $empty = byId("empty");
var $events = byId("events");
var $demo = byId("st-demo");
var $setupView = byId("setup-view");
var $mainView = byId("main-view");
var $missing = byId("setup-missing");
var $rejected = byId("rejected");
var $rejectedText = byId("rejected-text");
var $fixBrokerItem = byId("fix-broker-item");
var $closeSetup = byId("close-setup");
var $symbolsNow = byId("symbols-now");
var ITEM_LABELS = {
	broker: "증권사 키",
	ai: "AI 연결",
	symbols: "지켜볼 종목"
};
var REASONS = {
	"key_rejected": "키가 맞지 않습니다. 값을 확인하세요.",
	"retired_item": "예전의 한 칸짜리 자료 키는 받지 않습니다. 자료 키는 DART · ECOS · FRED · 네이버 칸으로 넣습니다.",
	"unreachable": "확인할 서버에 닿지 않습니다. 잠시 뒤에 다시 하세요.",
	"invalid_input": "입력이 잘못됐습니다. 빈 칸이나 너무 짧은 값이 없는지 확인하세요.",
	"too_many_attempts": "시도가 너무 많습니다. 잠시 뒤에 다시 하세요.",
	"fixed_by_argument": "종목을 실행 인자로 줘서 이 화면에서는 바꿀 수 없습니다.",
	"save_failed": "저장하지 못했습니다. 보관 폴더를 확인하세요.",
	"cli_not_installed": "그 경로에 Claude Code CLI가 없습니다. 실행 파일 경로를 확인하세요.",
	"cli_too_old": "Claude Code CLI 버전이 너무 낮습니다. 업데이트한 뒤 다시 확인하세요.",
	"cli_not_logged_in": "로그인돼 있지 않습니다. 터미널에서 claude auth login 을 실행한 뒤 다시 확인하세요.",
	"cli_not_subscription": "구독으로 로그인돼 있지 않습니다(API 키로 로그인돼 있습니다). 구독 계정으로 다시 로그인하세요.",
	"cli_unchecked": "확인하지 못했습니다(시간 초과 · 읽지 못함). 잠시 뒤에 다시 확인하세요.",
	"cli_work_dir_not_empty": "보관 폴더의 cli-work 폴더가 비어 있지 않아 CLI를 쓰지 않습니다. 안의 것을 옮기거나 지운 뒤 다시 확인하세요.",
	"shutting_down": "서버가 꺼지는 중입니다. 다시 켠 뒤에 고르세요."
};
var CLI_STATES = {
	"logged_in": "구독으로 로그인돼 있습니다.",
	"not_installed": "설치되지 않았습니다(그 경로에 실행 파일이 없습니다).",
	"too_old": "버전이 너무 낮습니다. 업데이트하세요.",
	"not_logged_in": "로그인돼 있지 않습니다. 터미널에서 claude auth login 을 실행하세요.",
	"not_subscription": "구독이 아닙니다(API 키로 로그인돼 있습니다).",
	"unchecked": "확인하지 못했습니다(시간 초과 · 읽지 못함).",
	"checking": "확인하는 중입니다…",
	"work_dir_not_empty": "보관 폴더의 cli-work 폴더가 비어 있지 않아 쓰지 않습니다(안의 것은 지우지 않았습니다)."
};
var lastSeq = null;
var retryMs = RETRY_MIN_MS;
var rows = {};
var lateSymbols = {};
var started = false;
var demoMode = false;
var manualSetup = false;
function setText(node, text) {
	node.textContent = String(text);
}
function setConn(text, kind) {
	setText($conn, text);
	$conn.className = kind;
	document.body.classList.toggle("offline", kind !== "ok");
}
function cell(tr, className) {
	var td = document.createElement("td");
	if (className) td.className = className;
	tr.appendChild(td);
	return td;
}
function rowFor(symbol) {
	if (rows[symbol]) return rows[symbol];
	var tr = document.createElement("tr");
	var row = {
		tr,
		last: null
	};
	setText(cell(tr), symbol);
	row.price = cell(tr, "num");
	row.currency = cell(tr);
	row.at = cell(tr);
	$quotes.appendChild(tr);
	rows[symbol] = row;
	return row;
}
function showQuotes(payload) {
	(payload && Array.isArray(payload.quotes) ? payload.quotes : []).forEach(function(quote) {
		if (!quote || typeof quote.symbol !== "string") return;
		var row = rowFor(quote.symbol);
		var price = Number(quote.price);
		var direction = "";
		if (row.last !== null && isFinite(price)) direction = price > row.last ? " up" : price < row.last ? " down" : "";
		if (isFinite(price)) row.last = price;
		row.price.className = "num" + direction;
		setText(row.price, quote.price);
		setText(row.currency, quote.currency || "");
		setText(row.at, timeText(quote.received_at));
	});
	$empty.hidden = Object.keys(rows).length > 0;
}
function showFeed(payload) {
	var state = payload ? payload.state : "";
	var text = "시세 상태를 알 수 없음";
	var kind = "warn";
	if (state === "no_broker_key") {
		text = "증권사 키가 없음 (시세를 받지 않습니다)";
		kind = "bad";
	} else if (state === "setup_required") {
		text = "필수 설정이 필요함 (시작하지 않았습니다)";
		kind = "bad";
	} else if (state === "request_rejected") {
		text = "증권사가 요청을 거절함 (종목을 확인하세요 · 간격을 늘려 다시 시도 중)";
		kind = "bad";
	} else if (state === "demo") {
		text = "시연 (가짜 시세)";
		kind = "warn";
	} else if (state === "live") {
		text = "증권사 시세";
		kind = "ok";
	} else if (state === "connecting") {
		text = "증권사에 연결 중 (아직 받은 시세가 없습니다)";
		kind = "warn";
	} else if (state === "broker_rejected") {
		text = "증권사가 키를 받지 않음 (키나 허용 IP를 확인하세요 · 간격을 늘려 다시 시도 중)";
		kind = "bad";
	} else if (state === "broker_unreachable") {
		text = "증권사에 닿지 않음 (간격을 늘려 다시 시도 중)";
		kind = "bad";
	}
	setText($feed, text);
	$feed.className = kind;
	$feed.hidden = false;
	$fixBrokerItem.hidden = state !== "broker_rejected";
}
function showLate(payload) {
	var list = payload && Array.isArray(payload.symbols) ? payload.symbols : [];
	lateSymbols = {};
	list.forEach(function(symbol) {
		lateSymbols[String(symbol)] = true;
	});
	Object.keys(rows).forEach(function(symbol) {
		rows[symbol].tr.classList.toggle("late", lateSymbols[symbol] === true);
	});
	var unknown = payload && payload.hours_unknown === true;
	var text = list.length > 0 ? "시세 늦음 (" + list.length + "종목)" : "시세 늦음";
	setText($late, unknown ? text + " · 장 시간을 모름" : text);
	$late.hidden = list.length === 0;
}
function render() {
	if (!started) manualSetup = false;
	var setup = !started || manualSetup;
	$setupView.hidden = !setup;
	$mainView.hidden = setup;
	$closeSetup.hidden = !started;
	$demo.hidden = !demoMode;
}
function showView(isStarted, demo) {
	started = isStarted;
	demoMode = demo;
	render();
}
function openSetup(focusId) {
	manualSetup = true;
	render();
	loadSetup();
	var field = focusId ? byId(focusId) : null;
	if (field) field.focus();
}
function closeSetup() {
	manualSetup = false;
	render();
}
function missingText(names) {
	var list = Array.isArray(names) ? names : [];
	if (list.length === 0) return "";
	return "아직 등록되지 않은 필수 항목: " + list.map(function(name) {
		return ITEM_LABELS[name] || String(name);
	}).join(", ");
}
function showSetupStatus(status) {
	if (!status || !Array.isArray(status.items)) return;
	var missing = [];
	status.items.forEach(function(item) {
		if (!item || typeof item.name !== "string") return;
		var badge = byId("reg-" + item.name);
		if (badge) {
			setText(badge, item.registered === true ? "등록됨" : "등록 안 됨");
			badge.className = "badge " + (item.registered === true ? "ok" : "warn");
		}
		if (item.required === true && item.registered !== true) missing.push(item.name);
	});
	setText($missing, missingText(missing));
	showAiStatus(status.ai);
	setText(byId("ai-limit-now"), formatAiLimit(readAiLimit(status)));
	var symbols = Array.isArray(status.symbols) ? status.symbols : [];
	var fixed = status.symbols_source === "argument";
	setText($symbolsNow, symbols.length === 0 ? "" : "지금 종목: " + symbols.join(", ") + (fixed ? " (실행 인자로 줘서 여기서는 바꿀 수 없습니다)" : status.symbols_source === "demo" ? " (시연용 기본 종목 — 종목을 등록하면 그것으로 바뀝니다)" : ""));
	showView(status.started === true, status.demo === true);
}
function loadSetup() {
	fetch(SETUP_PATH, {
		credentials: "same-origin",
		cache: "no-store"
	}).then(function(reply) {
		return reply.ok ? reply.json() : null;
	}).then(showSetupStatus).catch(function() {});
	loadSlots();
}
function loadSlots() {
	fetch(SETUP_SLOTS_PATH, {
		credentials: "same-origin",
		cache: "no-store"
	}).then(function(reply) {
		return reply.ok ? reply.json() : null;
	}).then(function(view) {
		if (!view || !Array.isArray(view.slots)) return;
		view.slots.forEach(function(slot) {
			var badge = slot && byId("reg-slot-" + slot.name);
			if (!badge) return;
			setText(badge, slot.registered === true ? "등록됨" : "등록 안 됨");
			badge.className = "badge " + (slot.registered === true ? "ok" : "warn");
		});
	}).catch(function() {});
	loadProviders();
}
function providerBlock(provider) {
	var name = String(provider.provider);
	var $block = document.createElement("div");
	$block.className = "slot";
	$block.id = "provider-" + name;
	var $title = document.createElement("h4");
	setText($title, (PROVIDER_LABELS[name] || name) + " ");
	var $badge = document.createElement("span");
	$badge.id = "reg-provider-" + name;
	$badge.className = "badge";
	$title.appendChild($badge);
	$block.appendChild($title);
	var fields = {};
	provider.fields.forEach(function(field) {
		var id = "f-provider-" + name + "-" + String(field);
		var $label = document.createElement("label");
		$label.htmlFor = id;
		setText($label, String(field));
		var $input = document.createElement("input");
		$input.id = id;
		applyKeyFieldRules($input);
		$block.appendChild($label);
		$block.appendChild($input);
		fields[String(field)] = id;
	});
	var $button = document.createElement("button");
	$button.id = "b-provider-" + name;
	$button.type = "button";
	setText($button, "등록");
	var $message = document.createElement("span");
	$message.id = "m-provider-" + name;
	$message.className = "msg";
	$message.setAttribute("aria-live", "polite");
	$block.appendChild($button);
	$block.appendChild($message);
	byId("provider-slots").appendChild($block);
	wireKey($button.id, $message.id, name, fields);
}
function loadProviders() {
	fetch(SETUP_PROVIDERS_PATH, {
		credentials: "same-origin",
		cache: "no-store"
	}).then(function(reply) {
		return reply.ok ? reply.json() : null;
	}).then(function(view) {
		if (!view || !Array.isArray(view.providers)) return;
		view.providers.forEach(function(provider) {
			if (!provider || typeof provider.provider !== "string" || !Array.isArray(provider.fields)) return;
			if (!byId("provider-" + provider.provider)) providerBlock(provider);
			var badge = byId("reg-provider-" + provider.provider);
			setText(badge, provider.registered === true ? "등록됨" : "등록 안 됨");
			badge.className = "badge " + (provider.registered === true ? "ok" : "warn");
		});
	}).catch(function() {});
}
function showSetup(payload) {
	if (!payload) return;
	if (payload.state === "started") showView(true, payload.demo === true);
	else if (payload.state === "required") {
		setText($missing, missingText(payload.missing));
		showView(false, payload.demo === true);
	}
	loadSetup();
}
function showRejected(payload) {
	var list = payload && Array.isArray(payload.symbols) ? payload.symbols : [];
	setText($rejectedText, list.length === 0 ? "" : "증권사가 거절한 종목 (빼고 받는 중 · 종목을 확인하세요): " + list.join(", "));
	$rejected.hidden = list.length === 0;
}
function register(path, body, $message, $button) {
	var accepted = false;
	$button.disabled = true;
	setText($message, "확인하는 중…");
	$message.className = "msg";
	return fetch(path, {
		method: "POST",
		credentials: "same-origin",
		cache: "no-store",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify(body)
	}).then(function(reply) {
		return reply.json().catch(function() {
			return null;
		}).then(function(data) {
			return {
				ok: reply.ok,
				status: reply.status,
				data
			};
		});
	}).then(function(result) {
		if (result.ok && result.data && result.data.registered === true) {
			accepted = true;
			setText($message, "등록됨");
			$message.className = "msg ok";
		} else {
			var text = REASONS[result.data && typeof result.data.error === "string" ? result.data.error : ""];
			if (!text) text = result.status === 401 ? "세션이 끝났습니다. 화면을 새로 고치세요." : result.status === 403 ? "이 컴퓨터에서 연 화면에서만 등록할 수 있습니다." : "등록하지 못했습니다.";
			setText($message, text);
			$message.className = "msg bad";
		}
		loadSetup();
	}).catch(function() {
		setText($message, "서버에 닿지 않습니다.");
		$message.className = "msg bad";
	}).then(function() {
		$button.disabled = false;
		return accepted;
	});
}
var aiPicked = false;
var cliEdited = {
	"f-cli-command": false,
	"f-cli-config-dir": false
};
function showAiPick(method) {
	byId("ai-pick-api").checked = method !== "cli";
	byId("ai-pick-cli").checked = method === "cli";
	byId("ai-api-box").hidden = method === "cli";
	byId("ai-cli-box").hidden = method !== "cli";
}
function showAiStatus(ai) {
	if (!ai || typeof ai !== "object") return;
	if (!aiPicked) showAiPick(ai.method === "cli" ? "cli" : "api_key");
	var $command = byId("f-cli-command");
	if (typeof ai.command === "string" && ($command.value === "" || ai.method === "cli") && !cliEdited["f-cli-command"] && document.activeElement !== $command) $command.value = ai.command;
	var $dir = byId("f-cli-config-dir");
	if (ai.method === "cli" && !cliEdited["f-cli-config-dir"] && document.activeElement !== $dir) $dir.value = typeof ai.config_dir === "string" ? ai.config_dir : "";
	var line = "";
	if (ai.method === "cli" && typeof ai.cli === "string") {
		line = "CLI 상태: " + (CLI_STATES[ai.cli] || "알 수 없음");
		if (ai.cli === "logged_in" && typeof ai.cli_version === "string") line += " (버전 " + ai.cli_version + (typeof ai.subscription === "string" ? ", 구독 " + ai.subscription : "") + ")";
	}
	setText(byId("ai-cli-state"), line);
	byId("ai-legacy").hidden = ai.legacy_auth_file !== true;
}
function chooseAi(body, $button) {
	var $message = byId("m-ai");
	if ($button) $button.disabled = true;
	setText($message, body.method === "cli" ? "확인하는 중…" : "저장하는 중…");
	$message.className = "msg";
	fetch(SETUP_AI_PATH, {
		method: "POST",
		credentials: "same-origin",
		cache: "no-store",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify(body)
	}).then(function(reply) {
		return reply.json().catch(function() {
			return null;
		}).then(function(data) {
			return {
				ok: reply.ok,
				status: reply.status,
				data
			};
		});
	}).then(function(result) {
		var data = result.data;
		if (result.ok && data && data.registered === true) {
			setText($message, "AI 연결이 갖춰졌습니다");
			$message.className = "msg ok";
		} else if (result.ok && data && data.registered === false) {
			setText($message, "API 키를 골랐습니다. 아래에 API 키를 등록하세요.");
			$message.className = "msg warn";
		} else {
			setText($message, REASONS[data && typeof data.error === "string" ? data.error : ""] || (result.status === 401 ? "세션이 끝났습니다. 화면을 새로 고치세요." : result.status === 403 ? "이 컴퓨터에서 연 화면에서만 고를 수 있습니다." : "고르지 못했습니다."));
			$message.className = "msg bad";
		}
		aiPicked = false;
		var why = data && typeof data.error === "string" ? data.error : "";
		if (result.ok || why.indexOf("cli_") === 0) {
			cliEdited["f-cli-command"] = false;
			cliEdited["f-cli-config-dir"] = false;
		}
		loadSetup();
	}).catch(function() {
		setText($message, "서버에 닿지 않습니다.");
		$message.className = "msg bad";
	}).then(function() {
		if ($button) $button.disabled = false;
	});
}
function wireAiLimit() {
	var $button = byId("b-ai-limit");
	var $message = byId("m-ai-limit");
	$button.addEventListener("click", function() {
		var parsed = parseAiLimit(byId("f-ai-limit").value);
		if (!parsed.ok) {
			setText($message, "1 이상 정수를 넣거나 비워 두세요.");
			$message.className = "msg bad";
			return;
		}
		var request = aiLimitRequest(parsed.limit);
		$button.disabled = true;
		setText($message, "저장하는 중…");
		$message.className = "msg";
		fetch(request.path, {
			method: "POST",
			credentials: "same-origin",
			cache: "no-store",
			headers: { "Content-Type": "application/json" },
			body: JSON.stringify(request.body)
		}).then(function(reply) {
			return reply.json().catch(function() {
				return null;
			}).then(function(data) {
				if (reply.ok) {
					setText(byId("ai-limit-now"), formatAiLimit(readAiLimit(data)));
					setText($message, "저장했습니다");
					$message.className = "msg ok";
					byId("f-ai-limit").value = "";
				} else {
					setText($message, reply.status === 403 ? "이 컴퓨터에서 연 화면에서만 바꿀 수 있습니다." : reply.status === 400 ? "1 이상 정수를 넣거나 비워 두세요." : "저장하지 못했습니다.");
					$message.className = "msg bad";
				}
			});
		}).catch(function() {
			setText($message, "서버에 닿지 않습니다.");
			$message.className = "msg bad";
		}).then(function() {
			$button.disabled = false;
		});
	});
}
function wireAi() {
	byId("ai-pick-api").addEventListener("change", function() {
		aiPicked = true;
		showAiPick("api_key");
		chooseAi({ "method": "api_key" }, null);
	});
	byId("ai-pick-cli").addEventListener("change", function() {
		aiPicked = true;
		showAiPick("cli");
	});
	Object.keys(cliEdited).forEach(function(id) {
		byId(id).addEventListener("input", function() {
			cliEdited[id] = true;
		});
	});
	var $button = byId("b-ai-cli");
	$button.addEventListener("click", function() {
		var dir = byId("f-cli-config-dir").value.trim();
		chooseAi({
			"method": "cli",
			"command": byId("f-cli-command").value.trim(),
			"config_dir": dir === "" ? null : dir
		}, $button);
	});
}
function wireKey(buttonId, messageId, item, fields) {
	var $button = byId(buttonId);
	var $message = byId(messageId);
	$button.addEventListener("click", function() {
		var inputs = {};
		Object.keys(fields).forEach(function(name) {
			inputs[name] = byId(fields[name]);
		});
		submitKeyFields(inputs, function(raw) {
			var values = {};
			Object.keys(raw).forEach(function(name) {
				values[name] = raw[name].trim();
			});
			return register(SETUP_KEYS_PATH, {
				item,
				fields: values
			}, $message, $button);
		});
	});
}
function showBackup(view) {
	var $repo = byId("f-backup-repo");
	var $recipient = byId("f-backup-recipient");
	if (view && view.configured) {
		$repo.value = view.repo || "";
		$recipient.value = view.recipient || "";
	}
	setText(byId("reg-backup"), view && view.configured ? "설정됨" : "설정 안 됨");
}
function loadBackup() {
	fetch(SETUP_BACKUP_PATH, { credentials: "same-origin" }).then(function(reply) {
		return reply.ok ? reply.json() : null;
	}).then(function(view) {
		if (view) showBackup(view);
	}).catch(function() {});
}
function wireBackup() {
	var $button = byId("b-backup");
	var $message = byId("m-backup");
	$button.addEventListener("click", function() {
		var body = {
			repo: byId("f-backup-repo").value.trim(),
			recipient: byId("f-backup-recipient").value.trim()
		};
		$button.disabled = true;
		fetch(SETUP_BACKUP_PATH, {
			method: "POST",
			credentials: "same-origin",
			headers: { "Content-Type": "application/json" },
			body: JSON.stringify(body)
		}).then(function(reply) {
			return reply.json().catch(function() {
				return null;
			}).then(function(data) {
				if (reply.ok && data) {
					showBackup(data);
					setText($message, "저장했습니다");
				} else setText($message, "저장소 이름(소유자/이름)이나 공개키(age1…)를 확인하세요");
			});
		}).catch(function() {
			setText($message, "서버에 닿지 못했습니다");
		}).then(function() {
			$button.disabled = false;
		});
	});
	loadBackup();
}
function postJson(path, body) {
	return fetch(path, {
		method: "POST",
		credentials: "same-origin",
		cache: "no-store",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify(body)
	});
}
function clearList(node) {
	while (node.firstChild) node.removeChild(node.firstChild);
}
function timeText(ms) {
	return new Date(ms).toLocaleString();
}
function showAccess(view) {
	var $locked = byId("access-locked");
	var $log = byId("access-log");
	clearList($locked);
	clearList($log);
	(view.locked || []).forEach(function(device) {
		var $item = document.createElement("li");
		var $unlock = document.createElement("button");
		$unlock.type = "button";
		$unlock.textContent = "풀기";
		$unlock.addEventListener("click", function() {
			$unlock.disabled = true;
			postJson(ACCESS_UNLOCK_PATH, { device: device.device }).then(function() {
				loadAccess();
			}).catch(function() {
				$unlock.disabled = false;
			});
		});
		$item.appendChild(document.createTextNode(device.device + " · " + timeText(device.until) + "까지 "));
		$item.appendChild($unlock);
		$locked.appendChild($item);
	});
	if (!view.locked || view.locked.length === 0) {
		var $none = document.createElement("li");
		$none.textContent = "없음";
		$locked.appendChild($none);
	}
	(view.entries || []).forEach(function(entry) {
		var $item = document.createElement("li");
		$item.textContent = timeText(entry.at) + " · " + entry.origin + " · " + entry.action;
		$log.appendChild($item);
	});
}
function loadAccess() {
	fetch(ACCESS_LOG_PATH, {
		credentials: "same-origin",
		cache: "no-store"
	}).then(function(reply) {
		return reply.ok ? reply.json() : null;
	}).then(function(view) {
		if (view) showAccess(view);
	}).catch(function() {});
}
function wireAccess() {
	var $button = byId("b-access-key");
	var $message = byId("m-access-key");
	var $once = byId("access-key-once");
	var $value = byId("access-key-value");
	var hideTimer = null;
	function hideAccessKey() {
		if (hideTimer !== null) {
			clearTimeout(hideTimer);
			hideTimer = null;
		}
		$once.hidden = true;
		setText($value, "");
	}
	$button.addEventListener("click", function() {
		$button.disabled = true;
		hideAccessKey();
		postJson(ACCESS_KEY_PATH, {}).then(function(reply) {
			return reply.json().catch(function() {
				return null;
			}).then(function(data) {
				if (reply.ok && data && data.access_key) {
					setText($value, data.access_key);
					$once.hidden = false;
					hideTimer = setTimeout(hideAccessKey, ACCESS_KEY_SHOW_MS);
					setText($message, "만들었습니다 (1분 뒤 화면에서 지웁니다)");
					loadAccess();
				} else if (reply.status === 403) setText($message, "이 컴퓨터에서만 만들 수 있습니다");
				else setText($message, "만들지 못했습니다");
			});
		}).catch(function() {
			setText($message, "서버에 닿지 못했습니다");
		}).then(function() {
			$button.disabled = false;
		});
	});
	byId("b-access-log").addEventListener("click", loadAccess);
	loadAccess();
}
function wireSetup() {
	wireBackup();
	wireAccess();
	wireKey("b-broker", "m-broker", "broker", {
		client_id: "f-client-id",
		client_secret: "f-client-secret"
	});
	wireKey("b-ai-key", "m-ai-key", "ai_api_key", { api_key: "f-api-key" });
	wireAi();
	wireAiLimit();
	Object.keys(SLOT_FIELDS).forEach(function(item) {
		var fields = {};
		fields[SLOT_FIELDS[item]] = "f-slot-" + item;
		wireKey("b-slot-" + item, "m-slot-" + item, item, fields);
	});
	byId("open-setup").addEventListener("click", function() {
		openSetup(null);
	});
	byId("close-setup").addEventListener("click", closeSetup);
	byId("fix-symbols").addEventListener("click", function() {
		openSetup("f-symbols");
	});
	byId("fix-broker").addEventListener("click", function() {
		openSetup("f-client-id");
	});
	var $button = byId("b-symbols");
	var $message = byId("m-symbols");
	var $input = byId("f-symbols");
	$button.addEventListener("click", function() {
		register(SETUP_SYMBOLS_PATH, { symbols: $input.value.split(",").map(function(code) {
			return code.trim();
		}).filter(function(code) {
			return code.length > 0;
		}) }, $message, $button).then(function(accepted) {
			if (accepted) $input.value = "";
		});
	});
}
function showEvent(frame) {
	var li = document.createElement("li");
	setText(li, "#" + frame.seq + " · " + timeText(frame.at) + " · " + frame.kind);
	$events.insertBefore(li, $events.firstChild);
	while ($events.childNodes.length > MAX_EVENTS) $events.removeChild($events.lastChild);
}
function onFrame(text) {
	var frame;
	try {
		frame = JSON.parse(text);
	} catch {
		return;
	}
	if (!frame || typeof frame.kind !== "string") return;
	if (typeof frame.seq === "number") {
		if (lastSeq !== null && frame.seq <= lastSeq) return;
		lastSeq = frame.seq;
		showEvent(frame);
		return;
	}
	if (frame.kind === "quotes") showQuotes(frame.payload);
	else if (frame.kind === "feed_status") showFeed(frame.payload);
	else if (frame.kind === "quotes_stale") showLate(frame.payload);
	else if (frame.kind === "setup") showSetup(frame.payload);
	else if (frame.kind === "symbols_rejected") showRejected(frame.payload);
}
function retryLater() {
	var wait = retryMs;
	retryMs = Math.min(retryMs * 2, RETRY_MAX_MS);
	setTimeout(start, wait);
}
function openSocket() {
	var url = (location.protocol === "https:" ? "wss://" : "ws://") + location.host + WS_PATH;
	if (lastSeq !== null) url += "?last_seq=" + encodeURIComponent(String(lastSeq));
	var socket;
	try {
		socket = new WebSocket(url);
	} catch {
		retryLater();
		return;
	}
	var closed = false;
	function onClose() {
		if (closed) return;
		closed = true;
		setConn("연결 끊김 (마지막으로 받은 상태 유지) · 다시 잇는 중", "bad");
		retryLater();
	}
	socket.onopen = function() {
		retryMs = RETRY_MIN_MS;
		setConn("연결됨", "ok");
	};
	socket.onmessage = function(event) {
		if (typeof event.data === "string") onFrame(event.data);
	};
	socket.onclose = onClose;
	socket.onerror = function() {
		try {
			socket.close();
		} catch {}
		onClose();
	};
}
var extrasMounted = false;
function mountExtrasOnce() {
	var root = document.getElementById("setup-extras");
	if (extrasMounted || !root) return;
	extrasMounted = true;
	mountSetupExtras(root);
}
function start() {
	fetch(SESSION_PATH, {
		method: "POST",
		credentials: "same-origin",
		cache: "no-store"
	}).then(function(reply) {
		if (!reply.ok) throw new Error("session");
		loadSetup();
		mountExtrasOnce();
		openSocket();
	}).catch(function() {
		setConn("연결 끊김 (마지막으로 받은 상태 유지) · 다시 잇는 중", "bad");
		retryLater();
	});
}
wireSetup();
start();
