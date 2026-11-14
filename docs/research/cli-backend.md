# CLI 백엔드 조사: 공식 Claude Code CLI를 불러 구독으로 모델 쓰기

결론: **OpenClaw에는 이 기능이 있고(`claude-cli` 백엔드), Hermes에는 없다.** 가격인하에는 OpenClaw의 "도구 없는 한 번 호출" 인자 묶음을 그대로 가져와 `pi-ai`의 두 번째 공급자로 얹을 수 있다. 다만 약관에 사용자가 직접 판단해야 할 문장이 셋 있다(4.5).

| 항목 | 내용 |
|---|---|
| 조사일 | 2026-11-14 |
| OpenClaw | `openclaw/openclaw` 커밋 `c19ae286a`(2026-11-14), 버전 2026.11.3, MIT. 아래 `O:` |
| Hermes Agent | `NousResearch/hermes-agent` 커밋 `5c1abd25`(2026-11-13), MIT. 아래 `H:` |
| 이 컴퓨터의 CLI | `claude` 2.1.316 (`~/.local/bin/claude`, Mach-O arm64, 310MB). `claude --version` · `--help` · `auth --help` · `setup-token --help` · `auth status --help`만 실행했다. 모델 호출 · 로그인 · 설정 변경은 하지 않았다 |
| 공식 문서 | `code.claude.com/docs/en/`의 `headless` · `cli-reference` · `authentication` · `legal-and-compliance` · `env-vars` · `errors` · `agent-sdk/typescript`, `support.claude.com` 글 1개, `anthropic.com/legal`의 약관 2개. 모두 2026-11-14에 받았다 |
| 표기 | [관찰] = 소스 · 문서 · 도움말에서 직접 확인. [추정] = 해석 · 판단. 근거는 저장소 루트 기준 `경로:줄` |
| 옮기지 않은 것 | 원본 코드. 구조 · 흐름 · 인자 이름 · 설정 모양만 적었다 |

### 사용한 rea 명령

REA CLI v6.7.0. 큰 저장소를 통째로 넣지 않고, 고른 파일만 경로를 유지해 빈 폴더에 복사한 뒤 그 폴더를 루트로 분석했다. 출력은 저장소 밖 임시 폴더에 두었다.

```bash
rea analyze-javascript-application . --artifact-format directory \
  --source-path <파일> [--source-path <파일> ...] --max-heap-mb 1024 --format json
```

| # | 대상 | 파일 | 시간 | 최대 RSS | 결과 |
|---|---|---|---|---|---|
| A | 플러그인 쪽: `O:extensions/anthropic/`의 `cli-backend` · `cli-shared` · `cli-transport` · `cli.runtime` · `cli-process` · `cli-output` · `cli-runtime-args` · `cli-constants` · `cli-version` · `cli-auth-seam` · `cli-user-input` · `cli-runtime-status` · `cli-runtime-tasks` · `cli-migration` · `claude-code-version-requirement` · `cli-model-discovery` | 16 | 4.7초 | 307MB | 성공. 16개 모두 해석, 노드 8,652, 확정된 호출 107(함수→함수 94) |
| B | 코어 쪽: `O:src/agents/cli-runner.ts` · `cli-backends.ts`, `O:src/agents/cli-runner/`의 `prepare` · `execute` · `execute-process` · `execute-plugin` · `execute-plugin-watchdog` · `helpers` · `exit-error` · `reliability` · `no-output-timeout-policy` · `execution-env` · `bundle-mcp` · `bundle-mcp-claude` · `cli-backend-auth-policy` · `cli-live-session-registry` · `cleanup` · `cli-run-recovery` · `types` · `prepare-tool-policy`, `O:src/plugins/cli-backend.types.ts` · `cli-backend-errors.ts` · `cli-backends.runtime.ts` | 23 | 7.1초 | 435MB | 성공. 23개 모두 해석, 노드 19,919, 확정된 호출 185(함수→함수 136) |

**REA로 본 것** (파일 안 호출 관계):

- 인자 조립의 줄기: `resolveClaudeCliExecutionArgs` → `normalizeClaudeBackendArgs` · `resolveClaudeCliSideQuestionExecutionArgs` · `resolveClaudeCliRestrictedExecutionArgs` · `applyClaudeCliEffortArgs` · `applyClaudeCliFastModeArgs` · `stripClaudeArgs` · `readClaudeToolDenials` (`O:extensions/anthropic/cli-shared.ts:438` 에서 뻗는다).
- 자격 전달: `prepare` → `resolveClaudeCliAuthInput` → `createClaudeCliAuthInput` (`cli-backend.ts:306 → 142 → 110`).
- 로그인 확인: `probeClaudeCliAuthStatus` → `readPreservedCliEnv` (`cli-auth-seam.ts:44 → 28`).
- 오래 사는 프로세스: `executeClaudeCli` → `createSession`, `createClaudeCliTransport` → `read` → `acceptLine` → `dispatch` → `send`, `close` → `beginShutdown` → `abortRequests` (`cli.runtime.ts:596`, `cli-transport.ts:24`).
- 코어: `runQueuedAttempt` → `executeAttempt`, `executeCliProcess` → `parseOutput` · `createOutputFailoverError`, `createCliExitFailoverError` → `resolveCliResumeAtError` · `createCliFailoverError`, `injectClaudeMcpConfigArgs` → `mergeClaudeDisallowedTools`.

**REA가 주지 못한 것**: 묶음 밖 모듈로 넘어가는 호출(가져오기 대상이 묶음에 없으면 이어지지 않는다), 인자 값 · 조건 분기의 뜻. 그래서 아래 내용은 REA가 짚어 준 함수와 문서를 **직접 읽어** 적었다. Hermes(Python)는 호출 관계가 나오지 않아 처음부터 직접 읽었다.

**직접 읽은 범위**: `O:docs/gateway/cli-backends.md`(1,040줄 가운데 1~505 · 555~1040), `O:docs/providers/anthropic.md`(74~94 · 196~330), `O:extensions/anthropic/`의 `cli-backend.ts` · `cli-shared.ts` · `cli-constants.ts` · `cli-auth-seam.ts` 전부와 `cli-output.ts` · `cli-runtime-args.ts` · `cli.runtime.ts` 일부, `O:src/agents/cli-runner/`의 `helpers.ts` · `execute.ts` · `execute-process.ts` · `exit-error.ts` · `reliability.ts` 일부, `O:src/agents/cli-output.ts` · `cli-output-records.ts` 일부. Hermes는 `H:agent/acp_client.py`(50~562), `H:hermes_cli/auth_external_process.py` 전부, `H:agent/anthropic_adapter.py` · `H:hermes_cli/auth.py` · `H:website/docs/integrations/providers.md` 일부.

---

## 1. 한눈에 비교

| 물음 | OpenClaw | Hermes |
|---|---|---|
| CLI 백엔드가 있는가 | **있다.** `claude-cli`(Claude Code), `google-gemini-cli`(Gemini CLI). `codex-cli`는 없다 | **없다.** `claude -p`를 부르는 코드가 없다 |
| 가장 가까운 것 | — | ① 외부 에이전트 CLI를 ACP(stdio JSON-RPC)로 부르는 범용 클라이언트(`external_process` 공급자). ② 구독 OAuth 토큰으로 Messages API를 직접 부르는 길(가격인하가 만들지 않기로 한 길) |
| 프로세스 | 에이전트 턴은 오래 사는 프로세스 하나에 계속 말을 건다(10분 놀면 닫음). 곁가지 한 번 호출은 매번 새 프로세스 | 요청마다 새 프로세스 |
| 대화 이어 가기 | CLI 세션에 맡긴다(`--session-id` · `--resume`) | 맡기지 않는다. 대화 전체를 글 하나로 만들어 매번 통째로 보낸다 |
| 자체 도구 | 기본은 꺼 두고 자기 도구를 MCP로 넘긴다. 한 번 호출은 전부 끈다 | 권한 요청은 모두 취소로 답한다. 파일 읽기 · 쓰기는 작업 폴더 안만 |
| 인증 | CLI의 로그인에 기댄다(토큰을 읽지도 저장하지도 않는다). 저장해 둔 토큰은 파일 서술자로 넘긴다 | CLI의 로그인에 기댄다 |
| 토큰 수 | 결과 줄의 `usage`에서 읽는다 | 0으로 채운다 |

---

## 2. OpenClaw

### 2.1 있는가, 무엇을 부르는가

- [관찰] 문서의 정의: "OpenClaw can run agent turns through a local AI CLI" — `O:docs/gateway/cli-backends.md:10`.
- [관찰] 묶어서 내는 백엔드는 둘이다. Anthropic 플러그인이 `claude-cli`를, Google 플러그인이 `google-gemini-cli`를 등록한다. Codex는 CLI 백엔드가 아니라 app-server 방식이고 "There is no bundled `codex-cli` backend" — `cli-backends.md:711`.
- [관찰] 실행 파일은 `claude`라는 이름을 게이트웨이 서비스의 `PATH`에서 찾는다. 다른 경로 · 인자가 필요하면 설정 파일이 아니라 플러그인 등록에서 바꾸게 한다 — `cli-backends.md:51-54`, `O:extensions/anthropic/cli-backend.ts:268`.
- [관찰] 버전을 한 번 확인해 인자를 고른다. 2.1.98 이상이면 `--exclude-dynamic-system-prompt-sections`를 더한다. 확인에 실패하면 옛 인자 그대로 쓴다 — `cli-shared.ts:19-23, 102-122`, `cli-backends.md:730-733`.

### 2.2 한 번 부르는 방법

