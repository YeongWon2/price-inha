//! data 4차: 뉴스 공급자(네이버 뉴스 검색) · 등록부 · 뉴스 값.
//! 잠긴 옛 테스트(`tests/suite/`)는 고치지 않고 이 파일에 쓴다.
//! 로컬 가짜 서버(wiremock)만 쓴다. 키는 진짜처럼 보이지 않는 글자다.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::{DateTime, Utc};
use pi_data::{
    Clock, DEFAULT_NEWS_MAX_BODY_BYTES, DEFAULT_TIMEOUT, DataAdapter, FactKind, FactRequest,
    FakeDataAdapter, FakeNewsProvider, Fetched, NAVER_NEWS_KEYS, NaverNews, NaverNewsConfig,
    NewsError, NewsFetched, NewsItem, NewsKeySource, NewsKeys, NewsProvider, NewsProviderId,
    NewsRegistry, NewsRequest, NewsSort, SourceName, clean_news_text, verify_news_keys,
};
use pi_secrets::{Credentials, VerifyFailure};
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const CLIENT_ID: &str = "test-naver-id-not-real-01";
const CLIENT_SECRET: &str = "test-naver-secret-not-real-01";
const NEWS_PATH: &str = "/v1/search/news.json";
/// 가짜 시계의 시각(= 받은 시각).
const NOW: &str = "2026-11-21T09:30:00+09:00";

fn ts(text: &str) -> DateTime<Utc> {
    text.parse::<DateTime<Utc>>().unwrap()
}

/// 테스트가 정하는 시계.
struct TestClock(Mutex<DateTime<Utc>>);

impl TestClock {
    fn at(text: &str) -> Arc<Self> {
        Arc::new(Self(Mutex::new(ts(text))))
    }
}

impl Clock for TestClock {
    fn now(&self) -> DateTime<Utc> {
        *self.0.lock().unwrap()
    }
}

/// 테스트가 정하는 뉴스 키. 처음에 없을 수 있고 나중에 넣을 수 있다. 받은 선언을 기록한다.
struct TestNewsKeys {
    answer: Mutex<Result<Option<Credentials>, NewsError>>,
    asked: Mutex<Vec<NewsKeys>>,
}

impl TestNewsKeys {
    fn with(answer: Result<Option<Credentials>, NewsError>) -> Arc<Self> {
        Arc::new(Self {
            answer: Mutex::new(answer),
            asked: Mutex::new(Vec::new()),
        })
    }

    fn both() -> Arc<Self> {
        Self::with(Ok(Some(both_creds())))
    }

    fn none() -> Arc<Self> {
        Self::with(Ok(None))
    }

    fn set(&self, answer: Result<Option<Credentials>, NewsError>) {
        *self.answer.lock().unwrap() = answer;
    }

    fn asked(&self) -> Vec<NewsKeys> {
        self.asked.lock().unwrap().clone()
    }
}

impl NewsKeySource for TestNewsKeys {
    fn read_news_keys(&self, keys: NewsKeys) -> Result<Option<Credentials>, NewsError> {
        self.asked.lock().unwrap().push(keys);
        self.answer.lock().unwrap().clone()
    }
}

fn both_creds() -> Credentials {
    Credentials::new()
        .with("client_id", CLIENT_ID)
        .with("client_secret", CLIENT_SECRET)
}

fn config(server: &MockServer) -> NaverNewsConfig {
    NaverNewsConfig::new(server.uri()).with_timeout(Duration::from_secs(5))
}

fn naver(server: &MockServer, keys: Arc<TestNewsKeys>) -> NaverNews {
    NaverNews::new(config(server), keys, TestClock::at(NOW)).unwrap()
}

async fn hits(server: &MockServer) -> usize {
    server.received_requests().await.unwrap_or_default().len()
}

fn has_secret(text: &str) -> bool {
    text.contains(CLIENT_ID) || text.contains(CLIENT_SECRET)
}

fn item(title: &str, description: &str, originallink: &str, link: &str, pub_date: &str) -> Value {
    json!({
        "title": title,
        "originallink": originallink,
        "link": link,
        "description": description,
        "pubDate": pub_date,
    })
}

/// 공식 문서 응답 예의 JSON 모양.
fn news_body(items: Vec<Value>) -> Value {
    json!({
        "lastBuildDate": "Sat, 21 Nov 2026 09:29:00 +0900",
        "total": items.len(),
        "start": 1,
        "display": items.len(),
        "items": items,
    })
}

fn error_body(code: &str) -> Value {
    json!({ "errorMessage": "test error", "errorCode": code })
}

async fn mount_news(server: &MockServer, template: ResponseTemplate) {
    Mock::given(method("GET"))
        .and(path(NEWS_PATH))
        .respond_with(template)
        .mount(server)
        .await;
}

async fn mount_ok(server: &MockServer, items: Vec<Value>) {
    mount_news(
        server,
        ResponseTemplate::new(200).set_body_json(news_body(items)),
    )
    .await;
}

/// 다른 곳(`/elsewhere`): 따라가면 받아들이는 응답을 준다.
async fn mount_elsewhere(server: &MockServer) -> String {
    Mock::given(method("GET"))
        .and(path("/elsewhere"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(news_body(vec![item(
                "따라간 곳",
                "요약",
                "https://news.example.com/x",
                "https://news.example.com/x",
                "Sat, 21 Nov 2026 08:00:00 +0900",
            )])),
        )
        .mount(server)
        .await;
    format!("{}/elsewhere", server.uri())
}

async fn elsewhere_hits(server: &MockServer) -> usize {
    server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .filter(|request| request.url.path() == "/elsewhere")
        .count()
}

/// 아무도 듣지 않는 루프백 주소.
fn dead_address() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    format!("http://127.0.0.1:{port}")
}

