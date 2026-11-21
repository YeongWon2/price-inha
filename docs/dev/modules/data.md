# data 모듈 (T10)

크레이트 `pi-data`, 폴더 `crates/data/`. 구성 요소 A10. `pi-secrets`를 쓴다(자료 키, secrets 6차의 출처별 칸). 모양은 `pi-market`과 같게 한다.
근거: `dev.md` D9 · C11 · E14 · E32 · V33 · V34 · V36, `principles.md` P10.

## 원본 동작 근거

- 원본에 없음(우리 고유): Hermes · OpenClaw · DeskRPG에는 재무 · 거시 · 공시 자료 어댑터가 없다.
- 기준: 각 공식 API의 공개 명세(DART 오픈API · 한국은행 ECOS · FRED), `docs/research/stock-investing-principles.md` 3장(판단 시점까지 알 수 있던 자료만).
- 어댑터의 모양(trait · 가짜 어댑터 · 주소 주입 · 키를 오류에 싣지 않기)은 `docs/dev/modules/market.md`를 따른다.

## 만드는 것 (1차)

1. `DataAdapter::fetch(종목|지표, 종류) -> Fact` (C11)
   - `Fact`의 칸: 값, 출처 이름 · 주소, 공표 시각, 알게 된 시각(받은 시각, Clock).
   - 종류: 공시(DART), 재무(DART), 금리 · 환율(ECOS), 미국 금리(FRED).
2. 기본 주소는 생성자 인자로 받는다. reqwest는 rustls를 쓴다.
3. 오류를 종류로 구분한다(E14): 닿지 않음, 시간 초과, 키 없음, 한도, 형식. 키가 쿼리에 실리는 API는 오류의 주소에서 키를 가린다.
4. 비어 있으면 오류가 아니라 `Missing(종류)`로 돌려준다(E32: 판단을 지켜보기로 두는 근거).
5. `FakeDataAdapter`(정해진 Fact와 오류를 돌려준다).

## 확인기

`pi-secrets`의 `Verifier::verify_slot` 기본은 언제나 거부다. 자료 키를 화면에서 등록하려면 이 모듈이 출처별로 닿는지 확인하는 함수를 공개하고, 조립(server)이 그것으로 `verify_slot`을 구현한다.

## 2차 작업 (1차 검토의 참고 — server 조립 전에 한다)

1. 받을 양의 상한(ECOS 10000행, DART 20쪽)을 넘으면 잘린 결과를 완전한 것처럼 돌려주지 않고 잘렸다는 표시(또는 오류)를 낸다.
2. `SecretStore` 읽기 오류(입출력 · 손상)를 키 없음으로 묻지 않고 따로 낸다.
3. 확인기에서 404 · 3xx 같은 비정상 상태는 키 거절이 아니라 닿지 않음 · 주소 오류로 낸다.
4. FRED `published_on`은 real-time 기간 때문에 실제 공표일이 아니다: 칸을 비우거나 주석을 고친다.
5. 리다이렉트를 따라가지 않음과 본문 상한을 테스트로 확인한다.

## 3차 후보 (2차 검토의 참고)

1. ECOS `list_total_count`가 없거나 숫자가 아니면 받은 행 수가 상한과 같을 때 잘림으로 본다.
2. 404(주소 오류)처럼 다시 해도 안 될 실패를 다시 시도 대상과 가를 수 있게 오류에 표시를 둔다(조립 · trigger가 쓴다).

## 3차 작업 (후보 일괄 처리)

2차 검토의 "3차 후보" 1~2번을 처리했다. 공개 API는 더하기만 했다.

1. ECOS `list_total_count`가 없거나 숫자가 아니면(음수 포함), 받은 행 수가 상한(10000)과 같을 때 잘림(`Truncated`)으로 본다. 상한보다 적으면 모두 돌려준다.
2. `DataError::is_retryable()`을 더했다(조립 · trigger가 다시 시도 대상을 가른다). 시간 초과 · 한도(429 · DART `020` · ECOS `ERROR-602`) · 닿지 않음(연결 실패 · 5xx)은 `true`. 404 · 3xx(주소 오류), 키 없음 · 키 거절 · 거절 · 형식 · 잘림 · 요청 오류 · 키 읽기 오류는 `false`.

## 4차 작업 (2026-11-21 발표 자료 대조 — 공급자 확장)

