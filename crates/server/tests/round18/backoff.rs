//! 10b 보강 5번(검토 A 참고 2): 규칙에 막힌 다시 내기는 물러선다. 같은 대기가 막히면 그 대기만 간격을 두 배로
//! (최대 10분) 늘려 다시 보고, 같은 까닭의 기록은 처음 한 번만 남긴다. 손절 대기가 10분 넘게 막혀 있으면
//! 사용자에게 "손절을 다시 내지 못하고 있습니다"를 알린다(수량 · 종목만).
//!
//! 막힌 다시 내기의 점검은 밖에서 직접 셀 수 없어, 서버가 남기는 기록(경고 이상)의 수로 센다: 이 시험 바이너리에
//! 기록 받는 곳을 하나 걸고(줄에 스레드 이름을 싣는다), 이 시험의 런타임을 한 스레드로 두어 서버의 뒤에서 도는
//! 일이 남긴 기록이 이 시험 스레드의 이름으로 남게 한다. 다른 시험의 줄은 스레드 이름으로 걸러 낸다.

use std::io;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::time::{Duration, Instant};

use pi_paper::Fault;
use price_inha::{DemoTurn, NOTICES_PATH, TRADE_ROOM_PREFIX};
use serde_json::json;
use tracing_subscriber::fmt::MakeWriter;

use crate::common::{Bench, ENTRY, HYNIX, Plan, STEP, STOP, get, kst, monday, plus, verdict};

/// 손절을 다시 내지 못할 때의 알림(지시서 글자 그대로).
const STUCK_STOP: &str = "손절을 다시 내지 못하고 있습니다";
/// 전체 정지로 팀장 팔기 대기를 거둘 때 그 매매의 방에 남기는 글(16차 1번, 지시서 글자 그대로).
const WITHDRAWN: &str = "전체 정지로 팀장 팔기 다시 내기를 거둠";
/// 1초씩 넣는 시세의 수(물러서기가 없으면 걸음마다 여러 번 점검한다).
const TICKS: usize = 120;
/// 120초 동안 물러서며 다시 볼 때의 점검 수(1 · 2 · 4 · … · 64초)에 여유를 둔 상한.
const MAX_RECORDS: usize = 10;

/// 서버가 남긴 기록 줄을 모으는 곳.
#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl Captured {
    /// 이 바이너리의 기록 받는 곳(처음 부를 때 건다).
    fn global() -> &'static Captured {
        static CAPTURED: OnceLock<Captured> = OnceLock::new();
        CAPTURED.get_or_init(|| {
            let captured = Captured::default();
            let subscriber = tracing_subscriber::fmt()
                .with_writer(captured.clone())
                .with_max_level(tracing::Level::WARN)
                .with_thread_names(true)
                .with_ansi(false)
                .finish();
            tracing::subscriber::set_global_default(subscriber).expect("기록 받는 곳은 하나다");
            captured
        })
    }

    /// 지금까지 받은 글의 길이.
    fn len(&self) -> usize {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).len()
    }

    /// `from` 뒤에 받은 줄 가운데 `thread` 스레드에서 `price_inha`가 남긴 줄의 수.
    fn server_lines(&self, from: usize, thread: &str) -> usize {
        let bytes = self
            .0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        String::from_utf8_lossy(&bytes[from..])
            .lines()
            .filter(|line| line.contains(thread) && line.contains("price_inha"))
            .count()
    }
}

