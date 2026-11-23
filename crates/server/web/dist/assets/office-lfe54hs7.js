import { i as decidePath, r as approvalPath, t as API } from "./paths-m4cykb5a.js";
var PENDING = "pending";
var CODE_LENGTH = 6;
function parse(payload) {
	if (typeof payload !== "object" || payload === null) return null;
	const p = payload;
	if (typeof p.number !== "string" || p.number === "") return null;
	if (typeof p.summary !== "string" || typeof p.short_code !== "string" || typeof p.status !== "string") return null;
	return {
		number: p.number,
		summary: p.summary,
		short_code: p.short_code,
		status: p.status
	};
}
function findCard(root, number) {
	for (const card of root.querySelectorAll("[data-approval]")) if (card.dataset.approval === number) return card;
	return null;
}
function part(card, name) {
	return card.querySelector(`[data-part="${name}"]`);
}
function textPart(tag, name, className = "") {
	const element = document.createElement(tag);
	element.dataset.part = name;
	if (className) element.className = className;
	return element;
}
function verbButton(root, number, verb, fetcher) {
	const button = document.createElement("button");
	button.type = "button";
	button.dataset.verb = verb;
	button.textContent = verb === "approve" ? "승인" : "거절";
	button.addEventListener("click", () => {
		decideApproval(root, number, verb, fetcher);
	});
	return button;
}
function newCard(root, number, fetcher) {
	const card = document.createElement("article");
	card.className = "approval-card";
	card.dataset.approval = number;
	const title = document.createElement("h2");
	title.textContent = "승인이 필요해요";
	const code = document.createElement("p");
	code.className = "muted";
	code.append("내용 지문 앞 6자 ", textPart("strong", "code"));
	const row = document.createElement("div");
	row.className = "row";
	row.append(verbButton(root, number, "approve", fetcher), verbButton(root, number, "reject", fetcher));
	const result = textPart("p", "result", "muted");
	result.setAttribute("role", "status");
	card.append(title, textPart("p", "summary"), code, row, result);
	root.append(card);
	return card;
}
/**
* 이벤트 한 건을 카드에 반영한다. `pending`이면 그 번호의 카드를 그리거나 고치고(같은 번호는 한 장),
* 아니면 그 카드를 닫는다(지운다). 모양이 맞지 않는 몸통은 무시한다.
*/
function applyApproval(root, payload, fetcher) {
	const approval = parse(payload);
	if (!approval) return;
	const existing = findCard(root, approval.number);
	if (approval.status !== PENDING) {
		existing?.remove();
		return;
	}
	const card = existing ?? newCard(root, approval.number, fetcher);
	const summary = part(card, "summary");
	const code = part(card, "code");
	if (summary) summary.textContent = approval.summary;
	if (code) code.textContent = approval.short_code.slice(0, CODE_LENGTH);
}
function say(card, text) {
	const result = card ? part(card, "result") : null;
	if (result) result.textContent = text;
}
function lockVerbs(card, locked) {
	if (!card) return;
	for (const button of card.querySelectorAll("button[data-verb]")) button.disabled = locked;
}
/**
* 승인 · 거절을 보낸다. 보내는 동안 그 카드의 두 단추를 막고(같은 카드에서 두 번 눌러도 한 번만 나간다),
* 실패하면 다시 켜며, 보냈으면 막힌 채 둔다(이미 결정됨).
*
* 폰과 같이 GET /api/approvals/{id}로 전체 지문을 받아, 그 앞 6자가 카드에 보인
* 지문 앞 6자와 같을 때만 POST /api/approvals/{id}/{verb} `{"fingerprint": 전체 지문}`을 보낸다.
*/
async function decideApproval(root, number, verb, fetcher) {
	const card = findCard(root, number);
	if (card?.dataset.deciding === "1") return "failed";
	if (card) card.dataset.deciding = "1";
	lockVerbs(card, true);
	const outcome = await send$1(card, number, verb, fetcher);
	if (card) delete card.dataset.deciding;
	if (outcome !== "sent") lockVerbs(card, false);
	return outcome;
}
async function send$1(card, number, verb, fetcher) {
	const shown = card ? part(card, "code")?.textContent ?? "" : "";
	try {
		const reply = await fetcher(approvalPath(number), {
			credentials: "same-origin",
			cache: "no-store"
		});
		if (!reply.ok) {
			say(card, "승인 내용을 받지 못했어요: " + reply.status);
			return "failed";
		}
		const body = await reply.json();
		const fingerprint = typeof body.fingerprint === "string" ? body.fingerprint : "";
		if (shown.length !== CODE_LENGTH || fingerprint.slice(0, CODE_LENGTH) !== shown) {
			say(card, "지문이 카드와 달라요. 보내지 않았어요.");
			return "mismatch";
		}
		const sent = await fetcher(decidePath(number, verb), {
			method: "POST",
			credentials: "same-origin",
			headers: { "Content-Type": "application/json" },
			body: JSON.stringify({ fingerprint })
		});
		if (!sent.ok) {
			say(card, "처리하지 못했어요: " + sent.status);
			return "failed";
		}
		say(card, verb === "approve" ? "승인했어요." : "거절했어요.");
		return "sent";
	} catch (error) {
		say(card, "처리하지 못했어요: " + (error instanceof Error ? error.message : String(error)));
		return "failed";
	}
}
/**
* `lines`가 `keep`줄을 넘으면 앞(오래된 쪽)에서 넘친 만큼 버린다(`lines`를 그 자리에서 고친다).
* 버린 응답 줄의 응답이 끝난 것(`complete` · `failed`)이면 `responses`에서도 지운다.
*/
function trimLines(lines, responses, keep = 200) {
	if (lines.length <= keep) return;
	for (const line of lines.splice(0, lines.length - keep)) {
		if (line.kind !== "resp") continue;
		const status = responses.get(line.requestId)?.status;
		if (status === "complete" || status === "failed") responses.delete(line.requestId);
	}
}
// \0vite/preload-helper.js
var scriptRel = /* @__PURE__ */ (function detectScriptRel() {
	const relList = typeof document !== "undefined" && document.createElement("link").relList;
	return relList && relList.supports && relList.supports("modulepreload") ? "modulepreload" : "preload";
})();
var assetsURL = function(dep) {
	return "/" + dep;
};
var seen = {};
var isCssPreloadUrl = function isCssPreloadUrl(url) {
	return url.pathname.endsWith(".css");
};
var preloadOnce = function preloadOnce(seen, href, preload) {
	if (href in seen) return seen[href];
	const promise = preload();
	if (!promise) {
		seen[href] = void 0;
		return;
	}
	const preloadPromise = promise.then(() => {
		seen[href] = void 0;
	}, (err) => {
		seen[href] = void 0;
		throw err;
	});
	seen[href] = preloadPromise;
	return preloadPromise;
};
var __vitePreload = function preload(baseModule, deps, importerUrl) {
	let promise = Promise.resolve();
	if (deps && deps.length > 0) {
		let preloadedHrefs;
		const cspNonceMeta = document.querySelector("meta[property=csp-nonce]");
		const cspNonce = cspNonceMeta?.nonce || cspNonceMeta?.getAttribute("nonce");
		function allSettled(promises) {
			return Promise.all(promises.map((p) => Promise.resolve(p).then((value) => ({
				status: "fulfilled",
				value
			}), (reason) => ({
				status: "rejected",
				reason
			}))));
		}
		function importMetaResolve(specifier) {
			if (import.meta.resolve) return new URL(import.meta.resolve(specifier));
			return new URL(
				specifier,
				/** #__KEEP__ */
				import.meta.url
			);
		}
		promise = allSettled(deps.map((depString) => {
			depString = assetsURL(depString, importerUrl);
			const dep = importMetaResolve(depString);
			const isCss = isCssPreloadUrl(dep);
			return preloadOnce(seen, dep.href, () => {
				if (preloadedHrefs === void 0) {
					preloadedHrefs = {
						all: /* @__PURE__ */ new Set(),
						styles: /* @__PURE__ */ new Set()
					};
					const links = document.getElementsByTagName("link");
					for (let i = links.length - 1; i >= 0; i--) {
						const link = links[i];
						preloadedHrefs.all.add(link.href);
						if (link.rel === "stylesheet") preloadedHrefs.styles.add(link.href);
					}
				}
				if ((isCss ? preloadedHrefs.styles : preloadedHrefs.all).has(dep.href)) return;
				const link = document.createElement("link");
				link.rel = isCss ? "stylesheet" : scriptRel;
				if (!isCss) link.as = "script";
				link.crossOrigin = "";
				link.href = dep.href;
				if (cspNonce) link.setAttribute("nonce", cspNonce);
				document.head.appendChild(link);
				if (isCss) return new Promise((res, rej) => {
					const handleSettle = (e) => {
						link.removeEventListener("load", handleSettle);
						link.removeEventListener("error", handleSettle);
						if (e.type === "error") rej(/* @__PURE__ */ new Error(`Unable to preload CSS for ${dep}`));
						else res(e);
					};
					link.addEventListener("load", handleSettle);
					link.addEventListener("error", handleSettle);
				});
			});
		}).filter((p) => p !== void 0));
	}
	function handlePreloadError(err) {
		const e = new Event("vite:preloadError", { cancelable: true });
		e.payload = err;
		window.dispatchEvent(e);
		if (!e.defaultPrevented) throw err;
	}
	return promise.then((res) => {
		for (const item of res || []) {
			if (item.status !== "rejected") continue;
			handlePreloadError(item.reason);
		}
		return baseModule().catch(handlePreloadError);
	});
};
// src/office/main.ts
var SESSION_PATH = API.session;
var SNAPSHOT_PATH = API.office;
var CHAT_PATH = API.officeChat;
var WS_PATH = API.socket;
var MAP_PATH = "./office-map.json";
var RETRY_MIN_MS = 1e3;
var RETRY_MAX_MS = 1e4;
var FOLLOW_PX = 80;
var AGENT_IDS = [
	"news",
	"strategy",
	"risk",
	"review",
	"trader",
	"leader"
];
var PLACE_LABEL = {
	bed: "자기 방 침대",
	desk: "사무실 자기 자리",
	meeting: "논의 탁자"
};
var WORK_LABEL = {
	idle: "",
	queued: "대기",
	thinking: "생각 중",
	failed: "실패"
};
var ROOM_TEAM = "team";
var $ = (id) => document.getElementById(id);
var state = {
	map: null,
	names: {},
	agents: new Map(AGENT_IDS.map((id) => [id, {
		place: "bed",
		work: "idle"
	}])),
	responses: /* @__PURE__ */ new Map(),
	lines: /* @__PURE__ */ new Map(),
	conversation: ROOM_TEAM,
	lastSeq: null,
	connected: false,
	view: null
};
function renderLight() {
	const list = $("light-list");
	list.replaceChildren();
	for (const id of AGENT_IDS) {
		const a = state.agents.get(id);
		const li = document.createElement("li");
		const button = document.createElement("button");
		button.type = "button";
		const name = document.createElement("strong");
		const work = WORK_LABEL[a.work] ? ` · ${WORK_LABEL[a.work]}` : "";
		name.textContent = `${state.names[id] ?? id}${work}`;
		const where = document.createElement("span");
		where.className = "where";
		where.textContent = PLACE_LABEL[a.place] ?? a.place;
		button.append(name, where);
		button.addEventListener("click", () => selectConversation(id));
		li.append(button);
		list.append(li);
	}
}
function showLight(reason) {
	if (state.view) {
		try {
			state.view.dispose();
		} catch {}
		state.view = null;
	}
	$("stage").hidden = true;
	$("light").hidden = false;
	const toggle = $("view-toggle");
	toggle.hidden = !reason || reason === "lost" ? true : false;
	renderLight();
}
async function show3d() {
	const { webglAvailable, createScene } = await __vitePreload(async () => {
		const { webglAvailable, createScene } = await import("./scene3d-g9kmeb28.js");
		return {
			webglAvailable,
			createScene
		};
	}, []);
	if (!webglAvailable()) throw new Error("webgl");
	$("stage").hidden = false;
	$("light").hidden = true;
	const scene = createScene({
		host: $("canvas-host"),
		labels: $("labels"),
		map: state.map,
		names: state.names,
		onPick: (id) => selectConversation(id),
		onLost: () => showLight("lost")
	});
	state.view = scene;
	for (const [id, view] of state.agents) scene.setAgent(id, view, { snap: true });
}
function applyAgent(view, snap) {
	if (!view || !state.agents.has(view.agent)) return;
	const next = {
		place: view.place,
		work: view.work
	};
	state.agents.set(view.agent, next);
	if (state.view) state.view.setAgent(view.agent, next, { snap });
	if (!$("light").hidden) renderLight();
	renderTabs();
}
function convKeyOf(resp) {
	return resp.room === ROOM_TEAM ? ROOM_TEAM : resp.agent;
}
function linesOf(key) {
	if (!state.lines.has(key)) state.lines.set(key, []);
	return state.lines.get(key);
}
/** 응답 객체는 requestId별로 updatedAt이 크거나 같은 것만 받아 덮어쓴다(늦게 온 옛 상태는 버린다). */
function applyChat(resp) {
	if (!resp || typeof resp.requestId !== "string") return;
	const old = state.responses.get(resp.requestId);
	if (old && Number(old.updatedAt) > Number(resp.updatedAt)) return;
	state.responses.set(resp.requestId, resp);
	if (!old) {
		const lines = linesOf(convKeyOf(resp));
		const at = lines.findIndex((l) => l.kind === "me" && l.id === resp.sourceMessageId);
		const entry = {
			kind: "resp",
			requestId: resp.requestId
		};
		if (at >= 0) {
			let end = at + 1;
			for (;;) {
				const next = lines[end];
				if (end >= lines.length || next.kind !== "resp" || state.responses.get(next.requestId)?.sourceMessageId !== resp.sourceMessageId) break;
				end++;
			}
			lines.splice(end, 0, entry);
		} else lines.push(entry);
		trimLines(lines, state.responses);
	}
	if (convKeyOf(resp) === state.conversation) renderLog(false);
}
function newId() {
	if (window.crypto?.randomUUID) return window.crypto.randomUUID();
	return `m-${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
}
function nameOf(id) {
	return state.names[id] ?? id;
}
function renderTabs() {
	const tabs = $("chat-tabs");
	tabs.replaceChildren();
	for (const key of [ROOM_TEAM, ...AGENT_IDS]) {
		const b = document.createElement("button");
		b.type = "button";
		b.setAttribute("role", "tab");
		b.setAttribute("aria-selected", String(key === state.conversation));
		const glyph = key === ROOM_TEAM ? "" : {
			queued: " ⏳",
			thinking: " 💭",
			failed: " ❗"
		}[state.agents.get(key)?.work ?? ""] ?? "";
		b.textContent = (key === ROOM_TEAM ? "팀 방" : nameOf(key)) + glyph;
		b.addEventListener("click", () => selectConversation(key));
		tabs.append(b);
	}
	$("chat-to-wrap").hidden = state.conversation !== ROOM_TEAM;
}
function selectConversation(key) {
	state.conversation = key;
	renderTabs();
	renderLog(true);
}
function lineElement(line) {
	const div = document.createElement("div");
	if (line.kind === "me") {
		div.className = "msg me";
		const who = document.createElement("span");
		who.className = "who";
		who.textContent = line.room === ROOM_TEAM ? `나 → ${nameOf(line.agent)}` : "나";
		div.append(who, document.createTextNode(line.text));
		if (line.failed) {
			div.classList.add("failed");
			const retry = document.createElement("button");
			retry.type = "button";
			retry.className = "retry";
			retry.textContent = "다시 보내기";
			retry.addEventListener("click", () => {
				send(line.room, line.agent, line.text, line);
			});
			div.append(document.createTextNode("\n보내지 못함"), retry);
		}
		return div;
	}
	const resp = state.responses.get(line.requestId);
	if (!resp) return null;
	const who = document.createElement("span");
	who.className = "who";
	who.textContent = nameOf(resp.agent);
	if (resp.status === "complete") {
		div.className = "msg agent";
		div.append(who, document.createTextNode(resp.content ?? ""));
	} else if (resp.status === "failed") {
		div.className = "msg status failed";
		div.textContent = `${nameOf(resp.agent)}: 실패 (${resp.error ?? "까닭 모름"})`;
	} else {
		div.className = "msg status";
		div.textContent = `${nameOf(resp.agent)}: ${WORK_LABEL[resp.status] ?? resp.status} …`;
	}
	return div;
}
function renderLog(forceBottom) {
	const log = $("chat-log");
	const follow = forceBottom || log.scrollHeight - log.scrollTop - log.clientHeight <= FOLLOW_PX;
	log.replaceChildren();
	for (const line of linesOf(state.conversation)) {
		const el = lineElement(line);
		if (el) log.append(el);
	}
	if (follow) log.scrollTop = log.scrollHeight;
}
async function send(room, agent, text, existing) {
	const line = existing ?? {
		kind: "me",
		id: newId(),
		room,
		agent,
		text
	};
	line.failed = false;
	if (!existing) {
		const lines = linesOf(room === ROOM_TEAM ? ROOM_TEAM : agent);
		lines.push(line);
		trimLines(lines, state.responses);
	}
	renderLog(true);
	try {
		const reply = await fetch(CHAT_PATH, {
			method: "POST",
			credentials: "same-origin",
			cache: "no-store",
			headers: { "content-type": "application/json" },
			body: JSON.stringify({
				room,
				agent,
				text,
				sourceMessageId: line.id
			})
		});
		if (!reply.ok) throw new Error(String(reply.status));
		applyChat(await reply.json());
	} catch {
		line.failed = true;
	}
	renderLog(true);
}
function wireChat() {
	const select = $("chat-to");
	for (const id of AGENT_IDS) {
		const option = document.createElement("option");
		option.value = id;
		option.textContent = nameOf(id);
		select.append(option);
	}
	select.value = "leader";
	const input = $("chat-input");
	const form = $("chat-form");
	const submit = () => {
		const text = input.value;
		if (!text.trim() || !state.connected) return;
		const room = state.conversation === ROOM_TEAM ? ROOM_TEAM : "direct";
		const agent = room === ROOM_TEAM ? select.value : state.conversation;
		input.value = "";
		send(room, agent, text);
	};
	form.addEventListener("submit", (e) => {
		e.preventDefault();
		submit();
	});
	input.addEventListener("keydown", (e) => {
		e.stopPropagation();
		if (e.key === "Enter" && !e.shiftKey && !e.isComposing) {
			e.preventDefault();
			submit();
		}
	});
	input.addEventListener("keyup", (e) => e.stopPropagation());
}
var retryMs = RETRY_MIN_MS;
function setConn(text, kind) {
	const el = $("conn");
	el.textContent = text;
	el.dataset.state = kind;
	state.connected = kind === "ok";
	$("chat-form").setAttribute("aria-disabled", String(!state.connected));
	$("chat-send").disabled = !state.connected;
}
function onFrame(text) {
	let frame;
	try {
		frame = JSON.parse(text);
	} catch {
		return;
	}
	if (!frame || typeof frame !== "object") return;
	if (typeof frame.seq === "number") {
		if (state.lastSeq !== null && frame.seq <= state.lastSeq) return;
		state.lastSeq = frame.seq;
		if (frame.kind === "office.agent") applyAgent(frame.payload, false);
		else if (frame.kind === "office.chat") applyChat(frame.payload);
		else if (frame.kind === "office.approval") applyApproval($("approvals"), frame.payload, (input, init) => fetch(input, init));
	}
}
async function loadSnapshot() {
	try {
		const reply = await fetch(SNAPSHOT_PATH, {
			credentials: "same-origin",
			cache: "no-store"
		});
		if (!reply.ok) return;
		const snap = await reply.json();
		for (const view of snap.agents ?? []) applyAgent(view, true);
		for (const resp of snap.pending ?? []) applyChat(resp);
		if (typeof snap.last_seq === "number") state.lastSeq = snap.last_seq;
	} catch {}
}
function retryLater() {
	const wait = retryMs;
	retryMs = Math.min(retryMs * 2, RETRY_MAX_MS);
	setTimeout(() => {
		start();
	}, wait);
}
function openSocket() {
	let url = `${location.protocol.replace("http", "ws")}//${location.host}${WS_PATH}`;
	if (state.lastSeq !== null) url += `?last_seq=${encodeURIComponent(String(state.lastSeq))}`;
	let socket;
	try {
		socket = new WebSocket(url);
	} catch {
		retryLater();
		return;
	}
	let closed = false;
	const onClose = () => {
		if (closed) return;
		closed = true;
		setConn("연결 끊김 (마지막으로 받은 상태 유지) · 다시 잇는 중", "bad");
		retryLater();
	};
	socket.onopen = () => {
		retryMs = RETRY_MIN_MS;
		setConn("연결됨", "ok");
	};
	socket.onmessage = (event) => {
		if (typeof event.data === "string") onFrame(event.data);
	};
	socket.onclose = onClose;
	socket.onerror = () => {
		try {
			socket.close();
		} catch {}
		onClose();
	};
}
async function start() {
	try {
		if (!(await fetch(SESSION_PATH, {
			method: "POST",
			credentials: "same-origin",
			cache: "no-store"
		})).ok) throw new Error("session");
		if (state.lastSeq === null) await loadSnapshot();
		openSocket();
	} catch {
		setConn("연결 끊김 (마지막으로 받은 상태 유지) · 다시 잇는 중", "bad");
		retryLater();
	}
}
async function main() {
	try {
		state.map = await (await fetch(MAP_PATH, { cache: "no-store" })).json();
	} catch {
		state.map = null;
	}
	for (const id of AGENT_IDS) state.names[id] = state.map?.agents?.[id]?.name ?? id;
	wireChat();
	renderTabs();
	renderLog(true);
	setConn("잇는 중", "wait");
	const narrow = window.matchMedia("(max-width: 760px)").matches;
	const toggle = $("view-toggle");
	if (!state.map || narrow) {
		showLight(state.map ? "narrow" : "lost");
		toggle.textContent = "3D 사무실";
	} else try {
		await show3d();
		toggle.hidden = false;
		toggle.textContent = "가벼운 화면";
	} catch {
		showLight("lost");
	}
	toggle.addEventListener("click", async () => {
		if (state.view) {
			showLight("chosen");
			toggle.hidden = false;
			toggle.textContent = "3D 사무실";
		} else try {
			await show3d();
			toggle.textContent = "가벼운 화면";
		} catch {
			showLight("lost");
		}
	});
	start();
}
main();
