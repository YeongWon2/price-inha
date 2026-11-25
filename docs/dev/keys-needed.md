# 필요한 키 · 토큰 · 계정 (사용자가 준비할 것)

이 문서는 코드(`crates/`)와 설계 문서를 읽고 정리했다. 키 값이나 값처럼 생긴 예시는 적지 않았다(칸 이름과 모양 설명만).
"확인 못 함"이라고 쓴 것은 코드나 문서에서 찾지 못한 것이다. 어떤 서비스에도 접속하지 않았다.

> 모든 키는 **보관 폴더**(기본 `~/.price-inha`, 바꾸려면 `--home` 또는 환경 변수 `PRICE_INHA_HOME`)의 `.env`에 들어간다.
> 서버는 비밀 값을 받는 인자나 환경 변수를 두지 않는다(`crates/server/src/settings.rs` "비밀 값(키 · 토큰)을 받는 인자와 환경 변수는 없다").
> 키는 설정 화면이나 `/keys`, 터미널 마법사(`price-inha setup`)로 등록한다.

---

## 1. 한눈에 보는 표

| 키 | 꼭 필요한가 | 무엇에 쓰나 | 없으면 어떻게 되나 | 어디서 받나 | 어디에 넣나 | 칸 이름 |
|---|---|---|---|---|---|---|
| 증권사 키 (토스증권 Open API) | **필수** (시연 모드 제외) | 시세(현재가)와 장 시간 달력을 받는다. 주문 · 계좌 · 보유는 부르지 않는다 | 시작하지 않는다(`ready`가 거짓). 시세 상태 "증권사 키가 없음" | 토스증권 개발자 사이트(`developers.tossinvest.com`). 자세한 순서는 그 서비스 문서 | 설정 화면 "증권사 키" · 다른 기기는 `/keys` · 터미널 `price-inha setup broker` | `client_id`, `client_secret` |
| AI 연결 (둘 중 하나) | **필수** (시연 모드 제외) | 투자 팀(담당 AI)의 판단 | 시작하지 않는다 | (가) Anthropic API 키 — Anthropic 콘솔 (나) Claude Code CLI 구독 로그인 — 공식 `claude` | 설정 화면 "AI 연결" · `/keys` "AI 키" · `price-inha setup ai` | API 키: `api_key` · CLI: 키 칸 없음(경로와 로그인만) |
| 지켜볼 종목 | **필수** (키는 아니지만 시작 조건) | 시세를 받을 종목 | 시작하지 않는다 | 사용자가 정한다 | 설정 화면 · `price-inha setup symbols` · `--symbols` | (비밀 아님) 종목 코드 |
| DART 키 | 선택 | 국내 공시 · 재무 | 그 자료를 묻지 않고 건너뛴다(오류로 세지 않는다) | 금융감독원 DART 오픈API(`opendart.fss.or.kr`) | 설정 화면 "바깥 키" · `/keys` | `api_key` |
| ECOS 키 | 선택 | 국내 금리 · 환율 | 위와 같음 | 한국은행 ECOS(`ecos.bok.or.kr`) | 설정 화면 "바깥 키" · `/keys` | `api_key` |
| FRED 키 | 선택 | 미국 금리(10년 국채, 계열 `DGS10`) | 자료에 넣지 않는다 | 세인트루이스 연준 FRED API(`api.stlouisfed.org`를 쓴다) | 설정 화면 "바깥 키" · `/keys` | `api_key` |
| 네이버 뉴스 키 | 선택 | 종목 뉴스 검색(제목 · 요약 · 링크) | 뉴스를 묻지 않고 자료에 넣지 않는다 | 네이버 개발자센터 "검색 > 뉴스" 오픈API(`openapi.naver.com`을 쓴다) | 설정 화면 "바깥 키" · `/keys`("네이버 뉴스 키") | `client_id`, `client_secret` |
| 텔레그램 봇 토큰 | 선택 | 메신저로 알림 받기 · 조회 · 거절 · 전체 정지 · 팀 대화 | 메신저 기능이 꺼져 있다(루프를 돌리지 않는다) | 텔레그램 BotFather(`/newbot`) | 설정 화면 메신저 칸(그 컴퓨터에서만) · `price-inha setup messenger` | `bot_token` (공급자 `telegram`) |
| 백업 GitHub 토큰 | 선택 | 암호화한 백업을 비공개 GitHub 저장소에 올린다 | 백업이 꺼져 있다("백업이 꺼져 있습니다") | GitHub fine-grained 개인 토큰 | 설정 화면 "백업" · `/keys` | `token` |
| 백업 저장소 이름 · age 수신자 공개키 | 선택 (백업을 쓸 때 필수) | 백업의 올릴 곳과 암호화 | 백업이 꺼져 있다 | 저장소는 GitHub 비공개 저장소. 공개키는 사용자가 만든 age 키 쌍의 공개키 | 설정 화면 "백업" | 저장소 `소유자/이름`, 수신자 (`age1`로 시작하는 공개키) |
| 접속 키 | **서버가 만듦** | 다른 기기에서 들어올 때 | 그 컴퓨터에서는 필요 없다. 만들기 전엔 원격에서 못 들어온다 | 서버가 만들어 한 번 보여 준다 | 설정 화면 "접속 키 만들기/바꾸기" · `price-inha access-key` | (사용자가 줄 값 없음) |
| 웹 푸시 VAPID 키 | **서버가 만듦** | 폰 · 브라우저 푸시 알림 서명 | — | 서버가 처음 켤 때 만든다 | (넣지 않는다) | — |
| 실행 증표 | **서버가 만듦** | CLI가 "이 보관 폴더의 서버"인지 확인 | — | 서버가 켤 때마다 새로 만든다 | (넣지 않는다) | — |
| 뜻 검색(Ollama) | 선택, **키 없음** | 지난 기록의 뜻 검색(임베딩) | 꺼짐이 기본. 꺼져 있으면 Ollama에 묻지 않는다 | 이 컴퓨터에 Ollama를 직접 설치 | 설정의 "뜻 검색 켜기" | (키 없음) |

