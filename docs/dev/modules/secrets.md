# secrets 모듈 (T2)

크레이트 `pi-secrets`, 폴더 `crates/secrets/`. 구성 요소 A13 비밀 보관.
근거: `dev.md`의 D3 · C3 · C16 · E4 · E38 · N8 · V46.

## 원본 동작 근거

- 대조표: `docs/dev/conformance.md` 2장 "비밀 보관".
- 따르는 원본: Hermes의 `.env` · `auth.json` 보관(위치, 권한, 원자 쓰기) — `docs/research/hermes-agent.md` 3.8 · 4.2 · 7.5. 사용자 결정 "Hermes 방식 파일"(D3).
- 의도해서 다른 것: 등록 전에 실제로 닿는지 확인한다(C3 · E4), 가리기(E38).
- 모르고 달랐던 것(다음 차례에 고친다): `auth.json`의 구조(만료 시각 · 공급자 · 버전 칸), refresh 토큰 필수, `.env` 줄 이름에 공급자 이름, 깨진 `auth.json`의 사본.

## 만드는 것

1. **보관 폴더**: 기본은 홈 폴더 아래 `.price-inha/`. 환경 변수 `PRICE_INHA_HOME`이나 생성자 인자로 바꿀 수 있다(테스트는 임시 폴더). 폴더 권한 0700.
2. **키 파일**: 모든 키는 `.env`(`이름=값` 줄), 권한 0600. (1 · 2차에는 구독 OAuth 토큰을 `auth.json`에 뒀다. 3차부터는 그 파일을 쓰지도 읽지도 않는다.) 쓸 때는 같은 폴더의 임시 파일에 쓰고 이름을 바꾼다(원자 쓰기). 임시 파일도 처음부터 0600이다.
   - 이미 있는 파일의 권한이 0600보다 넓으면 읽기 전에 0600으로 좁힌다.
   - Unix가 아닌 곳에서는 권한 부분을 건너뛰되 나머지는 같다. 권한 테스트는 `#[cfg(unix)]`로 둔다.
3. **키 종류 (C3)**: 증권사, AI, 자료. 종류마다 이름이 정해진 칸(예: 증권사는 `client_id`와 `client_secret`)을 가진다.
4. **등록 (C3 · C16 · E4)**: `register(종류, 값들, 확인기)`. 확인기(trait, 비동기)가 그 키로 실제로 닿는지 확인한다. 확인에 실패하면 **저장하지 않고** 오류를 돌려준다. 성공하면 "등록됨"만 돌려주고 값은 돌려주지 않는다. 다시 등록하면 덮어쓴다.
5. **상태 보기**: 종류별로 등록됐는지만 알려 준다. 값, 값의 일부, 길이를 알려 주지 않는다.
6. **가려진 값 타입**: 비밀 값을 담는 타입은 `Debug` · `Display`로 찍으면 가려진 글자만 나온다. `Serialize`를 구현하지 않는다. 꺼낼 때는 이름이 분명한 함수(`expose()` 같은) 하나만 쓴다. 버려질 때 메모리를 지운다(`zeroize`).
7. **가리기 (E38 · N8)**: `redact(글) → 글`. 등록된 비밀 값이 글에 들어 있으면 가려진 글자로 바꾼다. 값이 URL 인코딩이나 Base64로 들어 있어도 가린다. 짧은 값(8자 미만)은 등록할 때 거부한다(가릴 수 없으므로).
8. **오류에 값 없음**: 이 모듈이 내는 모든 오류 문장과 `Debug` 출력에 비밀 값이 들어가지 않는다.

## 2차 작업 (1차 AI 검토 되돌림)

1차(위 1~8번)는 구현이 끝났다. 2차는 아래를 **테스트로 먼저 쓰고** 구현한다. 공개 API 모양은 바꾸지 않는다(다른 모듈이 기대고 있다).

