//! 9번 뉴스 · FRED(data 4 · 5차, 결정 2026-11-21 3 · 16번): 네이버 뉴스를 `NewsRegistry`에 등록하고 키는 비밀
//! 보관소에서 읽는다. 논의 자료에 뉴스(제목 · 요약 · 링크 · 시각)와 출처를 싣고 판단 기록에 남긴다(V35).
//! 링크가 이상한 기사만 버리고(`NewsFetched::Partial`) 남은 기사를 쓰며, 버린 수를 판단 기록 · 방 로그에 남긴다.
//! 남은 것이 없으면 "뉴스 없음"이다. 논의할 때 FRED 미국 금리(`FactRequest::UsRate`)도 묻는다.
//!
//! 시연 모드로 띄운다(가짜 AI는 시연용, 자료 · 뉴스는 로컬 가짜 서버).

use pi_store::{StoredFact, Trade};
use price_inha::{DemoTurn, Discussed, TRADE_ROOM_PREFIX};

use crate::common::{
    Bench, ENTRY, FRED_KEY, FRED_PATH, HYNIX, NAVER_ID, NAVER_SECRET, NEWS_PATH, Plan, article,
    kst, monday, verdict,
};

const TITLE: &str = "시험회사 자사주 소각 결정";
const SUMMARY: &str = "시험회사가 보유 자사주를 모두 소각하기로 했다";
const LINK: &str = "https://news.example.test/articles/7";
/// 링크가 이상해 버려질 기사의 제목.
const BAD_TITLE: &str = "버려질 기사 제목";
/// 버려질 기사 수(다른 숫자와 헷갈리지 않는 값).
const BAD_COUNT: usize = 13;

/// 공시 · 뉴스를 넣고 [`HYNIX`]를 사기로 판단하는 논의를 한다.
async fn discuss_buy(bench: &Bench, news: Vec<serde_json::Value>) -> Discussed {
    bench.data.disclose(HYNIX, "20261116000123");
    bench.data.set_news(news);
    bench.script(DemoTurn::Verdict, &verdict("buy"));
    bench.quote(HYNIX, ENTRY, bench.now()).await;
    bench
        .crew()
        .discuss(HYNIX, "자기주식취득결정 공시")
        .await
        .expect("논의가 끝나야 한다")
}

/// 링크가 이상해 버려질 기사들.
fn bad_articles() -> Vec<serde_json::Value> {
    (0..BAD_COUNT)
        .map(|_| article(BAD_TITLE, "버려질 요약", "javascript:void(0)"))
        .collect()
}

/// 판단 기록(매매의 근거 · 자료 목록과 붙은 자료)을 글로.
fn record_text(trade: &Trade, facts: &[StoredFact]) -> String {
    format!("{:?} {:?} {facts:?}", trade.rationale, trade.sources)
}

/// 글 안에 그 수가 따로 선 숫자로 있는가.
fn has_number(text: &str, number: usize) -> bool {
    let wanted = number.to_string();
    text.split(|c: char| !c.is_ascii_digit())
        .any(|token| token == wanted)
}

// 검증: V35 V36 D9 C11
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 논의_자료와_판단_기록에_뉴스의_제목_요약_링크_시각과_출처가_남는다() {
    let bench = Plan::demo()
        .with_naver()
        .with_fred()
        .start(kst(monday(), 10, 0))
        .await;
    bench.ready().await;
    let discussed = discuss_buy(&bench, vec![article(TITLE, SUMMARY, LINK)]).await;

    let prompts = bench.prompts().join("\n");
    assert!(prompts.contains(TITLE), "논의 자료에 뉴스 제목이 실린다");
    assert!(prompts.contains(SUMMARY), "논의 자료에 뉴스 요약이 실린다");
    assert!(prompts.contains(LINK), "논의 자료에 뉴스 링크가 실린다");

    let store = bench.store();
    let trade = store.get_trade(discussed.trade).unwrap();
    let decided = trade.decided_at.expect("판단 시각");
    let facts = store.trade_facts(discussed.trade).unwrap();
    let news = facts
        .iter()
        .find(|fact| format!("{fact:?}").contains(TITLE))
        .expect("판단 기록에 뉴스 자료가 붙는다");
    let text = format!("{news:?}");
    assert!(text.contains(SUMMARY), "요약이 남는다: {text}");
    assert!(text.contains(LINK), "링크가 남는다: {text}");
    assert!(news.published_on.is_some(), "기사 시각이 남는다: {text}");
    assert!(!news.source_name.is_empty(), "출처가 남는다: {text}");
    assert!(news.known_at <= decided, "판단보다 늦게 안 뉴스가 아니다");
    assert!(
        !text.contains(NAVER_SECRET) && !text.contains(NAVER_ID),
        "키 글자가 판단 기록에 없다"
    );
    let searched = bench.data.requests(NEWS_PATH).await;
    assert!(!searched.is_empty(), "네이버 뉴스 검색을 불렀다");
    bench.close().await;
}