fn items_of(fetched: NewsFetched) -> Vec<NewsItem> {
    match fetched {
        NewsFetched::Items(items) => items,
        NewsFetched::Missing => Vec::new(),
    }
}

fn one_ok_item() -> Value {
    item(
        "제목",
        "요약",
        "https://news.example.com/a",
        "https://n.news.example.com/a",
        "Sat, 21 Nov 2026 08:00:00 +0900",
    )
}

// ───────────────────────── 1. 요청 모양 ─────────────────────────

// 검증: C11 V35 D9
#[tokio::test]
async fn 네이버_요청은_공식_경로와_쿼리와_머리글_두_개를_쓰고_키를_쿼리에_싣지_않는다() {
    let server = MockServer::start().await;
    mount_ok(&server, vec![one_ok_item()]).await;
    let keys = TestNewsKeys::both();
    let provider = naver(&server, keys.clone());

    provider
        .search(&NewsRequest::latest("삼성전자 주식", 7))
        .await
        .unwrap();

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    let request = &requests[0];
    assert_eq!(request.method.as_str(), "GET");
    assert_eq!(request.url.path(), NEWS_PATH);
    let raw = request.url.query().unwrap_or_default().to_owned();
    // 한글 검색어는 UTF-8 퍼센트 인코딩("삼성" = %EC%82%BC%EC%84%B1).
    assert!(raw.contains("query=%EC%82%BC%EC%84%B1"), "{raw}");
    let pairs: Vec<(String, String)> = request
        .url
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    let get = |name: &str| {
        pairs
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.clone())
    };
    assert_eq!(get("query").as_deref(), Some("삼성전자 주식"));
    assert_eq!(get("display").as_deref(), Some("7"));
    assert_eq!(get("start").as_deref(), Some("1"));
    assert_eq!(get("sort").as_deref(), Some("date"));
    assert!(!has_secret(&raw), "{raw}");
    assert_eq!(
        request
            .headers
            .get("x-naver-client-id")
            .and_then(|v| v.to_str().ok()),
        Some(CLIENT_ID)
    );
    assert_eq!(
        request
            .headers
            .get("x-naver-client-secret")
            .and_then(|v| v.to_str().ok()),
        Some(CLIENT_SECRET)
    );
    // 키는 네이버 선언으로 물었다.
    assert!(keys.asked().iter().all(|k| *k == NAVER_NEWS_KEYS));
    assert!(!keys.asked().is_empty());
}

// 검증: C11 V35
#[tokio::test]
async fn 정확도순은_sort_sim이다() {
    let server = MockServer::start().await;
    mount_ok(&server, vec![one_ok_item()]).await;
    let provider = naver(&server, TestNewsKeys::both());

    provider
        .search(&NewsRequest {
            query: "금리".to_owned(),
            max_items: 100,
            sort: NewsSort::Relevance,
        })
        .await
        .unwrap();

    let requests = server.received_requests().await.unwrap();
    let pairs: Vec<(String, String)> = requests[0]
        .url
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    assert!(pairs.contains(&("sort".to_owned(), "sim".to_owned())));
    assert!(pairs.contains(&("display".to_owned(), "100".to_owned())));
    assert!(pairs.contains(&("start".to_owned(), "1".to_owned())));
}

// 검증: C11
#[test]
fn 최신순_요청_만들기() {
    let request = NewsRequest::latest("환율", 10);
    assert_eq!(
        request,
        NewsRequest {
            query: "환율".to_owned(),
            max_items: 10,
            sort: NewsSort::Latest,
        }
    );
}

// ───────────────────────── 2. 키 ─────────────────────────

// 검증: D9 E14
#[tokio::test]
async fn 키가_없으면_요청을_보내지_않고_키_없음이다() {
    let server = MockServer::start().await;
    mount_ok(&server, vec![one_ok_item()]).await;
    let provider = naver(&server, TestNewsKeys::none());

    let err = provider
        .search(&NewsRequest::latest("주식", 10))
        .await
        .unwrap_err();

    assert_eq!(
        err,
        NewsError::NoKey {
            provider: NewsProviderId::Naver
        }
    );
    assert_eq!(hits(&server).await, 0);
}

// 검증: D9 E14
#[tokio::test]
async fn 칸이_하나만_있으면_요청을_보내지_않고_키_없음이다() {
    for partial in [
        Credentials::new().with("client_id", CLIENT_ID),
        Credentials::new().with("client_secret", CLIENT_SECRET),
        Credentials::new(),
    ] {
        let server = MockServer::start().await;
        mount_ok(&server, vec![one_ok_item()]).await;
        let provider = naver(&server, TestNewsKeys::with(Ok(Some(partial))));

        let err = provider
            .search(&NewsRequest::latest("주식", 10))
            .await
            .unwrap_err();

        assert_eq!(
            err,
            NewsError::NoKey {
                provider: NewsProviderId::Naver
            }
        );
        assert_eq!(hits(&server).await, 0);
    }
}