1. **확인 중인 값도 가린다 (N8 · E38)**: `register`는 칸 검사를 지난 값을 확인기를 부르기 **전에** 가리기 대상에 넣는다. 확인에 실패하거나(거부 · 닿지 않음) 저장에 실패해도 그 값은 이 보관소의 `redact`에서 계속 가려진다. 확인기가 도는 동안 `redact`를 불러도 가려진다.
2. **짧은 값은 읽을 때도 받지 않는다**: 키 파일에 손으로 적힌 8자 미만 값은 등록된 것으로 보지 않는다(가릴 수 없는 값을 내주지 않는다).
3. **보관 폴더는 절대 경로여야 한다**: 홈 폴더나 `PRICE_INHA_HOME`이 빈 글자이거나 상대 경로면 `HomeNotFound`로 거부한다(현재 폴더 아래에 키를 쓰지 않는다).
4. **가리기 대상에 끝이 있다 (N1)**: 같은 칸을 서로 다른 값으로 여러 번 다시 등록해도 가리기 대상이 끝없이 늘지 않는다. 칸마다 지금 값과 바로 앞의 값 몇 개(정한 개수)만 쥔다. 개수는 상수로 두고 문서 주석에 적는다. 지금 등록된 값과 확인 중인 값은 언제나 가려진다.

## 3차 작업: 구독 토큰 칸을 없앤다 (2026-11-15 사용자 결정)

근거: `docs/dev/decisions.md` 2026-11-15, `docs/product/stock-team/dev.md` D4. 구독은 공식 Claude Code CLI에 맡기므로 이 보관소는 구독 토큰을 받지도 쥐지도 않는다(`docs/research/cli-backend.md` 4.5 가: "developers may not collect, store, or intermediate Claude.ai credentials").

1. **키 종류에서 구독 OAuth를 뺀다**: AI 키는 API 키 하나다. 구독 토큰 종류로 등록하는 길이 공개 API에 없다.
2. **이미 있는 파일은 건드리지 않는다**: 예전에 `auth.json`에 적힌 구독 토큰 칸이 있어도 읽어 내주지 않고, 등록된 것으로 보지 않고, 지우지도 고치지도 않는다(사용자의 파일이다). 그 파일이 깨져 있어도 API 키 · 증권사 키를 읽는 데 지장이 없다.
3. 1 · 2차의 나머지(권한, 원자 쓰기, 가리기, 짧은 값 거부, 오류에 값 없음)는 그대로다. 그 테스트 가운데 구독 토큰을 예로 쓰던 것은 남은 종류로 바꿔 같은 성질을 계속 확인한다.

이 차수 뒤 조립(`crates/server`)은 구독 토큰 종류를 쓰던 곳 때문에 컴파일되지 않는다. gate 6차에서 고친다.

## 4차 작업: 옛 구독 토큰 파일이 있는지만 알려 준다 (gate 6차 8번을 받친다)

키 파일 이름은 이 모듈만 안다(`ARCH-SECRET-FILES`). 그래서 화면의 옛 `auth.json` 알림에 필요한 것을 여기서 공개한다. 공개 API는 더하기만 한다.

1. **`pub const LEGACY_AUTH_FILE: &str`**: 옛 구독 토큰 파일 이름(`auth.json`).
2. **`legacy_auth_file_present(보관 폴더) -> bool`**: 보관 폴더에 그 이름의 항목이 있는지만 본다(`symlink_metadata`). 열지도, 읽지도, 고치지도, 지우지도 않는다. 읽을 수 없는 권한(0o000)의 파일 · 심볼릭 링크 · 폴더여도 "있음"이다. 보관 폴더가 없으면 "없음"이다. 권한 · 크기 · 수정 시각 · 내용이 그대로인지 테스트로 확인한다.

## 5차 (취소, 2026-11-15)

지금 구현이 이미 아래를 만족해 실패하는 테스트를 먼저 쓸 수 없었다. 사용자 승인으로 차수를 취소하고 4차 검토 통과 상태로 되돌렸다. 아래는 다음에 회귀 테스트를 더할 때의 후보로 남긴다.

- `legacy.rs`에서 `File::open` · `read` · `OpenOptions`를 금지하는 패턴 검사(열어서 읽기만 하는 구현은 지금 테스트로 잡히지 않는다).
- 0o000 테스트는 root로 돌리면 뜻을 잃는다: root면 알리고 건너뛰게 한다.