**기본 인자** [관찰] `cli-backend.ts:44-56`:

```text
claude -p --output-format stream-json --include-partial-messages --verbose
  --setting-sources user --allowedTools mcp__openclaw__*
  --disallowedTools ScheduleWakeup,CronCreate,CronList,Bash(run_in_background:true),Monitor
```

| 넘기는 것 | 방법 | 근거 |
|---|---|---|
| 프롬프트 | **표준 입력** (`input: "stdin"`). 다른 백엔드는 마지막 인자로 주고, 길면 표준 입력으로 바꾼다 | `cli-backend.ts:277`, `O:src/agents/cli-runner/helpers.ts:220-231` |
| 모델 | `--model <이름>`. 별칭 표로 바꾼다(`opus` · `sonnet` · `haiku` · `fable`). 1M 문맥은 이름 뒤에 `[1m]` | `cli-backend.ts:278-279, 296-297`, `cli-constants.ts:91-98` |
| 시스템 글 | 임시 파일에 써서 `--append-system-prompt-file <경로>`. 오래 사는 프로세스의 에이전트 턴은 **덧붙이지 않고 통째로 바꾼다**(`--system-prompt-file`). 끝나면 임시 파일을 지운다 | `cli-backend.ts:287-289, 235`, `helpers.ts:286-309, 413-430`, `cli-backends.md:382-386` |
| 세션 | 새 대화는 `--session-id <UUID>`, 이어 가기는 `--resume <세션 ID>` | `cli-backend.ts:270, 282-283`, `helpers.ts:431-435` |
| 권한 모드 | 기본은 넘기지 않는다(CLI 기본값). 옛 `--dangerously-skip-permissions`는 지운다. 자기 exec 정책이 "전부 허용"일 때만 `--permission-mode bypassPermissions` | `cli-shared.ts:136-191`, `cli-backends.md:446` |
| 설정 출처 | 항상 `--setting-sources user`로 좁힌다(프로젝트 · 로컬 설정을 읽지 않는다) | `cli-shared.ts:37, 158, 179-181` |
| 생각 세기 | `--effort <low…max>`, 그리고 환경 변수 `MAX_THINKING_TOKENS` | `cli-shared.ts:72-95, 193-213` |
| 작업 폴더 | 에이전트의 워크스페이스 폴더 | `execute-process.ts:446` |
| 이미지 | 임시 파일로 써서 프롬프트 끝에 `@경로` | `helpers.ts:371-384` |

**환경 변수** [관찰]:

- 지우는 것(자식에게 넘기지 않는다): `ANTHROPIC_API_KEY` · `ANTHROPIC_AUTH_TOKEN` · `ANTHROPIC_BASE_URL` · `ANTHROPIC_OAUTH_TOKEN` · `CLAUDE_CODE_OAUTH_TOKEN` · `CLAUDE_CODE_USE_BEDROCK` · `CLAUDE_CODE_USE_VERTEX` · `CLAUDE_CODE_USE_FOUNDRY` · `OTEL_*` 등 41개. 까닭은 주석에 있다: 셸에서 물려받은 값이 다른 공급자 · 주소 · 토큰으로 호출을 돌리지 못하게 한다. **`CLAUDE_CONFIG_DIR`는 지우지 않는다**(로그인이 거기 있다) — `cli-constants.ts:14-65`.
- 넣는 것: `CLAUDE_CODE_DISABLE_GIT_INSTRUCTIONS=1`, `ENABLE_TOOL_SEARCH`, `CLAUDE_CODE_AUTO_COMPACT_WINDOW`, 200K일 때 `CLAUDE_CODE_DISABLE_1M_CONTEXT=1` — `cli-backend.ts:319-337`.

**프로세스 수명** [관찰]:

- 에이전트 턴: Claude Code의 구조화된 stdio 규약(`stream-json` 입출력, `control_request`의 `initialize` · `can_use_tool`)으로 **오래 사는 프로세스 하나**에 말을 건다. 모델 · 시스템 글 · 도구 구성이 바뀌면 새로 띄운다. 놀고 있으면 10분 뒤 닫는다. 게이트웨이를 끌 때 프로세스 트리까지 정리한다 — `cli-backends.md:377-391`, `cli.runtime.ts:22, 345-348`, `cli-transport.ts:251-258, 315-319`.
- 한 번 호출(세션 제목 · 요약 같은 "side question"): **매번 새 프로세스**, 도구 없음, 세션 저장 없음 — `cli-backends.md:59`, `cli-shared.ts:355-374`.

### 2.3 대화 이어 가기

- [관찰] `sessionMode: "always"` — 저장된 세션 ID가 없으면 UUID를 만들어 `--session-id`로 보내고, 있으면 `--resume`으로 보낸다. 출력의 `session_id` · `sessionId` · `conversation_id` · `conversationId` 가운데 있는 것을 세션 ID로 읽어 대화별로 저장한다 — `helpers.ts:209-218`, `cli-constants.ts:100-105`, `cli-backends.md:143, 557-563`.
- [관찰] **어긋날 때**: 이어 가기 전에 저장된 세션 ID의 대화 기록 파일이 실제로 읽히는지 본다. 없으면 묶음을 지우고(`reason=transcript-missing`) 새로 시작한다. 인증 계정이 바뀌거나 MCP 도구 이름이 바뀌어도 새 세션으로 간다(`reason=prompt-tools`) — `cli-backends.md:563, 637-641`.
- [관찰] 새 세션은 자기 쪽 대화 기록에서 다시 채운다. 채우는 글은 12,288자(약 3,000토큰)에서 자르고, Claude는 문맥 창 크기에 맞춰 더 준다 — `cli-backends.md:567, 990-994`.
- [관찰] 이어 가는 세션이 못 본 새 줄(예약 작업 결과 등)은 최근 8개, 줄마다 2,000자까지 인용 글로 앞에 붙인다 — `cli-backends.md:598-603`.
- [관찰] **문맥이 길어지면** CLI가 스스로 압축한다(`ownsNativeCompaction: true`). OpenClaw는 자기 요약기를 돌리지 않고 `CLAUDE_CODE_AUTO_COMPACT_WINDOW`로 창 크기만 알려 준다 — `cli-backend.ts:226`, `cli-backends.md:786-790`.
- [관찰] 자기 대화 기록은 따로 계속 쓴다(SQLite). 즉 **기록은 양쪽에 다 있고, CLI 세션이 문맥의 주인**이다 — `cli-backends.md:566-567`.

### 2.4 출력 읽기

- [관찰] 형식은 `stream-json`(줄마다 JSON). 마지막 `type: "result"` 줄의 `result` 글자를 응답으로 삼는다. `result`가 다시 JSON으로 감싸져 있으면 8겹까지 벗긴다 — `O:src/agents/cli-output-records.ts:179-190, 517-545`.
- [관찰] 토큰 수는 `usage`(또는 `message.usage`)에서 읽는다. 입력 · 출력 · 캐시 읽기 · 캐시 쓰기로 나눈다 — `cli-output-records.ts:123-150`.
- [관찰] 오류 판정: `is_error: true`, `subtype`이 `error_`로 시작, `status: "error"`, `type: "error"`, 또는 어시스턴트 글이 `API Error:`로 시작 — `cli-output-records.ts:192-229, 476-489`.
- [관찰] `terminal_reason`이 `max_turns` · `hook_stopped` · `budget_exhausted` 등이면 "CLI가 일부러 멈췄다"로 보고 **다른 모델로 다시 돌리지 않는다**(도구가 이미 실행됐을 수 있다) — `cli-backends.md:784`, `O:src/agents/cli-output.ts:26-66`.
- [관찰] 결과 글에 도구 호출 표식(`<invoke …>` · `<parameter …>`)이 날것으로 섞이면 형식 오류로 버린다 — `cli-output.ts:208-231`(`extensions/anthropic`).
- [관찰] 상한: 한 줄 8MiB, 턴 출력 8MiB. 넘으면 그 턴을 멈춘다. `stream-json`에서 `result` 줄이 끝내 없으면 오류다 — `cli-transport.ts:11, 278-279`, `cli-backends.md:683-694, 1004-1007`.
- [관찰] 스트리밍은 `--include-partial-messages`의 조각을 화면으로 흘린다. 비용(`total_cost_usd`)을 읽는 곳은 읽은 범위에서 찾지 못했다.

### 2.5 도구

- [관찰] 원칙: "OpenClaw does not inject tool calls into the CLI backend protocol." 자기 도구는 **루프백 HTTP MCP 서버**로 내놓고, 호출마다 만든 MCP 설정 파일을 `--strict-mcp-config --mcp-config <경로>`로 넘긴다. 실행마다 다른 토큰(`OPENCLAW_MCP_TOKEN`)으로 인증한다 — `cli-backends.md:12, 816-833`, `O:src/agents/cli-runner/bundle-mcp-claude.ts:71-80`.
- [관찰] CLI 자체 도구: 관리되는 턴에서는 파일 · 셸 도구를 주지 않는다. `Bash`는 `--disallowedTools`에 더해 끈다. 남겨 둘 수 있는 것은 `Agent` · `Workflow` · `WebSearch` 셋뿐이고 기본은 `WebSearch` 하나다. `WebFetch`는 "게이트웨이 컴퓨터에서 주소를 가져오므로" 주지 않는다 — `cli-backend.ts:83-108`, `cli-shared.ts:443-451`, `cli-backends.md:193-227`.
- [관찰] **전부 끄는 인자 묶음**(곁가지 한 번 호출) — `cli-shared.ts:355-374`:

