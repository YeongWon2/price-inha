# 원본 대조 (2026-11-14)

결론: 동작 86개를 대조했다. 같음 28 · 의도해서 다름 16 · **모르고 다름 19** · 아직 안 만듦 15 · 우리 고유 8. 가장 큰 문제는 구독 OAuth 자리다: 원본 둘(Hermes · OpenClaw)이 모두 보내는 헤더와 본문 변환이 없고, `auth.json`에 만료 시각 칸이 없어 갱신을 얹을 수 없다.

- 대조 대상: `pi-store` · `pi-secrets` · `pi-ai` · `pi-market` · `pi-paper` · `pi-gate` + `price-inha` (작업 T1~T5, T7).
- 방법: 조사 문서 넷(`docs/research/hermes-agent.md` · `deskrpg.md` · `deskrpg-hermes-plugin.md` · `openclaw.md`)에서 "가져옴" 동작을 뽑고, 얇은 곳은 원본 소스를 직접 읽어 확인했다. 원본은 실행하지 않았다.
- 코드는 고치지 않았다. 이 문서가 결과물의 전부다.

## 읽는 법

| 판정 | 뜻 |
|---|---|
| 같음 | 원본과 같은 동작 |
| 의도 | 사용자 결정이나 지시서 때문에 다르게 만든 것 (근거를 적음) |
| **모름** | 원본과 다른데 그렇게 정한 근거가 없는 것 |
| 미구현 | 이 모듈 책임인데 아직 없는 것 (미룬 작업 번호를 적음) |
| 고유 | 대조할 원본이 없는 것 |

원본 경로 표기 (모두 저장소 밖, 읽기 전용 `…/scratchpad/` 아래):

| 표기 | 실제 폴더 |
|---|---|
| `H:` | `src-hermes/hermes-agent/` |
| `D:` | `src-deskrpg/deskrpg/` |
| `P:` | `src-plugin/deskrpg-hermes-plugin/deskrpg_plugin/` |
| `O:` | `src-openclaw/openclaw/` |

우리 경로는 저장소 루트의 `crates/` 기준이다. "문서"는 조사 문서의 절 번호다.

---

## 1. AI 공급자 어댑터 (`pi-ai`, 조립은 `server/src/keys.rs`)