## 6차 작업: 자료 키 · 백업 토큰 · VAPID · 바이트 검사 (로드맵 W1, 2026-11-16 결정)

근거: `dev.md` D9 · D7 · E39 · N8, 결정 2026-11-16 18 · 23 · 24번. 원본 근거는 위 원본 동작 근거 절(Hermes `.env` 보관)를 그대로 따른다.
아래를 **테스트로 먼저 쓰고** 구현한다. 공개 API는 더하기만 한다.

1. **자료 키를 출처별 칸으로** (D9): DART · ECOS · FRED 칸. 확인기는 다른 키와 같다.
2. **백업 GitHub 토큰 칸** (D7): fine-grained 토큰 하나. 값을 돌려주지 않는다.
3. **VAPID 비밀키 보관**: 처음 켤 때 만든 값을 0600으로 두고 백업 대상에서 뺀다.
4. **`contains_registered(&[u8]) -> bool`** (E39 · N8): 바이트 안에 등록된 비밀 값이 그대로 · URL 인코딩 · Base64로 들어 있는지 본다(값은 돌려주지 않는다).

이 차수의 필수 번호(N1을 시작할 때 조정자가 아래 필수 번호 절에 더한다): D9 D7 E39

## 7차 후보 (6차 검토의 참고 — 아직 시작하지 않음)

1. 짧은(8자 미만) VAPID 손값을 "없음"으로 보고 덮어쓰지 말고 `Corrupt`로 알린다.
2. 회귀 테스트: 여러 스레드가 처음 켤 때 VAPID를 한 번만 만든다, 깨진 VAPID 줄은 `Corrupt`이고 덮어쓰지 않는다.
3. `contains_registered` 성능(수백 MB): 값마다 한 번 훑는 방식으로. 76자 줄바꿈 Base64는 대상 밖임을 문서에 적는다(백업기는 압축 전에 검사).

## 7차 작업 (후보 일괄 처리)

6차 검토의 "7차 후보" 1~3번을 처리했다. 공개 API는 바뀌지 않았다(더하기만).

1. 키 파일에 손으로 짧게(8자 미만) 적은 VAPID 값은 "없음"이 아니다: `vapid_private_key`가 `generate`를 부르지 않고 `Corrupt`로 알리며 파일을 덮어쓰지 않는다. 빈 줄은 없음으로 보고 새로 만든다.
2. 회귀 시험: 여러 스레드가 함께 처음 켜도 VAPID를 한 번만 만든다. 깨진 VAPID 줄은 `Corrupt`이고 덮어쓰지 않는다.
3. `contains_registered`는 첫 글자가 나오는 자리에서만 맞춰 보고, `%` 없는 값은 갈래 없이 따라간다(40MB 두 번 훑기가 디버그 빌드에서 약 42초 → 약 3초). 76자 줄바꿈 Base64(MIME)는 대상 밖이다. 백업기는 압축 전에 검사한다.

## 8차 작업 (2026-11-21 발표 자료 대조 — 공급자 확장)

근거: `docs/dev/decisions.md` 2026-11-21 2번(공급자는 이름과 자기 키 칸을 선언) · 3번(네이버 뉴스 키 칸 둘) · 4번(Voyage 임베딩 키), `dev.md` D9 · C3 · E4 · E38 · N8.
아래를 **테스트로 먼저 쓰고** 구현한다. **공개 API는 더하기만 한다.**

### 원본 동작 근거

- 공급자가 자기 키 이름을 선언하는 구조: Hermes `ProviderProfile`의 `name` · `env_vars`, 같은 이름은 last-writer-wins — `docs/research/hermes-agent.md` 4.1.
- 보관 방식(`.env` 줄, 0600, 원자 쓰기)은 위 원본 동작 근거 절(Hermes 3.8 · 4.2 · 7.5)을 그대로 따른다. 등록 전 확인 · 가리기는 위에 적은 "의도해서 다른 것" 그대로다.

### 고른 방식: 공급자 선언 칸 (고정 목록 늘리기가 아니다)

