// src/ui/discussion-end.ts
/** 지켜보기 까닭 코드(`pi_agent::WatchReason::code`) → 쉬운 말. */
var WATCH_REASON_TEXT = {
	missing_data: "자료가 모자라 지켜봄",
	role_unavailable: "담당이 답하지 못해 지켜봄",
	discussion_bounded: "논의가 길어져 지켜봄",
	consensus_below: "합의 점수가 기준에 못 미쳐 지켜봄",
	rationale_incomplete: "근거가 비어 지켜봄",
	leader_watch: "팀장이 지켜보기로 정함"
};
/** 오류 끝 까닭 코드(서버가 남김) → 쉬운 말. */
var FAILED_REASON_TEXT = {
	discussion_error: "논의 중 오류가 났음",
	gather_failed: "자료를 모으다 오류가 났음"
};
/** 끝난 논의의 앞 글. */
var HEAD = {
	watched: "지켜봄",
	failed: "논의가 끝나지 못함"
};
function isEnd(value) {
	if (typeof value !== "object" || value === null) return false;
	const end = value;
	return typeof end.reason_code === "string" && typeof end.at === "number";
}
/** 끝난 논의의 종류와 그 칸. 칸이 없거나 모양이 틀리면 없음. */
function endOf(fields) {
	if (isEnd(fields.watched)) return ["watched", fields.watched];
	if (isEnd(fields.failed)) return ["failed", fields.failed];
	return null;
}
/** 끝난 논의의 종류. 칸이 없으면 없음. */
function discussionEndKind(fields) {
	return endOf(fields)?.[0] ?? null;
}
/**
* 끝난 논의의 글: `"지켜봄 — <까닭 글>"` · `"논의가 끝나지 못함 — <까닭 글>"`. 모르는 까닭 코드면 앞 글만.
* 칸이 없으면 없음.
*/
function discussionEndText(fields) {
	const found = endOf(fields);
	if (found === null) return null;
	const [kind, end] = found;
	const table = kind === "watched" ? WATCH_REASON_TEXT : FAILED_REASON_TEXT;
	const reason = Object.prototype.hasOwnProperty.call(table, end.reason_code) ? table[end.reason_code] : null;
	return reason === null ? HEAD[kind] : `${HEAD[kind]} — ${reason}`;
}
/** 끝난 시각. 칸이 없으면 없음. */
function discussionEndAt(fields) {
	return endOf(fields)?.[1].at ?? null;
}
export { discussionEndKind as n, discussionEndText as r, discussionEndAt as t };