| # | 동작 | 원본 | 우리 | 판정 | 고칠 것 |
|---|---|---|---|---|---|
| A1 | API 키는 `x-api-key`로 보내고 `Authorization`은 안 보낸다 | hermes 4.1. `H:agent/anthropic_adapter.py:446-447, 508`, `O:packages/ai/src/transports/anthropic-transport-stream.ts:275-277` | `ai/src/anthropic.rs:164` | 같음 | — |
| A2 | OAuth 토큰은 `Authorization: Bearer`로 보내고 `x-api-key`는 안 보낸다 | hermes 4.2. `H:agent/anthropic_adapter.py:448-449, 508` | `ai/src/anthropic.rs:165-168` | 같음 | — |
| A3 | OAuth일 때 덧붙이는 헤더: `anthropic-beta`에 `claude-code-20250219` · `oauth-2025-04-20`, `user-agent: claude-code/<버전> …`, `x-app: cli` | hermes 4.2 끝. `H:agent/anthropic_adapter.py:238, 243, 294-307, 367, 509, 513-514`. OpenClaw도 같다: `O:packages/ai/src/transports/anthropic-transport-stream.ts:215-223`, `O:packages/ai/src/providers/anthropic-model-contract.ts:140-164` | 버전 헤더와 인증 헤더만 보낸다. `ai/src/anthropic.rs:91-95` | **모름** | 원본 주석은 이 헤더가 없으면 429 · 간헐 500 · 거부라고 적는다(`H:…anthropic_adapter.py:240-242, 423-425`). 지금 구현으로는 OAuth 토큰 등록 확인(`GET /v1/models`)부터 실패할 수 있다. 헤더를 넣을지(약관 판단 필요), OAuth 자리를 "아직 안 됨"으로 닫을지 사용자가 정해야 한다 |
| A4 | OAuth일 때 본문 변환: 시스템 글 맨 앞에 Claude Code 정체성 문장 블록, 도구 이름 `mcp__` 접두 | `H:agent/anthropic_adapter.py:318-319, 547-555, 594-597`, `O:packages/ai/src/transports/anthropic-payload-policy.ts:260-275` | 시스템 글을 그대로 보낸다. `ai/src/wire.rs:32-49` | **모름** | A3과 한 묶음으로 정한다. 도구 이름 변환은 도구 호출(T9) 때 같이 넣는다 |
| A5 | API 키일 때 공통 beta 헤더(`interleaved-thinking-…`, `fine-grained-tool-streaming-…`) | `H:agent/anthropic_adapter.py:233-234, 509` | 없음 | 미구현 (T9) | 도구 · 스트리밍 · thinking을 넣을 때 공식 문서로 다시 확인 |
| A6 | `anthropic-version` 값 | OpenClaw `2023-06-01` (`O:…anthropic-transport-stream.ts:234-236`). Hermes는 SDK에 맡긴다(값 확인하지 못함), 보조 경로는 `2023-06-01` (`H:hermes_cli/models.py:2140`) | `2026-10-21`. `ai/src/lib.rs:28` | 의도 | `ai.md` 3번 "헤더와 본문 모양은 공식 문서를 확인하고 맞춘다" |
| A7 | 토큰 종류 판별: 접두사로 본다(`sk-ant-api…`가 아니면서 `sk-ant-` · `eyJ` · `cc-`로 시작하면 OAuth). 풀 항목도 `sk-ant-oat`면 OAuth로 고친다 | `H:agent/anthropic_credentials.py:62-69`, `H:agent/credential_pool.py:201-205`, `O:packages/ai/src/providers/anthropic-auth-headers.ts:5-10` | 사용자가 고른 칸(종류)만 믿는다. `server/src/keys.rs:248-254`, `ai/src/credential.rs:35-39` | **모름** | OAuth 토큰을 API 키 칸에 넣으면 `x-api-key`로 나가 거절된다. 등록 때 접두사를 보고 칸을 바로잡거나 거부한다 |
| A8 | 자격이 둘 다 있을 때의 순서: OAuth 토큰이 API 키보다 먼저 | hermes 4.2 표 (`H:agent/anthropic_credentials.py:700-718`), openclaw 2.4 (`O:src/agents/auth-profiles/order.ts:292-300`). Hermes는 한쪽을 저장할 때 다른 칸을 비운다(`H:hermes_cli/config.py:2640-2660`) | API 키가 먼저. `server/src/keys.rs:151, 172-181` | **모름** | 조사 문서의 [추정] 권고("기본은 API 키")와는 맞지만 결정 기록이 없다. `dev.md` D4나 `ai.md`에 결정으로 적거나, 사용자가 고르는 설정을 둔다 |
| A9 | OAuth 로그인 · 토큰 갱신(만료 확인, 락 안에서 읽기 → POST → 쓰기, 영구 실패 기억) · Claude Code 자격 빌려 쓰기 | hermes 4.2 (`H:agent/anthropic_credentials.py:473-541, 763-816`), openclaw 2.4 | 없음 (`ai.md` "하지 않는 것") | 미구현 (**작업 번호 없음**) | 작업 계획에 넣는다. 지금은 access 토큰이 만료되면 사람이 다시 등록해야 한다 |
| A10 | 401이면 자격을 다시 읽어 값이 바뀌었을 때 턴당 한 번 다시 시도 | `H:agent/turn_recovery.py:413-414, 450-459`, `H:agent/client_lifecycle.py:843-897` | 인증 실패는 다시 시도하지 않는다. `ai/src/retry.rs:35` (호출마다 자격은 새로 읽는다: `ai/src/anthropic.rs:130-134`) | 미구현 (T9) | A9와 같이 |
| A11 | 자격이 없으면 바깥으로 요청하지 않고 오류 | `H:hermes_cli/runtime_provider.py:412-422` | `ai/src/anthropic.rs:130-134`, `:160-162` | 같음 | — |
| A12 | 자격 증명 풀(키 여러 개 순환, 모델별 쿨다운) | hermes 4.3 (`H:agent/credential_pool.py:118-124`), openclaw 2.4 쿨다운 표 | 자격 한 개 | 미구현 (**작업 번호 없음**) | "가져옴(단순화)" 판정인데 받을 작업이 없다. T9에 넣거나 "안 가져옴"으로 바꾼다 |
| A13 | 큰 오류 구분: 401 · 403 인증, 429 한도, 5xx · 529 장애, 400 잘못된 요청, 시간 초과 | hermes 3.2 표. `H:agent/error_classifier.py:555-556, 1245-1286, 1461-1463` | `ai/src/wire.rs:129-139`, `ai/src/anthropic.rs:176-182` | 같음 | — |
| A14 | 세부 오류 구분: 402 과금, 404 없는 모델, 413 너무 큼, 400 안의 컨텍스트 초과, 429 안의 과금 문구 | `H:agent/error_classifier.py:35-68, 1231-1242, 1363-1442` | 4xx를 모두 "잘못된 요청"으로 묶는다. `ai/src/wire.rs:133-136` | 미구현 (T9) | 컨텍스트 초과 → 압축, 과금 → 멈춤은 턴 루프가 구분해야 한다 |
| A15 | 시간 초과 · 연결 실패도 다시 시도한다 | `H:agent/error_classifier.py:489-497, 1063-1097` (`retryable=True`) | 다시 시도하지 않는다. `ai/src/retry.rs:17-18, 35` | **모름** | 근거가 코드 주석("이미 처리됐을 수 있다")뿐이다. 원본대로 다시 시도하거나, 지시서에 결정으로 적는다 |
| A16 | 다시 시도 횟수: 시도 총 3번(= 다시 시도 2번) | `H:hermes_cli/config_defaults.py:121`, `H:agent/conversation_loop.py:1583` | 다시 시도 2번. `ai/src/lib.rs:31` | 같음 | — |
| A17 | 간격: `min(2초 × 2^(n-1), 60초)` + 균등 jitter `[0, 간격의 절반]` | `H:agent/turn_recovery.py:1489`, `H:agent/retry_utils.py:121-134` | 1초부터 두 배, 상한 60초, jitter 없음. `ai/src/anthropic.rs:19`, `ai/src/retry.rs:9, 40-43` | **모름** | 첫 간격 2초, jitter 추가 |
| A18 | `Retry-After`: 초와 HTTP 날짜 둘 다 읽고, 다시 시도하는 모든 오류에 쓰고, 상한 600초, 헤더가 없으면 본문 `retry_after` | `H:agent/retry_utils.py:30-64, 179-195`, `H:agent/turn_recovery.py:1482-1489` | 초만 읽는다(`ai/src/wire.rs:116-118`). 429에만 쓴다(`ai/src/retry.rs:29-34`). 30초를 넘으면 기다리지 않고 오류를 돌려준다(`ai/src/retry.rs:13, 31`) | **모름** | 날짜 모양 읽기, 5xx에도 적용. 30초 상한을 둘지는 호출하는 쪽(T9)과 같이 정한다 |
| A19 | 시간 제한: 읽기 900초, 연결 10초 | `H:agent/anthropic_adapter.py:378-382` | 요청 전체 600초, 연결 제한은 따로 없음. `ai/src/anthropic.rs:16, 93` | **모름** | 연결 제한(10초)을 따로 둔다 |
| A20 | 스트리밍 호출이 기본 | `H:agent/anthropic_adapter.py:871-885` | 한 번에 받는다. `ai/src/anthropic.rs:128-151` | 미구현 (T9) | `ai.md` "하지 않는 것" |
| A21 | 다 실패한 뒤: 폴백 공급자 → 자동 복구 5주기 | hermes 3.2 표. `H:agent/turn_api_error.py:377-418` | 없음 | 미구현 (T9) | — |
| A22 | 기본 주소 `https://api.anthropic.com` | `H:hermes_cli/auth.py:208` | `ai/src/lib.rs:25` | 같음 | — |

## 2. 비밀 보관 (`pi-secrets`)

사용자 결정 D3은 "Hermes 방식대로 홈 폴더의 파일"이다. 위치 · 권한 · 원자 쓰기는 같고, **파일 안의 모양(줄 이름 · `auth.json` 구조)** 이 다르다.