고정 목록(`KeySlot`에 변형 더하기)은 잠긴 테스트와 맞지 않는다:

- 잠긴 `tests/round6.rs`가 `KeySlot::ALL`을 돌며 **첫 칸 하나만 채운 값**(`slot_values`)으로 등록이 성공하거나(`다시_연_보관소도_칸_값을_읽고_가린다`, `칸을_등록해도_키_파일은_0600이고_…`) 확인 실패로 끝나기(`확인에_실패하면_어느_칸도_…`)를 기대한다. 칸이 둘인 네이버 칸을 `ALL`에 넣으면 칸 검사에서 먼저 걸려 이 테스트들이 깨진다.
- 같은 파일이 `slot_status()`가 `KeySlot::ALL` 그대로임을, `KeySlot::DATA_SOURCES`가 DART · ECOS · FRED 셋임을 확인한다. `ALL`에서 빼고 따로 두면 상태 보기에서 새 칸이 보이지 않는다.
- `KeyKind`는 `non_exhaustive`가 아니어서 변형을 더하면 다른 크레이트의 `match`가 깨지고, 잠긴 `round3.rs`가 `KeyKind::ALL`을 돈다.

그래서 결정 2번대로 **공급자가 칸을 선언하는 길을 새로 더하고**, 옛 `KeyKind` · `KeySlot` · `ALL` · `DATA_SOURCES` · `status` · `slot_status` · `register` · `register_slot`은 한 글자도 바꾸지 않는다. 옛 칸(증권사 · AI · DART · ECOS · FRED · 백업)을 새 길로 옮기는 일은 줄 이름이 바뀌어 옮기기가 필요하므로 이번에 하지 않는다.

### 1. 공급자 키 선언 `ProviderKeys`

- `#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)] pub struct ProviderKeys`(칸은 비공개). `pub const fn new(provider: &'static str, fields: &'static [&'static str]) -> Self`(상수로 선언할 수 있게 검사하지 않는다), `provider()`, `fields()`, `check(&self) -> Result<(), SecretsError>`.
- 이름 규칙(`check`, 그리고 등록 · 읽기 때 언제나 먼저 본다): 공급자 이름과 칸 이름은 소문자 영문 · 숫자 · 밑줄 하나, 영문으로 시작, 밑줄로 끝나지 않고 밑줄 둘이 붙지 않음, 1~32자. 칸은 1~8개, 겹치지 않음. 어기면 새 변형 `SecretsError::InvalidProviderKeys`(값을 싣지 않는다. `SecretsError`는 이미 `non_exhaustive`라 변형을 더해도 다른 크레이트가 깨지지 않는다).
- 줄 이름: `PRICE_INHA_KEY__{공급자}__{칸}`(대문자, 구분은 밑줄 둘). `env_name(&self, field) -> Option<String>`. 이름 규칙이 밑줄 둘을 막으므로 두 공급자의 줄 이름이 겹치지 않고, 옛 줄(`PRICE_INHA_BROKER_…` 등)과도 겹치지 않는다.
- 이번에 쓰는 선언(선언은 그 공급자를 가진 모듈이 한다 — 결정 2번):
  - 네이버 뉴스: 공급자 `naver_news`, 칸 `client_id` · `client_secret` → `PRICE_INHA_KEY__NAVER_NEWS__CLIENT_ID`, `PRICE_INHA_KEY__NAVER_NEWS__CLIENT_SECRET`. 상수 `NAVER_NEWS_KEYS`는 data 4차가 둔다.
  - Voyage 임베딩: 공급자 `voyage`, 칸 `api_key` → `PRICE_INHA_KEY__VOYAGE__API_KEY`. 상수를 둘 곳(임베딩 클라이언트 위치)은 server 차수에서 잇는다.
  - 이 모듈의 테스트는 위 두 선언을 테스트 안에서 `ProviderKeys::new`로 만들어 쓴다.

### 2. 등록 · 읽기 · 상태

