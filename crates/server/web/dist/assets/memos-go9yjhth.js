import { n as API } from "./menu-kc5ybrux.js";
import { r as el, t as LOAD_FAILED } from "./call-enmqzayl.js";
import { a as readMemoPage, n as memoRows } from "./memo-model-pzip3mr0.js";
import { a as errorLine, i as emptyState, n as button, r as card, s as loading } from "./parts-ma3d1o7o.js";
var EMPTY_TEXT = "아직 결정 메모가 없습니다. 팀이 사거나 팔기로 정하면 여기에 쌓이고, 5거래일 뒤 정산됩니다.";
function pageUrl(before) {
	const query = new URLSearchParams();
	if (before !== null) query.set("before", String(before));
	query.set("max", String(20));
	return `${API.memos}?${query.toString()}`;
}
/** 줄 하나. */
function memoLine(row) {
	const item = el("li", "his-memo");
	item.dataset.memo = String(row.id);
	item.dataset.trade = String(row.trade);
	item.dataset.state = row.stateWord;
	const facts = [
		row.date,
		row.symbol,
		row.decision,
		row.state,
		row.result,
		row.returnText,
		row.holdText
	];
	item.append(el("div", "his-memo-head", facts.filter((part) => part !== "").join(" · ")));
	const note = row.stateWord === "settled" ? row.lesson : row.stateWord === "unsettled" ? `까닭: ${row.why}` : row.reason;
	if (note) item.append(el("p", "his-memo-note", row.stateWord === "pending" ? `그때 까닭: ${note}` : note));
	return item;
}
/** 판에 결정 메모 탭을 단다. */
function mountMemoTab(panel, fetcher) {
	const list = el("ul", "his-list his-memos");
	list.dataset.role = "memos";
	const body = el("div", "his-list-body");
	const foot = el("div", "his-foot");
	panel.append(card("결정 메모", el("p", "his-lead", "팀의 사기 · 팔기 결정이 5거래일 뒤 어떻게 됐는지와 교훈입니다."), body, foot));
	let loaded = false;
	let pending = Promise.resolve();
	const failed = (again) => {
		const retry = button("다시 시도", "accent");
		retry.dataset.action = "retry";
		retry.addEventListener("click", again);
		const box = el("div", "his-failed");
		box.append(errorLine(LOAD_FAILED), retry);
		return box;
	};
	const renderMore = (next) => {
		if (next === null) {
			foot.replaceChildren();
			return;
		}
		const more = button("더 보기");
		more.dataset.action = "more";
		more.addEventListener("click", () => void run(next));
		foot.replaceChildren(more);
	};
	const load = async (before) => {
		if (before === null) body.replaceChildren(loading());
		try {
			const reply = await fetcher(pageUrl(before), { cache: "no-store" });
			if (!reply.ok) throw new Error(`memos ${reply.status}`);
			const page = readMemoPage(await reply.json());
			loaded = true;
			const rows = memoRows(page).map(memoLine);
			if (before === null) {
				list.replaceChildren(...rows);
				if (rows.length === 0) {
					const empty = emptyState(EMPTY_TEXT);
					empty.dataset.role = "empty";
					body.replaceChildren(empty);
				} else body.replaceChildren(list);
			} else list.append(...rows);
			renderMore(page.next);
		} catch {
			const again = () => void run(before);
			if (before === null) body.replaceChildren(failed(again));
			else foot.replaceChildren(failed(again));
		}
	};
	const run = (before) => {
		pending = load(before);
		return pending;
	};
	return {
		get loaded() {
			return loaded;
		},
		open: () => loaded ? Promise.resolve() : run(null),
		settled: () => pending
	};
}
export { mountMemoTab };
