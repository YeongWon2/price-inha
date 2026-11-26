// src/ui/approval-extras.ts
function isObject(value) {
	return typeof value === "object" && value !== null && !Array.isArray(value);
}
var VOTE_KEYS = [
	"bull",
	"bear",
	"neutral",
	"abstain"
];
function isCount(value) {
	return typeof value === "number" && Number.isInteger(value) && value >= 0;
}
function isClamp(value) {
	return isObject(value) && typeof value.limit === "string" && typeof value.before === "string" && typeof value.after === "string" && typeof value.reason === "string";
}
/** 승인 카드 몸통(모양이 틀린 칸은 없는 것으로 본다)에서 보강 글을 만든다. */
function extrasLines(wire) {
	const body = isObject(wire) ? wire : {};
	const votes = isObject(body.votes) ? body.votes : null;
	const counted = votes !== null && VOTE_KEYS.every((key) => isCount(votes[key]));
	const list = Array.isArray(body.clamps) ? body.clamps : [];
	return {
		votes: counted ? `강세 ${String(votes.bull)} · 약세 ${String(votes.bear)} · 중립 ${String(votes.neutral)} · 기권 ${String(votes.abstain)}` : null,
		clamps: list.filter(isClamp).map((c) => `${c.before}주 → ${c.after}주(${c.reason})`)
	};
}
function line(name, label, text) {
	const node = document.createElement("p");
	node.className = "approval-extra";
	node.dataset.part = name;
	const head = document.createElement("span");
	head.className = "approval-extra-label";
	head.textContent = label;
	node.append(head, " ", text);
	return node;
}
/** 자리에 보강 글을 그린다(앞의 것은 지운다). 보일 것이 없으면 자리를 숨긴다. */
function renderApprovalExtras(box, wire) {
	if (!box) return;
	const lines = extrasLines(wire);
	const nodes = [];
	if (lines.votes !== null) nodes.push(line("votes", "담당 투표", lines.votes));
	for (const clamp of lines.clamps) nodes.push(line("clamp", "한도 적용", clamp));
	box.replaceChildren(...nodes);
	box.hidden = nodes.length === 0;
}
export { renderApprovalExtras as t };