- `register_provider<V: Verifier + ?Sized>(&self, keys: ProviderKeys, values: Credentials, verifier: &V) -> Result<ProviderRegistered, SecretsError>`: `register_slot`과 같은 순서다 — 이름 규칙 → 칸 검사(빠진 칸 · 모르는 칸 · 8자 미만 · 못 쓰는 글자) → **확인기를 부르기 전에** 가리기 대상에 넣기(2차 1번) → 확인 → `.env` 원자 쓰기(0600). 확인 · 저장에 실패하면 저장하지 않고 값은 계속 가린다. `ProviderRegistered { provider: &'static str }`에는 값이 없다. 다시 등록하면 그 공급자의 줄만 바꾼다(다른 줄 · 설명 · 모르는 줄은 글자 그대로).
- `Verifier`에 기본 구현이 있는 메서드를 더한다: `fn verify_provider(&self, keys: ProviderKeys, values: &Credentials) -> impl Future<Output = Result<(), VerifyFailure>> + Send` — `verify_slot`과 같은 모양, 기본은 언제나 `VerifyFailure::Unreachable`(확인할 줄 모르는 확인기로는 저장되지 않는다). 이미 있는 확인기 구현은 그대로 컴파일된다.
- `read_provider(&self, keys) -> Result<Option<Credentials>, SecretsError>`: 칸이 모두 있고 모두 8자 이상이어야 `Some`. 하나라도 없거나 짧으면 `None`(2차 2번과 같다). 그 공급자의 줄이 깨졌으면 `Corrupt`(다른 공급자 · 옛 칸 읽기에는 지장 없다).
- `provider_status(&self, &[ProviderKeys]) -> Result<Vec<ProviderStatus>, SecretsError>`: 넘긴 순서로 `ProviderStatus { provider, registered }`. 값 · 값의 일부 · 길이를 알려 주지 않는다. 옛 `status()` · `slot_status()`에는 새 칸이 나오지 않는다(잠긴 테스트).

### 3. 가리기 (E38 · N8)

- 등록 · 확인 중인 새 칸 값은 `redact`와 `contains_registered`의 대상이다(그대로 · URL 인코딩 · Base64, 7차의 규칙 그대로).
- **보관소를 열 때 `.env`의 `PRICE_INHA_KEY__`로 시작하는 줄은 모두 가리기 대상이다** — 어떤 공급자가 선언했는지 몰라도 가린다(모르면 가린다). 8자 미만 값은 대상이 아니다(가릴 수 없음, 등록된 것으로도 보지 않음).
- 같은 `(공급자, 칸)`을 여러 번 다시 등록해도 가리기 대상은 칸마다 지금 값과 앞의 값 `REDACT_PREVIOUS_KEPT`개만 쥔다(2차 4번).
- 오류 문장 · `Debug` 출력에 값이 없다.

### 테스트로 확인할 것 (임시 폴더와 가짜 확인기만, 시험 값은 `시험용-…` 모양)

1. `ProviderKeys::check`: `naver_news` · `voyage` 통과, 대문자 · 밑줄 둘 · 빈 이름 · 33자 · 숫자로 시작 · 칸 0개 · 칸 9개 · 겹친 칸은 `InvalidProviderKeys`이고 확인기를 부르지 않음.
2. 줄 이름이 정확히 `PRICE_INHA_KEY__NAVER_NEWS__CLIENT_ID` · `…__CLIENT_SECRET` · `PRICE_INHA_KEY__VOYAGE__API_KEY`.
3. 네이버(칸 둘) · Voyage(칸 하나) 등록 성공, 확인기가 선언과 값을 받음, 결과 · 상태에 값 없음.
4. 빠진 칸 · 모르는 칸 · 짧은 값 · 줄바꿈 값 → 오류, 확인기 안 부름, 저장 안 됨.
5. 확인 실패(거부 · 닿지 않음)와 `verify_provider`를 구현하지 않은 확인기 → 저장 안 됨, 값은 계속 가려짐 · `contains_registered` 참.
6. 다시 연 보관소: `read_provider`가 값을 줌, `register_provider`를 부르지 않고 연 보관소의 `redact`가 새 줄 값을(URL 인코딩 · Base64 포함) 가림. 선언을 모르는 `PRICE_INHA_KEY__OTHER__TOKEN` 줄 값도 가림.
7. 칸 하나만 적힌 줄 · 짧은 값 줄 → `None`, 깨진 줄 → 그 공급자만 `Corrupt`.
8. 옛 칸과 섞임: 옛 `register` · `register_slot`이 새 줄을 지우지 않고, `register_provider`가 옛 줄 · `OTHER_NAME=keep` 같은 줄을 지우지 않음. 옛 `status()` · `slot_status()` 결과가 그대로.
9. 권한: 폴더 0700, `.env` 0600, 임시 파일이 남지 않음, 값은 `BACKUP_EXCLUDED_FILES`의 파일에만 있음(`#[cfg(unix)]`).
10. 같은 칸을 여러 번 다시 등록해도 가리기 대상이 끝없이 늘지 않음.
11. `provider_status`의 순서와 등록 여부.