| # | 동작 | 원본 | 우리 | 판정 | 고칠 것 |
|---|---|---|---|---|---|
| B1 | 홈 폴더 아래 폴더 하나, 환경 변수로 바꾼다 | hermes 3.8. `H:hermes_cli/config.py:473-475` | `~/.price-inha`, `PRICE_INHA_HOME`. `secrets/src/lib.rs:67-70`, `secrets/src/fsio.rs:22-37` | 같음 | — |
| B2 | 폴더 권한 0700 | `H:hermes_cli/config_home.py:28-45` | `secrets/src/fsio.rs:51-69` | 같음 | — |
| B3 | `.env` 권한: 새 파일만 0600, 있던 파일은 넓어도 그대로 둔다 | `H:hermes_cli/config.py:2435-2459, 2604` | 쓸 때도 읽기 전에도 0600으로 좁힌다. `secrets/src/fsio.rs:73-82, 99, 128` | 의도 | `secrets.md` 2번 |
| B4 | 원자 쓰기: 같은 폴더 임시 파일 → fsync → 이름 바꾸기 | `H:hermes_cli/config.py:2431-2452`, `H:utils.py:238-302` | `secrets/src/fsio.rs:122-145` | 같음 | — |
| B5 | 값 따옴표 규칙: `#` · 따옴표 · 공백이 있으면 큰따옴표, `\` · `"`만 풀어 쓴다. 읽을 때도 그 둘만 되돌린다 | `H:hermes_cli/config.py:2483-2491`, `H:agent/secret_scope.py:316-331` | `$` · 백틱 · `\`도 따옴표 조건이고 `$` · 탭도 풀어 쓴다. 읽을 때 `\n` · `\r` · `\t` · `\$`도 되돌린다. `secrets/src/envfile.rs:35-55, 77-98` | **모름** | 우리끼리는 쓰고 읽으면 같다. Hermes 파서와 파일을 주고받을 일이 없으면 그대로 두고 결정으로만 적는다 |
| B6 | `.env` 줄 이름: 공급자가 정한 이름(`ANTHROPIC_API_KEY`, `ANTHROPIC_TOKEN` …) | hermes 4.2. `H:hermes_cli/config.py:2640-2660`, plugin 4-1 (`P:provider_keys.py:1-7`) | `PRICE_INHA_AI_API_KEY` 등. 공급자 이름이 없다. `secrets/src/kind.rs:49-58` | **모름** | 공급자를 더할 때(OpenAI 호환 · Codex는 "가져옴") 이름을 바꿔야 한다. 지금 공급자 이름을 넣은 줄 이름으로 바꾸거나(`PRICE_INHA_ANTHROPIC_API_KEY`), 원본 이름을 그대로 쓴다 |
| B7 | 우리 것이 아닌 줄(설명 · 모르는 이름)은 그대로 둔다 | `H:hermes_cli/config.py:2350-2360` | `secrets/src/envfile.rs:130-141` | 같음 | — |
| B8 | 같은 이름이 여러 줄이어도 결과는 값 하나 | 플러그인: 첫 자리에 한 줄만 남긴다(`P:envfile.py:72-84`). Hermes: 첫 줄만 바꾼다(`H:hermes_cli/config.py:2494-2513`) | 우리 줄을 모두 지우고 끝에 한 줄. 읽을 때는 마지막 줄. `secrets/src/envfile.rs:108-110, 130-148` | 같음 | 줄의 자리만 다르다 |
| B9 | 읽기: `export ` 머리, 작은따옴표, 공백 뒤 `#` 설명을 받는다 | `H:agent/secret_scope.py:293-313, 360-414` | `secrets/src/envfile.rs:16-75` | 같음 | — |
| B10 | 쓰기 락: 프로세스 사이 flock(`.env.lock`, `auth.lock`, 15초) | hermes 4.3. `H:hermes_cli/config.py:2400-2428`, `H:hermes_cli/auth.py:646-719` | 프로세스 안 잠금만. `secrets/src/store.rs:38, 225` | 의도 | D1(프로그램 하나), hermes 9.2("다중 프로세스 계층은 버려도 된다") |
| B11 | `auth.json` 구조: `version` · `providers` · `active_provider` · `credential_pool{공급자: [항목]}` · `updated_at`. 항목에 `id, label, auth_type, access_token, refresh_token, expires_at, last_status …` | hermes 3.8 · 4.3. `H:hermes_cli/auth.py:723, 787-788, 1099`, `H:agent/credential_pool.py:208-239` | `{"ai_oauth": {"access_token", "refresh_token"}}` 두 칸뿐. `secrets/src/authfile.rs:1`, `secrets/src/kind.rs:34, 62-69` | **모름** | 만료 시각 칸이 없어 갱신(A9)을 얹을 수 없고, 공급자 이름과 파일 버전도 없다. `version` + 공급자 묶음 + `expires_at`을 넣는다. 등록된 파일이 생기기 전에 고치는 편이 싸다 |
| B12 | refresh 토큰이 없는 토큰(`claude setup-token`으로 받은 것)도 받는다 | hermes 4.2 (`H:agent/anthropic_credentials.py:721-738`), OpenClaw `token` 종류(`O:src/agents/auth-profiles/credential-schema.ts:52-58`) | `refresh_token` 칸이 필수이고 8자 이상이어야 한다. `secrets/src/kind.rs:34`, `secrets/src/store.rs:58-62` | **모름** | `refresh_token`을 선택 칸으로 한다 |
| B13 | `auth.json` 0600 + 원자 쓰기 + 부모 폴더 0700 | `H:hermes_cli/auth.py:774-789`, `H:utils.py:400-419` | `secrets/src/fsio.rs:122-145` | 같음 | — |
| B14 | `auth.json`이 깨졌으면 `auth.json.corrupt`로 사본을 남기고 빈 저장소로 본다 | `H:hermes_cli/auth.py:726-757` | 오류를 내고, 다시 등록하면 사본 없이 덮어쓴다. `secrets/src/authfile.rs:66-76` | **모름** | 덮어쓰기 전에 사본을 남긴다 |
| B15 | 키를 저장하기 전에 실제로 닿는지 확인한다 | 원본은 **확인 없이 저장**한다: Hermes(`H:hermes_cli/model_setup_flows.py:852-862`, `H:hermes_cli/web_routers/config_env.py:1049-1052`), DeskRPG(`D:src/app/api/gateways/route.ts:28-48`) | 확인에 실패하면 저장하지 않는다. `secrets/src/store.rs:157-175`, `server/src/keys.rs:227-260` | 의도 | `dev.md` C3 · C16 · E4 |
| B16 | 가리기: 알려진 키 모양(정규식)을 찾아 앞 6자 · 뒤 4자만 남긴다 | `H:agent/redact.py:97-102, 168-185, 995-999` | 등록된 값과 대조해 통째로 `[가려짐]`. URL 인코딩 · Base64 변형도 가린다. `secrets/src/store.rs:217-222`, `secrets/src/lib.rs:79` | 의도 | `secrets.md` 7번, `dev.md` E38 · N8 · V46(값의 일부도 내보내지 않는다). 등록되지 않은 키 모양은 못 가린다 → T18에서 판단 |
| B17 | 저장된 키를 다시 읽어 주는 길이 없다 | plugin 5-1 (`P:profiles.py:218-296`), `D:src/app/api/gateways/route.ts:50-63` | 등록 여부만 알린다. `secrets/src/store.rs:195-203` | 같음 | — |
| B18 | 8자 미만 값 거부, 가리기 대상 개수 상한 | 없음 | `secrets/src/lib.rs:83, 94` | 고유 | — |

## 3. 저장소 (`pi-store`)