```text
--safe-mode --tools "" --disallowedTools mcp__* --strict-mcp-config
--no-session-persistence --max-turns 1 --permission-mode default
```

  그 전에 원래 인자에서 `--allowedTools` · `--disallowedTools` · `--tools` · `--mcp-config` · `--session-id` · `--resume` · `--continue` · `--fork-session` · `--bare` 등을 걷어 낸다(`cli-shared.ts:247-315`).

- [관찰] **도구 목록을 정확히 제한하는 묶음**(예약 작업 등) — `cli-shared.ts:39-40, 395-436`:

```text
--setting-sources "" --settings '{"disableAllHooks":true,"enabledPlugins":{},"autoMemoryEnabled":false,
  "claudeMdExcludes":["**/CLAUDE.md","**/CLAUDE.local.md","**/.claude/rules/**"]}'
--disable-slash-commands --no-chrome --strict-mcp-config --tools "<허용한 자체 도구>"
[--allowedTools mcp__openclaw__<도구>,…] [--disallowedTools mcp__*]
```

- [관찰] `--bare`를 쓰지 않는 까닭이 주석에 있다: "Safe mode also suppresses explicit MCP, while bare mode drops OAuth" — `cli-shared.ts:406-408`. 구독 로그인을 쓰려면 `--bare`는 안 된다.

### 2.6 인증

- [관찰] 기본은 **CLI의 로그인에 그냥 기댄다**: "Claude Code owns its existing local login and subscription. … It never reads, persists, refreshes, or forwards native tokens, or sends synthesized Anthropic API requests." 자기 설정에는 비밀이 아닌 표식 하나만 둔다 — `cli-backends.md:377-381`, `cli-constants.ts:11-12`.
- [관찰] 로그인 확인은 `claude auth status --json`을 10초 제한으로 돌려 `loggedIn` · `authMethod` · `subscriptionType`(`pro` · `max` · `team` · `enterprise`)을 읽는다. 결과는 `available` · `missing` · `not-installed`(`ENOENT`) · `timeout` · `unreadable` 다섯 가지다 — `cli-auth-seam.ts:13-114`.
- [관찰] 주석의 주의: `auth status`는 만료가 가까운 토큰을 갱신하면서 잠금을 잡는다. 도중에 죽이면 잠금이 남아 다음 호출이 멈추거나 실패한다. 그래서 호출한 쪽이 취소돼도 끝까지 기다린다 — `cli-auth-seam.ts:64-66`.
- [관찰] 안내 문구: 로그인이 없으면 "Run `claude auth login` first", 설치가 없으면 "Install Claude Code or add it to PATH" — `O:extensions/anthropic/auth.runtime.ts:278-286`.
- [관찰] 전용 로그인: 게이트웨이 환경에 `CLAUDE_CONFIG_DIR`를 주면 그 폴더의 로그인을 쓴다. macOS에서는 폴더마다 키체인 항목이 다르다 — `cli-backends.md:949-983`.
- [관찰] **토큰을 직접 다루는 길도 있다**: 사용자가 `claude setup-token`으로 받은 토큰을 OpenClaw에 저장해 두면, 호출할 때 **파일 서술자 3번**으로 자식에게 넘긴다(`CLAUDE_CODE_OAUTH_TOKEN_FILE_DESCRIPTOR=3`). 환경 변수 값으로는 넘기지 않는다. 이 변수 이름은 공식 `env-vars` 문서에 없다 — `cli-backend.ts:110-187`.
- [관찰] 같은 저장소에 구독 토큰으로 Messages API를 직접 부르는 길도 남아 있다("advertise a Claude Code client version") — `O:docs/providers/anthropic.md:278-289`. 가격인하가 만들지 않기로 한 길이다.

### 2.7 오류 · 시간 초과 · 동시성

| 상황 | 처리 | 근거 |
|---|---|---|
| 끝 코드가 0이 아님 | 표준 오류 · 표준 출력에서 구조화된 오류 글을 찾아 까닭(인증 · 한도 · 문맥 초과 · 모름)을 분류하고 "넘어가기 오류"로 올린다. 아무 활동 없이 끝났으면 다시 시도할 수 있음으로 표시 | `execute-process.ts:561, 636-648`, `exit-error.ts:76-100` |
| `result` 줄이 실패 | 프로세스가 그 뒤에 멈춰도 **그 결과가 우선**이다(시간 초과로 바꿔 부르면 끝난 도구가 다시 돌 수 있다) | `execute-process.ts:552-559` |
| 출력이 깨짐 · 넘침 | 형식 오류로 실패. 잘린 출력은 읽지 않는다 | `execute-process.ts:490-493, 650-656` |
| 무출력 감시 | 전체 제한과 별개. 새 세션은 전체의 0.8배를 3~10분 사이로, 이어 가기는 0.3배를 1~3분 사이로. 걸리면 프로세스를 죽이고 "승인 프롬프트를 기다렸을 수 있다"고 알린다 | `O:src/agents/cli-watchdog-defaults.ts`, `reliability.ts:17-57`, `execute-process.ts:563-621` |
| 전체 시간 초과 | 턴 전체 제한(게이트웨이 기본 48시간, `openclaw agent` 명령은 600초). 넘으면 프로세스와 그 안의 백그라운드 작업을 함께 끝낸다 | `cli-backends.md:303-325, 361` |
| 토큰 갱신 경합 | "another Claude Code process is refreshing it"를 `refresh_contention`으로 따로 분류한다(죽은 토큰과 구분) | `extensions/anthropic/cli-output.ts:11-19` |
| 한도 초과 | 일반 넘어가기 분류기가 글에서 `rate_limit`을 찾는다(읽은 범위 밖 `failover-error.ts`). 설정된 다음 모델로 넘어간다 | `exit-error.ts:88-92`, `cli-backends.md:109` |
| 표준 오류 | 실패 때만 길이를 자르고 가려서 진단으로 붙인다. 성공한 턴의 표준 오류는 기록하지 않는다 | `cli-backends.md:1014-1023` |
| 동시성 | `serialize: true`. 큐 열쇠가 세션 ID(없으면 소유자, 없으면 워크스페이스)라서 **같은 대화는 한 줄로, 다른 대화는 나란히** 돈다. 같은 세션을 이어 갈 때는 남아 있던 앞 프로세스를 갈아치운다 | `cli-backend.ts:291`, `helpers.ts:49-80`, `execute.ts:601-634`, `execute-process.ts:393-436` |
| 남은 프로세스 | 닫을 때 표준 입력을 닫고 0.5초, `SIGTERM` 뒤 1초, 그래도 살아 있으면 `SIGKILL`. 앞 프로세스의 정리가 끝나야 새 세션을 띄운다 | `cli-transport.ts:12-14, 66-94`, `cli-backends.md:938-942` |

전체 동시 실행 수의 상한은 읽은 범위에서 찾지 못했다.

### 2.8 설정

- [관찰] 사용자는 모델 이름은 그대로 두고 **모델별 실행기**를 고른다 — `cli-backends.md:113-131`:

```json5
{ agents: { defaults: {
  model: "anthropic/claude-opus-5-5",
  models: { "anthropic/claude-opus-5-5": { agentRuntime: { id: "claude-cli" } } },
} } }
```

- [관찰] 명령 · 인자 · 환경 · 파싱 · 세션 · 감시는 설정이 아니라 플러그인 코드에 둔다 — `cli-backends.md:133-135`.
- [관찰] 함께 쓰기: API를 1순위로 두고 CLI를 다음 순서(`fallbacks`)에 둘 수 있다. 1순위가 인증 · 한도 · 시간 초과로 실패하면 넘어간다. 같은 모델을 두 길에 동시에 걸 수는 없다 — `cli-backends.md:88-111`.
- [관찰] CLI를 명시해 고른 모델은 실행 파일이 안 돌아도 **API로 몰래 넘어가지 않고 실패한다**("a separate billing choice") — `anthropic.md:239-242`.
- [관찰] 보조 호출(제목 · 요약)은 고른 모델의 실행기를 따른다. API 자격이 있어도 자동으로 API로 바꾸지 않는다 — `cli-backends.md:59-63`.

### 2.9 제약과 경고 (원본 문서의 말)

- [관찰] "CLI backends do not receive OpenClaw tool calls directly" · "Streaming is backend-specific" · "Structured outputs depend on the CLI's own JSON format" — `cli-backends.md:996-1000`.
- [관찰] CLI와 OpenClaw가 같은 컴퓨터, 같은 OS 사용자여야 한다. 컨테이너는 호스트의 `~/.claude`를 물려받지 않는다 — `anthropic.md:255-264`.
- [관찰] 요금: "Agent SDK and `claude -p` usage currently draw from the signed-in subscription's limits. API-key auth … is preferable for shared automation or predictable production spend." 그리고 "Anthropic can change Claude Code billing and rate-limit behavior without an OpenClaw release" — `anthropic.md:78-94, 314-330`.
- [관찰] 오래 사는 프로세스가 닫힌 뒤 다음 턴은 프로세스 시작 시간만큼 늦는다 — `cli-backends.md:20-22`.
- [관찰] CLI를 새 버전으로 유지하라고 한다 — `cli-backends.md:422-429`.

---

## 3. Hermes

### 3.1 CLI 백엔드는 없다

