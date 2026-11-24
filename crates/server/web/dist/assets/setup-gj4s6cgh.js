import { n as API, t as mountMenu } from "./menu-mixz7f78.js";
import { n as submitKeyFields, t as applyKeyFieldRules } from "./field-e6l8inco.js";
import { a as errorLine, c as table, d as el, i as emptyState, l as LOAD_FAILED, n as button, o as field, r as card, s as loading, t as badge, u as call } from "./parts-clyrk8h1.js";
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
/** 모델 줄의 안내(보기만). */
var MODELS_NOTE = "담당 모델은 지금 보기만 합니다. 바꾸는 길은 아직 없습니다.";
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
	local_only: "뜻 검색 설정은 이 컴퓨터에서만 바꿀 수 있습니다."
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
	return {
		enabled: body.enabled,
		base_url: body.base_url,
		model: body.model
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
	const paint = () => {
		state.textContent = current.enabled ? "켜짐" : "꺼짐";
		toggle.textContent = current.enabled ? "끄기" : "켜기";
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
	return block("semantic", "뜻 검색", state, urlField, modelField, el("p", "hint", SEMANTIC_LOOPBACK_NOTE), toggle, message);
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
	root.replaceChildren(...Object.values(slots), sound.node);
	const status = read(API.setup).then((body) => {
		swap(slots.market, body === null ? failedBlock("market", titles.market) : marketLine(body));
		swap(slots.models, body === null ? failedBlock("models", titles.models) : modelsLine(body));
	});
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
		sound.ready
	]);
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
	setText($symbolsNow, symbols.length === 0 ? "" : "지금 종목: " + symbols.join(", ") + (fixed ? " (실행 인자로 줘서 여기서는 바꿀 수 없습니다)" : ""));
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
	} catch (error) {
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
	} catch (error) {
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
		} catch (error) {}
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