| # | 동작 | 원본 | 우리 | 판정 | 고칠 것 |
|---|---|---|---|---|---|
| C1 | SQLite WAL + 외래 키 켜기 | hermes 7장. `H:hermes_state_wal.py:157-163`, `H:hermes_state.py:816`, `D:src/db/index.ts:555-556` | `store/src/schema.rs:133-140` | 같음 | — |
| C2 | 스키마 버전과 이전 | Hermes: 표 `schema_version` 한 행 + 선언과 대조해 컬럼 추가 + 순차 체인(`H:hermes_state_schema.py:732-762, 1068-1173`). DeskRPG: 열 때마다 멱등 보정(`D:src/db/index.ts:479-527`) | `user_version` + `application_id` + 버전별 순차 이전, 전부 되거나 전부 안 됨. `store/src/schema.rs:19, 158-204` | 의도 | `store.md` 1번 "스키마 버전을 두고 순서대로 올린다" |
| C3 | 더 새 버전의 파일을 열 때: 거부하지 않고 그대로 연다 | `H:hermes_state_schema.py:1172-1173` | 거부한다. `store/src/schema.rs:186-190` | **모름** | 거부가 더 안전하다. 고치지 말고 결정으로 적는다 |
| C4 | DB 파일(`-wal` · `-shm` 포함)을 0600으로 | `H:hermes_state.py:247-269, 812-828`, `O:src/state/openclaw-agent-db-permissions.ts:7-8` | 파일 권한을 걸지 않는다(보관 폴더 0700에 기댄다). `store/src/schema.rs:127-128`, `server/src/app.rs:22` | **모름** | 열 때 0600으로 만든다 |
| C5 | 이벤트 번호: 1부터 빠짐없이, 한 트랜잭션 안에서 매긴다 | hermes 6.5. `H:gateway/hosted_rooms.py:1206-1222` | `store/src/log.rs:18-20` | 같음 | 원본은 방마다, 우리는 전체에서 하나(C7) |
| C6 | 쌓기만 하는 기록의 강제 | 코드 규약 + 기본 키뿐. 방을 없앨 때는 지운다(`H:gateway/hosted_rooms.py:595-600`) | 트리거로 UPDATE · DELETE · 같은 번호 다시 넣기를 막는다. `store/src/schema.rs:81-84, 119-121` | 의도 | `store.md` 2차 규칙 4 |
| C7 | 이벤트 행의 칸: 방 id, 멱등 키(`event_id`, 같은 내용이면 원본 반환 · 다르면 충돌), 행위자, 권한 세대 | hermes 6.5. `H:gateway/hosted_rooms.py:102-114, 1201-1205` | `seq, kind, payload`뿐. `store/src/schema.rs:66-70` | 미구현 (T9 · T21) | 팀 논의 방 로그를 얹으려면 방 범위와 멱등 키가 필요하다. 지금 표에 칸을 더할지, 방 로그를 따로 둘지 T9 지시서에서 정한다 |
| C8 | 이벤트 시각을 칸으로 둔다(`created_at`) | `H:gateway/hosted_rooms.py:110`, `D:src/db/sqlite-chat-rooms.js:34` | 시각이 `payload` 안의 봉투(`{at, payload}`)에 들어 있다. `gate/src/frame.rs:17-22`, `gate/src/hub.rs:144` | **모름** | 시각으로 찾거나 지우려면(보존 · 복기) JSON을 풀어야 한다. `at` 칸을 더하는 이전(버전 3) |
| C9 | 이벤트 종류 이름 규칙: `영역.동작`, `^[a-z][a-z0-9_.-]*$`, 64자 | `H:gateway/hosted_rooms.py:27, 48-58`. api_server도 `run.started` · `message.delta` 모양(hermes 5.2) | 규칙도 검사도 없다. 지금 이름은 `feed_status` · `quotes_stale` 같은 밑줄 모양. `server/src/app.rs:25-57`, `server/src/setup.rs:109` | **모름** | 저장되는 이벤트가 생기기 전에(T9) 이름 규칙을 정하고 `append`에서 검사한다 |
| C10 | 읽기: 번호가 주어진 것보다 큰 것을 번호 순서로, 개수 상한 | `H:gateway/hosted_rooms.py:1393-1442` | `store/src/log.rs:26-49` | 같음 | — |
| C11 | 세션 · 메시지 · 전문 검색 · 사용량 표 | hermes 3.5 · 7.1 | 없음 | 미구현 (T9 · T16) | — |
| C12 | 로그 용량 상한(방당 50,000개 · 256MiB, 내용 256KiB)과 보존 | `H:gateway/hosted_rooms.py:38-44, 573-593`, hermes 7.1 보존 | 발행 한 건 1MiB 상한만 있다(`gate/src/lib.rs:105`). 총량 상한 · 정리 없음 | 미구현 (**작업 번호 없음**) | 24시간 돌면 끝없이 는다. T19(백업)나 T9에 넣는다 |
| C13 | 접속 기록 | 없음 | `store/src/log.rs:52-76` | 고유 | — |
| C14 | 투자 팀 상태 · 매매 상태표 | 없음 | `store/src/team.rs`, `store/src/trade.rs` | 고유 | — |

## 4. 접속 관문 · 실시간 연결 · 설정 (`pi-gate`, `price-inha`)

### 접속과 세션

| # | 동작 | 원본 | 우리 | 판정 | 고칠 것 |
|---|---|---|---|---|---|
| D1 | 기본은 `127.0.0.1`에만 연다 | hermes 5.2 (`H:gateway/platforms/api_server.py:154-155`), `H:hermes_cli/web_server.py:1425-1426`, `O:src/gateway/net.ts:284-289` | `gate/src/lib.rs:162, 184`, `server/src/settings.rs:54` | 같음 | DeskRPG 운영 서버는 `0.0.0.0`이다(`D:server.js:28`) |
| D2 | 루프백 밖으로 열 때는 인증이 있어야 시작한다 | `O:src/gateway/server-runtime-config.ts:113-129`, `H:hermes_cli/web_server_boundary.py:111-123` | 열 수는 있지만 루프백이 아닌 상대는 모두 401. `gate/src/guard.rs:127-136` | 미구현 (T17) | 지금은 원본보다 엄격하다 |
| D3 | `Host` 헤더 허용 목록(DNS rebinding 방지) | `H:hermes_cli/web_server_boundary.py:50-108` (불일치 400). api_server에는 없다 | `gate/src/guard.rs:87-116` (불일치 403) | 같음 | 상태 코드만 다르다 |
| D4 | 다른 출처의 요청 거부 | `H:gateway/platforms/api_server.py:645-661` (Origin이 있고 목록에 없으면 403), `O:src/gateway/origin-check.ts:127-173` | 상태를 바꾸는 요청과 웹소켓은 같은 출처여야 한다. `gate/src/guard.rs:140-170` | 같음 | Origin이 없는 POST도 거부해 원본보다 엄격하다 |
| D5 | 로컬에서 들어오는 수단 | Hermes api_server: 루프백이어도 Bearer 키 필수(16자 이상). 대시보드: 프로세스 토큰을 HTML에 넣어 준다(`H:hermes_cli/web_server_dashboard.py:186-221`). DeskRPG: ID · 비밀번호 | 루프백이면 키 없이 `POST /api/session`으로 세션. `gate/src/guard.rs:201-228` | 의도 | `dev.md` D2 · C15 |
| D6 | 세션 쿠키 속성 | `token; HttpOnly; SameSite=Lax; Max-Age 7일; Path=/` (`D:src/app/api/auth/login/route.ts:47-53`) | `pi_session; HttpOnly; SameSite=Strict; Path=/`, `Max-Age` 없음(브라우저를 닫으면 사라진다), 서버 수명 7일. `gate/src/guard.rs:226`, `gate/src/session.rs:16` | 의도 | `gate.md` 4번. 폰에서 오래 붙어 있어야 하는 T17 · T22에서 `Max-Age`를 다시 본다 |
| D7 | 세션을 어디에 두나 | DeskRPG: 서버에 없음(JWT, 로그아웃해도 만료까지 유효. `D:src/lib/jwt.ts:34-41`) | 서버 메모리에 해시만. 다시 켜면 세션이 사라진다. `gate/src/session.rs:28-33` | 의도 | `gate.md` 4번, `dev.md` M6("접속 키를 바꾸면 세션이 끊긴다") |
| D8 | 세션 값: 256비트 OS 난수 | `H:hermes_cli/web_server_session_token.py:16-38` | `gate/src/session.rs:46-48` | 같음 | — |
| D9 | 웹소켓도 쿠키 세션 · Host · Origin · 상대 주소를 본다 | `D:src/server/socket-handlers.ts:1027-1069, 1305-1310`, `H:hermes_cli/web_server_boundary.py:251-285` | `gate/src/lib.rs:353-356, 366` | 같음 | — |
| D21 | 보안 응답 헤더(CSP, `X-Content-Type-Options: nosniff`, `X-Frame-Options: DENY`, `Referrer-Policy`) | `H:gateway/platforms/api_server.py:634-641, 682-687` | 없음(`gate/src`, `server/src`에서 찾지 못함) | **모름** | 관문의 바깥 층에 응답 헤더를 더한다. 화면이 한 장이라 CSP를 좁게 걸 수 있다 |
| D22 | 인증 실패 횟수 제한(10회 / 60초, 잠금 300초) | `O:src/gateway/auth-rate-limit.ts:99-105` | 없음 | 미구현 (T17) | — |