// 검증: D9 E14
#[tokio::test]
async fn 보관소가_깨졌으면_키_없음이_아니라_읽기_오류이고_요청을_보내지_않는다() {
    let server = MockServer::start().await;
    mount_ok(&server, vec![one_ok_item()]).await;
    let broken = NewsError::KeyUnreadable {
        provider: NewsProviderId::Naver,
    };
    let provider = naver(&server, TestNewsKeys::with(Err(broken.clone())));

    let err = provider
        .search(&NewsRequest::latest("주식", 10))
        .await
        .unwrap_err();

    assert_eq!(err, broken);
    assert_eq!(hits(&server).await, 0);
}

// 검증: D9
#[tokio::test]
async fn 나중에_넣은_키도_바로_쓴다() {
    let server = MockServer::start().await;
    mount_ok(&server, vec![one_ok_item()]).await;
    let keys = TestNewsKeys::none();
    let provider = naver(&server, keys.clone());
    let request = NewsRequest::latest("주식", 10);

    assert!(matches!(
        provider.search(&request).await,
        Err(NewsError::NoKey { .. })
    ));
    keys.set(Ok(Some(both_creds())));

    let fetched = provider.search(&request).await.unwrap();

    assert_eq!(items_of(fetched).len(), 1);
    assert_eq!(hits(&server).await, 1);
}

// ───────────────────────── 3. 응답 읽기 ─────────────────────────

// 검증: C11 V35 V36 P10
#[tokio::test]
async fn 응답을_읽어_태그를_지우고_엔티티를_풀고_링크와_시각을_정한다() {
    let server = MockServer::start().await;
    mount_ok(
        &server,
        vec![
            item(
                "<b>삼성전자</b> &quot;반도체&quot; &amp; 전망",
                "글로 쓴 &lt;b&gt; 꺾쇠와 &#39;따옴표&#39;   그리고\n 공백",
                "https://news.example.com/original",
                "https://n.news.example.com/naver",
                "Sat, 21 Nov 2026 08:50:00 +0900",
            ),
            item(
                "두 번째",
                "요약 둘",
                "",
                "https://n.news.example.com/only-naver",
                "Fri, 20 Nov 2026 23:00:00 +0000",
            ),
        ],
    )
    .await;
    let provider = naver(&server, TestNewsKeys::both());

    let fetched = provider
        .search(&NewsRequest::latest("삼성전자", 10))
        .await
        .unwrap();

    let items = items_of(fetched);
    assert_eq!(
        items,
        vec![
            NewsItem {
                provider: NewsProviderId::Naver,
                query: "삼성전자".to_owned(),
                title: "삼성전자 \"반도체\" & 전망".to_owned(),
                summary: "글로 쓴 <b> 꺾쇠와 '따옴표' 그리고 공백".to_owned(),
                link: "https://news.example.com/original".to_owned(),
                published_at: ts("2026-11-20T23:50:00Z"),
                known_at: ts(NOW),
            },
            NewsItem {
                provider: NewsProviderId::Naver,
                query: "삼성전자".to_owned(),
                title: "두 번째".to_owned(),
                summary: "요약 둘".to_owned(),
                link: "https://n.news.example.com/only-naver".to_owned(),
                published_at: ts("2026-11-20T23:00:00Z"),
                known_at: ts(NOW),
            },
        ]
    );
}

// 검증: E32
#[tokio::test]
async fn 빈_결과는_오류가_아니라_없음이다() {
    let server = MockServer::start().await;
    mount_ok(&server, vec![]).await;
    let provider = naver(&server, TestNewsKeys::both());

    let fetched = provider
        .search(&NewsRequest::latest("없는검색어", 10))
        .await
        .unwrap();

    assert_eq!(fetched, NewsFetched::Missing);
}

// 검증: V36 P10
#[tokio::test]
async fn 받은_시각보다_뒤의_기사는_버린다() {
    let server = MockServer::start().await;
    mount_ok(
        &server,
        vec![
            item(
                "미래",
                "요약",
                "https://news.example.com/future",
                "",
                "Sat, 21 Nov 2026 09:31:00 +0900",
            ),
            item(
                "딱 지금",
                "요약",
                "https://news.example.com/now",
                "",
                "Sat, 21 Nov 2026 09:30:00 +0900",
            ),
        ],
    )
    .await;
    let provider = naver(&server, TestNewsKeys::both());

    let items = items_of(
        provider
            .search(&NewsRequest::latest("주식", 10))
            .await
            .unwrap(),
    );

    assert_eq!(items.len(), 1);
    assert_eq!(items[0].title, "딱 지금");
    let decision_at = ts(NOW);
    assert!(items.iter().all(|i| i.published_at <= decision_at));
    assert!(items.iter().all(|i| i.known_by(decision_at)));
}

// 검증: V36 E32
#[tokio::test]
async fn 모든_기사가_미래면_없음이다() {
    let server = MockServer::start().await;
    mount_ok(
        &server,
        vec![item(
            "미래",
            "요약",
            "https://news.example.com/future",
            "",
            "Sun, 22 Nov 2026 09:00:00 +0900",
        )],
    )
    .await;
    let provider = naver(&server, TestNewsKeys::both());

    let fetched = provider
        .search(&NewsRequest::latest("주식", 10))
        .await
        .unwrap();

    assert_eq!(fetched, NewsFetched::Missing);
}

fn is_format(result: &Result<NewsFetched, NewsError>) -> bool {
    matches!(
        result,
        Err(NewsError::Format {
            provider: NewsProviderId::Naver,
            ..
        })
    )
}

