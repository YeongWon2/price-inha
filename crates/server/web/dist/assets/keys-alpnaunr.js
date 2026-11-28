import { n as API, t as mountMenu } from "./menu-kc5ybrux.js";
import { n as submitKeyFields, t as applyKeyFieldRules } from "./field-e6l8inco.js";
// src/keys/session.ts
/** 세션이 없을 때의 HTTP 상태. */
var NO_SESSION = 401;
/** 주소 이름이 그 컴퓨터(루프백)인가: `localhost` · `127.x.x.x` · `::1`(대괄호 포함). */
function isLoopbackHost(host) {
	const name = host.toLowerCase();
	if (name === "localhost" || name === "::1" || name === "[::1]") return true;
	const parts = name.split(".");
	return parts.length === 4 && parts[0] === "127" && parts.every((part) => /^\d{1,3}$/.test(part) && Number(part) <= 255);
}
/** 세션을 쓸 수 있게 됐는지 돌려준다(이미 있었거나 그 컴퓨터라 새로 얻었으면 참). 실패해도 던지지 않는다. */
async function ensureKeysSession(fetcher, host) {
	try {
		const probe = await fetcher(API.setupSlots, {
			credentials: "same-origin",
			cache: "no-store"
		});
		if (probe.ok) return true;
		if (probe.status !== NO_SESSION || !isLoopbackHost(host)) return false;
		return (await fetcher(API.session, {
			method: "POST",
			credentials: "same-origin",
			cache: "no-store"
		})).ok;
	} catch {
		return false;
	}
}
// src/keys/main.ts
mountMenu(document.getElementById("menu"), "keys");
var SLOT_FIELDS = {
	dart: "api_key",
	ecos: "api_key",
	fred: "api_key",
	backup_github_token: "token"
};
var PROVIDER_LABELS = { naver_news: "네이버 뉴스 키 (뉴스 검색)" };
var REASONS = {
	key_rejected: "키가 맞지 않습니다",
	unreachable: "확인할 곳에 닿지 못했습니다",
	invalid_input: "칸을 다시 확인하세요",
	body_too_large: "값이 너무 깁니다",
	save_failed: "저장하지 못했습니다",
	retired_item: "예전의 한 칸짜리 자료 키는 받지 않습니다. 자료 키는 DART · ECOS · FRED · 네이버 칸으로 넣습니다"
};
function byId(id) {
	return document.getElementById(id);
}
function say(node, text) {
	node.textContent = String(text);
}
function getView(path) {
	return fetch(path, {
		credentials: "same-origin",
		cache: "no-store"
	}).then((reply) => reply.ok ? reply.json() : null);
}
function loadSlots() {
	getView(API.setupSlots).then((view) => {
		if (!view || !Array.isArray(view.slots)) return;
		view.slots.forEach((slot) => {
			const node = slot && document.getElementById("k-slot-" + String(slot.name) + "-reg");
			if (node) say(node, slot.registered === true ? "(등록됨)" : "(등록 안 됨)");
		});
	}).catch(() => {});
	loadProviders();
}
function providerBlock(provider) {
	const name = String(provider.provider);
	const $block = document.createElement("div");
	$block.className = "item";
	$block.id = "k-provider-" + name;
	const $title = document.createElement("h2");
	say($title, (PROVIDER_LABELS[name] || name) + " ");
	const $reg = document.createElement("span");
	$reg.id = "k-provider-" + name + "-reg";
	$reg.className = "hint";
	$title.appendChild($reg);
	$block.appendChild($title);
	const fields = {};
	provider.fields.forEach((field) => {
		const id = "k-provider-" + name + "-" + String(field);
		const $label = document.createElement("label");
		$label.htmlFor = id;
		say($label, String(field));
		const $input = document.createElement("input");
		$input.id = id;
		applyKeyFieldRules($input);
		$block.appendChild($label);
		$block.appendChild($input);
		fields[String(field)] = id;
	});
	const $button = document.createElement("button");
	$button.id = "k-provider-" + name + "-button";
	$button.type = "button";
	say($button, "등록");
	const $message = document.createElement("span");
	$message.id = "k-provider-" + name + "-msg";
	$message.className = "msg";
	$message.setAttribute("aria-live", "polite");
	$block.appendChild($button);
	$block.appendChild($message);
	byId("k-providers").appendChild($block);
	wire($button.id, $message.id, name, fields);
}
function loadProviders() {
	getView(API.setupProviders).then((view) => {
		if (!view || !Array.isArray(view.providers)) return;
		view.providers.forEach((provider) => {
			if (!provider || typeof provider.provider !== "string" || !Array.isArray(provider.fields)) return;
			if (!document.getElementById("k-provider-" + provider.provider)) providerBlock(provider);
			say(byId("k-provider-" + provider.provider + "-reg"), provider.registered === true ? "(등록됨)" : "(등록 안 됨)");
		});
	}).catch(() => {});
}
function wire(buttonId, messageId, item, fields) {
	const $button = byId(buttonId);
	const $message = byId(messageId);
	$button.addEventListener("click", () => {
		const inputs = {};
		Object.keys(fields).forEach((name) => {
			inputs[name] = byId(fields[name]);
		});
		$button.disabled = true;
		submitKeyFields(inputs, (values) => fetch(API.remoteKeys, {
			method: "POST",
			credentials: "same-origin",
			headers: { "Content-Type": "application/json" },
			body: JSON.stringify({
				item,
				fields: values
			})
		}).then((reply) => reply.json().catch(() => null).then((data) => {
			if (reply.ok && data && data.registered) {
				say($message, "등록했습니다");
				loadSlots();
				return true;
			}
			if (reply.status === 401) say($message, "먼저 로그인하세요");
			else say($message, data && REASONS[String(data.error)] || "등록하지 못했습니다");
			return false;
		})).catch(() => {
			say($message, "서버에 닿지 못했습니다");
			return false;
		})).then(() => {
			$button.disabled = false;
		});
	});
}
wire("k-broker", "k-broker-msg", "broker", {
	client_id: "k-client-id",
	client_secret: "k-client-secret"
});
wire("k-ai", "k-ai-msg", "ai_api_key", { api_key: "k-api-key" });
Object.keys(SLOT_FIELDS).forEach((item) => {
	const fields = {};
	fields[SLOT_FIELDS[item]] = "k-slot-" + item;
	wire("k-slot-" + item + "-button", "k-slot-" + item + "-msg", item, fields);
});
ensureKeysSession((path, init) => fetch(path, init), location.hostname).then(loadSlots);
