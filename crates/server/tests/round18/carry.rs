//! 0번 10a 재검토에서 넘어온 것(돈을 지키는 쪽, `docs/dev/modules/server.md` 10b 0번).
//!
//! - (가) 실패한 파는 주문 다시 내기(E30): 실패한 손절 · 팀장 팔기가 실행기의 `pending`에 남으면 server가 때가
//!   되면 `Executor::retry_sell`로 다시 낸다.
//! - (나) 시세 흐름이 보유 종목도 받는다: 지켜보는 목록에 없어도 손절이 나가고, 그 종목 시세가 끊기면 이상 감지
//!   멈춤이 되며, 보유가 남아 있는 동안 지켜보는 목록에서 빼도 시세를 계속 받는다.
//! - (다) 매수가 막힌 까닭: 쓰는 템플릿이 멈춤이거나 없으면 다시 시작 응답 · 팀 상태 화면 · `doctor`에
//!   "새 템플릿을 고르세요".
//! - (라) 자료 이상 멈춤 ↔ 풀림이 번갈아 생겨도 같은 까닭의 알림은 한 시간에 한 번.
//!
//! (마) 개선안 답과 하루 마감의 승격 판정이 겹치는 경우는 두 일을 정해진 차례로 겹치게 만들 공개 길이 없어
//! 쓰지 않았다(우연한 동시 실행은 구현 전에도 통과할 수 있어 red를 보장하지 못한다).

use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use pi_approval::{ApprovalNumber, ApprovalView, Subject};
use pi_paper::Fault;
use pi_rules::Purpose;
use pi_store::{ApprovalStatus, NewTemplate, TeamState, TemplateId, TemplateState, TradeState};
use pi_template::{PromotionCriteria, Style, Template};
use price_inha::{
    APPROVAL_ANSWER_PATH, CommandLine, CommandOutput, CommandRunner, DemoTurn, KIND_DAY_CLOSE,
    KIND_DAY_START, NOTICES_PATH, Os, Placed, SETUP_SYMBOLS_PATH, Service, ServiceSetup, Settings,
    TEAM_PATH, TEAM_RESTART_PATH, doctor,
};
use rust_decimal::Decimal;
use serde_json::{Value, json};

use crate::common::{
    Bench, ENTRY, HYNIX, Plan, STEP, STOP, WATCHED, d, get, kst, monday, ms, on_date, order, plus,
    post, settle, tuesday, verdict, wait_for,
};

/// 쓰는 템플릿이 멈춤이거나 없을 때의 안내(지시서 그대로).
const CHOOSE_TEMPLATE: &str = "새 템플릿을 고르세요";
const TOPIC: &str = "실적 전망 하향 공시";

// ───────────── (가) 실패한 파는 주문 다시 내기 ─────────────