- [관찰] 저장소의 Python 파일 전체에서 `claude -p` · `--print` · `--output-format`으로 Claude Code를 부르는 코드를 찾지 못했다(`stream-json`은 Hermes 자신의 출력 형식 한 곳뿐 — `H:cli.py:1764`).
- [관찰] Hermes가 `claude` 실행 파일을 건드리는 곳은 둘이다. 설치 위치 찾기(`find_claude_code_cli`)와 `claude --version` 읽기. 버전은 구독 OAuth로 Messages API를 직접 부를 때 쓴다 — `H:agent/anthropic_adapter.py:252-307`.
- [관찰] 구독은 **직접 API 길**로만 쓴다. 문서: "the OAuth path routes as Claude Code against your Anthropic account and only works on a Claude Max plan with purchased extra usage credits" — `H:website/docs/integrations/providers.md:146, 173`. `CLAUDE_CODE_OAUTH_TOKEN`도 이 길의 자격 목록에 들어 있다 — `H:hermes_cli/auth.py:203-209`. 가격인하가 만들지 않기로 한 길이다.

### 3.2 가장 가까운 것: ACP로 외부 에이전트 CLI 부르기

| 물음 | 내용 | 근거 |
|---|---|---|
| 무엇인가 | `auth_type = "external_process"`인 공급자. "a spawned CLI that owns its own login". 범용 클라이언트 `ACPClient`가 OpenAI 클라이언트 모양을 흉내 낸다 | `H:hermes_cli/provider_catalog.py:20-33`, `H:agent/acp_client.py:1-13, 270-281` |
| 어떤 CLI | 코어에 든 하위 클래스는 Copilot CLI 하나(`copilot --acp --stdio`). `plugins/` 안에는 이 방식의 공급자 정의가 없다(코어에서 빠져나갔다). Claude Code는 `--acp`가 없어 이 길로 못 부른다 — 오류 문구가 "older release (e.g. Claude Code v2.x)"를 예로 든다 | `H:agent/copilot_acp_client.py:59-65`, `acp_client.py:396-402` |
| 실행 파일 찾기 | `PATH` → 사용자 로컬 bin → Claude Code 설치 위치 8곳(`~/.local/bin` · `~/.claude/local` · `/opt/homebrew/bin` 등). 서비스로 뜨면 `PATH`가 비어 있기 때문이다. `--help`를 5초 제한으로 돌려 `--acp`가 있는지 먼저 본다 | `H:hermes_cli/auth_external_process.py:1-39`, `anthropic_adapter.py:252-258`, `acp_client.py:64-90` |
| 한 번 부르기 | **요청마다 새 프로세스.** `initialize` → `session/new`(`cwd`, `mcpServers: []`) → 모델 선택 → `session/prompt`. 끝나면 프로세스를 끝낸다 | `acp_client.py:425-527` |
| 프롬프트 | 대화 전체를 "역할:\n글" 줄로 이어 붙여 **글 하나**로 보낸다. 앞에 고정 머리말, 뒤에 "Continue the conversation from the latest user request." | `acp_client.py:53-58, 187-202` |
| 모델 | 실행 인자로 주지 않는다(모르는 이름이면 프로세스가 죽는다). 세션을 연 뒤 규약으로 고른다. 실패하면 세션 기본 모델로 계속한다 | `acp_client.py:503-522` |
| 대화 이어 가기 | 하지 않는다. CLI 세션은 요청 하나로 끝나고, 대화는 Hermes가 쥔다 | `acp_client.py:4-6` |
| 출력 | `session/update`의 `agent_message_chunk`를 이어 붙여 글로, `agent_thought_chunk`를 생각으로. 멈춘 이유는 도구 호출이 있으면 `tool_calls`, 없으면 `stop`. **토큰 수는 0** | `acp_client.py:381-391, 537-544` |
| 자기 도구 | MCP로 넘기지 않는다. 도구 목록을 프롬프트 글로 설명하고, 응답 글에서 `<tool_call>{…}</tool_call>` 덩어리를 뽑아 자기 쪽에서 실행한다 | `acp_client.py:56, 193, 381` |
| CLI 자체 도구 | 권한 요청(`session/request_permission`)에는 **항상 `cancelled`**로 답한다. 파일 읽기 · 쓰기 요청은 작업 폴더 안의 절대 경로만 받고, 읽은 내용의 비밀은 가린다 | `acp_client.py:219-264, 548-557` |
| 인증 | CLI의 로그인에 기댄다. 자식 환경은 중앙 정책이 봇 토큰 · GitHub 인증 등을 걷어 낸 것이다. 규약의 인증 필요 오류(-32000)에는 공급자가 정한 안내를 붙인다 | `acp_client.py:107-119, 296-298, 318-321` |
| 시간 초과 | 세션 전체에 예산 하나(기본 900초). 요청마다 새로 주지 않는다 | `acp_client.py:46, 448-450` |
| 일찍 죽음 | 표준 오류 끝 40줄을 모아 "process exited early: …"로 낸다. 죽은 것과 시간 초과를 구분한다(다시 시도 방식이 다르다) | `acp_client.py:431, 479-487` |
| 동시성 | 제한이 없다. 살아 있는 자식을 집합으로 추적해 한 세션의 정리가 다른 세션의 프로세스를 죽이지 않게 한다 | `acp_client.py:342-373` |
| 정리 | `terminate` 뒤 2초 기다리고, 안 되면 `kill` | `acp_client.py:349-356` |

---

## 4. 공식 문서 · 약관에서 확인한 사실

### 4.1 원본이 쓰는 인자가 지금도 공식인가

[관찰] 공식 `cli-reference`와 이 컴퓨터의 `claude --help`(2.1.316)를 대조했다.

| 인자 | 공식 문서 | 이 컴퓨터 도움말 |
|---|---|---|
| `-p` · `--output-format` · `--input-format` · `--include-partial-messages` · `--verbose` · `--model` · `--effort` | 있음 | 있음 |
| `--session-id` · `--resume` · `--fork-session` · `--no-session-persistence` | 있음 | 있음 |
| `--system-prompt` · `--append-system-prompt` | 있음 | 있음 |
| `--system-prompt-file` · `--append-system-prompt-file` | 있음 | **없음** |
| `--tools` · `--allowedTools` · `--disallowedTools` · `--mcp-config` · `--strict-mcp-config` | 있음 | 있음 |
| `--setting-sources` · `--settings` · `--safe-mode` · `--disable-slash-commands` · `--no-chrome` · `--permission-mode` · `--permission-prompts` · `--exclude-dynamic-system-prompt-sections` | 있음 | 있음 |
| `--max-turns` | 있음 | **없음** |
| `--resume-session-at` · `--fast-mode` | **없음** | **없음** |
| `claude auth status` · `claude setup-token` | 있음 | 있음 |

- [관찰] `--resume-session-at`은 OpenClaw 주석도 "hidden print-mode flag"라고 적는다 — `cli-backend.ts:272-274`.
- [관찰] `--permission-mode default`: 도움말 목록에는 `manual`만 나오지만 공식 문서가 "`manual` as an alias for `default` … both values work"라고 한다.
- [관찰] 도움말에 원본이 쓰지 않는 `--restricted`가 있다: 명령 · 코드를 돌리는 내장 도구와 `WebFetch`를 빼고, 사용자 · 프로젝트 · 로컬 설정을 무시하고, `bypassPermissions`를 거부한다.

### 4.2 도구를 끄는 공식 인자

- [관찰] `--tools`: "Restrict which built-in tools Claude can use. Use `""` to disable all … The flag doesn't affect MCP tools; to deny those too, use `--disallowedTools "mcp__*"`" — `cli-reference`.
- [관찰] `--disallowedTools`: "A bare tool name removes the matching tools from Claude's context … `"*"` removes every tool, and `"mcp__*"` removes every MCP tool."
- [관찰] `--strict-mcp-config`: `.mcp.json` · `~/.claude.json` · 플러그인의 MCP 서버를 건너뛰고 `--mcp-config`로 준 것만 쓴다.
- [관찰] `--permission-mode dontAsk`: "denies every call that would otherwise prompt". `--permission-prompts none`: 물어볼 사람이 없을 때 쓰며, 물어야 하는 것은 모두 거부하고 사람 답이 필요한 도구(`AskUserQuestion`)를 뺀다(2.1.259 이상) — `headless`.
- [관찰] `--bare` 없이 `-p`로 돌리면 작업 폴더의 `.claude/settings.json` 훅과 `.mcp.json` 서버가 **신뢰 확인 없이** 돈다 — `headless`의 "Start faster with bare mode".

### 4.3 출력과 종료

- [관찰] `--output-format json`은 결과 하나를 낸다. 글은 `result`, 그 밖에 `session_id` · `usage` · `total_cost_usd` · `stop_reason` · `is_error` · `api_error_status` · `num_turns` · `duration_ms` · `permission_denials` · `terminal_reason`. 실패는 `subtype`이 `error_max_turns` · `error_during_execution` · `error_max_budget_usd` 등 — `agent-sdk/typescript`의 `SDKResultMessage`.
- [관찰] "exits with code 0 on success and a non-zero code when the run fails … When a failure happens inside the run, such as missing authentication, Claude Code prints the failure as the result on stdout." 잘못된 인자는 실행 전에 표준 오류로 알린다 — `headless`.
- [관찰] 표준 입력은 10MB까지. 넘으면 0이 아닌 코드로 끝난다.
- [관찰] `SIGTERM`을 받으면 143으로 끝나고 진행 중이던 턴의 결과를 남기지 않는다. 돌고 있던 Bash 프로세스 트리도 끝낸다.
- [관찰] `stream-json`에서는 다시 시도할 때 `system/api_retry` 줄이 나온다. `error` 값은 `authentication_failed` · `oauth_org_not_allowed` · `account_on_hold` · `billing_error` · `rate_limit` · `overloaded` · `invalid_request` · `model_not_found` · `server_error` · `max_output_tokens` · `cloud_credential_error` · `unknown`. 한도는 `rate_limit_event` 줄(`status: "rejected"`, `resetsAt`, `errorCode: "credits_required"`)로도 온다.
- [관찰] 사람에게 보이는 글: 한도는 "You've hit your session limit · resets 3:45pm"(`weekly` · `Opus` · `Sonnet` 변형), 로그인 없음은 "Not logged in · Please run /login", 만료는 "Login expired · Please run /login" — `errors`.
- [관찰] 출력 토큰 상한을 주는 실행 인자는 없다. 환경 변수 `CLAUDE_CODE_MAX_OUTPUT_TOKENS`가 있다 — `env-vars`.

