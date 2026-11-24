import { n as API, t as mountMenu } from "./menu-myat6qpc.js";
import { n as submitKeyFields, t as applyKeyFieldRules } from "./field-e6l8inco.js";
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
function start() {
	fetch(SESSION_PATH, {
		method: "POST",
		credentials: "same-origin",
		cache: "no-store"
	}).then(function(reply) {
		if (!reply.ok) throw new Error("session");
		loadSetup();
		openSocket();
	}).catch(function() {
		setConn("연결 끊김 (마지막으로 받은 상태 유지) · 다시 잇는 중", "bad");
		retryLater();
	});
}
wireSetup();
start();
