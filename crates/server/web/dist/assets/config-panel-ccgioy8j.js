import { n as API } from "./menu-kc5ybrux.js";
import { n as call, r as el } from "./call-enmqzayl.js";
import { a as errorLine, c as table, i as emptyState, n as button, r as card } from "./parts-ma3d1o7o.js";
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
export { ConfigPanel };