### 실시간 연결과 이벤트

| # | 동작 | 원본 | 우리 | 판정 | 고칠 것 |
|---|---|---|---|---|---|
| D10 | 전송 방식 | DeskRPG Socket.IO(deskrpg 3-2), Hermes SSE(hermes 5.2) | 웹소켓 하나 + REST | 의도 | `dev.md` D11 |
| D11 | 프레임 모양 | `{"event", "run_id", "timestamp"(초), …, "seq"}` (`H:gateway/platforms/api_server_runs.py:224-226, 1294-1297`) | `{"seq", "kind", "payload", "at"(밀리초)}`. `gate/src/frame.rs:59-76` | 의도 | `dev.md` D11, `gate.md` 6번 |
| D12 | 번호는 서버 한 곳이 빠짐없이 매긴다 | `H:…api_server_runs.py:175-185` (run마다 0부터, 메모리) | 전체에서 1부터, 저장소가 매기고 다시 켜도 이어진다. `gate/src/hub.rs:140-159`, `store/src/log.rs:18-20` | 같음 | 원본 SSE는 다시 켜면 재생이 안 된다. 우리는 된다(V6) |
| D13 | 다시 잇기: 마지막 번호보다 큰 것만 순서대로 재생하고 이어서 새 것 | `H:…api_server_runs.py:198, 1277-1281` (`Last-Event-ID` 또는 `?last_seq=`) | `?last_seq=`. `gate/src/ws.rs:30-44, 178-211`, `gate/src/hub.rs:218-229` | 같음 | 숫자가 아닌 값: 원본은 처음부터 재생, 우리는 400 |
| D14 | 마지막 번호를 안 주면 과거를 재생하지 않고 지금부터 | 칸반 웹소켓(`H:plugins/kanban/dashboard/plugin_api.py:1703-1717`), 플러그인 이벤트(plugin 3-4, `P:events.py:984-989`) | `gate/src/hub.rs:215-216` | 같음 | `last_seq=0`이면 전체 재생인 것도 같다 |
| D15 | 재생할 수 없는 구간 알림(`replay.truncated`, 버퍼 1000개) | `H:…api_server_runs.py:167, 1308-1316` | 전부 저장소에 있어 잘리는 일이 없다 | 의도 | `gate.md` 6번 "번호가 빠지거나 겹치지 않는다". C12(보존)를 넣으면 이 알림도 같이 필요하다 |
| D16 | 느린 연결: 줄 256개가 차면 그 연결만 끊고 알리지 않는다 | `H:…api_server_runs.py:161, 186-193` | `gate/src/lib.rs:103`, `gate/src/hub.rs:185-191` | 같음 | 쓰기 제한은 원본 5초, 우리 30초(`gate/src/ws.rs:25`) |
| D17 | 조용할 때 살아 있음 신호: 10초마다 keepalive, 처음 `: open`, 끝 `: stream closed` | `H:…api_server_runs.py:1307, 1327-1336` | 서버가 보내는 ping · keepalive가 없다. `gate/src/ws.rs:159-174` | **모름** | 시세가 안 흐를 때(시작 전 · 장 닫힘) 죽은 연결을 알아채지 못하고, 중간 장비가 조용한 연결을 끊을 수 있다. 주기 ping을 넣는다 |
| D18 | 연결하면 지금 상태를 먼저 주고 이벤트를 잇는다 | `D:src/server/socket-handlers.ts:1555-1583` (`players:state`, `npc:working`), 다시 열 때 `room:response-snapshot` (deskrpg 4-6) | 종류마다 가장 최근의 "지나가는 이벤트"를 먼저 보낸다. `gate/src/ws.rs:148-158`, `gate/src/hub.rs:243-248` | 같음 | — |
| D19 | 이벤트 종류 | Hermes 18개(`run.*`, `message.delta`, `tool.*`, `approval.*`), DeskRPG 약 60개, 플러그인 15개 | 5개(`quotes`, `feed_status`, `quotes_stale`, `setup`, `symbols_rejected`) 모두 우리 고유이고 **모두 번호 없는 지나가는 이벤트**다. 저장되는 이벤트를 내는 곳이 아직 없다(`Publisher::publish`를 부르는 곳이 `gate/src/lib.rs:433-438` 정의뿐) | 미구현 (T9 · T14 · T20 · T21) | 번호 이어 받기는 테스트로만 확인된 상태다. 첫 저장 이벤트를 넣는 작업에서 실제 흐름으로 다시 본다 |
| D20 | 응답 수명주기 객체(전체 객체를 보내고, `updatedAt`이 단조 증가하고, 끝난 뒤 안 바뀜) | deskrpg 4-6. `D:src/server/chat-response-tracker.ts:22, 40, 44` | 없음 | 미구현 (T21) | — |
| D27 | 화면: 끊기면 간격을 늘려 다시 잇고 마지막 상태를 보여 준다 | `D:src/app/game/GamePageClient.tsx:689-698, 725-740` (0.5~3초) | 1초부터 두 배, 상한 10초, `last_seq`로 잇는다. `server/web/index.html:140-141, 407-434` | 같음 | — |
| D28 | 연결 64개 상한, 화면이 보내는 프레임 4KiB 상한 | 없음(DeskRPG는 20MB) | `gate/src/ws.rs:21-23` | 고유 | — |

### 설정과 처음 설정