// 검증: E14
#[tokio::test]
async fn 시각이나_칸이_깨진_응답은_전체가_형식_오류다() {
    let good = one_ok_item();
    let mut no_title = good.clone();
    no_title.as_object_mut().unwrap().remove("title");
    let mut no_date = good.clone();
    no_date.as_object_mut().unwrap().remove("pubDate");
    let mut no_links = good.clone();
    no_links.as_object_mut().unwrap().remove("originallink");
    no_links.as_object_mut().unwrap().remove("link");
    let bad_date = item(
        "제목",
        "요약",
        "https://news.example.com/a",
        "",
        "2026-11-21 08:00",
    );
    let mut number_title = good.clone();
    number_title["title"] = json!(42);

    let bodies = vec![
        news_body(vec![good.clone(), no_title]),
        news_body(vec![good.clone(), no_date]),
        news_body(vec![good.clone(), no_links]),
        news_body(vec![good.clone(), bad_date]),
        news_body(vec![good.clone(), number_title]),
        json!({ "total": 1, "start": 1, "display": 1 }),
        json!({ "items": "not-an-array" }),
    ];
    for body in bodies {
        let server = MockServer::start().await;
        mount_news(&server, ResponseTemplate::new(200).set_body_json(&body)).await;
        let provider = naver(&server, TestNewsKeys::both());

        let result = provider.search(&NewsRequest::latest("주식", 10)).await;

        assert!(is_format(&result), "{body} → {result:?}");
    }

    let server = MockServer::start().await;
    mount_news(
        &server,
        ResponseTemplate::new(200).set_body_raw("not json", "application/json"),
    )
    .await;
    let provider = naver(&server, TestNewsKeys::both());
    assert!(is_format(
        &provider.search(&NewsRequest::latest("주식", 10)).await
    ));
}

// 검증: E14
#[tokio::test]
async fn http_https가_아닌_링크는_형식_오류다() {
    let cases = [
        ("javascript:alert(1)", ""),
        ("ftp://news.example.com/a", ""),
        ("", "data:text/html,x"),
        ("", ""),
        ("/relative/path", ""),
    ];
    for (originallink, link) in cases {
        let server = MockServer::start().await;
        mount_ok(
            &server,
            vec![item(
                "제목",
                "요약",
                originallink,
                link,
                "Sat, 21 Nov 2026 08:00:00 +0900",
            )],
        )
        .await;
        let provider = naver(&server, TestNewsKeys::both());

        let result = provider.search(&NewsRequest::latest("주식", 10)).await;

        assert!(is_format(&result), "{originallink:?} {link:?} → {result:?}");
    }
}

// ───────────────────────── clean_news_text ─────────────────────────

// 검증: V35
#[test]
fn 태그를_먼저_지우고_엔티티를_푼다() {
    assert_eq!(clean_news_text("<b>삼성</b>전자"), "삼성전자");
    assert_eq!(clean_news_text("&lt;b&gt;글&lt;/b&gt;"), "<b>글</b>");
    assert_eq!(
        clean_news_text("&quot;a&quot; &amp; &#39;b&#39; &apos;c&apos;"),
        "\"a\" & 'b' 'c'"
    );
    assert_eq!(clean_news_text("&#65;&#x42;&#x63;"), "ABc");
    assert_eq!(clean_news_text("a&nbsp;&nbsp;b"), "a b");
    assert_eq!(clean_news_text("<i class=\"x\">기울임</i> 글"), "기울임 글");
}

// 검증: V35
#[test]
fn 엔티티는_한_번만_풀고_모르는_엔티티는_그대로_둔다() {
    assert_eq!(clean_news_text("&amp;lt;"), "&lt;");
    assert_eq!(clean_news_text("&unknown; 그대로"), "&unknown; 그대로");
    assert_eq!(clean_news_text("A & B"), "A & B");
}

// 검증: V35
#[test]
fn 공백은_하나로_모으고_앞뒤를_자른다() {
    assert_eq!(clean_news_text("  a \n\t b  "), "a b");
    assert_eq!(clean_news_text(""), "");
    assert_eq!(clean_news_text("<b></b>"), "");
}

// ───────────────────────── 5 · 6. 오류 갈래와 비밀 ─────────────────────────

fn kind(err: &NewsError) -> &'static str {
    match err {
        NewsError::NoProvider { .. } => "no_provider",
        NewsError::NoKey { .. } => "no_key",
        NewsError::KeyUnreadable { .. } => "key_unreadable",
        NewsError::KeyRejected { .. } => "key_rejected",
        NewsError::RateLimited { .. } => "rate_limited",
        NewsError::Timeout { .. } => "timeout",
        NewsError::Unreachable { .. } => "unreachable",
        NewsError::BadAddress { .. } => "bad_address",
        NewsError::Rejected { .. } => "rejected",
        NewsError::Format { .. } => "format",
        NewsError::InvalidRequest { .. } => "invalid_request",
        NewsError::InsecureBaseUrl { .. } => "insecure",
        _ => "other",
    }
}