### 4.4 인증

- [관찰] 자격을 고르는 순서: ① 클라우드 공급자 ② `ANTHROPIC_AUTH_TOKEN` ③ `ANTHROPIC_API_KEY`("In non-interactive mode (`-p`), the key is always used when present") ④ `apiKeyHelper` ⑤ `CLAUDE_CODE_OAUTH_TOKEN` ⑥ Anthropic 프로필 ⑦ `/login`의 구독 OAuth — `authentication`의 "Authentication precedence". **환경에 `ANTHROPIC_API_KEY`가 남아 있으면 구독이 아니라 API 키로 과금된다.**
- [관찰] 저장 위치: macOS는 키체인(잠겨 있으면 `~/.claude/.credentials.json`, 0600), Linux는 `~/.claude/.credentials.json`. `CLAUDE_CONFIG_DIR`를 주면 그 폴더 아래로 옮겨 가고 키체인 항목도 따로 쓴다.
- [관찰] `claude setup-token`: "For CI pipelines, scripts, or other environments where interactive browser login isn't available, generate a one-year OAuth token." 토큰을 터미널에 한 번 찍고 어디에도 저장하지 않는다. `CLAUDE_CODE_OAUTH_TOKEN`으로 준다. "It can only make model requests." Pro · Max · Ultra · Team · Enterprise가 필요하다.
- [관찰] `claude auth status`: "Show authentication status as JSON … Exits with code 0 if logged in, 1 if not." `authMethod`는 `none` · `claude.ai` · `oauth_token` · `api_key` · `api_key_helper` · `third_party`.
- [관찰] **`--bare`는 구독을 쓰지 않는다**: "In bare mode, Claude Code never reads OAuth credentials or the system keychain." `CLAUDE_CODE_OAUTH_TOKEN`도 읽지 않는다.
- [관찰] **예고**: "`--bare` is the recommended mode for scripted and SDK calls, and will become the default for `-p` in a future release" — `headless`. 그때 `-p`에서 구독 로그인을 어떻게 켜는지는 문서에 없다.
- [관찰] 로그인 만료가 3일 안이면 경고만 한다. 만료돼 갱신도 안 되면 요청마다 실패하고, "sessions that run unattended"는 다시 로그인할 때까지 멈춘다.

### 4.5 "다른 프로그램이 공식 CLI를 불러 구독으로 쓰는 것"에 대한 말

**(가) 공식 문서 `legal-and-compliance`의 "Authentication and credential use"** [관찰]. 페이지에 날짜는 없다. 2026년 2월 안내와 같은 글인지는 확인하지 못했다.

| 구분 | 원문의 뜻 |
|---|---|
| OAuth의 쓰임 | "OAuth authentication is intended exclusively for purchasers of Claude Free, Pro, Max, Ultra, Team, and Enterprise subscription plans and is designed to support ordinary use of Claude Code and other native Anthropic applications." |
| 개발자에게 권하는 것 | "Developers building products or services that interact with Claude's capabilities, including those using the Agent SDK, should use API key authentication through Claude Console or a supported cloud provider." |
| **금지** | "Anthropic does not permit third-party developers to offer Claude.ai login into their own applications, or to route requests through Free, Pro, Max, or Ultra plan credentials on behalf of their users. Moreover, developers may not collect, store, or intermediate Claude.ai credentials or session tokens — sign-in to a Claude account must complete through Anthropic's own flow." |
| **막지 않는다고 한 것** | "Nor does it prevent an end user from signing in to the unmodified Claude Code binary with their own Claude subscription, including where a platform hosts Claude Code as described under *Can customers offer Claude Code in their products?* above." |
| 집행 | "Anthropic reserves the right to take measures to enforce these restrictions and may do so without prior notice." |
| 물어볼 곳 | "For questions about permitted authentication methods for your use case, please contact sales." |

**(나) 같은 페이지 "Can customers offer Claude Code in their products?"** [관찰]: 자기 제품에 Claude Code를 미리 깔거나 돌리려면 Commercial Terms에 동의하고 두 조건을 지켜야 한다. ① "The Claude Code binary must not be modified … may not remove, disable, or restrict any authentication method built into it." ② "Customers may not pay for, resell, or intermediate Claude usage on their end users' behalf. Each end user must authenticate with their own Anthropic API key, Claude subscription plan credentials, or 3P inference provider credential."

**(다) 같은 페이지 "Acceptable use"** [관찰]: "Advertised usage limits for Pro, Max, and Ultra plans assume ordinary, individual usage of Claude Code and the Agent SDK."

**(라) 고객센터 글 "Use the Claude Agent SDK with your Claude plan"** [관찰]:
- 2026-10-07: "when you're signed in with your Claude plan, the Claude Agent SDK and `claude -p` still draw from your subscription limits."
- 2026-06-15: "We've paused the previously-announced changes to Claude Agent SDK usage. For now, nothing has changed: Claude Agent SDK, `claude -p`, and third-party app usage still draw from your subscription limits."

**(마) 소비자 약관(Consumer Terms, 시행 2025-10-08, 한국어판) 3조의 금지 목록** [관찰]. Pro · Max 구독에 적용된다.
- "귀하가 Anthropic API 키를 통해 당사 서비스에 접속하거나 당사가 이를 명시적으로 허용하는 경우를 제외하고 봇(bot), 스크립트(script) 또는 기타 방식을 통해 자동화된 또는 비인간적인 방식을 통해 당사 서비스에 접속"
- "당사 서비스, 자료 또는 실행을 신뢰하여 증권을 매매하거나, 증권·상품·파생상품 또는 기타 금융상품이나 금융서비스에 관한 조언을 제공하거나 제공받기 위한 목적으로 이용하는 행위. 당사는 … 브로커-딜러나 등록된 투자자문업자가 아닙니다."

**(바) 상업 약관(Commercial Terms, 시행 2025-06-17)** [관찰]: API 키 · Team · Enterprise에 적용된다. 본문에서 `securities` · `broker` 낱말을 찾지 못했다. 이용 정책(AUP) 본문은 읽지 않았다.

**정리 (사실만)**

| 물음 | 문서의 답 |
|---|---|
| 본인이 자기 구독으로 로그인한, 손대지 않은 공식 CLI를 `-p`로 쓰는 것 | **허용으로 읽히는 문장이 있다**: (가)의 "막지 않는다", (라)의 "`claude -p` … draw from your subscription limits", 4.4의 `setup-token`("CI pipelines, scripts") |
| 다른 프로그램이 Claude.ai 로그인을 대신 받거나, 토큰을 모으고 · 저장하고 · 중개하거나, 사용자 대신 구독 자격으로 요청을 보내는 것 | **금지** — (가) |
| 구독 토큰을 다른 프로그램이 API에 직접 쓰는 것 | (가)의 금지 문장 범위다. 서버에서 막았다는 사실은 이번에 따로 확인하지 못했다 |
| 본인이 만든 프로그램이 로그인된 공식 CLI를 **자식 프로세스로 불러** 쓰는 것 — 정확히 이 경우 | **이 경우를 짚은 문장은 없다.** (가)는 "개발자는 API 키를 써야 한다(should)"와 "최종 사용자가 손대지 않은 CLI에 자기 구독으로 로그인하는 것은 막지 않는다"를 함께 적는다 |
| 24시간 자동으로 돌리는 것 | "ordinary, individual usage"를 전제한다는 문장(다)과 자동화 접속 조항(마)이 있다. 횟수 · 시간 기준은 **말이 없다** |
| 주식 매매 판단에 쓰는 것 | 소비자 약관에 금지 목록 항목이 있다 — (마) 둘째 줄. 상업 약관 본문에는 같은 낱말이 없다 — (바) |

---

## 5. 가격인하에 옮길 것

사정: `pi-ai`의 공급자 trait은 `complete(요청{모델, 시스템 글, 대화, 최대 토큰}) → 응답{글, 멈춘 이유, 토큰 수}` 하나다(`crates/ai/src/types.rs:25-84`). 구현은 Anthropic API 직접 호출뿐이다. 에이전트 실행기는 아직 없다. `docs/product/stock-team/dev.md:25-29`의 D4는 "CLI를 호출마다 띄우기"를 버린 대안으로 적었으므로 고쳐야 한다.

### 5.1 가져올 것 · 줄일 것 · 가져오지 않을 것