### 근거 코드 경로

- 키 종류와 칸: `crates/secrets/src/kind.rs`(`KeyKind::Broker` `client_id` · `client_secret`, `KeyKind::AiApiKey` `api_key`, 옛 `KeyKind::Data`), `crates/secrets/src/slot.rs`(`KeySlot::Dart` · `Ecos` · `Fred` `api_key`, `BackupGithubToken` `token`), `crates/secrets/src/provider.rs`(공급자 키 `PRICE_INHA_KEY__{공급자}__{칸}`).
- 필수 / 선택: `crates/server/src/setup.rs`의 `SETUP_ITEMS`(broker · ai · symbols 필수, data 선택), `crates/server/src/desk.rs`의 `ready()`와 `registered()`.
- 바깥 키 칸의 등록 이름과 경로: `crates/server/src/slots.rs`(`KEY_SLOTS`, `provider_declarations`, `/api/setup/slots`, `/api/setup/providers`).
- 키 확인기: `crates/server/src/keys.rs`(`KeyVerifier`), `crates/data/src/verify.rs`, `crates/backup/src/token.rs`, `crates/market/src/toss.rs`.
- 화면: `crates/server/web/index.html`(설정 화면 제목들), `crates/server/web/keys.html`과 `crates/server/web/src/keys/main.ts`(`/keys`).
- 터미널 마법사: `crates/server/src/cli/setup.rs`(구역 `ai` · `broker` · `symbols` · `messenger` · `service` · `open`), `crates/server/src/cli/command.rs`.
- AI 연결: `crates/server/src/ai_link.rs`, `crates/server/src/cli/setup/ai.rs`.
- 메신저: `crates/server/src/messenger.rs`(`TELEGRAM_KEYS`), `crates/server/src/messenger/http.rs`.
- 백업: `crates/server/src/reach/backup_http.rs`, `crates/backup/src/lib.rs`, `crates/server/src/cli/restore.rs`.
- 자료 사용 방식: `crates/server/src/team/facts.rs`(DART · ECOS · FRED), `crates/server/src/team/news.rs`(네이버).
- 접속 키 · 증표 · VAPID: `crates/gate/src/access.rs`, `crates/server/src/cli/access.rs`, `crates/server/src/run_proof.rs`, `crates/notify/src/vapid.rs`, `crates/server/src/reach/linked.rs`.
- 시연 모드: `crates/server/src/settings.rs`(`--demo`), `crates/server/src/app/run.rs`, `crates/server/src/app/parts.rs`, `crates/server/src/demo.rs`.
- 바깥 주소 전체: 코드에서 찾은 바깥 서버는 토스증권 · DART · ECOS · FRED · 네이버 · 텔레그램 · GitHub · Anthropic 여덟 곳뿐이다(`*/src`의 주소 상수).
- 문서: `docs/dev/toss-api-notes.md`, `docs/research/cli-backend.md`, `docs/research/onboarding-messenger.md`, `docs/dev/decisions.md` 결정 23 · 24 · 26, `docs/product/stock-team/prd.md` R11-3 · R13 · R15 · R16.

---

## 2. 처음 켤 때 최소로 필요한 것

### 시연 모드 (`price-inha run --demo`)

키 · 토큰 · 계정이 하나도 필요 없다.

- 시세는 가짜 어댑터가 만든다. 바깥으로 요청을 보내지 않는다(`crates/server/src/app/parts.rs`의 `settings.demo` 분기, `crates/server/src/demo.rs`).
- 필수 설정 없이 바로 시작한다(`crates/server/src/setup.rs` 머리 설명 "시연 모드(`--demo`)는 설정 없이 바로 시작한다").
- 팀의 AI도 시연용 가짜 AI를 쓴다(`DemoAi`, `crates/server/src/app/run.rs`의 `demo_ai_pause`). 공시도 시연용 가짜다(`DemoWiring::binary`).
- 메신저는 루프를 돌리지 않고, 텔레그램에 등록하려 하면 409로 거절한다(`MESSENGER_DEMO_REFUSED`).
- 시연 모드에서 DART · ECOS · FRED · 네이버가 어떻게 동작하는지는 확인 못 함. 시연 공시가 가짜로 나오는 것까지만 코드에서 봤다.

