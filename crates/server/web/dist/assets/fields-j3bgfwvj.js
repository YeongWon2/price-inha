// src/ui/fields.ts
var FIELD_LABELS = {
	risk_per_trade_pct: "1회 위험 비율",
	daily_loss_limit_pct: "일일 손실 한도",
	max_drawdown_pct: "최대 낙폭 한도",
	symbol_weight_pct: "종목 비중",
	conviction_weight_pct: "확신 종목 비중",
	sector_weight_pct: "업종 비중",
	auto_order_limit_pct: "자동 주문 금액 한도",
	min_cash_pct: "최소 현금 비중",
	max_positions: "최대 동시 보유 종목 수",
	max_trades_per_day: "일일 최대 매매 횟수",
	loss_streak: "연속 손실 정지",
	"stop.fixed_loss_pct": "고정 손절 폭",
	"stop.atr": "ATR 손절",
	"stop.thesis_break": "논리 손절",
	"stop.flatten_before_close": "장 마감 청산",
	max_holding: "최대 보유 기간",
	chase_limit_pct: "추격 허용 폭",
	promotion: "승격 기준",
	universe: "유니버스",
	trading_window: "거래 시간대",
	entry_signals: "진입 신호",
	triggers: "구독 트리거",
	rebalance: "리밸런싱 주기",
	take_profit: "익절 방식",
	approval_wait: "승인 대기 시간",
	unfilled_cancel: "미체결 취소 시간"
};
var WORDS = {
	breakout: "돌파",
	pullback: "눌림목",
	value_range: "가치 구간",
	event: "이벤트",
	quote: "시세",
	disclosure: "공시",
	news: "뉴스",
	schedule: "정해진 시각",
	kr: "국내",
	us: "미국",
	day: "일봉",
	minute: "분봉",
	same_day: "당일",
	unlimited: "제한 없음",
	rest_of_day: "그날 남은 시간",
	at_close: "장 마감까지 둠"
};
var NONE = "없음";
function isObject(value) {
	return typeof value === "object" && value !== null && !Array.isArray(value);
}
function numberText(value) {
	if (typeof value === "number" && Number.isFinite(value)) return value.toLocaleString("ko-KR", { maximumFractionDigits: 4 });
	if (typeof value === "string" && /^-?\d+(\.\d+)?$/.test(value.trim())) {
		const [whole, fraction] = value.trim().split(".");
		return (whole.startsWith("-") ? "-" : "") + whole.replace("-", "").replace(/\B(?=(\d{3})+(?!\d))/g, ",") + (fraction !== void 0 ? "." + fraction : "");
	}
	return null;
}
/** 십진수 글자 → 수 글(천 단위 쉼표). 수가 아니면 받은 글 그대로. */
function numberLabel(value) {
	return numberText(value) ?? value;
}
function wonText(value) {
	const number = typeof value === "number" ? value : typeof value === "string" ? Number(value) : NaN;
	if (!Number.isFinite(number)) return null;
	const EOK = 1e8;
	if (Math.abs(number) >= EOK) return (number / EOK).toLocaleString("ko-KR", { maximumFractionDigits: 1 }) + "억 원";
	return number.toLocaleString("ko-KR") + "원";
}
function word(value) {
	return typeof value === "string" ? WORDS[value] ?? value : generic(value);
}
function durationText(value) {
	if (!isObject(value) || typeof value.secs !== "number") return generic(value);
	const secs = value.secs;
	if (secs >= 60 && secs % 60 === 0) return `${secs / 60}분`;
	return `${secs}초`;
}
function generic(value) {
	if (value === null || value === void 0) return NONE;
	if (typeof value === "boolean") return value ? "켬" : "끔";
	const number = numberText(value);
	if (number !== null) return number;
	if (typeof value === "string") return WORDS[value] ?? value;
	if (Array.isArray(value)) return value.length === 0 ? NONE : value.map(word).join(" · ");
	if (isObject(value)) return Object.entries(value).map(([key, inner]) => `${WORDS[key] ?? key} ${generic(inner)}`).join(" · ");
	return String(value);
}
function percent(value) {
	const number = numberText(value);
	return number === null ? generic(value) : `${number}%`;
}
var SHAPED = {
	max_positions: (v) => v.low === v.high ? `${generic(v.low)}종목` : `${generic(v.low)}~${generic(v.high)}종목`,
	loss_streak: (v) => {
		const cooldown = v.cooldown;
		const rest = isObject(cooldown) && typeof cooldown.trading_days === "number" ? `${cooldown.trading_days}거래일 쉼` : `${word(cooldown)} 쉼`;
		return `${generic(v.losses)}번 연속 손실이면 ${rest}`;
	},
	"stop.fixed_loss_pct": (v) => v.low === v.high ? percent(v.low) : `${generic(v.low)}~${percent(v.high)}`,
	"stop.atr": (v) => `ATR × ${generic(v.multiple)}(${word(v.bar)})`,
	max_holding: (v) => typeof v.trading_days === "number" ? `${v.trading_days}거래일` : null,
	promotion: (v) => {
		const parts = [
			`매매 ${generic(v.min_trades)}건 이상`,
			`${generic(v.min_trading_days)}거래일 이상`,
			`손익 ${generic(v.net_pnl_above)} 초과`,
			`낙폭 ${percent(v.max_drawdown_pct)} 이하`
		];
		if (v.out_of_sample_pnl_above !== null && v.out_of_sample_pnl_above !== void 0) parts.push(`표본 밖 ${generic(v.out_of_sample_pnl_above)} 초과`);
		return parts.join(" · ");
	},
	universe: (v) => {
		const parts = [
			Array.isArray(v.markets) ? v.markets.map(word).join(" · ") : generic(v.markets),
			`시가총액 ${wonText(v.min_market_cap) ?? generic(v.min_market_cap)} 이상`,
			`거래대금 ${wonText(v.min_turnover) ?? generic(v.min_turnover)} 이상`
		];
		if (Array.isArray(v.excluded_sectors) && v.excluded_sectors.length > 0) parts.push(`제외 업종 ${v.excluded_sectors.map(word).join(" · ")}`);
		return parts.join(" · ");
	},
	trading_window: (v) => `장 시작 ${generic(v.skip_open_minutes)}분 건너뜀 · 시간외 ${v.after_market ? "거래" : "안 함"}`,
	rebalance: (v) => `${generic(v.every_trading_days)}거래일마다 · 이탈 ${percent(v.drift_pct)}p 넘으면`,
	take_profit: (v) => {
		const parts = [`목표 ${generic(v.r_target)}R`];
		if (v.partial_at_target) parts.push("목표에서 일부 익절");
		if (v.trailing) parts.push("추적 익절");
		if (v.valuation_exit) parts.push("가치 도달 청산");
		if (v.rebalance_exit) parts.push("리밸런싱 청산");
		return parts.join(" · ");
	},
	approval_wait: (v) => `사기 ${durationText(v.buy)} · 손절 ${durationText(v.stop_loss)} · 익절 ${durationText(v.take_profit)}`,
	unfilled_cancel: (v) => v.after !== void 0 ? `${durationText(v.after)} 뒤 취소` : null
};
/** 칸 이름 → 보이는 이름. 모르는 키는 키 그대로. */
function fieldLabel(key) {
	return FIELD_LABELS[key] ?? key;
}
/** 칸 값 → 글. 퍼센트 칸(`_pct`로 끝나는 키)의 수는 `%`를 붙이고, 없음은 "없음". */
function formatValue(key, value) {
	if (value === null || value === void 0) return NONE;
	if (isObject(value)) {
		const shaped = SHAPED[key]?.(value);
		if (shaped) return shaped;
	}
	if (key.endsWith("_pct")) return percent(value);
	return generic(value);
}
/** 상한 글자 → 안내 글("상한 2%"). 상한이 없으면 없음. */
function capText(cap) {
	return cap === null || cap === void 0 ? null : `상한 ${percent(cap)}`;
}
/** 바뀐 칸 하나 → 줄(이름 · 앞 값 글 · 뒤 값 글 · 넓힘). */
function changeRow(change) {
	return {
		key: change.key,
		label: fieldLabel(change.key),
		before: formatValue(change.key, change.before),
		after: formatValue(change.key, change.after),
		widened: change.widened
	};
}
export { isObject as a, formatValue as i, changeRow as n, numberLabel as o, fieldLabel as r, capText as t };
