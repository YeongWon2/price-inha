import { n as API, t as mountMenu } from "./menu-ndyyrws6.js";
/* empty css               */
import { a as errorLine, c as table, d as el, i as emptyState, l as LOAD_FAILED, n as button, r as card, s as loading, t as badge, u as call } from "./parts-clyrk8h1.js";
import { a as isObject, i as formatValue, n as changeRow, r as fieldLabel, t as capText } from "./fields-j3bgfwvj.js";
// src/rules/model.ts
/** 상태 글. */
var STATE_LABELS = {
	proposed: "제안됨",
	cooling: "냉각중",
	applied: "반영됨",
	rejected: "거절됨"
};
/** 실수 종류 글(journal `Mistake`의 뜻). */
var MISTAKES = {
	chased_entry: "추격 매수",
	late_stop: "손절이 늦음",
	oversized_position: "비중 과다",
	no_rationale: "근거 없이 진입",
	panic_exit: "성급한 청산"
};
/** 거절 까닭 글(E35 · V21). 모르는 까닭은 글자 그대로 보인다. */
var REASONS = {
	user_rejected: "거절했습니다. 규칙은 그대로입니다.",
	already_resolved: "이미 답한 개선안입니다. 목록을 새로 읽었습니다.",
	template_locked: "쓰는 템플릿의 값이 잠겨 있어(승격대기 · 실전중) 반영하지 않았습니다. 승인 번호는 그대로 기다립니다.",
	fingerprint_mismatch: "승인 내용이 달라(지문 다름) 반영하지 않았습니다.",
	not_found: "모르는 승인 번호입니다.",
	expired: "승인 기한이 지났습니다."
};
/** 상태 글. */
function proposalStateLabel(state) {
	return STATE_LABELS[state] ?? state;
}
/**
* 실수 종류 글: `chased_entry` "추격 매수" · `late_stop` "손절이 늦음" · `oversized_position` "비중 과다" ·
* `no_rationale` "근거 없이 진입" · `panic_exit` "성급한 청산"(journal `Mistake`의 뜻). 그 밖의 글은 그대로.
*/
function mistakeLabel(word) {
	return MISTAKES[word] ?? word;
}
/** 그 칸의 바뀐 값(없으면 없음). */
function changeOf(changes, key) {
	return changes.find((change) => change.key === key);
}
/** 규칙 표의 줄(응답의 칸 순서). 템플릿이 없으면 빈 목록. */
function ruleRows(reply) {
	return reply.fields.map((field) => {
		const value = formatValue(field.key, field.value);
		let changed = null;
		for (const recent of reply.recent) {
			const change = changeOf(recent.changes, field.key);
			if (!change) continue;
			if (formatValue(field.key, change.after) === value) changed = {
				proposal: recent.proposal,
				before: formatValue(field.key, change.before)
			};
			break;
		}
		let cooling = null;
		for (const pending of reply.cooling) {
			const change = changeOf(pending.changes, field.key);
			if (change) cooling = {
				proposal: pending.proposal,
				after: formatValue(field.key, change.after)
			};
		}
		return {
			key: field.key,
			label: fieldLabel(field.key),
			value,
			cap: capText(field.cap),
			changed,
			cooling
		};
	});
}
/** 원칙의 절대 상한 줄: 키 → 보이는 이름과 상한 글("상한 2%"), 응답의 `caps` 차례. */
function capRows(reply) {
	return Object.entries(reply.caps).map(([key, cap]) => ({
		key,
		label: fieldLabel(key),
		cap: capText(cap) ?? ""
	}));
}
/** 개선안 목록의 줄. 번호가 큰 것(새것)부터. */
function proposalRows(reply) {
	return [...reply.proposals].sort((a, b) => b.id - a.id).map((proposal) => ({
		id: proposal.id,
		state: proposal.state,
		stateLabel: proposalStateLabel(proposal.state),
		mistake: mistakeLabel(proposal.mistake),
		risky: proposal.increases_risk,
		riskLabel: proposal.increases_risk ? "위험을 키움" : "위험을 키우지 않음",
		changes: (proposal.changes ?? []).map(changeRow),
		canAnswer: proposal.state === "proposed" && typeof proposal.number === "string" && proposal.number.length > 0 && typeof proposal.fingerprint === "string" && proposal.fingerprint.length > 0,
		number: proposal.number,
		fingerprint: proposal.fingerprint
	}));
}
/** 답하기 결과 → 문구. */
function answerMessage(reply) {
	const body = isObject(reply.body) ? reply.body : {};
	if (reply.status !== 200) return {
		kind: "failed",
		text: "지금 답을 처리하지 못했습니다. 잠시 뒤 다시 시도해 주세요."
	};
	if (body.result === "applied") return {
		kind: "applied",
		text: "반영했습니다. 다음 매매부터 새 값을 씁니다."
	};
	if (body.result === "cooling") return {
		kind: "cooling",
		text: "승인했습니다. 위험을 키우는 변경이라 냉각중이며 다음 거래일부터 반영됩니다."
	};
	const reason = typeof body.reason === "string" ? body.reason : "";
	return {
		kind: "rejected",
		text: REASONS[reason] ?? `반영하지 않았습니다(${reason || "까닭 모름"}).`
	};
}
// src/rules/view.ts
/** 개선안 상태 배지 색. */
var STATE_TONES = {
	proposed: "warning",
	cooling: "neutral",
	applied: "success",
	rejected: "danger"
};
/** 표시 줄 하나(`data-role`과 글). */
function mark(role, className, text) {
	const node = el("span", className, text);
	node.dataset.role = role;
	return node;
}
/** 규칙 표의 줄 하나: 이름 · 지금 값(바뀜 · 냉각중 표시) · 상한. */
function ruleRow(row) {
	const tr = document.createElement("tr");
	tr.dataset.key = row.key;
	const name = el("th", "rul-name", row.label);
	name.scope = "row";
	const value = el("td", "rul-value");
	value.append(mark("value", "rul-now", row.value));
	if (row.changed) {
		tr.dataset.changed = "true";
		value.append(mark("changed", "rul-changed", `바뀜 · 앞 값 ${row.changed.before} (개선안 ${row.changed.proposal}번)`));
	}
	if (row.cooling) value.append(mark("cooling", "rul-cooling", `다음 거래일부터 ${row.cooling.after} (개선안 ${row.cooling.proposal}번)`));
	const cap = el("td", "rul-cap", row.cap ?? "—");
	cap.dataset.role = "cap";
	if (row.cap === null) cap.setAttribute("aria-label", "상한 없음");
	tr.append(name, value, cap);
	return tr;
}
/** 규칙 표. */
function renderRules(rows) {
	const node = table([
		"항목",
		"지금 값",
		"상한"
	], []);
	node.classList.add("rul-rules");
	node.dataset.role = "rules";
	const body = node.tBodies[0];
	for (const row of rows) body.append(ruleRow(row));
	return node;
}
/** 원칙의 절대 상한 목록. */
function renderCaps(rows) {
	const list = el("ul", "rul-caps");
	list.dataset.role = "caps";
	for (const row of rows) {
		const item = el("li", "rul-cap-item");
		item.dataset.key = row.key;
		item.append(el("span", "rul-cap-name", row.label), el("span", "rul-cap-value", row.cap));
		list.append(item);
	}
	return list;
}
/** 개선안 줄의 앞 → 뒤 값 목록. */
function changeList(row) {
	const list = el("ul", "rul-changes");
	for (const change of row.changes) {
		const line = el("li", "rul-change");
		line.dataset.role = "change";
		line.dataset.key = change.key;
		if (change.widened) line.dataset.widened = "true";
		const values = el("span", "rul-change-values");
		values.append(el("span", "rul-before", change.before), el("span", "rul-arrow", " → "), el("span", "rul-after", change.after));
		line.append(el("span", "rul-change-name", change.label), values);
		if (change.widened) line.append(badge("넓힘", "warning"));
		list.append(line);
	}
	if (row.changes.length === 0) list.append(el("li", "rul-change-empty", "바뀐 칸을 읽을 수 없습니다"));
	return list;
}
/** 개선안 줄 하나. */
function proposalRow(row, note) {
	const item = el("li", "rul-proposal");
	item.dataset.proposal = String(row.id);
	item.dataset.state = row.state;
	const head = el("div", "rul-proposal-head");
	const state = badge(row.stateLabel, STATE_TONES[row.state] ?? "neutral");
	state.dataset.role = "state";
	const risk = mark("risk", "rul-risk", row.riskLabel);
	risk.dataset.risky = String(row.risky);
	head.append(el("strong", "rul-proposal-no", `개선안 ${row.id}번`), state, mark("mistake", "rul-mistake", row.mistake), risk);
	item.append(head, changeList(row));
	const foot = el("div", "rul-proposal-foot");
	if (row.canAnswer) {
		const approve = button("승인", "accent");
		approve.dataset.action = "approve";
		const reject = button("거절", "danger");
		reject.dataset.action = "reject";
		foot.append(approve, reject);
	}
	const result = el("p", "rul-result");
	result.dataset.role = "answer-result";
	result.setAttribute("aria-live", "polite");
	if (note) {
		result.textContent = note.text;
		result.dataset.tone = note.kind;
	}
	foot.append(result);
	item.append(foot);
	return item;
}
/** 개선안 목록. `notes`는 개선안 번호 → 그 줄에 보일 답 결과 문구. */
function renderProposals(rows, notes) {
	const list = el("ol", "rul-proposals");
	list.dataset.role = "proposals";
	for (const row of rows) list.append(proposalRow(row, notes.get(row.id)));
	return list;
}
// src/rules/screen.ts
/** 빈 상태 줄에 `data-role`을 단다. */
function empty(role, text) {
	const node = emptyState(text);
	node.dataset.role = role;
	return node;
}
/** 안내 글 줄. */
function hint(text) {
	return el("p", "rul-hint", text);
}
/** 화면 하나의 상태와 그리기. */
var RulesScreen = class {
	body = el("div", "rul-body");
	rows = [];
	/** 개선안 번호 → 그 줄에 남길 답 결과 문구. */
	notes = /* @__PURE__ */ new Map();
	busy = false;
	constructor(main) {
		main.append(this.body);
	}
	renderLoading() {
		this.body.replaceChildren(loading());
	}
	/** 규칙 · 개선안 목록을 읽어 다시 그린다. 하나라도 실패하면 오류 줄과 다시 시도. */
	async load() {
		try {
			const [rules, proposals] = await Promise.all([call("GET", API.rules), call("GET", API.proposals)]);
			if (!rules.ok || typeof rules.body !== "object" || rules.body === null) throw new Error(`rules ${rules.status}`);
			if (!proposals.ok || typeof proposals.body !== "object" || proposals.body === null) throw new Error(`proposals ${proposals.status}`);
			this.render(rules.body, proposals.body);
		} catch {
			this.rows = [];
			this.renderFailed();
		}
	}
	render(rules, proposals) {
		const ruleList = ruleRows(rules);
		const current = rules.template === null || ruleList.length === 0 ? empty("no-template", "아직 고른 템플릿이 없어 적용 중인 규칙이 없습니다. 템플릿 화면에서 스타일을 고르세요.") : el("div", "rul-table-wrap");
		if (ruleList.length > 0) current.append(renderRules(ruleList));
		const rulesCard = card("지금 규칙", hint(this.rulesHint(rules)), current);
		const capsCard = card("절대 상한", hint("원칙이 정한 한도입니다. 개선안도 템플릿 값도 이 상한을 넘을 수 없습니다."), renderCaps(capRows(rules)));
		this.rows = (proposals.proposals ?? []).length > 0 ? proposalRows(proposals) : [];
		const list = this.rows.length === 0 ? empty("no-proposals", "아직 개선안이 없습니다. 복기에서 같은 실수가 반복되면 개선안이 올라옵니다.") : renderProposals(this.rows, this.notes);
		list.addEventListener("click", (event) => this.onClick(event));
		const proposalsCard = card("개선안", hint("위험을 키우는 개선안은 승인 뒤 냉각중이 되고 다음 거래일부터 반영됩니다."), list);
		this.body.replaceChildren(rulesCard, capsCard, proposalsCard);
	}
	rulesHint(rules) {
		if (rules.template === null) return "지금 쓰는 템플릿의 위험 규칙을 보여 줍니다.";
		return `지금 쓰는 템플릿 ${rules.template}번의 규칙입니다. 값은 템플릿 화면이나 승인한 개선안으로만 바뀝니다.`;
	}
	renderFailed() {
		const retry = button("다시 시도", "accent");
		retry.dataset.action = "retry";
		retry.addEventListener("click", () => {
			this.renderLoading();
			this.load();
		});
		const box = el("div", "rul-failed");
		box.append(errorLine(LOAD_FAILED), retry);
		this.body.replaceChildren(box);
	}
	onClick(event) {
		const target = event.target instanceof Element ? event.target.closest("button[data-action]") : null;
		const item = target?.closest("[data-proposal]");
		if (!target || !item) return;
		const action = target.dataset.action;
		if (action !== "approve" && action !== "reject") return;
		const row = this.rows.find((candidate) => String(candidate.id) === item.dataset.proposal);
		if (row) this.answer(row, action, item);
	}
	/** 개선안 하나에 답한다(번호 · 지문은 목록의 것 그대로). 답하는 동안 그 줄의 단추를 막는다. */
	async answer(row, answer, item) {
		if (this.busy || !row.canAnswer) return;
		this.busy = true;
		for (const node of item.querySelectorAll("button[data-action]")) node.disabled = true;
		try {
			const reply = await call("POST", API.proposalAnswer, {
				id: row.id,
				number: row.number,
				fingerprint: row.fingerprint,
				answer
			});
			this.notes.set(row.id, answerMessage({
				status: reply.status,
				body: reply.body
			}));
		} catch {
			this.notes.set(row.id, {
				kind: "failed",
				text: "서버와 연결하지 못했습니다. 잠시 뒤 다시 시도해 주세요."
			});
		}
		this.busy = false;
		await this.load();
	}
};
/**
* 본문 자리(`main`)에 규칙 화면을 그린다: 세션을 얻고 규칙 · 개선안 목록을 읽어 그린다.
* 첫 그림이 끝나면 풀린다. 자리가 없으면 아무것도 하지 않는다.
*/
async function mountRules(main) {
	if (!main) return;
	const screen = new RulesScreen(main);
	screen.renderLoading();
	try {
		await call("POST", API.session);
	} catch {}
	await screen.load();
}
// src/rules/main.ts
mountMenu(document.getElementById("menu"), "rules");
mountRules(document.getElementById("screen"));
