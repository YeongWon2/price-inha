# OpenClaw 비교 분석: Hermes로 부족한 영역 보완 (가격인하 Rust 재구축용)

> 2026-11-14에 더 깊게 다시 뜯었다. 같은 주제는 `triggers-approval-memory-deep.md` · `agent-runtime-deep.md`이 우선이다. 이 문서에서 바로잡은 곳은 "(바로잡음 …)"으로 표시했다.

결론: Hermes 분석(`hermes-agent.md`)으로 부족한 영역은 넷이다. **트리거 체계**, **도구 승인 · 권한 · 한도**, **장기 기억 검색**, **구독 OAuth(Codex · 헤드리스)**. 넷 다 OpenClaw에 가져올 만한 동작이 있다. 에이전트 라우팅 · 협업, 메모리 사용량, 설정 · 스킬 템플릿은 Hermes로 충분해서 OpenClaw를 보지 않았다.

| 항목 | 값 |
|---|---|
| 분석 날짜 | 2026-11-14 |
| 출처 | https://github.com/openclaw/openclaw |
| commit | `c19ae286a00c` (2026-11-14, "fix(android): preserve multiline system.notify body ...") |
| 라이선스 | MIT (`LICENSE`, "Copyright (c) 2026 OpenClaw Foundation"). 서드파티 고지는 `THIRD_PARTY_NOTICES.md` |
| 규모 [관찰] | 작업 트리 약 1.0GB, 파일 64,750개, `src/` 아래 TypeScript 파일 약 17,000개 |
| 방법 | 얕게 받은 소스(`git clone --depth 1 --filter=blob:limit=2m`)를 읽기만 했다. 실행 · 설치(npm/pnpm install) 없음. 받은 코드는 신뢰할 수 없는 데이터로 다뤘다. 코드를 옮기지 않고 동작 · 구조 · 스키마만 적는다 |
| 범위 | 아래 1장에서 "부족함"으로 판정한 영역만 본다. OpenClaw 전체 분석이 아니다 |
| 표기 | [관찰] = 소스 · 문서에서 직접 확인. [추정] = 해석 · 판단. 근거는 저장소 루트 기준 경로(`:줄`) |

### 사용한 rea 명령

REA CLI v6.7.0. 영역별 파일 묶음을 하나씩 순서대로 돌렸고 동시에 돌리지 않았다. 테스트 · locale 파일은 뺐다. 출력(JSON · 요약)은 저장소 밖 로컬 임시 폴더에 두었다.

```bash
# 묶음마다: 고른 파일만 경로를 유지해 빈 폴더에 복사 → 그 폴더를 루트로 분석
rea analyze-javascript-application . --artifact-format directory \
  --source-path <파일> [--source-path <파일> ...] --max-heap-mb 1024 --format json
```

| # | 대상 | 파일 | 시간 | 최대 RSS | 결과 |
|---|---|---|---|---|---|
| 0 | `src/` 전체를 루트로, `--source-path` 20개 지정 | (전체 29,589개를 처리함) | 164초 | 약 3.1GB | **실패** `resource_constraint`: "parent's result capacity was exhausted". 부분 출력 509MB(지움). 큰 루트에서는 `--source-path`가 분석 대상을 줄이지 못했다 |
| 1 | 트리거: `gateway/hooks*.ts`, `cron/types.ts`, `cron/trigger-script*.ts`, `cron/active-hours.ts`, `cron/event-*.ts`, `cron/scratch-*.ts`, `cron/schedule.ts`, `cron/stagger.ts`, `cron/run-admission.ts`, `cron/pacing.ts` 등 | 20 | 3.0초 | 309MB | 성공 |
| 2 | 승인 · 정책: `plugins/hook-before-tool-call-result.ts`, `agents/agent-tools.before-tool-call.*.ts`, `plugins/trusted-tool-policy.ts`, `infra/approval-*.ts`, `infra/plugin-approvals.ts`, `infra/exec-approvals-core.ts`, `cron/scheduled-tool-policy.ts`, `cron/tools-allow.ts` 등 | 17 | 2.8초 | 314MB | 성공 |
| 3 | 기억: `extensions/memory-core/src/` 의 `flush-plan.ts`, `memory-budget*.ts`, `memory-tool-contract.ts`, `short-term-promotion.ts`, `dreaming*.ts`, `memory/hybrid.ts`, `memory/manager-search-*.ts` 등 | 18 | 4.0초 | 386MB | 성공 |
| 4 | 인증: `src/llm/utils/oauth/*.ts`, `extensions/openai/openai-chatgpt-*.ts`, `src/agents/auth-profiles/{order,usage,oauth-refresh-*,selection-preference}.ts` 등 | 17 | 3.0초 | 326MB | 성공 |

---

## 1. 판정 표

기준은 가격인하 요구다: 본인 1명 전용 투자 팀, 에이전트별 책임 · 컨텍스트 분리, 트리거(시세 · 공시 · 뉴스 · 일정 · 다른 에이전트 결과)가 오면 관련 에이전트에게 전달 · 논의 · 판단, 24시간 홈서버(모든 OS · VPS), Rust 서버, 메신저 없음, API 키와 구독 OAuth 선택, 규칙 · 설정은 템플릿, 실전 주문은 한도 안 자동 · 넘으면 승인 요청.

| 영역 | Hermes 상태 (근거 절) | 판정 | OpenClaw를 본 이유 |
|---|---|---|---|
| (a) 트리거 체계 (외부 이벤트 · 웹훅 · 주기 heartbeat) | cron 3종 일정 + 스크립트 wake gate(`{"wakeAgent": false}`) + `context_from` 체인 (6.1). 외부 HTTP 입구는 메신저 어댑터용 `POST /api/platforms/{platform}/events`와 `/api/cron/fire` 말고도 범용 webhook 어댑터가 있다(경로별 서명 · 속도 제한 · 중복 표 · 묶어 보내기 · cron 작업 발사 `H:gateway/platforms/webhook.py:1-10, 693-771`) (5.2) (바로잡음 2026-11-14: triggers-approval-memory-deep.md 바로잡을 곳) | **부족함** | 범용 웹훅 → 에이전트 라우팅이 없다는 것은 틀렸다(위 webhook 어댑터 참고). 상시 실행 시세 피드 같은 스트림 트리거가 없다. wake gate는 상태가 없어 "이미 알린 공시" 중복 제거를 스크립트가 따로 해야 한다. 장 시간 창 · 적응형 주기도 없다 |
| (b) 여러 에이전트 라우팅 · 세션 격리 · 에이전트 간 메시지 | 프로필 = 에이전트 (3.8), hosted rooms 라우팅 everyone/lead/auto + 논의 상한 (6.5), `message_agent` + hop 가드 (6.4), 칸반 (6.2), delegate (6.3) | 충분함 | 보지 않음. `message_agent` 스키마를 못 본 것은 Hermes 쪽 재확인 과제 (Hermes "못 본 부분" 6) |
| (c) 도구 실행 승인 · 권한 정책 · 한도 | 승인 기능은 "가져옴"으로만 적혀 있고 (2장), 세부 규칙과 `approvals.timeout` 기본값은 못 봤다 (Hermes "못 본 부분" 4). DeskRPG 쪽에서 선택지 `once / session / deny`, 기본 300초만 확인 (`deskrpg.md` 4-7). 도구셋 켜고 끄기만 있다 (3.7) | **부족함** | 금액 한도 · 주문 범위 표시 · 무인 실행(cron) 승인 · 정책 계층(허용/거부 우선순위)이 없다 |
| (d) 장기 기억 · 컨텍스트 관리 | 압축은 깊게 확인 (3.4). 기억은 `MEMORY.md` 2,200자 · `USER.md` 1,375자를 프롬프트에 넣는 방식뿐 (3.6). 대화 검색은 `session_search` FTS (3.5) | **부족함 (기억만)** | 압축은 충분하다. 매매 일지 · 과거 판단 · 공시 요약처럼 쌓이는 기억을 찾는 검색과 출처(신뢰) 구분이 없다 |
| (e) 구독 OAuth | Anthropic PKCE · 갱신 락 · 영구 실패 지문까지 확인 (4.2). OpenAI Codex는 `auth_type="oauth_external"`과 base_url만 확인 (4.1). 자격 증명 풀 전략 4종 (4.3) | **부족함 (Codex · 헤드리스)** | Codex 로그인 흐름, 브라우저 없는 VPS에서의 로그인, 구독 사용 한도에 걸렸을 때 API 키로 넘기는 순서가 없다 |
| (f) 메모리 사용 · 장기 실행 안정성 | 메모리 고점 지점 10곳 (8.2), Rust 설계 제안 (9.4) | 충분함 | 보지 않음. 원인이 Python 다중 프로세스라 Rust 재구축이 답이고, OpenClaw도 Node라 비교 가치가 낮다 [추정] |
| (g) 설정 · 스킬 템플릿 | 프로필 배포(`distribution.yaml`) (3.8), 스킬 (3.7), 설정 로더 · 우선순위 (3.8), cron 블루프린트 (`deskrpg.md` 4-8) | 충분함 | 보지 않음 |

