import { c as tradePath, n as API, t as mountMenu } from "./menu-ebudw6pk.js";
/* empty css               */
import { n as call, r as el, t as LOAD_FAILED } from "./call-enmqzayl.js";
import { a as errorLine, i as emptyState, n as button, r as card, s as loading, t as badge } from "./parts-ma3d1o7o.js";
// src/history/model.ts
/** 매매 상태 글자 전부(상태 거르기의 고르기 차례). */
var TRADE_STATES = [
	"discussing",
	"judged",
	"awaiting_approval",
	"blocked",
	"ordered",
	"holding",
	"sold",
	"reviewed"
];
/** 승인 요청 결과 글자 전부(상태 거르기의 고르기 차례). */
var APPROVAL_STATUSES = [
	"pending",
	"approved",
	"rejected",
	"no_answer",
	"expired"
];
/** 값이 없을 때 보이는 글. */
var NONE_TEXT = "없음";
/** 매매 상태 글자 → 스펙의 이름. */
var TRADE_STATE_LABELS = {
	discussing: "논의중",
	judged: "판단됨",
	awaiting_approval: "승인대기",
	blocked: "막힘",
	ordered: "주문됨",
	holding: "보유중",
	sold: "팔림",
	reviewed: "복기됨"
};
/** 승인 요청 결과 글자 → 보이는 글. */
var APPROVAL_STATUS_LABELS = {
	pending: "기다림",
	approved: "승인",
	rejected: "거절",
	no_answer: "답 없음",
	expired: "만료"
};
/** 한국 시각(UTC+9)의 밀리초 차이. */
var KST_OFFSET_MS = 324e5;
/** 근거 요약 줄의 칸과 이름(F7-7 차례). */
var BASIS_LABELS = [
	["hypothesis", "가설"],
	["trigger", "계기"],
	["entry", "진입"],
	["stop", "손절"],
	["target", "목표"],
	["scores", "담당 점수"],
	["facts", "자료"]
];
/** 표에서 그 글자의 글을 찾는다. 없으면 글자 그대로. */
function labelOf(table, word) {
	return Object.prototype.hasOwnProperty.call(table, word) ? table[word] : word;
}
/**
* 매매 상태 글(스펙의 매매 상태 이름): 논의중 · 판단됨 · 승인대기 · 막힘 · 주문됨 · 보유중 · 팔림 · 복기됨.
* 모르는 글자는 그대로.
*/
function tradeStateLabel(state) {
	return labelOf(TRADE_STATE_LABELS, state);
}
/** 승인 요청 결과 글: 기다림 · 승인 · 거절 · 답 없음 · 만료. 모르는 글자는 그대로. */
function approvalStatusLabel(status) {
	return labelOf(APPROVAL_STATUS_LABELS, status);
}
/** 매매 방향 글: 사기 · 팔기, 없으면 [`NONE_TEXT`]. */
function sideText(side) {
	if (side === "buy") return "사기";
	if (side === "sell") return "팔기";
	return side === null ? NONE_TEXT : side;
}
function two(value) {
	return String(value).padStart(2, "0");
}
/**
* 유닉스 밀리초 → 한국 시각 글 `"YYYY-MM-DD HH:mm"`(24시간, 두 자리). 컴퓨터의 시간대와 상관없이 UTC+9로 본다.
* 예: `Date.UTC(2026, 10, 15, 15, 0)` → `"2026-11-16 00:00"`.
*/
function timeText(ms) {
	const at = new Date(ms + KST_OFFSET_MS);
	return `${`${at.getUTCFullYear()}-${two(at.getUTCMonth() + 1)}-${two(at.getUTCDate())}`} ${two(at.getUTCHours())}:${two(at.getUTCMinutes())}`;
}
/** 정수 부분 글자에 세 자리마다 쉼표. */
function grouped(digits) {
	return digits.replace(/\B(?=(\d{3})+(?!\d))/g, ",");
}
/**
* 십진수 금액 글자 → 그 시장 돈 모양. 원화(`KRW`)는 천 단위 쉼표와 "원"(`"100000"` → `"100,000원"`,
* `"-5000"` → `"-5,000원"`), 달러(`USD`)는 "$"와 천 단위 쉼표 · 소수 둘째 자리(`"1234.5"` → `"$1,234.50"`,
* `"-12"` → `"-$12.00"`). 없으면 [`NONE_TEXT`]. 모르는 통화는 수 글 뒤에 통화 글자.
* 부동소수점으로 바꾸지 않고 글자로만 다룬다.
*/
function moneyText(value, currency) {
	if (value === null) return NONE_TEXT;
	const shape = /^([+-]?)(\d+)(?:\.(\d+))?$/.exec(value.trim());
	if (!shape) return value;
	const sign = shape[1] === "-" ? "-" : "";
	const whole = grouped(shape[2]);
	const fraction = shape[3] ?? "";
	if (currency === "USD") return `${sign}$${whole}.${(fraction + "00").slice(0, 2)}`;
	const number = fraction ? `${whole}.${fraction}` : whole;
	if (currency === "KRW") return `${sign}${number}원`;
	return `${sign}${number} ${currency}`;
}
/** 매매 목록의 줄(응답 차례). */
function tradeRows(reply) {
	return reply.trades.map((item) => ({
		id: item.id,
		symbol: item.symbol,
		state: item.state,
		stateLabel: tradeStateLabel(item.state),
		created: timeText(item.created_at),
		updated: timeText(item.updated_at),
		decided: item.decided_at === null ? null : timeText(item.decided_at)
	}));
}
/** 상태 흐름의 줄(응답 차례 = 시각 순서). */
function flowRows(detail) {
	return detail.states.map((change) => ({
		state: change.state,
		label: tradeStateLabel(change.state),
		at: timeText(change.at)
	}));
}
/** 글이 비었으면 [`NONE_TEXT`]. */
function orNone(text) {
	return text === null || text.trim() === "" ? NONE_TEXT : text;
}
/**
* 근거 요약 줄(F7-7): 가설 · 계기 · 진입 · 손절 · 목표 · 담당 점수 · 자료, 이 차례로 늘 일곱 줄.
* - 진입 · 손절 · 목표는 [`moneyText`]로 그 매매의 통화 모양.
* - 담당 점수는 담당마다 `"<담당> <점수>점 — <까닭>"`을 `" · "`로 잇는다.
* - 자료는 붙은 자료 이름(`facts`)과 판단에 쓴 자료 이름(`basis.sources`, 겹치면 한 번)을 `" · "`로 잇는다.
* - 값이 없으면 [`NONE_TEXT`].
*/
function basisLines(detail) {
	const basis = detail.basis;
	const names = [.../* @__PURE__ */ new Set([...detail.facts, ...basis.sources.map((source) => source.name)])];
	const text = {
		hypothesis: orNone(basis.hypothesis),
		trigger: orNone(basis.trigger),
		entry: moneyText(basis.entry, detail.currency),
		stop: moneyText(basis.stop, detail.currency),
		target: moneyText(basis.target, detail.currency),
		scores: orNone(basis.scores.map((s) => `${s.role} ${s.score}점 — ${s.reason}`).join(" · ")),
		facts: orNone(names.join(" · "))
	};
	return BASIS_LABELS.map(([key, label]) => ({
		key,
		label,
		text: text[key]
	}));
}
/** 승인 기록의 줄(응답 차례). */
function approvalRows(reply) {
	return reply.approvals.map((item) => ({
		id: item.id,
		number: item.number,
		summary: item.summary,
		status: item.status,
		statusLabel: approvalStatusLabel(item.status),
		created: timeText(item.created_at),
		resolved: item.resolved_at === null ? null : timeText(item.resolved_at),
		consumed: item.consumed_at === null ? null : timeText(item.consumed_at)
	}));
}
// src/history/view.ts
/** 탭 차례와 글. */
var TABS = [
	["trades", "매매"],
	["summary", "하루 정리"],
	["approvals", "승인 기록"]
];
/** 탭 단추의 id. */
function tabId(name) {
	return `history-tab-${name}`;
}
/** 탭 줄과 판 셋. 처음 고른 탭은 `selected`. */
function renderTabs(selected) {
	const tabs = el("div", "his-tabs");
	tabs.setAttribute("role", "tablist");
	tabs.setAttribute("aria-label", "히스토리");
	const panels = {};
	for (const [name, label] of TABS) {
		const tab = el("button", "his-tab", label);
		tab.type = "button";
		tab.id = tabId(name);
		tab.dataset.tab = name;
		tab.setAttribute("role", "tab");
		tab.setAttribute("aria-controls", `history-panel-${name}`);
		const panel = el("div", "his-panel");
		panel.id = `history-panel-${name}`;
		panel.dataset.tab = name;
		panel.setAttribute("role", "tabpanel");
		panel.setAttribute("aria-labelledby", tab.id);
		panel.tabIndex = 0;
		tabs.append(tab);
		panels[name] = panel;
	}
	selectTab(tabs, panels, selected);
	return {
		tabs,
		panels
	};
}
/** 그 탭을 고른다: 고른 탭만 `aria-selected="true"` · 포커스 차례(tabindex 0), 그 판만 보인다. */
function selectTab(tabs, panels, selected) {
	for (const tab of tabs.querySelectorAll("[role=\"tab\"]")) {
		const on = tab.dataset.tab === selected;
		tab.setAttribute("aria-selected", String(on));
		tab.tabIndex = on ? 0 : -1;
	}
	for (const [name] of TABS) panels[name].hidden = name !== selected;
}
/** 상태 거르기 칸: 첫 칸 ""은 전체, 이어서 `words`(글은 `label`). */
function renderFilter(role, caption, words, label) {
	const box = el("label", "his-filter");
	const select = el("select", "ui-input his-select");
	select.dataset.role = role;
	const all = el("option", "", "전체");
	all.value = "";
	select.append(all);
	for (const word of words) {
		const option = el("option", "", label(word));
		option.value = word;
		select.append(option);
	}
	box.append(el("span", "his-filter-label", caption), select);
	return box;
}
/** 매매 상태 거르기. */
function renderTradeFilter() {
	return renderFilter("trade-state", "상태", TRADE_STATES, tradeStateLabel);
}
/** 승인 결과 거르기. */
function renderApprovalFilter() {
	return renderFilter("approval-status", "결과", APPROVAL_STATUSES, approvalStatusLabel);
}
/** 매매 상태의 배지 색. */
function tradeTone(state) {
	if (state === "blocked") return "danger";
	if (state === "awaiting_approval") return "warning";
	if (state === "sold" || state === "reviewed" || state === "holding") return "success";
	return "neutral";
}
/** 승인 결과의 배지 색. */
function approvalTone(status) {
	if (status === "approved") return "success";
	if (status === "rejected") return "danger";
	if (status === "pending") return "warning";
	return "neutral";
}
/** 칸 하나(`data-role`과 글). */
function cell(className, role, text) {
	const node = el("span", className, text);
	node.dataset.role = role;
	return node;
}
/** 목록 머리 줄(칸 이름, 화면 읽기 프로그램은 줄마다 이름을 따로 읽는다). */
function renderHead(className, columns) {
	const head = el("div", `his-head ${className}`);
	head.setAttribute("aria-hidden", "true");
	for (const column of columns) head.append(el("span", "", column));
	return head;
}
/** 매매 목록의 줄들(목록 `[data-role="trades"]`에 붙일 것). */
function renderTradeRows(rows) {
	return rows.map((row) => {
		const item = el("li", "his-trade");
		item.dataset.trade = String(row.id);
		item.dataset.state = row.state;
		const toggle = el("button", "his-row his-trade-cols");
		toggle.type = "button";
		toggle.dataset.action = "toggle";
		toggle.setAttribute("aria-expanded", "false");
		toggle.setAttribute("aria-controls", `history-trade-${row.id}`);
		toggle.setAttribute("aria-label", `매매 ${row.id} · ${row.symbol} · ${row.stateLabel} · 근거 펼치기`);
		const state = badge(row.stateLabel, tradeTone(row.state));
		state.dataset.role = "state";
		const stateCell = el("span", "his-cell-state");
		stateCell.append(state);
		toggle.append(cell("his-num", "id", `#${row.id}`), cell("his-symbol", "symbol", row.symbol), stateCell, cell("his-time", "time", row.created), cell("his-time his-decided", "decided", row.decided ?? "—"), el("span", "his-chevron"));
		item.append(toggle);
		return item;
	});
}
/** 펼친 줄 안의 근거 요약과 상태 흐름. `side`는 방향 · 수량 글. */
function renderDetail(basis, flow, side) {
	const detail = el("div", "his-detail");
	detail.dataset.role = "detail";
	const sideLine = el("p", "his-side", side);
	sideLine.dataset.role = "side";
	const list = el("dl", "his-basis");
	list.dataset.role = "basis";
	for (const line of basis) {
		const pair = el("div", "his-basis-line");
		pair.dataset.key = line.key;
		const value = el("dd", "");
		if ((line.key === "scores" || line.key === "facts") && line.text.includes(" · ")) {
			value.className = `his-items his-items-${line.key}`;
			for (const part of line.text.split(" · ")) value.append(el("span", "his-item", part));
		} else value.textContent = line.text;
		pair.append(el("dt", "", line.label), value);
		list.append(pair);
	}
	const steps = el("ol", "his-flow");
	steps.dataset.role = "flow";
	for (const step of flow) {
		const node = el("li", "his-step");
		node.dataset.state = step.state;
		node.append(el("span", "his-step-label", step.label), el("time", "his-step-at", step.at));
		steps.append(node);
	}
	const basisBox = el("section", "his-detail-part");
	basisBox.append(el("h3", "his-detail-title", "근거"), list);
	const flowBox = el("section", "his-detail-part");
	flowBox.append(el("h3", "his-detail-title", "상태 흐름"), steps);
	const parts = el("div", "his-detail-parts");
	parts.append(basisBox, flowBox);
	detail.append(sideLine, parts);
	return detail;
}
/** 하루 정리의 한 덩이(제목과 글). */
function summaryPart(part, title, text) {
	const node = el("section", "his-summary-part");
	node.dataset.part = part;
	node.append(el("h3", "his-summary-title", title), el("p", "his-summary-text", text));
	return node;
}
/** 하루 정리. */
function renderSummary(summary) {
	const box = el("div", "his-summary");
	box.dataset.role = "summary";
	const date = el("p", "his-summary-date", `${summary.date} 하루 정리`);
	date.dataset.role = "summary-date";
	box.append(date, summaryPart("done", "한 일", summary.done), summaryPart("pnl", "손익", summary.pnl), summaryPart("review", "복기", summary.review));
	return box;
}
/** 승인 기록의 줄들(목록 `[data-role="approvals"]`에 붙일 것). */
function renderApprovalRows(rows) {
	return rows.map((row) => {
		const item = el("li", "his-row his-approval-cols his-approval");
		item.dataset.approval = String(row.id);
		item.dataset.status = row.status;
		const status = badge(row.statusLabel, approvalTone(row.status));
		status.dataset.role = "status";
		const statusCell = el("span", "his-cell-state");
		statusCell.append(status);
		const closed = row.resolved === null ? "닫히지 않음" : `닫힘 ${row.resolved}`;
		const times = el("span", "his-times");
		times.append(cell("his-time", "time", row.created), el("span", "his-time his-closed", closed));
		item.append(cell("his-num his-number", "number", row.number), statusCell, cell("his-approval-summary", "summary", row.summary), times);
		return item;
	});
}
// src/history/screen.ts
/** JSON 몸통을 읽는다. 실패(상태 · 모양)면 던진다. */
async function read(url) {
	const reply = await call("GET", url);
	if (!reply.ok || typeof reply.body !== "object" || reply.body === null) throw new Error(`${url} ${reply.status}`);
	return reply.body;
}
/** 경로에 물음 값을 붙인다(빈 값은 뺀다). */
function withQuery(path, values) {
	const query = new URLSearchParams();
	for (const [name, value] of Object.entries(values)) if (value !== null && value !== "") query.set(name, String(value));
	const text = query.toString();
	return text ? `${path}?${text}` : path;
}
/** 오류 줄과 다시 시도 단추. */
function failed(again) {
	const retry = button("다시 시도", "accent");
	retry.dataset.action = "retry";
	retry.addEventListener("click", again);
	const box = el("div", "his-failed");
	box.append(errorLine(LOAD_FAILED), retry);
	return box;
}
/** 빈 상태 줄(`data-role="empty"`). */
function empty(text) {
	const node = emptyState(text);
	node.dataset.role = "empty";
	return node;
}
/** 쪽 넘기는 목록 하나(매매 · 승인 기록): 거르기 · 목록 · 더 보기 · 오류. */
var PagedList = class {
	spec;
	list;
	body = el("div", "his-list-body");
	foot = el("div", "his-foot");
	filter = "";
	loaded = false;
	constructor(spec, filterBox) {
		this.spec = spec;
		this.list = el("ul", "his-list");
		this.list.dataset.role = spec.role;
		const select = filterBox.querySelector("select");
		select?.addEventListener("change", () => {
			this.filter = select.value;
			this.load(null);
		});
	}
	/** 판에 붙일 덩이. */
	parts() {
		return [this.body, this.foot];
	}
	/** `before`가 없으면 처음 쪽부터 다시, 있으면 다음 쪽을 이어 붙인다. */
	async load(before) {
		if (before === null) this.body.replaceChildren(loading());
		const more = this.foot.querySelector("button");
		if (more) more.disabled = true;
		const filter = this.filter;
		let reply;
		try {
			reply = await read(withQuery(this.spec.path, {
				before,
				[this.spec.filterName]: filter
			}));
		} catch {
			const again = () => void this.load(before);
			if (before === null) this.body.replaceChildren(failed(again));
			else this.foot.replaceChildren(failed(again));
			return;
		}
		if (filter !== this.filter) return;
		this.loaded = true;
		const rows = this.spec.rows(reply);
		if (before === null) {
			this.list.replaceChildren(...rows);
			this.body.replaceChildren(rows.length === 0 ? empty(this.spec.emptyText) : this.wrap());
		} else this.list.append(...rows);
		this.renderMore(this.spec.next(reply));
	}
	wrap() {
		const box = el("div", "his-table");
		box.append(this.spec.head, this.list);
		return box;
	}
	renderMore(next) {
		if (next === null) {
			this.foot.replaceChildren();
			return;
		}
		const more = button("더 보기");
		more.dataset.action = "more";
		more.addEventListener("click", () => void this.load(next));
		this.foot.replaceChildren(more);
	}
};
/** 화면 하나의 상태와 그리기. */
var HistoryScreen = class {
	tabs;
	panels;
	trades;
	approvals;
	summaryBody = el("div", "his-summary-body");
	dateInput = el("input", "ui-input his-date");
	summaryLoaded = false;
	/** 펼친 줄의 근거(다시 펼칠 때 다시 읽지 않는다). */
	details = /* @__PURE__ */ new Map();
	constructor(main) {
		const { tabs, panels } = renderTabs("trades");
		this.tabs = tabs;
		this.panels = panels;
		tabs.addEventListener("click", (event) => {
			const tab = event.target instanceof Element ? event.target.closest("[role=\"tab\"]") : null;
			if (tab?.dataset.tab) this.choose(tab.dataset.tab);
		});
		tabs.addEventListener("keydown", (event) => this.onTabKey(event));
		const tradeFilter = renderTradeFilter();
		this.trades = new PagedList({
			path: API.trades,
			filterName: "state",
			role: "trades",
			head: renderHead("his-trade-cols", [
				"번호",
				"종목",
				"상태",
				"만든 시각",
				"판단 시각",
				""
			]),
			emptyText: "아직 매매가 없습니다. 팀이 종목을 논의하기 시작하면 여기에 쌓입니다.",
			rows: (reply) => renderTradeRows(tradeRows(reply)),
			next: (reply) => reply.next
		}, tradeFilter);
		this.trades.list.addEventListener("click", (event) => this.onTradeClick(event));
		panels.trades.append(card("매매", this.lead("줄을 누르면 근거와 상태 흐름이 펼쳐집니다."), tradeFilter, ...this.trades.parts()));
		const approvalFilter = renderApprovalFilter();
		this.approvals = new PagedList({
			path: API.approvalLog,
			filterName: "status",
			role: "approvals",
			head: renderHead("his-approval-cols", [
				"승인 번호",
				"결과",
				"요약",
				"시각"
			]),
			emptyText: "아직 승인 요청이 없습니다.",
			rows: (reply) => renderApprovalRows(approvalRows(reply)),
			next: (reply) => reply.next
		}, approvalFilter);
		panels.approvals.append(card("승인 기록", this.lead("요약의 비밀 값은 가려져 있습니다."), approvalFilter, ...this.approvals.parts()));
		panels.summary.append(card("하루 정리", this.datePicker(), this.summaryBody));
		main.append(tabs, ...TABS.map(([name]) => panels[name]));
	}
	lead(text) {
		return el("p", "his-lead", text);
	}
	datePicker() {
		const box = el("label", "his-filter");
		this.dateInput.type = "date";
		this.dateInput.dataset.role = "date";
		this.dateInput.addEventListener("change", () => void this.loadSummary(this.dateInput.value || null));
		box.append(el("span", "his-filter-label", "날짜"), this.dateInput);
		return box;
	}
	/** 첫 그림: 매매 목록. */
	async start() {
		await this.trades.load(null);
	}
	choose(name) {
		selectTab(this.tabs, this.panels, name);
		if (name === "summary" && !this.summaryLoaded) this.loadSummary(null);
		if (name === "approvals" && !this.approvals.loaded) this.approvals.load(null);
	}
	/** 좌우 방향키 · Home · End로 탭을 옮긴다. */
	onTabKey(event) {
		const names = TABS.map(([name]) => name);
		const current = names.findIndex((name) => this.panels[name].hidden === false);
		const moves = {
			ArrowRight: (current + 1) % names.length,
			ArrowLeft: (current - 1 + names.length) % names.length,
			Home: 0,
			End: names.length - 1
		};
		if (!(event.key in moves)) return;
		event.preventDefault();
		const name = names[moves[event.key]];
		this.choose(name);
		this.tabs.querySelector(`[data-tab="${name}"]`)?.focus();
	}
	/** 하루 정리를 읽는다(`date`가 없으면 오늘). */
	async loadSummary(date) {
		this.summaryBody.replaceChildren(loading());
		try {
			const summary = await read(withQuery(API.dailySummary, { date }));
			this.summaryLoaded = true;
			this.dateInput.value = summary.date;
			if (date === null) this.dateInput.max = summary.date;
			this.summaryBody.replaceChildren(renderSummary(summary));
		} catch {
			this.summaryBody.replaceChildren(failed(() => void this.loadSummary(date)));
		}
	}
	onTradeClick(event) {
		const toggle = event.target instanceof Element ? event.target.closest("button[data-action=\"toggle\"]") : null;
		const item = toggle?.closest("[data-trade]");
		if (toggle && item) this.toggle(Number(item.dataset.trade), item, toggle);
	}
	/** 줄을 펼치거나 접는다. 처음 펼칠 때 매매 하나를 읽는다. */
	async toggle(id, item, toggle) {
		const open = toggle.getAttribute("aria-expanded") === "true";
		const shown = this.details.get(id);
		if (open) {
			toggle.setAttribute("aria-expanded", "false");
			if (shown) shown.hidden = true;
			return;
		}
		toggle.setAttribute("aria-expanded", "true");
		if (shown) {
			shown.hidden = false;
			return;
		}
		item.querySelector(".his-detail-wait")?.remove();
		const wait = el("div", "his-detail-wait");
		wait.append(loading());
		item.append(wait);
		try {
			const detail = await read(tradePath(id));
			const side = `방향 ${sideText(detail.side)} · 수량 ${detail.quantity === null ? "없음" : `${detail.quantity}주`}`;
			const node = renderDetail(basisLines(detail), flowRows(detail), side);
			node.id = `history-trade-${id}`;
			this.details.set(id, node);
			wait.replaceWith(node);
			node.hidden = toggle.getAttribute("aria-expanded") !== "true";
		} catch {
			toggle.setAttribute("aria-expanded", "false");
			wait.replaceChildren(errorLine("근거를 불러오지 못했습니다. 줄을 다시 눌러 주세요."));
		}
	}
};
/**
* 본문 자리(`main`)에 히스토리 화면을 그린다: 세션을 얻고 매매 목록을 읽어 그린다.
* 첫 그림이 끝나면 풀린다. 자리가 없으면 아무것도 하지 않는다.
*/
async function mountHistory(main) {
	if (!main) return;
	const screen = new HistoryScreen(main);
	try {
		await call("POST", API.session);
	} catch {}
	await screen.start();
}
// src/history/main.ts
mountMenu(document.getElementById("menu"), "history");
mountHistory(document.getElementById("screen"));
