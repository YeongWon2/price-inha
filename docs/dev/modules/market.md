# market 모듈 (T4)

크레이트 `pi-market`, 폴더 `crates/market/`. 구성 요소 A9 시장 어댑터. `pi-secrets`만 쓸 수 있다.
근거: `dev.md`의 D5 · A9 · N6 · V31 · V32, `docs/dev/toss-api-notes.md`.

## 원본 동작 근거

- 원본에 없음(우리 고유): Hermes · DeskRPG · OpenClaw에는 증권사 시세 기능이 없다. `docs/dev/conformance.md` 5장.
- 기준은 토스증권 공식 명세다 — `docs/dev/toss-api-notes.md`.

## 만드는 것

1. **시장 어댑터 trait (D5)**: 시장 이름(국내 · 미국, 나중에 코인이 더해질 수 있게), 비동기 `quotes(종목들) → 시세들`, 비동기 `calendar(날짜) → 그날과 앞뒤 영업일의 세션들`, `session_at(달력, 시각) → 지금 어떤 세션인가(없으면 장 닫힘)`. 에이전트와 규칙 엔진은 이 trait만 안다.
2. **시세 값**: 종목, 가격(십진수 — 부동소수점 금지, `rust_decimal`), 통화(KRW · USD), 거래소가 준 시각(없을 수 있음), 우리가 받은 시각.
3. **늦은 시세 (N6)**: `is_stale(시세, 지금)`. 장이 열린 동안 받은 시각이 30초 넘게 지났으면 늦은 것이다. 기준 30초는 설정값이다.
4. **토스증권 어댑터**: `docs/dev/toss-api-notes.md`와 공식 명세를 따른다.
   - 토큰: `POST /oauth2/token`(form). 받은 토큰을 만료 전까지 다시 쓴다. **필요할 때만 다시 발급한다**(다시 발급하면 이전 토큰이 무효가 된다). 만료가 가까우면(기본 60초 전) 미리 다시 받는다. `expired-token`이나 `token-revoked`를 받으면 한 번만 다시 발급해 다시 시도한다. 동시에 여러 요청이 와도 토큰 발급은 한 번만 나간다.
   - 현재가: `GET /api/v1/prices?symbols=`. 200개가 넘으면 나눠 부른다. `lastPrice` 문자열을 십진수로 읽는다.
   - 장 시간: `GET /api/v1/market-calendar/KR`과 `/US`. 휴장(null)을 그대로 다룬다.
   - 429는 `Retry-After`만큼 기다렸다가 정해진 횟수(기본 2번)까지 다시 시도한다.
   - 오류 구분: 자격 없음(V31 — 키가 없으면 요청을 보내지 않는다), 인증 실패, 허용 IP 아님(`ip-not-allowed` · `access_denied`), 한도 초과, 점검 · 장애, 응답 모양 다름, 연결 실패.
   - 기본 주소와 시간 제한은 설정값이다. 자격은 `pi-secrets`에서 읽고 오류 · 기록에 담지 않는다.
5. **가짜 어댑터**: 정해 준 시세와 달력을 돌려주는 어댑터. 다른 모듈의 테스트와 시연용이다. 공개 API다.
6. **지켜보기 (V32)**: `watch(어댑터, 종목들, 간격) → 시세 흐름`. 간격마다 시세를 받아 채널로 보낸다. 받기에 실패하면 흐름을 끊지 않고 실패도 흐름으로 알린다(받는 쪽이 N6을 판단할 수 있게). 받는 쪽이 사라지면 멈춘다.
7. **키 확인기**: `pi-secrets`의 확인기 trait을 구현해 증권사 키가 닿는지 확인한다(토큰 발급 한 번).

## 2차 작업 (1차 AI 검토의 참고 — 2026-11-15 사용자 "다 개선")

1차는 구현 · 검토가 끝났다. 안전장치가 "모르면 멈추는 쪽"이 되게 하는 것이 뜻이다. **공개 API는 더하기만 한다**(모의계좌 · 접속 관문이 이 크레이트에 기대어 컴파일한다 — 지금 있는 함수 · 타입의 모양을 바꾸지 않는다).