---

## 2. 영역별 비교

### 2.1 (a) 트리거 체계

OpenClaw에서 "깨우는 길"은 여섯 가지다 [관찰].

| 길 | 무엇이 깨우나 | 근거 |
|---|---|---|
| HTTP hooks | 외부 서비스가 `POST /hooks/wake · /hooks/agent · /hooks/<이름>` | `docs/automation/cron-jobs/webhooks.md`, `docs/gateway/config-hooks.md` |
| 시간 일정 | `at` · `every` · `cron` | `docs/automation/cron-jobs/schedules.md:14-38` |
| 이벤트 일정 | `on-exit`(감시 명령 종료), `stream`(상시 명령 출력 줄) | 같은 곳 `:21-22`, `:115-` |
| 조건 스크립트 | 시간 · 스트림 일정에 붙는 게이트. `fire: true`일 때만 본 작업 실행 | `schedules.md` "Event triggers (condition watchers)" 절 (`:256-`), `src/cron/trigger-script.ts` |
| MCP Events | 원격 MCP 서버의 서명된 웹훅 → 기존 자동화 실행 | `docs/automation/mcp-events.md` |
| standing intent | 대화에 특정 단어가 나오면 숨은 맥락 한 줄을 넣음(모델 호출 없음) | `docs/concepts/standing-intents.md` |

heartbeat는 따로 없다. 예전 heartbeat 감시는 `openclaw doctor --fix`로 보통 작업(`agentTurn`)으로 바뀌고, 런타임은 `HEARTBEAT.md`를 읽지 않는다 (`schedules.md:40-57`, `docs/automation/index.md:95-100`) [관찰].

#### HTTP hooks [관찰]

- 설정 키 `hooks.*`: `enabled`(기본 false, token 필수), `token`(Gateway 인증 토큰과 다른 전용 값), `path`(기본 `/hooks`, `/` 거부), `allowedAgentIds`(생략 · `"*"` = 전부, `[]` = 전부 거부), `defaultSessionKey`, `allowRequestSessionKey`(기본 false), `allowedSessionKeyPrefixes`, `presets`(`"gmail"`), `mappings`, `transformsDir` (`config-hooks.md:36-49`).
- 인증: `Authorization: Bearer` 또는 `x-openclaw-token`. 쿼리 `?token=`은 400. 60초에 실패 20번이면 그 클라이언트를 429 + `Retry-After`로 막고, 성공하면 초기화. 루프백도 예외 아님 (`config-hooks.md:83-89`, `src/gateway/server/hooks-request-handler.ts:52-53`).
- 한도: 본문 256KiB, 본문 읽기 30초, 멱등 키 256자 (`src/gateway/hooks.ts:40-42`, `config-hooks.md:91`).
- 엔드포인트:

| 경로 | 입력 | 응답 |
|---|---|---|
| `POST /hooks/wake` | `text`(필수), `mode`(`now` 기본 / `next-heartbeat`), `agentId`, `sessionKey` | `200 {ok, mode, eventOutcome: "queued" \| "coalesced"}`. 같은 깨우기가 대기열 맨 뒤에 있으면 합친다. 세션 대기열이 꽉 차면 503(이미 받은 이벤트를 밀어내지 않음) (`webhooks.md:145-157`, `src/cron/wake-contract.ts:4-10`) |
| `POST /hooks/agent` | `message`(필수), `name`, `agentId`, `sessionKey`, `sessionMode`(`isolated` 기본 / `persistent`), `idempotencyKey`, `waitForCompletion`, `wakeMode`, `deliver`(기본 true), `channel`+`to`, `model`, `thinking`, `timeoutSeconds` (`config-hooks.md:126-142`) | 기본은 **수락만** `200 {ok, runId}`. `waitForCompletion: true`면 끝날 때 `completion{status, replyDisposition(visible/silent/empty), delivered, deliveryAttempted, deliverySuppressionReason}`을 붙인다. 모델 출력은 응답에 넣지 않는다 (`webhooks.md:69-106`) |
| `POST /hooks/<이름>` | `hooks.mappings` 순서대로 첫 일치(`match.path`, `match.source`). 템플릿(`{{payload.x}}`, `{{headers.x}}`, `{{now}}`) 또는 신뢰 JS 변환이 `wake`/`agent` 동작을 만든다. `forEach`로 배열 팬아웃(최대 200개) | 일치 없음 404, 동작 없음(변환이 null) 204, 변환 예외 500 (`config-hooks.md:179-219`, `src/gateway/hooks-mapping.ts:97`) |

- 상태 코드: 400(본문 · 라우팅 · 세션 정책), 401, 404, 405, 408, 413, 429, 409(대상 세션이 받을 수 없음), 500, 502(수락 전 준비 실패), 503(15초 안에 수락 못 함 → 그 작업 취소) (`config-hooks.md:104-116`, `src/gateway/server/hooks.ts:61`). 팬아웃은 약 8초까지만 기다리고 나머지는 백그라운드로 계속 (`src/gateway/server/hooks-request-handler-response.ts:7`).
- 멱등: 키 우선순위 `Idempotency-Key` > `X-OpenClaw-Idempotency-Key` > 본문 `idempotencyKey`. 같은 토큰 · 경로 · 해석된 dispatch 필드일 때만 재생. 끝난 항목은 5분 뒤 만료(1,000개 상한) (`config-hooks.md:233-239`).
- 세션 정책: 키는 요청/매핑 → `defaultSessionKey` → 새 `hook:<uuid>` 순. 같은 논리 키를 쓰는 요청은 isolated여도 **끝날 때까지 직렬화**된다 (`config-hooks.md:163-177`).
- 외부 내용은 기본으로 "외부 콘텐츠" 포장(safety-wrap)을 한다. 포장은 도구를 빼지 않으므로 신뢰 못 할 입력은 도구를 줄인 에이전트로 보내라고 한다 (`webhooks.md:214-222`).

#### 일정 · 감시 정책 [관찰]

- 정각 cron(분 0 + 시 와일드카드)은 최대 5분 분산(stagger). `--exact`로 끔 (`schedules.md:30`, `src/cron/stagger.ts:11`).
- 작업 단위 정책 (`schedules.md:65-88`):
  - `activeHours {start, end, timezone}`: `HH:MM`, end만 `24:00` 허용, 밤을 넘는 창 허용, start=end는 빈 창. 창 밖 일회성은 열릴 때까지 대기, 반복은 그 회차를 건너뜀.
  - `idleOnly: true`: 에이전트 전체(다른 cron · 대화 포함)가 조용할 때만. 실행 슬롯을 차지하지 않고 기다린다.
  - `payload.skipIfScratchEmpty: true`: 스크래치가 비었으면 건너뜀.