## 9차 작업 (2026-11-26 조정자, server 39차 N2 보고 — 공급자 키 지우기)

근거: 결정 24(메신저). server 39차의 `DELETE /api/messenger/telegram`은 등록한 봇 토큰을 지워야 하는데, 이 모듈에는 공급자 키를 **지우는** 공개 길이 없다(등록 · 읽기뿐). server가 비밀 파일을 직접 고치면 A13 · `ARCH-SECRET-FILES`를 어긴다. 사용자가 넣은 비밀값을 지울 길이 없는 것은 그 자체로도 모자란 것이다.

1. `SecretStore::remove_provider(keys: ProviderKeys) -> Result<bool, SecretsError>`: 그 공급자의 칸을 모두 비밀 파일에서 지운다. 있어서 지웠으면 `true`, 없었으면 `false`(오류 아님). 지금의 등록과 같은 방식으로 원자적으로 쓰고(임시 파일 → 옮기기) 권한 0600을 지킨다. 다른 공급자 · 다른 키 종류의 줄은 건드리지 않는다.
2. 지운 뒤 `read_provider`는 `None`이고 상태(`slot_status` 등 지금 공급자 상태를 내는 곳)는 "없음"이다.
3. **가리기는 남긴다**: 지운 값도 이 프로세스가 끝날 때까지 가리기 대상에 그대로 둔다(지운 직후에도 기록 · 화면에 그 글자가 나오면 안 된다 — 지금 "실패한 등록의 값도 계속 가린다"와 같은 규칙).
4. 지우는 중 실패하면 파일은 그대로다(반쯤 지워지지 않는다).

- 원본 동작 근거: 원본에 없음(우리 것) — 결정 24, A13. 원본 OpenClaw · Hermes도 채널을 지우는 명령이 있다(`docs/research/onboarding-messenger.md` 2.4).
- 시험으로 확인할 것: 등록한 공급자 키를 지우면 `true` · 읽기 `None` · 상태 없음 / 없는 것을 지우면 `false` / 다른 공급자 · 증권 · AI 키는 그대로 읽힌다 / 지운 뒤 파일 권한 0600 · 파일에 그 값이 없다 / 지운 값이 여전히 가려진다 / 다시 등록하면 읽힌다.

## 하지 않는 것

OS 키체인, DB 저장, 백업에 넣기, 화면 만들기. 실제 증권사 · AI에 닿는 확인기는 이 모듈에 없다(확인기 trait만 있고, 테스트는 가짜 확인기를 쓴다).

## 테스트 주의

테스트용 비밀 값은 진짜 키처럼 보이지 않는 글자(예: `시험용-비밀-값-0001`)를 쓴다. `sk-ant-…` 같은 모양은 쓰지 않는다.

## 필수 번호

D3 C3 C16 E4 E38 N8 N1 V46 D9 D7 E39

### 9차 대조 (2026-11-26 조정자)

### 대조

| 시험 · 문서 경로 | 판정 | 근거 |
|---|---|---|
| `crates/secrets/tests/`의 모든 기존 시험 | 유지 | 9차는 지우는 길 하나를 더할 뿐이다 — 등록 · 읽기 · 가리기 · 권한의 지금 동작은 그대로 |