### 모의 운용 (실제 시세 + 모의계좌)

필수 셋이 모두 있어야 시작한다(`SETUP_ITEMS`).

1. **증권사 키** (토스증권 `client_id` · `client_secret`)
2. **AI 연결** — API 키 **또는** Claude Code CLI 로그인 가운데 하나
3. **지켜볼 종목** — 하나 이상(최대 200개)

나머지(DART · ECOS · FRED · 네이버 · 텔레그램 · 백업)는 선택이다.
마지막 필수 항목이 등록되면 다시 켜지 않아도 바로 시작한다(`setup.rs` "알림" 절).

터미널에서 한 번에 하려면 `price-inha setup`(구역 차례: ai → broker → symbols → messenger → service → open).
메신저 구역은 전체를 돌 때는 안내 한 줄만 보이고, `price-inha setup messenger`로 따로 해야 묻는다(`cli/setup.rs`).

---

## 3. 키마다

공통 사항(코드에서 확인한 것):

- 등록하면 **먼저 실제로 닿는지 확인하고**, 확인에 실패한 키는 저장하지 않는다(`SecretStore::register` · `register_slot`, `crates/secrets/src/store.rs` · `slot.rs`). 거절은 `key_rejected`(422), 닿지 못함은 `unreachable`(502)로 나온다(`/keys` 화면 글자는 "키가 맞지 않습니다" · "확인할 곳에 닿지 못했습니다").
- 값은 **8글자 이상**이어야 한다(`MIN_SECRET_CHARS = 8`). 짧으면 확인기를 부르기 전에 거부한다.
- 키 등록 시도 횟수에 제한이 있다. 넘으면 확인기를 부르지 않고 거부한다(`TooManyAttempts`, 429).
- 같은 칸에 다시 등록하면 덮어쓴다.
- 등록한 값은 다시 보여 주지 않는다. 응답은 등록 여부뿐이다(R15-1 · V46).
- 키 지우기 길: 텔레그램만 `DELETE /api/messenger/telegram`이 있다. **나머지 키는 지우는 화면 · 명령을 코드에서 찾지 못했다**(`crates/server/src/setup_http.rs`의 경로 목록에 지우기가 없음). 바꾸려면 다시 등록(덮어쓰기)한다. 지우려면 `.env`의 해당 줄을 손으로 지우는 길이 있어 보이나(시작한 뒤 키 파일이 지워지면 멈추고 다시 필요하다고 알린다는 설명이 `setup.rs`에 있음), 이 방법을 안내하는 문서는 확인 못 함.
- 점검은 `price-inha doctor`(CLI, 키 칸마다 "있음 / 없음"만 보인다) 또는 설정 화면의 진단 표(`GET /api/setup/doctor`: `store` · `keys` · `ai` · `ai_daily_limit` · `backup` · `demo` · `market`). 진단은 바깥에 요청을 보내지 않고 등록 여부만 본다(`crates/server/src/setup_doctor.rs`).

### 3.1 증권사 키 (토스증권)

- **쓰이는 곳**: `POST /oauth2/token`(토큰 발급), `GET /api/v1/prices`(현재가), `GET /api/v1/market-calendar/{KR|US}`(장 시간)만 부른다. **주문 · 계좌 · 보유는 부르지 않는다**(`crates/market/src/toss.rs` 머리 설명).
- **실제 증권사 주문은 나가지 않는다.** 토스 Open API에는 모의투자가 없고 주문 API는 실제 계좌에 실제 주문을 내므로, 가격인하는 **자체 모의계좌(`pi-paper`)**를 쓰고 그 주문 API를 아예 부르지 않는다(`docs/dev/toss-api-notes.md` "모의투자 | 없다", AGENTS.md 6번). 따라서 이 키는 **시세와 달력 읽기 전용**으로만 쓰인다.
- **받는 곳**: 토스증권 개발자 사이트(`docs/dev/toss-api-notes.md`의 안내 주소). 발급 화면 순서는 확인 못 함 — 자세한 순서는 그 서비스 문서.
- **받을 때 알아둘 것(문서에서 확인)**:
  - **허용 IP를 등록해야 한다.** 등록하지 않은 IP는 403 `ip-not-allowed`(토큰 발급에서는 `access_denied`)다. 코드는 이를 `IpNotAllowed`로 읽고 키 확인에서 "거절"로 낸다(`keys.rs`의 `verify_broker`). 이 서버를 돌리는 컴퓨터의 공인 IP가 바뀌면 다시 등록해야 할 수 있다(`docs/research/remote-access-security.md` 410행도 새 컴퓨터의 공인 IP를 토스 허용 IP에 추가하라고 적음).
  - **client마다 유효한 토큰은 1개**다. 다시 발급하면 이전 토큰이 바로 무효가 된다. 같은 키를 다른 프로그램에서도 쓰고 있으면 서로 토큰을 끊을 수 있다(`toss-api-notes.md` "토큰 주의"). 등록 확인도 토큰을 한 번 발급한다(`verify_broker`).
  - 이 서버 코드가 요구하는 권한 범위 이름은 확인 못 함(시세 · 장 시간 조회만 쓴다는 것까지만 확인).