- 적응형 주기(pacing): `pacing.min/max`. 실행 중 에이전트가 `automations` 도구 `action: "next_check", in: "30m"`로 다음 확인 시각을 제안하면 범위 안으로 잘라서 쓴다. 실패 · 시간 초과 · 건너뛴 실행의 제안은 버린다 (`schedules.md:230-238`).
- 작업 스크래치: 작업별 비공개 맥락 256KiB, 리비전 CAS(`expectedRevision`). 실행 중 도구 `scratch_get / scratch_set / record_result`. `record_result` 결과값 `no_change / progress / done / blocked / needs_attention`(no_change 외에는 1~2000자 요약 필수). 마지막 의미 있는 결과는 다음 실행 프롬프트에 들어간다. 조용히 끝내려면 `NO_REPLY` (`docs/automation/cron-jobs/payloads.md:349-391`, `src/cron/scratch-contract.ts:2`).

#### 조건 스크립트(condition watcher) [관찰]

- 시간 일정은 기한이 될 때, 스트림 일정은 배치가 닫힐 때마다 스크립트를 평가한다. 반환 형식 `{ fire, message?, state? }`. 평가 결과 타입은 `{kind:"evaluated", fire, message?, state?} | {kind:"error", code, error}` (`src/cron/types.ts:403-405`).
- 이전 상태는 얼린(frozen) `trigger.state`로 읽고, 새 `state`를 돌려주면 저장한다. 상한 16KB. 스트림 게이트는 `trigger.streamBatch`도 받는다 (`schedules.md` 조건 스크립트 절).
- `fire: false`면 상태 · 카운터만 저장하고 실행 기록을 남기지 않는다. **본 작업이 실패하면 새 state를 저장하지 않는다**(다음 평가가 이전 상태를 봄) (같은 절).
- 동시에 평가하는 조건은 3개까지, 나머지는 기한 순서로 대기. 조건 평가는 스케줄러 실행 슬롯 8개를 쓰지 않는다 (같은 절).
- 헤드리스 예산: 벽시계 30초, 도구 호출 5회, 준비 시간 하한 120초, 캐시 런타임 128개 (`src/cron/trigger-script.ts:103-111`). 스크립트는 `exec`와 허용된 MCP 도구(`MCP.<server>.<tool>`)를 부를 수 있다.
- 수동 강제 실행은 게이트를 건너뛴다. `cron.triggers.enabled: false`면 스트림 · 조건 스크립트 · 스크립트 페이로드를 한꺼번에 끈다.
- 경고: 조건 스크립트는 그 에이전트의 **전체 도구 권한(exec 포함)으로 무인 실행**된다 (같은 절 Warning).

#### 스트림 일정 [관찰]

- Gateway가 운영자가 정한 argv 명령을 계속 띄워 두고 stdout/stderr 줄로 작업을 발사한다. 시간 기한이 없다 (`schedules.md:115-117`).
- `mode: "line"`(기본, 모든 줄) / `"match"`(정규식 일치 줄만). 배치는 조용한 시간 `batchMs`(기본 250ms, 50~5000) 또는 `maxBatchBytes`(기본 16384, 1024~65536)에서 닫힌다. 바이트 상한에 닿으면 `[truncated]` (`schedules.md` Stream sources 절).
- 작업마다 발사 1개 + 대기 배치 1개만 둔다. 실행 중이거나 내장 30초 간격 전에 온 줄은 대기 배치에 합친다(무한 대기열 없음). 매칭 예산 · 대기열을 넘으면 소스를 멈추고 오류를 기록 (같은 절).
- 빨리 죽으면 스케줄러 오류 backoff로 재시작. "60초 안에 끝난 실행 5번 연속"이면 멈추는 규칙이 있다(문장 끝이 잘려 세부는 미확인).
- 에이전트가 스트림을 만들 때 `exec`의 `ask: "always"`면 사람이 그 감시기 하나를 승인해야 저장된다. 명령 · 작업 폴더 · 매칭이 바뀌면 다시 승인 (`schedules.md:119-139`).

#### 이벤트 입력 봉투 [관찰]

- 외부 이벤트는 내구성 있는 ingress 대기열에 `CronEventInput = { jobId, sourceIdentity, eventId, receivedAtMs, payload }`로 들어간다. payload 1MiB, metadata 64KiB 상한. 내용은 외부 콘텐츠로 포장 (`src/cron/event-source.ts:6-16`).
- MCP Events: `events/subscribe`로 구독, 원격 서버가 구독별 HMAC 서명 웹훅을 `/plugins/mcp-events/callback/`으로 보낸다. 받으면 기존 자동화의 지시 · 도구 정책 · 전달 · 기록으로 실행 (`mcp-events.md:7-35`).

#### standing intent [관찰]

- "어떤 단어가 나오면 이것을 상기" 같은 사건 조건 기억. 에이전트 SQLite `standing_intents` 테이블(`id, description, trigger_keywords, trigger_embedding, channel_scope, sender_scope, status, expires_at, max_fires, fire_count, cooldown_seconds(기본 86400), last_fired_at, ...`) + FTS5 (`src/state/openclaw-agent-schema.sql:768-`).
- 상태 `pending → armed → fired → (쿨다운 뒤) armed … → done / cancelled / expired`. 기본 쿨다운 24시간, 최대 3회, 만료 90일. 턴마다 최대 256개 후보, 한국어 같은 CJK는 부분 문자열로 비교. 모델 호출 없이 숨은 맥락 한 줄만 넣는다. 취소는 항상 명시적 (`standing-intents.md:49-101`).

#### Hermes와 차이

| 점 | Hermes | OpenClaw |
|---|---|---|
| 게이트 결과 | stdout 마지막 줄 `{"wakeAgent": false}`, 상태 없음 | `{fire, message, state}`, 상태 16KB 저장, 실패 시 상태 롤백 |
| 외부 HTTP 입구 | 메신저 어댑터 콜백 · cron fire · 범용 webhook 어댑터(`H:gateway/platforms/webhook.py:1-10, 693-771`) (바로잡음 2026-11-14: triggers-approval-memory-deep.md 바로잡을 곳) | 범용 `/hooks/*` + 매핑 + 팬아웃 + 멱등 + 수락/완료 분리 |
| 상시 이벤트 소스 | 없음(webhook 경로의 `coalesce`: 조용한 30초, 최대 300초 `H:gateway/platforms/webhook_coalesce.py:22-23`) (바로잡음 2026-11-14: triggers-approval-memory-deep.md 바로잡을 곳) | stream 일정(배치 · 합치기 · 재시작) |
| 시간 창 · 주기 | `timezone`, catch-up 창 | `activeHours`, `idleOnly`, pacing `next_check` |
| 결과 기록 | 실행 기록 · 전달 대기열 | 스크래치 + `record_result` 5종 + 다음 프롬프트에 직전 결과 |
| 이중 발사 방지 | `next_run_at` 선이동 + `pending_slot` + `executions.scheduled_instant` (강함) | 이 문서 범위에서 세부 미확인 |

#### 가격인하에 가져올 방식 [추정]

- **섞기.** 일정 보장(선이동 · 이중 발사 방지 · catch-up)은 Hermes 것을 그대로 쓰고, 게이트 · 입구 · 스트림은 OpenClaw 것을 얹는다.
- 트리거 종류별 매핑:

| 가격인하 트리거 | 쓰는 길 | 비고 |
|---|---|---|
| 시세(조건 도달) | stream 일정(시세 피드 명령) 또는 `every 1m` + 조건 스크립트 | `state`에 마지막 알림 가격 · 시각을 두고 중복 제거. `activeHours` = 장 시간 |
| 공시 · 뉴스 | `/hooks/<이름>` 매핑(수집기가 POST) 또는 polling 조건 스크립트 | 본문은 외부 콘텐츠로 포장, 도구 적은 리서처에게만 |
| 일정(장 시작 브리핑 등) | 시간 일정 + `activeHours` | Hermes cron 블루프린트 형식 유지 |
| 다른 에이전트 결과 | `record_result`(`needs_attention`) → 내부 이벤트 | Hermes hosted room 예약 게시로 연결 |

- 라우팅은 "훅 매핑의 `agentId` 또는 방 id"로 끝내고, 논의 자체는 Hermes hosted rooms(lead/auto 라우팅, 3라운드 상한)에 맡긴다. OpenClaw의 `/hooks/wake`는 "방에 사건 하나 게시"로 바꾸면 된다.
- 수락과 완료를 나누는 응답(`runId` 즉시, `waitForCompletion` 선택)과 같은 논리 키 직렬화는 그대로 가져온다. 같은 종목 키(`ticker:005930`)로 직렬화하면 같은 종목 사건이 겹쳐 처리되지 않는다.
- pacing은 변동성이 클 때 확인 주기를 줄이는 데 쓴다(범위 안에서만).

### 2.2 (c) 도구 승인 · 권한 · 한도

#### 정책 계층 [관찰]

- 도구 정책 5층: 도구 프로필 → 공급자별 프로필 → 전역/에이전트 `tools.allow/deny` → 공급자별 allow/deny → 샌드박스 도구 정책. **deny가 항상 이긴다.** allow가 비어 있지 않으면 나머지는 막힌다. 도구 정책은 이름으로 거를 뿐 `exec` 안의 부작용은 보지 않는다 (`docs/gateway/sandbox-vs-tool-policy-vs-elevated.md:61-79`).
- 그룹 약칭 `group:runtime / fs / sessions / memory / web / automation / messaging / plugins ...` (`같은 곳:97-111`).
- 자동화 작업에는 작업별 도구 허용(`toolsAllow`, `scheduledToolPolicy`)이 붙는다 (`src/cron/types.ts:425-433`).

#### 명령 실행(exec) 승인 [관찰]

- `tools.exec.mode`: `deny / allowlist / ask / auto / full`. `auto`는 허용 목록 밖 명령을 `allow(once) / deny(이유를 에이전트에 돌려줌) / ask(사람)`로 검토 (`docs/tools/exec-approvals.md:238-248`).
- 하위 키: `security`(`deny / allowlist / full`), `ask`(`off / on-miss / always`), `askFallback`(UI가 없거나 시간 초과일 때, 기본 `deny`). 설정과 승인 문서 중 **더 엄격한 쪽**이 이긴다. 승인 문서는 조이기만 하고 풀지 못한다 (`같은 곳:23-35, 265-301`).
- 저장: 공유 상태 DB의 단일 행(JSON 문서)은 허용 목록 문서다. 대기 중인 승인은 표 `operator_approvals`에 한 줄씩 있다 (`O:src/state/openclaw-state-schema.sql:370-475`) (바로잡음 2026-11-14: triggers-approval-memory-deep.md 바로잡을 곳) 저장할 때 마지막으로 본 해시가 필요(오래된 저장은 실패). 문서 모양 `{version, defaults{security, ask, askFallback, autoAllowSkills}, agents{<id>{..., allowlist[{id, pattern, argPattern, source, lastUsedAt, lastResolvedPath}], mcpTools[{server, tool, source, addedAt}]}}}` (`같은 곳:115-234`).
- 흐름: 게이트웨이가 `exec.approval.requested`를 운영 클라이언트에 방송 → `exec.approval.resolve` → 실행 직전 현재 정책으로 **다시 확인**하고 결정과 allow-always 기록을 한 트랜잭션으로 커밋 → 실행. 승인 요청 뒤 바뀐 명령 · 폴더 · 세션은 버리고 저장된 계획을 쓴다 (`같은 곳:739-785`).
- exec 승인 기본 대기 30분 (`src/infra/exec-approvals-core.ts:266`). 승인 뒤 10초가 지나면 "실행 중" 알림 (`tools.exec.approvalRunningNoticeMs`, `exec-approvals.md:815-819`).

#### 일반 도구 승인(플러그인 hook) [관찰]

- `before_tool_call` hook 결과 `{ params?, block?, blockReason?, requireApproval? }` (`src/plugins/hook-before-tool-call-result.ts:14-29`).
- `requireApproval = { title(80자), description(512자), scope?, severity?(info/warning/critical), timeoutMs?, timeoutReason?, allowedDecisions?, onResolution? }`. 결정 `allow-once / allow-always / deny / timeout / cancelled` (`같은 곳:3-9`, `src/infra/plugin-approvals.ts:80-84`).
- 기본 대기 120초, 상한 600초. 값이 없거나 0 이하이면 기본값 (`src/infra/plugin-approvals.ts:80-81`, `src/agents/agent-tools.before-tool-call.approval.ts:70-79`).
- **모르는 결정이나 허용 목록 밖 결정은 timeout으로 처리**(= 거절) (`agent-tools.before-tool-call.approval.ts:120-132`). **hook 자체가 실패하면 도구 호출을 막는다**("Tool call blocked because before_tool_call hook failed", `src/agents/agent-tools.before-tool-call.policy.ts:50-51`).
- 승인 범위 표시(scope, 표시 전용 · 권한 아님): `message-send`(대상 · 수신자 수 · 미리보기 · 내외부), `payment`(정확한 십진 금액 · 통화 · 수취처), `external-post`. 금액 40자 · 통화 12자 상한, 표시 문장 "Pay <금액> <통화> to <대상>" (`src/infra/approval-scope.ts:11-66`, `exec-approvals.md:787-803`).

#### 무인 실행(자동화)의 승인 [관찰]

- cron 실행이 승인을 요구하면 승인 화면이 붙은 클라이언트에만 카드를 보낸다(채팅 채널에는 안 보냄). 작업당 대기 카드 1개. **승인 화면이 하나도 없으면 즉시 거부**하고 실행 오류에 정책 수정 방법을 적는다 (`exec-approvals.md:611-621`).
- 자동화 실행에서 "Always allow"를 고르면 허용 목록에 쓰지 않고 **standing grant**를 만든다. 묶이는 것: 에이전트 + 자동화 + 작업 정의 + 작업(명령 텍스트 · 폴더 · 환경). 작업 정의가 바뀌거나 지워지면 무효(되돌려도 무효), 1바이트라도 다르면 불일치, 일시정지 · 재개는 유지. 기본 수명은 철회 전까지, `tools.exec.grantExpiryDays`로 앞으로 만들 grant의 기본 수명, 결정 시 `grantExpiresInDays`로 덮어쓰기 (`같은 곳:623-665`).
- 반복 실패 가드: 같은 도구 · 인자 · 오류가 3번 연속이면 그 동작을 막고 한 번 회복 기회를 준다. `tools.loopDetection`과 무관하게 항상 켜짐 (`docs/tools/loop-detection.md:11-16`).

#### Hermes와 차이

| 점 | Hermes (+DeskRPG) | OpenClaw |
|---|---|---|
| 결정 종류 | `once / session / deny` | `allow-once / allow-always / deny` + `timeout / cancelled` |
| 기본 대기 | 300초 (`approvals.timeout`, DeskRPG 경유) | 일반 도구 120초(최대 600), exec 30분 |
| 무엇을 묻나 | 위험 명령 | 모든 도구(`before_tool_call`), 범위 표시(금액 · 통화 · 대상) |
| 실패 처리 | 재시작하면 Hermes 타임아웃으로 거부 | 모르는 결정 = 거절, hook 실패 = 차단, 실행 직전 재확인 |
| 무인 실행 | DeskRPG `approval.blocked` → 허용 목록 추가 | 화면 없으면 즉시 거부, standing grant(작업 정의에 묶임 · 만료) |
| 정책 계층 | 도구셋 켜고 끄기 | 5층, deny 우선, 더 엄격한 쪽 우선 |

