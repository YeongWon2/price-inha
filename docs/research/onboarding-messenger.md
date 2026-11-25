# 설치 · 세팅 · 로그인 · 메신저 게이트 조사와 차이 분석 (2026-11-26)

이 문서는 조사와 제안이다. 저장소는 건드리지 않았다.

표기:
- [관찰]: 소스나 문서에서 직접 본 것.
- [에이전트 관찰]: 읽기 전용 하위 조사가 경로와 줄을 달아 보고한 것. 일부는 다시 열어 확인했다(아래 "확인함" 표시).
- [추정]: 해석이나 판단.
- 못 본 것은 "못 봄"이라고 적었다.

원본 경로의 접두는 다음과 같다.
- `O:` = OpenClaw `scratchpad/refs/src4/openclaw` (commit `873e50b5`, 2026-11-25)
- `H:` = Hermes Agent `scratchpad/refs/src4/hermes-agent` (commit `a1435125`, 2026-11-25)
- 우리 경로는 저장소 루트 기준이다.

---

## 0. 요약

사용자가 바라는 흐름은 다섯 단계다.
1. 한 줄로 설치한다.
2. 터미널 마법사로 세팅한다.
3. AI 로그인(OAuth 또는 CLI)을 한다.
4. 로컬 웹(대시보드와 RPG)을 연다.
5. 메신저(텔레그램 · 슬랙)를 등록하고 게이트로 쓴다.

원본 둘(OpenClaw · Hermes)은 이 다섯 단계를 모두 갖고 있다.

우리 쪽 상태는 이렇다.
- **3 · 4 단계는 상당 부분 있다**:
  - AI 연결은 API 키와 Claude Code CLI 둘 다 된다.
  - 대시보드와 RPG 사무실, 접속 키, 웹 푸시, 승인 카드가 있다.
- **1 · 2 · 5 단계는 없다.**
- **기존 문서 두 곳이 이 요구와 맞지 않는다**:
  - PRD "범위 밖"에 `텔레그램 같은 메신저 연동은 하지 않는다`(`docs/product/stock-team/prd.md:414`)가 있다.
  - 설치 조사의 결정에 "`setup` · `config` 명령을 두지 않는다"(`docs/research/install-and-cli.md:394, 396, 434`)가 있다.
- 따라서 **스펙 변경이며 사용자 결정이 필요하다**(AGENTS.md 9번).

---

## 1. 이미 있는 조사에서 덮인 것과 빈 것

| 문서 | 이미 조사됨 | 비어 있거나 이번 요구와 어긋남 |
|---|---|---|
| `docs/research/install-and-cli.md` | 두 원본의 한 줄 설치 단계, 서비스(launchd · systemd · 예약 작업), `doctor`, 업데이트 · 제거, 우리 명령 구성 제안(8.1) | 마법사를 **"줄임: 터미널에서 묻지 않고 주소만 연다"**(:394)로 정했고 `setup`을 넣지 않았다(:434). 온보딩 단계 순서는 "문서로만" 봤다(:654). 메신저 단계는 없다 |
| `docs/research/openclaw.md` | 2.4 구독 OAuth: token sink, Codex PKCE(1455) · device code, 프로필 순서와 쿨다운. 승인 hook 계약 | 3장에서 **"Claude CLI 재사용 · 개인 계정: 안 가져옴"**(:318). 채널은 다루지 않았다 |
| `docs/research/hermes-agent.md` | 4장 공급자 · Anthropic 토큰 순서 · Claude Code 자격 빌려 쓰기. 5.2 api_server. 5.4 대시보드 토큰 | **5.5 "메신저 어댑터 안 가져옴"**(:585-587). 페어링과 허용 목록은 다루지 않았다 |
| `docs/research/cli-backend.md` | `claude -p` 호출, `auth status`, `setup-token`, 4.5 약관 문장 원문, 5.9 사용자 결정 12개 | Codex CLI 재사용은 없다(OpenClaw에 `codex-cli` 백엔드가 없다는 사실만 있다, :47) |
| `docs/research/remote-access-security.md` | 위협 모델, 승인 한 건의 흐름(5.1), 리플레이와 바꿔치기 방지, 비대칭(5.3) | Tailscale · TOTP 설계는 **dev.md D2에서 버렸다**(`dev.md:16`). 메신저 경로에서 받는 승인은 다루지 않았다 |
| `agent-runtime-deep.md` · `triggers-approval-memory-deep.md` | 승인 상태 기계, 지문 바인딩(OpenClaw `system-run-approval-binding`) | 채널 승인 렌더링과 승인자 검증은 없다 |

**이번에 새로 조사한 것**: 2장 전체. 특히 2.4~2.6(채널 · 페어링 · 메신저 승인)은 처음이다.

---

## 2. 원본 조사 (근거 포함)

### 2.0 방법

1. 두 원본을 `scratchpad/refs/src4/` 아래에 `git clone --depth 1 --filter=blob:limit=512k`로 받았다. 읽기만 했고 실행하지 않았다.
2. rea는 앞선 조사와 같은 방식으로 썼다. 고른 파일만 빈 폴더(`scratchpad/refs/rea-msg/in`)에 복사한 뒤 다음을 돌렸다:
   `rea analyze-javascript-application . --artifact-format directory --max-heap-mb 1024 --format json`
   - 대상 9개 파일: `src/pairing/{pairing-store-model,pairing-store,command-owner}.ts`, `extensions/telegram/src/{approval-callback-data,bot-handlers.callback-router-controls,token}.ts`, `extensions/slack/src/approval-actions.ts`, `src/auto-reply/reply/commands-approve.ts`, `src/gateway/approval-channel-custody.ts`
   - 결과: 6초, 최대 RSS 약 305MB, 성공.
   - 뽑힌 식별자(이 문서 근거로 씀): `approveChannelPairingCode`, `upsertChannelPairingRequest`, `bootstrapCommandOwnerFromPairing`, `buildTelegramApprovalCallbackData`, `parseTelegramApprovalCallbackData`, `resolveAuthorizedApprovalKinds`, `resolveCanonicalApproval`, `terminalizeApprovalMessage`, `encodeSlackApprovalAction`, `decodeSlackApprovalAction`, `parseApproveCommand`, `resolveApprovalCommandAuthorization`, `prepareApprovalChannelCustody`, `resolveTelegramToken`, `readTelegramTokenFile`, `senderIsOwner`
3. 넓게 읽는 일은 읽기 전용 하위 조사 둘에 나눴다(OpenClaw 채널, Hermes 설치 · 로그인 · 메신저). 핵심 숫자는 다시 열어 확인했다(아래 "확인함").
4. **용량 한도를 넘겼다.** 받은 두 저장소가 합쳐 약 1.5GB다(OpenClaw 1.1GB, Hermes 440MB, 대부분 `.git`과 `ui/`). 지시는 500MB 안이었다. 줄이려고 지우기를 시도했지만 권한에서 막혔다. `scratchpad/refs/src4`와 `scratchpad/refs/rea-msg`는 사용자가 지워도 된다.

### 2.1 설치와 첫 세팅

**OpenClaw**
- 한 줄 설치 [관찰]:
  - macOS · Linux: `curl -fsSL https://openclaw.ai/install.sh | bash`
  - Windows: `iwr -useb https://openclaw.ai/install.ps1 | iex`
  - 근거: `O:README.md:30, 35`. 설치 단계는 앞선 조사 `install-and-cli.md` 3.1에 있다.
  - 마지막 단계는 터미널이 있으면 `exec openclaw onboard`다(`scripts/install.sh:4056-4080`, 앞선 조사).