근거: `docs/dev/decisions.md` 2026-11-21 2번(공급자 확장 구조) · 3번(뉴스: 네이버 뉴스 검색 API), `dev.md` D9 · V35 · V36 · E14 · E32, `principles.md` P10. 이 차수가 결정 2026-11-16 17번의 "뉴스는 2차"를 한다.
아래를 **테스트로 먼저 쓰고** 구현한다. **공개 API는 더하기만 한다.** DART · ECOS · FRED 어댑터는 그대로 둔다.

### 원본 동작 근거

- 뉴스 공급자 자체: 원본에 없음 — 사용자 결정 2026-11-21 3번. 요청 · 응답 · 오류의 모양은 네이버 개발자센터 공식 문서 "검색 > 뉴스"(`https://developers.naver.com/docs/serviceapi/search/news/news.md`, 2026-11-21 확인)를 따른다.
- 공급자 등록부와 "공급자가 자기 키 칸을 선언": Hermes `ProviderProfile`(이름 · `env_vars` 선언, 같은 이름은 last-writer-wins) — `docs/research/hermes-agent.md` 4.1.
- 판단 시점까지 알 수 있던 자료만 쓴다: `docs/research/stock-investing-principles.md` 3장(위 원본 동작 근거와 같다).

### 잠긴 옛 테스트와 부딪히는 곳 (설계가 이것을 피한다)

- **`FactKind` · `FactRequest` · `SourceName`에 뉴스를 더하지 않는다.** 까닭: (1) 잠긴 `tests/suite/client.rs` "종류마다_출처가_정해져_있다"가 `FactKind::ALL`이 지금 다섯 종류와 같음을 확인한다. (2) `SourceName::ALL`을 도는 잠긴 테스트(`client.rs` · `second.rs`)는 출처마다 `KeySlot`이 있다고 본다(`SourceName::slot`). 네이버 키는 칸이 둘이고 `KeySlot`이 아니다. (3) 세 enum 모두 `non_exhaustive`가 아니고 server(`team/facts.rs`)가 빠짐없이 `match`하므로, 변형을 더하면 server가 컴파일되지 않는다. 그래서 뉴스는 따로 된 값 · trait로 둔다(사용자 확인 필요 — 보고에 적음).
- `DataAdapter` · `DataConfig` · `DataError` · `KeySource`는 모양을 바꾸지 않는다. `DataError`의 모든 변형이 `origin: SourceName`을 가지므로 뉴스 오류는 새 enum으로 둔다.

### 1. 뉴스 값 (결정 3번: 제목 · 요약 · 링크 · 시각만)

- `#[non_exhaustive] pub enum NewsProviderId { Naver }`. `as_str()`는 `"naver"`, `parse(&str) -> Option<_>`, `Display`.
- `pub struct NewsRequest { pub query: String, pub max_items: u16, pub sort: NewsSort }`, `#[non_exhaustive] pub enum NewsSort { Latest, Relevance }`. `NewsRequest::latest(query, max_items)`. 확인: 앞뒤 공백을 뺀 검색어가 비었거나 100자를 넘거나 제어 문자가 있으면, `max_items`가 1~100 밖이면 요청 없이 `NewsError::InvalidRequest`.
- `pub struct NewsItem { pub provider: NewsProviderId, pub query: String, pub title: String, pub summary: String, pub link: String, pub published_at: DateTime<Utc>, pub known_at: DateTime<Utc> }`. `known_by(decision_at)`는 `Fact::known_by`와 같은 뜻이다(V36). 다른 칸(기사 본문 · 언론사 · 썸네일)은 받지 않는다.
- `pub enum NewsFetched { Items(Vec<NewsItem>), Missing }` — 결과가 비면 오류가 아니라 `Missing`(E32와 같은 뜻).
- `pub fn clean_news_text(&str) -> String`(순수 함수): **태그를 먼저 지우고**(`<b>` · `</b>` 등 `<...>` 전부) 그다음 엔티티를 푼다(`&quot;` `&amp;` `&lt;` `&gt;` `&#39;` `&apos;` `&nbsp;`, `&#숫자;` · `&#x16진;`). 순서가 거꾸로면 `&lt;b&gt;`처럼 글로 쓴 꺾쇠가 태그로 지워진다. 공백은 하나로 모으고 앞뒤를 자른다. 모르는 엔티티는 그대로 둔다.

### 2. 뉴스 공급자 trait과 등록부 (결정 2번)