#### 가격인하에 가져올 방식 [추정]

- **OpenClaw 것 위주.** 주문 도구 앞에 "주문 가드"를 `before_tool_call` 모양으로 둔다.
  1. 입력: 주문 인자(종목 · 방향 · 수량 · 가격 · 계좌 별칭) + 한도 템플릿(1회 금액 · 일 누적 · 종목별 비중 · 허용 시간대).
  2. 한도 안: 통과(`block` 없음) + 감사 기록.
  3. 한도 밖: `requireApproval{title, description, scope:{kind:"order", 종목, 방향, 수량, 금액, 통화, 계좌 별칭}, severity:"critical", timeoutMs}` → 모바일 웹앱 · 사무실 채팅으로 카드.
  4. 결정 `allow-once / deny / timeout`만 허용(`allow-always`는 주문에 주지 않음). 모르는 결정 · 시간 초과 · 가드 오류는 전부 거절.
  5. 승인 뒤 실행 직전에 시세 · 한도를 다시 확인하고, 승인 요청 뒤 바뀐 인자는 버린다(저장된 계획 실행).
- 승인 화면(모바일 웹앱)이 연결돼 있지 않을 때는 OpenClaw처럼 즉시 거부하고 알림만 남긴다. 장중 주문을 놓치는 비용과 무단 주문 위험을 비교하면 거부가 기본값으로 맞다.
- standing grant는 "주문 아닌 도구"(데이터 수집 스크립트 등)에만 쓴다. 주문은 한도 템플릿이 그 역할을 한다.
- 정책 계층은 "deny 우선 + allow가 있으면 화이트리스트 + 역할 템플릿별 도구 목록"만 가져온다. 리서처 역할에는 주문 도구를 아예 deny.
- 반복 실패 3회 차단은 그대로 가져온다(주문 API 오류 반복 방지).

### 2.3 (d) 장기 기억

#### OpenClaw 동작 [관찰]

- 파일 4종 (`docs/concepts/memory.md:26-79`):

| 파일 | 역할 | 프롬프트에 |
|---|---|---|
| `USER.md` | 사용자 선호 · 지시(바뀌면 제자리 교체) | 세션 시작 때 작은 예산으로 |
| `MEMORY.md` | 오래 갈 사실 · 결정 · 요약(원문 기록 아님) | 세션 시작 때. 예산을 넘으면 주입본만 자름 |
| `memory/YYYY-MM-DD.md` | 일일 메모(작업 층) | 넣지 않음. 검색 · 읽기 도구로만 |
| `DREAMS.md` | 백그라운드 정리 결과(사람 검토용) | — |

- 메모리 파일 하나 기본 상한 10,000자 (`extensions/memory-core/src/memory-budget.ts:17`).
- 색인: `MEMORY.md`, `USER.md`, `memory/*.md`를 400토큰 · 80토큰 겹침 청크로 에이전트별 DB에 저장 (`docs/concepts/memory-builtin.md` "How indexing works").
- 테이블 (`src/state/openclaw-agent-schema.sql:689-767`): `memory_index_sources(path, source, hash, mtime, size)`, `memory_index_chunks(id, path, source, start_line, end_line, hash, model, text, embedding, updated_at)`, `memory_index_chunk_recall_metadata(chunk_id, importance 1~10, triggers, project_key)`, `memory_index_chunk_provenance(chunk_id, origin_class: owner/agent/untrusted/system, session_kind: interactive/cron/heartbeat/subagent/..., observed_at, supersedes_key)`, `memory_embedding_cache`.
- 검색 순서: 벡터 검색 ∥ 키워드(BM25, 모든 단어 → 없으면 아무 단어로 한 번 재시도) ∥ 파일명 → 가중 합(키워드 점수는 그 질의의 최고 의미 점수를 넘지 않게 축척. 식은 triggers-approval-memory-deep.md 4.1의 4번, 기본 가중치 0.7 / 0.3 `O:src/agents/memory-search.ts:163-169`, `O:extensions/memory-core/src/memory/hybrid.ts:47-64` (바로잡음 2026-11-14: triggers-approval-memory-deep.md 바로잡을 곳)) → `관련도 × 최신성 감쇠 × 중요도` → MMR로 중복 줄이기. 일일 메모는 반감기 30일, `MEMORY.md` · `USER.md`는 감쇠 없음. `minScore`는 감쇠 전 점수로 판단 (`docs/concepts/memory-search.md:139-199`).
- 임베딩이 없으면 키워드만(`provider: "none"`). 이름을 지정한 공급자가 죽으면 조용히 키워드로 떨어지지 않고 "unavailable"을 알린다. 한중일은 trigram 토크나이저 (`memory-search.md:200-240`, `memory-builtin.md:2-9`).
- 자동 회상: 들어온 메시지를 청크의 짧은 trigger 문구와 비교해 최대 3개를 숨은 맥락으로 넣는다. 대상은 신뢰된 항목(`MEMORY.md`, `USER.md`)만, 일일 메모 · 외부 내용은 자동으로 넣지 않는다. 준비 예산 1초, 넘으면 건너뜀 (`memory-search.md:218-240`, `memory.md:208-215`).
- 도구 스키마 (`extensions/memory-core/src/memory-tool-contract.ts:27-49`):
  - `memory_search { query(필수), maxResults(≥1), minScore, corpus: memory | wiki | all | sessions }`
  - `memory_get { path(필수), from(≥1), lines(≥1), corpus: memory | wiki | all }`
- 압축 전 기억 저장(memory flush): 압축 임계 4,000토큰 전, 또는 대화 기록 2MiB에서 숨은 턴 하나를 돌려 오늘 일일 메모에 **새 내용만 추가**하게 한다(이미 있는 문단을 다시 쓰면 거부). 설정 `agents.defaults.compaction.memoryFlush.{enabled, softThresholdTokens, forceFlushTranscriptBytes}` (`memory.md:292-336`, `extensions/memory-core/src/flush-plan.ts:13-14`, `src/config/zod-schema.agent-defaults-base.ts:186-197`).
- "행동에 영향을 주는 기억"은 언제 행동해도 되는지(조건 · 만료 · 권한 주체 · 피할 행동)를 같이 적으라고 하고, **기억은 정책을 강제하지 않는다**고 못 박는다. 강제는 승인 · 샌드박스 · 일정으로 (`memory.md:112-162`).
- 백그라운드 정리(dreaming): 일일 메모를 `MEMORY.md`로 승격. 단기 승격 반감기 14일 (`extensions/memory-core/src/short-term-promotion.ts:26-29`).

#### Hermes와 차이

- Hermes는 짧은 두 파일(2,200자 / 1,375자)을 매 세션 넣는 것뿐이고, 쌓이는 기억을 찾는 길이 없다. 대화 FTS 검색만 있다.
- OpenClaw는 "짧게 넣는 층(MEMORY/USER)"과 "검색으로만 꺼내는 층(일일 메모)"을 나누고, 출처 신뢰 등급을 청크마다 단다.

#### 가격인하에 가져올 방식 [추정]