| # | 동작 | 원본 | 우리 | 판정 | 고칠 것 |
|---|---|---|---|---|---|
| D23 | 동작 설정이 들어오는 길 | 설정 파일(`config.yaml`)이 정본, `.env`는 비밀만(hermes 3.8). 대시보드는 인자 `--host` · `--port`(`H:hermes_cli/subcommands/dashboard.py:18-19`) | 인자 > 환경 변수 > 기본값. 설정 파일 없음. `server/src/settings.rs:51-66, 107-116` | 의도 | `gate.md` "price-inha가 만드는 것" 1번 |
| D24 | 설정 파일 체계(프로필 = 폴더, 모델 · 재시도 · 도구셋, 파일 서명 캐시, 원자 쓰기) | hermes 3.8, deskrpg 5-3 | 종목만 `watchlist.json`에 둔다. `server/src/setup.rs:116, 331-357` | 미구현 (T8 · T9) | 모델 이름을 둘 자리가 아직 없다(`ai/src/anthropic.rs:24`). T8 지시서에서 설정 파일의 모양을 정한다 |
| D25 | 프로세스 환경 변수의 키도 읽는다(`.env`를 환경으로 올린다) | `H:hermes_cli/env_loader.py:671-678`, hermes 4.2 표 1 · 2번 | 키 파일만 읽는다. 환경 변수 · 인자의 키는 받지 않는다. `server/src/settings.rs:66`, `server/src/keys.rs:79-81` | 의도 | `gate.md` "비밀 값은 인자로 받지 않는다" |
| D26 | 처음 실행(키가 없을 때) | Hermes 게이트웨이는 그냥 시작한다(`H:gateway/run_startup.py:1554-1557`). CLI는 설정으로 보낸다(`H:hermes_cli/main.py:1555-1585`). DeskRPG는 사용자가 0명이면 가입 화면(`D:src/app/api/auth/status/route.ts:5-8`) | 필수 항목이 다 등록될 때까지 시작하지 않고 설정 화면만 보여 준다. `server/src/feed.rs:170-176`, `server/src/setup.rs:130` | 의도 | `gate.md` 3차(2026-11-14 사용자 결정) |
| D29 | 키 등록 시도 횟수 제한(5회 / 60초) | 없음 | `server/src/settings.rs:45-47` | 고유 | — |
| D30 | 시세 상태 알림 · 늦음 판단 · 물러서기 | 없음 | `server/src/feed.rs`, `server/src/retry.rs` | 고유 | — |

## 5. 시세 (`pi-market`) · 모의계좌 (`pi-paper`)

| # | 동작 | 원본 | 우리 | 판정 |
|---|---|---|---|---|
| E1 | 토스증권 시세 · 장 시간 | 원본에 없다 | `crates/market/` | 고유 |
| F1 | 모의계좌 체결 | 원본에 없다 | `crates/paper/` | 고유 |

---

## 고칠 것 모음

### 모르고 다름 (19개, 심각한 순서)

| 순위 | # | 무엇이 다른가 | 고치는 범위 | 기계로 잡는 테스트 |
|---|---|---|---|---|
| 1 | A3 · A4 | OAuth 토큰일 때 원본이 보내는 beta 헤더 · user-agent · `x-app`과 시스템 글 앞 문장이 없다. OAuth 자리가 실제로는 안 될 가능성이 크다 | 사용자 결정(약관) → `pi-ai` 요청 조립 | 가짜 서버가 OAuth 요청의 `anthropic-beta`에 두 값이 있는지 확인 |
| 2 | B11 | `auth.json`에 만료 시각 · 공급자 · 버전이 없다 | `pi-secrets` 파일 모양(등록된 파일이 생기기 전에) | 쓴 `auth.json`에 `version`과 `expires_at` 칸이 있는지 |
| 3 | B12 | refresh 토큰이 없는 토큰을 등록할 수 없다 | `pi-secrets` 칸 정의 | `refresh_token` 없이 등록이 되는지 |
| 4 | A8 | 둘 다 있으면 API 키가 먼저(원본은 OAuth가 먼저) | 결정 기록, 필요하면 `keys.rs` | 둘 다 등록한 뒤 어느 헤더로 나가는지 |
| 5 | A7 | 토큰 종류를 접두사로 보지 않는다 | `price-inha` 등록 길 | API 키 칸에 `sk-ant-oat…` 모양을 넣으면 거부되거나 OAuth로 가는지 |
| 6 | B6 | `.env` 줄 이름에 공급자가 없다 | `pi-secrets` 이름표(+ 옛 이름 읽기) | 쓴 `.env`의 줄 이름 |
| 7 | D17 | 실시간 연결에 ping · keepalive가 없다 | `pi-gate` 웹소켓 | 조용한 연결에서 정한 시간 안에 ping 프레임이 오는지 |
| 8 | A18 | `Retry-After`를 초만, 429에만, 30초까지만 쓴다 | `pi-ai` 다시 시도 | 날짜 모양 값, 5xx + `Retry-After` |
| 9 | A15 | 시간 초과 · 연결 실패를 다시 시도하지 않는다 | `pi-ai` 다시 시도 또는 결정 기록 | 가짜 서버가 첫 연결을 끊었을 때 두 번째 요청이 오는지 |
| 10 | A17 | 간격이 1초부터, jitter 없음 | `pi-ai` 다시 시도 | 첫 간격이 2초 이상 3초 이하인지 |
| 11 | C9 | 이벤트 종류 이름 규칙이 없다 | `pi-store` 검사 + 이름 정하기 | 규칙에 안 맞는 이름의 `append` 거부 |
| 12 | C8 | 이벤트 시각이 칸이 아니라 내용 안에 있다 | `pi-store` 이전(버전 3) + `pi-gate` | 버전 2 파일을 열면 `at` 칸이 채워지는지 |
| 13 | D21 | 보안 응답 헤더가 없다 | `pi-gate` 바깥 층 | 모든 응답에 `nosniff` · `frame-ancestors`가 있는지 |
| 14 | C4 | DB 파일 권한을 걸지 않는다 | `pi-store` 열기 | 새 DB 파일의 모드가 0600인지(Unix) |
| 15 | B14 | 깨진 `auth.json`을 사본 없이 덮어쓴다 | `pi-secrets` | 깨진 파일 위에 등록한 뒤 `.corrupt` 사본이 있는지 |
| 16 | A19 | 연결 시간 제한이 따로 없다 | `pi-ai` 설정 | 닿지 않는 주소에서 10초 안에 끝나는지 |
| 17 | B5 | `.env` 이스케이프 세부가 원본과 다르다 | 결정 기록(호환이 필요 없으면 그대로) | — |
| 18 | C3 | 새 버전 파일을 거부한다(원본은 연다) | 결정 기록(고치지 않음) | 이미 있음 |

(A3 · A4를 한 줄로 묶어 18줄이다.)

### 아직 안 만듦 (15개)

| 중요도 | # | 무엇 | 받을 작업 |
|---|---|---|---|
| 높음 | A9 | OAuth 로그인 · 토큰 갱신 | **없음** — 작업 계획에 넣어야 한다 |
| 높음 | D19 | 저장되는(번호 있는) 이벤트를 내는 곳 | T9 · T14 · T20 · T21 |
| 높음 | C7 | 이벤트 행의 방 · 멱등 키 · 행위자 칸 | T9 · T21 |
| 높음 | D24 | 설정 파일 체계(모델 이름 자리 포함) | T8 · T9 |
| 중간 | C12 | 이벤트 로그 용량 상한 · 보존 | **없음** |
| 중간 | A12 | 자격 증명 풀 · 쿨다운 | **없음** |
| 중간 | A14 | 세부 오류 구분(과금 · 컨텍스트 초과 · 없는 모델) | T9 |
| 중간 | A10 | 401 때 자격을 다시 읽어 한 번 더 | T9 |
| 중간 | A20 · A21 | 스트리밍, 폴백 · 자동 복구 | T9 |
| 중간 | C11 | 세션 · 메시지 · 검색 표 | T9 · T16 |
| 중간 | D2 · D22 | 루프백 밖 접속의 인증, 실패 횟수 제한 | T17 |
| 낮음 | A5 | API 키 경로의 beta 헤더 | T9 |
| 낮음 | D20 | 응답 수명주기 객체 | T21 |

