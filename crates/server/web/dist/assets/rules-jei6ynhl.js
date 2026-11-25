import { n as API, t as mountMenu } from "./menu-ebudw6pk.js";
/* empty css               */
import { n as call, r as el, t as LOAD_FAILED } from "./call-enmqzayl.js";
import { a as errorLine, c as table, i as emptyState, n as button, r as card, s as loading, t as badge } from "./parts-ma3d1o7o.js";
import { a as isObject$1, i as formatValue, n as changeRow, r as fieldLabel, t as capText } from "./fields-j3bgfwvj.js";
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
	const body = isObject$1(reply.body) ? reply.body : {};
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
/** 이 컴퓨터가 아니면 보이는 안내 글. */
var READONLY_TEXT = "이 컴퓨터에서만 바꿀 수 있습니다";
function isObject(value) {
	return typeof value === "object" && value !== null && !Array.isArray(value);
}
function bannedRow(value) {
	if (!isObject(value)) return null;
	const { market, symbol } = value;
	if (market !== "kr" && market !== "us" || typeof symbol !== "string") return null;
	return {
		market,
		symbol
	};
}
function sectorRow(value) {
	if (!isObject(value)) return null;
	const { symbol, sector } = value;
	if (typeof symbol !== "string" || typeof sector !== "string") return null;
	return {
		symbol,
		sector
	};
}
/** 배열의 줄마다 확인한다. 하나라도 틀리면 없음. */
function rowsOf(value, row) {
	if (!Array.isArray(value)) return null;
	const rows = [];
	for (const item of value) {
		const checked = row(item);
		if (checked === null) return null;
		rows.push(checked);
	}
	return rows;
}
/**
* 읽기 응답 몸통을 확인해 값으로 바꾼다. 모양이 맞지 않으면(칸이 없거나 종류가 다름) 없음.
* 모르는 칸은 버린다.
*/
function readConfig(body) {
	if (!isObject(body) || typeof body.editable !== "boolean") return null;
	const banned = rowsOf(body.banned, bannedRow);
	const sectors = rowsOf(body.sectors, sectorRow);
	const unsectored = rowsOf(body.unsectored, (item) => typeof item === "string" ? item : null);
	if (!banned || !sectors || !unsectored) return null;
	return {
		banned,
		sectors,
		editable: body.editable,
		unsectored
	};
}
/** 종목 글자인가(앞뒤 공백을 뗀 뒤 영문 · 숫자 · `.` · `-` 1~32자). */
function isSymbol(text) {
	return /^[A-Za-z0-9.-]{1,32}$/.test(text.trim());
}
/** 읽은 값에서 초안을 만든다(줄을 복사한다 — 읽은 값을 바꾸지 않는다). */
function draftOf(reply) {
	return {
		banned: reply.banned.map((row) => ({ ...row })),
		sectors: reply.sectors.map((row) => ({ ...row }))
	};
}
/**
* 금지 종목 한 줄을 끝에 더한다(종목은 앞뒤 공백을 뗀다). 시장이 `kr` · `us`가 아니거나 종목 글자가 아니거나
* 같은 (시장, 종목)이 이미 있으면 더하지 않고 까닭을 준다. 받은 초안은 바꾸지 않는다.
*/
function addBanned(draft, market, symbol) {
	const code = symbol.trim();
	if (market !== "kr" && market !== "us") return {
		ok: false,
		problem: "시장은 국내 · 미국 가운데 고릅니다"
	};
	if (!isSymbol(code)) return {
		ok: false,
		problem: "종목은 영문 · 숫자 · 점 · 줄표 1~32자입니다"
	};
	if (draft.banned.some((row) => row.market === market && row.symbol === code)) return {
		ok: false,
		problem: "이미 금지 종목에 있습니다"
	};
	return {
		ok: true,
		draft: {
			...draft,
			banned: [...draft.banned, {
				market,
				symbol: code
			}]
		}
	};
}
/** 그 차례의 금지 종목 줄을 뺀 새 초안(차례 밖이면 같은 줄들). */
function removeBanned(draft, index) {
	return {
		...draft,
		banned: draft.banned.filter((_, at) => at !== index)
	};
}
/**
* 업종표 한 줄을 끝에 더한다(종목 · 업종 이름은 앞뒤 공백을 뗀다). 종목 글자가 아니거나, 업종 이름이 비었거나
* [`SECTOR_MAX_CHARS`]자를 넘거나, 같은 종목이 이미 있으면 더하지 않고 까닭을 준다. 받은 초안은 바꾸지 않는다.
*/
function addSector(draft, symbol, sector) {
	const code = symbol.trim();
	const name = sector.trim();
	const length = [...name].length;
	if (!isSymbol(code)) return {
		ok: false,
		problem: "종목은 영문 · 숫자 · 점 · 줄표 1~32자입니다"
	};
	if (length < 1 || length > 40) return {
		ok: false,
		problem: `업종 이름은 1~40자입니다`
	};
	if (draft.sectors.some((row) => row.symbol === code)) return {
		ok: false,
		problem: "이미 업종이 있는 종목입니다"
	};
	return {
		ok: true,
		draft: {
			...draft,
			sectors: [...draft.sectors, {
				symbol: code,
				sector: name
			}]
		}
	};
}
/** 그 차례의 업종표 줄을 뺀 새 초안(차례 밖이면 같은 줄들). */
function removeSector(draft, index) {
	return {
		...draft,
		sectors: draft.sectors.filter((_, at) => at !== index)
	};
}
/** 저장 요청 몸통 `{"banned": [{market, symbol}], "sectors": [{symbol, sector}]}`(초안의 차례 그대로, 다른 칸 없음). */
function saveBody(draft) {
	return {
		banned: draft.banned.map(({ market, symbol }) => ({
			market,
			symbol
		})),
		sectors: draft.sectors.map(({ symbol, sector }) => ({
			symbol,
			sector
		}))
	};
}
/**
* 업종 없는 종목 경고 줄: `unsectored`가 있으면 "업종이 없어 살 수 없는 종목: 005930, 000660"(응답 차례, `", "`로
* 잇는다), 없으면 없음.
*/
function unsectoredLine(reply) {
	if (reply.unsectored.length === 0) return null;
	return `업종이 없어 살 수 없는 종목: ${reply.unsectored.join(", ")}`;
}
/**
* 저장 결과 → 문구. 200이면 저장함("저장했습니다"가 든다, 다음 주문 전 점검부터 쓰인다는 뜻), 403이면 실패이고
* [`READONLY_TEXT`]가 든다, 400이면 실패이고 "올바르지 않"이 든다, 그 밖(5xx 등 · 연결 실패는 `status` 0)은
* 실패이고 "다시 시도"가 든다.
*/
function saveMessage(status, body) {
	if (status === 200) return {
		kind: "saved",
		text: "저장했습니다. 다음 주문 전 점검부터 씁니다"
	};
	if (status === 403) return {
		kind: "failed",
		text: `${READONLY_TEXT}`
	};
	if (status === 400) return {
		kind: "failed",
		text: "값이 올바르지 않아 저장하지 않았습니다"
	};
	return {
		kind: "failed",
		text: "저장하지 못했습니다. 잠시 뒤 다시 시도하세요"
	};
}
// src/rules/config-view.ts
/** 카드 제목. */
var TITLE = "금지 종목 · 업종표";
/** 시장 글. */
var MARKET_TEXT = {
	kr: "국내",
	us: "미국"
};
/** `data-role`을 단 요소. */
function marked(node, role) {
	node.dataset.role = role;
	return node;
}
/** 지우기 단추 칸. */
function removeCell(action, label, onClick) {
	const cell = el("td", "rul-cfg-act");
	const remove = button("지우기", "danger");
	remove.dataset.action = action;
	remove.setAttribute("aria-label", label);
	remove.addEventListener("click", onClick);
	cell.append(remove);
	return cell;
}
/** 금지 종목 표(비었으면 빈 상태 글). */
function bannedTable(state, handlers) {
	const rows = state.draft.banned;
	if (rows.length === 0) return marked(emptyState("금지 종목이 없습니다."), "no-banned");
	const editable = state.reply.editable;
	const node = marked(table(editable ? [
		"시장",
		"종목",
		""
	] : ["시장", "종목"], []), "banned");
	node.classList.add("rul-cfg-table");
	rows.forEach((row, index) => {
		const tr = node.tBodies[0].insertRow();
		tr.dataset.banned = `${row.market}:${row.symbol}`;
		tr.append(el("td", "rul-cfg-market", MARKET_TEXT[row.market] ?? row.market), el("td", "rul-cfg-symbol", row.symbol));
		if (editable) tr.append(removeCell("remove-banned", `${row.symbol} 금지 풀기`, () => handlers.removeBanned(index)));
	});
	return node;
}
/** 업종표(비었으면 빈 상태 글). */
function sectorTable(state, handlers) {
	const rows = state.draft.sectors;
	if (rows.length === 0) return marked(emptyState("업종표가 비어 있습니다. 업종을 모르는 종목은 사지 않습니다."), "no-sectors");
	const editable = state.reply.editable;
	const node = marked(table(editable ? [
		"종목",
		"업종",
		""
	] : ["종목", "업종"], []), "sectors");
	node.classList.add("rul-cfg-table");
	rows.forEach((row, index) => {
		const tr = node.tBodies[0].insertRow();
		tr.dataset.sector = row.symbol;
		tr.append(el("td", "rul-cfg-symbol", row.symbol), el("td", "rul-cfg-sector", row.sector));
		if (editable) tr.append(removeCell("remove-sector", `${row.symbol} 업종 지우기`, () => handlers.removeSector(index)));
	});
	return node;
}
/** 이름 붙은 입력 칸(라벨과 함께). */
function labeled(text, control, name) {
	control.name = name;
	control.id = `rul-cfg-${name}`;
	control.classList.add("ui-input");
	const label = el("label", "ui-field-label", text);
	label.htmlFor = control.id;
	const box = el("div", "ui-field rul-cfg-field");
	box.append(label, control);
	return box;
}
function textInput(placeholder) {
	const input = el("input", "");
	input.type = "text";
	input.autocomplete = "off";
	input.placeholder = placeholder;
	return input;
}
/** 금지 종목 더하기 줄. */
function bannedForm(handlers) {
	const market = el("select", "");
	for (const [value, text] of Object.entries(MARKET_TEXT)) {
		const option = el("option", "", text);
		option.value = value;
		market.append(option);
	}
	const symbol = textInput("예: 005930");
	const add = button("금지 종목 더하기");
	add.dataset.action = "add-banned";
	add.addEventListener("click", () => handlers.addBanned(market.value, symbol.value));
	const form = el("div", "rul-cfg-form");
	form.append(labeled("시장", market, "banned-market"), labeled("종목", symbol, "banned-symbol"), add);
	return form;
}
/** 업종표 더하기 줄. */
function sectorForm(handlers) {
	const symbol = textInput("예: 000660");
	const name = textInput("예: 반도체");
	const add = button("업종 더하기");
	add.dataset.action = "add-sector";
	add.addEventListener("click", () => handlers.addSector(symbol.value, name.value));
	const form = el("div", "rul-cfg-form");
	form.append(labeled("종목", symbol, "sector-symbol"), labeled("업종", name, "sector-name"), add);
	return form;
}
/** 저장 단추와 결과 줄(고칠 수 있을 때). */
function saveRow(state, handlers) {
	const save = button(state.saving ? "저장하는 중" : "저장", "accent");
	save.dataset.action = "save-config";
	save.disabled = state.saving;
	save.addEventListener("click", () => handlers.save());
	const row = el("div", "rul-cfg-save");
	row.append(save);
	if (state.saved?.kind === "saved") row.append(marked(el("p", "rul-cfg-result", state.saved.text), "config-result"));
	if (state.saved?.kind === "failed") row.append(marked(errorLine(state.saved.text), "config-error"));
	return row;
}
/** 소제목. */
function heading(text) {
	return el("h3", "rul-cfg-heading", text);
}
/** 금지 종목 · 업종표 묶음(카드 하나)을 그린다. */
function renderConfig(state, handlers) {
	const editable = state.reply.editable;
	const parts = [el("p", "rul-hint", "금지 종목은 사지 않고, 업종표에 없는 종목도 사지 않습니다. 저장하면 다음 주문 전 점검부터 씁니다.")];
	const warning = unsectoredLine(state.reply);
	if (warning !== null) parts.push(marked(el("p", "rul-cfg-warn", warning), "unsectored"));
	parts.push(heading("금지 종목"), bannedTable(state, handlers));
	if (editable) parts.push(bannedForm(handlers));
	parts.push(heading("업종표"), sectorTable(state, handlers));
	if (editable) parts.push(sectorForm(handlers));
	if (editable && state.problem !== null) {
		const problem = marked(el("p", "rul-cfg-problem", state.problem), "config-problem");
		problem.setAttribute("role", "status");
		parts.push(problem);
	}
	parts.push(editable ? saveRow(state, handlers) : marked(el("p", "rul-cfg-readonly", READONLY_TEXT), "config-readonly"));
	return marked(card(TITLE, ...parts), "rules-config");
}
/** 이 묶음만 읽지 못했을 때의 묶음(카드 하나, `[data-role="config-load-failed"]`). */
function renderConfigFailed() {
	const note = marked(el("p", "rul-cfg-failed", "금지 종목 · 업종표를 불러오지 못했습니다. 잠시 뒤 화면을 다시 열어 주세요."), "config-load-failed");
	return marked(card(TITLE, note), "rules-config");
}
// src/rules/config-panel.ts
/** 더하기 입력 칸 이름(다시 그릴 때 적던 글을 지키거나 비운다). */
var BANNED_INPUTS = ["banned-market", "banned-symbol"];
var SECTOR_INPUTS = ["sector-symbol", "sector-name"];
/** 금지 종목 · 업종표 묶음. [`ConfigPanel.element`]를 화면에 붙이고, 고치면 그 안만 다시 그린다. */
var ConfigPanel = class {
	holder = el("div", "rul-cfg-holder");
	state = null;
	handlers = {
		addBanned: (market, symbol) => this.add(addBanned(this.draft(), market, symbol), BANNED_INPUTS),
		removeBanned: (index) => this.edit(() => removeBanned(this.draft(), index)),
		addSector: (symbol, sector) => this.add(addSector(this.draft(), symbol, sector), SECTOR_INPUTS),
		removeSector: (index) => this.edit(() => removeSector(this.draft(), index)),
		save: () => void this.save()
	};
	/** 화면에 붙일 자리(늘 같은 요소). */
	element() {
		return this.holder;
	}
	/** 지금 값을 읽어 초안을 새로 만든다. 못 읽으면 이 묶음만 못 읽음. */
	async load() {
		let reply = null;
		try {
			const answer = await call("GET", API.rulesConfig);
			reply = answer.ok ? readConfig(answer.body) : null;
		} catch {
			reply = null;
		}
		this.state = reply ? {
			reply,
			draft: draftOf(reply),
			problem: null,
			saved: null,
			saving: false
		} : null;
		this.render();
	}
	draft() {
		if (!this.state) throw new Error("읽기 전에는 고치지 않는다");
		return this.state.draft;
	}
	edit(next) {
		if (!this.state || this.state.saving) return;
		this.state = {
			...this.state,
			draft: next(),
			problem: null,
			saved: null
		};
		this.render();
	}
	/** 줄 더하기: 되면 그 입력 칸을 비우고, 안 되면 까닭을 보이고 적던 글을 남긴다. */
	add(result, inputs) {
		if (!this.state || this.state.saving) return;
		const typed = this.typed();
		if (result.ok) {
			this.state = {
				...this.state,
				draft: result.draft,
				problem: null,
				saved: null
			};
			for (const name of inputs) if (!name.endsWith("market")) typed.delete(name);
		} else this.state = {
			...this.state,
			problem: result.problem,
			saved: null
		};
		this.render(typed);
	}
	/** 초안을 통째로 보낸다. 200이면 서버가 답한 값으로 다시 그리고, 실패면 오류 줄 — 초안은 그대로다. */
	async save() {
		if (!this.state || this.state.saving) return;
		const body = saveBody(this.state.draft);
		this.state = {
			...this.state,
			saving: true,
			problem: null,
			saved: null
		};
		this.render(this.typed());
		let status = 0;
		let answer = null;
		try {
			const reply = await call("POST", API.rulesConfig, body);
			status = reply.status;
			answer = reply.body;
		} catch {
			status = 0;
		}
		const message = saveMessage(status, answer);
		const read = status === 200 ? readConfig(answer) : null;
		const next = read ? {
			reply: read,
			draft: draftOf(read)
		} : {};
		this.state = {
			...this.state,
			...next,
			saving: false,
			saved: message
		};
		this.render(this.typed());
	}
	/** 지금 적혀 있는 더하기 입력 칸의 글. */
	typed() {
		const typed = /* @__PURE__ */ new Map();
		for (const node of this.holder.querySelectorAll("input[name], select[name]")) typed.set(node.name, node.value);
		return typed;
	}
	render(typed = /* @__PURE__ */ new Map()) {
		const card = this.state ? renderConfig(this.state, this.handlers) : renderConfigFailed();
		for (const [name, value] of typed) {
			const node = card.querySelector(`[name="${name}"]`);
			if (node) node.value = value;
		}
		this.holder.replaceChildren(card);
	}
};
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
	/** 금지 종목 · 업종표 묶음(읽기 · 초안 · 저장). */
	config = new ConfigPanel();
	constructor(main) {
		main.append(this.body);
	}
	renderLoading() {
		this.body.replaceChildren(loading());
	}
	/**
	* 규칙 · 개선안 목록을 읽어 다시 그린다. 하나라도 실패하면 오류 줄과 다시 시도. `withConfig`면 금지 종목 ·
	* 업종표도 다시 읽는다(그 묶음의 실패는 그 묶음 안에만 보인다).
	*/
	async load(withConfig = true) {
		try {
			const configRead = withConfig ? this.config.load() : Promise.resolve();
			const [rules, proposals] = await Promise.all([
				call("GET", API.rules),
				call("GET", API.proposals),
				configRead
			]);
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
		this.body.replaceChildren(rulesCard, capsCard, this.config.element(), proposalsCard);
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
		await this.load(false);
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
