//! 30차 1번: 모의계좌 읽기(C31 · V86 · R3-1 · R16-5).

use price_inha::{ACCOUNT_PAPER, ACCOUNT_PATH, TEMPLATES_VIEW_PATH};
use rust_decimal::Decimal;
use serde_json::Value;

use crate::common::{
    Bench, ENTRY, HYNIX, INITIAL_KRW, INITIAL_USD, QUANTITY, d, dec, get, get_anonymous, get_with,
    kst, monday, row,
};

async fn start() -> Bench {
    Bench::start(kst(monday(), 10, 0)).await
}

/// 계좌를 읽는다(200이어야 한다).
async fn account(bench: &Bench) -> Value {
    let reply = get(bench.addr(), ACCOUNT_PATH).await;
    assert_eq!(reply.status, 200, "계좌를 읽어야 한다: {}", reply.body);
    reply.body
}

/// 그 통화의 현금 전부(쓸 수 있는 + 묶인 + 결제 전).
fn cash_total(body: &Value, currency: &str) -> Decimal {
    let cash = row(body, "cash", "currency", currency);
    dec(&cash["available"]) + dec(&cash["held"]) + dec(&cash["unsettled"])
}

/// (최고 − 지금) ÷ 최고 × 100.
fn drawdown_of(peak: Decimal, total: Decimal) -> Decimal {
    if total >= peak {
        Decimal::ZERO
    } else {
        (peak - total) / peak * Decimal::from(100)
    }
}

/// 그 템플릿의 `max_drawdown_pct` 값(템플릿 화면 읽기에서).
async fn template_drawdown(bench: &Bench, id: i64) -> Decimal {
    let reply = get(bench.addr(), TEMPLATES_VIEW_PATH).await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    let template = reply.body["templates"]
        .as_array()
        .expect("템플릿 목록")
        .iter()
        .find(|t| t["id"].as_i64() == Some(id))
        .expect("고른 템플릿이 있다")
        .clone();
    let field = template["fields"]
        .as_array()
        .expect("칸 목록")
        .iter()
        .find(|f| f["key"] == "max_drawdown_pct")
        .expect("최대 낙폭 칸이 있다")
        .clone();
    match &field["value"] {
        Value::String(text) => d(text),
        other => d(&other.to_string()),
    }
}

// 검증: C31 V86 R3-1 R16-5 D11
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 팀_없이_띄운_서버의_막_켠_계좌는_처음_돈_그대로이고_보유가_없고_낙폭이_0이다() {
    let bench = start().await;
    let body = account(&bench).await;

    // 계좌는 모의계좌뿐이라고 응답에 적는다.
    assert_eq!(body["account"], ACCOUNT_PAPER, "{body}");
    assert_eq!(ACCOUNT_PAPER, "paper");

    // 통화마다 현금 한 줄: 처음 돈이 모두 쓸 수 있는 돈이다.
    for (currency, initial) in [("KRW", INITIAL_KRW), ("USD", INITIAL_USD)] {
        let cash = row(&body, "cash", "currency", currency);
        assert_eq!(dec(&cash["available"]), d(initial), "{currency}: {body}");
        assert_eq!(dec(&cash["held"]), Decimal::ZERO, "{currency}: {body}");
        assert_eq!(dec(&cash["unsettled"]), Decimal::ZERO, "{currency}: {body}");
    }
    assert_eq!(
        body["cash"].as_array().unwrap().len(),
        2,
        "통화는 둘: {body}"
    );

    // 보유는 빈 목록이다.
    assert_eq!(body["holdings"], Value::Array(Vec::new()), "{body}");

    // 평가액 = 최고 = 그날 시작 = 처음 돈, 오늘 손익 0, 낙폭 0. 쓰는 템플릿이 없으니 낙폭 한도는 모른다(null).
    for (currency, initial) in [("KRW", INITIAL_KRW), ("USD", INITIAL_USD)] {
        let equity = row(&body, "equity", "currency", currency);
        assert_eq!(dec(&equity["total"]), d(initial), "{currency}: {body}");
        assert_eq!(dec(&equity["peak"]), d(initial), "{currency}: {body}");
        assert_eq!(dec(&equity["day_start"]), d(initial), "{currency}: {body}");
        assert_eq!(dec(&equity["day_pnl"]), Decimal::ZERO, "{currency}: {body}");
        assert_eq!(
            dec(&equity["drawdown_pct"]),
            Decimal::ZERO,
            "{currency}: {body}"
        );
        assert_eq!(
            equity["drawdown_limit_pct"],
            Value::Null,
            "{currency}: {body}"
        );
    }
}

// 검증: C31 V86 R3-1
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 체결_전인_사는_주문의_돈은_묶인_돈으로_보이고_보유는_아직_없다() {
    let bench = start().await;
    bench.ready().await;
    bench.order_buy(HYNIX).await;
    let body = account(&bench).await;

    let cash = row(&body, "cash", "currency", "KRW");
    assert!(
        dec(&cash["held"]) > Decimal::ZERO,
        "사는 주문의 돈이 묶인다: {body}"
    );
    assert!(
        dec(&cash["available"]) < d(INITIAL_KRW),
        "쓸 수 있는 돈이 줄었다: {body}"
    );
    assert_eq!(
        cash_total(&body, "KRW"),
        d(INITIAL_KRW),
        "체결 전에는 돈이 그대로다: {body}"
    );
    assert_eq!(
        body["holdings"],
        Value::Array(Vec::new()),
        "체결 전에는 보유가 없다: {body}"
    );
}