// 검증: E14
#[tokio::test]
async fn 오류_갈래표대로_가르고_다시_시도_대상을_표시한다() {
    // (상태, 오류 코드, 기대 갈래, 다시 시도)
    let table = [
        (400_u16, "SE01", "rejected", false),
        (400, "SE02", "rejected", false),
        (400, "SE06", "rejected", false),
        (401, "024", "key_rejected", false),
        (403, "403", "key_rejected", false),
        (404, "SE05", "bad_address", false),
        (429, "012", "rate_limited", true),
        (500, "SE99", "unreachable", true),
        (502, "502", "unreachable", true),
        (503, "503", "unreachable", true),
    ];
    for (status, code, want, retry) in table {
        let server = MockServer::start().await;
        // 본문에 키를 되돌려 실어도 오류에는 나오지 않아야 한다.
        let mut body = error_body(code);
        body["echo"] = json!(format!("{CLIENT_ID} {CLIENT_SECRET}"));
        mount_news(&server, ResponseTemplate::new(status).set_body_json(body)).await;
        let provider = naver(&server, TestNewsKeys::both());

        let err = provider
            .search(&NewsRequest::latest("주식", 10))
            .await
            .unwrap_err();

        assert_eq!(kind(&err), want, "{status} {code} → {err:?}");
        assert_eq!(err.is_retryable(), retry, "{status} → {err:?}");
        assert!(!has_secret(&format!("{err:?} {err}")), "{status}");
        assert_eq!(hits(&server).await, 1, "{status}: 다시 시도하지 않는다");
    }
}

// 검증: E14
#[tokio::test]
async fn 거절_오류는_공급자_오류_코드를_싣는다() {
    let server = MockServer::start().await;
    mount_news(
        &server,
        ResponseTemplate::new(400).set_body_json(error_body("SE01")),
    )
    .await;
    let provider = naver(&server, TestNewsKeys::both());

    let err = provider
        .search(&NewsRequest::latest("주식", 10))
        .await
        .unwrap_err();

    assert_eq!(
        err,
        NewsError::Rejected {
            provider: NewsProviderId::Naver,
            code: "SE01".to_owned(),
        }
    );
}

// 검증: E14
#[tokio::test]
async fn 리다이렉트는_따라가지_않고_주소_오류이며_키를_다른_곳으로_보내지_않는다() {
    for status in [301_u16, 302, 307, 308] {
        let server = MockServer::start().await;
        let elsewhere = mount_elsewhere(&server).await;
        mount_news(
            &server,
            ResponseTemplate::new(status).insert_header("Location", elsewhere.as_str()),
        )
        .await;
        let provider = naver(&server, TestNewsKeys::both());

        let err = provider
            .search(&NewsRequest::latest("주식", 10))
            .await
            .unwrap_err();

        assert_eq!(kind(&err), "bad_address", "{status} → {err:?}");
        assert!(!err.is_retryable());
        assert_eq!(elsewhere_hits(&server).await, 0, "{status}");
        assert_eq!(hits(&server).await, 1, "{status}");
    }
}

// 검증: E14
#[tokio::test]
async fn 연결이_안_되면_닿지_않음이고_다시_시도_대상이다() {
    let provider = NaverNews::new(
        NaverNewsConfig::new(dead_address()).with_timeout(Duration::from_secs(5)),
        TestNewsKeys::both(),
        TestClock::at(NOW),
    )
    .unwrap();

    let err = provider
        .search(&NewsRequest::latest("주식", 10))
        .await
        .unwrap_err();

    assert_eq!(kind(&err), "unreachable", "{err:?}");
    assert!(err.is_retryable());
    assert!(!has_secret(&format!("{err:?} {err}")));
}

// 검증: E14
#[tokio::test]
async fn 정해진_시간_안에_답이_없으면_시간_초과다() {
    let server = MockServer::start().await;
    mount_news(
        &server,
        ResponseTemplate::new(200)
            .set_body_json(news_body(vec![one_ok_item()]))
            .set_delay(Duration::from_secs(3)),
    )
    .await;
    let provider = NaverNews::new(
        config(&server).with_timeout(Duration::from_millis(200)),
        TestNewsKeys::both(),
        TestClock::at(NOW),
    )
    .unwrap();

    let err = provider
        .search(&NewsRequest::latest("주식", 10))
        .await
        .unwrap_err();

    assert_eq!(
        err,
        NewsError::Timeout {
            provider: NewsProviderId::Naver
        }
    );
    assert!(err.is_retryable());
}

// 검증: E14
#[test]
fn 다시_시도_대상은_시간_초과_한도_닿지_않음뿐이다() {
    let p = NewsProviderId::Naver;
    let retry = [
        NewsError::Timeout { provider: p },
        NewsError::RateLimited { provider: p },
        NewsError::Unreachable {
            provider: p,
            detail: "연결 실패".to_owned(),
        },
    ];
    for err in retry {
        assert!(err.is_retryable(), "{err:?}");
    }
    let no_retry = [
        NewsError::NoProvider { provider: p },
        NewsError::NoKey { provider: p },
        NewsError::KeyUnreadable { provider: p },
        NewsError::KeyRejected {
            provider: p,
            code: "401".to_owned(),
        },
        NewsError::BadAddress { provider: p },
        NewsError::Rejected {
            provider: p,
            code: "SE01".to_owned(),
        },
        NewsError::Format {
            provider: p,
            detail: "x".to_owned(),
        },
        NewsError::InvalidRequest {
            provider: p,
            detail: "x".to_owned(),
        },
        NewsError::InsecureBaseUrl { provider: p },
    ];
    for err in no_retry {
        assert!(!err.is_retryable(), "{err:?}");
    }
}