- `openclaw onboard`의 Quick start [관찰, `O:docs/start/wizard.md:103-150`]:
  1. 보안 안내 한 줄을 보여 준 뒤 Quick start와 Custom 중에서 고른다.
  2. 이미 있는 모델, API 키 환경 변수, **로컬 AI CLI**, 닿는 Ollama · LM Studio를 탐지한다. 읽기만 하고 아무것도 받지 않는다.
  3. 하나를 고른다. **고른 것만 실제 완성 호출 한 번으로 검증한다.** 실패하면 재시도 · 다른 것 · 건너뛰기 중에서 고른다.
  4. "More…"에서 공급자별 browser · device · API-key · token 방식을 고른다.
  5. **"Keep OpenClaw running in the background?" (기본 예)**: 예면 서비스를 설치하고, 아니오면 포그라운드로 돈다.
  6. 검증된 경로를 저장하고, 작업 폴더를 준비하고, 게이트웨이 설정을 쓴다.
  7. **브라우저 대시보드를 연다.**
- Classic(Custom) 단계 [관찰, `wizard.md:257-311`]: Workspace → Model/Auth → Gateway(포트 · bind · 비밀 저장 · Tailscale) → **Channels** → Web search → Skills → Daemon → Health check.
- 채널을 나중에 더하는 명령: `openclaw channels add [--channel telegram --token …]`. 인자가 없으면 목록에서 고른다(`O:src/cli/channels-cli.ts:345`, `docs/channels/index.md:15-34`) [에이전트 관찰].
- 구현 폴더: `O:src/wizard/setup.*.ts`(`setup.gateway-service-offer.ts`, `setup.inference-verification.ts`, `setup.finalize.ts` 등) [관찰: 파일 목록만 봤다].

**Hermes**
- 한 줄 설치: `curl -fsSL https://hermes-agent.nousresearch.com/install.sh | bash`(`H:README.md:40`) [에이전트 관찰].
  - 단계(`stage_names()`, `H:scripts/install.sh:499`): prerequisites → repository → venv → python-deps → config → products → **setup**(`/dev/tty`가 있을 때만 `hermes setup`, :1170) → **gateway**(`hermes gateway install --if-missing`, :1179) → complete.
- `hermes setup`(`_run_setup_wizard_impl`, `H:hermes_cli/setup.py:672`) [에이전트 관찰]:
  - 터미널이 아니면 안내만 하고 끝난다(:695).
  - 첫 실행에서는 먼저 다른 에이전트 설정 가져오기를 제안한다(`offer_agent_import`).
  - 이어서 "How do you want to power Hermes?"를 묻는다(:742). 선택지(`_POWER_ROWS`, `setup_quick.py:157`)는 다음과 같다: 가져온 모델 유지 / 무료 / Nous Portal 로그인 / "Use my own API key or a subscription" / Full / Blank Slate.
  - Full(`_run_full_setup`, `setup.py:627`): ① 모델 · 공급자 ② 터미널 백엔드 ③ **메시징 플랫폼(`setup_gateway`)** ④ 도구.
  - 섹션별로 다시 돌릴 수 있다: `hermes setup <model|gateway|tools|…>`(`SETUP_SECTIONS`, :591).
  - 메시징 단계(`setup_gateway`, `setup_platforms.py:289`):
    - 플랫폼 체크리스트를 띄운다.
    - 텔레그램은 **토큰 → 허용 사용자 ID → 홈 채널**(자기 ID를 홈으로 쓸지 제안) 순서로 묻는다(`_setup_telegram`, `gateway_setup_wizard.py` 약 :147-185).
    - 서비스 설치는 명시적으로 동의해야 한다(`SERVICE_INSTALL_QUESTION`, `gateway_setup_service.py:8`).
- `hermes doctor`(`DOCTOR_CHECKS`, `H:hermes_cli/doctor.py:114`): `--live`를 주면 실제 네트워크 점검을 하고, `--fix`도 있다 [에이전트 관찰].

### 2.2 로그인

**OpenClaw**
- **Claude CLI 재사용을 먼저 쓴다** [관찰, `O:docs/concepts/oauth.md:134-140`]:
  - "OpenClaw supports Anthropic Claude CLI reuse and `claude -p` as a sanctioned …"
  - setup-token은 남아 있지만 CLI 재사용을 우선한다.
  - 약관 판단 근거는 문서에 "Anthropic 직원이 허용했다"는 서술뿐이다(:20-21, :145). **확인 필요**: 우리는 이 서술을 사실로 받지 않는다.
- CLI 상태 확인은 `claude --version`, `claude auth status --text`이고, 전용 로그인 폴더로 `CLAUDE_CONFIG_DIR=…`를 쓴다(`O:docs/gateway/cli-backends.md:434, 483, 936`) [관찰].
- token sink 원칙 [관찰, `oauth.md:53-73`, 앞선 조사 2.4]:
  - 자체 OAuth 프로필이 생기면 그것이 정본이다.
  - 외부 CLI 토큰은 다시 읽기만 하고, 복사한 refresh 토큰을 쓰지 않는다.
  - Codex CLI bootstrap은 "빈 상태를 처음 채울 때만" 쓴다(:68).
- Codex(ChatGPT 구독) [앞선 조사, `openclaw.md:255-258`]:
  - PKCE `localhost:1455/auth/callback`과 붙여 넣기가 경쟁해 먼저 끝나는 쪽을 쓴다.
  - **device code** 방식은 15분 제한에 5초 폴링이다.
- Anthropic 자체 PKCE [앞선 조사]: 루프백 포트 53692 `/callback`.

**Hermes** [에이전트 관찰]
- 저장:
  - OAuth 자격은 `~/.hermes/auth.json`(0600 원자 쓰기, 폴더 0700, flock)에 둔다(`H:hermes_cli/auth.py:479-482, 711, 782-786`).
  - API 키는 `~/.hermes/.env`에 둔다(`save_env_value`).
- 흐름별:
  - Nous · xAI · Codex(기본)는 **device code**다.
  - Codex `--browser`는 PKCE `localhost:1455`이다(`auth_codex_browser.py:5, 37`).
  - Anthropic 자체 흐름은 PKCE지만 콜백이 원격이라 **코드를 붙여 넣는다**(`run_hermes_oauth_login_pure`, `agent/anthropic_credentials.py:771`).
  - `claude setup-token`을 대신 실행하는 옵션도 있다(:729).
  - OpenRouter는 PKCE `127.0.0.1:<임의 포트>`이고 결과로 API 키를 받는다.
- **Claude Code 자격 빌려 쓰기**(`read_claude_code_credentials`, :359):
  - 키체인 `Claude Code-credentials`와 `~/.claude/.credentials.json`을 읽는다. 복사하지 않고 빌려 쓴다.
  - **갱신하면 Claude Code 파일과 키체인에 다시 쓴다**(:519, 543, 582).
- **Codex CLI 자격**(`_import_codex_cli_tokens`, `auth_codex.py:587`): `~/.codex/auth.json`을 읽어 자기 `auth.json`으로 가져온다.
- 외부 로그인 사용은 `auth.adopt_external_logins`(기본 참)로 끌 수 있다(`agent/credential_sources.py:32`).
- 프로필: `-p <name>`이면 `~/.hermes/profiles/<name>`을 쓴다(`profiles.py:295`). 프로필에 없는 공급자는 전역 `auth.json`을 읽기 전용으로 참조한다.

**약관 사실** (`docs/research/cli-backend.md` 4.5에 원문이 있다. 새로 확인한 것은 없다)
- Anthropic은 "developers may not collect, store, or intermediate Claude.ai credentials or session tokens"라고 적는다.
- 같은 문서는 "손대지 않은 공식 CLI에 사용자가 자기 구독으로 로그인하는 것"은 막지 않는다고 적는다.
- **"자기 프로그램이 로그인된 CLI를 자식 프로세스로 부르는 경우"를 짚은 문장은 없다.**
- 소비자 약관에는 "증권 매매 목적" 금지 항목이 있다.
- 따라서 Hermes처럼 **Claude Code 토큰 파일을 직접 읽고 갱신하는 길은 금지 문장 범위로 읽힌다** [추정].
- Codex · OpenAI의 해당 약관은 **이번에 확인하지 않았다(확인 필요)**.

### 2.3 로컬 웹(대시보드) 띄우기와 인증

