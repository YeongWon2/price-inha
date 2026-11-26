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
export { readMemoPage as a, readMemoList as i, memoRows as n, pendingCount as r, cardMemos as t };