---

## 다음 모듈의 지시서가 가리켜야 할 절

지금까지의 지시서 여섯 개 가운데 조사 문서를 가리키는 것은 `paper.md` 하나(투자 원칙 문서)뿐이다. 아래 작업의 지시서는 "근거" 줄에 이 절들을 적어야 한다.

| 작업 | 꼭 가리킬 절 | 이 대조에서 나온 선행 조건 |
|---|---|---|
| T8 템플릿 | `hermes-agent.md` 3.8(파일 위치 · 프로필 배포 = 템플릿 · 프로필별 모델), `deskrpg.md` 5-3, `deskrpg-hermes-plugin.md` 4-1 · 4-2 · 4-5 | D24 |
| T9 에이전트 실행기 | `hermes-agent.md` 3.1 · 3.2(불변식 · 재시도 표) · 3.3 · 3.4 · 3.5 · 3.6 · 4.1 · 4.3 · 6.3 · 6.4 · 6.5 · 7.1 · 9.1, `deskrpg.md` 4-5(지시문 세 층) · 6(이어 말하기 예산), `openclaw.md` 2.4(순서 · 쿨다운) · 4의 9번 | A3 · A4 · A9 · A12 · A14 · A20 · A21 · C7 · C9 · C11 · D19 |
| T11 트리거 | `hermes-agent.md` 6.1(tick 순서와 보장 · wake gate) · 7.3, `openclaw.md` 2.1 전체 · 3의 트리거 줄 · 4의 1~4번 | C12 |
| T14 승인 | `deskrpg.md` 4-7 · 3-2 승인 이벤트 표, `openclaw.md` 2.2 · 3의 승인 줄 · 4의 5~6번, `hermes-agent.md` 5.2(`/v1/runs/{id}/approval`, `approval.request`), `deskrpg-hermes-plugin.md` 4-1 무인 승인 정책 · 3-4 `approval.blocked` | D19 |
| T15 복기 | `hermes-agent.md` 3.6(메모리) · 3.4, `openclaw.md` 2.3(압축 전 기억 저장) | C8 |
| T16 지식 검색 | `openclaw.md` 2.3 · 3의 기억 줄 · 4의 7번, `hermes-agent.md` 3.5(검색 묶음) · 7.1(`messages_fts`) | C11 |
| T17 접속 키 | `remote-access-security.md` 전체, `hermes-agent.md` 5.2(키 16자 · 비교 방식) · 5.4, `deskrpg.md` 7-1(세션 · 쿠키) · 9-1 인증 줄, `deskrpg-hermes-plugin.md` 5-1 | D2 · D6 · D21 · D22 |
| T18 가리기 검사 | `deskrpg-hermes-plugin.md` 5-1(값을 응답 · 로그 · 예외에 싣지 않는 방법) · 5-3, 이 문서 B16 | — |
| T19 백업 | `hermes-agent.md` 7.1 보존 · 7.5 파일 저장, `deskrpg.md` 5-1 · 5-2 | C4 · C12 |
| T20 3D 사무실 | `deskrpg.md` 3-2 입장 · 이동 · NPC 이동 표, 6(NPC 이동 권한), 7 전체, 9-2 · 9-4(라이선스) | D19 |
| T21 채팅 | `deskrpg.md` 3-2 DM · 방 채팅 표, 4-6(수명주기 상태 기계 · 큐 · 타임아웃) · 6(멘션 · 이어 말하기) · 5-1 채팅 표, `hermes-agent.md` 5.3(스트리밍 네 규칙) · 6.5 | C7 · D19 · D20 |
| T22 폰 화면 | `deskrpg.md` 7-1 표와 그 아래 추가 항목 · 3-1 주의 수신함, `remote-access-security.md` | D6 · D17 |

### 더 뜯어야 할 것 (조사 문서가 얇거나 빠진 곳)

| 작업 | 무엇 | 왜 |
|---|---|---|
| T9 | Hermes 컨텍스트 압축의 요약 구조와 경계 고르기(`agent/context_compressor_summary.py`) | `hermes-agent.md` 못 본 부분 3. "같은 동작"으로 만들 수 없다 |
| T9 | Hermes 오류 분류기의 문구 표(`agent/error_classifier.py`의 과금 · 한도 · 400 꼬리 규칙) | A14를 만들 때 필요 |
| T9 | hosted room 계획기의 라우팅 세부(everyone · lead · auto)와 `message_agent` 도구 스키마 | 못 본 부분 5 · 6 |
| T9 | OAuth 경로에서 응답의 도구 이름을 되돌리는 곳(`normalize_response`) | A4를 넣으면 짝으로 필요 |
| T9 | 자격 풀의 고르기 · 복구(`credential_pool.select`, `_recover_auth_failure`) | A12 |
| T9 | Anthropic 스트리밍 이벤트 모양과 API 버전 `2026-10-21`에서 OAuth 토큰이 받는 처리 | 원본이 아니라 공식 문서로 확인할 것 |
| T11 | cron 작업 파일을 매 tick 다시 읽는지, OpenClaw의 이중 발사 방지 | hermes 못 본 부분 7, openclaw 5.3의 1 · 2 |
| T14 | Hermes 승인 흐름(위험 판정 규칙, `approvals.timeout`), OpenClaw 승인이 다시 켠 뒤 복구되는 방법 | hermes 못 본 부분 4, openclaw 5.3의 5 |
| T16 | 기억 검색 가중치 기본값, Hermes의 한글 검색(`hermes_state_search`) | openclaw 5.3의 6 |
| T17 | Hermes 대시보드 인증 게이트 모드의 쿠키 · 로그인 실패 제한 | 이번에 확인하지 못함 |
| T19 | Hermes 백업(`hermes_cli/backup*.py`) | 조사 문서에 "나중에" 한 줄뿐 |
| T20 | `GamePageClient.tsx` · `office-simulation.ts`의 화면 상태 관리 | deskrpg 부록 B |
| T21 | 다시 이었을 때 60줄 넘게 놓친 방 메시지를 메우는 길 | 원본에서 찾지 못함. 우리는 번호로 메운다 |
| T22 | 웹 푸시 | DeskRPG에는 없다(deskrpg 7-1). OpenClaw에는 승인용 웹 푸시가 있다(조용한 시간 · 종류별 설정 포함 `O:src/gateway/approval-web-push.ts:1-20`) (바로잡음 2026-11-14: triggers-approval-memory-deep.md 바로잡을 곳) ~~뜯을 원본이 없으니 표준 문서로 만든다~~ |