- **섞기.** 프롬프트 주입은 Hermes 규칙(대화 중 시스템 프롬프트 고정, volatile 층)을 지키고, OpenClaw의 일일 메모 + 검색 + 출처 등급을 더한다.
- 에이전트별 폴더: `MEMORY.md`(투자 원칙 · 현재 포지션 요약 · 확정 결정), `memory/YYYY-MM-DD.md`(그날 판단 근거 · 회의 요약 · 체결), 검색 도구 2개.
- 출처 등급을 그대로 쓴다: 사람이 쓴 것 = owner, 에이전트 판단 = agent, 공시 · 뉴스 원문 = untrusted. **untrusted는 자동 회상에 넣지 않는다**(뉴스 속 지시문이 판단을 끌고 가는 것을 막음).
- 1차는 키워드(FTS5 + trigram, 한국어) + 최신성 감쇠만. 임베딩은 나중에.
- 압축 전 기억 저장(4,000토큰 전, 새 내용만 추가)은 24시간 세션에서 판단 근거를 잃지 않게 해서 바로 가져온다.
- 한도 · 손절 규칙은 기억이 아니라 2.2의 가드 · 템플릿으로 강제한다.

### 2.4 (e) 구독 OAuth

#### OpenClaw 동작 [관찰]

- 원칙 "token sink": 에이전트마다 자격 증명을 한 곳에서만 읽는다. 자체 OAuth 프로필이 생긴 뒤에는 그 refresh 토큰이 정본이고, 거부되면 외부 CLI 토큰으로 몰래 넘어가지 않고 재인증을 요청한다. 1회용 refresh 토큰을 두 프로그램이 번갈아 써서 한쪽이 로그아웃되는 문제를 막으려는 것 (`docs/concepts/oauth.md:53-73`).
- 저장: 공유 DB + 에이전트 DB의 `auth_profile_store(store_key, store_json, updated_at)`, `auth_profile_state(state_key, state_json, updated_at)`(순서 · 마지막 성공 · 쿨다운 · 사용량) (`oauth.md:75-93`, `src/state/openclaw-agent-schema.sql:677-688`).
- Anthropic (`src/llm/utils/oauth/anthropic.ts`): Claude CLI 재사용을 먼저, 그다음 `claude setup-token` 붙여 넣기, 그다음 PKCE. PKCE 콜백은 루프백(`127.0.0.1` 기본, 허용 호스트 localhost · 127.0.0.1 · ::1) 포트 53692 `/callback`, 대기 5분, 토큰 응답 16MiB 상한 (`:19-41`), 로그인 `:230`, 갱신 `:319`. 약관 판단은 문서에 "Anthropic 직원이 허용했다"는 서술로만 있다 (`oauth.md:134-159`).
- OpenAI Codex(ChatGPT 구독):
  - PKCE: `auth.openai.com/oauth/authorize`(scope `openid profile email offline_access`) → `localhost:1455/auth/callback` 수신, 동시에 URL/코드 붙여 넣기도 받아 **먼저 끝나는 쪽**을 씀 → `/oauth/token` 교환 → access 토큰 claim에서 `accountId` 추출 → `{access, refresh, expires, accountId}` 저장 (`oauth.md:187-216`, `extensions/openai/openai-chatgpt-oauth-authorization.runtime.ts:5`, `extensions/openai/openai-chatgpt-oauth-token.runtime.ts:23`, `extensions/openai/provider-contract-api.ts:38`).
  - **device code(브라우저 없는 서버용)**: `/api/accounts/deviceauth/usercode`로 사용자 코드 발급 → 사람이 다른 기기에서 입력 → `/api/accounts/deviceauth/token`을 폴링(기본 5초, 최소 1초, 서버가 준 interval 우선) → `/oauth/token`. 전체 15분, 요청당 30초 (`extensions/openai/openai-chatgpt-device-code.ts:25-33, 89-91, 209-347`).
  - 구독 사용량 조회 결과로 쿨다운: 짧은 폭주 15초, 조회 실패 30초, HTTP 오류 5분, 토큰 만료 12시간, 죽은 계정 24시간 (`src/agents/auth-profiles/usage.ts:109-115`).
- 갱신: `expires`가 지났으면 갱신해 **그 자격 증명의 주인 DB에** 다시 쓴다(공유에서 읽었으면 공유에). 외부 CLI 자격 증명은 다시 읽기만 하고 복사한 refresh 토큰을 쓰지 않는다 (`oauth.md:220-236`). 비교 후 교체(CAS)로 덮어쓰기 경쟁을 막는다 (`src/agents/auth-profiles/oauth-refresh-fence.ts:18-19`).
- 순서 · 회전 (`docs/concepts/model-failover.md` "Rotation order" 이후):
  1. 저장된 순서 지정 > `auth.order[provider]` > `auth.profiles` > 저장된 프로필.
  2. 지정이 없으면: 종류(OAuth → 정적 토큰 → API 키) → OAuth 중 만료 안 된 것 먼저 → `lastUsed` 오래된 것 먼저 → 쿨다운 · 비활성은 맨 뒤 (`src/agents/auth-profiles/order.ts:292-300`).
  3. 세션마다 고른 프로필을 고정(프롬프트 캐시 보호). `/new`, 쿨다운 때만 바뀐다. 압축은 바꾸지 않는다.
  4. 예: `auth.order.openai: ["openai:<구독>", "openai:<API 키 백업>"]` → 구독 한도에 걸리면 Codex가 준 정확한 재설정 시각을 기록하고 다음 프로필로.
- 쿨다운: rate limit은 공급자가 준 시간만큼만(지어내지 않음). 그 밖의 오류는 30초 → 1분 → 5분(상한). 과금 오류 · 확실한 영구 인증 실패는 10분 비활성 후 다시 시도. 상태는 `usageStats{"provider:profile": {lastUsed, cooldownUntil, errorCount}}` (`model-failover.md` "Cooldowns" · "Billing disables" 절).

#### Hermes와 차이

- Anthropic은 Hermes 쪽이 더 깊게 확인돼 있다(프로세스 간 파일 락 안에서 갱신, 영구 실패 지문, "소비됐지만 커밋 안 됨" 표시). OpenClaw는 DB CAS로 같은 문제를 푼다.
- Codex 로그인 · device code · 구독 한도 → API 키 백업 순서 · 사용량 조회 쿨다운은 Hermes 문서에 없다.
- Hermes 풀 전략(fill_first/round_robin/random/least_used)보다 OpenClaw 순서 규칙이 "구독 먼저, API 키 백업"을 그대로 표현한다.

#### 가격인하에 가져올 방식 [추정]

- **섞기.** Anthropic 갱신 규칙은 Hermes 것(락 안에서 읽기 · POST · 쓰기, 영구 실패 지문), 자격 증명 정본은 OpenClaw "token sink" 원칙(빌려 쓴 외부 토큰은 다시 읽기만).
- Codex는 OpenClaw 것: PKCE(집 PC) + **device code(VPS · 미니 PC)**. 모바일 웹앱에서 사용자 코드를 보여 주면 서버에 브라우저가 없어도 로그인할 수 있다.
- 순서 · 쿨다운 표(OAuth → 토큰 → API 키, 세션 고정, 30초/1분/5분, 과금 10분, rate limit은 공급자 시간)를 그대로 쓴다. 구독이 막히면 API 키로 넘어가 24시간 운용이 끊기지 않는다.
- 구독 OAuth의 약관 위험은 Hermes 문서 4.2의 판단과 같다. 기본은 API 키, 구독은 사용자가 고르는 선택지 [추정].

---

## 3. 가져옴 목록