- **넣는 법**: 설정 화면 "증권사 키 (필수)"의 `client_id` · `client_secret`(이 컴퓨터) · 다른 기기는 접속 키로 들어와 `/keys` · 터미널 `price-inha setup broker`(첫 칸이 빈 줄이면 건너뛰기). 환경 변수 · 인자로는 받지 않는다.
- **확인**: 등록할 때 토큰 발급 한 번으로 확인한다. 등록 뒤 `price-inha doctor`의 "증권사 키 있음 / 없음", 설정 화면의 시세 상태.
- **바꾸기**: 같은 칸에 다시 등록한다. 시작한 뒤 새 키로 이어 간다(`setup.rs`: "시작한 뒤 키를 다시 등록하면 새 키로 이어 간다").
- **주의**: 키가 털렸다고 의심되면 토스 쪽에서 재발급한 뒤 다시 등록한다. 키는 백업에 들어가지 않으므로 새 컴퓨터에서 되살리면 **다시 등록해야 한다**.

### 3.2 AI 연결 (둘 중 하나)

사용자가 고른 **하나만** 쓴다. 둘 다 있어도 넘어가지 않는다. 고른 것이 없으면 API 키를 고른 것으로 본다(`ai_link.rs`). 고른 방식은 보관 폴더의 `ai.json`에 둔다(비밀 아님).

**(가) Anthropic API 키**

- **받는 곳**: Anthropic 콘솔(코드가 부르는 주소는 `api.anthropic.com`). 자세한 순서는 그 서비스 문서.
- **넣는 법**: 설정 화면 "AI 연결"에서 API 키를 고르고 `/keys`의 "AI 키"(`api_key`) · 터미널 `price-inha setup ai`에서 답 `key` 입력.
- **확인**: 등록할 때 `AnthropicProvider::verify`로 한 번 확인한다. 로그인 거절은 `key_rejected`.
- **주의**: 사용량만큼 API로 과금된다(구독이 아니다). 하루 호출 상한은 `--ai-daily-limit <수>` 또는 환경 변수 `PRICE_INHA_AI_DAILY_LIMIT`로 둘 수 있고 주지 않으면 상한이 없다(`settings.rs`). 상한을 두는 편이 안전하지만 기본값은 상한 없음이다.

**(나) Claude Code CLI (구독 로그인)**

- **받는 곳**: 공식 Claude Code CLI(`claude`)를 사용자가 직접 설치하고 터미널에서 `claude auth login`으로 로그인한다(`cli/setup.rs`의 `SETUP_CLI_LOGIN_HINT`).
- **이 보관소는 구독 토큰을 받지도 쥐지도 않는다**(`secrets/src/kind.rs` 머리 설명, 결정 24 · D4). 키 칸 없이 "CLI 경로"와 선택적 "전용 로그인 폴더"만 `ai.json`에 둔다.
- **찾는 법**: 마법사는 `claude`를 `PATH`에서만 찾는다. 설정 화면은 서버가 `PATH` → `~/.local/bin/claude` → `/opt/homebrew/bin/claude` → `/usr/local/bin/claude` 순으로 찾아 칸을 채운다(`ai_link.rs`의 `CLI_USER_LOCATION` · `CLI_COMMON_LOCATIONS`).
- **확인**: 버전 → 로그인 순으로 보고, 모델을 부르지 않는다. 로그인된 **구독**(`claude.ai`)이어야 갖춰진 것이다. API 키로 로그인돼 있으면 "구독이 아닙니다"로 거절한다(`setup/main.ts`의 메시지).
- **주의**:
  - 환경에 `ANTHROPIC_API_KEY`가 있는데 CLI를 고르면 "CLI도 API 키로 과금될 수 있다"고 경고한다(`SETUP_CLI_BILLING_WARNING`). 서버가 CLI 자식에게 넘기는 환경은 `HOME` · `PATH` · `LANG` · `LC_ALL` · `TMPDIR` · `USER` · `LOGNAME` 일곱 개뿐이다(`CLI_ENV_ALLOWLIST`). 그래도 CLI가 자체 설정으로 API 키를 쓰는지는 확인 못 함.
  - CLI를 고르면 화면이 "구독 약관은 자동화된 이용과 증권 매매 목적 이용을 제한할 수 있습니다. 판단은 사용자에게 있습니다."라는 한 줄을 보인다(`CLI_TERMS_NOTICE`).
  - 구독 사용량 한도에 걸리면 호출이 한도 오류로 나온다(`docs/research/cli-backend.md`).
  - 옛 `auth.json`이 보관 폴더에 남아 있어도 읽지도 · 고치지도 · 지우지도 않고 알림만 보인다(`LEGACY_AUTH_NOTICE`).