### 조사 문서에서 바로잡을 곳 (이번에 소스로 확인)

| 문서 | 적힌 것 | 소스 |
|---|---|---|
| `hermes-agent.md` 3.2 | 간격 `min(5 × 2^(n-1), 120)` | 실제 호출 값은 첫 간격 2초, 상한 60초 (`H:agent/turn_recovery.py:1489`) |
| `hermes-agent.md` 3.2 | `api_max_retries` 기본 3 | 다시 시도 횟수가 아니라 **시도 총 횟수**다 (`H:agent/conversation_loop.py:1583`) |
| `hermes-agent.md` 5.2 | `last_seq` 이후 재생 | 번호는 0부터, 기본 `last_seq`는 -1. 버퍼는 1000개이고 만든 지 300초가 지나고 듣는 쪽이 없으면 지운다 (`H:gateway/platforms/api_server_runs.py:167, 175, 1279, 1563-1570`) |
| `hermes-agent.md` 3.8 | `.env`는 0600 | 새 파일일 때만. 있던 파일은 좁히지 않는다 (`H:hermes_cli/config.py:2435-2459`) |
| `deskrpg.md` 4-2 | 게이트웨이 등록 → 테스트 | 등록은 확인 없이 저장하고, 실패한 키도 남는다 (`D:src/app/api/gateways/route.ts:28-48`) |
| `openclaw.md` 2.4 | 자격 저장 | 파일이 아니라 에이전트별 SQLite다 (`O:src/agents/auth-profiles/paths.ts:52, 69`) |

2026-11-14 앞 문서에 반영함 (못 한 것: `openclaw.md` 2.4 자격 저장. 그 절은 이미 "공유 DB + 에이전트 DB"로 적혀 있어 "파일"이라고 적은 문장이 없고, 표의 "에이전트별 SQLite"와 어떻게 다른지 판단을 더하지 않아 고치지 않았다. `hermes-agent.md` 3.8 `.env` 0600은 그 문서에 해당 문장이 없어, 같은 내용이 있는 `remote-access-security.md` 7.1 "비밀 저장" 칸에 반영했다). 아래 "더 뜯어야 할 것"의 T22 줄은 트리거 조사(`triggers-approval-memory-deep.md`)에 따라 판정을 바로잡았다.

---

## REA를 쓴 범위와 직접 읽은 범위

| 구분 | 범위 |
|---|---|
| REA (`rea analyze-javascript-application`, v6.7.0) | DeskRPG 인증 파일 3개(`src/server/socket-handlers.ts`, `src/lib/jwt.ts`, `src/app/api/auth/login/route.ts`)만 빈 폴더에 복사해 돌렸다. 파싱 실패 0. 얻은 것: import 관계(`jose`의 `jwtVerify`), 호출 관계(`socket-handlers.ts:1306 → authenticateSocket`, `:1052 → getJwtSecret`), `cookies.set` 위치(`login/route.ts:47`). `@/…` 별칭은 풀리지 않았다. 출력은 `…/scratchpad/conformance/rea-deskrpg-auth.json` |
| 직접 읽음 (grep + 파일 읽기) | 위를 뺀 전부. Hermes(Python)는 REA가 호출 관계를 못 내므로 전부 직접 읽었다. 쿠키 속성 값, 소켓 인증 실패 때의 동작, OpenClaw 헤더, 플러그인 이벤트 커서도 직접 읽은 것이다 |
| 우리 코드 | `crates/{ai,secrets,store,gate,server}/src`의 테스트가 아닌 부분을 읽었다. `market` · `paper`는 원본이 없어 공개 모양만 봤다 |

원본 확인은 보조 에이전트 셋이 나눠 읽고 `경로:줄`로 보고한 것을 받아 적었다. 이 문서의 원본 줄 번호는 그 보고에서 온 것이고, 우리 쪽 줄 번호는 직접 읽은 것이다.

## 확인하지 못한 것

1. OAuth 토큰에 헤더(A3)가 없을 때 실제로 거절되는지. 근거는 원본 소스의 주석뿐이고, 실제 요청을 보내 보지 않았다.
2. Hermes가 쓰는 SDK가 붙이는 `anthropic-version` 값과 기본 user-agent(SDK 소스가 원본 트리에 없다).
3. API 버전 `2026-10-21`에서 OAuth 토큰과 beta 헤더가 어떻게 다뤄지는지(공식 문서의 인증 절에는 API 키와 WIF만 있었다).
4. Hermes 자격 풀에서 API 키 항목이 실제 호출에 뽑히는 길, `ProviderNotConfiguredError`가 나는 조건.
5. OpenClaw에 독립된 `Host` 헤더 검사가 있는지, `auth_profile_store` 행의 정확한 모양.
6. DeskRPG에서 이어진 뒤 JWT가 만료된 소켓을 끊는지, 60줄 넘는 누락을 메우는 길.
7. Hermes 대시보드 인증 게이트 모드의 쿠키 속성과 로그인 실패 제한, `hermes setup`의 필수 단계.
8. 우리 테스트를 돌려 보지 않았다. 이 문서의 "우리" 칸은 코드를 읽은 결과다.
9. `pi-market` · `pi-paper` 안쪽은 대조하지 않았다(원본 없음).

## 2026-11-14 뒤에 정한 "의도해서 다름"

| 동작 | 원본 | 우리 | 까닭 |
|---|---|---|---|
| 저장소 파일이 아닌 파일을 열려 할 때의 권한 | Hermes는 내용을 보기 전에 0600으로 좁힌다(`hermes_state.py:247-300`) | 거부하는 파일은 권한도 바꾸지 않는다(store 4차 테스트) | 저장소의 약속 "거부된 호출은 아무것도 바꾸지 않는다"가 먼저다. 남의 파일을 건드리지 않는 쪽이 더 안전하다 |
| 옛 기록 형식 풀기 | 원본에 해당 없음(우리 형식) | 저장소가 남긴 경계 번호 이하에서만 푼다 | 사용자 결정 "경계 번호로"(`docs/dev/decisions.md`) |

## 2026-11-15에 정한 의도해서 다름: 구독은 CLI 백엔드로

사용자가 AI 연결을 "API 키 + CLI 백엔드"로 정했다(`docs/dev/decisions.md`). 그래서 1장의 A2 · A3 · A4 · A7 · A8 · A9(구독 OAuth 토큰을 API에 직접 보내는 길과 그 헤더 · 본문 · 판별 · 순서 · 갱신)는 **의도해서 다름(안 가져옴)**이다. 구독은 OpenClaw `claude-cli`의 도구 없는 한 번 호출을 따른다(`docs/research/cli-backend.md` 2장, `docs/dev/modules/ai.md` 2차). 그 안에서 원본과 다른 것: 오래 사는 프로세스 · 세션 이어 가기 · MCP 도구 넘기기 · `--include-partial-messages`(스트리밍) · 무출력 감시 · 대화별 줄 대신 전체 동시 실행 수(`cli-backend.md` 5.1의 까닭).

