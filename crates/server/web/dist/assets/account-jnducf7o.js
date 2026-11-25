import { n as API, t as mountMenu } from "./menu-ndyyrws6.js";
/* empty css               */
import { n as call, r as el, t as LOAD_FAILED } from "./call-k6mg8w4a.js";
import { a as errorLine, c as table, i as emptyState, n as button, r as card, s as loading, t as badge } from "./parts-ma3d1o7o.js";
// src/account/model.ts
/** 팀 상태 → 화면 글(F1-6). */
var TEAM_STATE_TEXT = {
	operating: "운용 중",
	resting: "쉼",
	daily_limit_halt: "하루 한도 멈춤",
	anomaly_halt: "이상 감지 멈춤",
	full_stop: "전체 정지"
};
/** 모의계좌 안내(AGENTS 6번 · 결정 17): 화면의 어느 상태에서도 늘 보인다. */
var PAPER_NOTICE = "모의계좌입니다 — 실제 주문은 나가지 않습니다";
/** 전체 정지일 때 팀 카드의 까닭 글(R3-3 · R14-1: 사람만 다시 시작한다). */
var FULL_STOP_TEXT = "최대 손실 한도에 닿아 팀이 모두 멈췄습니다. 사람이 다시 시작해야 다시 돕니다.";
/** 보유가 없을 때의 빈 상태 글. */
var EMPTY_HOLDINGS = "보유 종목이 없습니다";
/** 값이 없을 때 보이는 글. */
var NONE_TEXT = "없음";
/** 실시간 연결이 없어도 다시 읽는 간격(밀리초, 30차 3번 — 조립 상수). */
var REFRESH_MS = 1e4;
/** `/ws`에서 이 종류의 이벤트를 받으면 팀 상태와 계좌를 다시 읽는다(하루 시작 · 마감 · 오늘 판단). */
var REFRESH_KINDS = [
	"day_start",
	"day_close",
	"day_plan"
];
/** 십진수 글자(`-12.5`, `+3`, `0.00`)를 읽는다. 십진수가 아니면 없음. 부동소수점으로 바꾸지 않는다. */
function digitsOf(value) {
	const found = /^([+-]?)(\d+)(?:\.(\d+))?$/.exec(value.trim());
	if (!found) return null;
	return {
		negative: found[1] === "-",
		whole: found[2],
		fraction: found[3] ?? ""
	};
}
/** 소수 `places`자리로 반올림(절반은 올림)한 크기 — `places`자리 정수(BigInt). */
function scaled(digits, places) {
	const kept = digits.fraction.padEnd(places, "0").slice(0, places);
	let amount = BigInt(digits.whole + kept);
	if ((digits.fraction[places] ?? "0") >= "5") amount += BigInt(1);
	return amount;
}
/** 세 자리마다 쉼표. */
function grouped(whole) {
	return whole.replace(/\B(?=(\d{3})+(?!\d))/g, ",");
}
/** 반올림한 크기를 (부호, 쉼표 붙은 정수 자리, 소수 자리)로. 0이 되면 부호가 없다. */
function rounded(value, places) {
	const digits = digitsOf(value);
	if (!digits) return null;
	const amount = scaled(digits, places);
	const text = amount.toString().padStart(places + 1, "0");
	const whole = text.slice(0, text.length - places);
	const fraction = text.slice(text.length - places);
	return {
		negative: digits.negative && amount !== BigInt(0),
		whole: grouped(whole),
		fraction
	};
}
/** 팀 상태 글. 모르는 글자는 그대로 보인다. */
function teamStateText(state) {
	return Object.hasOwn(TEAM_STATE_TEXT, state) ? TEAM_STATE_TEXT[state] : state;
}
/** 팀 상태 배지 빛깔: 운용 중 success · 쉼 neutral · 하루 한도 · 이상 감지 멈춤 warning · 전체 정지 danger(모르면 neutral). */
function teamTone(state) {
	switch (state) {
		case "operating": return "success";
		case "daily_limit_halt":
		case "anomaly_halt": return "warning";
		case "full_stop": return "danger";
		default: return "neutral";
	}
}
/**
* 오늘 할 일 글(F1-6): `plan`이 `"trade"`면 "오늘 매매합니다", `"rest"`면 "오늘 쉽니다", 없으면
* "오늘 할 일을 아직 정하지 않았습니다".
*/
function planText(team) {
	if (team.plan === "trade") return "오늘 매매합니다";
	if (team.plan === "rest") return "오늘 쉽니다";
	return "오늘 할 일을 아직 정하지 않았습니다";
}
/** 쉬는 이유: 오늘 쉬기로 했고 이유가 있으면 그 글, 아니면 없음. */
function restReason(team) {
	if (team.plan !== "rest" || typeof team.reason !== "string") return null;
	const reason = team.reason.trim();
	return reason === "" ? null : reason;
}
/** "다시 시작" 단추를 보이는가: 전체 정지일 때만(R3-3 · R14-1). */
function canRestart(state) {
	return state === "full_stop";
}
/**
* 돈 모양: 원은 세 자리마다 쉼표 + "원"(`"100000"` → `"100,000원"`), 달러는 "$" + 세 자리 쉼표 + 소수 둘째 자리
* (`"1234.5"` → `"$1,234.50"`). 음수는 앞에 "-"(`"-5000"` → `"-5,000원"`, `"-12"` → `"-$12.00"`). 없음은 [`NONE_TEXT`].
*/
function moneyText(value, currency) {
	if (value === null) return NONE_TEXT;
	const usd = currency === "USD";
	const parts = rounded(value, currency === "KRW" ? 0 : 2);
	if (!parts) return value;
	const sign = parts.negative ? "-" : "";
	if (currency === "KRW") return `${sign}${parts.whole}원`;
	if (usd) return `${sign}$${parts.whole}.${parts.fraction}`;
	return `${sign}${parts.whole}.${parts.fraction} ${currency}`;
}
/** 부호를 붙인 돈 모양(손익): 양수는 앞에 "+", 음수는 "-", 0은 부호 없음. 없음은 [`NONE_TEXT`]. */
function signedMoneyText(value, currency) {
	if (value === null) return NONE_TEXT;
	const text = moneyText(value, currency);
	return trendOf(value) === "up" ? `+${text}` : text;
}
/** 퍼센트 글: 소수 둘째 자리까지 + "%"(`"1.5"` → `"1.50%"`, `"0"` → `"0.00%"`). 없음은 [`NONE_TEXT`]. */
function percentText(value) {
	if (value === null) return NONE_TEXT;
	const parts = rounded(value, 2);
	if (!parts) return value;
	return `${parts.negative ? "-" : ""}${parts.whole}.${parts.fraction}%`;
}
/** 손익의 오름내림: 양수 up · 음수 down · 0 flat · 없음은 없음. */
function trendOf(value) {
	if (value === null) return null;
	const digits = digitsOf(value);
	if (!digits) return null;
	if (/^0*$/.test(digits.whole + digits.fraction)) return "flat";
	return digits.negative ? "down" : "up";
}
/**
* 낙폭 막대의 비율 = 낙폭 ÷ 한도, 0~1로 자른다(한도를 넘으면 1, 음수면 0). 한도가 없거나 0 이하면 없음.
*/
function drawdownRatio(drawdownPct, limitPct) {
	if (limitPct === null) return null;
	const limit = Number(limitPct);
	const drawdown = Number(drawdownPct);
	if (!Number.isFinite(limit) || limit <= 0 || !Number.isFinite(drawdown)) return null;
	return Math.min(1, Math.max(0, drawdown / limit));
}
/**
* 보유 표의 줄(응답 차례): 수량은 글자 그대로, 평균 단가 · 지금 값 · 평가는 [`moneyText`], 평가 손익은
* [`signedMoneyText`](모두 그 종목의 통화), `trend`는 평가 손익의 [`trendOf`].
*/
function holdingRows(account) {
	return (account.holdings ?? []).map((line) => ({
		symbol: line.symbol,
		quantity: line.quantity,
		average: moneyText(line.average_price, line.currency),
		last: moneyText(line.last_price, line.currency),
		value: moneyText(line.value, line.currency),
		pnl: signedMoneyText(line.pnl, line.currency),
		trend: trendOf(line.pnl)
	}));
}
/** 몸통의 글자 칸 하나(글자가 아니면 없음). */
function textField(body, name) {
	if (typeof body !== "object" || body === null) return null;
	const value = body[name];
	return typeof value === "string" ? value : null;
}
/**
* 다시 시작 응답을 결과로 읽는다:
* - 200 `{"state": "needs_approval", "number"}` → 승인이 필요함(승인 번호).
* - 그 밖의 200 `{"state", "notice"?}` → 다시 시작됨(바뀐 상태 · 함께 온 안내).
* - 그 밖(409 `{"error", "message"?}` 등) → 거절: 이유는 `message`가 있으면 그 글, `error`가
*   `"not_stopped"`면 "팀이 멈춰 있지 않습니다", 그 밖은 "서버가 받지 않았습니다".
*/
function restartResult(status, body) {
	const state = textField(body, "state");
	if (status === 200 && state !== null) {
		if (state === "needs_approval") return {
			kind: "needs_approval",
			number: textField(body, "number") ?? ""
		};
		return {
			kind: "restarted",
			state,
			notice: textField(body, "notice")
		};
	}
	const message = textField(body, "message");
	if (message !== null) return {
		kind: "refused",
		reason: message
	};
	if (textField(body, "error") === "not_stopped") return {
		kind: "refused",
		reason: "팀이 멈춰 있지 않습니다"
	};
	return {
		kind: "refused",
		reason: "서버가 받지 않았습니다"
	};
}
/**
* 다시 시작 결과 글: 다시 시작됨은 "다시 시작했습니다 — 지금 상태: <상태 글>"(안내가 있으면 " · <안내>"를 잇는다),
* 승인이 필요함은 "승인이 필요합니다 — 승인 번호 <번호>", 거절은 "다시 시작하지 못했습니다 — <이유>".
*/
function restartText(result) {
	switch (result.kind) {
		case "restarted": {
			const base = `다시 시작했습니다 — 지금 상태: ${teamStateText(result.state)}`;
			return result.notice ? `${base} · ${result.notice}` : base;
		}
		case "needs_approval": return `승인이 필요합니다 — 승인 번호 ${result.number}`;
		case "refused": return `다시 시작하지 못했습니다 — ${result.reason}`;
	}
}
// src/account/view.ts
/** 낙폭 막대가 경고 빛깔로 바뀌는 한도 대비 비율(한도의 3/4). */
var NEAR_LIMIT = .75;
/** 보유 표의 머리 칸. */
var HOLDING_COLUMNS = [
	"종목",
	"수량",
	"평균 단가",
	"지금 값",
	"평가",
	"평가 손익"
];
/** 통화 이름. */
var CURRENCY_NAME = {
	KRW: "원화",
	USD: "달러"
};
/** 모의계좌 안내 줄(`data-role="paper-notice"`, 글은 `PAPER_NOTICE`). */
function renderNotice() {
	const node = el("p", "acc-notice", PAPER_NOTICE);
	node.dataset.role = "paper-notice";
	node.setAttribute("role", "note");
	return node;
}
/**
* 팀 상태 카드(`data-role="team"`): 상태 배지(`data-role="team-state"`) · 오늘 할 일(`data-role="team-plan"`) ·
* 쉬는 이유(`data-role="team-reason"`, 있을 때만). 전체 정지면 까닭(`FULL_STOP_TEXT`)과
* `button[data-action="restart"]`를 더한다(그 밖의 상태에는 단추가 없다). 단추를 누르면 `onRestart`를 부른다.
*/
function renderTeam(team, onRestart) {
	const state = badge(teamStateText(team.state), teamTone(team.state));
	state.dataset.role = "team-state";
	const plan = el("p", "acc-plan", planText(team));
	plan.dataset.role = "team-plan";
	const head = el("div", "acc-team-head");
	head.append(state, plan);
	const node = card("팀 상태", head);
	node.classList.add("acc-team");
	node.dataset.role = "team";
	node.dataset.state = team.state;
	const reason = restReason(team);
	if (reason !== null) {
		const line = el("p", "acc-reason");
		line.dataset.role = "team-reason";
		line.append(el("span", "acc-label", "쉬는 이유"), el("span", "", reason));
		node.append(line);
	}
	if (canRestart(team.state)) {
		const stop = el("div", "acc-stop");
		const restart = button("다시 시작", "danger");
		restart.dataset.action = "restart";
		restart.addEventListener("click", () => onRestart());
		stop.append(el("p", "acc-stop-text", FULL_STOP_TEXT), restart);
		node.append(stop);
	}
	return node;
}
/** 이름 · 값 한 쌍(`data-role`은 값에 단다). */
function stat(label, value, role) {
	const box = el("div", "acc-stat");
	const shown = el("dd", "acc-stat-value", value);
	shown.dataset.role = role;
	box.append(el("dt", "acc-label", label), shown);
	return {
		box,
		value: shown
	};
}
/** 낙폭 글과 한도 대비 막대(한도를 모르면 막대 없음). */
function drawdownBlock(line) {
	const box = el("div", "acc-drawdown");
	const limit = line.drawdown_limit_pct;
	const words = limit === null ? `${percentText(line.drawdown_pct)} · 한도 모름` : `${percentText(line.drawdown_pct)} / 한도 ${percentText(limit)}`;
	const label = el("div", "acc-drawdown-label");
	const value = el("span", "acc-drawdown-value", words);
	value.dataset.role = "drawdown";
	label.append(el("span", "acc-label", "최고 대비 낙폭"), value);
	box.append(label);
	const ratio = drawdownRatio(line.drawdown_pct, limit);
	if (ratio === null) return box;
	const bar = el("div", "acc-bar");
	bar.dataset.role = "drawdown-bar";
	bar.dataset.ratio = String(ratio);
	bar.dataset.level = ratio >= 1 ? "over" : ratio >= NEAR_LIMIT ? "near" : "ok";
	bar.setAttribute("role", "meter");
	bar.setAttribute("aria-label", "낙폭 한도 대비");
	bar.setAttribute("aria-valuemin", "0");
	bar.setAttribute("aria-valuemax", "100");
	bar.setAttribute("aria-valuenow", String(Math.round(ratio * 100)));
	const fill = el("div", "acc-bar-fill");
	fill.style.width = `${(ratio * 100).toFixed(1)}%`;
	bar.append(fill);
	box.append(bar);
	return box;
}
/** 통화 하나의 칸: 총 평가액 · 쓸 수 있는 현금 · 오늘 손익 · 낙폭. */
function currencyPanel(line, cash) {
	const panel = el("section", "acc-currency");
	panel.dataset.currency = line.currency;
	const name = CURRENCY_NAME[line.currency] ?? line.currency;
	const head = el("div", "acc-currency-head");
	head.append(el("h3", "acc-currency-name", `${name} · ${line.currency}`), el("span", "acc-label", "총 평가액"));
	const total = el("p", "acc-total", moneyText(line.total, line.currency));
	total.dataset.role = "equity-total";
	const stats = el("dl", "acc-stats");
	const available = stat("쓸 수 있는 현금", moneyText(cash?.available ?? null, line.currency), "cash");
	const pnl = stat("오늘 손익", signedMoneyText(line.day_pnl, line.currency), "day-pnl");
	pnl.value.dataset.trend = trendOf(line.day_pnl) ?? "flat";
	stats.append(available.box, pnl.box);
	if (cash && trendOf(cash.held) === "up") stats.append(stat("주문에 묶인 돈", moneyText(cash.held, line.currency), "cash-held").box);
	if (cash && trendOf(cash.unsettled) === "up") stats.append(stat("결제 전 돈", moneyText(cash.unsettled, line.currency), "cash-unsettled").box);
	panel.append(head, total, stats, drawdownBlock(line));
	return panel;
}
/**
* 계좌 카드(`data-role="account"`): 통화마다(`[data-currency]`) 총 평가액(`data-role="equity-total"`) · 쓸 수 있는
* 현금(`data-role="cash"`) · 오늘 손익(`data-role="day-pnl"`, `data-trend`) · 낙폭 글(`data-role="drawdown"`)과
* 한도 대비 막대(`data-role="drawdown-bar"`, `data-ratio`, 한도를 모르면 막대 없음).
*/
function renderAccount(account) {
	const grid = el("div", "acc-currencies");
	for (const line of account.equity ?? []) grid.append(currencyPanel(line, (account.cash ?? []).find((cash) => cash.currency === line.currency)));
	const node = card("모의계좌", grid);
	node.classList.add("acc-account");
	node.dataset.role = "account";
	return node;
}
/**
* 보유 표(`data-role="holdings"`): 줄마다 `tr[data-symbol]`, 평가 손익 칸은 `data-trend`. 보유가 없으면
* 빈 상태(`data-role="empty"`, `EMPTY_HOLDINGS`).
*/
function renderHoldings(account) {
	const rows = holdingRows(account);
	let body;
	if (rows.length === 0) {
		body = emptyState(EMPTY_HOLDINGS);
		body.dataset.role = "empty";
	} else {
		const grid = table(HOLDING_COLUMNS, rows.map((row) => [
			row.symbol,
			row.quantity,
			row.average,
			row.last,
			row.value,
			row.pnl
		]));
		grid.classList.add("acc-holdings");
		grid.tBodies[0].querySelectorAll("tr").forEach((tr, index) => {
			const row = rows[index];
			tr.dataset.symbol = row.symbol;
			[...tr.cells].forEach((cell, at) => cell.dataset.label = HOLDING_COLUMNS[at]);
			if (row.trend !== null) tr.cells[tr.cells.length - 1].dataset.trend = row.trend;
		});
		body = el("div", "acc-table-wrap");
		body.append(grid);
	}
	const node = card("보유 종목", body);
	node.dataset.role = "holdings";
	return node;
}
// src/account/screen.ts
/** 몸통이 JSON 객체인가. */
function isObject(value) {
	return typeof value === "object" && value !== null;
}
/** 화면 하나의 상태와 그리기. */
var AccountScreen = class {
	body = el("div", "acc-body");
	/** 다시 시작 결과 줄: 다시 그려도 남는다(다시 시작 뒤 단추가 사라져도 결과는 보인다). */
	result = el("p", "acc-restart-result");
	running = null;
	again = false;
	restarting = false;
	constructor(main) {
		this.result.dataset.role = "restart-result";
		this.result.setAttribute("role", "status");
		main.append(renderNotice(), this.body);
	}
	renderLoading() {
		this.body.replaceChildren(loading());
	}
	/** 팀 상태와 계좌를 다시 읽어 그린다. 읽는 중이면 끝난 뒤 한 번만 더 읽는다. */
	refresh() {
		if (this.running) {
			this.again = true;
			return this.running;
		}
		this.running = (async () => {
			do {
				this.again = false;
				await this.load();
			} while (this.again);
			this.running = null;
		})();
		return this.running;
	}
	/** 둘을 함께 읽어 그린다. 하나라도 실패하면 오류 줄과 다시 시도. */
	async load() {
		try {
			const [team, account] = await Promise.all([call("GET", API.team), call("GET", API.account)]);
			if (!team.ok || !isObject(team.body)) throw new Error(`team ${team.status}`);
			if (!account.ok || !isObject(account.body)) throw new Error(`account ${account.status}`);
			this.render(team.body, account.body);
		} catch {
			this.renderFailed();
		}
	}
	render(team, account) {
		const teamCard = renderTeam(team, () => void this.restart());
		teamCard.append(this.result);
		const top = el("div", "acc-top");
		top.append(teamCard, renderAccount(account));
		this.body.replaceChildren(top, renderHoldings(account));
	}
	renderFailed() {
		const retry = button("다시 시도", "accent");
		retry.dataset.action = "retry";
		retry.addEventListener("click", () => {
			this.renderLoading();
			this.refresh();
		});
		const box = el("div", "acc-failed");
		box.append(errorLine(LOAD_FAILED), retry);
		this.body.replaceChildren(box);
	}
	/** 확인 한 번 뒤에만 다시 시작을 보낸다(R3-3 · R14-1: 사람이 화면에서 누르고 확인하는 것이 승인이다). */
	async restart() {
		if (this.restarting || !window.confirm("팀을 다시 시작할까요? 다시 시작하면 팀이 쉬는 중으로 돌아가 다음 판단부터 운용합니다.")) return;
		this.restarting = true;
		for (const node of this.body.querySelectorAll("button[data-action=\"restart\"]")) node.disabled = true;
		try {
			let status = 0;
			let body = null;
			try {
				({status, body} = await call("POST", API.teamRestart));
			} catch {}
			this.result.textContent = restartText(restartResult(status, body));
			await this.refresh();
		} finally {
			this.restarting = false;
			for (const node of this.body.querySelectorAll("button[data-action=\"restart\"]")) node.disabled = false;
		}
	}
};
/** 같은 출처의 `/ws`를 열어 `REFRESH_KINDS` 이벤트에 다시 읽는다. 끊기면 `REFRESH_MS` 뒤 다시 잇는다. */
function listen(screen) {
	if (typeof WebSocket === "undefined") return;
	const scheme = location.protocol === "https:" ? "wss:" : "ws:";
	let socket;
	try {
		socket = new WebSocket(`${scheme}//${location.host}${API.socket}`);
	} catch {
		return;
	}
	socket.onmessage = (event) => {
		let kind = null;
		try {
			kind = JSON.parse(String(event.data)).kind;
		} catch {
			return;
		}
		if (typeof kind === "string" && REFRESH_KINDS.includes(kind)) screen.refresh();
	};
	socket.onclose = () => {
		setTimeout(() => listen(screen), REFRESH_MS);
	};
}
/**
* 본문 자리(`main`)에 계좌 · 운용 화면을 그린다: 세션을 얻고 팀 상태 · 계좌를 읽어 그린다.
* 첫 그림이 끝나면 풀린다. 자리가 없으면 아무것도 하지 않는다.
*/
async function mountAccount(main) {
	if (!main) return;
	const screen = new AccountScreen(main);
	screen.renderLoading();
	try {
		await call("POST", API.session);
	} catch {}
	await screen.refresh();
	listen(screen);
	setInterval(() => {
		if (!document.hidden) screen.refresh();
	}, REFRESH_MS);
	document.addEventListener("visibilitychange", () => {
		if (!document.hidden) screen.refresh();
	});
}
// src/account/main.ts
mountMenu(document.getElementById("menu"), "account");
mountAccount(document.getElementById("screen"));