**OpenClaw** [관찰, `O:docs/web/dashboard.md`]
- 주소는 `http://127.0.0.1:18789/`이다(:12).
- `openclaw dashboard`는 **일회용 페어링 링크**(owner pairing handoff)를 열거나 복사한다(:37-44). 공유 토큰을 URL에 넣지 않는다(:55, :106).
- 공유 비밀은 `gateway.auth.token` / `OPENCLAW_GATEWAY_TOKEN`이다. 붙여 넣으면 sessionStorage에 둔다(:31, :51).
- 원격 접속 수단: Tailscale Serve(신원 헤더, `gateway.auth.allowTailscale`), trusted-proxy, SSH 터널(:63-64).
- "Control UI is an admin surface … Do not expose it publicly"(:31).
- 웹 알림(`docs/web/notifications.md`):
  - 종류별 설정, 조용한 시간, 시간대, 잠금 화면 상세도를 둔다(:47).
  - **조용한 시간에는 보내지 않고 쌓아 두지도 않는다**(:56).
  - 기본으로 켜진 것은 승인 요청과 결과뿐이다.

**Hermes** [에이전트 관찰]
- `hermes dashboard`는 `127.0.0.1:9119`에 연다.
- 실행할 때마다 `token_urlsafe(32)` 세션 토큰을 만들어 HTML에 넣는다(`web_server_session_token.py`, `web_server.py:310-330`).
- 루프백이 아닌 주소나 public_url을 쓰면 **로그인 게이트를 강제한다**(`should_require_dashboard_auth`, :373). Host 검사도 한다(:388).

### 2.4 메신저 채널

**채널 추상화**
- OpenClaw [에이전트 관찰]:
  - 인터페이스는 `ChannelPlugin`(`O:src/channels/plugins/types.plugin.ts:69-129`)이다. 선택형 어댑터 묶음이다: `config` · `pairing` · `security` · `outbound` · `gateway` · `approvalCapability` · `commands` · `allowlist` · `streaming` · `threading`.
  - 수명은 `startAccount` · `stopAccount`(`types.adapters.ts:229-275`)로 다룬다.
  - 보내기는 `ChannelOutboundAdapter`(`outbound.types.ts:128`)다. 칸: `chunker`, `textChunkLimit`, `sanitizeText`, `deliveryMode`.
- Hermes [에이전트 관찰]:
  - `BasePlatformAdapter`(`H:gateway/platforms/base.py:1780`)의 추상 메서드는 `connect` · `disconnect` · `send`(:2594-2604)다.
  - 공통 메서드로 `send_exec_approval`(:2744)과 `truncate_message`가 있다. 기본 `MAX_MESSAGE_LENGTH`는 4096이다.
  - 플랫폼은 `plugins/platforms/<name>/`에 26종이 있다.

**Telegram**

| 항목 | OpenClaw | Hermes |
|---|---|---|
| 토큰 | BotFather `/newbot`. 키 `botToken` / `tokenFile`(심볼릭 링크 거부) / env `TELEGRAM_BOT_TOKEN`. 우선순위 tokenFile > botToken > env이고 env는 기본 계정만 쓴다(`resolveTelegramToken`, `extensions/telegram/src/token.ts:122-175`) | env `TELEGRAM_BOT_TOKEN`(`plugin.yaml`) |
| 받기 | **기본 long polling**(grammY). `webhookUrl`이 있으면 webhook이고 `webhookSecret`이 필요하다(`channel.ts:791`, `docs/channels/telegram/transports.md:10-22`). 토큰당 poller 하나를 lease로 잡는다 | **기본 long polling**. `TELEGRAM_WEBHOOK_URL`이 있으면 webhook이고 `_SECRET`이 필수다(`adapter.py:2858-2918`) |
| 길이 | `TELEGRAM_TEXT_CHUNK_LIMIT = 4000`(`text-chunk-limit.ts:7`, **확인함**) | 4096(UTF-16 기준) |
| 서식 | HTML parse_mode | MarkdownV2 |
| 버튼 | 인라인 키보드. callback_data는 64바이트 한도(`approval-callback-data.ts:4`) | 인라인 키보드 `ea:<choice>:<id>` |

**Slack**
- OpenClaw: 모드는 `socket | http | relay`이고 **기본은 `socket`**이다(`extensions/slack/src/accounts.ts:185`, **확인함**).
  - Socket Mode는 `appToken`(xapp, `connections:write`) + `botToken`(xoxb)을 쓴다.
  - HTTP 모드는 `signingSecret`과 `/slack/events`를 쓴다.
  - 앱은 매니페스트로 만든다. 글자 수 한도는 `SLACK_TEXT_LIMIT = 8000`이다 [에이전트 관찰].
- Hermes: **Socket Mode만 쓴다**(`AsyncSocketModeHandler`, `adapter.py:1231`). 토큰은 `SLACK_BOT_TOKEN` + `SLACK_APP_TOKEN`이다 [에이전트 관찰].

**누가 봇에 말할 수 있나**
- OpenClaw:
  - DM 정책은 `pairing | allowlist | open | disabled`, 그룹 정책은 `open | disabled | allowlist`다(`O:src/config/zod-schema.core.ts:471-473`).
  - **텔레그램과 슬랙 DM의 기본값은 `pairing`이다.** 그룹은 allowlist이고, 항목이 없으면 모두 막는다.
  - `open`은 `allowFrom`에 `"*"`가 있어야 열린다. `allowlist`인데 목록이 비어 있으면 설정 검증에서 거부한다 [에이전트 관찰].
  - **페어링 코드**(`O:src/pairing/pairing-store-model.ts:7-11`, **확인함**):
    - 8자리, 글자는 `ABCDEFGHJKLMNPQRSTUVWXYZ23456789`.
    - TTL 1시간, 대기는 최대 3건.
    - 코드 메시지는 요청이 새로 생길 때만 보낸다.
  - 승인은 터미널 `openclaw pairing approve telegram <CODE>`(`approveChannelPairingCode`) 또는 화면 Settings → Channels에서 한다.
  - **첫 승인자는 명령 owner가 된다**(`bootstrapCommandOwnerFromPairing`, `src/pairing/command-owner.ts:14`).
  - 승인된 목록은 SQLite 표 `channel_pairing_allow_entries`에 둔다.
- Hermes:
  - 판정 순서(`_is_user_authorized`, `H:gateway/authz_mixin.py:855-939`): … → 페어링 승인 목록 → `{PLATFORM}_ALLOWED_USERS` → (목록이 하나도 없을 때만) `GATEWAY_ALLOW_ALL_USERS` → 그 밖에는 거부.
  - 모르는 DM에 대한 기본 처리는 pair다. 허용 목록이 있으면 무시한다.
  - 페어링: 8자리, 1시간, 플랫폼당 대기 3건, 사용자당 10분에 1회, **승인 5회 실패하면 1시간 잠금**. 저장은 `~/.hermes/pairing/`(0600)이고 코드는 기록에 남기지 않는다(`H:gateway/pairing.py:1-37`).

### 2.5 나가는 메시지 · 들어오는 명령 · 라우팅

**나가는 것**
- 공통: 에이전트 답, cron · heartbeat 결과, 승인 프롬프트와 그 결과, 페어링 알림.
- 답은 **들어온 채널로 돌아간다. 모델이 채널을 고르지 않는다**(`O:docs/channels/channel-routing.md:10-13`).
- cron은 `delivery.target: "owner"`이거나 `channel: "last"`를 쓴다.
- Hermes에는 **홈 채널** 개념이 있다:
  - env `TELEGRAM_HOME_CHANNEL`로 정하거나 채팅에서 `/sethome`으로 정한다.
  - 홈이 없으면 cron을 그 플랫폼으로 보낼 수 없다.
  - cron `deliver`는 origin · local · telegram 등이고 여러 개를 줄 수 있다 [에이전트 관찰].

**조용한 시간**
- 채널 쪽: OpenClaw는 채널 수준에서 없음. cron · heartbeat의 `activeHours`만 있다.
- 웹 알림 쪽: OpenClaw에 있다(2.3).

**다시 보내기**
- OpenClaw: 전송 대기열 복구는 최대 5회 backoff다(`src/infra/outbound/delivery-queue-recovery-policy.ts:9`).
- 텔레그램 429는 `retry_after`를 지키는 계정별 throttler로 처리한다 [에이전트 관찰].