// 검증: E14 D9
#[tokio::test]
async fn 키_오류와_키_없음의_글과_디버그에_아이디와_시크릿이_없다() {
    let server = MockServer::start().await;
    mount_news(
        &server,
        ResponseTemplate::new(401).set_body_json(error_body("024")),
    )
    .await;
    let provider = naver(&server, TestNewsKeys::both());
    let err = provider
        .search(&NewsRequest::latest(CLIENT_ID, 10))
        .await
        .unwrap_err();
    assert!(matches!(err, NewsError::KeyRejected { .. }), "{err:?}");
    assert!(!has_secret(&format!("{err:?} {err}")));

    let partial = TestNewsKeys::with(Ok(Some(Credentials::new().with("client_id", CLIENT_ID))));
    let err = naver(&server, partial)
        .search(&NewsRequest::latest("주식", 10))
        .await
        .unwrap_err();
    assert!(!has_secret(&format!("{err:?} {err}")));
}

// ───────────────────────── 7. 주소 · 본문 상한 ─────────────────────────

// 검증: E14
#[test]
fn https가_아닌_기본_주소는_거부하고_루프백은_받는다() {
    for bad in [
        "http://news.example.com",
        "ftp://news.example.com",
        "news.example.com",
        "",
    ] {
        let result = NaverNews::new(
            NaverNewsConfig::new(bad),
            TestNewsKeys::both(),
            TestClock::at(NOW),
        );
        assert!(
            matches!(
                result,
                Err(NewsError::InsecureBaseUrl {
                    provider: NewsProviderId::Naver
                })
            ),
            "{bad}"
        );
    }
    for good in [
        "https://news.example.com",
        "http://127.0.0.1:9",
        "http://localhost:9",
        "http://[::1]:9",
    ] {
        assert!(
            NaverNews::new(
                NaverNewsConfig::new(good),
                TestNewsKeys::both(),
                TestClock::at(NOW),
            )
            .is_ok(),
            "{good}"
        );
    }
}

// 검증: E14
#[tokio::test]
async fn 본문이_상한보다_크면_형식_오류이고_본문과_키를_싣지_않는다() {
    let server = MockServer::start().await;
    let mut big = vec![b' '; 8 * 1024];
    big.extend_from_slice(CLIENT_SECRET.as_bytes());
    mount_news(
        &server,
        ResponseTemplate::new(200).set_body_raw(big, "application/json"),
    )
    .await;
    let provider = NaverNews::new(
        config(&server).with_max_body_bytes(1024),
        TestNewsKeys::both(),
        TestClock::at(NOW),
    )
    .unwrap();

    let err = provider
        .search(&NewsRequest::latest("주식", 10))
        .await
        .unwrap_err();

    assert_eq!(kind(&err), "format", "{err:?}");
    let text = format!("{err:?} {err}");
    assert!(!has_secret(&text));
    assert!(text.len() < 1024);
}

// 검증: C11
#[test]
fn 공식_설정은_https_기본_주소와_기존과_같은_기본값을_쓴다() {
    let official = NaverNewsConfig::official();
    assert!(official.base_url.starts_with("https://"));
    assert_eq!(official.base_url, pi_data::DEFAULT_NAVER_NEWS_BASE_URL);
    assert_eq!(official.timeout, DEFAULT_TIMEOUT);
    assert_eq!(official.max_body_bytes, DEFAULT_NEWS_MAX_BODY_BYTES);
    assert_eq!(DEFAULT_NEWS_MAX_BODY_BYTES, 16 * 1024 * 1024);

    let local = NaverNewsConfig::new("http://127.0.0.1:9");
    assert_eq!(local.base_url, "http://127.0.0.1:9");
    assert_eq!(local.timeout, DEFAULT_TIMEOUT);
    assert_eq!(local.max_body_bytes, DEFAULT_NEWS_MAX_BODY_BYTES);
    let changed = local
        .with_timeout(Duration::from_secs(2))
        .with_max_body_bytes(10);
    assert_eq!(changed.timeout, Duration::from_secs(2));
    assert_eq!(changed.max_body_bytes, 10);

    let provider =
        NaverNews::new(official.clone(), TestNewsKeys::none(), TestClock::at(NOW)).unwrap();
    assert_eq!(provider.config(), &official);
}

// ───────────────────────── 8. 요청 확인 ─────────────────────────

// 검증: E14 C11
#[tokio::test]
async fn 올바르지_않은_요청은_보내지_않고_요청_오류다() {
    let long = "가".repeat(101);
    let cases: Vec<NewsRequest> = vec![
        NewsRequest::latest("", 10),
        NewsRequest::latest("   ", 10),
        NewsRequest::latest(long.as_str(), 10),
        NewsRequest::latest("삼성\n전자", 10),
        NewsRequest::latest("삼성\u{7}전자", 10),
        NewsRequest::latest("주식", 0),
        NewsRequest::latest("주식", 101),
    ];
    for request in cases {
        let server = MockServer::start().await;
        mount_ok(&server, vec![one_ok_item()]).await;
        let provider = naver(&server, TestNewsKeys::both());

        let err = provider.search(&request).await.unwrap_err();

        assert!(
            matches!(
                err,
                NewsError::InvalidRequest {
                    provider: NewsProviderId::Naver,
                    ..
                }
            ),
            "{request:?} → {err:?}"
        );
        assert_eq!(hits(&server).await, 0, "{request:?}");
    }
}

// 검증: C11
#[tokio::test]
async fn 경계의_요청은_받는다() {
    let hundred = "가".repeat(100);
    let padded = format!("  {hundred}  ");
    let cases = [
        NewsRequest::latest(hundred.as_str(), 1),
        NewsRequest::latest(padded.as_str(), 100),
    ];
    for request in cases {
        let server = MockServer::start().await;
        mount_ok(&server, vec![]).await;
        let provider = naver(&server, TestNewsKeys::both());

        let fetched = provider.search(&request).await.unwrap();

        assert_eq!(fetched, NewsFetched::Missing);
        assert_eq!(hits(&server).await, 1);
    }
}