| 원본의 방식 | 판정 | 까닭 |
|---|---|---|
| 공식 CLI를 손대지 않고 부르고, 로그인은 CLI에 맡긴다 (O 2.6) | **그대로** | 토큰을 읽지도 저장하지도 않는다. 4.5 (가)의 금지 문장과 "비밀 값을 담지 않는다" 규칙에 맞는다 |
| 도구 없는 한 번 호출의 인자 묶음 (O 2.5) | **그대로** | 우리 호출은 전부 이 모양이다. 공식 인자만으로 된다 |
| 프롬프트를 표준 입력으로 (O 2.2) | **그대로** | 인자 길이 제한이 없고 `ps`에 대화가 보이지 않는다 |
| 물려받은 `ANTHROPIC_*` · `CLAUDE_CODE_USE_*` 지우기 (O 2.2) | **그대로** | 남아 있으면 구독이 아니라 API 키로 과금된다(4.4) |
| `claude auth status --json`으로 로그인 확인, 다섯 가지 상태 (O 2.6) | **그대로** | 모델을 부르지 않는다 |
| 실행 파일을 `PATH` 다음에 설치 위치에서 찾기 (H 3.2) | **그대로** | 24시간 서비스로 뜨면 `PATH`가 비어 있다 |
| 종료: 표준 입력 닫기 → `SIGTERM` → `SIGKILL` (O 2.7) | **그대로** | 남은 프로세스가 없어야 한다 |
| `result` 줄이 실패면 그 뒤 시간 초과보다 우선 (O 2.7) | **그대로** | 오류 종류를 정확히 옮긴다 |
| CLI를 고른 뒤 실패해도 API로 몰래 넘어가지 않기 (O 2.8) | **그대로** | 과금 주체가 바뀌는 일은 사용자가 정한다 |
| 대화 전체를 글 하나로 만들어 매번 통째로 (H 3.2) | **줄여서** | 우리 trait이 이미 대화를 통째로 받는다. 머리말 · 도구 설명 글은 뺀다 |
| 무출력 감시 (O 2.7) | **줄여서** | `--output-format json`은 끝에 한 번만 찍으므로 무출력 감시가 뜻이 없다. 전체 제한 하나만 둔다 |
| 대화별 직렬화 큐 (O 2.7) | **줄여서** | 세션을 이어 가지 않으면 대화별 줄이 필요 없다. 전체 동시 실행 수 제한 하나로 바꾼다 |
| 출력 상한 8MiB (O 2.4) | **줄여서** | 상한은 두되 크기는 설정값으로(ai 모듈 2차 작업 2번과 같은 자리) |
| 오래 사는 프로세스와 stdio 제어 규약 (O 2.2) | 안 가져옴 | 놀고 있는 프로세스 6개가 메모리를 계속 쥔다. 규약이 숨은 인자 · 버전에 묶인다 |
| CLI 세션 이어 가기 · 어긋남 복구 · 다시 채우기 (O 2.3) | 안 가져옴(1차) | 5.3의 까닭. 2차 후보로 남긴다 |
| 자기 도구를 MCP로 넘기기 (O 2.5) | 안 가져옴 | `pi-ai`에 도구 호출이 아직 없다("나중 작업"). 생기면 다시 본다 |
| 저장한 토큰을 파일 서술자로 넘기기 (O 2.6) | 안 가져옴 | 변수 이름이 공식 문서에 없다. 토큰을 우리가 쥐게 된다 |
| 구독 토큰으로 API 직접 호출 (O 2.6, H 3.1) | 안 가져옴 | 만들지 않기로 한 길 |
| ACP (H 3.2) | 안 가져옴 | Claude Code에 `--acp`가 없다 |
| 이미지 · 스킬 플러그인 · 압축 제어 · 세션 갈래치기 | 안 가져옴 | 쓰지 않는다 |

### 5.2 공급자의 모양 (제안)

이름은 `ClaudeCliProvider`(가칭). `AiProvider`를 구현한다.

**설정 값** — 모두 인자로 받는다. `pi-ai`의 `src`는 환경 변수를 읽지 않으므로 자식에게 줄 환경도 조립 단계가 만들어 넘긴다.

| 값 | 뜻 | 기본 |
|---|---|---|
| `command` | 실행 파일의 절대 경로. 찾기는 조립 단계가 한다(`PATH` → 설치 위치) | 없음(없으면 `NotConfigured`) |
| `config_dir` | 전용 로그인 폴더. 주면 자식에 `CLAUDE_CONFIG_DIR`로 넣는다 | 없음(사용자의 평소 로그인) |
| `work_dir` | 자식의 작업 폴더. **비어 있는 전용 폴더** | 데이터 폴더 아래 한 곳 |
| `base_env` | 자식에게 줄 환경의 전부. 조립 단계가 허용 목록으로 만든다(`HOME` · `PATH` · `LANG` · `TMPDIR` 정도) | — |
| `timeout` | 호출 하나의 전체 제한 | 지금 공급자의 `timeout`과 같은 설정값 |
| `max_concurrency` | 동시에 띄울 프로세스 수 | 2 (5.6) |
| `max_output_bytes` | 표준 출력 · 표준 오류를 읽는 상한 | 설정값 |

**한 번 부르는 흐름**

1. 동시 실행 자리를 얻는다(세마포어). 못 얻으면 줄을 선다. 줄에서 기다린 시간도 `timeout`에 넣는다.
2. 시스템 글이 있으면 0600 임시 파일에 쓴다.
3. 자식을 띄운다. 환경은 `base_env`로 **통째로 바꾸고**(물려주지 않는다) 2.2의 지우는 목록은 한 번 더 뺀다. 인자:

```text
claude -p --output-format json --model <모델>
  --system-prompt-file <임시 파일>
  --safe-mode --tools "" --disallowedTools "mcp__*" --strict-mcp-config
  --setting-sources "" --settings '<2.5의 제한 설정 JSON>'
  --disable-slash-commands --no-chrome
  --no-session-persistence --max-turns 1
  --permission-mode dontAsk --permission-prompts none
```

4. 대화를 글 하나로 만들어 표준 입력에 쓰고 닫는다. 대화가 사용자 글 하나뿐이면 그 글 그대로, 여러 줄이면 "사용자:\n… / 담당:\n…" 줄로 이어 붙이고 끝에 "마지막 사용자 요청에 이어서 답한다" 한 줄(H 3.2).
5. 표준 출력 · 표준 오류를 상한까지 읽으며 끝나기를 기다린다. `timeout`이 지나면 5.6의 순서로 끝낸다.
6. 표준 출력을 JSON 하나로 읽는다(5.4). 임시 파일을 지운다(실패해도).

- 시스템 글은 덧붙이지 않고 **바꾼다**. OpenClaw의 에이전트 턴과 같다(2.2). 공식 문서에 따르면 바꾸면 git 상태 스냅샷도 빠진다. OpenClaw 문서는 "Claude Code still adds its short SDK identity line"이라고 적는다 — 이것은 CLI가 스스로 붙이는 것이고 우리가 붙이는 것이 아니다.
- `--system-prompt-file`과 `--max-turns`는 공식 문서에는 있고 이 컴퓨터 도움말에는 없다(4.1). 시작할 때 `claude --version`으로 버전을 확인하고, 처음 실제 호출에서 "unknown option"이 나오면 `InvalidRequest`로 드러나게 한다. `--permission-prompts`는 2.1.259 이상이 필요하다.
- **최대 토큰**: 실행 인자가 없다. `CLAUDE_CODE_MAX_OUTPUT_TOKENS`를 자식 환경에 넣는 길이 있다(4.3). 요청의 `max_tokens`를 이 값으로 넘길지는 사용자가 정한다(5.9).
- `--restricted`를 더 얹을 수 있다(4.1). OpenClaw는 쓰지 않는다. `--safe-mode` · `--tools ""`와 겹치지만 서로 부딪히는지는 실행해 보지 않아 **확인하지 못했다**.

### 5.3 세션을 이어 갈지

| | (A) 우리 쪽이 대화를 쥐고 매번 통째로 넘긴다 | (B) 담당마다 CLI 세션에 맡긴다 |
|---|---|---|
| 원본 | Hermes의 ACP 클라이언트 | OpenClaw의 `claude-cli` |
| trait | 그대로 맞는다 | 대화 열쇠(담당 · 세션)를 받을 자리가 필요하다. trait을 바꿔야 한다 |
| 정본 | 우리 DB 하나 | 둘(우리 DB와 `~/.claude/projects/`). 어긋남을 다뤄야 한다 — 2.3의 기록 파일 확인 · 초기화 · 다시 채우기 |
| 공급자 바꾸기 | API ↔ CLI를 호출마다 바꿔도 대화가 같다 | CLI 세션에만 있는 문맥은 API로 넘어갈 때 따로 옮겨야 한다(OpenClaw의 "fallback prelude") |
| 디스크 | `--no-session-persistence`로 남기지 않는다 | 투자 판단 대화가 사용자 홈의 CLI 폴더에 평문으로 쌓인다 |
| 긴 대화 | 우리가 자른다. 표준 입력 10MB 한도 | CLI가 스스로 압축한다 |
| 비용 · 속도 | 매번 전체를 다시 보낸다. 프롬프트 캐시가 앞부분에 걸리는지는 **확인하지 못했다** | 이어 가는 호출은 새 글만 보낸다 |
| 구현 | 작다 | 크다(세션 저장 · 검증 · 초기화 까닭 · 다시 채우기 상한) |

[추정] 1차는 (A)가 맞다. trait과 "대화는 우리 DB가 쥔다"는 지금 구조를 그대로 두고, 판단 한 번이 독립된 호출이라 긴 대화가 드물기 때문이다. (B)는 담당의 대화가 길어져 비용이 문제가 될 때의 2차 후보다.

### 5.4 출력 파싱과 오류 옮기기

**성공**: 끝 코드 0, 표준 출력이 JSON 객체 하나, `type == "result"`, `is_error`가 참이 아님, `result`가 글자.