**가리기**
- OpenClaw: 일반 나가는 글을 가리는 단계는 **못 봄**. 채널별 `sanitizeText` 훅만 있다.
- Hermes:
  - cron 출력은 `redact_sensitive_text(force=True)`를 거치고, 가리기가 실패하면 보내지 않는다(`scheduler_delivery.py:182`).
  - 승인 카드와 오류 글도 가린다.
  - 일반 최종 답을 보내기 직전의 가리기는 **못 봄** [에이전트 관찰].

**들어오는 명령**
- OpenClaw `/new /reset /stop /status /approve <id> <decision> /restart /config /model /whoami`:
  - `commands.allowFrom`을 설정하면 그것이 유일한 권한 근거다.
  - owner 전용 명령은 `commands.ownerAllowFrom`으로 정한다(`docs/tools/slash-commands.md:188-237`).
- Hermes `/new /stop /status /approve /deny /sethome /restart …`(`COMMAND_REGISTRY`, `hermes_cli/commands.py:83`):
  - 관리자 목록을 따로 정하지 않으면 **말할 수 있는 사람 누구나 모든 명령을 쓴다**(`gateway/slash_access.py:1-19`).

**루프 방지**
- OpenClaw 봇 쌍 sliding window: 60초에 20개, 쿨다운 60초(`docs/channels/bot-loop-protection.md`).
- 사람 발신자에 대한 inbound 속도 제한은 **못 봄**.

### 2.6 메신저에서 승인 받기 (가장 중요한 원본 동작)

**OpenClaw** [에이전트 관찰, 식별자는 rea로 확인]

렌더링:
- 텔레그램은 인라인 버튼이다. callback_data는 `tga1:` + 종류(e/p/s) + 결정(o/a/d)이다(`buildTelegramApprovalCallbackData`, `extensions/telegram/src/approval-callback-data.ts:5-17`).
- 슬랙은 Block Kit 버튼이고 값은 `openclaw:approval:v1:{approvalId, approvalKind, decision}`이다. 형식이 틀리면 거부한다(`encodeSlackApprovalAction` / `decodeSlackApprovalAction`, `extensions/slack/src/approval-actions.ts:6-45`).
- 버튼이 없는 채널은 `/approve <id> allow-once|allow-always|deny`로 받는다.

**승인자 검증**:
- 승인자는 `channels.<ch>.execApprovals.{enabled, approvers, target: dm|channel|both}`로 정한다. 비어 있으면 `commands.ownerAllowFrom`을 쓴다. 텔레그램 승인자는 **숫자 user ID**여야 한다.
- 권한이 없는 탭의 처리:
  - 텔레그램: 조용히 무시한다(`resolveAuthorizedApprovalKinds`, `bot-handlers.callback-router-controls.ts:92-202`).
  - 슬랙: ephemeral 메시지로 "not authorized"를 보낸다.
- 게이트웨이가 최종 결정 전에 권한을 다시 확인한다(`prepareApprovalChannelCustody`, `src/gateway/approval-channel-custody.ts:40`).
- 문서: **`allowFrom`에 있다고 승인자가 되지는 않는다**(`telegram/rich-messages.md:194`).

**ID · 만료 · 모르는 값**:
- ID는 `randomUUID()`다.
- 기본 대기는 30분이다(`DEFAULT_EXEC_APPROVAL_TIMEOUT_MS = 1_800_000`, `src/infra/exec-approvals-core.ts:267`, **확인함**). 시간이 지나면 거절이다.
- 이미 처리된 요청은 "no longer pending"으로 답하고 버튼을 지운다(`terminalizeApprovalMessage`).
- 요청에 없던 결정은 거부한다. **먼저 온 답이 이긴다.**

**내용 바인딩**:
- 승인은 cwd · argv · env 해시 · 실행 파일 경로 · 스크립트 sha256에 묶인다. 바뀌면 실행을 거부한다(`src/infra/system-run-approval-binding.ts`).

**문서의 경고**:
- 채널(그룹)로 보내면 명령 글이 드러나므로 믿는 그룹에서만 켜라.
- 맨 `/approve`나 지어낸 ID로는 승인할 수 없다.
- `operator.approvals`는 원격 실행급 권한으로 다뤄라.

**Hermes** [에이전트 관찰]
- 보내기:
  - 버튼을 지원하면 `send_exec_approval`로 보낸다.
  - 실패하면 `/approve` 안내 글로 대신한다.
  - egress 가드가 거부하면 대신 보내지 않고 `notify_failed`로 닫는다(`run_turn_runner.py:798-884`).
- 선택지는 once · session · always · deny다. 동의 단어는 허용 목록 방식이고 모르는 답은 막힘이다(`CONSENT_ANSWERS`, `tools/approval.py:885`).
- **승인자 기본값은 `["*"]`(말할 수 있는 누구나)**다(`gateway/approval_actor.py:11`, **확인함**). 공유 채팅에서는 아무나 승인할 수 있다고 문서가 경고한다.
- 대기는 300초이고 시간이 지나면 거절이다. 버튼은 지운다.
- 무인 플랫폼은 기본으로 즉시 거부한다(`unattended_mode: deny`).

---

## 3. 지금 우리 것

| 영역 | 지금 | 근거 |
|---|---|---|
| 설치 | `git clone` → `cargo build --release`. 설치 스크립트, 릴리스 빌드(`.github/` 없음), `scripts/install*`가 **없다** | `README.md:21-31`, ls |
| CLI 하위 명령 | `run`(기본) · `--version` · `service install\|uninstall\|start\|stop\|restart\|status` · `doctor [--json]` · `access-key [--rotate]` · `restore <파일> --identity --into`. **`setup` · `open` · `logs` · `update` · `uninstall` · `channels` · `pairing` 없음** | `crates/server/src/cli/command.rs:13-187`, `main.rs:159-217` |
| 서비스 | launchd 사용자 에이전트 · systemd 사용자 unit · Windows 예약 작업 | `crates/server/src/cli/service.rs:1-4` |
| doctor | 1 보관 폴더 권한 · 2 저장소 무결성 · 3 키 칸 등록 여부 · 4~7 주소 · 접속 키 · 서비스 · 백업 · 8 Ollama. 자동으로 고치지 않고 바깥을 부르지 않는다 | `cli/doctor.rs:1-2, 212, 289, 386`, `cli/doctor/host.rs`, `cli/doctor/ollama.rs` |
| 첫 설정 | **브라우저 화면만**: `/api/setup`, `/keys`, `/api/setup/{keys,symbols,ai,doctor,sound,backup,semantic,slots,providers}`. 필수 항목이 갖춰지기 전에는 팀을 시작하지 않는다 | `setup.rs:1-15, 102-111`, `ai_link.rs:79`, `setup_doctor.rs:39` |
| AI 연결 | `{"method":"api_key"}` 또는 `{"method":"cli","command":<절대경로>,"config_dir":<선택>}` 중 **사용자가 고른 하나**만 쓴다. 넘어가기 없음. CLI는 `ClaudeCliProvider::check`(버전 → 로그인, 모델 호출 없음)로 `LoggedIn`일 때만 갖춰진다. **구독 토큰을 받는 칸이나 길은 없다**(D4). Codex는 없다 | `ai_link.rs:1-40`, `crates/ai/src/cli.rs:1-5`, `crates/ai/src/cli/check.rs`, `crates/secrets/src/lib.rs` "구독 토큰은 받지도 쥐지도 않는다", `dev.md:25-29` |
| 대시보드 · RPG | 사무실(`/api/office*`, 공지 · 소환 · 대화), 팀 · 템플릿 · 규칙 · 히스토리 · 계좌 화면, 실시간 WS | `reach.rs:84-438`, `page.rs:26-34`, `trading.rs`, `team.rs` |
| 접속 관문 | 그 컴퓨터(루프백이고 전달 헤더 없음)는 키 없이 들어온다. 밖에서는 **접속 키**를 쓴다(소금 친 SHA-256 저장, 세대, 잠금, 접속 기록). Tailscale · TOTP는 스펙에서 버렸다 | `crates/gate/src/access.rs:1-13`, `enter.rs`, `access_http.rs`, `dev.md:13-17` |
| 터미널에서 일회용 키 | `price-inha access-key`가 도는 서버에 키를 만들게 하고 한 번만 보여 준다(OpenClaw 3.3 근거) | `cli/access.rs:1-9` |
| 폰 · 푸시 | 웹 푸시(VAPID ES256, RFC 8291 순수 Rust), 알림 목록, 승인 카드, 승인/거절, 하루 정리 | `crates/notify/src/lib.rs`, `reach/phone_http.rs`, `reach.rs:296-336` |
| 알림 종류 | `ApprovalRequest, Stall, Anomaly, AccessLock, BackupFailure, DailySummary`, 긴급도 4단 | `crates/notify/src/kind.rs:5-30` |
| 조용한 시간 | notify에 기능은 있지만(`quiet.rs`, "기본값 없음") **서버가 `quiet_hours: None`으로 고정**한다. 화면 설정은 없다 | `crates/server/src/reach/linked.rs:150` |
| 승인 | 승인 요청 → 카드 → `POST /api/approvals/{id}/{approve\|reject}` 몸통 `{"fingerprint": …}`. **세션 + 화면에 보인 내용 지문(주문 내용 SHA-256)**이 레코드와 같아야 한다. 확인 실패나 모르는 값은 거절이다(P19). 답이 없으면 E22 처리 | `reach/phone_http.rs:178-207`, `crates/approval/src/subject.rs:1-4`, `approvals.rs:1-10`, `dev.md:247-253`, `principles.md:28-30` |
| 가리기 | 등록된 비밀 값과 확인 중인 값을 바깥 글에서 가린다. 푸시 전에도 가린다(E38) | `crates/secrets/src/lib.rs`, `notify/src/lib.rs:4` |
| 메신저 | **없음.** PRD 범위 밖으로 명시 | `prd.md:414` |
| 대조표 | 구독 OAuth 자리 A2~A9는 "2026-11-15 의도해서 다름: 구독은 CLI 백엔드로". 웹 푸시 T22는 OpenClaw 승인 웹 푸시와 대조됨 | `docs/dev/conformance.md:37-44, 244, 290-292` |
| 쓸 수 있는 의존성 | `reqwest`, `tokio-tungstenite`, `axum`, `sha2`, `url`, `serde_json`이 이미 workspace에 있다. 텔레그램 long polling(HTTPS)과 슬랙 Socket Mode(WSS)는 **새 무거운 의존성 없이** 된다 [추정]. 슬랙 HTTP 모드의 서명 확인에는 HMAC-SHA256이 필요하고, `hmac` 크레이트는 workspace 목록에서 **못 봄** | `Cargo.toml:32-58` |