impl io::Write for Captured {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut held = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        held.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for Captured {
    type Writer = Captured;

    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

/// 그 매매의 방에 남은 줄 가운데 팀장 팔기 대기를 거뒀다는 줄의 수.
fn withdrawn_lines(bench: &Bench, trade: pi_store::TradeId) -> usize {
    let room = format!("{TRADE_ROOM_PREFIX}{}", trade.0);
    bench
        .store()
        .room_events(&room, 0, 1000)
        .unwrap()
        .iter()
        .filter(|event| event.payload.to_string().contains(WITHDRAWN))
        .count()
}

/// 알림 목록 가운데 손절을 다시 내지 못한다는 것들(제목 + 본문).
async fn stuck_notices(bench: &Bench) -> Vec<String> {
    let reply = get(bench.addr(), &format!("{NOTICES_PATH}?after=0&max=200")).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    let list = reply.body.as_array().cloned().unwrap_or_default();
    list.iter()
        .map(|notice| format!("{} {}", notice["title"], notice["body"]))
        .filter(|text| text.contains(STUCK_STOP))
        .collect()
}

// 16차 1번으로 고침: 전체 정지가 되면 막힌 팀장 팔기 대기는 지워져 다시 점검하지 않는다(이름은 그대로 둔다).
// 16차 1번으로 고침: 전체 정지가 되면 막힌 팀장 팔기 대기는 지워져 다시 점검하지 않는다(이름은 그대로 둔다).
// 검증: E30 E13 F4 C10 E34 C8
#[tokio::test]
async fn 전체_정지_중_막힌_팀장_팔기_대기는_1초마다_다시_점검하지_않고_물러선다() {
    let bench = Plan::demo().start(kst(monday(), 10, 0)).await;
    bench.data.disclose(HYNIX, "20261116000123");
    bench.ready().await;
    let held = bench.holding(HYNIX).await;

    // 모의계좌가 주문을 받지 않는 동안 팀장이 팔기로 판단한다: 팀장 팔기가 다시 내기를 기다린다.
    bench.script(DemoTurn::Verdict, &verdict("sell"));
    let draft = json!({"side": "sell", "note": "보유 정리"});
    bench.script(DemoTurn::Draft, &draft.to_string());
    bench.trading().paper().set_fault(Some(Fault::RejectOrders));
    bench.quote(HYNIX, ENTRY, plus(bench.now(), 1)).await;
    let _failed = bench.crew().discuss(HYNIX, "실적 전망 하향 공시").await;
    assert_eq!(bench.pending_sells(held).len(), 1, "팀장 팔기가 기다린다");

    // 전체 정지가 되면 팀장 팔기 대기는 거둔다(손절 · 청산만 나간다, 16차 1번). 계좌는 돌아왔다.
    bench.full_stop();
    bench.trading().paper().set_fault(None);
    let captured = Captured::global();
    let thread = std::thread::current()
        .name()
        .expect("시험 스레드 이름")
        .to_string();
    let from = captured.len();
    for _ in 0..TICKS {
        bench.quote(HYNIX, ENTRY, plus(bench.now(), 1)).await;
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let lines = captured.server_lines(from, &thread);

    assert!(
        bench.sells(HYNIX).await.is_empty(),
        "막힌 팀장 팔기는 나가지 않는다"
    );
    assert!(
        bench.pending_sells(held).is_empty(),
        "전체 정지가 되면 팀장 팔기 다시 내기 대기는 지워진다"
    );
    assert_eq!(
        withdrawn_lines(&bench, held),
        1,
        "거뒀다는 줄은 그 매매의 방에 한 번만 남는다(점검이 되풀이돼도 늘지 않는다)"
    );
    assert!(
        lines <= MAX_RECORDS,
        "대기가 없으니 다시 점검하며 기록을 쌓지 않는다: 기록 {lines}줄"
    );
    bench.close().await;
}

// 검증: E30 E34 F4 N5
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 손절_대기가_10분_넘게_막히면_손절을_다시_내지_못하고_있다고_알린다() {
    let bench = Plan::demo().start(kst(monday(), 15, 10)).await;
    bench.ready().await;
    let held = bench.holding(HYNIX).await;

    // 장 마감 직전 모의계좌가 주문을 받지 않는 동안 손절값에 닿는다: 손절이 다시 내기를 기다린다.
    bench.trading().paper().set_fault(Some(Fault::RejectOrders));
    bench
        .quote(HYNIX, STOP - 1_000, kst(monday(), 15, 29))
        .await;
    let pending = bench.pending_sells(held);
    assert_eq!(pending.len(), 1, "실패한 손절이 기다린다: {pending:?}");
    assert_eq!(pending[0]["purpose"], json!("stop_loss"));

    // 장이 닫혀 다시 내기는 규칙에 막힌다(장 시간 밖). 계좌는 돌아왔다.
    bench.set(kst(monday(), 15, 31));
    bench.trading().paper().set_fault(None);
    bench.advance(5 * 60).await;
    assert!(
        stuck_notices(&bench).await.is_empty(),
        "10분이 되기 전에는 알리지 않는다"
    );

    let deadline = Instant::now() + STEP;
    while stuck_notices(&bench).await.is_empty() {
        assert!(
            Instant::now() < deadline,
            "손절 대기가 10분 넘게 막히면 알려야 한다"
        );
        bench.advance(20).await;
    }
    let notices = stuck_notices(&bench).await;
    assert!(
        notices.iter().any(|text| text.contains(HYNIX)),
        "알림에 종목이 있다: {notices:?}"
    );
    assert!(
        bench.sells(HYNIX).await.is_empty(),
        "손절은 아직 나가지 못했다"
    );
    bench.close().await;
}