| 응답의 칸 | 어디서 |
|---|---|
| 글 | `result` |
| 멈춘 이유 | `stop_reason`을 지금 `StopReason`으로(`end_turn` · `max_tokens` · `refusal` …). 모르는 값은 `Other`. `null`이면 `Other("")`로 둘지 `EndTurn`으로 볼지 정해야 한다 |
| 토큰 수 | `usage.input_tokens` · `usage.output_tokens`. CLI 자신의 시스템 글이 입력에 들어가 API 직접 호출보다 크게 나온다. 캐시 토큰은 지금 `Usage`에 자리가 없다 |
| 세션 ID · 비용 | 읽지 않는다(세션은 안 쓰고, `total_cost_usd`는 "client-side estimates"다) |

**오류** — 지금 종류 8개에 옮긴다. 새 종류는 만들지 않는다.

| 상황 | 알아채는 법 | `AiError` |
|---|---|---|
| `command`가 설정에 없음, 실행 파일이 없음(`ENOENT`) | 띄우기 전 · 띄울 때 | `NotConfigured` (바깥으로 아무것도 보내지 않았다 — V31) |
| 로그인 없음 · 만료 | 결과가 실패이고 글이 "Not logged in" · "Login expired"이거나 `api_error_status`가 401 · 403 | `Auth { status }` (상태가 없으면 401로 채운다) |
| 조직 불허 · 계정 보류 · 결제 문제 | 실패 글 · 403 | `Auth { status: 403 }` |
| 구독 사용량 한도 | 실패 글이 "You've hit your … limit · resets …"이거나 `api_error_status`가 429 | `RateLimited { retry_after }` (풀리는 시각을 읽을 수 있으면 그때까지, 없으면 `None`) |
| 공급자 장애 · 과부하 | `api_error_status`가 5xx · 529 | `ProviderUnavailable { status }` |
| 토큰 갱신 경합 | 글에 "another Claude Code process is refreshing" (O 2.7) | `ProviderUnavailable { status: 503 }` — 다시 시도할 만한 유일한 CLI 고유 오류 |
| 잘못된 인자 · 모르는 모델 · 표준 입력 10MB 초과 | 끝 코드가 0이 아니고 표준 출력에 결과가 없음, 또는 `api_error_status`가 400 · 404 | `InvalidRequest { message }` (표준 오류 첫 줄을 길이 자르고 가려서) |
| 전체 제한을 넘김 | 타이머 | `Timeout` (프로세스를 끝낸다) |
| 띄우기 실패(권한 등), 신호로 죽음 | `ENOENT`가 아닌 띄우기 오류, 끝 코드 없음 | `Connection` |
| 끝 코드 0인데 JSON이 아님, `result` 칸이 없음, 출력 상한 초과, `subtype: error_max_turns`, 분류 못 한 실패 | 파싱 | `InvalidResponse` |

- 글로 알아채는 줄은 CLI 버전이 바뀌면 깨질 수 있다. `stream-json`으로 바꾸면 `system/api_retry`의 `error` 값과 `rate_limit_event`의 `resetsAt`을 구조로 읽을 수 있다(4.3). 대신 `--verbose`가 필요하고 줄 파서가 든다. 어느 쪽으로 할지는 5.9.
- **다시 시도**: CLI가 안에서 이미 다시 시도한다(`api_retry`). 구독 한도는 몇 시간 뒤에 풀리므로 지금 `retry.rs`의 "2번, 간격 늘려"가 뜻이 없다. [추정] CLI 공급자에서는 `RateLimited` · `Auth`를 다시 시도하지 않고 바로 올리고, 실행기가 풀리는 시각까지 그 담당을 쉬게 하는 편이 맞다.
- 오류 문장 · `Debug`에 표준 출력 · 표준 오류의 원문을 통째로 담지 않는다(E38). 대화 내용이 섞여 있을 수 있다.

### 5.5 CLI의 자체 도구는 반드시 끈다

**까닭**: 담당은 뉴스 · 공시 같은 **바깥 글**을 읽고 판단한다. 그 글에 지시가 숨어 있으면(프롬프트 주입) 도구를 가진 모델은 그 지시대로 사용자 컴퓨터에서 셸을 돌리고 파일을 읽을 수 있다. 그 컴퓨터에는 증권사 키 파일(`.env` · `auth.json`)과 CLI의 로그인이 있다. "AI 에이전트는 증권사 키를 보지 못한다"와 "주문 수량 · 가격은 규칙 엔진이 정한다"는 규칙은 모델이 **글만 내놓을 때만** 성립한다. `-p`는 폴더 신뢰 확인도 묻지 않는다(4.2).

**원본이 막는 법**: OpenClaw는 도구 없는 호출에 `--safe-mode --tools "" --disallowedTools mcp__* --strict-mcp-config --no-session-persistence --max-turns 1`을 쓴다(2.5). 이 묶음을 강제하지 못하는 백엔드는 설정 검사에서 경고한다 — "Utility completions require an exact, empty tool cap"(`cli-backends.md:65-68`). Hermes는 권한 요청에 항상 `cancelled`로 답하고 파일 요청을 작업 폴더 안으로 가둔다(3.2).

**우리가 막는 법** — 겹으로 막는다. 한 겹이 CLI 버전이 바뀌어 뚫려도 다음 겹이 남는다.

| 겹 | 인자 · 조치 | 막는 것 | 공식 근거 |
|---|---|---|---|
| 1 | `--tools ""` | 내장 도구 전부(Bash · 파일 · 웹) | "Use `""` to disable all" |
| 2 | `--disallowedTools "mcp__*"` · `--strict-mcp-config`(`--mcp-config` 없이) | MCP 도구 전부, 사용자의 `~/.claude.json` · `.mcp.json` 서버 | "`mcp__*` removes every MCP tool" |
| 3 | `--safe-mode` · `--setting-sources ""` · `--settings '{"disableAllHooks":true,…}'` · `--disable-slash-commands` · `--no-chrome` | 사용자의 훅 · 플러그인 · 스킬 · `CLAUDE.md` · 자동 기억 · 브라우저 연동 | 도움말의 `--safe-mode` 설명 |
| 4 | `--permission-mode dontAsk` · `--permission-prompts none` | 그래도 남은 도구 호출이 승인을 물으면 거부 | "denies every call that would otherwise prompt" |
| 5 | `--max-turns 1` | 도구를 부르고 다시 도는 흐름 | "Limit the number of agentic turns" |
| 6 | 작업 폴더를 빈 전용 폴더로, 환경을 허용 목록으로 통째로 바꿈 | 설령 읽혀도 키가 그 폴더 · 환경에 없다 | — |
| 7 | 결과의 `permission_denials`가 비어 있지 않거나 `num_turns`가 1보다 크면 `InvalidResponse`로 버리고 경고를 남긴다 | 겹이 뚫렸음을 알아챈다 | `SDKResultMessage` |

- `bypassPermissions` · `--dangerously-skip-permissions` · `--allowedTools` · `--mcp-config` · `--add-dir` · `--plugin-dir`는 **어떤 설정으로도 넘길 수 없게** 한다(추가 인자를 받는 설정 칸을 만들지 않는다).
- [추정] 7번이 실제로 그렇게 나오는지(`--tools ""`일 때 `num_turns`가 늘 1인지)는 모델을 부르지 않아 **확인하지 못했다**.
- 남는 위험: 4겹까지는 CLI가 지켜 주는 약속이다. OS 수준 격리가 아니다. 더 강하게 하려면 CLI를 다른 OS 사용자나 샌드박스에서 돌려야 하는데, 그러면 로그인도 그쪽에서 해야 한다.

### 5.6 동시성과 자원

- **프로세스 수**: 담당 6명이 한꺼번에 판단해도 동시에 띄우는 수는 `max_concurrency`로 묶는다. [추정] 기본 2가 맞다. 까닭은 셋이다. ① 여러 프로세스가 동시에 토큰을 갱신하면 경합 오류가 난다(O 2.7). ② 구독 한도는 "ordinary, individual usage"를 전제한다(4.5 다). ③ 메모리.
- **메모리**: 실행 파일은 310MB짜리 단일 바이너리다. `claude --version`은 24MB · 0.10초였지만 이것은 빠른 길이라 실제 호출의 크기가 아니다. 실제 `-p` 호출 하나의 RSS는 모델을 불러야 잴 수 있어 **확인하지 못했다**. [추정] 수백 MB로 본다. OpenClaw · Hermes 문서에도 숫자가 없다.
- **500MB 목표와의 관계**: 자식 프로세스는 서버 프로세스 밖이다. 목표가 "서버 프로세스만"이면 영향이 없고, "컴퓨터에서 쓰는 전부"면 호출 중에는 넘는다. 어느 쪽인지 정해야 한다(5.9).
- **시간**: 호출마다 프로세스 시작이 든다. `--safe-mode`와 빈 작업 폴더가 시작을 줄인다. 실제 시간은 **확인하지 못했다**.
- **시간 제한**: 전체 제한 하나. 줄에서 기다린 시간을 넣는다. 원본 숫자는 Hermes 900초, OpenClaw `openclaw agent` 600초다.
- **정리**:
  1. 자식을 **자기 프로세스 그룹**으로 띄운다. 제한을 넘기거나 호출이 취소되면 그룹에 `SIGTERM`, 유예(원본은 1~2초) 뒤 `SIGKILL`.
  2. `tokio`의 `kill_on_drop`을 켠다. 호출한 쪽이 사라져도 자식이 남지 않는다.
  3. 서버가 꺼질 때 살아 있는 자식을 모두 끝내고 기다린다.
  4. 서버가 갑자기 죽으면 자식이 고아로 남을 수 있다. 다음 시작 때 치우려면 자식의 PID를 기록해야 한다 — 1차에서 할지는 5.9.
  5. `claude auth status`는 **도중에 죽이지 않는다**(O 2.6의 잠금 주의). 제한만 두고 끝까지 기다린다.