---

## 4. 차이 분석 표

| # | 바라는 것 | 원본 동작(근거) | 지금 우리 것(근거) | 빠진 것 | 권장 만들기 방식 |
|---|---|---|---|---|---|
| 1 | 다른 컴퓨터에서 한 줄로 설치 | O `curl …/install.sh \| bash` → 끝에 `exec openclaw onboard`. H install.sh 9단계 → `hermes setup` → `gateway install --if-missing`(2.1) | 소스 빌드만 있음(`README.md:21-31`). 릴리스 CI 없음 | 릴리스 바이너리, install.sh · install.ps1, 체크섬 | `install-and-cli.md` 8.2를 그대로 쓴다: OS · 아키텍처 판별 → 릴리스 파일과 SHA-256 → `~/.local/bin` → PATH 한 줄 → `/dev/tty`가 있으면 `price-inha setup` 실행. 설치기는 셸 스크립트이고 Rust 의존성은 없다 |
| 2 | 터미널 세팅 마법사 | O Quick: 탐지 → 고르기 → **실제 호출 1회 검증** → 서비스(기본 예) → 대시보드 열기. H Full: 모델 → 터미널 → **메시징** → 도구, 섹션별 재실행(2.1) | 없음. 앞선 결정이 "주소만 연다"(`install-and-cli.md:394, 434`) | `price-inha setup` | **결정 변경 필요.** 마법사는 화면과 같은 저장 길(`/api/setup/*`와 같은 함수)을 부르는 얇은 터미널 표면으로 만든다. 단계는 5(가) |
| 3 | AI 로그인: CLI 재사용 | O Claude CLI 재사용을 우선, `claude auth status`로 확인(2.2). H Claude Code 토큰 파일을 직접 읽고 **갱신해 다시 씀** | **있다**: CLI 경로 · 전용 config_dir · `check`(버전 → 로그인). 화면에서만 고를 수 있다(`ai_link.rs`) | 터미널에서의 자동 탐지(`which claude`, `auth status`), "로그인이 안 됨 → `claude auth login`을 직접 실행하라" 안내 | 마법사가 `ClaudeCliProvider::check`를 재사용해 탐지하고 결과를 보여 준다. **토큰 파일은 읽지 않는다**(Hermes 방식은 약관 금지 범위로 읽힘, 2.2) |
| 4 | AI 로그인: OAuth | O Codex PKCE 1455 + device code. Anthropic PKCE 53692. H device code 다수(2.2) | **의도해서 없음**(D4, 결정 2026-11-15) | — | Anthropic OAuth는 만들지 않는다(약관 문장 "may not collect, store, or intermediate"). Codex OAuth는 **약관 확인 전 보류**(6장 결정 2) |
| 5 | AI 로그인: API 키 | 둘 다 가려진 입력 → `.env` 0600 | 화면 `/keys`에서 등록하고, **저장 전에 실제로 닿는지 확인**한다(원본보다 엄격, conformance B15) | 터미널에서 입력 | 마법사는 가려진 입력(echo 끔)으로 받아 같은 등록 함수(확인 → 원자 쓰기)를 부른다. 그 컴퓨터에서만 |
| 6 | 프로필 여러 개 | H `-p name` → 폴더 하나 | `--home`으로 대신한다(`install-and-cli.md:403` "가져오지 않음") | — | 지금 그대로 둔다(1인 1인스턴스) |
| 7 | 세팅이 끝나면 로컬 웹을 띄워 줌 | O `openclaw dashboard`가 **일회용 페어링 링크**를 연다(`dashboard.md:37-44`). H 토큰을 HTML에 넣는다 | 루프백이면 키 없이 세션을 받는다(C15). 주소는 표준 오류로 알린다. `open` 명령 없음 | `price-inha open`, 마법사 끝에서 브라우저 열기 | 루프백은 지금 규칙으로 충분하다. URL에 비밀을 싣지 않는다. `open`은 잠금 파일 · 서비스 정의에서 주소를 찾아 OS 열기 명령을 부른다(`CommandRunner` 뒤) |
| 8 | 대시보드와 RPG에서 일하는 모습 보기 | O Control UI. H 대시보드 + 칸반 | **있다**(사무실 · 팀 · 히스토리 · 하루 정리) | — | 유지. 메신저 이벤트(받음 · 보냄 · 막힘)를 사무실 피드에 더한다 |
| 9 | 대시보드에서 설정 관리 | O Settings 화면, `config` 명령. H 대시보드 REST | 키 · 종목 · AI · 소리 · 백업 · 뜻 검색 · 규칙 · 템플릿은 화면에 있음 | **알림 설정**(조용한 시간 · 종류별 경로, 지금 `None` 고정), **채널 설정** | 5(라) 목록 |
| 10 | 메신저 등록(텔레그램 · 슬랙) | O `channels add`, 위저드 단계, 매니페스트. H `setup gateway`(2.4) | **없음**. PRD 범위 밖(`prd.md:414`) | 채널 크레이트, 토큰 보관, 등록 화면 · 명령 | 새 모듈 `channel`(5(다)). 텔레그램은 long polling(기본, 들어오는 포트 없음), 슬랙은 Socket Mode(기본, 들어오는 포트 없음). webhook · HTTP 모드는 만들지 않는다(루프백만 연다는 D2와 맞음) |
| 11 | 누가 봇에 말할 수 있나 | 둘 다 **DM 기본 pairing**. 8자리 · 1시간 · 대기 3건. 승인은 서버 쪽(터미널 · 화면)에서. O 첫 승인자가 owner. H 실패 5회 → 1시간 잠금(2.4) | 없음 | 페어링, 허용 목록 | 원본과 같게: 8자리(같은 글자 집합) · 1시간 · 대기 3 · 코드는 기록 금지 · 승인은 **그 컴퓨터의 화면이나 터미널에서만**. 그룹 · 채널은 기본으로 막는다. 사용자가 한 명이므로 승인된 사람은 최대 1명으로 시작한다(결정 4) |
| 12 | 일하는 메시지를 메신저로 받기 | O 답은 들어온 채널로. cron `target: owner`. H 홈 채널 · `/sethome`(2.5) | 웹 푸시로만. 종류 6개 | 종류별 경로 정하기, 길이 나누기, 가리기 | notify에 "전달처" 한 겹을 더한다: 종류마다 {웹 푸시, 메신저} 켜기. 메신저 글은 **보내기 전에 E38 가리기를 반드시 거치고, 가리기가 실패하면 보내지 않는다**(Hermes cron 방식). 4000자(텔레그램) · 8000자(슬랙)로 나눈다 |
| 13 | 조용한 시간 | O 웹 알림: 조용한 시간에는 **버리고 쌓지 않는다**(`notifications.md:56`). 채널 수준은 없음 | notify에 있으나 설정 길이 없음 | 화면 설정 | 웹 푸시와 메신저에 같은 조용한 시간을 적용한다. 승인 요청은 조용한 시간에도 보낸다(R4-1. 원본도 승인만 기본으로 켬) |
| 14 | 메신저에서 명령 | O `/new /stop /status /approve …`, owner 목록. H 기본은 누구나 모든 명령 | 없음 | 명령 처리 | 페어링된 본인만. 읽기와 위험 줄이기 명령만 연다(5(다)) |
| 15 | **메신저에서 승인 게이트** | O 버튼 + **승인자 ID 목록** + 게이트웨이 재확인 + 30분 + first-wins + 내용 바인딩. H 버튼 + 기본 승인자 `*` + 300초 | 승인은 **세션 + 화면에 보인 지문**(`phone_http.rs:178-207`) | 메신저 승인 경로 전체 | 5(다)의 안전 규칙. 핵심은 **기본은 "메신저로 알리고, 승인은 링크로 연 웹 카드에서"**이고, 버튼 승인은 사용자가 켜는 선택지다 |
| 16 | 메신저 비밀값 보관 | O `botToken` · `tokenFile` · env. H `.env` | secrets 크레이트: 칸별 파일 0600, 등록 전 확인, 가리기(`secrets/src/lib.rs`) | 채널 키 종류 | `KeyKind`에 Telegram 봇 토큰, Slack 앱 토큰 · 봇 토큰을 더한다. 등록 전 확인은 텔레그램 `getMe`, 슬랙 `auth.test`로 한다(시험은 로컬 가짜 서버로) |
| 17 | 원격에서 대시보드 | O Tailscale Serve · SSH 터널. H 로그인 게이트 | 접속 키(D2) | — | 유지. 메신저 링크는 **사용자가 설정한 바깥 주소가 있을 때만** 싣고, 없으면 "그 컴퓨터에서 열기"라고만 적는다 |
| 18 | doctor | H `--live`(실제 네트워크) | 바깥을 부르지 않는다 | 채널 점검 | doctor 9: 채널 토큰 등록 여부(이름만)와 페어링 수. 바깥 확인은 하지 않는다(지금 원칙 유지) |