- `#[async_trait] pub trait NewsProvider: Send + Sync { fn id(&self) -> NewsProviderId; fn keys(&self) -> pi_secrets::ProviderKeys; async fn search(&self, request: &NewsRequest) -> Result<NewsFetched, NewsError>; }`. 키 칸 선언 타입 `ProviderKeys`는 secrets 8차가 만든다.
- `pub struct NewsRegistry`: `register(Arc<dyn NewsProvider>)`(같은 ID는 바꾼다 — Hermes last-writer-wins), `set_default(NewsProviderId)`, `providers() -> Vec<NewsProviderId>`(등록 순서), `search(&NewsRequest)`: 기본 → 처음 등록된 공급자. 없거나 기본이 등록되지 않은 공급자면 요청 없이 `NewsError::NoProvider`.
- `FakeNewsProvider`(정해 준 결과와 오류를 돌려주고 받은 요청을 기록한다). 공개 API다.

### 3. 첫 구현: 네이버 뉴스 검색 (`NaverNews`)

- 키 칸 선언: `pub const NAVER_NEWS_KEYS: ProviderKeys` = 공급자 `naver_news`, 칸 `["client_id", "client_secret"]`(보관소 줄 이름은 secrets 8차 규칙으로 `PRICE_INHA_KEY__NAVER_NEWS__CLIENT_ID` · `..._CLIENT_SECRET`).
- 생성: `NaverNews::new(NaverNewsConfig, keys: Arc<dyn NewsKeySource>, clock: Arc<dyn Clock>) -> Result<Self, NewsError>`(`DataClient::new`과 같은 모양, 주소 검사는 여기서). `NaverNewsConfig { pub base_url, pub timeout, pub max_body_bytes }`, `NaverNewsConfig::official()`의 기본 주소는 `https://openapi.naver.com`. 시간 제한 · 본문 상한의 기본값은 기존 `DataConfig` · http와 같게 한다.
- `pub trait NewsKeySource: Send + Sync { fn read_news_keys(&self, keys: ProviderKeys) -> Result<Option<Credentials>, NewsError>; }` — `SecretStore`에 구현한다(`read_provider`로 읽고, 읽기 오류는 `KeyUnreadable`, 칸이 다 없으면 `None`). 요청할 때마다 묻는다(나중에 넣은 키도 바로 쓴다, `KeySource`와 같다).
- 요청(공식 문서): `GET {base}/v1/search/news.json?query=<UTF-8 퍼센트 인코딩>&display=<max_items>&start=1&sort=<date|sim>`. 머리글 `X-Naver-Client-Id: <client_id>`, `X-Naver-Client-Secret: <client_secret>` **두 개**. 키는 쿼리에 싣지 않는다. 키가 없으면 요청을 보내지 않고 `NoKey`.
- 응답: `items[]`의 `title` · `description` · `originallink` · `link` · `pubDate`만 읽는다. 제목 · 요약은 `clean_news_text`를 거친다. 링크는 `originallink`가 비지 않았으면 그것, 아니면 `link`. `http` · `https`가 아니면 응답 모양 오류. `pubDate`는 RFC 2822 모양(`Mon, 26 Sep 2016 07:50:00 +0900`)을 UTC로 바꾼다. 읽을 수 없거나 칸이 없으면 응답 전체가 `Format`(기존 어댑터와 같은 "모양이 다르면 받지 않는다"). `published_at`이 받은 시각(`known_at`, Clock)보다 뒤인 기사는 버린다(V36: 판단 시각보다 늦은 자료 0건). `items`가 비면 `Missing`.
- 리다이렉트를 따라가지 않는다. 기본 주소가 `https`가 아니면 거부한다(루프백만 예외, market 2차 7번과 같다). 본문 상한을 넘으면 읽지 않고 오류.
- 오류 `#[non_exhaustive] pub enum NewsError`(`src/error.rs`, 모든 변형에 `provider: NewsProviderId`): `NoProvider`, `NoKey`, `KeyUnreadable`, `KeyRejected { code }`, `RateLimited`, `Timeout`, `Unreachable { detail }`, `BadAddress`, `Rejected { code }`, `Format { detail }`, `InvalidRequest { detail }`, `InsecureBaseUrl`. 상태 갈래(공식 오류표): 400 `SE01`~`SE04` · `SE06` → `Rejected`, 401 · 403(인증 실패 · 검색 API 권한 없음) → `KeyRejected`, 429 → `RateLimited`, 404 `SE05` · 3xx → `BadAddress`, 500 `SE99` · 그 밖의 5xx · 연결 실패 → `Unreachable`.
- `NewsError::is_retryable()`: 3차 2번의 `DataError::is_retryable`과 같은 갈래(시간 초과 · 한도 · 닿지 않음만 `true`).
- 오류 문장 · `Debug` · 기록에 클라이언트 ID · 시크릿 값이 들어가지 않는다.
- 확인기: `verify_news_keys(&NaverNewsConfig, &Credentials) -> Result<(), VerifyFailure>`. 한 번 검색(`display=1`)한다. 200이면 `Ok`, 401 · 403이면 `Rejected`, 그 밖(연결 · 5xx · 404 · 3xx)은 `Unreachable`(2차 3번과 같다). server가 secrets의 `verify_provider`를 이것으로 구현한다(server 차수에서 잇는다).