- **바꾸기**: 설정 화면 "AI 연결"에서 다른 방식을 다시 고른다. 고른 뒤 CLI 확인이 실패하면 AI 연결이 갖춰지지 않은 것으로 보고, 시작한 상태면 멈추고 설정이 필요하다고 알린다.

### 3.3 지켜볼 종목

키는 아니지만 시작 조건이다. 설정 화면 · `price-inha setup symbols` · `--symbols`(또는 `PRICE_INHA_SYMBOLS`). 인자로 주면 그 실행 동안은 그것이 우선이고 저장 파일(`watchlist.json`)을 덮지 않는다. 업종표에 없는 종목은 규칙 화면에서 업종을 넣어야 살 수 있다(`SETUP_SECTOR_NOTICE`).

### 3.4 DART · ECOS · FRED 키 (선택)

- **쓰는 곳**: `crates/data`가 요청한다. DART는 국내 공시 · 재무(고유번호가 있는 종목만), ECOS는 국내 금리 · 환율, FRED는 미국 10년 국채 금리(`DGS10`)(`team/facts.rs`).
- **받는 곳**: DART `opendart.fss.or.kr` · ECOS `ecos.bok.or.kr` · FRED `api.stlouisfed.org`(코드의 기본 주소). 신청 화면 순서는 확인 못 함 — 자세한 순서는 각 서비스 문서.
- **넣는 법**: 설정 화면 "바깥 키 (선택)" 또는 `/keys`(칸은 모두 `api_key`). 터미널 마법사에는 이 키 구역이 없다(`SetupSection` 목록에 없음).
- **확인**: 등록할 때 키로 한 번 닿아 본다. DART는 회사를 정하지 않은 공시검색 1건, ECOS는 통계표 목록 1행, FRED는 계열 `DGS10` 정보를 부른다(`crates/data/src/verify.rs`). 한 번만 보내고 다시 시도하지 않는다.
- **없으면**: 키가 없어 묻지 못한 것은 "물은 것도 실패한 것도 아니다". 자료 이상 신호로도 세지 않고, FRED는 자료 항목에서도 뺀다(`facts.rs`).
- **주의**: DART와 FRED는 키를 요청 주소의 쿼리(`crtfc_key` · `api_key`)에 싣는다(`dart.rs` · `fred.rs`의 `KEY_PARAM`). 기록 · 화면에는 가리기가 적용되지만, 사용자 쪽에서 프록시나 요청 기록을 따로 남기면 주소에 키가 남을 수 있다. 호출 한도 · 과금은 각 서비스 정책이다(코드는 한도 응답을 `RateLimited`로 읽는다. 세부 확인 못 함).
- **옛 `data` 칸(`PRICE_INHA_DATA_API_KEY`)**: 설정 상태의 선택 항목 `data`는 이 옛 칸의 등록 여부다. 그런데 옛 `KeyKind::Data`는 확인할 곳이 없어 **등록 요청이 언제나 실패한다**(`keys.rs` `KeyKind::Data => Err(Unreachable)`). 실제 자료 키는 위의 DART · ECOS · FRED · 네이버 칸에 넣는다. 설정 상태의 `data` 항목이 계속 "미등록"으로 보여도 정상일 수 있다(이것이 화면에서 어떻게 보이는지는 확인 못 함).

### 3.5 네이버 뉴스 키 (선택)

- **쓰는 곳**: 종목 뉴스 검색(`GET /v1/search/news.json`). 키는 머리글(`X-Naver-Client-Id` · `X-Naver-Client-Secret`)로만 보내고 주소에 싣지 않는다(`crates/data/src/naver.rs`).
- **받는 곳**: 네이버 개발자센터의 "검색 > 뉴스" 오픈API(`naver.rs`가 따르는 공식 문서). 애플리케이션 등록 순서는 확인 못 함 — 자세한 순서는 그 서비스 문서.
- **넣는 법**: 설정 화면 "바깥 키" 또는 `/keys`의 "네이버 뉴스 키 (뉴스 검색)": `client_id` · `client_secret`. 보관소 줄 이름은 `PRICE_INHA_KEY__NAVER_NEWS__CLIENT_ID` · `..__CLIENT_SECRET`.
- **확인**: 등록할 때 검색 한 번(`verify_news_keys`). 401 · 403이면 거절.
- **없으면**: 묻지 않고 자료에도 넣지 않는다(`team/news.rs`).
- **주의**: 네이버의 하루 요청 상한을 `TeamWiring::news_daily_limit`로 세고 저장소에 남긴다. 상한 숫자는 확인 못 함.

### 3.6 텔레그램 봇 토큰 (선택)