---

## 5. 권장 설계 초안

### (가) 설치 한 줄과 `price-inha setup`

**설치**는 `install-and-cli.md` 8.2를 그대로 쓴다. 다른 점은 마지막 단계에서 `price-inha setup`을 부른다는 것 하나다.
- 터미널이 없으면 부르지 않고 "`price-inha setup` 또는 브라우저"를 안내한다.

**`price-inha setup [section]`**: 터미널 전용이다. 원칙은 다음과 같다.
- 마법사는 **도는 서버의 루프백 경로를 그대로 부른다**(`access-key`와 같은 방식, `cli/access.rs`). 도는 서버가 없으면 먼저 띄울지 묻는다.
- 화면과 저장 길이 하나라서 규칙(등록 전 확인 · 시도 제한 · 원자 쓰기)이 두 벌이 되지 않는다.

| 단계 | 하는 일 | 원본 근거 |
|---|---|---|
| 0 | 보관 폴더와 권한, 이미 설정돼 있으면 "다시 할 섹션 고르기" | H 기존 설치 재실행(`setup.py:716-729`) |
| 1 AI 연결 | 탐지: `claude`가 PATH에 있는지 → `check`(버전 · 로그인). 결과를 보여 주고 고른다: [Claude CLI(로그인됨)] [Claude CLI 설치는 됐지만 로그인 안 됨 → "다른 창에서 `claude auth login`을 실행한 뒤 Enter"] [API 키 입력(가려짐)] [건너뛰기]. 고른 것만 확인한다 | O Quick start 2~3(`wizard.md:117-125`) |
| 2 증권 키 | 모의계좌라 필요한 칸만 묻는다(지금 `/keys`의 칸과 같음). 가려진 입력 → 등록 전 확인 | H 키 입력, 우리 C3 · C16 |
| 3 종목 | 지켜볼 종목 | 우리 `/api/setup/symbols` |
| 4 메신저(선택) | [텔레그램] [슬랙] [나중에]. 텔레그램: 토큰 → `getMe` 확인 → "봇에게 아무 말이나 보내세요" → 터미널에 뜬 페어링 요청을 **승인할지 묻기**(코드 대조). 슬랙: 매니페스트 안내 → 앱 토큰 · 봇 토큰 → `auth.test` → 같은 페어링 | H 메시징 단계(`setup_platforms.py:289`), O 채널 위저드(`setup-surface.ts:32`) |
| 5 서비스 | "계속 켜 둘까요?" 기본 예. 예 → `service install`. 아니오 → 포그라운드 | O 5단계 |
| 6 열기 | 브라우저로 대시보드를 연다(URL에 비밀 없음) | O 7단계 |

- 비대화형이면 마법사 대신 안내만 하고 종료 코드 2를 낸다(H :695).
- 시험 방법: 표준 입력을 흉내 내고, 가짜 CLI 실행 파일 · 가짜 텔레그램 · 슬랙 서버를 로컬에 띄운다.

### (나) 로그인: 무엇을 어떤 순서로

| 순서 | 방식 | 지원 | 까닭 |
|---|---|---|---|
| 1 | **Claude Code CLI 재사용**(평소 로그인 또는 전용 `CLAUDE_CONFIG_DIR`) | 이미 있음. 마법사 탐지만 더함 | 토큰을 읽지도 쥐지도 않는다(D4). 원본 OpenClaw도 이것을 우선한다 |
| 2 | **API 키** | 이미 있음 | 공식 문서가 개발자에게 권하는 길(`cli-backend.md` 4.5 (가)) |
| 3 | Ollama(로컬) | 뜻 검색에만 있다 | 범위 밖. 그대로 둔다 |
| — | Anthropic OAuth(PKCE · 붙여 넣기) · Claude Code 토큰 파일 읽기 · `setup-token` 보관 | **만들지 않음** | 약관 문장 "developers may not collect, store, or intermediate Claude.ai credentials or session tokens"(사실). 2026-11-15 결정 |
| 보류 | Codex(ChatGPT) OAuth · Codex CLI 재사용 | **확인 필요** | OpenAI 약관을 이번에 확인하지 않았다. 원본 동작(device code 15분 · 5초 폴링, PKCE 1455)은 조사돼 있다 |

조심할 점(사실만):
1. 구독으로 돌리면 소비자 약관의 "증권 매매 목적" 금지 항목과 "ordinary, individual usage" 전제가 걸린다(`cli-backend.md` 4.5 (다)(마)).
2. `--bare`가 `-p`의 기본값이 되면 구독 길이 끊길 수 있다고 공식 문서가 예고했다(4.4).
3. 환경에 `ANTHROPIC_API_KEY`가 남아 있으면 CLI도 API 키로 과금된다(4.4). 마법사가 이를 감지해 경고하는 것은 [추정] 유용하다.