1. **달력이 덮지 않는 시각은 늦은 것으로 본다 (N6)**: 달력의 세 날 범위 밖의 시각이면 `is_stale`은 참이다("장 닫힘"과 "달력이 낡음"을 구분한다).
2. **받은 시각이 미래면 늦은 것으로 본다 (N6)**: 시계가 뒤로 맞춰진 경우다.
3. **토큰 유효 시간이 여유 이하이면 거절한다**: `expires_in`이 다시 발급 여유 이하이면 응답 모양 오류로 끝낸다(요청마다 다시 발급해 서로 토큰을 깨는 일을 막는다).
4. **발급이 실패하면 잠깐 쉰다**: 인증 실패 · 허용 IP 아님으로 발급이 실패하면 정한 시간(설정값) 동안 발급을 다시 보내지 않고 같은 오류를 돌려준다.
5. **종목 글자를 확인한다**: `Symbol::parse(글자) -> Result<Symbol, _>`를 더한다. 공식 명세의 글자(영문 · 숫자 · `.` · `-`)가 아니거나 비었거나 32자를 넘으면 거부한다. `Symbol::new`는 그대로 두되, 어댑터는 시세를 부르기 전에 모든 종목을 같은 규칙으로 확인해 틀린 종목이 있으면 요청을 보내지 않고 오류로 끝낸다(오류에 그 글자를 담지 않는다).
6. **0 이하의 가격은 받지 않는다**: 응답 모양 오류로 끝낸다. 요청하지 않은 종목이 응답에 있으면 버린다.
7. **평문 주소를 막는다**: 기본 주소가 `https`가 아니면 거부한다. 루프백 주소(테스트의 가짜 서버)만 예외다.
8. **응답 크기에 끝이 있다**: 정한 크기(설정값)를 넘는 응답은 읽지 않고 오류로 끝낸다.

## 3차 작업 (2차 검토의 참고 — 2026-11-15 결정)

아래를 **테스트로 먼저 쓰고** 구현한다. 공개 API는 더하기만 한다. 결정은 `docs/dev/decisions.md` 2026-11-15의 조정자 추천을 따른다.

- 0 이하 가격 검사를 요청하지 않은 종목 버리기 뒤로 옮긴다. 요청한 종목이 0원 이하이면 지금처럼 응답 모양 오류다(잠긴 테스트 `toss.rs` "영_이하의_가격은_응답_모양_오류다"와 같게). 요청하지 않은 종목의 0원 이하 가격은 버리고 오류로 보지 않는다.
- 토큰 유효 시간이 여유 이하인 응답도 발급 실패 쉬기에 넣는다.
- 테스트 빈칸: chunked(길이 없는) 응답의 크기 상한, 상한을 넘는 2xx 아닌 응답, 쉬는 중 자격이 바뀌면 바로 발급, 쉬는 동안 동시 호출.
- 미 동부 서머타임은 새 의존 없이 손으로 센다(3월 둘째 일요일 2시 ~ 11월 첫째 일요일 2시). 경계 앞뒤 시각을 테스트한다.

## 4차 작업 (3차 검토의 참고 — 2026-11-15 조정자 결정, 사용자 "알아서 개선")

1. 요청하지 않은 종목의 읽히지 않는 가격도 응답 모양 오류라는 지금 동작을 테스트로 고정한다(시각 칸은 버린 뒤 보므로 둘이 다르다).
2. 묶어 부를 때 요청 종목 목록을 묶음별로 둔다: 다른 묶음의 종목이 섞여 와도 한 종목이 결과에 두 번 들어가지 않는다.
3. 동시 호출 테스트를 `tokio::spawn`으로 여러 스레드(멀티스레드 런타임)에서 겹치게 한다.

## 5차 작업 (4차 검토의 참고 — 2026-11-15 조정자 결정)

1. **결과는 요청 순서다**: 서버가 종목 순서를 바꿔 돌려줘도 결과는 호출한 쪽이 넘긴 종목 순서다. 섞인 순서로 응답하는 가짜 서버로 확인한다.
2. **한 묶음 안에서 같은 종목이 두 번 와도 결과에는 한 번이다**: 같은 종목의 값이 두 번 오면 응답 모양 오류로 본다(어느 값이 맞는지 알 수 없으므로 고르지 않는다). 호출한 쪽이 같은 종목을 두 번 넘기면 한 번만 묻고 결과에도 한 번이다.