- **받는 곳**: 텔레그램의 BotFather에서 `/newbot`(`docs/research/onboarding-messenger.md` 180행). 대화 순서는 그 문서나 텔레그램 안내 참고(코드 문서에는 `/newbot` 한 줄만 있음).
- **넣는 법**(**그 컴퓨터에서만**, 원격에서는 관문이 403): `price-inha setup messenger` 또는 설정 화면의 메신저 칸.
  1. 봇 토큰 입력(터미널에서는 보이지 않는 입력). 서버가 먼저 `getMe`로 확인하고 저장한다.
  2. 봇에게 아무 말이나 보낸다. 봇이 페어링 코드를 답한다(8자리, 1시간 유효, 대기 3건, 틀린 코드 5번이면 잠김 — `crates/channel/src/pairing.rs`와 `messenger.rs`).
  3. 받은 코드를 설정 화면이나 마법사에 넣어 승인한다. 말할 수 있는 사람은 **페어링한 한 명**이다. 1:1 대화만 받는다.
- **확인**: 설정 화면 메신저 상태(`GET /api/messenger`: 봇 이름 · 상태 `off` / `polling` / `unauthorized` / `error` · 페어링 여부). 토큰이 거절되면 상태가 `unauthorized`가 되고 받기 루프를 멈춘다. `price-inha doctor`의 키 줄에는 텔레그램이 나오지 않는다(`cli/doctor.rs`의 키 줄은 공급자 선언 가운데 네이버만 읽음) — 메신저는 설정 화면에서 본다.
- **지우거나 바꾸기**: `DELETE /api/messenger/telegram`이 토큰 · 페어링 장부(`messenger-pairing.json`) · 받은 자리(`messenger-cursor.json`)를 지우고 루프를 멈춘다. 허용된 사람만 빼는 길은 `POST /api/messenger/pairing/revoke`. 새 토큰은 다시 등록하면 된다.
- **메신저로 할 수 있는 것**: 알림 받기, `/status` · `/approvals` · `/reject` · `/stop` · `/help`, 팀 대화(결정 26). **돈이 움직이는 승인 · 다시 시작 · 규칙 변경 · 실전 승격은 받지 않는다.** 나가는 글은 가린 글만이다.
- **주의**: 봇 토큰이 유출되면 BotFather에서 토큰을 새로 받아 다시 등록한다(페어링은 한 명만 허용되고, 이미 허용된 사람이 있으면 먼저 빼야 새 승인이 된다 — `MESSENGER_OWNER_EXISTS`). 서버의 연결 기록은 주소에 토큰이 들어가므로 HTTP 라이브러리의 자세한 기록을 켜지 않는다.

### 3.7 백업 (GitHub 토큰 · 저장소 · age 공개키) (선택)

- **무엇을 하나**: 매일 저장소(SQLite)를 떠서 manifest와 묶고 age 수신자 공개키로 암호화한 뒤 **비공개 GitHub 저장소**에 contents API로 올린다. 보존은 일 7 · 주 4 · 월 12(`crates/backup/src/lib.rs`, 결정 23).
- **사용자가 준비할 것 셋**:
  1. **비공개 GitHub 저장소**(`소유자/이름`, 영문 · 숫자 · `.` `_` `-`).
  2. **GitHub fine-grained 토큰** 하나(`token` 칸). 받는 곳은 GitHub 설정의 개인 토큰 화면(코드는 `api.github.com`을 부른다). **어떤 권한이 필요한지는 코드와 문서 어디에도 정확히 적혀 있지 않아 확인 못 함** — 코드는 `PUT/GET {저장소}/contents` 형태(`crates/backup/src/remote.rs`)로 올리므로 그 저장소의 contents 쓰기가 필요할 것으로 보이나 이는 추정이다. 권한은 그 저장소 하나로 좁히는 것이 안전하다(결정 23이 "fine-grained 토큰"이라고 정했다).
  3. **age 키 쌍**. 공개키(`age1`로 시작)를 설정에 넣고, **비밀키는 사용자가 GitHub 밖에 직접 보관**한다(결정 23). 키 쌍을 만드는 길은 이 저장소에 없다(되살리기 명령은 `age-keygen` 출력 모양의 파일을 읽는다고만 적혀 있다, `cli/restore.rs`). 만드는 도구는 사용자가 따로 마련한다.
- **넣는 법**: 설정 화면 "백업 (선택)"에서 저장소와 공개키(`POST /api/setup/backup`, 그 컴퓨터에서만), 토큰은 설정 화면 · `/keys`의 "백업 GitHub 토큰". 저장소와 공개키는 보관 폴더의 `backup-settings.json`에 남는다(비밀 아님). 공개키는 빈 묶음을 실제로 봉해 보는 것으로 검사한다.
- **확인**: 토큰은 등록할 때 `GET /user` 한 번으로 확인한다(200이면 받아들임, 401 · 403이면 거절). 이 확인은 **토큰이 유효한지만** 보며 저장소 접근 권한까지는 보지 않는다. 설정 화면 진단의 "백업" 줄(꺼짐 / 마지막 백업 시각 / 실패해 다시 올리기를 기다림).
- **되살리기**: `price-inha restore <백업 파일> --identity <age 비밀키 파일> --into <빈 폴더>`. 비밀키 파일은 인자로만 쓰고 어디에도 남기지 않는다. 빈 폴더에만 되살린다.
- **주의**: age 비밀키를 잃으면 백업을 열 수 없다. 키는 백업에 들어가지 않으므로 **되살린 뒤 증권사 · AI · 텔레그램 키 등을 다시 등록해야 한다**.

