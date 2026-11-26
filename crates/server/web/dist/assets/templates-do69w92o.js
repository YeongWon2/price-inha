import { a as approvalPath, n as API, o as decidePath, s as templateStopPath, t as mountMenu } from "./menu-g4dweas1.js";
/* empty css               */
import { n as call, r as el, t as LOAD_FAILED } from "./call-enmqzayl.js";
import { a as errorLine, c as table, i as emptyState, n as button, r as card, s as loading, t as badge } from "./parts-ma3d1o7o.js";
import { a as isObject, i as formatValue, n as changeRow, o as numberLabel, r as fieldLabel, t as capText } from "./fields-j3bgfwvj.js";
// src/templates/model.ts
/** 스타일 고르기 단추 차례와 보이는 이름: 단타 · 스윙 · 중장기. */
var STYLES = [
	["day_trade", "단타"],
	["swing", "스윙"],
	["long_term", "중장기"]
];
/**
* 실전 안내(AGENTS 6번 · 결정 17 · E58): "실전 운용 시작" 단추 옆에 늘 보인다. 실전 운용을 시작해도
* 실제 증권사로 주문은 나가지 않는다는 뜻이다.
*/
var LIVE_NOTICE = "실전 운용을 시작해도 실제 증권사로 주문은 나가지 않습니다. 이 시스템의 주문은 언제나 직접 만든 모의계좌로만 갑니다.";
/**
* 값 맞추기 경로(`POST /api/templates/adjust`)가 받는 칸(서버 `change_of`와 같은 일곱 칸). 그 밖의 칸은 이 화면에서
* 보기만 한다(입력 칸은 있으나 읽기 전용).
*/
var ADJUSTABLE_KEYS = [
	"risk_per_trade_pct",
	"daily_loss_limit_pct",
	"max_drawdown_pct",
	"symbol_weight_pct",
	"sector_weight_pct",
	"min_cash_pct",
	"auto_order_limit_pct"
];
/** 이 화면에서 값을 보낼 수 있는 칸인가. */
function isAdjustable(key) {
	return ADJUSTABLE_KEYS.includes(key);
}
/** 상태 배지 글. */
var STATE_LABELS = {
	simulating: "모의중",
	promotion_pending: "승격대기",
	live: "실전중",
	stopped: "멈춤"
};
/** 승격 기준 이름. */
var CRITERION_LABELS = {
	trades: "매매 건수",
	trading_days: "거래일 수",
	net_pnl: "비용 뺀 손익",
	max_drawdown: "최대 낙폭",
	out_of_sample: "표본 밖 성과"
};
/** 응답에서 쓰는 템플릿(`current`의 번호). 없으면 없음. */
function currentTemplate(reply) {
	if (reply.current === null) return null;
	return reply.templates.find((entry) => entry.id === reply.current) ?? null;
}
/** 템플릿 하나 → 칸 표 줄(`fields` 순서 그대로). */
function fieldRows(entry) {
	return entry.fields.map((field) => ({
		key: field.key,
		label: fieldLabel(field.key),
		value: formatValue(field.key, field.value),
		cap: capText(field.cap),
		locked: !field.editable
	}));
}
/** 템플릿 하나 → 승격 기준 표 줄(`promotion` 순서 그대로). */
function promotionRows(entry) {
	return entry.promotion.map((line) => ({
		criterion: line.criterion,
		label: CRITERION_LABELS[line.criterion] ?? line.criterion,
		need: line.need,
		have: line.have ?? "아직 없음",
		met: line.met,
		short: !line.met
	}));
}
/** 템플릿 하나 → 판 이력 줄(`revisions` 순서 그대로). */
function revisionRows(entry) {
	return entry.revisions.map((revision) => ({
		revision: revision.revision,
		at: revision.at,
		proposal: revision.proposal,
		changes: revision.changes.map(changeRow)
	}));
}
/** 상태 글자 → 배지 글(모의중 · 승격대기 · 실전중 · 멈춤). */
function stateLabel(state) {
	return STATE_LABELS[state] ?? state;
}
/** 스타일 글자 → 보이는 이름. */
function styleLabel(style) {
	return STYLES.find(([name]) => name === style)?.[1] ?? style;
}
/** "실전 운용 시작"을 누를 수 있는가: 승격대기이고 승격 승인 번호가 있을 때만. */
function canPromote(entry) {
	return entry !== null && entry.state === "promotion_pending" && typeof entry.approval === "string" && entry.approval.length > 0;
}
/** "멈추기" 단추가 보이는가: 실전중일 때만. */
function canStop(entry) {
	return entry !== null && entry.state === "live";
}
/** 값 저장 결과 → 문구. 상한 안내는 `rows`의 상한으로 만든다. */
function saveMessage(reply, rows) {
	const body = isObject(reply.body) ? reply.body : {};
	if (reply.status === 200 && body.state === "needs_approval") {
		const number = typeof body.number === "string" ? body.number : "";
		return {
			kind: "needs_approval",
			number,
			text: `위험을 키우는 변경이라 승인을 기다립니다(승인 번호 ${number}). 승인 뒤 다음 거래일부터 적용됩니다.`
		};
	}
	if (reply.status === 200) {
		const revision = typeof body.revision === "number" ? body.revision : null;
		return {
			kind: "saved",
			text: revision === null ? "저장했습니다." : `저장했습니다 — 판 ${revision}.`
		};
	}
	if (reply.status === 400 && body.error === "over_cap") return {
		kind: "over_cap",
		fields: (Array.isArray(body.fields) ? body.fields.filter((key) => typeof key === "string") : []).map((key) => {
			return {
				key,
				text: rows.find((row) => row.key === key)?.cap ?? "허용 범위를 벗어났습니다"
			};
		}),
		text: "상한을 넘거나 허용 범위를 벗어난 값이 있어 저장하지 않았습니다. 표시된 칸의 상한 안으로 맞춰 주세요."
	};
	if (reply.status === 409) return {
		kind: "locked",
		text: "실전중에는 위험을 키우는 변경만 승인을 거쳐 바꿀 수 있습니다. 이 변경은 저장하지 않았습니다."
	};
	if (reply.status === 400) return {
		kind: "failed",
		text: "값을 읽을 수 없어 저장하지 않았습니다. 수를 넣어 주세요."
	};
	return {
		kind: "failed",
		text: "저장하지 못했습니다. 잠시 뒤 다시 시도해 주세요."
	};
}
// src/templates/view.ts
/** 글을 수로(쉼표 · `%` · 공백을 뺀다). 수가 아니면 NaN. */
function numberOf(text) {
	const cleaned = text.replace(/[,%\s]/g, "");
	return cleaned === "" ? NaN : Number(cleaned);
}
/** 칸 줄 하나: 이름 · 지금 값 · 상한 · 입력과 칸 옆 안내. */
function fieldRow(row) {
	const tr = document.createElement("tr");
	tr.dataset.key = row.key;
	const name = el("th", "tpl-name", row.label);
	name.scope = "row";
	const value = el("td", "tpl-value", row.value);
	value.dataset.role = "value";
	const cap = el("td", "tpl-cap", row.cap ?? "—");
	cap.dataset.role = "cap";
	if (row.cap === null) cap.setAttribute("aria-label", "상한 없음");
	const entry = el("td", "tpl-entry");
	const input = el("input", "ui-input tpl-input");
	input.type = "text";
	input.inputMode = "decimal";
	input.autocomplete = "off";
	input.disabled = row.locked;
	if (!isAdjustable(row.key)) {
		input.readOnly = true;
		input.tabIndex = -1;
		input.placeholder = "보기만";
		input.setAttribute("aria-label", `${row.label} — 이 화면에서는 보기만 합니다`);
	} else {
		input.placeholder = row.locked ? "잠김" : "새 값";
		input.setAttribute("aria-label", `${row.label} 새 값`);
	}
	const note = el("span", "tpl-note");
	note.dataset.role = "field-note";
	note.setAttribute("aria-live", "polite");
	entry.append(input, note);
	tr.append(name, value, cap, entry);
	return tr;
}
/** 칸 표의 묶음 머리 줄. */
function groupRow(title, hint) {
	const tr = el("tr", "tpl-group");
	const th = el("th", "");
	th.colSpan = 4;
	th.scope = "colgroup";
	th.append(el("span", "tpl-group-title", title), el("span", "tpl-group-hint", hint));
	tr.append(th);
	return tr;
}
/** 칸 표. 잠긴 칸의 입력은 막는다. */
function renderFields(rows) {
	const node = table([
		"항목",
		"지금 값",
		"상한",
		"새 값"
	], []);
	node.classList.add("tpl-fields");
	node.dataset.role = "fields";
	const risk = node.tBodies[0];
	risk.dataset.group = "risk";
	const other = node.createTBody();
	other.dataset.group = "other";
	const riskRows = rows.filter((row) => isAdjustable(row.key));
	const otherRows = rows.filter((row) => !isAdjustable(row.key));
	if (riskRows.length > 0) {
		risk.append(groupRow("위험 한도", "절대 상한 안에서 바꿀 수 있습니다"));
		for (const row of riskRows) risk.append(fieldRow(row));
	}
	if (otherRows.length > 0) {
		other.append(groupRow("운용 방식", "이 화면에서는 보기만 합니다"));
		for (const row of otherRows) other.append(fieldRow(row));
	}
	return node;
}
/** 칸 옆 안내를 모두 지우고 `notes`의 칸 옆에 글을 단다(상한 넘음 E5). */
function showFieldNotes(table, notes) {
	for (const tr of table.querySelectorAll("tr[data-key]")) {
		const note = tr.querySelector("[data-role=\"field-note\"]");
		const input = tr.querySelector("input");
		const found = notes.find((item) => item.key === tr.dataset.key);
		if (note) note.textContent = found ? found.text : "";
		if (input) {
			if (found) input.setAttribute("aria-invalid", "true");
			else input.removeAttribute("aria-invalid");
		}
		if (found) tr.dataset.invalid = "true";
		else delete tr.dataset.invalid;
	}
}
/** 칸 표의 입력에서 바꾼 값만 모은다(빈 칸 · 지금 값과 같은 칸은 뺀다). 키 → 십진수 글자. */
function changedValues(table, rows) {
	const changes = {};
	const lines = [...table.querySelectorAll("tr[data-key]")];
	for (const row of rows) {
		if (row.locked) continue;
		const input = lines.find((line) => line.dataset.key === row.key)?.querySelector("input");
		if (!input || input.disabled || input.readOnly) continue;
		const typed = input.value.trim().replace(/,/g, "").replace(/%$/, "").trim();
		if (typed === "") continue;
		const now = numberOf(row.value);
		const next = Number(typed);
		if (Number.isFinite(now) && Number.isFinite(next) && now === next) continue;
		changes[row.key] = typed;
	}
	return changes;
}
/** 승격 기준의 필요한 값 글(단위 · 비교). */
function needText(criterion, need) {
	const value = numberLabel(need);
	switch (criterion) {
		case "trades": return `${value}건 이상`;
		case "trading_days": return `${value}거래일 이상`;
		case "max_drawdown": return `${value}% 이하`;
		default: return `${value} 초과`;
	}
}
/** 승격 기준의 지금 값 글. */
function haveText(criterion, have) {
	if (!/^-?\d/.test(have)) return have;
	const value = numberLabel(have);
	switch (criterion) {
		case "trades": return `${value}건`;
		case "trading_days": return `${value}거래일`;
		case "max_drawdown": return `${value}%`;
		default: return value;
	}
}
/** 승격 기준 표. 모자란 줄은 `data-short="true"`. */
function renderPromotion(rows) {
	const node = table([
		"기준",
		"필요",
		"지금",
		"판정"
	], []);
	node.classList.add("tpl-promotion");
	node.dataset.role = "promotion";
	const body = node.tBodies[0];
	for (const row of rows) {
		const tr = document.createElement("tr");
		tr.dataset.criterion = row.criterion;
		tr.dataset.short = row.short ? "true" : "false";
		const name = el("th", "tpl-name", row.label);
		name.scope = "row";
		const verdict = el("td", "tpl-verdict");
		verdict.append(badge(row.met ? "채움" : "모자람", row.met ? "success" : "warning"));
		tr.append(name, el("td", "tpl-num", needText(row.criterion, row.need)), el("td", "tpl-num", haveText(row.criterion, row.have)), verdict);
		body.append(tr);
	}
	return node;
}
/** 유닉스 밀리초 → "2025. 11. 17. 09:40". */
function timeText(at) {
	return new Date(at).toLocaleString("ko-KR", {
		year: "numeric",
		month: "2-digit",
		day: "2-digit",
		hour: "2-digit",
		minute: "2-digit",
		hour12: false
	});
}
/** 판 이력. 판마다 바뀐 칸과 넓힘 표시. */
function renderRevisions(rows) {
	const list = el("ol", "tpl-revisions");
	list.dataset.role = "revisions";
	for (const row of rows) {
		const item = el("li", "tpl-revision");
		item.dataset.revision = String(row.revision);
		const head = el("div", "tpl-revision-head");
		head.append(el("strong", "tpl-revision-no", `판 ${row.revision}`));
		const time = el("time", "tpl-revision-at", timeText(row.at));
		time.dateTime = new Date(row.at).toISOString();
		head.append(time);
		if (row.proposal !== null) head.append(badge(`개선안 ${row.proposal}번`));
		item.append(head);
		if (row.changes.length === 0) item.append(el("p", "tpl-revision-empty", row.revision === 1 ? "처음 값(고른 스타일의 기본값)" : "바뀐 칸 없음"));
		else {
			const changes = el("ul", "tpl-changes");
			for (const change of row.changes) {
				const line = el("li", "tpl-change");
				line.dataset.key = change.key;
				if (change.widened) line.dataset.widened = "true";
				const values = el("span", "tpl-change-values");
				values.append(el("span", "tpl-before", change.before), el("span", "tpl-arrow", " → "), el("span", "tpl-after", change.after));
				line.append(el("span", "tpl-change-name", change.label), values);
				if (change.widened) line.append(badge("넓힘", "warning"));
				changes.append(line);
			}
			item.append(changes);
		}
		list.append(item);
	}
	return list;
}
/** 실전 안내 줄(`LIVE_NOTICE`). */
function liveNotice() {
	const node = el("p", "tpl-live-notice", LIVE_NOTICE);
	node.dataset.role = "live-notice";
	node.setAttribute("role", "note");
	return node;
}
// src/templates/screen.ts
/** 상태 배지 색. */
var STATE_TONES = {
	simulating: "neutral",
	promotion_pending: "warning",
	live: "success",
	stopped: "danger"
};
/** 결과 문구 줄. `tone`은 글의 색(토큰)만 바꾼다. */
function resultLine(role, message) {
	const node = el("p", "tpl-result");
	node.dataset.role = role;
	node.setAttribute("aria-live", "polite");
	if (message) {
		node.textContent = message.text;
		node.dataset.tone = message.kind;
	}
	return node;
}
/** 화면 하나의 상태와 그리기. */
var TemplatesScreen = class {
	body = el("div", "tpl-body");
	view = null;
	entry = null;
	rows = [];
	saveNote = null;
	actionNote = null;
	busy = false;
	constructor(main) {
		main.append(this.body);
	}
	/** 목록을 읽어 다시 그린다. 실패하면 오류 줄과 다시 시도. */
	async load() {
		try {
			const reply = await call("GET", API.templatesView);
			if (!reply.ok || typeof reply.body !== "object" || reply.body === null) throw new Error(`view ${reply.status}`);
			this.view = reply.body;
			this.entry = currentTemplate(this.view);
			this.rows = this.entry ? fieldRows(this.entry) : [];
			this.render();
		} catch {
			this.view = null;
			this.entry = null;
			this.rows = [];
			this.renderFailed();
		}
	}
	/** 첫 그림 전: 고르기 · 불러오는 중 · 실전 운용. */
	renderLoading() {
		this.body.replaceChildren(this.stylesCard(), loading(), this.liveCard());
	}
	renderFailed() {
		const retry = button("다시 시도", "accent");
		retry.dataset.action = "retry";
		retry.addEventListener("click", () => {
			retry.disabled = true;
			this.load();
		});
		const failed = el("div", "tpl-failed");
		failed.append(errorLine(LOAD_FAILED), retry);
		this.body.replaceChildren(failed, this.stylesCard(), this.liveCard());
	}
	render() {
		const parts = [this.stylesCard()];
		if (this.entry) parts.push(this.valuesCard(this.entry), this.promotionCard(this.entry));
		parts.push(this.liveCard());
		if (this.entry) parts.push(this.revisionsCard(this.entry));
		this.body.replaceChildren(...parts);
	}
	/** (가) 스타일 고르기와 지금 쓰는 템플릿. */
	stylesCard() {
		const choices = el("div", "tpl-styles");
		choices.dataset.role = "styles";
		choices.setAttribute("role", "group");
		choices.setAttribute("aria-label", "스타일 고르기");
		for (const [style, label] of STYLES) {
			const pick = button(label, this.entry?.style === style && this.entry.state !== "stopped" ? "accent" : "default");
			pick.dataset.style = style;
			pick.addEventListener("click", () => void this.choose(style, label));
			choices.append(pick);
		}
		const hint = el("p", "tpl-hint", "고르면 그 스타일의 기본값으로 새 템플릿을 만들고 모의 운용부터 시작합니다.");
		return card("스타일 고르기", hint, choices, this.currentLine());
	}
	/** 지금 쓰는 템플릿 줄(번호 · 스타일 · 판 · 상태 배지). */
	currentLine() {
		if (!this.entry) return this.view ? emptyState("아직 고른 템플릿이 없습니다. 위에서 스타일을 고르세요.") : el("span", "");
		const line = el("div", "tpl-current");
		line.append(el("span", "tpl-current-label", "지금 쓰는 템플릿"));
		line.append(el("strong", "tpl-current-name", `${styleLabel(this.entry.style)} · ${this.entry.id}번 · 판 ${this.entry.revision}`));
		const state = badge(stateLabel(this.entry.state), STATE_TONES[this.entry.state]);
		state.dataset.role = "state";
		line.append(state);
		return line;
	}
	/** (나) 값 맞추기. */
	valuesCard(entry) {
		const parts = [];
		if (entry.state === "live") {
			const notice = el("p", "tpl-notice", "실전중 템플릿입니다 — 값을 바꾸면 승인이 필요합니다. 위험을 키우는 변경은 승인 뒤 다음 거래일부터 적용됩니다.");
			notice.dataset.role = "approval-notice";
			notice.dataset.tone = "warning";
			parts.push(notice);
		} else if (entry.state === "promotion_pending" || entry.state === "stopped") {
			const why = entry.state === "stopped" ? "멈춘 템플릿은 값이 잠겨 있습니다. 바꾸려면 스타일을 새로 고르세요." : "승격대기 중에는 기준을 통과한 값이 잠겨 있습니다.";
			const notice = el("p", "tpl-notice", why);
			notice.dataset.role = "locked-notice";
			parts.push(notice);
		}
		const fields = renderFields(this.rows);
		const wrap = el("div", "tpl-table-wrap");
		wrap.append(fields);
		parts.push(wrap);
		const save = button("저장", "accent");
		save.dataset.action = "save";
		save.disabled = this.rows.every((row) => row.locked);
		const result = resultLine("save-result", this.saveNote);
		save.addEventListener("click", () => void this.save(entry, fields, result, save));
		fields.addEventListener("keydown", (event) => {
			if (event.key === "Enter" && event.target instanceof HTMLInputElement) save.click();
		});
		const actions = el("div", "tpl-actions");
		actions.append(save, result);
		parts.push(actions);
		return card("값 맞추기", ...parts);
	}
	/** (다) 승격 기준 표. */
	promotionCard(entry) {
		const rows = promotionRows(entry);
		const short = rows.filter((row) => row.short).length;
		const lead = el("p", "tpl-hint", short === 0 ? "모든 기준을 채웠습니다." : `모자란 기준 ${short}개 — 모두 채우면 실전 승격 승인을 요청합니다.`);
		const wrap = el("div", "tpl-table-wrap");
		wrap.append(renderPromotion(rows));
		return card("승격 기준", lead, wrap);
	}
	/** (마 · 바) 실전 운용 시작 · 멈추기와 늘 보이는 실전 안내. */
	liveCard() {
		const promote = button("실전 운용 시작", "accent");
		promote.dataset.action = "promote";
		promote.disabled = !canPromote(this.entry);
		promote.addEventListener("click", () => void this.promote(promote));
		const actions = el("div", "tpl-actions");
		actions.append(promote);
		if (canStop(this.entry)) {
			const stop = button("멈추기", "danger");
			stop.dataset.action = "stop";
			stop.addEventListener("click", () => void this.stop(stop));
			actions.append(stop);
		}
		const hint = el("p", "tpl-hint", this.promoteHint());
		return card("실전 운용", hint, actions, liveNotice(), resultLine("action-result", this.actionNote));
	}
	promoteHint() {
		const entry = this.entry;
		if (!entry) return "템플릿을 고르고 승격 기준을 모두 채우면 실전 운용을 시작할 수 있습니다.";
		switch (entry.state) {
			case "promotion_pending": return entry.approval ? `승격 기준을 모두 채웠습니다. 승인 번호 ${entry.approval}로 실전 운용을 시작할 수 있습니다.` : "승격 승인 요청을 기다리는 중입니다.";
			case "live": return "실전 운용 중입니다. 멈추면 다시 실전으로 돌아갈 수 없고 스타일을 새로 골라야 합니다.";
			case "stopped": return "멈춘 템플릿입니다. 다시 운용하려면 스타일을 새로 고르세요.";
			default: return "모의 운용 중입니다. 승격 기준을 모두 채우면 실전 운용을 시작할 수 있습니다.";
		}
	}
	/** (라) 판 이력. */
	revisionsCard(entry) {
		return card("판 이력", renderRevisions(revisionRows(entry)));
	}
	async choose(style, label) {
		if (this.busy) return;
		this.busy = true;
		try {
			const reply = await call("POST", API.templates, { style });
			this.saveNote = null;
			this.actionNote = reply.ok ? null : {
				kind: "failed",
				text: `${label} 템플릿을 만들지 못했습니다. 잠시 뒤 다시 시도해 주세요.`
			};
		} catch {
			this.actionNote = {
				kind: "failed",
				text: `${label} 템플릿을 만들지 못했습니다. 서버와 연결을 확인해 주세요.`
			};
		} finally {
			this.busy = false;
		}
		await this.load();
	}
	async save(entry, fields, result, save) {
		if (this.busy) return;
		const changes = changedValues(fields, this.rows);
		if (Object.keys(changes).length === 0) {
			showFieldNotes(fields, []);
			this.show(result, {
				kind: "failed",
				text: "바꾼 값이 없습니다. 새 값 칸에 수를 넣은 뒤 저장하세요."
			});
			return;
		}
		this.busy = true;
		save.disabled = true;
		let message;
		try {
			const reply = await call("POST", API.templatesAdjust, {
				id: entry.id,
				changes
			});
			message = saveMessage({
				status: reply.status,
				body: reply.body
			}, this.rows);
		} catch {
			message = {
				kind: "failed",
				text: "저장하지 못했습니다. 서버와 연결을 확인해 주세요."
			};
		}
		this.busy = false;
		save.disabled = false;
		showFieldNotes(fields, message.kind === "over_cap" ? message.fields : []);
		this.show(result, message);
		if (message.kind === "saved") {
			this.saveNote = message;
			await this.load();
		}
	}
	show(node, message) {
		node.textContent = message.text;
		node.dataset.tone = message.kind;
	}
	async promote(promote) {
		const number = this.entry?.approval;
		if (this.busy || !number) return;
		this.busy = true;
		promote.disabled = true;
		try {
			const read = await call("GET", approvalPath(number));
			const shown = read.body ?? {};
			if (!read.ok || typeof shown.fingerprint !== "string") this.actionNote = {
				kind: "failed",
				text: "승인 카드를 읽지 못했습니다. 이미 처리됐거나 기한이 지났을 수 있습니다."
			};
			else {
				const summary = typeof shown.summary === "string" ? shown.summary : "실전 승격";
				if (!confirm(`실전 운용을 시작할까요?\n\n${summary}\n승인 번호 ${number}\n\n실전 운용을 시작해도 실제 증권사로 주문은 나가지 않습니다. 이 시스템의 주문은 언제나 직접 만든 모의계좌로만 갑니다.`)) {
					this.busy = false;
					promote.disabled = false;
					return;
				}
				const answer = await call("POST", decidePath(number, "approve"), { fingerprint: shown.fingerprint });
				const body = answer.body ?? {};
				this.actionNote = answer.ok && body.result === "approved" ? {
					kind: "saved",
					text: "승인했습니다. 실전 운용을 시작합니다(실제 증권사 주문은 나가지 않습니다)."
				} : {
					kind: "failed",
					text: `승인되지 않았습니다${typeof body.reason === "string" ? `(${body.reason})` : ""}.`
				};
			}
		} catch {
			this.actionNote = {
				kind: "failed",
				text: "승인하지 못했습니다. 서버와 연결을 확인해 주세요."
			};
		}
		this.busy = false;
		await this.load();
	}
	async stop(stop) {
		const entry = this.entry;
		if (this.busy || !entry) return;
		if (!confirm(`${entry.id}번 템플릿의 실전 운용을 멈출까요?\n멈춘 뒤에는 다시 실전으로 돌아갈 수 없고, 스타일을 새로 골라야 합니다.`)) return;
		this.busy = true;
		stop.disabled = true;
		try {
			const reply = await call("POST", templateStopPath(entry.id));
			this.actionNote = reply.ok ? {
				kind: "saved",
				text: "실전 운용을 멈췄습니다."
			} : {
				kind: "failed",
				text: "멈추지 못했습니다. 이미 실전중이 아닐 수 있습니다."
			};
		} catch {
			this.actionNote = {
				kind: "failed",
				text: "멈추지 못했습니다. 서버와 연결을 확인해 주세요."
			};
		}
		this.busy = false;
		await this.load();
	}
};
/**
* 본문 자리(`main`)에 템플릿 화면을 그린다: 세션을 얻고(`POST /api/session`) 목록을 읽어 그린다.
* 첫 그림이 끝나면 풀린다. 자리가 없으면 아무것도 하지 않는다.
*/
async function mountTemplates(main) {
	if (!main) return;
	const screen = new TemplatesScreen(main);
	screen.renderLoading();
	try {
		await call("POST", API.session);
	} catch {}
	await screen.load();
}
// src/templates/main.ts
mountMenu(document.getElementById("menu"), "templates");
mountTemplates(document.getElementById("screen"));