### 4. 자료 출처를 공급자 선언 구조로

- 새 공급자는 `ProviderKeys`(공급자 이름 + 칸)를 선언한다: 이번에는 `NAVER_NEWS_KEYS` 하나.
- 옛 `SourceName`(DART · ECOS · FRED)과 `KeySlot` 연결은 잠긴 테스트가 쓰므로 그대로 둔다. 옛 출처를 공급자 선언으로 옮기는 일(키 파일 줄 이름이 바뀌어 옮기기가 필요하다)은 이번에 하지 않는다.
- FRED는 이미 있으니 그대로다.
- 판단 기록에 뉴스와 출처를 남기는 일(V35), AI 도구로 내주는 일, 화면의 키 칸 목록에 `NAVER_NEWS_KEYS`를 더하는 일은 server 차수에서 잇는다.

### 테스트로 확인할 것 (로컬 가짜 서버만 — 실제 `openapi.naver.com`을 테스트에 넣지 않는다)

1. 요청 모양: 경로 `/v1/search/news.json`, 한글 검색어의 퍼센트 인코딩, `display` · `start=1` · `sort=date|sim`, 머리글 두 개의 이름과 값, 쿼리에 키 없음.
2. 키 없음 · 칸 하나만 있음 → `NoKey`이고 요청 0번. 보관소가 깨졌으면 `KeyUnreadable`.
3. 응답 읽기: `<b>` 지움, 엔티티 풀기, `&lt;b&gt;`는 글로 남음, `originallink` 먼저 · 비면 `link`, `+0900` 시각이 UTC로, `known_at`이 가짜 시계 값.
4. 빈 `items` → `Missing`. 미래 기사는 버림. 시각 · 칸이 깨진 응답 → `Format`. `http(s)` 아닌 링크 → `Format`.
5. 오류 갈래 표(400 SE01 · 401 · 403 · 404 SE05 · 3xx · 429 · 500 SE99 · 연결 실패 · 시간 초과)와 `is_retryable`.
6. 오류 · `Debug`에 ID · 시크릿 값이 없음.
7. `https` 아닌 기본 주소 거부(루프백 예외), 리다이렉트 안 따라감, 본문 상한.
8. `NewsRequest` 확인(빈 검색어, 101자, 제어 문자, `max_items` 0 · 101) → 요청 없이 `InvalidRequest`.
9. 등록부: 기본 고르기, 같은 ID 바꾸기, 공급자 없음 → `NoProvider`, `FakeNewsProvider`.
10. `NAVER_NEWS_KEYS`의 이름 · 칸, `verify_news_keys`의 갈래.
11. `NewsItem::known_by`(V36).
12. 옛 API 그대로: `FactKind::ALL` · `SourceName::ALL` 다섯 · 셋, `DataAdapter` 그대로.

## 하지 않는 것

이 차수 뒤에도 하지 않는 것: 뉴스 본문 긁어 오기, 다른 뉴스 공급자(구글 등 — 다음에 어댑터만 더한다), 뉴스의 `FactKind` 편입(위 부딪히는 곳).
뉴스 검색(V35)은 4차에서 한다(결정 2026-11-21 3번). SEC EDGAR는 미룬다(결정 2026-11-16 17번: EDGAR의 User-Agent 연락처는 사용자가 정한다). 저장(store 10차), 판단, 다시 시도 일정(trigger).

## 테스트 주의

로컬 가짜 서버(wiremock)만 쓴다. 실제 공식 API 주소를 테스트에 넣지 않는다. 받은 값의 알게 된 시각이 가짜 시계의 값과 같은지 본다.

## 필수 번호

C11 E14 E32 V33 V34 V36 P10 D9 V35