### 3.8 접속 키 (서버가 만든다)

- 같은 컴퓨터에서는 키 없이 들어온다. 다른 기기에서 열려면 접속 키를 넣는다.
- **만드는 법**: 설정 화면의 "접속 키 만들기/바꾸기" 또는 터미널 `price-inha access-key [--rotate]`(도는 서버에 요청해 만들고, 서버가 없으면 같은 규칙으로 저장소에 직접 만든다). 키는 **만들 때 한 번만** 보여 준다. 20글자이고 글자 집합은 숫자와 대문자 중 헷갈리는 글자를 뺀 32개다(`gate/src/access.rs`의 `ACCESS_KEY_LEN` · `ACCESS_KEY_ALPHABET`).
- 서버에는 소금을 친 SHA-256 해시만 저장한다. 키 글자는 응답 · 기록 · 로그에 남기지 않는다.
- 다른 기기에서 같은 기기가 5번 틀리면 15분 동안 막고 알림이 온다(`MAX_KEY_FAILURES` · `LOCKOUT`). 키를 바꾸면 다른 기기의 세션이 끊기고 새 키를 넣어야 한다. 원격 세션은 30일 유효하다.
- 원격 기기에서 키 등록은 `/keys`(세션이 있어야 열림)로만 한다. 코드 주석에 "TOTP 로그인"이라는 말이 있으나(`page.rs` · `setup.rs`) TOTP를 구현한 코드는 찾지 못했다 — 원격 로그인은 접속 키로 한다.

---

## 4. 키가 저장되는 곳과 지켜지는 방식 (코드에서 확인한 사실)

- **위치**: 보관 폴더(기본 `~/.price-inha`)의 `.env` 한 파일. 이름은 `PRICE_INHA_BROKER_CLIENT_ID` · `PRICE_INHA_BROKER_CLIENT_SECRET` · `PRICE_INHA_AI_API_KEY` · `PRICE_INHA_DART_API_KEY` · `PRICE_INHA_ECOS_API_KEY` · `PRICE_INHA_FRED_API_KEY` · `PRICE_INHA_BACKUP_GITHUB_TOKEN`, 공급자 키는 `PRICE_INHA_KEY__{공급자}__{칸}`(텔레그램 · 네이버 뉴스)이다(`secrets/src/kind.rs`, `provider.rs`).
- **보관 폴더 경로**: 절대 경로만 받는다. 상대 경로이거나 빈 값이면 거부한다(현재 폴더 아래에 키를 쓰지 않기 위해서, `fsio.rs`).
- **권한**: 폴더는 0700, `.env`는 0600(유닉스). 폴더가 넓게 열려 있으면 0700으로 좁힌다. 파일을 읽기 **전에** 권한을 0600으로 좁힌다.
- **쓰기**: 같은 폴더에 처음부터 0600인 임시 파일에 쓰고 디스크에 내린 뒤 이름을 바꾼다(원자 쓰기). 다른 줄(사용자가 적은 주석 등)은 그대로 둔다(`store.rs` 시험).
- **값 보호**: 값은 `SecretString`에만 담겨 `Debug` · `Display`가 `[가려짐]`만 찍고 직렬화되지 않는다. 오류에 값이나 자유 글자를 싣지 않는다.
- **가리기**: 바깥으로 나가는 글(기록 · AI 입력 · 알림 · 메신저)에서 등록된 값을 `[가려짐]`으로 바꾼다. 지금 값과 바로 앞 값 3개까지, 확인 중인 값까지 가린다(`REDACT_PREVIOUS_KEPT`). 가리기에 실패하면 메신저로 보내지 않는다.
- **짧은 값**: 손으로 고친 파일에 8글자 미만 값이 있으면 등록되지 않은 것으로 본다.
- **백업에 들어가는가**: **들어가지 않는다.** 백업은 SQLite 스냅샷만 담고, `.env`와 옛 `auth.json`은 `BACKUP_EXCLUDED_FILES`로 대상에서 뺀다. 게다가 백업기는 올리기 전에 묶음에 등록된 비밀 값이 들어 있는지 검사하고, 들어 있으면 올리지 않는다(`contains_registered`).
- **키가 아닌 설정 파일(비밀 아님)**: `ai.json`(고른 AI 방식 · CLI 경로) · `watchlist.json` · `backup-settings.json`(저장소와 공개키) · `semantic-settings.json` · `messenger-pairing.json`(0600) · `messenger-cursor.json`. 이 파일들도 SQLite 스냅샷 밖이라 백업에 들어가는지는 코드 문서로는 "스냅샷만 담는다"는 것까지만 확인했다. 되살린 뒤 다시 정해야 할 수 있다.
- **화면**: 브라우저 저장소 · 쿠키 · 주소에 값을 남기지 않는다(V46 · D11). 키는 등록 요청 본문에만 실린다.
- **키 등록 경로의 접근**: `POST /api/setup/keys`는 세션 + 같은 출처 + **그 컴퓨터**에서만. 다른 기기는 접속 키로 로그인한 세션으로 `POST /api/remote/keys`(`/keys` 화면)만 쓴다.

