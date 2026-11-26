// src/ui/basis-card.ts
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
var DECISION_TEXT$1 = {
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
	const decision = oneOf(DECISION_TEXT$1, value.decision);
	if (decision === null) return null;
	return {
		decision: DECISION_TEXT$1[decision],
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
// src/ui/basis-card-view.ts
/** 확신도 막대의 칸 수. */
var LEVELS = 3;
function make(tag, className, text) {
	const node = document.createElement(tag);
	node.className = className;
	if (text !== void 0) node.textContent = text;
	return node;
}
function part(tag, name, className, text) {
	const node = make(tag, className, text);
	node.dataset.part = name;
	return node;
}
/** 확신도 막대: 세 칸 가운데 `level`칸이 찬다. */
function meter(row) {
	const bar = part("span", "confidence", "basis-meter");
	bar.setAttribute("role", "meter");
	bar.setAttribute("aria-valuemin", "0");
	bar.setAttribute("aria-valuemax", String(LEVELS));
	bar.setAttribute("aria-valuenow", String(row.level));
	bar.setAttribute("aria-label", `확신도 ${row.confidence}`);
	bar.title = `확신도 ${row.confidence}`;
	for (let i = 1; i <= LEVELS; i++) {
		const seg = make("span", "basis-meter-seg");
		seg.dataset.on = String(i <= row.level);
		bar.append(seg);
	}
	return bar;
}
function opinionItem(row) {
	const item = make("li", "basis-row");
	item.dataset.agent = row.agent;
	item.dataset.tone = row.tone;
	if (row.abstained) item.dataset.abstained = "true";
	const head = make("div", "basis-row-head");
	head.append(part("strong", "name", "basis-name", row.name), part("span", "direction", "basis-chip", row.direction));
	if (!row.abstained) head.append(meter(row));
	head.append(part("span", "horizon", "basis-horizon", row.horizon));
	item.append(head, part("p", "reason", "basis-reason", row.reason));
	return item;
}
function leaderBox(leader) {
	const box = make("section", "basis-leader");
	box.dataset.role = "leader";
	box.append(make("h4", "basis-sub", "팀장"));
	if (leader === null) {
		box.append(part("p", "summary", "basis-muted", NONE));
		return box;
	}
	const decision = part("p", "decision", "basis-decision");
	decision.append("결정 ", make("strong", "", leader.decision));
	const flip = part("p", "flip", "basis-flip");
	flip.append(make("span", "basis-flip-label", "이러면 뒤집힌다"), " ", leader.flip);
	box.append(decision, part("p", "summary", "basis-summary", leader.summary), flip);
	return box;
}
/** 근거 카드 한 장. */
function renderBasisCard(card) {
	const root = make("section", "basis-card");
	root.dataset.role = "basis-card";
	root.append(make("h3", "basis-title", "근거 카드"));
	if (!card.recorded) {
		const none = make("p", "basis-muted", NO_RECORD);
		none.dataset.role = "no-record";
		root.append(none);
	} else {
		const votes = make("p", "basis-votes");
		votes.dataset.role = "votes";
		votes.append(make("span", "basis-sub", "찬반"), " ", card.votes);
		const list = make("ol", "basis-rows");
		list.dataset.role = "opinions";
		for (const row of card.rows) list.append(opinionItem(row));
		root.append(votes, list, leaderBox(card.leader));
	}
	const sources = make("p", "basis-sources");
	sources.dataset.role = "sources";
	sources.append(make("span", "basis-sub", "자료"), " ", card.sources);
	root.append(sources);
	return root;
}
var STATE_TEXT = {
	pending: "정산 대기",
	settled: "정산됨",
	unsettled: "정산 못 함"
};
var VERDICT_TEXT = {
	right: "맞음",
	wrong: "틀림",
	unknown: "판단 못 함"
};
var DECISION_TEXT = {
	buy: "사기",
	sell: "팔기"
};
var WHY_TEXT = { no_price: "시세 없음" };
/** 백분율 수 → `+1.23%`(0은 부호 없음). 없으면 빈 글. */
function pctText(value) {
	if (typeof value !== "number" || !Number.isFinite(value)) return "";
	return `${value > 0 ? "+" : value < 0 ? "-" : ""}${Math.abs(value).toFixed(2)}%`;
}
/** 메모 하나 → 줄. */
function memoRow(memo) {
	const verdict = memo.verdict ?? "";
	const why = memo.unsettled_reason ? WHY_TEXT[memo.unsettled_reason] ?? memo.unsettled_reason : "";
	return {
		id: memo.id ?? memo.trade,
		trade: memo.trade,
		date: memo.date,
		symbol: memo.symbol,
		decision: DECISION_TEXT[memo.decision] ?? memo.decision,
		stateWord: memo.state,
		state: STATE_TEXT[memo.state] ?? memo.state,
		verdict,
		result: verdict ? VERDICT_TEXT[verdict] : "",
		returnText: pctText(memo.return_pct),
		excessText: pctText(memo.excess_pct),
		holdText: typeof memo.hold_days === "number" ? `${memo.hold_days}거래일` : "",
		lesson: memo.lesson ?? "",
		reason: memo.reason ?? "",
		why
	};
}
/** 쪽의 줄들(서버 순서). */
function memoRows(page) {
	return page.memos.map(memoRow);
}
/** 카드에 보일 줄: 정산된 메모만 서버 순서대로 `max`개까지. */
function cardMemos(list, max = 3) {
	return list.filter((memo) => memo.state === "settled").slice(0, max).map(memoRow);
}
/** 정산 대기 메모 수. */
function pendingCount(list) {
	return list.filter((memo) => memo.state === "pending").length;
}
function isMemo(value) {
	if (typeof value !== "object" || value === null) return false;
	const memo = value;
	return typeof memo.trade === "number" && typeof memo.date === "string" && typeof memo.symbol === "string" && typeof memo.state === "string";
}
/** `?days=` 응답(배열)을 읽는다. 모양이 틀리면 던진다. */
function readMemoList(body) {
	if (!Array.isArray(body) || !body.every(isMemo)) throw new Error("결정 메모 목록 모양이 아니다");
	return body;
}
/** `?before=&max=` 응답(쪽)을 읽는다. 모양이 틀리면 던진다. */
function readMemoPage(body) {
	if (typeof body !== "object" || body === null || Array.isArray(body)) throw new Error("결정 메모 쪽 모양이 아니다");
	const page = body;
	const next = page.next;
	if (!Array.isArray(page.memos) || !page.memos.every(isMemo)) throw new Error("결정 메모 쪽 모양이 아니다");
	if (next !== null && next !== void 0 && typeof next !== "number") throw new Error("결정 메모 쪽 모양이 아니다");
	return {
		memos: page.memos,
		next: typeof next === "number" ? next : null
	};
}
export { readMemoPage as a, readMemoList as i, memoRows as n, renderBasisCard as o, pendingCount as r, readBasisCard as s, cardMemos as t };
