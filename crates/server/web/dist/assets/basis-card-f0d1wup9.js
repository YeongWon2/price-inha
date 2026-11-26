// \0rolldown/runtime.js
var __defProp = Object.defineProperty;
var __exportAll = (all, no_symbols) => {
	let target = {};
	for (var name in all) __defProp(target, name, {
		get: all[name],
		enumerable: true
	});
	if (!no_symbols) __defProp(target, Symbol.toStringTag, { value: "Module" });
	return target;
};
// src/ui/basis-card.ts
var basis_card_exports = /* @__PURE__ */ __exportAll({
	ABSTAIN: () => ABSTAIN,
	NONE: () => NONE,
	NO_RECORD: () => NO_RECORD,
	readBasisCard: () => readBasisCard,
	voteText: () => voteText
});
/** 옛 매매(칸이 생기기 전)의 글. */
var NO_RECORD = "기록 없음";
/** 빈 칸의 글. */
var NONE = "없음";
/** 기권의 글. */
var ABSTAIN = "기권";
var DIRECTION_TEXT = {
	bull: "강세",
	bear: "약세",
	neutral: "중립"
};
var CONFIDENCE_TEXT = {
	low: "낮음",
	mid: "보통",
	high: "높음"
};
var CONFIDENCE_LEVEL = {
	low: 1,
	mid: 2,
	high: 3
};
var DECISION_TEXT = {
	buy: "사기",
	sell: "팔기",
	watch: "지켜보기"
};
function isObject(value) {
	return typeof value === "object" && value !== null && !Array.isArray(value);
}
function oneOf(table, value) {
	return typeof value === "string" && Object.hasOwn(table, value) ? value : null;
}
/** 글이면 앞뒤 공백을 뗀 것, 비었거나 글이 아니면 "없음". */
function filled(value) {
	return typeof value === "string" && value.trim() !== "" ? value.trim() : NONE;
}
function count(value) {
	return typeof value === "number" && Number.isFinite(value) && value >= 0 ? Math.floor(value) : 0;
}
/** 찬반 수 글: "강세 n · 약세 n · 중립 n · 기권 n"(R4-4 · V55와 같은 말). */
function voteText(votes) {
	return `강세 ${count(votes.bull)} · 약세 ${count(votes.bear)} · 중립 ${count(votes.neutral)} · 기권 ${count(votes.abstain)}`;
}
/** 서버 찬반 수(모양이 틀리면 0). */
function readVotes(value) {
	const v = isObject(value) ? value : {};
	return {
		bull: count(v.bull),
		bear: count(v.bear),
		neutral: count(v.neutral),
		abstain: count(v.abstain)
	};
}
/** 담당 한 줄. `agent`가 글이 아니면 `null`(그 줄만 뺀다). */
function readRow(value) {
	if (!isObject(value) || typeof value.agent !== "string" || value.agent === "") return null;
	const direction = oneOf(DIRECTION_TEXT, value.direction);
	const abstained = value.abstained === true || direction === null;
	const confidence = abstained ? null : oneOf(CONFIDENCE_TEXT, value.confidence);
	const days = value.horizon_days;
	const horizon = typeof days === "number" && Number.isInteger(days) && days > 0 ? `${days}거래일` : NONE;
	return {
		agent: value.agent,
		name: typeof value.name === "string" && value.name !== "" ? value.name : value.agent,
		abstained,
		tone: abstained || direction === null ? "abstain" : direction,
		direction: abstained || direction === null ? ABSTAIN : DIRECTION_TEXT[direction],
		confidence: confidence === null ? NONE : CONFIDENCE_TEXT[confidence],
		level: confidence === null ? 0 : CONFIDENCE_LEVEL[confidence],
		reason: filled(value.reason),
		horizon: `보는 기간 ${horizon}`
	};
}
function readLeader(value) {
	if (!isObject(value)) return null;
	const decision = oneOf(DECISION_TEXT, value.decision);
	if (decision === null) return null;
	return {
		decision: DECISION_TEXT[decision],
		summary: filled(value.summary),
		flip: filled(value.flip_condition)
	};
}
function readSources(value) {
	if (!Array.isArray(value)) return NONE;
	const names = value.map((source) => isObject(source) && typeof source.name === "string" ? source.name.trim() : "").filter((name) => name !== "");
	return names.length > 0 ? [...new Set(names)].join(" · ") : NONE;
}
/** 서버 몸통(모양이 틀린 칸은 없는 것으로 본다)을 근거 카드 글로 바꾼다. */
function readBasisCard(wire) {
	const body = isObject(wire) ? wire : {};
	const rows = (Array.isArray(body.opinions) ? body.opinions : []).map(readRow).filter((row) => row !== null);
	const leader = readLeader(body.leader);
	return {
		recorded: rows.length > 0 || leader !== null,
		rows,
		votes: voteText(readVotes(body.votes)),
		leader,
		sources: readSources(body.sources)
	};
}
export { __exportAll as a, readBasisCard as i, NO_RECORD as n, basis_card_exports as r, NONE as t };