## 6차 작업 (5차 검토의 참고 — 2026-11-15 조정자 결정)

1. 응답 안에 같은 종목이 가격 · 통화 · 거래소 시각 모두 같은 값으로 두 번 오면 결과에 한 번만 든다(오류가 아니다)는 결정을 잠긴 통합 테스트로 굳힌다. 시각만 다르면 응답 모양 오류다.
2. `toss.rs`에서 `cooldown_end`의 문서 주석이 `first_occurrences` 위로 밀려난 것을 제자리로 돌리고 `first_occurrences`에 제 주석을 단다.

## pi-secrets와 잇는 법

`pi-market`은 `pi-secrets`에 직접 기대지 않는다. 이 모듈 안에 자격을 주는 작은 trait(`CredentialSource`)을 두고 그것만 안다.
`pi-secrets`로 그 trait을 구현하는 일과 증권사 키 확인기를 `pi-secrets`의 확인기 trait에 잇는 일은 조립 단계(`price-inha`)에서 한다.
이 모듈에서는 "이 자격으로 닿는지 확인하는 함수 `verify`"까지만 만든다. 위 본문에서 "`pi-secrets`에서 읽는다"는 말은 이 trait을 거친다는 뜻이다.

## 7차 작업 (2026-11-21 발표 자료 대조 — 공급자 확장)

근거: `docs/dev/decisions.md` 2026-11-21 2번(공급자 확장 구조) · 5번(VI · 거래정지 추정), `dev.md` D5 · N6, `principles.md` P12.
아래를 **테스트로 먼저 쓰고** 구현한다. **공개 API는 더하기만 한다.** 이번 구현의 시세 공급자는 토스 하나다.

### 원본 동작 근거

- 공급자 등록부: Hermes의 `ProviderProfile`(공급자가 이름과 자기 키 이름 `env_vars`를 선언)과 공급자 정의의 last-writer-wins — `docs/research/hermes-agent.md` 4.1. 같은 공급자 ID를 다시 등록하면 앞의 것을 바꾼다는 규칙이 여기서 온다. 시세 공급자 자체는 원본에 없다(위 원본 동작 근거 절).
- 거래 상태 추정: 원본에 없음 — 사용자 결정 2026-11-21 5번. 시장 규칙의 숫자는 `docs/research/stock-investing-principles.md` 5장(국내 가격제한 ±30%, 정적 VI는 2분 단일가, 미국은 일일 상하한이 없고 LULD로 5분 정지)을 따른다.

### 잠긴 옛 테스트와 부딪히는 곳 (설계가 이것을 피한다)

- `Quote`는 여러 크레이트의 잠긴 테스트가 구조체 글자로 만든다(`market/tests/local.rs` · `round3.rs`, `paper/tests/*`, `exec/tests/suite/common.rs`, `rules/tests/suite/common.rs`, `trigger/tests/*`, `server/tests/*`). **`Quote`에 칸을 더하지 않고 `#[non_exhaustive]`도 달지 않는다.** 거래 상태는 따로 돌려준다.
- `MarketAdapter`는 다른 크레이트도 구현한다(`trigger/tests/round3`, `server/src/demo.rs` · `team/wake.rs` · `feed/fetch.rs`). **새 메서드는 모두 기본 구현을 가진다.** 있는 메서드의 모양은 그대로다.
- `FakeAdapter::new(Market)`, `TossClient::adapter(Market)`, `is_stale`, `StalePolicy`는 그대로다.
- `Market`에 `Crypto`를 이번에 더하지 않는다. `server`의 `_ =>` 갈래 여럿(`demo.rs`, `team/tools.rs`, `feed/late.rs`, `trading/base.rs`)이 모르는 시장을 국내로 다룬다. 코인 공급자를 붙이는 차수에 server와 함께 더한다(사용자 확인 필요 — 보고에 적음).

### 1. 공급자 ID · 자산 종류 (결정 2번)