---

## 5. 서버가 스스로 만드는 것 (사용자가 줄 필요가 없는 것)

| 무엇 | 언제 · 어디에 | 비고 |
|---|---|---|
| 접속 키 | 사용자가 "만들기"를 누를 때 · 서버 저장소에는 소금 친 해시만 | 한 번만 보여 준다 |
| 세션 쿠키 | 그 컴퓨터에서 들어올 때 · 접속 키로 들어올 때 | 값은 난수 256비트, 서버는 해시만 메모리에 둔다. 서버를 다시 켜면 세션이 사라진다. 원격 세션은 30일 |
| 실행 증표 | 서버를 켤 때마다 새로 · 보관 폴더의 `run.proof`(0600) | CLI(`setup` · `access-key` · `open`)가 "이 보관 폴더의 서버"인지 확인한 뒤에만 비밀 값을 보낸다. 끄면 지운다 |
| 웹 푸시 VAPID 비밀키 | 알림을 처음 켤 때 없으면 만든다 · 보관 폴더의 `vapid.key`(0600) | 서버가 실제로 쓰는 것은 `crates/notify`의 `VapidKey::load_or_create`다. `pi_secrets`에도 `.env`용 VAPID 칸이 있으나 서버 조립에서 부르는 곳은 찾지 못했다. 공개키는 `GET`으로 브라우저에 준다 |
| 푸시 연락처(`sub`) | 코드의 기본값 | 사용자가 줄 필요 없음 |
| 페어링 코드 | 텔레그램 봇에 말을 걸면 봇이 답한다 | 사용자가 만드는 것이 아니라 받아서 입력한다 |

사용자가 따로 줄 필요가 없는 것: 토스 토큰(서버가 키로 발급 · 갱신), 푸시 구독(브라우저가 서버에 구독을 등록), 뜻 검색의 Ollama(키 없음, 루프백 `127.0.0.1:11434`만 받는다. 모델은 `bge-m3`가 기본이고 Ollama에 미리 받아 두어야 하는지는 확인 못 함. `price-inha doctor`는 켜져 있을 때만 Ollama에 닿고 모델이 있는지 본다).

---

## 6. 아직 지원하지 않는 것

| 항목 | 상태 | 근거 |
|---|---|---|
| 슬랙 | **만들지 않는다.** 메신저는 텔레그램 하나다 | `docs/dev/decisions.md` 결정 26 |
| Codex(ChatGPT) 로그인 | 약관을 확인하기 전까지 두지 않는다 | 결정 24 |
| Anthropic OAuth 직접 구현 · 다른 프로그램의 토큰 파일이나 키체인 읽기 | 하지 않는다(약관 · D4). 구독은 공식 CLI에 맡긴다 | 결정 24, `secrets/src/lib.rs` |
| 구독 토큰 등록 칸 | 없다. 옛 `auth.json`은 읽지 않는다 | `secrets/src/kind.rs` |
| 메신저로 돈이 움직이는 승인 · 다시 시작 · 규칙 변경 · 실전 승격 | 받지 않는다(웹에서만) | 결정 24 · 26 |
| 실제 증권사 주문 | 보내지 않는다. 모의계좌(`pi-paper`)만 | AGENTS.md 6번, `toss-api-notes.md` |
| 토스 웹소켓 실시간 시세 | 쓰지 않는다(REST로 주기적으로 받는다) | `toss-api-notes.md` "이번에 쓰지 않는 것" |
| 키를 지우는 화면 · 명령 | 텔레그램 외에는 찾지 못함(다시 등록하여 덮어쓴다) | `setup_http.rs` 경로 목록 |

---

## 7. 코드에서 확인하지 못한 것 (요약)

- 토스증권 · DART · ECOS · FRED · 네이버 · 텔레그램 BotFather · GitHub 토큰 · Anthropic 콘솔의 **발급 화면 순서** (받는 곳만 적었다).
- 토스 키에 필요한 권한 범위 이름, 백업 GitHub 토큰에 필요한 권한 이름(contents 쓰기로 추정할 뿐).
- age 키 쌍을 만드는 길(저장소에 생성 코드 없음. 되살리기가 `age-keygen` 출력 모양을 읽는 것만 확인).
- 시연 모드에서 DART · ECOS · FRED · 네이버 키의 동작.
- CLI 방식에서 `ANTHROPIC_API_KEY`가 실제로 과금에 영향을 주는 경로(서버는 자식에게 일곱 환경 변수만 넘기지만 CLI 자체 설정은 보지 않았다).
- 옛 `data` 설정 항목이 화면에서 어떻게 보이는지, 네이버의 하루 상한 숫자, Ollama 모델을 미리 받아야 하는지.
- 키를 `.env`에서 손으로 지우는 방법을 안내하는 문서.