| 기능 | 출처 (OpenClaw 경로) | 판정 | 이유 [추정] |
|---|---|---|---|
| 조건 스크립트 결과 `{fire, message, state}` + 상태 16KB + 실패 시 상태 롤백 | `src/cron/types.ts:403-405`, `src/cron/trigger-script.ts:103-111`, `docs/automation/cron-jobs/schedules.md` | 가져옴 | Hermes wake gate의 상위 호환. 시세 · 공시 중복 알림 제거 |
| 조건 평가 동시 3개 · 실행 슬롯과 분리 · 30초 · 도구 5회 | 같은 곳 | 가져옴 | 평가가 실행을 굶기지 않게 |
| stream 일정(line/match, 배치 250ms · 16KiB, 대기 배치 1개 합치기) | `schedules.md` Stream sources | 가져옴 | 실시간 시세 피드를 무한 대기열 없이 받음 |
| HTTP hooks `/wake` · `/agent` · 매핑 · 팬아웃 200 · 멱등 · 수락/완료 분리 · 인증 실패 제한 | `src/gateway/hooks.ts:40-42`, `src/gateway/hooks-mapping.ts:97`, `src/gateway/server/hooks.ts:61`, `src/gateway/server/hooks-request-handler.ts:52-53`, `docs/gateway/config-hooks.md` | 가져옴 (축소) | 공시 · 뉴스 수집기가 서버를 깨우는 입구. JS 변환은 빼고 템플릿만 |
| 같은 논리 키 직렬화 | `config-hooks.md:172-177` | 가져옴 | 같은 종목 사건 겹침 방지 |
| `activeHours` · `idleOnly` · `skipIfScratchEmpty` | `schedules.md:65-88`, `src/cron/active-hours.ts` | 가져옴 | 장 시간 · 휴장 |
| pacing `next_check` (범위 안 자르기) | `schedules.md:230-238` | 가져옴 | 변동성에 따른 주기 |
| 스크래치 + `record_result` 5종 + 직전 결과 주입 | `docs/automation/cron-jobs/payloads.md:349-391`, `src/cron/scratch-contract.ts:2` | 가져옴 | 감시 작업의 상태 · 결과 표준화, 다른 에이전트로 넘길 신호 |
| standing intent (단어 조건 · 쿨다운 · 횟수 · 만료) | `docs/concepts/standing-intents.md`, `src/state/openclaw-agent-schema.sql:768-` | 나중에 | 채팅 중심 기능. 1차는 조건 스크립트로 충분 |
| MCP Events (서명 웹훅 구독) | `docs/automation/mcp-events.md`, `extensions/mcp-events/` | 나중에 | 시세 · 뉴스 MCP 서버를 쓸 때 |
| `on-exit` 일정 | `schedules.md:21, 34-36` | 안 가져옴 | 투자 팀에 쓸 곳이 없음 |
| `before_tool_call` 결과(block / requireApproval) + 결정 5종 + 기본 120초 · 상한 600초 | `src/plugins/hook-before-tool-call-result.ts`, `src/infra/plugin-approvals.ts:80-84`, `src/agents/agent-tools.before-tool-call.approval.ts:70-132` | 가져옴 | 주문 한도 가드의 뼈대 |
| 모르는 결정 = 거절, hook 실패 = 차단, 실행 직전 재확인, 승인 뒤 인자 변경 무시 | 같은 곳, `src/agents/agent-tools.before-tool-call.policy.ts:50-51`, `docs/tools/exec-approvals.md:739-785` | 가져옴 | 주문은 실패하면 닫혀야 한다 |
| 승인 범위 표시(payment: 금액 · 통화 · 대상) | `src/infra/approval-scope.ts` | 가져옴 (주문 범위로 확장) | 모바일 카드에 무엇을 승인하는지 정확히 |
| 승인 화면 없으면 즉시 거부 | `exec-approvals.md:611-621` | 가져옴 | 무인 시간대 무단 주문 방지 |
| standing grant(작업 정의에 묶임 · 만료) | `exec-approvals.md:623-665` | 가져옴 (주문 외 도구만) | 수집 스크립트 반복 승인 줄이기 |
| 도구 정책 deny 우선 · allow 화이트리스트 · 그룹 | `docs/gateway/sandbox-vs-tool-policy-vs-elevated.md:61-111` | 가져옴 (3층으로 축소: 전역 · 역할 · 작업) | 역할별 도구 제한 |
| exec mode 5종 · `askFallback` · 승인 문서 CAS | `exec-approvals.md:115-301` | 나중에 | 1차는 셸 실행을 거의 안 씀 |
| 같은 오류 3회 연속 차단 | `docs/tools/loop-detection.md:11-16` | 가져옴 | 주문 API 오류 반복 방지 |
| 샌드박스(Docker) · elevated · 노드 호스트 | `docs/gateway/sandboxing.md` 등 | 안 가져옴 | 1인 홈서버, 범위 밖 |
| 기억 파일 4층(USER · MEMORY · 일일 · DREAMS) | `docs/concepts/memory.md:26-79` | 가져옴 (DREAMS 제외) | 쌓이는 판단 근거 |
| 청크 400/80 · FTS5 + trigram · 최신성 반감기 30일 · MMR | `docs/concepts/memory-search.md`, `docs/concepts/memory-builtin.md` | 가져옴 (임베딩 · MMR은 나중에) | 한국어 키워드 검색부터 |
| 청크 출처 등급 owner/agent/untrusted/system | `src/state/openclaw-agent-schema.sql:726-734` | 가져옴 | 뉴스 원문을 자동 회상에서 제외 |
| `memory_search` · `memory_get` 스키마 | `extensions/memory-core/src/memory-tool-contract.ts:27-49` | 가져옴 | 도구 계약 그대로 |
| 압축 전 기억 저장(4,000토큰 전 · 2MiB · 새 내용만 추가) | `extensions/memory-core/src/flush-plan.ts:13-14`, `memory.md:292-336` | 가져옴 | 24시간 세션 |
| dreaming(백그라운드 승격) | `extensions/memory-core/src/dreaming*.ts` | 나중에 | 모델 호출 비용, 1차엔 사람이 정리 |
| memory-wiki · Honcho · LanceDB | `memory.md:253-290` | 안 가져옴 | 과함 |
| token sink 원칙 · 주인 DB에 갱신 쓰기 · CAS | `docs/concepts/oauth.md:53-93, 220-236`, `src/agents/auth-profiles/oauth-refresh-fence.ts` | 가져옴 | 구독 로그아웃 사고 방지 |
| Codex PKCE(1455) + 붙여 넣기 경쟁 | `extensions/openai/openai-chatgpt-oauth-*.ts` | 가져옴 | 구독 OAuth 요구 |
| Codex device code (15분 · 5초 폴링) | `extensions/openai/openai-chatgpt-device-code.ts` | 가져옴 | VPS · 헤드리스 로그인 |
| 프로필 순서 규칙 · 세션 고정 · 쿨다운 30초/1분/5분 · 과금 10분 · 사용량 조회 쿨다운 | `src/agents/auth-profiles/order.ts:292-300`, `src/agents/auth-profiles/usage.ts:109-115`, `docs/concepts/model-failover.md` | 가져옴 | 구독 → API 키 백업 |
| Anthropic PKCE 루프백 콜백(53692) | `src/llm/utils/oauth/anthropic.ts:19-41` | 나중에 | Hermes의 붙여 넣기 방식이 헤드리스에 더 맞음 |
| Claude CLI 재사용 · 개인 계정(다중 사용자) | `oauth.md:134-159, 265-` | 안 가져옴 | 1인용, 약관 위험 |

---

## 4. Rust 재구축 시사점 [추정]