- `#[non_exhaustive] pub enum ProviderId { Toss }`. `as_str()`는 `"toss"`, `ProviderId::parse(&str) -> Option<ProviderId>`(소문자 그대로만 받는다, 모르는 글자는 `None`), `Display`는 `as_str()`와 같다.
- `ProviderId::credential_fields(self) -> &'static [&'static str]`: 공급자가 자기 키 칸을 선언한다. 토스는 `["client_id", "client_secret"]`. 이 크레이트는 `pi-secrets`에 기대지 않으므로(위 "pi-secrets와 잇는 법") 칸 이름만 선언하고, 보관소 칸과 잇는 일은 server 차수에서 잇는다(이번에는 옛 `KeyKind::Broker`를 그대로 쓴다).
- `#[non_exhaustive] pub enum AssetClass { Stock, Crypto }`. `Market::asset_class(self) -> AssetClass`: `Kr` · `Us`는 `Stock`. `Crypto`는 값으로만 두고 이번에는 어느 시장도 돌려주지 않는다.
- 주문 공급자 등록부는 이 크레이트에 만들지 않는다(주문 API 없음, 아래 하지 않는 것).

### 2. 시세 공급자 등록부 (결정 2번)

`pub struct MarketRegistry`. 공급자를 `(ProviderId, 시장)`으로 쥔다. 시장은 `adapter.market()`으로 정한다.

- `new()`, `register(&mut self, provider: ProviderId, adapter: Arc<dyn MarketAdapter>)`: 같은 `(공급자, 시장)`을 다시 등록하면 앞의 것을 바꾼다(Hermes last-writer-wins). 등록 순서는 기억한다.
- `set_default(&mut self, market, provider)`: 그 시장의 기본 공급자. `route(&mut self, market, symbol, provider)`: 그 종목만 다른 공급자로 보낸다.
- `select(&self, market, &Symbol) -> Result<(ProviderId, Arc<dyn MarketAdapter>), MarketError>`. 고르는 순서: 종목 경로 → 시장 기본 → 그 시장에 처음 등록된 공급자. 하나도 없거나, 경로 · 기본이 **등록되지 않은 공급자**를 가리키면 `MarketError::NoProvider { market }`(새 변형, `MarketError`는 이미 `non_exhaustive`). 다른 공급자로 몰래 넘어가지 않는다.
- `quotes(&self, market, &[Symbol]) -> Result<Vec<Quote>, MarketError>`: 종목마다 공급자를 고르고 공급자별로 한 번씩 부른다. 결과는 **호출한 쪽이 넘긴 종목 순서**(5차 1번과 같다), 같은 종목은 한 번. 공급자 하나라도 실패하면 전체가 그 오류다(모르면 멈춤). 공급자가 하나도 없으면 요청을 보내지 않고 `NoProvider`.
- `providers(&self) -> Vec<(ProviderId, Market)>`: 등록 순서.
- 토스: server가 `TossClient`의 `adapter(Market::Kr)` · `adapter(Market::Us)`를 `ProviderId::Toss`로 등록한다(server 차수에서 잇는다).

### 3. 거래 상태 값과 공급자가 주는 상태 (결정 5번)

- `#[non_exhaustive] pub enum TradingStatus { Normal, Vi, Halted, Unknown }`. `is_tradable(self) -> bool`은 `Normal`만 참이다(모르면 막는다, 결정 2026-11-16 19번).
- `#[non_exhaustive] pub enum StatusSource { Provider, Estimated }`.
- `MarketAdapter`에 기본 구현이 있는 메서드를 더한다: `async fn trading_statuses(&self, symbols: &[Symbol]) -> Result<Option<Vec<(Symbol, TradingStatus)>>, MarketError>`. 기본은 `Ok(None)`("이 공급자는 상태를 주지 않는다"), 요청을 보내지 않는다. 토스는 공식 명세의 현재가 응답에 상태 칸이 없으므로(`toss-api-notes.md`) 기본 그대로 `None`이다.
- `FakeAdapter::set_status(&self, Symbol, TradingStatus)`: 하나라도 정하면 `trading_statuses`가 `Some`(정한 종목만, 넘긴 순서)을 준다. 정하지 않았으면 `None`.