### 5.7 설정 화면과 확인

**고르는 모양**: `README.md:56`의 필수 항목 "AI 키: API 키 또는 구독 OAuth 토큰 가운데 하나"를 "AI 연결: **API 키** 또는 **Claude Code CLI 사용** 가운데 하나"로 바꾼다.

| 칸 | 내용 |
|---|---|
| 고르기 | ○ API 키 ○ Claude Code CLI 사용(구독) |
| CLI를 골랐을 때 | 실행 파일 경로(자동으로 찾아 채우고 고칠 수 있다), 전용 로그인 폴더(비워 두면 평소 로그인), 동시 실행 수 |
| 상태 줄 | "설치됨 2.1.316 · 로그인됨(구독: max)" / "설치되지 않음" / "로그인되지 않음 — 터미널에서 `claude auth login`" / "API 키로 로그인돼 있음 — 구독이 아님" / "확인하지 못함" |
| 넣지 않는 것 | 토큰 입력 칸. 로그인은 사용자가 터미널에서 공식 CLI로 한다(4.5 가: "sign-in … must complete through Anthropic's own flow") |

- `Credential::OAuthToken`(`crates/ai/src/credential.rs:38-39`)과 `auth.json`의 구독 토큰 자리는 이 안에서는 쓸 곳이 없어진다. 지울지는 5.9.
- 둘 다 설정했을 때의 순서는 **사용자가 고른 하나만** 쓴다(O 2.8). 넘어가기를 둘지는 5.9.

**모델을 부르지 않고 확인하는 법**

| 확인 | 명령 | 알 수 있는 것 | 못 아는 것 |
|---|---|---|---|
| 설치 | `claude --version` (5초 제한) | 있다 · 없다, 버전 | — |
| 로그인 | `claude auth status --json` (10초 제한, 끝까지 기다림) | 끝 코드 0 · 1, `loggedIn`, `authMethod`, `subscriptionType` | 구독 한도가 남았는지, 조직 정책이 막는지, 키체인을 서비스가 읽는지 |

- `authMethod`가 `claude.ai`가 아니면(`api_key` 등) 구독이 아니다. 화면에 그대로 알린다.
- 확인할 때의 환경과 `CLAUDE_CONFIG_DIR`는 **실제 호출과 같아야** 한다(OpenClaw 주석 — `cli-auth-seam.ts:24-25`).
- [추정] macOS에서 서버를 launchd 서비스로 띄우면 키체인을 못 읽을 수 있다. OpenClaw 문서가 "a login held in a macOS Keychain item the Gateway cannot read"를 경우로 든다(`cli-backends.md:623`). 이번에 `auth status`를 실행하지 않아 **확인하지 못했다**.
- 지금 `pi-ai`의 키 확인기는 "가장 싼 요청 한 번"이다(`ai.md` 7번). CLI에서는 위 두 명령으로 바꾸고, 실제 호출 확인은 사용자가 누를 때만 한다.

### 5.8 기계로 검사할 수 있는 것과 없는 것

**검사할 수 있다** — 가짜 `claude` 실행 파일(셸 스크립트나 작은 Rust 바이너리)을 임시 폴더에 두고 `command`로 가리킨다. 가짜는 받은 인자 · 표준 입력 · 환경 · 작업 폴더를 파일에 적고, 정해 둔 표준 출력 · 표준 오류 · 끝 코드 · 지연을 낸다.

| 검사 | 보는 것 |
|---|---|
| 인자 | 5.2의 묶음이 순서대로, 빠짐없이. 금지 인자가 없다 |
| 표준 입력 | 대화가 정한 모양으로 한 번 쓰이고 닫힌다. 프롬프트가 인자에 없다 |
| 환경 | `ANTHROPIC_API_KEY` 등이 부모에 있어도 자식에 없다. 증권사 키 이름이 없다. `config_dir`를 주면 `CLAUDE_CONFIG_DIR`가 있다 |
| 작업 폴더 | 전용 폴더다 |
| 성공 파싱 | 글 · 멈춘 이유 · 토큰 수 |
| 오류 옮기기 | 5.4의 줄마다 가짜 출력을 주고 `AiError` 종류를 본다 |
| 시간 초과 | 가짜가 잠들면 `Timeout`이 나오고 가짜 프로세스(와 그 자식)가 죽어 있다 |
| 동시성 | 6개를 한꺼번에 불러도 동시에 산 가짜가 `max_concurrency`를 넘지 않는다 |
| 정리 | 호출을 버려도(drop) 자식이 남지 않는다. 임시 시스템 글 파일이 지워진다 |
| 출력 상한 | 가짜가 끝없이 찍으면 상한에서 끊고 `InvalidResponse` |
| 비밀 | 오류 문장 · `Debug`에 표준 출력 원문 · 환경 값이 없다 |
| 실행 파일 없음 | `NotConfigured`, 아무것도 띄우지 않았다 |
| 로그인 확인 | 가짜 `auth status` 출력 다섯 가지 → 상태 다섯 가지 |
| 도구 뚫림 감지 | `permission_denials`가 찬 가짜 출력 → `InvalidResponse` |

**검사할 수 없다** (실제 CLI · 실제 계정이 있어야 한다)

- 실제 CLI가 이 인자 묶음을 받아들이는지, 그리고 도구가 정말 0개인지.
- 실패 때 실제 JSON의 모양과 글(로그인 없음 · 한도 · 만료). 4.3은 문서에서 읽은 것이다.
- 구독 한도가 실제로 어떻게 소모되는지, 6명이 24시간 돌 때 한도에 닿는지.
- 프로세스 하나의 메모리와 시작 시간.
- 서비스로 띄웠을 때 키체인 로그인이 읽히는지.
- CLI가 새 버전에서 인자 · 출력 · `-p`의 기본값(`--bare` 예고)을 바꾸는지.

→ [추정] "실제 CLI로 한 번 불러 보는" 손 확인 절차를 사용자 확인 목록에 넣어야 한다.

### 5.9 사용자가 정해야 하는 것

1. **약관**: 4.5의 세 문장 — "개발자는 API 키를 써야 한다"(가), "자동화된 방식의 접속"(마), "증권 매매 · 금융 조언 목적"(마) — 을 읽고 구독으로 이 프로그램을 돌릴지 정한다. 문서는 Anthropic에 묻는 길(contact sales)을 적어 두었다.
2. **D4 고치기**: "CLI를 호출마다 띄우기"를 버린 대안에서 빼고 "API 직접 호출과 CLI 호출을 공급자 둘로 둔다"로 바꿀지. `docs/dev/modules/ai.md:10`의 "의도해서 다른 것: CLI를 띄우지 않고 직접 부른다"도 같이 바뀐다.
3. **구독 OAuth 토큰 자리**: `Credential::OAuthToken`과 `auth.json`의 토큰 자리를 지울지 남길지.
4. **세션**: 5.3의 (A)로 갈지. (B)를 2차로 둘지.
5. **넘어가기**: CLI가 한도 · 로그인 문제로 실패할 때 API 키로 넘어갈지(과금 주체가 바뀐다), 그 담당을 쉬게 할지.
6. **동시 실행 수**의 기본값과, 500MB 목표가 자식 프로세스를 포함하는지.
7. **출력 형식**: `json`(단순, 글로 오류 판정) 대 `stream-json`(구조화된 오류 · 한도 풀리는 시각, 줄 파서 필요).
8. **최대 토큰**: 요청의 `max_tokens`를 `CLAUDE_CODE_MAX_OUTPUT_TOKENS`로 넘길지, CLI 공급자에서는 무시한다고 문서에 적을지.
9. **로그인**: 사용자의 평소 로그인을 같이 쓸지, 전용 폴더(`CLAUDE_CONFIG_DIR`)에 따로 로그인하게 할지. `claude setup-token`의 1년 토큰을 받아 우리가 보관해 자식 환경에 넣는 길은 공식 문서가 "scripts"용으로 적어 두었지만(4.4) 토큰을 우리가 쥐게 된다 — 지원할지.
10. **`--bare` 예고**: `-p`의 기본값이 바뀌면 구독 길이 끊길 수 있다. 그때를 대비해 지원하는 CLI 버전 범위를 정하고 시작할 때 확인할지.
11. **고아 프로세스**: 서버가 갑자기 죽은 뒤 남은 자식을 다음 시작 때 치우는 일을 1차에 넣을지.
12. **`--restricted`**를 더 얹을지(실제 CLI로 확인한 뒤).

---

## 6. 확인하지 못한 것

- Anthropic의 2026년 2월 안내문 원문과 날짜. `legal-and-compliance` 페이지의 현재 글만 읽었다.
- 구독 토큰의 API 직접 사용을 서버에서 막았다는 사실.
- 이용 정책(AUP) 본문, 소비자 약관의 영어 원문(한국어판을 읽었다).
- 실제 `claude -p` 호출의 동작 전부: 인자 묶음이 받아들여지는지, 실패 때의 JSON, 메모리, 시작 시간. 모델을 부르지 않았다.
- `claude auth status`의 실제 출력과 서비스 환경의 키체인 접근. 실행하지 않았다.
- OpenClaw의 전체 동시 실행 상한, 한도 오류를 글에서 분류하는 규칙(`failover-error.ts`), `total_cost_usd`를 읽는지.
- Hermes에서 코어 밖으로 나간 Copilot ACP 공급자 플러그인의 지금 위치.
- `--system-prompt-file` · `--max-turns`가 2.1.316에서 받아들여지는지(공식 문서에는 있고 도움말에는 없다).