// 검증: V35 E14 E32 D9
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 링크가_이상한_기사만_버리고_남은_기사를_쓰며_버린_수가_판단_기록과_방_로그에_남는다() {
    let bench = Plan::demo()
        .with_naver()
        .with_fred()
        .start(kst(monday(), 10, 0))
        .await;
    bench.ready().await;
    let mut news = bad_articles();
    news.push(article(TITLE, SUMMARY, LINK));
    let discussed = discuss_buy(&bench, news).await;

    let prompts = bench.prompts().join("\n");
    assert!(prompts.contains(TITLE), "남은 기사를 논의 자료로 쓴다");
    assert!(!prompts.contains(BAD_TITLE), "버린 기사는 싣지 않는다");

    let store = bench.store();
    let trade = store.get_trade(discussed.trade).unwrap();
    let facts = store.trade_facts(discussed.trade).unwrap();
    let record = record_text(&trade, &facts);
    assert!(record.contains(TITLE), "남은 기사가 판단 기록에 있다");
    assert!(
        has_number(&record, BAD_COUNT),
        "버린 기사 수({BAD_COUNT})가 판단 기록에 남는다: {record}"
    );

    let room = format!("{TRADE_ROOM_PREFIX}{}", discussed.trade.0);
    let events = store.room_events(&room, 0, 10_000).unwrap();
    let log: Vec<String> = events.iter().map(|e| e.payload.to_string()).collect();
    let log = log.join("\n");
    assert!(
        has_number(&log, BAD_COUNT),
        "버린 기사 수({BAD_COUNT})가 그 매매의 방 로그에 남는다: {log}"
    );
    bench.close().await;
}

// 검증: V35 E32
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 남은_기사가_없으면_뉴스_없음으로_논의한다() {
    let bench = Plan::demo()
        .with_naver()
        .with_fred()
        .start(kst(monday(), 10, 0))
        .await;
    bench.ready().await;
    let _discussed = discuss_buy(&bench, bad_articles()).await;

    let prompts = bench.prompts().join("\n");
    assert!(
        prompts.contains("뉴스 없음"),
        "버리고 남은 기사가 없으면 뉴스 없음이다"
    );
    assert!(!prompts.contains(BAD_TITLE), "버린 기사는 싣지 않는다");
    bench.close().await;
}

// 검증: V34 D9 C11
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 논의할_때_fred_미국_금리를_묻고_판단_기록에_출처와_함께_남긴다() {
    let bench = Plan::demo().with_fred().start(kst(monday(), 10, 0)).await;
    bench.ready().await;
    let discussed = discuss_buy(&bench, Vec::new()).await;

    let asked = bench.data.requests(FRED_PATH).await;
    assert!(!asked.is_empty(), "FRED 미국 금리를 묻는다");
    let query = asked[0].url.query().unwrap_or_default().to_string();
    assert!(query.contains("series_id="), "계열 id를 준다: {query}");

    let facts = bench.store().trade_facts(discussed.trade).unwrap();
    let us_rate = facts
        .iter()
        .find(|fact| fact.source_name == "FRED")
        .expect("판단 기록에 FRED 자료가 붙는다");
    assert!(
        us_rate.source_url.starts_with(&bench.data.uri()),
        "출처 주소는 받은 곳(로컬 가짜 서버)이다: {}",
        us_rate.source_url
    );
    assert!(
        !us_rate.source_url.contains(FRED_KEY),
        "출처 주소에 키가 없다"
    );
    bench.close().await;
}