### 4. 시세로 거래 상태 추정 (결정 5번, 순수 함수)

`estimate_status(recent: &[Quote], reference_price: Option<Decimal>, calendar: &MarketCalendar, now: DateTime<Utc>, policy: &StatusPolicy) -> StatusEstimate`

- `recent`는 한 종목의 최근 시세들이다. 순서는 함수가 받은 시각으로 맞춘다. `reference_price`는 가격제한의 기준가(국내는 전일 정규장 종가)다. 토스는 기준가를 주지 않으므로 server가 지난 정규장의 마지막 시세로 넘긴다(server 차수에서 잇는다).
- `pub struct StatusEstimate { pub status: TradingStatus, pub reasons: Vec<SuspectReason> }`. `status`는 `Normal` 아니면 `Unknown`뿐이다(추정으로는 VI와 거래정지를 가르지 못한다). `reasons`가 비었을 때만 `Normal`이다.
- `#[non_exhaustive] pub enum SuspectReason { NoQuote, MixedSymbols, MarketClosed, Stale, NotObservedLongEnough, NotMoving, NoReferencePrice, NearUpperLimit, NearLowerLimit }`. 해당하는 것을 모두 담는다(하나에서 멈추지 않는다). 순서는 이 enum 순서다.
- 규칙 — 아래가 모두 참이면 `Normal`, 아니면 `Unknown`과 까닭:
  1. 시세가 있다(`NoQuote`). 모두 같은 종목이다(`MixedSymbols`).
  2. `now`에 장이 열려 있다(`calendar.session_at(now)`, `MarketClosed`). 달력이 `now`를 덮지 않으면 `Stale`(N6 2차 규칙과 같다).
  3. **늦지 않다**: 가장 늦게 받은 시세가 `is_stale(.., &policy.stale)`로 거짓(`Stale`).
  4. **충분히 지켜봤다**: 받은 시각이 `now - still_after` 이전인 시세가 하나 있다(`NotObservedLongEnough`). 장이 막 열렸을 때는 이 때문에 잠깐 막힌다(모르면 멈춤).
  5. **가격이 움직인다**: 받은 시각이 `(now - still_after, now]` 안인 시세 가운데, 바로 앞 시세와 가격이 다른 것이 하나 있다(`NotMoving`). VI(단일가 2분) · 거래정지 · LULD 정지(5분) 동안은 체결가가 멈춘다.
  6. **가격제한에서 멀다**: 그 시장에 가격제한(`limit_band`)이 있으면, 기준가가 있어야 하고(`NoReferencePrice`) 마지막 가격의 등락률 `(가격 - 기준가) / 기준가`가 `+(band - margin)` 이상이면 `NearUpperLimit`, `-(band - margin)` 이하면 `NearLowerLimit`. 딱 경계 값이면 의심이다. 가격제한이 없는 시장(미국)은 이 검사를 건너뛴다(기준가가 없어도 된다).
- `pub struct StatusPolicy { pub stale: StalePolicy, pub still_after: Duration, pub limit_band: Option<Decimal>, pub near_limit_margin: Decimal }` — 모두 설정값이다. `StatusPolicy::for_market(Market)`이 기본값을 준다:
  - `stale`: `StalePolicy::default()`(30초, N6과 같은 기준을 다시 쓴다).
  - `still_after`: **60초**. 까닭: 정적 VI는 2분 단일가라 60초면 VI의 뒤 절반 이상을 막는다. 더 짧게 하면 거래가 드문 종목이 정상인데도 자주 막힌다. 막혀도 지켜보기일 뿐이라 짧은 쪽보다 이 값이 낫다.
  - `limit_band`: 국내 `Some(0.30)`(±30%), 미국 `None`(일일 상하한 없음).
  - `near_limit_margin`: **0.02**(2%p, 등락률 ±28% 이상이면 의심). 까닭: 상한가 · 하한가 근처는 잔량이 쌓여 체결이 안 되거나 크게 밀린다(`stock-investing-principles.md` 5장 3번). 실제 상 · 하한 가격은 호가 단위로 깎이므로 계산한 경계와 몇 호가 차이가 날 수 있어 그 여유를 둔다.
