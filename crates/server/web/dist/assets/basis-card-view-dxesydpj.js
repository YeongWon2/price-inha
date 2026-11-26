import { a as __exportAll, n as NO_RECORD, t as NONE } from "./basis-card-f0d1wup9.js";
// src/ui/basis-card-view.ts
var basis_card_view_exports = /* @__PURE__ */ __exportAll({ renderBasisCard: () => renderBasisCard });
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
export { renderBasisCard as n, basis_card_view_exports as t };