### (다) 메신저 게이트

**모듈**: 새 크레이트 `crates/channel`(pi-channel). architecture.toml · graph.toml에 모듈을 더하는 일이 필요하다. 규칙 파일 수정 여부는 6장 결정 6이다.

**채널 추상화 한 겹** (원본 OpenClaw `ChannelPlugin`을 축소한 것):
- `trait Channel`
  - `start(cancel) -> 받은 메시지 흐름`
  - `send(to, OutText) -> MsgRef`
  - `edit(MsgRef, OutText)`
  - `send_approval(to, ApprovalCard) -> MsgRef`
  - `limits() -> {text_max, button_data_max}`
- 구현은 둘이다.
  - `Telegram`: `getUpdates` long polling. `reqwest`로 하고, 토큰당 poller는 하나다.
  - `Slack`: `apps.connections.open` → WSS. `tokio-tungstenite`로 하고 envelope마다 ack한다.
- 바깥 주소는 설정으로 받는다. 시험은 이 주소를 로컬 가짜 서버로 돌린다(AGENTS 5번).

**들어오는 것 판정** (원본과 같게):
1. 발신자 ID가 **페어링 승인 목록**에 있으면 통과한다.
2. 없으면 DM일 때만 페어링 요청을 만든다.
   - 8자리, 글자 `ABCDEFGHJKLMNPQRSTUVWXYZ23456789`, 1시간, 대기 3건.
   - 같은 발신자는 10분에 1회(H).
   - 코드는 요청이 새로 생길 때만 답한다(O).
3. 그룹 · 채널 메시지는 무시한다.
4. 페어링 승인은 **그 컴퓨터의 화면이나 `price-inha pairing approve <채널> <코드>`**에서 한다. 원격 세션으로는 하지 않는다(키 등록과 같은 급).
5. 승인 실패 5회면 1시간 잠근다(H).
6. 승인 목록은 저장소 표에 둔다. 코드는 기록에 남기지 않는다.

**나가는 메시지 종류** (notify 종류에 맞춘다):

| 종류 | 기본 | 내용 |
|---|---|---|
| `ApprovalRequest` | 켬 | 종목 · 방향 · 수량 · 가격 상한 · 금액 · 만료 시각 · 지문 앞 8자 · (바깥 주소가 있으면) 카드 링크. 버튼(5 아래 규칙) |
| `Stall` · `Anomaly` | 켬 | 멈춤 · 오류 · 큰 손실(R4-2) |
| `AccessLock` | 켬 | 접속 키 잠금(R13-3) |
| `BackupFailure` | 켬 | 백업 실패 |
| `DailySummary` | 켬 | 하루 정리(긴 글은 나눔) |
| 승인 결과 | 켬 | "승인됨/거절됨/만료됨". 이전 카드는 편집해 버튼을 지운다(O `terminalizeApprovalMessage`) |
| 팀 대화 · 사무실 피드 | 끔(선택) | 에이전트 대화 요약. 양이 많아 기본은 끈다 [추정] |

- 모든 메신저 글은 **E38 가리기 → 실패하면 보내지 않음** 순서를 거친다.
- 길이는 텔레그램 4000 · 슬랙 8000으로 문단 경계에서 나눈다.
- 텔레그램 429는 `retry_after`를 지킨다. 대기열은 다시 보내기 최대 5회(O)로 두고 메모리 상한을 둔다.

**메신저로 받을 명령** (페어링된 본인만):

| 명령 | 효과 | 재인증 |
|---|---|---|
| `/status` | 팀 상태 · 오늘 손익 · 대기 승인 수 | 없음(읽기) |
| `/summary` | 하루 정리 | 없음 |
| `/approvals` | 대기 승인 목록 | 없음 |
| `/reject <id>`, 거절 버튼 | 거절 | 없음. 위험 줄이기는 바로(P18 · `remote-access-security.md` 5.3) |
| `/pause`(신규 진입 멈춤) · 킬 스위치 켜기 | 멈춤 | 없음(위험 줄이기) |
| `/approve <id>`, 승인 버튼 | 5 아래 규칙 | 규칙 참고 |
| 재개 · 규칙 개선 · 실전 승격 · 키 변경 · 페어링 · 설정 | **메신저에서 불가**. "대시보드에서 하세요" 안내 | — |

모르는 명령은 도움말을 보여 준다. 대화는 공지처럼 사무실로 넘길 수 있지만 규칙 · 승인 길은 타지 않는다(E46과 같은 원칙) [추정].

**메신저에서 승인 받을 때의 안전 규칙**

지금 웹 승인의 정본은 다음 셋이다.
- ① 세션
- ② 화면에 보인 지문 = 레코드 지문
- ③ 상태 전이 1회, 모르는 값은 거절(P19)

메신저 탭에는 ①이 없다. 신원은 텔레그램 · 슬랙 계정에 기댄다.

권장안: **두 단계로 나눈다.**

1. **기본(처음 만드는 것): 메신저는 알림 + 거절만 한다.** 승인은 카드 링크로 연 웹 화면에서 지금 방식(세션 + 지문)으로 한다.
   - 까닭 ①: 돈이 움직이는 승인(P18)의 정본 경로를 하나로 둔다.
   - 까닭 ②: 메신저 계정 탈취, 텔레그램 클라우드 대화 보관, 봇 토큰 유출이 승인 위조로 이어지지 않는다.
   - 까닭 ③: 승인 경로가 막혀도 P20(답 없음 → 보수적)이 이미 있다.
   - 원본 OpenClaw도 문서에서 `operator.approvals`를 "원격 실행급 권한"으로 다루라고 적는다.
2. **선택(사용자가 켜는 것): 메신저 버튼 승인.** 켤 때 모두 지킨다.
   - (a) **주문 승인(한도 넘는 주문)에만** 쓴다. 재개 · 규칙 개선 · 실전 승격은 언제나 웹에서만 한다.
   - (b) 승인자는 **설정된 숫자 user ID 하나**다. `allowFrom`(말할 수 있음)과 따로 둔다(O `rich-messages.md:194`). Hermes 기본값 `*`는 따르지 않는다(의도해서 다름: P18 "본인").
   - (c) 버튼 데이터는 `승인 번호 + 지문 앞 16자(hex) + 결정`이다. 텔레그램 64바이트 안이다. 서버는 **레코드 지문의 앞 16자와 같을 때만** 받는다. 메시지에 보인 내용과 지문이 같은 레코드에서 나왔음을 묶는다(지금 C7 입력 "화면에 보인 내용의 지문"과 같은 뜻).
   - (d) DM에서만 받는다. 그룹이나 채널에서 온 탭은 무시한다.
   - (e) 상태 전이는 1회다. 먼저 온 답이 이긴다(O). 늦은 탭은 "이미 처리됨"을 받고 버튼이 지워진다.
   - (f) 만료는 지금 승인 레코드의 만료를 그대로 쓴다. 메신저용으로 따로 늘리지 않는다.
   - (g) 모르는 결정, 형식이 틀린 데이터, 승인자가 아닌 사람의 탭은 **거절도 아니고 무시**다. 기록은 남긴다(O 텔레그램 방식). 승인 확인 실패를 "답 없음"으로 바꾸지 않는다(`remote-access-security.md` 5.4).
   - (h) 실행 직전 재확인은 지금과 같다(저장된 계획만 실행).
   - (i) 사무실과 하루 정리에 "어느 경로로 승인됐는지(웹/텔레그램/슬랙)"를 남긴다.
   - 모의계좌뿐이라 실제 손실은 없다. 그래도 P18은 지킨다.

남는 위험 [추정]: 메신저 계정이 뚫리면 (2)를 켠 경우 한도 넘는 주문이 승인될 수 있다. P1~P8은 승인과 무관하게 강제되므로 피해는 금액 한도 위쪽의 정해진 범위로 묶인다.