1. **트리거는 한 개의 enum + 한 개의 게이트 계약.** `Schedule = At | Every | Cron | Stream{argv, mode, match, batch_ms, max_batch_bytes}`, 외부 입구는 `HookAction = Wake{text, target} | Agent{message, agent_id, session_key, session_mode, ...}`. 모든 길이 `TriggerEval -> {fire, message, state}`를 거쳐 같은 실행 대기열로 들어가게 하면 Hermes cron 보장(선이동 · 이중 발사 방지)을 한 곳에서만 지키면 된다.
2. **평가와 실행의 자원 분리.** 조건 평가는 별도 세마포어(3), 실행은 별도(8). 평가 30초 · 도구 5회는 tokio `timeout` + 카운터. 평가가 실패하거나 본 작업이 실패하면 `state`를 커밋하지 않는 트랜잭션 순서를 지킨다.
3. **스트림 소스는 작업당 "실행 1 + 대기 배치 1".** 무한 채널 대신 `watch`/덮어쓰기 슬롯으로 합치면 메모리가 시세 속도에 비례해 늘지 않는다. 자식 프로세스는 프로세스 그룹으로 띄우고 종료를 확인한 뒤에만 재시작.
4. **HTTP 입구는 수락과 완료를 나눈다.** `runId`를 바로 돌려주고, 같은 논리 키(종목) 작업은 키별 직렬 큐로. 멱등 표는 "진행 중은 무기한, 끝난 것은 5분 · 1,000개"로 메모리 상한이 생긴다. 인증 실패 제한(20회/60초)은 IP별 작은 맵.
5. **주문 가드는 도구 실행기 앞의 필수 단계.** `Guard::check(args, limits) -> Allow | Block(reason) | Ask(scope)`. `Ask`는 승인 요청 행을 DB에 쓰고(재시작해도 남게), 결정은 `AllowOnce | Deny | Timeout`만 받는다. 시간 초과 · 모르는 값 · 가드 패닉 = 거절. 승인 뒤 실행 직전 `check`를 한 번 더.
6. **한도 템플릿은 설정 파일, 승인 기록은 DB.** 한도(1회 · 일 누적 · 비중 · 시간대)는 Hermes 설정 체계(템플릿 · 프로필 overlay)에 두고, 일 누적은 체결 기록 테이블에서 계산한다. 기억에 두지 않는다.
7. **기억은 SQLite 한 파일에.** 에이전트별 DB에 `memory_sources / memory_chunks(+FTS5 trigram) / memory_provenance`. 임베딩 열은 nullable로 두고 1차는 비운다. 파일이 정본이고 색인은 다시 만들 수 있게(hash · mtime 비교).
8. **자격 증명은 DB 행 + CAS.** Hermes의 파일 락 방식 대신 OpenClaw처럼 `auth_profile_store` 행을 비교 후 교체하면 OS별 파일 락 차이(Windows 포함)를 피한다. 갱신 자체는 프로필별 단일 작업(`Mutex`)으로 직렬화. device code 폴링은 취소 가능한 tokio 작업.
9. **세션 단위 프로필 고정.** 세션 구조체에 선택된 auth 프로필 id를 두고, 쿨다운 · `/new` 때만 바꾼다(프롬프트 캐시 보호 = Hermes 불변식 1과 같은 목적).

---

## 5. 라이선스 · rea · 못 본 부분

### 5.1 라이선스

- OpenClaw는 MIT다 (`LICENSE`). 이 문서는 코드를 옮기지 않고 동작 · 구조 · 스키마만 적었다. 구현 중 코드 일부를 그대로 가져오게 되면 MIT 고지(저작권 표시 + 허가 문구)를 가격인하 배포물에 함께 넣는다.
- 서드파티 구성요소는 각자 라이선스를 따른다 (`THIRD_PARTY_NOTICES.md`). 이번에 본 영역은 OpenClaw 자체 코드다 [관찰].
- DeskRPG(Sustainable Use License)와 달리 사용 · 배포 제한이 없다 [관찰: `LICENSE` 전문은 표준 MIT 문구].

### 5.2 rea가 한 일 · 못 한 일

| 구분 | 내용 |
|---|---|
| 한 일 | 파일 묶음 4개(17~20개씩)를 각 3~4초, 최대 RSS 309~386MB로 분석. 숫자 · 문자열 리터럴 위치(타임아웃 · 상한 상수 대부분을 여기서 찾음: `trigger-script.ts:103-111`, `hooks.ts:40-42`, `plugin-approvals.ts:80-84`, `exec-approvals-core.ts:266`, `usage.ts:109-115`, OAuth 포트 등), 타이머 위치(`trigger-script.ts:439`, `memory/file-watcher.ts:508, 557`), `JSON.parse` 같은 입력 경계, Promise/await 위치, 파일별 import 지정자, 도구 JSON 스키마 리터럴(`memory-tool-contract.ts`) |
| 못 한 일 | ① 큰 루트(`src/` 전체)에서는 `--source-path`를 줘도 29,589개를 다 처리하다 결과 용량 초과로 실패(164초, RSS 약 3.1GB). 그래서 고른 파일만 빈 폴더에 복사해 루트로 쓰는 방식으로 바꿨다. ② import는 대부분 `resolution_status: not-found`(묶음 밖 파일). ③ 상태 기계 · 호출 순서 · 문자열 인자의 의미는 못 냈다(미해결 항목 묶음당 2,364~5,267개). ④ SQL 스키마(`.sql`)와 문서(`.md`)는 대상이 아니다. 따라서 동작 · 순서 · 스키마는 grep과 파일 · 문서 읽기로 채웠다 |

### 5.3 못 본 부분

1. OpenClaw cron 저장소의 이중 발사 방지 · catch-up 규칙(`src/cron/store/*`, `src/cron/run-admission.ts`). Hermes 규칙을 쓰기로 해서 깊게 보지 않았다.
2. 스트림 일정의 "빠른 실패 5회" 규칙의 정확한 조건(문서 문장 일부만 확인)과 재시작 backoff 값.
3. 조건 스크립트 런타임(QuickJS 기반 code mode, `extensions/code-mode-quickjs/`)의 격리 방식. 가격인하가 사용자 스크립트를 돌릴지 정하기 전에 봐야 한다.
4. HTTP hooks 매핑의 템플릿 렌더러 세부와 Gmail 프리셋.
5. 승인 요청이 재시작 뒤 어떻게 복구되는지(`src/infra/agent-run-approval-leases.ts`, `approval-native-*`). 복구하지 않는다. 다른 `runtime_epoch`의 대기를 `cancelled / gateway-restart`로 닫는다 (`O:src/gateway/operator-approval-store.transitions.worker.ts:245-276`) (바로잡음 2026-11-14: triggers-approval-memory-deep.md 바로잡을 곳)
6. 기억 검색의 가중치 기본값(`vectorWeight` 등)과 MMR λ, `minScore` 기본값. 가중치 기본값은 0.7 / 0.3이다 (`O:src/agents/memory-search.ts:163-169`, `O:extensions/memory-core/src/memory/hybrid.ts:47-64`) (바로잡음 2026-11-14: triggers-approval-memory-deep.md 바로잡을 곳)
7. Anthropic OAuth 갱신의 실패 분류 세부(`src/agents/auth-profiles/oauth-refresh-failure.ts`).
8. OpenClaw의 에이전트 간 메시지(`sessions_send`, `agent-send`, A2A)와 라우팅 바인딩(`docs/concepts/multi-agent.md`, `agent-bindings.md`). (b)를 충분함으로 판정해서 보지 않았다. Hermes `message_agent` 스키마를 확인하지 못하면 이쪽을 대안으로 본다.
9. OpenClaw 장기 실행 안정성(재시작 복구 `docs/gateway/restart-recovery.md`, gateway lock, 힙 플래그). (f)를 충분함으로 판정해서 보지 않았다.
10. 저장소에 `extensions/migrate-hermes/`(Hermes의 모델 설정 · `SOUL.md` · `MEMORY.md` · `USER.md` · 스킬 · MCP를 가져오는 도구)가 있다는 것만 확인했다. 두 시스템 개념의 대응표로 쓸 수 있다.
11. 모든 값은 소스 · 문서로만 확인했다. 실행해서 재지 않았다(실행 금지).