// 검증: C31 V86 R3-1 R16-5
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 체결된_뒤에는_보유_줄이_생기고_현금이_줄고_값이_내리면_평가_손익과_낙폭이_보인다() {
    let bench = start().await;
    let template = bench.ready().await;
    bench.bought(HYNIX).await;

    // 산 값 그대로의 시세: 보유 한 줄, 현금은 산 만큼 줄었다.
    let body = account(&bench).await;
    let holdings = body["holdings"].as_array().expect("보유 목록");
    assert_eq!(holdings.len(), 1, "{body}");
    let held = &holdings[0];
    assert_eq!(held["symbol"], HYNIX, "{body}");
    assert_eq!(held["market"], "kr", "{body}");
    assert_eq!(held["currency"], "KRW", "{body}");
    assert_eq!(dec(&held["quantity"]), d(QUANTITY), "{body}");
    assert!(
        dec(&held["average_price"]) >= Decimal::from(ENTRY),
        "{body}"
    );
    assert_eq!(dec(&held["last_price"]), Decimal::from(ENTRY), "{body}");
    let spent = Decimal::from(ENTRY) * d(QUANTITY);
    assert!(
        cash_total(&body, "KRW") <= d(INITIAL_KRW) - spent,
        "현금이 산 만큼(수수료 포함) 줄었다: {body}"
    );
    assert_eq!(
        cash_total(&body, "USD"),
        d(INITIAL_USD),
        "다른 통화는 그대로: {body}"
    );

    // 값이 내린다(95,000원): 평가 · 평가 손익 · 평가액 · 낙폭.
    let later = bench.now() + chrono::Duration::seconds(5);
    bench.quote(HYNIX, 95_000, later).await;
    let body = account(&bench).await;
    let held = row(&body, "holdings", "symbol", HYNIX);
    let quantity = dec(&held["quantity"]);
    let average = dec(&held["average_price"]);
    let last = dec(&held["last_price"]);
    assert_eq!(last, Decimal::from(95_000), "{body}");
    let value = dec(&held["value"]);
    assert_eq!(value, quantity * last, "평가 = 수량 × 지금 값: {body}");
    let pnl = dec(&held["pnl"]);
    assert_eq!(
        pnl,
        value - quantity * average,
        "평가 손익 = 평가 − 수량 × 평균 단가: {body}"
    );
    assert!(pnl < Decimal::ZERO, "값이 내리면 손실이다: {body}");

    let equity = row(&body, "equity", "currency", "KRW");
    let total = dec(&equity["total"]);
    assert_eq!(
        total,
        cash_total(&body, "KRW") + value,
        "평가액 = 현금 전부 + 보유 평가: {body}"
    );
    let peak = dec(&equity["peak"]);
    assert!(
        peak >= d(INITIAL_KRW),
        "최고는 처음 돈 아래로 내려가지 않는다: {body}"
    );
    assert!(peak > total, "{body}");
    let drawdown = dec(&equity["drawdown_pct"]);
    assert!(drawdown > Decimal::ZERO, "{body}");
    assert!(
        (drawdown - drawdown_of(peak, total)).abs() <= d("0.005"),
        "낙폭 % = (최고 − 지금) ÷ 최고 × 100(소수 둘째 자리): {body}"
    );
    assert_eq!(
        dec(&equity["day_pnl"]),
        total - dec(&equity["day_start"]),
        "오늘 손익 = 평가액 − 그날 시작: {body}"
    );
    assert!(dec(&equity["day_pnl"]) < Decimal::ZERO, "{body}");
    // 낙폭 한도는 지금 쓰는 템플릿의 최대 낙폭 값이다.
    assert_eq!(
        dec(&equity["drawdown_limit_pct"]),
        template_drawdown(&bench, template).await,
        "{body}"
    );
}

// 검증: C31 V86 D11
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 계좌의_금액_수량_퍼센트는_모두_십진수_글자다() {
    let bench = start().await;
    bench.ready().await;
    bench.bought(HYNIX).await;
    let body = account(&bench).await;
    let numbers: [(&str, &[&str]); 3] = [
        ("cash", &["available", "held", "unsettled"]),
        (
            "holdings",
            &["quantity", "average_price", "last_price", "value", "pnl"],
        ),
        (
            "equity",
            &[
                "total",
                "day_start",
                "day_pnl",
                "peak",
                "drawdown_pct",
                "drawdown_limit_pct",
            ],
        ),
    ];
    for (list, keys) in numbers {
        let rows = body[list]
            .as_array()
            .unwrap_or_else(|| panic!("{list}: {body}"));
        assert!(!rows.is_empty(), "{list}: {body}");
        for line in rows {
            for key in keys {
                assert!(
                    line[*key].is_string(),
                    "{list}.{key}는 글자다(수가 아니다): {line}"
                );
                dec(&line[*key]);
            }
        }
    }
}

// 검증: C31 E4 N6 V86
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 다른_기기는_세션_없이_계좌를_못_읽고_세션이_있으면_읽는다() {
    let bench = start().await;
    bench.go_remote();
    let reply = get_anonymous(bench.addr(), ACCOUNT_PATH).await;
    assert_eq!(reply.status, 401, "{}", reply.body);
    let cookie = bench.remote_login().await;
    let reply = get_with(bench.addr(), ACCOUNT_PATH, &cookie).await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["account"], ACCOUNT_PAPER, "{}", reply.body);
}