// ───────────────────────── 9. 등록부 · 가짜 ─────────────────────────

fn sample_item(title: &str) -> NewsItem {
    NewsItem {
        provider: NewsProviderId::Naver,
        query: "주식".to_owned(),
        title: title.to_owned(),
        summary: "요약".to_owned(),
        link: "https://news.example.com/a".to_owned(),
        published_at: ts("2026-11-21T08:00:00+09:00"),
        known_at: ts(NOW),
    }
}

// 검증: V35 E14
#[tokio::test]
async fn 공급자가_없으면_요청_없이_공급자_없음이다() {
    let registry = NewsRegistry::new();
    assert!(registry.providers().is_empty());

    let err = registry
        .search(&NewsRequest::latest("주식", 10))
        .await
        .unwrap_err();

    assert!(matches!(err, NewsError::NoProvider { .. }), "{err:?}");
    assert!(!err.is_retryable());
}

// 검증: V35 E14
#[tokio::test]
async fn 기본이_등록되지_않은_공급자면_공급자_없음이다() {
    let mut registry = NewsRegistry::new();
    registry.set_default(NewsProviderId::Naver);

    let err = registry
        .search(&NewsRequest::latest("주식", 10))
        .await
        .unwrap_err();

    assert_eq!(
        err,
        NewsError::NoProvider {
            provider: NewsProviderId::Naver
        }
    );
}

// 검증: V35
#[tokio::test]
async fn 등록부는_기본이나_처음_등록된_공급자에게_맡긴다() {
    let fake = Arc::new(FakeNewsProvider::new(NewsProviderId::Naver));
    fake.set(Ok(NewsFetched::Items(vec![sample_item("가짜 기사")])));
    let mut registry = NewsRegistry::new();
    registry.register(fake.clone());
    let request = NewsRequest::latest("주식", 5);

    // 기본을 정하지 않으면 처음 등록된 공급자.
    let first = registry.search(&request).await.unwrap();
    assert_eq!(first, NewsFetched::Items(vec![sample_item("가짜 기사")]));

    // 기본을 정하면 그 공급자.
    registry.set_default(NewsProviderId::Naver);
    let second = registry.search(&request).await.unwrap();
    assert_eq!(second, first);

    assert_eq!(fake.calls(), vec![request.clone(), request]);
    assert_eq!(registry.providers(), vec![NewsProviderId::Naver]);
}

// 검증: V35
#[tokio::test]
async fn 같은_이름으로_다시_등록하면_바꾼다() {
    let old = Arc::new(FakeNewsProvider::new(NewsProviderId::Naver));
    old.set(Ok(NewsFetched::Items(vec![sample_item("옛것")])));
    let new = Arc::new(FakeNewsProvider::new(NewsProviderId::Naver));
    new.set(Ok(NewsFetched::Items(vec![sample_item("새것")])));
    let mut registry = NewsRegistry::new();
    registry.register(old.clone());
    registry.register(new.clone());

    let fetched = registry
        .search(&NewsRequest::latest("주식", 10))
        .await
        .unwrap();

    assert_eq!(fetched, NewsFetched::Items(vec![sample_item("새것")]));
    assert_eq!(registry.providers(), vec![NewsProviderId::Naver]);
    assert!(old.calls().is_empty());
    assert_eq!(new.calls().len(), 1);
}

// 검증: V35 E14 E32
#[tokio::test]
async fn 가짜_공급자는_정해_준_결과와_오류를_돌려주고_요청을_기록한다() {
    let fake = FakeNewsProvider::new(NewsProviderId::Naver);
    assert_eq!(fake.id(), NewsProviderId::Naver);
    assert_eq!(fake.keys(), NAVER_NEWS_KEYS);
    let request = NewsRequest::latest("주식", 3);

    // 정하지 않았으면 없음.
    assert_eq!(fake.search(&request).await.unwrap(), NewsFetched::Missing);

    let limited = NewsError::RateLimited {
        provider: NewsProviderId::Naver,
    };
    fake.set(Err(limited.clone()));
    assert_eq!(fake.search(&request).await.unwrap_err(), limited);

    assert_eq!(fake.calls(), vec![request.clone(), request]);
}

// 검증: V35 E14
#[tokio::test]
async fn 등록부는_공급자의_오류를_그대로_전한다() {
    let fake = Arc::new(FakeNewsProvider::new(NewsProviderId::Naver));
    let timeout = NewsError::Timeout {
        provider: NewsProviderId::Naver,
    };
    fake.set(Err(timeout.clone()));
    let mut registry = NewsRegistry::new();
    registry.register(fake);

    let err = registry
        .search(&NewsRequest::latest("주식", 10))
        .await
        .unwrap_err();

    assert_eq!(err, timeout);
}

// 검증: V35
#[tokio::test]
async fn 네이버_공급자를_등록부에_넣어_검색한다() {
    let server = MockServer::start().await;
    mount_ok(&server, vec![one_ok_item()]).await;
    let provider = naver(&server, TestNewsKeys::both());
    assert_eq!(provider.id(), NewsProviderId::Naver);
    assert_eq!(provider.keys(), NAVER_NEWS_KEYS);
    let mut registry = NewsRegistry::new();
    registry.register(Arc::new(provider));

    let items = items_of(
        registry
            .search(&NewsRequest::latest("주식", 10))
            .await
            .unwrap(),
    );

    assert_eq!(items.len(), 1);
    assert_eq!(items[0].known_at, ts(NOW));
}

