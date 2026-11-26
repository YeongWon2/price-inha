import { a as approvalPath, i as SCREEN_ROOTS, n as API, o as decidePath, t as mountMenu } from "./menu-g4dweas1.js";
import { t as renderApprovalExtras } from "./approval-extras-nqc3sg5q.js";
// src/phone/main.ts
mountMenu(document.getElementById("menu"), "phone");
var OFFICE_PATH = SCREEN_ROOTS.office;
var $ = (selector) => document.querySelector(selector);
var lastSeq = 0;
function message(error) {
	return error instanceof Error ? error.message : String(error);
}
function base64UrlToBytes(text) {
	const pad = "=".repeat((4 - text.length % 4) % 4);
	const raw = atob((text + pad).replace(/-/g, "+").replace(/_/g, "/"));
	return Uint8Array.from(raw, (c) => c.charCodeAt(0));
}
async function getJson(path) {
	const response = await fetch(path, { credentials: "same-origin" });
	if (!response.ok) throw new Error(String(response.status));
	return response.json();
}
async function postJson(path, body) {
	const response = await fetch(path, {
		method: "POST",
		credentials: "same-origin",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify(body)
	});
	if (!response.ok) throw new Error(String(response.status));
	return response.status === 204 ? null : response.json().catch(() => null);
}
async function registerWorker() {
	if (!("serviceWorker" in navigator)) return null;
	return navigator.serviceWorker.register("sw.js", { scope: "./" });
}
async function enablePush() {
	const status = $("[data-part=\"push-status\"]");
	try {
		const registration = await registerWorker();
		if (!registration || !("pushManager" in registration)) {
			status.textContent = "이 브라우저는 알림을 받을 수 없어요. 홈 화면에 추가한 뒤 다시 해 보세요.";
			return;
		}
		if (await Notification.requestPermission() !== "granted") {
			status.textContent = "알림 권한이 없어요.";
			return;
		}
		const { key } = await getJson(API.pushKey);
		const json = (await registration.pushManager.subscribe({
			userVisibleOnly: true,
			applicationServerKey: base64UrlToBytes(key)
		})).toJSON();
		await postJson(API.pushSubscribe, {
			endpoint: json.endpoint,
			p256dh: json.keys?.p256dh,
			auth: json.keys?.auth,
			device: navigator.userAgent.slice(0, 60)
		});
		status.textContent = "알림을 켰어요.";
	} catch (error) {
		status.textContent = "알림을 켜지 못했어요: " + message(error);
	}
}
function addNotice(notice) {
	const item = document.createElement("li");
	item.dataset.seq = String(notice.seq);
	const title = document.createElement("strong");
	title.textContent = notice.title;
	const body = document.createElement("div");
	body.className = "muted";
	body.textContent = notice.body;
	item.append(title, body);
	const link = notice.link;
	if (link) item.addEventListener("click", () => openLink(link));
	$("#notice-list").prepend(item);
}
async function loadNotices() {
	try {
		const list = await getJson(API.notices + "?after=" + lastSeq + "&max=50");
		for (const notice of list) {
			addNotice(notice);
			lastSeq = Math.max(lastSeq, notice.seq);
		}
	} catch {}
}
async function showApproval(id) {
	const card = $("[data-card=\"approval\"]");
	try {
		const approval = await getJson(approvalPath(id));
		card.dataset.approvalId = id;
		card.dataset.fingerprint = approval.fingerprint || "";
		$part(card, "summary").textContent = approval.summary || "";
		renderApprovalExtras($part(card, "extras"), approval);
		$part(card, "fingerprint").textContent = approval.fingerprint || "";
		$part(card, "result").textContent = "";
		delete card.dataset.deciding;
		card.querySelector("[data-field=\"code\"]").value = (approval.fingerprint || "").slice(0, 6);
		setDecideButtons(card, false);
		card.hidden = false;
	} catch {
		card.hidden = true;
	}
}
function $part(card, name) {
	return card.querySelector("[data-part=\"" + name + "\"]");
}
/** 카드의 승인 · 거절 단추를 함께 막거나 켠다. */
function setDecideButtons(card, locked) {
	for (const verb of ["approve", "reject"]) {
		const button = card.querySelector("[data-action=\"" + verb + "\"]");
		if (button) button.disabled = locked;
	}
}
async function decide(verb) {
	const card = $("[data-card=\"approval\"]");
	const result = $part(card, "result");
	if (!card.dataset.approvalId || !card.dataset.fingerprint) {
		result.textContent = "승인 내용을 아직 받지 못했어요.";
		return;
	}
	if (card.dataset.deciding) return;
	const sentId = card.dataset.approvalId;
	card.dataset.deciding = sentId;
	setDecideButtons(card, true);
	try {
		await postJson(decidePath(sentId, verb), { fingerprint: card.dataset.fingerprint });
		if (card.dataset.approvalId === sentId) result.textContent = verb === "approve" ? "승인했어요." : "거절했어요.";
	} catch (error) {
		if (card.dataset.approvalId === sentId) {
			result.textContent = "처리하지 못했어요: " + message(error);
			setDecideButtons(card, false);
		}
	} finally {
		if (card.dataset.deciding === sentId) delete card.dataset.deciding;
	}
}
async function loadDailySummary() {
	try {
		const summary = await getJson(API.dailySummary);
		for (const part of [
			"done",
			"pnl",
			"review"
		]) {
			const box = document.querySelector("#daily-summary [data-part=\"" + part + "\"] p");
			const text = summary[part];
			if (box && text) box.textContent = text;
		}
	} catch {}
}
function openLink(link) {
	const match = /\/approvals\/([^/?#]+)/.exec(link);
	if (match) showApproval(decodeURIComponent(match[1]));
}
document.addEventListener("click", (event) => {
	const action = event.target?.closest("[data-action]");
	if (!action) return;
	switch (action.dataset.action) {
		case "enable-push":
			enablePush();
			break;
		case "approve":
			decide("approve");
			break;
		case "reject":
			decide("reject");
			break;
		case "open-office": window.location.href = OFFICE_PATH;
	}
});
if ("serviceWorker" in navigator) navigator.serviceWorker.addEventListener("message", (event) => {
	const data = event.data || {};
	if (data.type === "notice") loadNotices();
	if (data.type === "open" && data.link) openLink(new URL(data.link).pathname);
});
registerWorker().catch(() => {});
openLink(window.location.pathname + window.location.hash.replace("#", ""));
loadNotices();
loadDailySummary();