### (라) 대시보드에서 관리할 설정

이미 있는 것: 키 · 종목 · AI 연결 · 소리 · 백업 · 뜻 검색 · 규칙 · 템플릿 · 접속 키.

더할 것:
1. **알림**:
   - 조용한 시간(시작 · 끝 · 시간대). 지금 `None` 고정을 푼다.
   - 종류별 전달처 {웹 푸시, 텔레그램, 슬랙} 켜고 끄기.
   - 잠금 화면 상세도(최소/보통). O `notifications.md:47`과 같은 모양.
2. **채널**: 텔레그램과 슬랙 각각 다음을 둔다.
   - 켜기 · 끄기, 토큰 등록 · 교체(그 컴퓨터에서만), 연결 상태
   - 페어링된 사람(ID · 이름 · 해제), 대기 중인 페어링 요청(승인 · 무시, 그 컴퓨터에서만)
3. **메신저 승인**: 끔(기본) / 주문 승인 버튼 켬. 승인자 ID 하나. 켤 때 경고 문구를 보여 주고 그 컴퓨터에서만 바꿀 수 있다.
4. **바깥 주소**(선택): 메신저 카드 링크에 쓸 주소. 비밀이 아니다.
5. **팀 대화를 메신저로**: 끔 / 요약만.

### (마) 차수 계획

파이프라인은 차수마다 같다: red → green → verify.sh → AI 검토 → 사람 확인.

| 차수 | 모듈 | 내용 | 의존 | 시험으로 확인할 것 | 기계 e2e |
|---|---|---|---|---|---|
| A | docs | PRD "범위 밖" 줄 고치기, R(메신저 · 설치) 요구 · 스펙 FN · dev C/E/V 번호, D2 · D4와 맞추기, 결정 기록 | 사용자 결정 1 · 3 | — | 문서 검사(지금 verify.sh의 문서 규칙) |
| B | server(cli), 스크립트 | `price-inha setup`(단계 0 · 1 · 2 · 3 · 5 · 6), `open`. 마법사는 루프백 경로를 부른다 | A | 비대화형 종료 코드 · 안내, 가짜 CLI로 탐지 분기 4개, 가려진 입력이 기록 · 출력에 없음, 등록 전 확인 실패 시 저장 안 함 | 가짜 표준 입력으로 setup 전체를 돌린 뒤 `/api/setup` 상태가 "갖춰짐" |
| C | 릴리스 · install.sh | `install-and-cli.md` 8.2 · 8.4 | B | 체크섬 불일치면 중단, `--no-modify-path`, 터미널이 없으면 setup 건너뛰기 | 로컬 HTTP로 가짜 릴리스를 내려 주고 `sh install.sh --dry-run`과 실제 설치를 임시 HOME에서 |
| D | channel(새), secrets | 채널 trait, 텔레그램 long polling · 보내기 · 편집 · 나누기, 키 종류 · `getMe` 확인, 페어링 저장소 · 규칙(8자리 · 1시간 · 3건 · 10분 · 5회 잠금) | A | 가짜 Bot API 서버로 받기 · 보내기 · 429 retry_after · 4000자 나누기, 페어링 경계값, 그룹 무시, 토큰이 기록 · 오류 글에 없음 | 가짜 텔레그램 서버 + 서버 띄우기 → DM → 페어링 코드 → `pairing approve` → `/status` 답 |
| E | notify, server(reach) | 종류별 전달처, 조용한 시간 설정 화면 · 경로, 메신저 글 가리기(실패 시 안 보냄), 승인 결과 편집 | D | 조용한 시간에 승인 외 종류는 안 보냄, 가리기 실패 → 보내지 않음, 길이 나누기 | 승인 요청 → 가짜 텔레그램에 카드 · 링크 도착, 웹에서 승인 → 카드 편집 |
| F | channel, server | 메신저 명령(읽기 · 거절 · 멈춤), 위험 키우기 명령은 거부 안내 | E | 페어링 안 된 발신자 무시, 거절 즉시, 재개 명령 거부 | 가짜 서버로 `/reject` → 승인 레코드 거절 |
| G | channel, approval, server | (선택) 메신저 버튼 승인: (a)~(i) | F, 사용자 결정 5 | 승인자 아닌 ID · 그룹 · 지문 불일치 · 만료 · 두 번 탭 · 모르는 결정 → 모두 처리 없음 · 기록. 켜짐 · 꺼짐 기본값 | 가짜 서버 callback_query로 승인 → 모의계좌(`pi-paper`) 체결, 같은 탭 재전송 → 처리 없음 |
| H | channel | 슬랙 Socket Mode(가짜 WSS 서버), Block Kit 버튼, 8000자 | D~G | envelope ack, 재연결, 버튼 값 디코드 실패 → 무시 | 가짜 슬랙 WSS로 D~G e2e 되풀이 |
| I | server(cli) | doctor 9(채널 등록 · 페어링 수), setup 섹션 4(메신저) 터미널 | D, H | 이름만 보이고 값은 없음 | setup 메신저 섹션 e2e |

- 의존 순서: A → B → (C ‖ D) → E → F → G → H → I. H는 D 직후에 당겨도 된다.
- 모든 시험은 로컬 가짜 서버만 쓴다. 실제 텔레그램 · 슬랙 · Anthropic은 부르지 않는다.

### (바) 사용자가 정할 것 (권장안 포함)

1. **PRD 범위 바꾸기**: "텔레그램 같은 메신저 연동은 하지 않는다"(`prd.md:414`)를 지우고 메신저 요구를 넣는가. **권장: 넣는다.** 텔레그램과 슬랙만, 1인용.
2. **Codex(ChatGPT) 로그인**을 지원하는가. **권장: 보류.** OpenAI 약관 확인 뒤 정한다. Claude CLI와 API 키로 시작한다.
3. **`setup` 마법사 도입**: 앞선 결정("주소만 연다")을 바꾸는가. **권장: 바꾼다.** 화면과 같은 저장 길을 쓰는 얇은 터미널 표면으로 만든다.
4. **메신저로 말할 수 있는 사람 수**: 1명(본인)으로 고정할지, 여러 명 페어링을 허용할지. **권장: 1명.** PRD "나 혼자만 쓴다"에 맞는다.
5. **메신저에서 돈이 움직이는 승인을 받을지**: (가) 받지 않음(알림 · 거절 · 링크만) (나) 사용자가 켜면 주문 승인만 버튼으로, 5(다) 규칙 적용. **권장: (가)를 기본으로 만들고 (나)는 G차수에서 선택 기능으로.**
6. **새 모듈 `channel` 추가**: architecture.toml · graph.toml에 모듈을 더하는 것은 "규칙을 고치지 않는다"(AGENTS 7번)와 부딪힐 수 있다. 사람이 고칠지 정한다.
7. **바깥 주소 칸**: 메신저 카드 링크용 바깥 주소(Tailscale 등 사용자 몫)를 설정에 둘지. D2는 Tailscale을 스펙에서 뺐다. **권장: 비밀이 아닌 선택 칸으로만 둔다.**

---

## 6. 확인하지 못한 것

- OpenClaw 온보딩 코드의 단계 순서: 문서(`wizard.md`)로만 봤고, `src/wizard/setup.*.ts`는 파일 목록만 봤다.
- OpenClaw의 일반 나가는 글 비밀 가리기와 Hermes의 최종 답 가리기: 둘 다 못 봄.
- Hermes 문서에 적힌 동의 단어 "ok/go"의 코드 근거: 못 봄.
- OpenAI(Codex) 약관과 Telegram · Slack 이용 정책 중 봇 자동 메시지 관련 조항: 읽지 않음.
- 슬랙 Socket Mode를 `tokio-tungstenite`만으로 구현할 때의 재연결 규칙(`disconnect` envelope 등): 공식 문서 확인 필요.
- 하위 조사 보고의 줄 번호 가운데 "확인함" 표시가 없는 것은 다시 열어 보지 않았다.
- 받은 저장소가 약 1.5GB로 지시(500MB)를 넘었고, 지우기는 권한에서 막혔다(2.0).
