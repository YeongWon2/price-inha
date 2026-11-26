import { n as API, s as guidelineRetirePath } from "./menu-ixz3ji1b.js";
import { n as call, r as el } from "./call-enmqzayl.js";
import { i as emptyState, n as button, r as card, t as badge } from "./parts-ma3d1o7o.js";
// src/rules/guide-model.ts
/** 묶음 제목. */
var GUIDE_TITLE = "팀 지침(AI가 배운 것)";
/** 1층 규칙과 다르다는 한 줄. */
var LAYER_NOTE = "AI가 복기 · 실수 · 결정 메모에서 스스로 배운 참고입니다. 위의 규칙과 달리 승인 없이 바뀌고, 주문은 언제나 위의 규칙 검사를 그대로 거칩니다.";
/** 종류의 쉬운 말. */
var KIND_LABEL = {
	avoid: "피하기",
	enter: "들어가기",
	manage: "운용"
};
/** 응답을 읽는다. 모양이 다르면 `null`. */
function readGuidelines(body) {
	if (typeof body !== "object" || body === null) return null;
	const { active, retired, editable } = body;
	if (!Array.isArray(active) || !Array.isArray(retired) || typeof editable !== "boolean") return null;
	if (![...active, ...retired].every(isRow)) return null;
	return {
		active,
		retired,
		editable
	};
}
/** 지침 줄 모양인가(화면이 쓰는 칸만 본다). */
function isRow(item) {
	if (typeof item !== "object" || item === null) return false;
	const row = item;
	return typeof row.id === "string" && (row.kind === "avoid" || row.kind === "enter" || row.kind === "manage") && typeof row.text === "string" && typeof row.when === "string" && typeof row.uses === "number" && typeof row.wins === "number" && (row.win_rate_pct === null || typeof row.win_rate_pct === "number");
}
/** 익절 비율 글. 아직 끝난 매매가 없으면 그렇다고 쓴다. */
function rateText(row) {
	if (row.win_rate_pct === null || row.uses === 0) return "아직 끝난 매매 없음";
	return `익절 ${row.win_rate_pct}% (${row.uses}번 중 ${row.wins}번)`;
}
/** 쓰임 글. */
function usesText(row) {
	return `쓰임 ${row.uses}번`;
}
/** 내린 까닭(서버 기계 글자) → 쉬운 말. */
var RETIRED_WHY = {
	merged: "겹치는 지침과 합쳤습니다",
	poor_win_rate: "익절 비율이 낮아 내렸습니다",
	unused: "오래 쓰이지 않아 내렸습니다",
	blocking_trades: "매매를 너무 막아 내렸습니다",
	person: "사람이 끔"
};
/** 내린 까닭 글. */
function whyText(why) {
	if (why === null) return "";
	return RETIRED_WHY[why] ?? why;
}
/** 끄기 응답(상태 · 본문)을 결과 글로 바꾼다. 상태 0은 연결 실패다. */
function retireMessage(status, body) {
	if (status === 200) return {
		kind: "ok",
		text: "지침을 껐습니다."
	};
	if (status === 0) return {
		kind: "failed",
		text: "서버와 연결하지 못했습니다. 잠시 뒤 다시 시도해 주세요."
	};
	if (status === 403) return {
		kind: "failed",
		text: "지침은 이 컴퓨터에서만 끌 수 있습니다."
	};
	if (status === 404) return {
		kind: "failed",
		text: "그 지침이 없습니다. 목록을 다시 읽었습니다."
	};
	return {
		kind: "failed",
		text: "지침을 끄지 못했습니다. 잠시 뒤 다시 시도해 주세요."
	};
}
// src/rules/guide-panel.ts
var KIND_TONE = {
	avoid: "warning",
	enter: "success",
	manage: "neutral"
};
/** `data-role`을 단 요소. */
function marked(tag, className, role, text) {
	const node = el(tag, className, text);
	node.dataset.role = role;
	return node;
}
/** 지침 한 줄(활성 · 내린 것 공통). */
function guideItem(row, retireButton) {
	const item = el("li", "rul-guide-item");
	item.dataset.guideline = row.id;
	const kind = badge(KIND_LABEL[row.kind], KIND_TONE[row.kind]);
	kind.dataset.role = "kind";
	const head = el("div", "rul-guide-head");
	head.append(kind, marked("span", "rul-guide-text", "text", row.text));
	const where = row.symbol ? ` · 종목 ${row.symbol}` : "";
	const stats = el("p", "rul-guide-stats");
	stats.append(marked("span", "", "rate", rateText(row)), marked("span", "", "uses", usesText(row)));
	item.append(head, marked("p", "rul-guide-when", "when", `언제: ${row.when}${where}`), stats);
	if (row.state === "retired") item.append(marked("p", "rul-guide-why", "why", whyText(row.retired_why)));
	if (retireButton) {
		const off = button("끄기");
		off.dataset.action = "retire-guideline";
		item.append(off);
	}
	return item;
}
/** 팀 지침 묶음. [`GuidePanel.element`]를 화면에 붙이고, 읽거나 끄면 그 안만 다시 그린다. */
var GuidePanel = class {
	holder = el("div", "rul-guide-holder");
	reply = null;
	result = null;
	busy = false;
	constructor() {
		this.holder.dataset.role = "guidelines";
		this.holder.addEventListener("click", (event) => this.onClick(event));
	}
	/** 화면에 붙일 자리(늘 같은 요소). */
	element() {
		return this.holder;
	}
	/** 지금 지침을 읽어 다시 그린다. 못 읽으면 이 묶음만 못 읽음. */
	async load() {
		try {
			const answer = await call("GET", API.guidelines);
			this.reply = answer.ok ? readGuidelines(answer.body) : null;
		} catch {
			this.reply = null;
		}
		this.render();
	}
	render() {
		const body = [marked("p", "rul-guide-note", "guidelines-note", LAYER_NOTE)];
		if (this.reply) body.push(...this.listParts(this.reply));
		else {
			const retry = button("다시 시도");
			retry.dataset.action = "retry-guidelines";
			body.push(marked("p", "rul-guide-failed", "guidelines-load-failed", "팀 지침을 불러오지 못했습니다. 잠시 뒤 다시 시도해 주세요."), retry);
		}
		if (this.result) {
			const line = marked("p", `rul-guide-result rul-guide-${this.result.kind}`, "guidelines-result", this.result.text);
			body.push(line);
		}
		this.holder.replaceChildren(card(GUIDE_TITLE, ...body));
	}
	/** 활성 목록 · 원격 안내 · 내린 것 접기. */
	listParts(reply) {
		const parts = [];
		if (reply.active.length === 0) {
			const empty = emptyState("아직 배운 지침이 없습니다. 복기 · 실수 · 결정 메모에서 같은 것이 되풀이되면 생깁니다.");
			empty.dataset.role = "no-guidelines";
			parts.push(empty);
		} else {
			const list = marked("ul", "rul-guide-list", "guidelines-active");
			list.append(...reply.active.map((row) => guideItem(row, reply.editable)));
			parts.push(list);
		}
		if (!reply.editable) parts.push(marked("p", "rul-guide-readonly", "guidelines-readonly", "지침 끄기는 이 컴퓨터에서만 할 수 있습니다."));
		if (reply.retired.length > 0) {
			const folded = marked("details", "rul-guide-retired", "guidelines-retired");
			const list = el("ul", "rul-guide-list");
			list.append(...reply.retired.map((row) => guideItem(row, false)));
			folded.append(el("summary", "rul-guide-summary", `내린 지침 ${reply.retired.length}개`), list);
			parts.push(folded);
		}
		return parts;
	}
	onClick(event) {
		const target = event.target instanceof Element ? event.target.closest("button[data-action]") : null;
		if (!target) return;
		if (target.dataset.action === "retry-guidelines") {
			this.load();
			return;
		}
		const id = target.closest("[data-guideline]")?.dataset.guideline;
		if (target.dataset.action === "retire-guideline" && id) this.retire(id, target);
	}
	/** 지침 하나를 끈다. 끄는 동안 그 단추를 막고, 끝나면 결과를 남기고 다시 읽는다. */
	async retire(id, target) {
		if (this.busy) return;
		this.busy = true;
		target.disabled = true;
		try {
			const reply = await call("POST", guidelineRetirePath(id));
			this.result = retireMessage(reply.status, reply.body);
		} catch {
			this.result = retireMessage(0, null);
		}
		this.busy = false;
		await this.load();
	}
};
export { GuidePanel };