- 돈 · 비율은 모두 `Decimal`이다(부동소수점 금지).

### 5. 공급자 상태와 추정 합치기 (결정 5번)

`resolve_status(provided: Option<TradingStatus>, estimate: &StatusEstimate) -> (TradingStatus, StatusSource)`

- 공급자가 상태를 주면(`Some`) 그것을 먼저 쓴다: `(그 값, Provider)`. 공급자가 `Unknown`을 주면 추정으로 `Normal`을 만들지 않고 `Unknown`이다(모르면 멈춤).
- `None`이면 `(estimate.status, Estimated)`.
- 늦은 시세(N6) 판단은 이것과 따로 그대로 한다(공급자가 `Normal`이라 해도 `is_stale`은 따로 막는다).
- 주문 직전 점검(P12)에서 이 값을 쓰는 일은 rules · server 차수에서 잇는다.

### 테스트로 확인할 것 (가짜 서버 · 가짜 어댑터만, 시각은 테스트가 정한다)

1. `ProviderId::Toss`의 `as_str` · `parse` · `Display` · `credential_fields`, 모르는 글자 · 대문자는 `None`.
2. `Market::asset_class`가 `Kr` · `Us`에서 `Stock`.
3. 등록부: 시장 기본 고르기, 종목 경로가 기본보다 앞섬, 같은 `(공급자, 시장)` 다시 등록은 바꿈, 공급자 없는 시장 · 등록되지 않은 공급자를 가리키는 경로는 `NoProvider`이고 어느 어댑터도 불리지 않음.
4. 등록부 `quotes`: 가짜 어댑터 둘에 종목을 나눠 보내고 결과가 요청 순서 · 같은 종목 한 번, 한 공급자의 실패가 전체 오류.
5. `trading_statuses` 기본은 `None`이고 요청이 없다(테스트 안에서 이 메서드를 구현하지 않은 어댑터로 확인). 토스 어댑터도 `None`이고 로컬 가짜 서버에 요청이 0번. `FakeAdapter::set_status`가 `Some`을 준다.
6. 추정 — 정상 하나와 까닭마다 하나씩: 시세 없음, 다른 종목 섞임, 장 닫힘, 달력 밖, 늦음(30초 넘음 · 딱 30초는 아님), 덜 지켜봄, 60초 동안 같은 가격, 60초 안에 한 번 바뀜(정상), 국내 +28% · -28%(경계 포함) · +27.99%(정상), 국내 기준가 없음, 미국 기준가 없고 큰 등락도 정상, 순서가 섞인 입력, 까닭 여럿이 함께 담김.
7. `StatusPolicy::for_market`의 기본값(30초 · 60초 · 0.30 · 0.02 · 미국 `None`)과, 값을 바꾸면 판단이 따라 바뀜.
8. `resolve_status`: 공급자 `Vi` · `Halted` · `Normal`이 추정보다 앞섬, 공급자 `Unknown`은 `Unknown`, `None`이면 추정.
9. `TradingStatus::is_tradable`은 `Normal`만 참.
10. 옛 공개 API가 그대로 컴파일됨: `Quote` 구조체 글자, `FakeAdapter::new`, 새 메서드를 구현하지 않은 `MarketAdapter` 구현.

이 차수의 하지 않는 것: 코인 공급자(업비트), 코인의 24시간 달력 · 소수 수량 · 규칙 차이, `Market::Crypto`, 다른 증권사 어댑터, 기준가 저장(server), 웹소켓.

## 하지 않는 것

**주문 · 계좌 · 보유 API를 만들지 않는다.** 토스증권에는 모의투자가 없어 주문 API는 실제 돈을 움직인다. 웹소켓 실시간도 이번에는 하지 않는다.

## 테스트 주의

로컬 가짜 서버만 쓴다. 시간은 테스트가 정할 수 있게 한다(토큰 만료, 늦은 시세, 지켜보기 간격). 실제 `sleep`으로 오래 기다리는 테스트를 쓰지 않는다.

## 필수 번호

D5 N6 V31 V32 P12