/// 시세를 1초씩 넣으며 `done`이 될 때까지 기다린다(다시 내기의 간격이 시계로 지나가게).
async fn quote_until(bench: &Bench, price: i64, what: &str, done: impl Fn(&Bench) -> bool) {
    let deadline = Instant::now() + STEP;
    while !done(bench) {
        assert!(Instant::now() < deadline, "시간 안에 되지 않았다: {what}");
        bench.quote(HYNIX, price, plus(bench.now(), 1)).await;
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

// 검증: E30 E34 F4 C10
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 실패한_손절이_다시_내기를_기다리면_때가_되어_다시_나가_팔린다() {
    let bench = Plan::demo().start(kst(monday(), 10, 0)).await;
    bench.ready().await;
    let held = bench.holding(HYNIX).await;

    // 모의계좌가 주문을 받지 않는 동안 손절값에 닿는다: 손절이 실패해 다시 내기를 기다린다.
    bench.trading().paper().set_fault(Some(Fault::RejectOrders));
    bench.quote(HYNIX, STOP - 1_000, plus(bench.now(), 1)).await;
    let pending = bench.pending_sells(held);
    assert_eq!(pending.len(), 1, "실패한 손절이 기다린다: {pending:?}");
    assert_eq!(pending[0]["purpose"], json!("stop_loss"));
    bench.trading().paper().set_fault(None);

    // 계좌가 돌아오면 server가 때가 되어 다시 내고, 다음 시세에 체결된다.
    quote_until(
        &bench,
        STOP - 1_000,
        "다시 낸 손절이 팔림",
        |bench| bench.trade_state(held) == TradeState::Sold,
    )
    .await;
    assert!(
        bench.pending_sells(held).is_empty(),
        "다시 낸 뒤에는 기다리는 것이 없다"
    );
    assert_eq!(
        bench.sells(HYNIX).await.len(),
        1,
        "파는 주문은 한 번 나갔다"
    );
    bench.close().await;
}

// 검증: E30 E13 F4 C10
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 실패한_팀장_팔기가_다시_내기를_기다리면_때가_되어_다시_나간다() {
    let bench = Plan::demo().start(kst(monday(), 10, 0)).await;
    bench.data.disclose(HYNIX, "20261116000123");
    bench.ready().await;
    let held = bench.holding(HYNIX).await;

    bench.script(DemoTurn::Verdict, &verdict("sell"));
    let draft = json!({"side": "sell", "note": "보유 정리"});
    bench.script(DemoTurn::Draft, &draft.to_string());
    bench.trading().paper().set_fault(Some(Fault::RejectOrders));
    bench.quote(HYNIX, ENTRY, plus(bench.now(), 1)).await;
    let _failed = bench.crew().discuss(HYNIX, TOPIC).await;
    let pending = bench.pending_sells(held);
    assert_eq!(pending.len(), 1, "실패한 팀장 팔기가 기다린다: {pending:?}");
    assert_eq!(pending[0]["purpose"], json!("team_exit"));
    assert!(bench.sells(HYNIX).await.is_empty(), "아직 나간 매도가 없다");
    bench.trading().paper().set_fault(None);

    let deadline = Instant::now() + STEP;
    while bench.sells(HYNIX).await.is_empty() {
        assert!(Instant::now() < deadline, "다시 낸 팀장 팔기가 나가야 한다");
        bench.quote(HYNIX, ENTRY, plus(bench.now(), 1)).await;
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(
        bench.pending_sells(held).is_empty(),
        "다시 낸 뒤에는 기다리는 것이 없다"
    );
    let sells = bench.sells(HYNIX).await;
    assert_eq!(sells.len(), 1, "팀장 팔기는 한 번만 나간다: {sells:?}");
    assert_eq!(sells[0].quantity, d("5"), "보유 수량을 판다");
    bench.close().await;
}

// ───────────── (나) 시세 흐름이 보유 종목도 받는다 ─────────────

/// 시계를 1초씩 움직이며 `done`이 될 때까지 기다린다(모의계좌는 주문 뒤에 받은 시세로만 체결하므로, 시세 흐름이
/// 가짜 시세 서버에서 주문 뒤 시각의 시세를 받아 오게).
async fn tick_until(bench: &Bench, what: &str, done: impl Fn(&Bench) -> bool) {
    let deadline = Instant::now() + STEP;
    while !done(bench) {
        assert!(Instant::now() < deadline, "시간 안에 되지 않았다: {what}");
        bench.set(plus(bench.now(), 1));
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

// 검증: F4 E34 N6 C10
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 지켜보는_목록에_없는_보유_종목도_시세_흐름이_받아_손절이_나간다() {
    let bench = Plan::live()
        .watching(&[WATCHED])
        .start(kst(monday(), 10, 0))
        .await;
    bench.ready().await;
    let held = bench.holding(HYNIX).await;

    // 테스트는 더 넣지 않는다: 시세 흐름이 가짜 시세 서버에서 그 종목의 손절값 아래 시세를 받아야 한다.
    bench.set_price(HYNIX, STOP - 1_000);
    tick_until(
        &bench,
        "시세 흐름이 받은 시세로 손절이 나가 팔림",
        |bench| bench.trade_state(held) == TradeState::Sold,
    )
    .await;
    let asked = bench.asked_symbols().await;
    assert!(
        asked.iter().any(|symbols| symbols.contains(HYNIX)),
        "시세 흐름이 보유 종목을 물었다: {asked:?}"
    );
    bench.close().await;
}

// 검증: N6 V11 P11 E14
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 보유_종목의_시세가_끊기면_이상_감지_멈춤이_된다() {
    let bench = Plan::live()
        .watching(&[WATCHED])
        .start(kst(monday(), 10, 0))
        .await;
    bench.ready().await;
    bench.set_price(HYNIX, ENTRY);
    let _held = bench.holding(HYNIX).await;
    bench.advance(40).await;
    assert_eq!(
        bench.team(),
        TeamState::Operating,
        "보유 종목의 시세가 오는 동안은 운용 중이다"
    );

    // 지켜보는 종목의 시세는 계속 오지만 보유 종목의 시세가 끊긴다.
    bench.drop_price(HYNIX);
    bench.advance(40).await;
    wait_for(
        "보유 종목 시세가 끊겨 이상 감지 멈춤",
        || bench.team() == TeamState::AnomalyHalt,
    )
    .await;
    bench.close().await;
}

// 검증: F4 E34 N6 C3
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 보유_종목을_지켜보는_목록에서_빼도_시세를_계속_받아_손절이_나간다() {
    let bench = Plan::live()
        .watching(&[WATCHED, HYNIX])
        .start(kst(monday(), 10, 0))
        .await;
    bench.set_price(HYNIX, ENTRY);
    bench.ready().await;
    let held = bench.holding(HYNIX).await;

    // 보유가 남아 있는 동안 그 종목을 목록에서 빼는 요청은 거부되거나 효과가 없다(어느 쪽이든 시세를 계속 받는다).
    let reply = post(
        bench.addr(),
        SETUP_SYMBOLS_PATH,
        &json!({"symbols": [WATCHED]}),
    )
    .await;
    assert!(
        reply.status == 200 || (400..500).contains(&reply.status),
        "거부하거나 받는다: {} {}",
        reply.status,
        reply.text
    );
    let before = bench.asked_symbols().await.len();
    let deadline = Instant::now() + STEP;
    while bench.asked_symbols().await.len() < before + 5 {
        assert!(Instant::now() < deadline, "시세 흐름이 계속 돈다");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    bench.set_price(HYNIX, STOP - 1_000);
    tick_until(
        &bench,
        "목록에서 빼려 한 보유 종목의 손절이 나가 팔림",
        |bench| bench.trade_state(held) == TradeState::Sold,
    )
    .await;
    bench.close().await;
}

// ───────────── (다) 매수가 막힌 까닭: 새 템플릿을 고르세요 ─────────────

// 검증: E9 C16 F2
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 템플릿을_고르지_않았으면_팀_상태_화면에_새_템플릿을_고르라고_나온다() {
    let bench = Plan::demo().start(kst(monday(), 10, 0)).await;
    let view = get(bench.addr(), TEAM_PATH).await;
    assert_eq!(view.status, 200, "{}", view.text);
    assert!(
        view.text.contains(CHOOSE_TEMPLATE),
        "쓰는 템플릿이 없으면 안내가 있다: {}",
        view.text
    );

    bench.ready().await;
    let view = get(bench.addr(), TEAM_PATH).await;
    assert!(
        !view.text.contains(CHOOSE_TEMPLATE),
        "템플릿을 고르면 안내가 없다: {}",
        view.text
    );
    bench.close().await;
}

// 검증: E9 C16 F2
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 템플릿_없이_전체_정지에서_다시_시작하면_응답에_새_템플릿을_고르라고_나온다() {
    let bench = Plan::demo().start(kst(monday(), 10, 0)).await;
    bench.trade_today();
    bench.full_stop();

    let reply = post(bench.addr(), TEAM_RESTART_PATH, &json!(null)).await;
    assert_eq!(reply.status, 200, "다시 시작은 된다: {}", reply.text);
    assert!(
        reply.text.contains(CHOOSE_TEMPLATE),
        "쓰는 템플릿이 없으면 다시 시작 응답에 안내가 있다: {}",
        reply.text
    );
    bench.close().await;
}

/// 매매 1건 · 거래일 1일 · 손익 0 초과만 보는 스윙 템플릿을 저장소에 만들고(가장 나중에 만든 템플릿을 쓴다)
/// 오늘 매매하기로 한다.
fn loose_template(bench: &Bench) -> TemplateId {
    let mut template = Template::new(Style::Swing);
    template.params.promotion = PromotionCriteria {
        min_trades: 1,
        min_trading_days: 1,
        net_pnl_above: Decimal::ZERO,
        max_drawdown_pct: d("100"),
        out_of_sample_pnl_above: None,
    };
    let value: Value = serde_json::from_str(&template.to_json()).unwrap();
    let saved = bench
        .store()
        .create_template(NewTemplate {
            value,
            at: ms(bench.now()),
        })
        .unwrap();
    bench.trade_today();
    saved.id
}

/// 5주를 10만 원에 사서 10만 5천 원에 익절로 판다(수수료 · 세금을 빼도 손익이 0보다 크다).
async fn profitable_trade(bench: &Bench) {
    let trade = bench.holding(HYNIX).await;
    let at = plus(bench.now(), 1);
    bench.quote(HYNIX, ENTRY + 5_000, at).await;
    let placed = bench
        .place(order(trade, HYNIX, Purpose::TakeProfit, ENTRY + 5_000, at))
        .await;
    assert!(matches!(placed, Placed::Ordered(_)), "판다: {placed:?}");
    bench.quote(HYNIX, ENTRY + 5_000, plus(at, 1)).await;
    assert_eq!(bench.trade_state(trade), TradeState::Sold);
}

fn template_state(bench: &Bench, id: TemplateId) -> TemplateState {
    bench.store().get_template(id).unwrap().state
}

/// 그 템플릿의 실전 승격을 묻는, 기다리는 승인 요청.
fn promotion_request(bench: &Bench, id: TemplateId) -> Option<ApprovalView> {
    let approvals = bench.approvals();
    bench
        .store()
        .approvals_in_status(ApprovalStatus::Pending)
        .unwrap()
        .iter()
        .filter_map(|row| row.summary["number"].as_str())
        .filter_map(|number| approvals.get(&ApprovalNumber::new(number)).unwrap())
        .find(|view| matches!(view.subject, Subject::LivePromotion { template, .. } if template == id))
}

/// 월요일을 마감해 승격대기가 되게 하고, 승격 승인 요청에 승인으로 답해 실전중으로 만든 뒤 화요일 장을 연다.
async fn live_on_tuesday(bench: &Bench, id: TemplateId) -> DateTime<Utc> {
    bench.set(kst(monday(), 15, 31));
    let store = bench.store();
    wait_for("월요일 마감", || {
        on_date(&store, KIND_DAY_CLOSE, monday()) == 1
    })
    .await;
    wait_for("승격 승인 요청", || {
        promotion_request(bench, id).is_some()
    })
    .await;
    let request = promotion_request(bench, id).unwrap();
    let body = json!({
        "number": request.number.as_str(),
        "fingerprint": request.fingerprint.as_str(),
        "answer": "approve",
    });
    let reply = post(bench.addr(), APPROVAL_ANSWER_PATH, &body).await;
    assert_eq!(reply.body["result"], json!("approved"), "{}", reply.text);
    assert_eq!(template_state(bench, id), TemplateState::Live, "실전중이다");

    settle().await;
    let open = kst(tuesday(), 9, 1);
    bench.set(open);
    wait_for("화요일 시작", || {
        on_date(&store, KIND_DAY_START, tuesday()) >= 1
    })
    .await;
    settle().await;
    open
}

// 검증: E9 C16 F2 F6
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 쓰는_템플릿이_전체_정지로_멈추면_다시_시작_응답과_팀_상태에_새_템플릿을_고르라고_나온다() {
    let bench = Plan::demo().start(kst(monday(), 10, 0)).await;
    let id = loose_template(&bench);
    profitable_trade(&bench).await;
    live_on_tuesday(&bench, id).await;
    bench.trade_today();
    bench.full_stop();
    assert_eq!(
        template_state(&bench, id),
        TemplateState::Stopped,
        "전체 정지가 되면 실전중 템플릿은 멈춤이 된다"
    );

    let reply = post(bench.addr(), TEAM_RESTART_PATH, &json!(null)).await;
    assert_eq!(reply.status, 200, "다시 시작은 된다: {}", reply.text);
    assert!(
        reply.text.contains(CHOOSE_TEMPLATE),
        "쓰는 템플릿이 멈춤이면 다시 시작 응답에 안내가 있다: {}",
        reply.text
    );
    let view = get(bench.addr(), TEAM_PATH).await;
    assert!(
        view.text.contains(CHOOSE_TEMPLATE),
        "팀 상태 화면에도 안내가 있다: {}",
        view.text
    );
    bench.close().await;
}

/// 모든 명령에 성공으로 답하는 가짜 실행기(실제 서비스 관리자를 부르지 않는다).
struct OkRunner;

impl CommandRunner for OkRunner {
    fn run(&self, _command: &CommandLine) -> io::Result<CommandOutput> {
        Ok(CommandOutput {
            success: true,
            stdout: String::new(),
            stderr: String::new(),
        })
    }
}

/// 꺼진 보관 폴더 `home`을 진단한 글.
fn diagnose(home: &Path) -> String {
    let definitions = tempfile::tempdir().unwrap();
    let setup = ServiceSetup {
        os: Os::MacOs,
        exe: PathBuf::from("/opt/price-inha/bin/price-inha"),
        home: home.to_path_buf(),
        definition_dir: definitions.path().to_path_buf(),
        uid: 501,
        path_env: "/usr/local/bin:/usr/bin:/bin".to_string(),
        run_args: Vec::new(),
    };
    let runner = OkRunner;
    let service = Service::new(setup, &runner);
    let args: Vec<String> = ["--home", home.to_str().unwrap(), "--port", "0"]
        .iter()
        .map(|arg| (*arg).to_string())
        .collect();
    let settings = Settings::parse(&args, &|_| None, None).unwrap();
    doctor(&settings, &service).render()
}

// 검증: D1 E9 F2
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 진단은_쓰는_템플릿이_없으면_새_템플릿을_고르라고_알린다() {
    let empty = Plan::demo().start(kst(monday(), 10, 0)).await;
    let empty = empty.stop().await;
    let text = diagnose(empty.path());
    assert!(
        text.contains(CHOOSE_TEMPLATE),
        "템플릿이 없으면 진단에 안내가 있다: {text}"
    );

    let chosen = Plan::demo().start(kst(monday(), 10, 0)).await;
    chosen.ready().await;
    let chosen = chosen.stop().await;
    let text = diagnose(chosen.path());
    assert!(
        !text.contains(CHOOSE_TEMPLATE),
        "쓸 수 있는 템플릿이 있으면 안내가 없다: {text}"
    );
}

// ───────────── (라) 자료 이상 알림 묶기 ─────────────

/// 알림 목록 가운데 자료 이상을 알리는 것의 수.
async fn data_notices(bench: &Bench) -> usize {
    let reply = get(bench.addr(), &format!("{NOTICES_PATH}?after=0&max=200")).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    let list = reply.body.as_array().cloned().unwrap_or_default();
    list.iter()
        .filter(|notice| {
            let text = format!("{}{}", notice["title"], notice["body"]);
            text.contains("자료")
        })
        .count()
}

/// 지금 시세로 [`HYNIX`]를 세 번 논의해(자료 출처가 모두 실패한다) 자료 이상으로 멈추게 한다.
async fn halt_by_data(bench: &Bench) {
    for _ in 0..3 {
        bench.quote(HYNIX, ENTRY, plus(bench.now(), 1)).await;
        let _watching = bench.crew().discuss(HYNIX, TOPIC).await;
    }
    wait_for("자료 이상으로 이상 감지 멈춤", || {
        bench.team() == TeamState::AnomalyHalt
    })
    .await;
}

/// 마지막 실패 뒤 10분이 넘게 지나 자료 이상이 풀리고, 정상 60초가 지나 그날 계획대로 운용 중이 된다.
async fn clear_by_time(bench: &Bench, to: DateTime<Utc>) {
    bench.set(to);
    settle().await;
    bench.advance(100).await;
    wait_for("자료 이상이 스스로 풀려 운용 중", || {
        bench.team() == TeamState::Operating
    })
    .await;
}

// 검증: N5 E14 P11 V11
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 자료_이상_멈춤이_한_시간_안에_다시_생겨도_같은_까닭의_알림은_한_번이다() {
    let start = kst(monday(), 10, 0);
    let bench = Plan::live()
        .watching(&[WATCHED])
        .polling(Duration::from_secs(86_400))
        .start(start)
        .await;
    bench.data.disclose(HYNIX, "20261116000123");
    bench.ready().await;
    bench.data.fail();

    halt_by_data(&bench).await;
    let deadline = Instant::now() + STEP;
    while data_notices(&bench).await == 0 {
        assert!(Instant::now() < deadline, "자료 이상 멈춤 알림이 가야 한다");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(data_notices(&bench).await, 1, "처음 멈춤의 알림 하나");

    // 출처는 계속 죽어 있다: 시간이 지나 풀렸다가 논의에서 다시 실패해 또 멈춘다(한 시간 안).
    clear_by_time(&bench, plus(start, 11 * 60)).await;
    halt_by_data(&bench).await;
    settle().await;
    settle().await;
    assert_eq!(
        data_notices(&bench).await,
        1,
        "한 시간 안에 같은 까닭으로 다시 멈춰도 알림을 더 보내지 않는다"
    );

    // 한 시간이 지난 뒤 다시 멈추면 다시 알린다.
    clear_by_time(&bench, plus(start, 62 * 60)).await;
    halt_by_data(&bench).await;
    let deadline = Instant::now() + STEP;
    while data_notices(&bench).await < 2 {
        assert!(Instant::now() < deadline, "한 시간 뒤의 멈춤은 다시 알린다");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(data_notices(&bench).await, 2);
    bench.close().await;
}