// 검증: V35
#[test]
fn 공급자_이름은_기계용_글자로_오간다() {
    assert_eq!(NewsProviderId::Naver.as_str(), "naver");
    assert_eq!(NewsProviderId::Naver.to_string(), "naver");
    assert_eq!(NewsProviderId::parse("naver"), Some(NewsProviderId::Naver));
    assert_eq!(NewsProviderId::parse("google"), None);
    assert_eq!(NewsProviderId::parse(""), None);
}

// ───────────────────────── 10. 키 선언 · 확인기 ─────────────────────────

// 검증: D9
#[test]
fn 네이버_뉴스_키_선언은_공급자_이름과_칸_두_개다() {
    assert_eq!(NAVER_NEWS_KEYS.provider(), "naver_news");
    assert_eq!(NAVER_NEWS_KEYS.fields(), &["client_id", "client_secret"]);
    let other = NewsKeys::new("other_news", &["token"]);
    assert_eq!(other.provider(), "other_news");
    assert_eq!(other.fields(), &["token"]);
    assert_ne!(other, NAVER_NEWS_KEYS);
}

// 검증: D9 E14
#[tokio::test]
async fn 확인기는_한_번_검색해_200이면_받아들인다() {
    let server = MockServer::start().await;
    mount_ok(&server, vec![]).await;

    let result = verify_news_keys(&config(&server), &both_creds()).await;

    assert_eq!(result, Ok(()));
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].url.path(), NEWS_PATH);
    let display: Vec<String> = requests[0]
        .url
        .query_pairs()
        .filter(|(k, _)| k == "display")
        .map(|(_, v)| v.into_owned())
        .collect();
    assert_eq!(display, vec!["1".to_owned()]);
    assert!(!has_secret(requests[0].url.as_str()));
    assert_eq!(
        requests[0]
            .headers
            .get("x-naver-client-secret")
            .and_then(|v| v.to_str().ok()),
        Some(CLIENT_SECRET)
    );
}

// 검증: D9 E14
#[tokio::test]
async fn 확인기의_갈래는_401_403은_거부_그_밖은_닿지_않음이다() {
    let table = [
        (401_u16, VerifyFailure::Rejected),
        (403, VerifyFailure::Rejected),
        (404, VerifyFailure::Unreachable),
        (500, VerifyFailure::Unreachable),
        (503, VerifyFailure::Unreachable),
        (429, VerifyFailure::Unreachable),
    ];
    for (status, want) in table {
        let server = MockServer::start().await;
        mount_news(
            &server,
            ResponseTemplate::new(status).set_body_json(error_body("x")),
        )
        .await;

        let result = verify_news_keys(&config(&server), &both_creds()).await;

        assert_eq!(result, Err(want), "{status}");
        assert_eq!(hits(&server).await, 1, "{status}");
    }

    let server = MockServer::start().await;
    let elsewhere = mount_elsewhere(&server).await;
    mount_news(
        &server,
        ResponseTemplate::new(302).insert_header("Location", elsewhere.as_str()),
    )
    .await;
    assert_eq!(
        verify_news_keys(&config(&server), &both_creds()).await,
        Err(VerifyFailure::Unreachable)
    );
    assert_eq!(elsewhere_hits(&server).await, 0);

    assert_eq!(
        verify_news_keys(&NaverNewsConfig::new(dead_address()), &both_creds()).await,
        Err(VerifyFailure::Unreachable)
    );
}

// 검증: D9
#[tokio::test]
async fn 확인기는_칸이_빠진_값이면_요청하지_않고_닿지_않음이다() {
    let server = MockServer::start().await;
    mount_ok(&server, vec![]).await;

    let result = verify_news_keys(
        &config(&server),
        &Credentials::new().with("client_id", CLIENT_ID),
    )
    .await;

    assert_eq!(result, Err(VerifyFailure::Unreachable));
    assert_eq!(hits(&server).await, 0);
}

// ───────────────────────── 11. known_by ─────────────────────────

// 검증: V36 P10
#[test]
fn 기사는_알게_된_시각이_판단_시각_이하일_때만_쓸_수_있다() {
    let news = sample_item("기사");
    assert!(news.known_by(ts(NOW)));
    assert!(news.known_by(ts("2026-11-21T09:30:01+09:00")));
    assert!(!news.known_by(ts("2026-11-21T09:29:59+09:00")));
}

// ───────────────────────── 12. 옛 API 그대로 ─────────────────────────

// 검증: C11 V33 V34
#[tokio::test]
async fn 옛_자료_종류와_출처와_어댑터는_그대로다() {
    assert_eq!(FactKind::ALL.len(), 5);
    assert_eq!(SourceName::ALL.len(), 3);
    let adapter: Arc<dyn DataAdapter> = Arc::new(FakeDataAdapter::new());
    let _ = adapter;
    let _: fn(&FactRequest) -> FactKind = FactRequest::kind;
    let _: Option<Fetched> = None;
    // 뉴스는 옛 종류에 들지 않는다: 옛 종류는 모두 옛 세 출처 중 하나에서 온다.
    for kind in FactKind::ALL {
        assert!(SourceName::ALL.contains(&kind.source()));
    }
    // 뉴스 공급자 이름은 옛 출처 이름과 겹치지 않는다.
    for source in SourceName::ALL {
        assert_ne!(source.as_str(), NewsProviderId::Naver.as_str());
    }
}
