# 트리거 · 승인 · 장기 기억 · 접속 인증 · 백업 깊게 뜯기 (2026-11-14)

결론: 원본과 같게 만들 수 있을 만큼 뜯은 것은 예약 실행 · 승인 · 기억 검색이고, "답 없는 승인을 팀이 판단" · "다시 켠 뒤 승인 잇기" · "되살리면 멈춤"은 어느 원본에도 없어 새로 정해야 한다.

1. **트리거**: 두 원본 모두 "실행 전에 표시하고, 죽었다 켜지면 다시 돌리지 않는다". Hermes는 파일에 표시(`pending_slot` · `run_claim`)를, OpenClaw는 SQLite 영수증(`(작업, 예정 시각)` 번호 + "실행 중은 하나" 유일 인덱스)을 쓴다. SQLite 한 곳을 쓰는 우리에게는 OpenClaw 쪽이 맞다. Hermes는 작업 파일을 매 tick 다시 읽는다(확인).
2. **승인**: 대기 중인 승인을 DB에 두는 것은 OpenClaw뿐이고(`operator_approvals`), 상태 기계가 표 제약으로 박혀 있다. 승인한 내용과 실행할 내용을 맞춰 보는 장치도 OpenClaw에만 있다(argv · cwd · 환경 해시 · 스크립트 파일 해시). 세 원본 모두 **답 없음 = 거절**이고 다시 켜면 대기 승인을 닫는다. 우리 스펙의 "답이 없으면 팀이 판단" · "다시 켠 뒤 이어서"는 원본에 없어 새로 정해야 한다.
3. **기억 검색**: 가중치 기본값을 확인했다(벡터 0.7 · 키워드 0.3, 후보 4배, 결과 6, 문턱 0.35, MMR 0.7, 반감기 30일, 조각 400/80). 한글은 두 원본 모두 trigram + 3글자 미만 LIKE로 푼다.
4. **접속**: Hermes 게이트 모드의 쿠키 5종과 시도 제한(IP당 60초 10번, 잠금 없음)을 확인했다. "여러 번 틀리면 잠근다"와 "키를 바꾸면 기존 연결을 끊는다"는 OpenClaw에 있다(5분 잠금, 인증 세대).
5. **백업**: 두 원본 모두 묶음 자체를 암호화하지 않고, 되살린 뒤 멈춘 상태로 두는 장치도 없다(Hermes는 오히려 서비스를 다시 살린다). 우리 스펙의 두 요구는 원본과 다르게 새로 만든다.

- 뜯은 것: Hermes Agent · OpenClaw · DeskRPG · DeskRPG Hermes 플러그인의 예약 실행, 깨우기, 승인, 기억 검색, 접속 인증, 백업 · 옮기기. `docs/dev/conformance.md` "더 뜯어야 할 것"의 T11 · T14 · T16 · T17 · T19 줄을 메운다.
- 기존 조사 문서(`hermes-agent.md`, `openclaw.md`, `deskrpg.md`, `deskrpg-hermes-plugin.md`, `remote-access-security.md`)에 이미 있는 것은 다시 쓰지 않고 절 번호로 가리킨다.
- 표기: [관찰] = 소스에서 직접 확인, [추정] = 해석. 근거는 `H:`(Hermes) · `O:`(OpenClaw) · `D:`(DeskRPG) · `P:`(플러그인) + 저장소 루트 기준 `경로:줄`.
- 라이선스: Hermes MIT, OpenClaw MIT, DeskRPG Sustainable Use(방식만 참고), 플러그인 LICENSE 없음(명세만 참고). 코드는 옮기지 않았다(구조 · 흐름 · 이름 · 숫자만).

| 항목 | 값 |
|---|---|
| Hermes commit | `5c1abd25` |
| OpenClaw commit | `c19ae286a00c` (`openclaw.md` 머리말과 같은 받은 소스) |
| 방법 | 받은 소스를 읽기만 했다. 실행 · 설치 없음 |

---

## 1. 예약 실행 (cron)

### 1.1 Hermes

기존 문서 `hermes-agent.md` 6.1 · 7.3에 작업 칸 목록, 일정 3종, tick 보장 9개가 있다. 여기서는 그 문서가 못 본 것(작업 파일을 매 tick 다시 읽는가, 한 tick의 정확한 순서, 놓친 실행의 계산, 실패 뒤 처리, 서머타임)만 적는다.

#### 동작 원리 [관찰]

**ticker (누가 언제 tick을 부르나)**

1. 게이트웨이 프로세스 안의 스레드 하나가 60초마다 `tick()`을 부른다(`interval=60`). 프로필이 여럿이어도 ticker는 하나이고 프로필을 차례로 돈다 (`H:cron/scheduler_provider.py:152, 439, 540`).
2. tick마다 `ticker_heartbeat` 파일에 `<시각> <pid>`를, 성공한 tick에는 `ticker_last_success`에 시각을 적는다. "살아서 도는 것"과 "실제로 발사하는 것"을 구분하려고 둘을 따로 둔다 (`H:cron/jobs.py:1273-1289`).
3. tick이 주기를 넘겼거나 컴퓨터가 잠들었다 깨면, 밀린 만큼 연달아 부르지 않고 다음 tick 시각을 지금 기준으로 다시 잡는다 (`H:cron/scheduler_provider.py:530-537`).
4. 파일 핸들이 바닥난 오류(EMFILE)가 이어지면 대기를 실패마다 두 배로 늘리고 15분에서 멈춘다 (`H:cron/scheduler_provider.py:19-35`).

**한 tick의 순서** (`H:cron/scheduler_tick.py:33-204`)

1. 게이트웨이가 방금 새 코드로 다시 켜졌는지 본다. 디스크의 코드와 떠 있는 코드가 다르면 이 tick을 양보한다(예외로 빠짐).
2. 프로필별 tick 잠금 `<home>/cron/.tick.lock`을 잡는다. 못 잡으면 0을 돌려주고 끝.
3. 전체 정지(estop) 표시를 본다. 멈춰 있으면 끝 (`agent.estop.check_paused("cron")`).
4. `can_dispatch()`가 거짓이면(게이트웨이가 하던 일을 비우는 중) 끝. 기한이 된 작업은 다음 tick에 남는다.
5. 발사 전 정리(`_pre_dispatch_housekeeping`), 작업 폴더 정리.
6. `get_due_jobs()`: **잠금을 잡고 `jobs.json`을 통째로 다시 읽는다**(캐시 없음). 그 안에서 아래 "기한 판정"을 작업마다 돌리고, 고친 것이 있으면 한 번에 저장한다 (`H:cron/jobs.py:3269-3313`).
7. 이 프로세스에서 너무 오래 실행 중으로 남은 작업을 치운다(`_sweep_stale_inflight_for_tick`).
8. headless(`hermes cron tick`, 게이트웨이 밖)면 에이전트 작업을 빼고 `last_fire_error`만 남긴다.
9. 기한 된 작업이 없으면 저장소 건강 상태만 다시 보고 끝.
10. 저장소에 쓸 수 없는 상태면(`store_health.dispatch_blocked`) 발사하지 않는다. 쓸 수 없는데 발사하면 표시를 못 남겨 두 번 나갈 수 있기 때문이다.
11. `_advance_or_drop_recurring`: 반복 작업의 `next_run_at`을 **실행 전에** 다음 회차로 옮긴다. 옮기지 못한 작업은 이번에 빼 버린다.
12. 프로필별 스레드 풀(`cron.max_parallel_jobs`, 없으면 제한 없음)에 작업을 넣는다. `sync=True`면 다 끝날 때까지 기다리고, 아니면 넣고 바로 돌아온다.
13. tick 잠금을 푼다(실행은 풀 안에서 계속된다).

**기한 판정 (작업 하나)** (`H:cron/jobs.py:3177-3266`). 순서가 중요하다고 원본이 못 박고 있다.

1. 일회성(`once`) 작업에 살아 있는 `run_claim`(다른 프로세스가 잡은 실행 표시, TTL 1800초보다 젊음)이 있으면 건너뛴다. 시각이 미래이거나 깨진 표시는 "낡은 것"으로 본다.
2. `next_run_at`을 정한다: 주인 없는 `pending_slot` 복구 → 저장된 값 → 없으면 일정으로 다시 계산.
3. 이 회차가 이미 끝난 것으로 장부에 있으면(`completed_occurrence`) 다음 회차로 옮기고 건너뛴다.
4. 시간대를 고정하지 않은 cron 작업은 프로필 시간대가 바뀌어 벽시계가 어긋났는지 보고 고친다. 고정한 작업은 이 수리를 **하지 않는다**(하면 기한 된 회차를 미래로 읽어 영영 안 나간다).
5. 오류 상태로 한 주기 넘게 굳은 반복 작업을 다시 건다.
6. `next_run_at`이 아직 미래면 끝(기한 아님).
7. 놓친 반복 실행 빨리 감기(아래 "놓친 실행").
8. 일회성이면: 유예가 지난 것은 은퇴시키고, 발사 횟수 한도(`repeat.times`)에 닿았으면 건너뛰고, 아니면 `run_claim = {at, by(기계 id)}`을 **디스크에 먼저 적는다**. 유예 120초를 넘겨 늦게 나가는 것이면 `outage: true`를 붙인다.
9. 반복이면 `last_dispatch = {scheduled_at, dispatched_at, lateness_seconds, kind}`와 `pending_slot`을 같이 적는다. `kind`는 늦은 정도로 `on_time`(300초 이내) / `late`(300초 초과, 유예 이내) / `catch_up`(유예 초과) (`H:cron/jobs.py:963-976`).

**놓친 실행(꺼져 있던 동안)** (`H:cron/jobs.py:943-960`, `H:cron/missed_runs.py:25-125`)

1. 유예(grace) = 주기의 절반을 120초~7200초로 자른 값. 작업의 `grace_minutes`가 있으면 그 값(단 120초 밑으로는 안 내려간다. tick이 늘 몇 초 늦게 오기 때문). 주기를 모르면 120초.
2. 늦은 정도 ≤ 유예: 그냥 지금 실행한다.
3. 늦은 정도 > 유예이고 반복 작업: `next_run_at`을 다음 회차로 즉시 옮겨 저장하고, 놓친 회차 수를 센다(1000개에서 멈춤).
   - 따라잡기가 켜져 있으면(작업의 `catch_up: true`, 없으면 `cron.catch_up_missed` 기본 true. 단 `grace_minutes`를 직접 준 작업은 false) **한 번만** 지금 실행하고, 나머지 회차는 실행 장부에 "missed ×N, collapsed into one catch-up run" 한 줄로 남긴다.
   - 꺼져 있으면 실행하지 않고 "skipped, catch-up off" 한 줄을 남긴다.
4. 일회성: 유예 120초(`ONESHOT_GRACE_SECONDS`)를 넘겼으면 `catch_up: true`를 준 작업만 늦게 한 번 나간다. 아니면 지우지 않고 은퇴 표시 + "missed ×1" 장부 줄. 끝난 일회성은 7일 뒤 정리(`COMPLETED_ONESHOT_RETENTION_DAYS`) (`H:cron/jobs.py:120, 2810`).

**실행이 끝난 뒤** (`H:cron/jobs.py:2432-2564`)

1. `mark_job_run(job_id, success, error, delivery_error, status, execution_id …)`이 잠금 안에서 결과를 적는다. 같은 `execution_id`가 이미 `canonical_completions`에 있으면 아무것도 하지 않는다(완료 보고가 두 번 와도 한 번만 반영).
2. `expected_fire_owner`가 지금 `fire_claim.by`와 다르면 낡은 완료로 보고 버린다.
3. `repeat.completed`를 올리고, `times`에 닿으면 `enabled=false, state="completed", next_run_at=null`. 아니면 **끝난 시각을 기준으로** 다음 실행을 다시 계산한다(tick이 미리 옮겨 둔 값은 잠정값).
4. 에이전트는 성공했는데 전달이 실패하면 `last_status = "delivery_failed"`이고 `failure_streak`은 건드리지 않는다.
5. 반복 작업의 다음 시각을 계산하지 못하면 끄지 않고 `state = "error"`로 둔다(조용히 꺼지지 않게).

#### 상태와 저장되는 것 [관찰]

| 무엇 | 어디 | 모양 |
|---|---|---|
| 작업 정의 + 상태 | `<home>/cron/jobs.json` | `{"jobs": [...], "updated_at": ISO}`. 칸은 `hermes-agent.md` 6.1 + 이 문서의 `run_claim{at, by, outage?}`, `fire_claim{by,…}`, `pending_slot`, `last_dispatch`, `manual_run_at`, `canonical_completions[]`, `catch_up`, `unreachable_retry{attempt, at, expr}`, `quota_hold_until` |
| 쓰기 방식 | 같은 폴더 | 잠금 파일(30초까지 기다림) 안에서 → 디스크에 그 사이 새로 생긴 작업이 있으면 합치고(최대 5번 다시 시도) → 임시 파일에 쓰고 fsync → 이름 바꾸기 → 폴더 fsync → 권한 좁히기 (`H:cron/jobs.py:118, 1556-1629`) |
| tick 잠금 | `<home>/cron/.tick.lock` | flock |
| 실행 출력 | `<home>/cron/output/<job>/<시각>.md` | 작업마다 최근 50개만 남김 (`H:cron/jobs.py:3323`) |
| ticker 생존 표시 | `<home>/cron/ticker_heartbeat`, `ticker_last_success` | 0600 |
| 실행 장부 · 전달 대기열 | SQLite `executions` · `deliveries` | `hermes-agent.md` 7.3 |

#### 실패했을 때 [관찰]

| 경우 | 동작 | 근거 |
|---|---|---|
| 에이전트가 600초 동안 아무 활동이 없음 | 강제로 끊는다. 벽시계가 아니라 유휴 시간이다. `HERMES_CRON_TIMEOUT`으로 바꾸고 0은 무제한. 붙은 스크립트는 따로 3600초 | `H:cron/AGENTS.md:21-25`, `H:cron/scheduler.py:1383` |
| 모델에 아예 닿지 못함(네트워크 · DNS 오류이고 API 호출 0번) | 반복 작업만 5분 · 15분 · 30분 뒤 자동으로 다시 돈다. 그동안 실패 알림은 보내지 않는다. 모델에 닿은 실행이 한 번이라도 있으면 사다리를 처음으로. `cron.retry_unreachable: false`로 끔 | `H:cron/unreachable_retry.py:3-39` |
| 공급자가 "N초 뒤 다시"라고 답한 한도 오류 | 그 시간(+60초 여유) 동안 반복 작업을 세워 둔다 | `H:cron/quota_hold.py:29-39` |
| 같은 반복 작업이 연달아 실패 | **자동으로 끄지 않는다.** 3번째(`cron.failure_nudge_threshold`, 0이면 끔)부터 실패 알림 끝에 "N번 연속 실패, 멈추려면 pause" 문장을 붙인다. 같은 실패 알림은 6시간에 한 번 | `H:cron/scheduler.py:137-163, 329` |
| 에이전트가 스스로 실패라고 알림 | 응답 첫 줄이 정확히 `[CRON_FAILURE]`이면 실패로 장부에 적는다 | `H:cron/scheduler.py:546-559` |
| 일반 실패의 자동 재시도 | 없다. 다음 회차를 기다린다 | `H:cron/unreachable_retry.py:7-13` |
| `jobs.json`이 깨짐 | 제어 문자 정도는 고쳐 읽고, 못 고치면 예외. 작업 하나가 깨졌으면 그 작업만 건너뛴다 | `H:cron/jobs.py:1388-1400, 3284-3301` |
| 저장소에 못 씀 | 수리는 메모리에만 두고 다음 tick에 다시 저장. 발사는 막는다 | `H:cron/jobs.py:3303-3308` |

**조용히 끝내기**: 최종 응답 전체, 또는 첫 줄이나 마지막 줄이 `[SILENT]`(또는 괄호 없는 `SILENT` / `NO_REPLY` / `NO REPLY`, 대소문자 무시)이면 전달하지 않는다. 문장 가운데 들어 있으면 보통 내용으로 보고 전달한다. 스크립트가 `wakeAgent=false`를 냈거나 출력이 비었을 때도 같은 표시로 처리한다 (`H:cron/scheduler.py:542, 562-574, 1550-1554`).

#### 시간대와 서머타임 [관찰]

- cron 식을 푸는 시간대: 작업의 `timezone` → 프로필의 시간대 → 기준 시각의 시간대 순 (`H:cron/jobs.py:1239`).
- 계산: 기준 시각을 그 시간대의 벽시계로 바꿔 croniter로 다음 벽시계 시각을 구한 뒤, 그 벽시계에 `fold=0`과 `fold=1`(가을에 두 번 오는 시각의 앞 · 뒤)을 차례로 입혀 **기준 시각보다 뒤인 첫 순간**을 고른다. 둘 다 아니면 다음 회차로 한 번 더 (`H:cron/jobs.py:1240-1257`).
  - 두 번 오는 시각(가을): 앞의 것에서 한 번 나가고, 뒤의 것은 기준보다 뒤일 때만 후보가 된다. 곧 "기준 시각보다 뒤"라는 검사 하나로 같은 순간이 두 번 나가지 않는다.
  - 사라지는 시각(봄): [추정] 파이썬은 없는 벽시계 시각을 앞쪽 오프셋으로 풀어 1시간 뒤 순간으로 만든다. 건너뛰지 않고 늦게 한 번 나간다. 따로 처리하는 코드는 없었다.
- 간격(`interval`) 일정은 UTC에서 분을 더한다. 서머타임과 무관하다 (`H:cron/jobs.py:1214`).
- 시계가 뒤로 갔을 때: 미래 시각의 `run_claim`은 낡은 것으로 본다. 그 밖의 전용 방어는 확인하지 못함.

### 1.2 OpenClaw

`openclaw.md` 2.1은 일정 종류 · activeHours · 조건 스크립트 · 스트림을 적었고, 이중 발사 방지는 "세부 미확인"으로 남겼다(5.3). 여기서 그 빈 곳을 메운다.

#### 동작 원리 [관찰]

**저장과 구조**

- 작업 · 실행 상태 · 실행 기록이 모두 공유 SQLite 상태 DB에 있다. 예전 `jobs.json`은 `doctor --fix`가 SQLite로 옮긴다 (`O:docs/automation/cron-jobs/how-it-works.md:17`, `managing-jobs.md:308`).
- 스케줄러는 작업자(worker) 하나가 일정 · 대기열 · 결과 기록을 맡고, 게이트웨이는 작업자가 확정한 실행 요청을 띄우기만 한다 (`how-it-works.md:33`).

**타이머** (`O:src/cron/store/scheduler-actor.worker.ts:209-254`)

1. 켜진 시간 일정 작업 가운데 실행 중이 아닌 것의 `max(nextRunAtMs, admissionDeferred.retryAtMs)` 최솟값을 다음 깨어날 시각으로 잡는다.
2. 타이머는 길어야 60초(`MAX_CRON_TIMER_DELAY_MS`)다. 다음 기한이 더 멀어도 60초마다 깨서 **벽시계가 바뀌었는지** 본다(타이머만으로는 시계 변경 · 절전을 알 수 없어서). 이때 SQL을 다시 읽지는 않는다.
3. 지연이 0이면 2초(`MIN_REFIRE_GAP_MS`)를 넣는다. 다음 실행 계산이 방금 끝난 실행과 같은 초를 돌려줄 때 무한히 다시 도는 것을 막는 안전망이다 (`O:src/cron/service/timer-constants.ts:1-16`).
4. cron이 꺼져 있거나 모든 채널이 멈춤이면 타이머를 걸지 않는다.

**tick** (`scheduler-actor.worker.ts:455-600`)

1. 벽시계 경과와 단조 시계 경과를 비교한다. 차이가 2초를 넘거나(시계가 앞으로 뜀), 깨어나기로 한 시각보다 2초 넘게 늦었으면 "따라잡기 필요"로 표시한다.
2. **DB에서 작업을 전부 다시 읽는다**(`readJobs`). tick 하나가 동기 명령이라 그 사이의 쓰기를 모두 본다.
3. 시작할 때이거나 따라잡기 필요면 "상한 있는 따라잡기"(아래), 아니면 보통 판정(`isRunnableJob`: 기한이 됐고 실행 중이 아님).
4. 맡을 에이전트가 없는 작업은 "ownerless"로 건너뛰고 기록한다.
5. 남은 작업마다 실행 요청을 만든다. **영수증 번호(receiptId)는 `(저장소 키, 작업 id, 예정 시각)`으로 만든다.** 같은 회차는 번호가 같으므로 두 번 접수되지 않는다 (`:561-573`).
6. 요청을 접수하고 동시 실행 한도 8개(`DEFAULT_CRON_MAX_CONCURRENT_RUNS`, `O:src/config/cron-limits.ts:3`) 안에서 띄울 것을 고른다.

**같은 작업이 두 번 나가지 않게** [관찰]

- `cron_run_receipts` 표가 "내구성 있는 실행 울타리"다. 상태 `running / ok / error / skipped / interrupted / superseded`, `running`이면 `finished_at_ms`가 비어 있어야 한다는 CHECK, 그리고 **`(store_key, job_id)`에 `status = 'running'`인 줄은 하나뿐**이라는 부분 유일 인덱스가 있다 (`O:src/state/openclaw-state-schema.sql:1479-1509`).
- 영수증에는 `owner_pid`, `owner_start_time`(주인 프로세스), `config_revision`(작업 정의의 판), `delivery_attempt_state`(`unknown / not-started / started`)가 있다. 전달을 시작했는지를 따로 적어 두어, 죽었다 켜진 뒤 "보냈을지도 모르는 결과"를 다시 보내지 않는다.

**다시 켠 뒤** (`O:src/cron/service/startup-run-repair.ts:30-81`, `how-it-works.md:22, 27, 31`)

1. `runningAtMs`가 남은 작업은 "중단됨"으로 닫는다: `lastRunStatus = "error"`, `lastError` = 시작 때 중단 문구, `lastDeliveryStatus = "unknown"`, 영수증은 `interrupted`.
2. **다시 실행하지 않는다.** "시작했는지 불확실한 일회성은 절대 다시 돌리지 않는다"는 주석이 있다. 일회성(`at`)은 대체 시각이 없으면 `enabled = false`로 남겨 사람이 보게 한다.
3. 놓친 작업(상한 있는 따라잡기, `scheduler-actor.worker.ts:261-345`, `timer-constants.ts`): 바로 돌리는 것은 최대 5개(`maxMissedJobsPerRestart`), 나머지는 5초 간격(`missedJobStaggerMs`)으로 편다. **에이전트 실행이 필요한 놓친 작업은 시작할 때 바로 돌리지 않고 2분 뒤로 미룬다**(`startupDeferredMissedAgentJobDelayMs`). 모델 호출을 시작 과정에서 빼려는 것이다.
4. `cron.skipMissedJobs: true`면 반복 작업은 놓친 회차를 버리고 다음 회차로 간다(기본 false = 따라잡음). 일회성은 이 설정과 무관하게 따라잡는다 (`managing-jobs.md:261`).
5. 성공했거나 건너뛴 실행 **도중에** 지나간 회차는 다시 켠 뒤 재생하지 않는다 (`how-it-works.md:31`).

**실행** [관찰]

- 시간 초과(벽시계): 직접 준 `timeoutSeconds`가 있으면 그 값(0 이하 = 무제한). 없으면 에이전트 턴 60분, 명령 10분, 스크립트 5분. 준비 · 실행 · 전달이 모두 이 안에 든다. 실행 자리를 기다리는 시간은 세지 않는다 (`O:src/cron/service/timeout-policy.ts:10-31`, `how-it-works.md:26`).
- 실행 전 자리가 없어 미뤄지면 그 회차를 쥔 채 30초 뒤 다시, 두 배씩 늘려 5분까지. 다섯 번 미뤄지면 화면에 경고 (`how-it-works.md:30`).
- 한 번만 도는 작업은 성공하면(전달 확인 · 전달 불필요 · 일부러 조용히) 지워지고, 전달이 실패했거나 알 수 없으면 꺼진 채 남는다. 내용을 다시 보내지 않는다 (`how-it-works.md:25`).

#### 실패했을 때 [관찰]

| 경우 | 동작 | 근거 |
|---|---|---|
| 반복 작업이 연달아 오류 | 다음 실행을 30초 → 60초 → 5분 → 15분 → 60분 뒤로 민다(연속 오류 수로 고르고 마지막 값에 머문다). 기준은 "끝난 시각 + 지연". 성공하면 처음으로 | `O:src/cron/service/jobs-scheduling.ts:92-98, 148-174` |
| 일회성의 일시 오류(한도 · 과부하 · 네트워크 · 시간 초과 · 서버 오류) | 같은 표의 앞 3칸(30초 · 60초 · 5분)으로 3번까지 다시. 영구 오류 · 중단은 다시 하지 않는다. 실행이 시작됐는지(`executionStarted`)도 판단에 쓴다 | `O:src/cron/service/timer-trigger.ts:26, 126-183` |
| 반복 작업(`cron` · `every`)이 10번 연달아 실패 | **자동으로 끈다**: `enabled = false`, `nextRunAtMs` 비움, `state.autoDisabled = {reason: "consecutive-failures", atMs, consecutiveErrors}` + 알림 | `O:src/cron/service/auto-disable.ts:14, 52-101` |
| 다음 실행 시각을 계산할 수 없음 | `reason: "schedule-errors"`로 끈다 | `jobs-scheduling.ts:110-127` |
| 실패 알림 | 연속 2번째 실패부터, 같은 작업은 1시간에 한 번 | `O:src/cron/service/failure-alerts.ts:45-46` |
| 실행 중 프로세스가 죽음 | 위 "다시 켠 뒤" 1 · 2 | |

보존: 격리 실행 세션은 24시간(`cron.sessionRetention`), 끝난 실행 기록은 7일(`lost`는 24시간), 작업 · 종류마다 최신 2000줄 (`managing-jobs.md:294`, `O:src/cron/session-reaper.ts:33`).

#### 시간대와 서머타임 [관찰]

- cron 식은 `croner`로 푼다. 시간대는 작업의 `tz` → 없으면 **호스트의 시간대**(`Intl.DateTimeFormat().resolvedOptions().timeZone`) (`O:src/cron/expression.ts:5-11`, `O:src/cron/schedule.ts:3, 16-41`).
- 사라지는 시각: croner가 없는 벽시계 시각을 지어내므로, 전환 순간을 이분 탐색으로 찾아 그 틈을 기준으로 다시 계산한다. 두 번 오는 시각: croner는 1시간짜리 겹침만 접으므로, 실제 오프셋 변화량을 써서 30분 · 2시간짜리 겹침에서도 **첫 번째에 한 번만** 돈다 (`schedule.ts:99-188`).
- `every`: `anchorMs`(없으면 지금)에서 `floor((지금 - anchor) / everyMs) + 1`걸음 뒤. 재시작해도 박자가 anchor에 붙어 있다. `everyMs < 1`은 무효 (`schedule.ts:218-236`).
- 시계가 뒤로 갔을 때: 전용 방어는 확인하지 못함. 앞으로 뛴 것은 위 tick 1단계가 잡는다.

REA로 본 것: 이 절의 파일 24개를 빈 폴더에 복사해 돌렸다(끝의 "REA를 쓴 범위"). 파싱 실패 0, import 관계 524개 · 내보내는 이름 111개를 얻었다. 숫자 · 조건 · 순서는 전부 직접 읽은 것이다.

---

## 2. 깨우기 (바깥 이벤트로 턴 시작)

### 2.1 Hermes: webhook 어댑터

`openclaw.md` 1장 판정 표는 Hermes에 "범용 웹훅 → 에이전트 라우팅이 없다"고 적었다. **틀렸다.** 범용 webhook 어댑터가 있다(끝의 "바로잡을 곳").

#### 동작 원리 [관찰] (`H:gateway/platforms/webhook.py`)

1. 따로 듣는 포트 기본 8644. 주소 기본값은 "지정 없음"(IPv4 · IPv6 모두) (`:63-64`).
2. 경로(route)는 `platforms.webhook.extra.routes`(설정)와 `webhook_subscriptions.json`(실행 중에 더하는 것)에 있다. 경로마다: `events`(받을 이벤트 이름), `secret`(필수), `signature`(서명 방식 고정), `prompt`(틀. `{a.b}`로 본문 값을 넣음), `skills`, `script`, `deliver` · `deliver_extra`, `deliver_only`, `cron_job`, `coalesce`, `mirror_to_session` (`:1-10, 66, 79`).
3. 요청 하나의 처리 순서 (`:693-771`):
   1. 경로 찾기.
   2. 본문 크기 검사(기본 1MiB. 읽기 전에 Content-Length로, 읽은 뒤 한 번 더) → 413.
   3. 비밀이 없거나 빈 값이면 403(막힌 쪽으로 실패). 서명 검사 실패는 401.
   4. 세워 둔(parked) 프로필이면 거절(전달 id를 기록하기 **전**이라 보낸 쪽이 다시 보낼 수 있다).
   5. 속도 제한: 경로마다 60초에 30번(`rate_limit`). 넘으면 429. **인증 뒤에** 센다.
   6. 본문 풀기(JSON 또는 form). 못 풀면 400.
   7. 이벤트 이름 = `X-GitHub-Event` → `X-GitLab-Event` → 본문 `event_type` → `type` → `"unknown"`. `events`에 없으면 `200 {status:"ignored"}`.
   8. 경로 필터가 안 맞으면 `{status:"ignored", reason:"filter"}`.
   9. `script`가 있으면 돌려서 버릴지 정하고 본문을 바꿀 수 있다. 버리면 `{status:"ignored", reason:"script"}`.
   10. 프롬프트를 만든다(틀 + 스킬).
   11. 중복 막기: 키는 `(프로필, 경로, 전달 id)`. `deliver_only` · `cron_job` · `coalesce` 경로는 메모리 안의 표(1시간 보존)로만 막는다. 같은 것이 또 오면 `200 {status:"duplicate"}`. 보통 경로는 내구성 있는 접수 장부가 막는다.
   12. 갈래: `cron_job`이면 있는 cron 작업을 지금 발사(만든 프롬프트는 그 한 번의 맥락) → 202. `deliver_only`면 모델 없이 프롬프트를 그대로 메시지로 보냄 → 200, 실패 502. `coalesce`면 묶음에 넣고 `202 {status:"coalesced"}`. 그 밖은 에이전트 실행을 접수 → 202.
4. 묶어 보내기(coalesce): 본문에서 뽑은 키로 묶고, 조용한 시간 30초가 지나면 **그 묶음의 마지막 이벤트 하나만** 보낸다. 첫 이벤트에서 300초가 지나면 조용하지 않아도 보낸다(계속 오는 흐름이 발사를 굶기지 않게) (`H:gateway/platforms/webhook_coalesce.py:1-24`).
5. 서명: 방식은 `generic`(v2 · standard) / `github` / `gitlab` / `linear`. generic v2는 시각을 서명에 묶고 300초보다 낡으면 거절한다. 본문만 서명하는 v1은 "서명이 영영 만료되지 않는다"는 이유로 항상 거절한다. 비교는 상수 시간(`hmac.compare_digest`), 해시는 HMAC-SHA256 (`:70-78, 130-160`).
6. 세션: `(프로필, 경로, 전달 id)`가 세션 키에 들어가 동시에 온 webhook은 서로 다른 실행이 된다 (`:823-826`).
7. 응답의 조용히 끝내기 규칙은 cron과 같은 판정 함수를 쓴다 (`H:cron/scheduler.py:565`).

#### 실패했을 때 [관찰]

- 비밀 `INSECURE_NO_AUTH`(검사 건너뛰기)는 루프백 주소에서만 된다. 그렇지 않으면 시작할 때 예외로 멈춘다(실행 중에 더한 경로는 건너뜀) (`:274-283, 436-446`).
- 묶음의 접수가 실패하면 로그를 남기고 그 묶음의 전달 id들을 중복 표에서 되돌린다(보낸 쪽이 다시 보낼 수 있게) (`:785-795`).

### 2.2 OpenClaw: wake · 시스템 이벤트 · heartbeat

트리거의 종류 여섯 가지, HTTP hooks, 조건 스크립트, 스트림, 이벤트 봉투는 `openclaw.md` 2.1에 있다. 여기서는 깨우는 흐름 · 합치기 · 조용히 끝내기만 적는다.

#### 동작 원리 [관찰]

1. **heartbeat는 지금 없다**(`openclaw.md` 2.1의 말이 맞다). `HEARTBEAT_OK` 토큰과 heartbeat 이름이 소스에 남아 있지만, `O:src/infra/heartbeat-compat.ts:1`은 "Deprecated v4 controls"라고 적고 예전 heartbeat를 보통 예약 작업으로 바꾼 것들을 켜고 끄는 일만 한다. `mode: "next-heartbeat"`라는 이름도 "다음 예약 실행 때"라는 뜻으로 남았다.
2. `wake(mode, text, sessionKey?, agentId?)` (`O:src/cron/service/wake.ts:99-225`):
   1. 글이 비면 실패. 하위 에이전트 세션은 깨울 수 없다(`unwakeable-session-key`).
   2. `mode = "now"`이거나 세션을 지정했으면 그 세션에 시스템 이벤트를 넣고 바로 턴을 시작한다.
   3. `next-heartbeat`이고 세션이 없으면 다음 예약 세션 작업이 가져가도록 대기열에만 넣는다. 받을 예약 작업이 없으면 "mode now를 고르거나 일정이 있는 자동화를 만들라"는 오류.
   4. 요청을 받을 때 잡아 둔 대상(에이전트 · 세션)이 넣는 순간에도 같은 곳을 가리키는지 다시 확인한다. 달라졌으면 거절.
3. **시스템 이벤트 대기열** (`O:src/infra/system-events.ts:50-51, 255-329`): 세션마다 메모리 안의 줄.
   - 항목: `{id(uuid), text, ts, contextKey, deliveryContext}`.
   - 중복 막기: `contextKey`가 없으면 **맨 뒤 항목과 글이 같을 때**, 있으면 **같은 키 · 같은 전달 경로 · 같은 글이 줄 어디에든 있을 때** 넣지 않는다.
   - `replace: true`(키 필수): 같은 키의 대기 항목을 지우고 새것으로 바꾼다. 같은 글 하나뿐이면 아무것도 안 한다.
   - 상한: "다음 턴이 가져갈 알림" 20개 + "스스로 턴을 시작하는 깨우기" 20개를 **따로** 센다(한 세션에 최대 40개). 가득 차면 넣지 않고 경고(호출한 쪽이 원하면 예외). HTTP 쪽은 이것을 503으로 돌려준다(`openclaw.md` 2.1).
4. **자동화가 넣는 알림의 합치기** (`system-events.ts:412-462`): 아직 시작하지 않은 대기 항목 가운데 같은 작업 id · 같은 "이 실행 뒤" 표시 · 같은 합치기 키 · 같은 글이 있으면 새로 넣지 않고 `"coalesced"`를 돌려준다. 한 작업이 한 턴에 쌓을 수 있는 알림 글은 합쳐서 2000자, 넘으면 오류.
5. **조용히 끝내기**: 응답 전체가 공백을 뺀 뒤 정확히 `NO_REPLY`(대소문자 무시)일 때만 조용히 끝낸다. 문장 속에 들어 있으면 보통 응답이다 (`O:src/auto-reply/tokens.ts:2-20`). Hermes는 첫 줄 · 마지막 줄도 인정하는 점이 다르다.
6. 깨워진 턴이 받는 입력의 정확한 머리말 문구(대기열의 글이 프롬프트에 들어가는 모양)는 **확인하지 못함**(`drainSystemEvents`를 부르는 쪽을 읽지 못했다).

#### 가격인하에 옮길 때 (1.1 + 1.2 + 2.1 + 2.2)

| 무엇 (OpenClaw) | 판정 | 까닭 · 닿는 곳 |
|---|---|---|
| 영수증 번호 = `(작업, 예정 시각)` + "실행 중은 하나"라는 유일 인덱스 | **그대로** | SQLite 한 곳을 쓰는 우리에게 Hermes의 파일 표시보다 맞다. T11 · A5. `conformance.md` C12 |
| 다시 켠 뒤: 실행 중이던 것은 "중단됨"으로 닫고 **다시 돌리지 않음**, 전달을 시작했는지 따로 기록 | 그대로 | 주문이 걸린 턴을 다시 돌리면 안 된다(F9 "되살리면 멈춘 상태", F1) |
| 놓친 것은 5개까지 · 5초 간격 · 에이전트 실행은 2분 뒤 | 줄여서 | 숫자는 설정으로. 장 마감 뒤 켜졌을 때 장중 트리거를 따라잡을지는 원본에 없다(아래 "새로 정해야 하는 것") |
| 오류 지연 30초 · 60초 · 5분 · 15분 · 60분, 10번이면 자동으로 끔, 알림은 2번째부터 1시간에 한 번 | 그대로 | Hermes는 끄지 않는다. 24시간 도는 팀에서는 끄고 알리는 쪽이 비용을 막는다 |
| 60초 상한 타이머 + 벽시계 · 단조 시계 비교 | 그대로 | 절전에서 깬 노트북 · 시계 맞춤 |
| 시스템 이벤트 대기열(세션마다 20 + 20, 키 중복 · replace · 합치기) | 그대로 | 시세 조건이 연달아 맞을 때 같은 종목 · 같은 조건을 하나로(F1 "묶어 보내기") |
| `NO_REPLY` 전체 일치만 | 그대로 | Hermes의 첫 줄 · 마지막 줄 규칙보다 실수로 삼킬 일이 적다 |

원본에 없어서 우리가 새로 정해야 하는 것:

1. **장 시간과 따라잡기.** 원본의 유예는 주기만 본다. "장 시작 브리핑을 10시에 켜졌을 때 돌릴지", "장중 시세 조건을 장 마감 뒤에 따라잡을지"는 트리거마다 정해야 한다.
2. **시세 조건의 다시 걸기(재무장).** OpenClaw 조건 스크립트의 `state`(16KB)가 자리만 준다. "한 번 알린 가격은 얼마나 벗어나야 다시 알리나"는 우리가 정한다.
3. **휴장일 달력.** 어느 원본에도 없다.

#### 가격인하에 옮길 때 — Hermes 쪽 (1.1 + 2.1)

| 무엇 | 판정 | 까닭 · 닿는 곳 |
|---|---|---|
| tick 순서(잠금 → 전체 정지 확인 → 파일 다시 읽기 → 기한 판정 → **먼저 옮기고** 발사) | 그대로 | 장 시작 · 마감 예약(F1, T11, A5). 전체 정지(스펙 F1 2c · F4 7b1)가 tick 3단계에 이미 끼어 있는 모양도 그대로 |
| 작업 저장 | 줄여서 | 우리는 SQLite 한 곳이다. 파일 합치기 · fsync 대신 트랜잭션 하나로 "기한 판정 + 표시"를 묶는다 |
| 유예 = 주기 절반(120초~2시간), 놓친 회차는 한 번만 따라잡고 장부에 "N번 놓침" | 그대로 | 다시 켠 뒤의 처리(F1 · F9). 단 **장 시간 밖의 따라잡기**는 원본에 없다. 새로 정해야 한다 |
| 일회성의 `run_claim`(TTL 1800초 + 60초마다 갱신), 반복의 `pending_slot` | 줄여서 | 프로세스가 하나라 다른 프로세스와의 경쟁은 없다. "죽었다 켜졌을 때 한 번만 복구"만 남긴다 |
| 실패 처리(자동으로 끄지 않음, 3번째부터 알림에 덧붙임, 모델에 못 닿았을 때만 5 · 15 · 30분) | 그대로 | 주문이 걸린 실행은 다시 돌려도 부작용이 없을 때만 자동 재시도한다는 원본의 까닭이 우리에게 더 중요하다 |
| 조용히 끝내기(전체 · 첫 줄 · 마지막 줄만) | 그대로 | 아무 일도 없는 감시 턴 |
| webhook(서명 · 300초 · 속도 제한 · 중복 표 · 묶어 보내기 30초/300초) | 줄여서 | 뉴스 · 공시 수집기는 우리 프로세스 안에 있다. 바깥 HTTP 입구는 1차에 두지 않고, **묶어 보내기와 중복 키 규칙만** 내부 트리거에 쓴다(같은 종목의 공시가 연달아 올 때) |
| 서머타임 | 안 가져옴 | 한국 시장만 다루고 한국은 서머타임이 없다. 미국 장을 넣을 때 fold 규칙을 가져온다 |

---

## 3. 승인

### 3.1 Hermes: 위험한 명령 승인

`hermes-agent.md` "못 본 부분" 4(위험 판정 규칙 · `approvals.timeout`)를 메운다. `/v1/runs/{id}/approval`과 `approval.request` 이벤트의 자리는 같은 문서 5.2에 있다.

#### 동작 원리 [관찰]

**판정 순서** (명령 하나, `H:tools/approval.py:1207-1341`)

1. 컨테이너 안(호스트 접근 없음)이면 사용자의 거부 규칙만 보고 통과.
2. **바닥(floor)** — 어떤 설정으로도 풀리지 않는다. yolo · `approvals.mode: off` · cron 승인 모드보다 **먼저** 본다 (`H:tools/approval_floors.py:1-8`):
   - 절대 금지 패턴 12개(`HARDLINE_PATTERNS`: 시스템 폴더 통째 삭제, 디스크 장치에 쓰기, 포크 폭탄 등)
   - 지금 돌고 있는 런타임(파이썬 · venv)을 지우는 명령
   - 비밀번호를 `sudo -S`에 흘려 넣는 것
   - 사용자가 적은 `approvals.deny` 규칙("yolo여도 안 됨")
3. yolo이거나 `approvals.mode == "off"`면 통과.
4. `command_allowlist`(영구 허용 목록)에 맞으면 통과. 결과에 "어느 규칙 덕에 통과했는지"를 붙인다.
5. 사람이 없는 맥락(CLI도 게이트웨이도 아님)이면 **기다리지 않고 즉시 정한다**: 맥락별 설정(`approvals.cron_mode`, `single_query_mode`, `unattended_mode`. 값은 `approve`/`deny`, 기본 `deny`)이 deny이고 위험 패턴에 걸리면 막고, 아니면 통과.
6. 위험 패턴 약 114개(`DANGEROUS_PATTERNS`)에 대어 본다. 걸린 것마다 `(pattern_key, 설명)`이 나온다. 대기 전에 명령을 정규화한다(홈 경로 접기, 보이지 않는 유니코드 제어 문자 검사, 셸을 거쳐 실행하는 `bash -c` · `eval` · 파이프 끝의 셸 풀기) (`H:tools/approval_detection.py:1-240`).
7. 걸린 키가 모두 이미 승인돼 있으면(세션 또는 영구) 통과. 결과에 `pre_approved[{description, scope}]`를 붙여 "건너뛴 것"이 아니라 "전에 승인받은 것"임을 남긴다 (`H:tools/approval_provenance.py:11-27`).
8. 남은 키를 **한 번의 질문**으로 묶어 사람에게 묻는다. `approvals.mode == "smart"`면 그 전에 보조 모델이 먼저 판정한다(승인이면 묻지 않음, 거부면 사람에게 넘기되 선택지를 `once / deny`로 줄인다).

**묻고 기다리기** (게이트웨이 · API 실행, `H:tools/approval_gateway_wait.py:25-297`)

1. 대기 항목을 만든다: `request_id = uuid4().hex`, `threading.Event`, `timeout_s`(만드는 순간의 `approvals.timeout`을 붙잡아 둔다. 기다리는 중 설정을 바꿔도 이 요청은 안 바뀐다).
2. 세션별 대기 줄(`_gateway_queues[session_key]`)에 넣는다. **같은 명령 글 + 같은 패턴 키 묶음**의 요청이 이미 줄에 있으면 새로 묻지 않고 그 뒤에 붙는다(병렬 도구 호출이 같은 질문을 N번 띄우지 않게). 따라붙은 쪽은 앞선 요청의 `session / always / deny / 시간 초과`를 그대로 받고, `once`는 앞선 것 하나만 덮으므로 나머지는 새 질문 하나로 다시 묶인다.
3. 알림 콜백으로 질문을 내보낸다. 내용: `{command, description, pattern_key, pattern_keys, allow_permanent, allow_session, smart_denied?, request_id}`. 명령 · 설명은 **가려서**(redact) 보낸다. 실제 실행은 원래 명령으로 한다 (`H:tools/approval.py:989-1005`).
4. 에이전트 스레드는 그 자리에서 **막힌다**. 1초씩 끊어 기다리며 약 10초마다 "승인을 기다리는 중" 활동 신호를 보내 무활동 감시에 끊기지 않게 한다. 턴이 중단되면(/stop 등) 기다림도 끝난다.
5. API 실행이면 실행 상태를 `waiting_for_approval`로 바꾸고 `approval.request` 이벤트를 흘린다. 이벤트에는 클라이언트가 보낼 수 있는 `choices`가 들어 있다 (`H:gateway/platforms/api_server.py:90-102`, `api_server_runs.py:1002-1013`).
6. 답: `POST /v1/runs/{run_id}/approval` 본문 `{choice, request_id?, all?}`. `choice`는 `once / session / always / deny`. `request_id`가 있으면 그 항목, `all`이면 전부, 없으면 **가장 오래된 것**(FIFO). 대기가 없으면 409 `approval_not_pending`, 선택지가 틀리면 400. 방 권한 토큰으로 온 답은 `once / deny`만 되고 `request_id`가 필수다 (`api_server_runs.py:1361-1413`).
7. 항목을 줄에서 빼는 것과 결과를 적는 것이 한 잠금 안이다. 기다리던 쪽이 마감 직후 같은 잠금 안에서 결과를 읽으므로, "답을 받았다고 응답했는데 시간 초과로 처리되는" 일이 없다 (`H:tools/approval.py:150-184`).

**결과** (`approval.py:932-1066`)

| 답 | 일어나는 일 |
|---|---|
| `once` | 이번 한 번 통과. 저장하지 않는다 |
| `session` | 패턴 키들을 이 세션의 허용 집합(메모리)에 넣는다 |
| `always` | 세션 허용 + 설정 파일 `command_allowlist`에 패턴 키를 더한다. 쓸 때 "지금 디스크에 있는 것 + 이 프로세스가 새로 승인한 것"만 합친다(사람이 손으로 지운 항목을 되살리지 않는다) (`:484-497, 553-583`) |
| `deny` | 막힌다(BLOCKED). `/deny <이유>`의 이유 글을 모델에 그대로 돌려준다 |
| 시간 초과 | 막힌다. 문구에 "Silence is not consent."를 붙인다. 기본 **300초**(`approvals.timeout`) (`H:tools/approval_context.py:318-336`) |
| 질문이 취소됨(턴 중단) | 막힌다. 사용자가 거절한 것으로 적지 않는다 |
| 알림을 못 보냄 | 막힌다(`notify_failed`) |

- 보조 모델의 거부가 한 세션에서 3번(`approvals.denial_breaker_threshold`) 이어지면 거부 문구에 "그만 시도하라"는 글을 덧붙인다(사람의 거절은 세지 않는다) (`approval.py:71-112`).

#### 상태와 저장되는 것 [관찰]

| 무엇 | 어디 | 다시 켜면 |
|---|---|---|
| 대기 중인 승인(줄 · Event) | 프로세스 메모리 | **사라진다.** 기다리던 턴도 같이 죽는다 |
| 세션 허용 | 메모리 `_session_approved[session_key]` | 사라진다 |
| 영구 허용 | `config.yaml`의 `command_allowlist`(패턴 키 목록) | 남는다 |
| 설정 | `approvals.mode`(`manual` 기본 / `smart` / `off`), `approvals.timeout`(300), `approvals.cron_mode` · `single_query_mode` · `unattended_mode`(기본 `deny`), `approvals.deny`, `approvals.denial_breaker_threshold`(3) | |

**승인한 내용과 실행할 내용이 같은지 확인하는 장치: 없다** [관찰]. 승인은 "이 명령 문자열"이 아니라 "이 패턴 키"에 붙는다. 다만 기다리는 동안 에이전트 스레드가 그 도구 호출 안에서 막혀 있으므로, 승인 뒤 실행되는 것은 물었던 그 명령이다(중간에 바뀔 틈이 없다). 화면에 보인 글은 가린 사본이고 실행은 원문이라는 차이는 있다. `session` · `always`로 승인하면 같은 패턴의 **다른** 명령도 묻지 않고 통과한다.

### 3.2 OpenClaw: 승인 요청의 수명주기

정책 계층 · exec 모드 · `before_tool_call` · scope 표시 · standing grant의 뜻은 `openclaw.md` 2.2에 있다. 여기서는 그 문서가 못 본 것(5.3의 5: 저장 모양과 다시 켠 뒤)을 적는다. `openclaw.md` 2.2는 저장을 허용 목록 문서 한 줄로만 적었는데, **대기 중인 승인도 SQLite에 있다.**

#### 상태와 저장되는 것 [관찰] (`O:src/state/openclaw-state-schema.sql:370-535`)

표 `operator_approvals` (한 줄 = 승인 요청 하나):

| 칸 | 뜻 |
|---|---|
| `approval_id` (기본 키) | 요청 번호. `randomUUID()`로 만든다(부르는 쪽이 번호를 줄 수도 있다) (`O:src/gateway/exec-approval-registration.ts:53`) |
| `resolution_ref` (유일, 43자) | `sha256(종류 + "\0" + 번호)`의 base64url. 번호 전체를 실을 수 없는 전송로(메신저 버튼 등)가 대신 쓰는 찾기 키 (`O:src/infra/approval-resolution-ref.ts:1-16`) |
| `kind` | `exec` / `plugin` / `system-agent` |
| `status` | `pending` / `allowed` / `denied` / `expired` / `cancelled` |
| `presentation_json` | 사용자에게 보일 내용 |
| `requested_by_device_id` · `requested_by_client_id` · `requested_by_device_token_auth` | 누가 요청했나 |
| `reviewer_device_ids_json` | 답할 수 있는 기기 목록. 비어 있으면 승인 권한(`operator.approvals`)을 가진 어느 화면이든 답할 수 있다(먼저 답한 쪽이 이긴다) (`O:src/gateway/operator-approval-reviewer-binding.ts`) |
| `source_agent_id` · `source_session_key` · `source_session_id` · `source_run_id` · `source_tool_call_id` · `source_tool_name` | 어느 턴의 어느 도구 호출인가 |
| `audience_session_keys_json` | 이 승인을 볼 수 있는 대화 |
| `runtime_epoch` | 이 요청을 만든 게이트웨이 실행의 표시(켤 때마다 바뀜) |
| `created_at_ms` · `expires_at_ms` · `updated_at_ms` | |
| `decision` | `allow-once` / `allow-always` / `deny` |
| `terminal_reason` | `user` / `timeout` / `malformed-verdict` / `no-route` / `run-aborted` / `gateway-restart` / `storage-corrupt` |
| `resolved_at_ms` · `resolver_kind`(`device`/`channel`/`runtime`/`system`) · `resolver_id` | 누가 언제 답했나 |
| `consumed_at_ms` · `consumed_by` | 승인을 실행에 **쓴** 시각과 쓴 쪽 |

표의 CHECK가 상태 기계를 강제한다:

- `pending`: 결정 · 사유 · 답한 쪽 · 소비가 모두 비어 있어야 한다.
- `allowed`: 결정이 `allow-once`/`allow-always`, 사유 `user`.
- `denied`: 결정 `deny`, 사유 `user`/`malformed-verdict`/`no-route`/`storage-corrupt`. 소비 칸은 비어 있어야 한다.
- `expired`: 결정 `deny`, 사유 `timeout`.
- `cancelled`: 결정 `deny`, 사유 `run-aborted`/`gateway-restart`.
- **소비는 `allowed` + `allow-once`에서만** 적을 수 있다.

곧 "답 없음 · 모르는 답 · 보낼 곳 없음 · 중단 · 재시작"은 모두 `decision = deny`로 끝난다.

그 밖의 표: `operator_approval_execution_identities(approval_id, source_context_id, source_execution_id)`, `operator_approval_standing_grants(grant_id, minted_by_approval_id, agent_id, cron_job_id, job_config_revision, operation_binding, created_at_ms, expires_at_ms, revoked_at_ms, revoked_by, last_used_at_ms, use_count)`(자동화용 계속 허용. 작업 정의의 판과 동작 지문에 묶인다).

#### 동작 원리 [관찰]

1. 등록: 번호를 만들고 `pending` 줄을 넣는다. 만료 시각 = 지금 + 대기 시간(exec 30분, 일반 도구 120초 · 최대 600초. `openclaw.md` 2.2).
2. 답: `pending`이고 만료 전인 줄만 바꾼다. 이미 끝난 줄이면 `already-resolved`를 돌려준다(두 번째 답은 첫 답을 바꾸지 못한다). 답하려는 순간 이미 만료 시각이 지났으면 만료로 닫는다 (`O:src/gateway/operator-approval-store.transitions.worker.ts:60-96, 147-191`).
3. 만료: 기한이 된 `pending` 줄을 `expired` + `deny` + `timeout`으로 닫는다 (`:213-236`).
4. **다시 켠 뒤**: 시작할 때 `status = 'pending'`이고 `runtime_epoch`가 지금 것과 **다른** 줄을 전부 `cancelled` + `deny` + `gateway-restart`로 닫는다(오래된 것부터) (`:245-276`). 다시 묻지 않는다. 기다리던 턴은 이미 죽었기 때문이다.
5. **한 번만 쓰기**: `allow-once`를 실행에 쓸 때 `consumed_at_ms` · `consumed_by`를 적는다. 이미 적혀 있으면 `already-consumed`. 답이 난 지 일정 시간(redemption window)이 지났으면 `redemption-expired`. 메모리 쪽은 답이 난 항목을 15초(`EXEC_APPROVAL_RESOLVED_ENTRY_GRACE_MS`)만 붙들고 있다가 버린다 (`:310-375`, `O:src/gateway/exec-approval-lifecycle.ts:35`, `exec-approval-consumption.ts`).
6. **승인한 내용과 실행할 내용이 같은지 확인하는 장치: 있다** (`O:src/infra/system-run-approval-binding.ts:61-160`). 승인을 요청할 때 `{argv, cwd, agentId, sessionKey, envHash}`를 묶어 둔다. `envHash`는 환경 변수 항목을 정렬한 JSON의 SHA-256이다. 실행 직전에 실제 값과 비교해 argv · cwd · 에이전트 · 세션이 하나라도 다르면 불일치, 환경 해시가 다르면 "approval id env binding mismatch"로 거절한다. 스크립트 파일을 실행하는 명령이면 그 파일 내용의 SHA-256도 묶는다(승인 뒤 파일을 바꿔치기하는 것을 막음) (`:188-230`).
7. 알림: 승인이 생기면 붙어 있는 화면에 방송하고, **웹 푸시**(`O:src/gateway/approval-web-push.ts`. 조용한 시간 · 종류별 켜기 · 에이전트별 허용 설정이 있다)와 iOS 푸시(`exec-approval-ios-push.ts`)로도 보낸다. 메신저로 넘기는 길(`exec-approval-forwarder.ts`)은 `resolution_ref`를 쓴다.

REA로 본 것: 없음(이 절은 SQL 스키마와 소스를 직접 읽었다).

### 3.3 DeskRPG · 플러그인: 화면에 띄우고 답 돌려보내기

`deskrpg.md` 4-7이 흐름 · 카드 키 · 선택지 · 묶기 · 요약을 이미 적었다. 빈 곳만 더한다 [관찰].

- 소켓 이벤트 넷 (`D:src/lib/tool-approval-types.ts:1-70`): 서버 → 승인자 `tool-approval:request`(같은 키로 다시 보내면 화면이 그 카드를 제자리에서 고친다), 서버 → 승인자 `tool-approval:resolved {key, status}`, 서버 → 회의 · 방 `tool-approval:pending {key, npcId, approverName, roomId?}`(다른 참가자에게 "누구의 승인을 기다리는 중"만 알리고 명령은 안 보인다) 또는 `{key, cleared: true}`, 화면 → 서버 `tool-approval:decide {key, choice}`.
- 카드의 칸: `key, runId, requestId, npcId, channelId, context(dm/meeting/room), roomId?, kind(mcp/command), command(Hermes가 가린 글), description, choices, expiresAt, groupKey, repeat{count, lastStatus}, summary{state: pending/ready/unavailable, text?}`. 화면은 1초마다 남은 시간을 다시 그린다(`D:src/components/approvals/ToolApprovalCard.tsx:25`, REA가 찾은 타이머).
- 답의 검증은 서버에서 한다 (`D:src/server/tool-approvals.ts:225-267`): 선택지가 셋 중 하나인가 → 카드가 있는가(없으면 `closed`) → **답한 사람이 승인자와 같은가**(아니면 `not_approver`) → 이미 답하는 중인가(`deciding` 표시로 두 번 누르기를 막음) → 이 카드가 그 선택지를 주었는가.
- 한 카드 뒤에 Hermes 요청이 여러 개 있을 수 있다(같은 요청이 다른 실행에서 또 옴). 답은 **요청마다 따로** `POST /v1/runs/{id}/approval`로 보낸다. 하나라도 받아들여지면 성공, Hermes가 404 · 409를 주면(이미 시간 초과됐거나 실행이 끝남) 카드를 `expired`로 닫는다.
- 만료는 양쪽이 따로 잰다. DeskRPG는 `expiresAt`에 자기 타이머로 카드를 닫고, 실제 거부는 Hermes가 자기 시간 초과로 한다. 시간은 플러그인으로 읽은 `approvals.timeout`(NPC별 10분 캐시), 못 읽으면 300초 (`tool-approvals.ts:33-34, 376-387`).
- 다시 붙었을 때: 소켓이 인증을 마치면 그 사용자의 대기 카드를 다시 보낸다 (`D:src/server/socket-handlers.ts:1315`).
- 서버가 재시작하면 대기 카드는 사라지고 Hermes가 시간 초과로 거부한다(파일 머리 주석, `tool-approvals.ts:1-7`).
- 승인 카드의 내용과 실행 내용을 맞춰 보는 장치는 **없다**. 카드는 Hermes가 가려서 준 글을 보일 뿐이다.

REA로 본 것: 이전에 승인 관련 파일 22개를 `rea-triggers/dk-approval/`에 복사해 돌린 결과(`rea-triggers/out/dk-approval/`)에서 타이머 위치(`tool-approvals.ts:107`, `ToolApprovalCard.tsx:25`, `use-tool-approvals.ts:174`)를 얻었다. 나머지는 직접 읽었다.

### 3.4 가격인하에 옮길 때

| 무엇 | 판정 | 까닭 · 닿는 곳 |
|---|---|---|
| OpenClaw `operator_approvals` 표(상태 5개 + CHECK로 강제하는 상태 기계 + 사유 7종) | **그대로** | F2 · FN2 · T14 · A7. "답 없음 · 모르는 답 · 재시작은 전부 거절"이 표 제약으로 박혀 있어 코드 실수로 못 깬다. 우리는 `terminal_reason`에 `team-decision`(답이 없어 팀이 판단) 하나를 더한다 |
| 번호 = UUID, 짧은 찾기 키 = 해시 43자 | 줄여서 | 승인 번호는 그대로. 메신저가 없으니 `resolution_ref`는 웹 푸시 알림의 딥링크에만 쓴다 |
| 내용 지문: `{argv, cwd, agent, session, envHash}` + 실행 직전 비교 + 불일치면 거절 | **그대로(모양을 주문으로 바꿔)** | A7의 "내용 해시". 우리 지문 = `{계좌 별칭, 종목, 방향, 수량, 가격 또는 가격 조건, 주문 종류, 신호 id}`의 SHA-256. FN2의 "승인 번호와 내용 지문" |
| `allow-once`는 한 번만 소비(`consumed_at_ms`) + 답 뒤 쓸 수 있는 시간 창 | 그대로 | 같은 승인으로 주문이 두 번 나가지 않게 |
| 다시 켠 뒤: 다른 `runtime_epoch`의 대기를 전부 `gateway-restart`로 닫음 | **바꿔서** | 우리 스펙(FN2 "다시 켠 뒤의 처리")은 주문 승인이 신호의 유효 시간 안이면 살아 있어야 한다. 원본은 기다리던 턴이 메모리에 있어서 닫을 수밖에 없다. 우리는 승인 대기를 턴 밖(DB)에 두고, 다시 켠 뒤 만료 전이면 **다시 묻고** 만료됐으면 닫는다. **원본에 없는 동작이라 새로 정해야 한다** |
| Hermes의 "기다리는 동안 스레드가 막힘" | 안 가져옴 | 승인 대기가 30분~몇 시간일 수 있다. 턴을 끝내고 승인 결과가 새 트리거로 팀을 깨우는 쪽으로 |
| Hermes 바닥(floor): 어떤 모드로도 안 풀리는 금지 | 그대로 | 주문 가드의 절대 한도(계좌 전체 한도 · 금지 종목)는 "자동 승인" 설정보다 먼저 본다 |
| 같은 요청 합치기(Hermes: 같은 명령 + 같은 키, DeskRPG: 1시간 안 같은 대화 · 같은 명령은 한 카드 + 반복 횟수) | 그대로 | 같은 신호로 승인 카드가 여러 장 뜨지 않게 |
| 답할 수 있는 사람의 검사는 서버에서(DeskRPG `not_approver`, OpenClaw `reviewer_device_ids`) | 줄여서 | 사용자 한 명이라 "접속 키로 들어온 세션인가"만 본다. 기기별 제한은 안 둔다 |
| `always` 선택지 | 안 가져옴 | DeskRPG도 주지 않는다. 주문에 영구 허용은 없다(한도 템플릿이 그 역할) |
| 가려서 보이기(redact) | 그대로 | 계좌 번호는 별칭으로만 |
| 웹 푸시로 승인 알리기 + 조용한 시간 | 그대로 | `conformance.md`는 "웹 푸시는 원본에 없다"고 적었다. **OpenClaw에 있다**(끝의 "바로잡을 곳"). T22 |

원본에 없어서 우리가 새로 정해야 하는 것:

1. **답이 없을 때 "팀이 위험을 키우지 않는 쪽으로 판단"**(FN2). 세 원본 모두 답 없음 = 거절 하나뿐이다. 신호마다 다른 대기 시간, 시간이 지난 뒤 팀이 고를 수 있는 동작의 범위(아무것도 안 함 / 줄여서 한도 안으로 / 손절은 실행)는 우리가 정한다.
2. **규칙 변경 · 전체 정지 뒤 다시 시작의 승인.** 원본의 승인 대상은 도구 호출뿐이다. 설정 변경을 승인 대상으로 삼는 길은 없다.
3. **승인 뒤 시세가 달라졌을 때.** 원본은 "같은 내용인가"만 본다. "승인한 가격에서 얼마나 벗어나면 다시 묻나"는 없다.

---

## 4. 장기 기억 검색

### 4.1 OpenClaw: 키워드 + 벡터 하이브리드

파일 4종, 표 이름, 검색 순서의 큰 그림, 자동 회상, 도구 입력 모양, 압축 전 기억 저장은 `openclaw.md` 2.3에 있다. 여기서는 그 문서가 못 본 것(5.3의 6: 가중치 기본값)과 식 · 다시 색인 조건 · 출력 모양을 적는다.

#### 동작 원리 [관찰]

**조각 나누기** (`O:packages/memory-host-sdk/src/host/markdown-chunks.ts:74-200`, `O:src/agents/memory-search.ts:260`)

1. 기본 `chunking = {tokens: 400, overlap: 80}`.
2. 토큰은 세지 않고 글자 수로 어림한다: 한 조각의 글자 상한 = `max(32, tokens × 토큰당 글자 수 어림값)`, 겹침 = `overlap × 어림값`. 어림값 상수는 한 군데(`CHARS_PER_TOKEN_ESTIMATE`)에 있는데 정의 줄은 **확인하지 못함**(같은 확장의 다른 곳은 4를 쓴다, `O:extensions/memory-core/src/short-term-promotion-utils.ts:90`).
3. 줄 단위로 쌓다가 상한을 넘으면 조각을 닫고, 닫은 조각의 끝부분을 겹침만큼 다음 조각 앞에 다시 넣는다. 한 줄이 상한보다 길면 그 줄을 글자 단위로 자른다(한자처럼 한 글자가 여러 토큰인 글자는 어림 단위로 센다).
4. 조각마다 `start_line` · `end_line`(1부터) · `hash`를 갖는다. 조각 경계 규칙의 판 번호를 색인 메타에 적어, 규칙이 바뀌면 안 바뀐 파일도 다시 나눈다 (`markdown-chunks.ts:21`).

**저장** (`O:src/state/openclaw-agent-schema.sql:689-766`, 에이전트별 DB)

| 표 | 칸 |
|---|---|
| `memory_index_meta` | `key, value` (색인의 정체: 공급자 · 모델 · 조각 설정 · 벡터 차원 · 토크나이저 · 범위 해시) |
| `memory_index_sources` | `id, path, source(기본 'memory'), hash, mtime, size`, `(path, source)` 유일 |
| `memory_index_chunks` | `chunk_rowid, id(유일), path, source, start_line, end_line, hash, model, text, embedding BLOB, updated_at` |
| `memory_index_chunks_vec` | sqlite-vec 가상 표 `(id, embedding)`. 임베딩은 본 표의 BLOB에도 있고 여기에도 있다 (`O:extensions/memory-core/src/memory/manager-db-kernel.ts:35-53`) |
| `memory_embedding_cache` | 기본 키 `(provider, model, provider_key, hash)`, `embedding BLOB, dims, updated_at`. 상한 50,000줄(LRU). 살아 있는 조각 수보다 작으면 다음 동기화가 돈을 내고 다시 임베딩하게 된다는 주석이 있다 (`memory-search.ts:88-92`) |
| `memory_index_state` | `id = 1, revision` (색인 판 번호) |
| FTS | 토크나이저 `unicode61`(기본) 또는 `trigram` 가운데 설정으로 고른다 (`O:packages/memory-host-sdk/src/host/memory-schema.ts:441`) |

**언제 색인하나** (`memory-search.ts:293-305`): 세션 시작 때(`onSessionStart`), 검색 직전(`onSearch`), 파일이 바뀔 때(`watch`, 1.5초 묶음 `watchDebounceMs: 1500`). 주기 색인은 기본 꺼짐(`intervalMinutes: 0`). 대화 기록 말뭉치(`sessions`)는 10만 바이트 또는 메시지 50개가 늘 때, 그리고 압축 직후 강제로.

**검색 식** (`O:extensions/memory-core/src/memory/hybrid.ts:44-262`, `keyword-query.ts:110-185`, `temporal-decay.ts:21-28`, `mmr.ts:8-60`)

기본값 (`memory-search.ts:85-86, 163-169`): `maxResults = 6`, `minScore = 0.35`, `vectorWeight = 0.7`, `textWeight = 0.3`, `candidateMultiplier = 4`, MMR 켜짐 `lambda = 0.7`, 최신성 감쇠 켜짐 `halfLifeDays = 30`. 하이브리드는 항상 켜져 있다(가중치를 설정에서 읽지 않고 코드에 박았다).

1. 후보: 벡터 검색과 키워드 검색을 각각 `maxResults × 4`개 뽑는다.
2. 키워드 질의 만들기: 질의를 토큰으로 쪼개 **모든 단어(AND)** 로 찾는다. `matchAny`면 **아무 단어(OR)** 로 찾고 질의에서 뽑은 핵심어를 6개까지 더한다. trigram 토크나이저에서는 3글자(코드 포인트) 미만 단어는 MATCH가 못 찾으므로 따로 부분 문자열(LIKE) 검색으로 돌린다 — 한글 두 글자 단어("매수", "손절")가 여기에 해당한다.
3. 키워드 점수: SQLite의 `rank`(음수일수록 좋음)를 `relevance = -rank`, `점수 = relevance / (1 + relevance)`로 0~1에 넣는다. rank가 음수가 아니면 `1 / (1 + rank)`.
4. **키워드 점수 다시 재기**(기존 문서의 "최고 의미 점수를 넘지 않게"의 정확한 식): 포화를 되돌려 `r(s) = s / (1 - s)`를 구하고, 이 질의의 가장 센 키워드 적중 대비 몫 `share = r(s) / max r`을 낸 뒤 `min(s, share × 가장 높은 벡터 점수)`로 자른다. 점수가 1인 것(파일 이름 일치 등)은 가장 센 것으로 친다. 까닭은 주석에 있다: 고정 가중치는 "키워드 창 안에 들었는지"만 보상하게 되기 때문이다.
5. 합: `vectorWeight × 벡터 점수 + textWeight × 다시 잰 키워드 점수`.
6. 최신성 감쇠: `점수 × exp(-(ln 2 / halfLifeDays) × 나이(일))`. `MEMORY.md` · 주제 파일 같은 "늘 푸른" 지식은 감쇠하지 않는다. 날짜가 붙은 일일 메모와 세션만 감쇠한다. `minScore` 판정은 감쇠 전 점수로 한다(오래됐다고 탈락하지는 않고 순위만 밀린다).
7. 중요도 · 프로젝트 가까움을 곱한다.
8. MMR: 점수를 최소~최대로 0~1에 다시 펴고, `lambda × 관련도 - (1 - lambda) × 이미 고른 것과의 최대 비슷함`이 가장 큰 것을 차례로 고른다. 비슷함은 조각 글(snippet)을 토큰으로 쪼갠 집합끼리 잰다. `lambda = 1`이면 점수순 그대로.
9. 가중 점수가 전부 문턱 아래면 "전부 키워드"로 물러선 결과를 유지한다 (`hybrid.ts:292-293`).

**다시 색인하는 조건** (`O:extensions/memory-core/src/memory/manager-reindex-state.ts:48-263`, `manager-sync-ops.ts:173-260`)

- 전체 다시 색인: 메타의 `provider` 또는 `model`이 바뀜 / 벡터를 쓸 수 있는데 메타에 `vectorDims`가 없음 / 벡터 차원이 바뀜 / 색인 범위 해시(`scopeHash`: 어떤 경로 · 말뭉치를 색인하나)가 바뀜 / `chunkTokens` 또는 `chunkOverlap`이 바뀜 / FTS 토크나이저가 바뀜 / 임베딩 입력 형식의 판이 바뀜 / 사람이 강제(`force`).
- 파일별 증분: `memory_index_sources.hash`가 같으면 건너뛴다.
- 전체 다시 색인은 **옆에 새 색인을 만들어 한 번에 바꿔치기**한다(임시 스키마에 만든 뒤 벡터 표를 지우고 새로 만들어 복사). 도중에 실패해도 기존 색인이 남는다 (`manager-db-kernel.ts:35-53`).
- 다시 색인이 실패하면 지수로 늘어나는 간격 뒤에 다시 한다(`2^(시도-1)`에 비례, `manager-sync-base.ts:361-368`).
- **설정한 임베딩 공급자가 죽었을 때 벡터 색인을 키워드 전용 줄로 덮어쓰지 않는다**("An unavailable configured provider must not replace semantic vectors with FTS-only rows", `manager-sync-ops.ts:206-207`). 새 색인이거나 이미 키워드 전용이면 그대로 진행한다.

**검색 도구의 출력** (`O:extensions/memory-core/src/tools.ts:498-520`, `tools.citations.ts:26-43`)

- `memory_search` 결과: `{results: [...], provider, model, fallback, citations(모드), mode, partial?, action?, debug?}`.
- 결과 한 건: `path, startLine, endLine, score, snippet, source` + 인용을 켜면 `citation = "<path>#L<시작>-L<끝>"`(한 줄이면 `#L<줄>`)이고 snippet 끝에 `"\n\nSource: <citation>"`를 붙인다. snippet은 700자까지 (`O:extensions/memory-core/src/memory/manager-search-retrieval.ts:37`).
- `memory_get`: 기본 120줄, 12,000자까지 (`O:packages/memory-host-sdk/src/host/read-file-shared.ts:8-10`).
- 못 쓸 때: 오류 대신 "쓸 수 없음" 결과를 돌려준다. 까닭 글 + `warning` + `action`(무엇을 하면 되는지) + `code`. 한도 초과(quota · 429)와 SQLite 모듈 없음을 따로 알아본다 (`O:extensions/memory-core/src/tools.shared.ts:69-83`).
- 공급자: 설정이 없거나 `auto`면 `openai`, `"none"`이면 임베딩 없이 키워드만(플러그인 탐색도 하지 않는다) (`memory-search.ts:93-120`).

REA로 본 것: 없음(직접 읽었다. 이전 `openclaw.md` 조사 때 이 영역 18개 파일을 REA로 돌린 결과가 그 문서 머리말 표 3번에 있다).

### 4.2 Hermes: 세션 검색(FTS)

`session_search` 도구의 자리와 `messages_fts`의 존재는 `hermes-agent.md` 3.5 · 7.1에, 메모리 도구(두 파일 · 글자 상한)는 3.6에 있다. `conformance.md`가 못 본 "한글 검색(`hermes_state_search`)"만 적는다.

#### 동작 원리 [관찰]

**색인 둘** (`H:hermes_state_common.py:962-1082`)

- `messages_fts`: FTS5 기본 토크나이저(unicode61). 열 `content, tool_name, tool_calls`. 원본 표를 직접 읽지 않고 뷰 `messages_fts_src`를 거친다. 도구 결과(role = tool)는 **앞 8,192자만** 색인한다(`FTS_TOOL_CONTENT_PREFIX_CHARS`, `:329`). 뷰 · 트리거 · 채우기가 같은 식 하나를 쓰게 해 "색인에 넣은 것"과 "검사 때 다시 읽는 것"이 어긋나지 않게 했다(어긋나면 FTS5가 깨졌다고 본다).
- `messages_fts_trigram`: trigram 토크나이저. 한중일 부분 문자열 검색용. **가장 비싼 색인**(덮는 글의 약 2.6배)이라 도구 출력, `tool_calls` JSON, cron · 하위 에이전트 세션은 넣지 않는다(뷰 `messages_fts_trigram_src`).
- 둘 다 `messages`의 INSERT · DELETE · UPDATE 트리거로 갱신한다.

**질의 하나의 길** (`H:hermes_state_search.py:1179-1285`)

1. 빈 질의면 빈 결과.
2. 도구 결과만 찾으라는 요청이면 색인을 쓰지 않고 원본을 LIKE로 훑는다(색인에는 앞부분만 있어서).
3. 색인이 낡았다고 표시돼 있으면 LIKE로.
4. 질의에 한중일 글자가 있으면(한글 음절 포함, `_CJK_RANGES`) **CJK 길**:
   - 끝의 `*`와 따옴표를 뗀다.
   - trigram 길을 쓸 수 있으면(단어가 3글자 이상 등) `messages_fts_trigram`에서 찾는다.
   - 아니면 연산자가 아닌 단어마다 `%단어%` LIKE 절을 만들어 훑는다(`"A OR B"`가 각 단어에 맞도록).
5. 그 밖은 `messages_fts`에서 MATCH. 정렬은 기본 `rank`, `newest`/`oldest`면 시각 먼저.
6. 문법 오류면 빈 결과. 색인이 깨진 오류면 색인을 떼고 LIKE로 답한다(고치는 일은 다른 길이 맡는다).
7. 색인을 다시 만드는 중이면 아직 안 들어간 구간을 LIKE로 채워 넣는다.
8. 결과가 0이고 한중일이 아니면: trigram 색인으로 한 번 더(영문과 한중일이 붙어 한 토큰이 된 경우를 건진다) → 그래도 0이면 **모든 단어(AND)를 아무 단어(OR)로 풀어** 한 번 더.
9. LIKE 길의 발췌는 일치 위치 앞 40자부터 120자.
10. 1초(`HERMES_SEARCH_SLOW_MS`)보다 느린 검색은 어느 길을 탔는지와 함께 로그에 남긴다.

### 4.3 가격인하에 옮길 때

| 무엇 | 판정 | 까닭 · 닿는 곳 |
|---|---|---|
| 표 모양(`sources` · `chunks` · `meta` · `embedding_cache`), 조각 400/80, 줄 번호 인용 | 그대로 | T16 · A12. 인용 `경로#L줄`은 복기 문서로 돌아가는 링크가 된다 |
| 합치는 식(0.7/0.3, 키워드 점수 다시 재기, 반감기 30일, MMR 0.7, 후보 4배, 결과 6, 문턱 0.35) | 그대로(숫자는 설정으로) | F2의 4번(비슷했던 지난 매매와 복기를 꺼내 본다). 가중치를 처음부터 우리가 지어내지 않는다 |
| 한글 처리: trigram + 3글자 미만은 LIKE | **그대로** | 두 원본이 같은 답을 냈다. "매수" · "손절" 같은 두 글자 단어가 trigram MATCH로는 안 잡힌다. `conformance.md` C11 |
| 0건이면 AND → OR로 풀어 다시(Hermes · OpenClaw 둘 다) | 그대로 | 에이전트가 문장으로 물을 때 |
| 임베딩 없으면 키워드만, 죽은 공급자가 벡터 색인을 덮어쓰지 않음, "쓸 수 없음"을 결과로 돌려줌 | 그대로 | 임베딩 공급자를 나중에 붙이는 1차 계획과 맞다 |
| 다시 색인 조건(메타 비교)과 옆에 만들어 바꿔치기 | 줄여서 | 조건 목록은 그대로, 바꿔치기는 한 트랜잭션 안의 표 교체로 |
| 도구 결과는 앞 8,192자만 색인, 가장 비싼 색인에서 기계 출력 빼기 | 그대로 | 시세 · 호가 원자료를 색인에 넣지 않는다 |
| sqlite-vec 가상 표 | 나중에 | 1차는 키워드. 벡터를 넣을 때 BLOB 칸 + 확장 표 두 곳에 두는 원본 모양을 따른다 |
| "조건으로 찾기"(종목 · 기간 · 손익 범위로 거르기) | **원본에 없음** | OpenClaw 검색은 글만 본다(걸러 내는 칸은 `source` · 경로뿐). 매매 기록의 구조화된 칸으로 거르는 것은 우리 표(`pi-store`)의 SQL로 새로 정한다 |
| 에이전틱 RAG(검색 → 읽기 → 다시 검색을 에이전트가 도는 것) | 방식만 | 원본은 `memory_search` → `memory_get` 두 도구를 주고 도는 것은 모델에 맡긴다. 따로 된 계획기는 없다 |

---

## 5. 로컬 접속 인증

네 원본의 비교 표(여는 주소 · 인증 수단 · 쿠키 · 원격 접속)는 `remote-access-security.md` 7.1에, DeskRPG의 쿠키 속성과 JWT 검증은 `conformance.md` 4절에 있다. 여기서는 그 표가 "확인 못 함"으로 남긴 칸과 키를 바꿨을 때의 처리만 적는다.

### 5.1 Hermes 대시보드: 인증 게이트 모드

`conformance.md` "확인하지 못한 것" 7번(게이트 모드의 쿠키 속성과 로그인 실패 제한)을 메운다.

#### 동작 원리 [관찰]

1. 모드는 둘이다.
   - **루프백 모드**(기본): 프로세스가 켜질 때 세션 토큰 하나를 정해(`resolve_session_token`) 화면(SPA)에 넣어 준다. 요청은 헤더 `x-hermes-session-token` 또는 `Authorization: Bearer`로 그 값을 보내고, 서버는 `hmac.compare_digest`로 비교한다 (`H:hermes_cli/web_server.py:226-236, 307-327`).
   - **게이트 모드**: 대시보드를 루프백이 아닌 주소에 `--insecure` 없이 열면 켜진다. 모든 요청이 등록된 인증 공급자가 검증한 세션을 들고 와야 한다. 이때는 토큰을 화면에 **넣지 않는다**(쿠키 인증) (`H:hermes_cli/dashboard_auth/__init__.py:1-4`, `web_server.py:334`). 공급자가 하나도 없으면 루프백이 아닌 주소로 여는 것 자체를 거부한다(`remote-access-security.md` 7.1).
2. 게이트 모드의 쿠키 (`H:hermes_cli/dashboard_auth/cookies.py:3-51, 74-85`):

| 쿠키 | 내용 | 수명 |
|---|---|---|
| `hermes_session_at` | 접근 토큰 | 토큰의 TTL |
| `hermes_session_rt` | 돌려 쓰는(rotating) 갱신 토큰. 공급자가 줬을 때만 | 30일 |
| `hermes_session_provider` | 어느 공급자의 토큰인지(비밀 아님. 갱신 토큰을 엉뚱한 공급자에 건네지 않으려는 길잡이) | 30일 |
| `hermes_session_pkce` | PKCE 상태 + CSRF nonce + 공급자 | 10분 |
| `hermes_sso_attempt` | 자동 SSO가 맴도는 것을 막는 표시 | 60초 |

   - 전부 `HttpOnly`, `SameSite=Lax`, Path는 프록시 접두사 또는 `/`. `Secure`는 요청이 https일 때만.
   - 이름 접두사: HTTP에서는 맨 이름, HTTPS + Path `/`에서는 `__Host-`, 프록시 접두사 뒤에서는 `__Secure-`. 쓰는 쪽과 읽는 쪽이 같은 함수로 이름을 정한다(어긋나면 세션이 조용히 깨진다는 주석).
   - PKCE 쿠키만 HTTPS에서 `SameSite=None; Secure`다(로그인 302 응답에서 심기 때문).
3. 비밀번호 로그인 `POST /auth/password-login {provider, username, password, next}` (`H:hermes_cli/dashboard_auth/routes.py:399-496`):
   - **시도 제한: 클라이언트 IP마다 60초에 10번.** 넘으면 429 "Too many login attempts. Try again shortly." 프로세스 메모리 안의 미끄러지는 창이고(다시 켜면 초기화), IP를 모르면 한 통(`_unknown_`)에 몰아 센다(막는 쪽으로 실패). 통은 4096개까지, 넘으면 오래된 것부터 버린다.
   - **잠금 시간은 따로 없다.** 창에서 오래된 시도가 빠지면 다시 된다. 성공해도 세던 것을 지우지 않는다. 시도는 성공 · 실패를 가리지 않고 센다.
   - 실패 응답은 일부러 뭉뚱그린다: 모르는 공급자 404, 틀린 자격 401 "Invalid credentials", 저장소에 못 닿음 503. 아이디가 있는지 알려 주지 않는다.
   - 프록시 뒤라면 IP 정규화는 앞단이 해야 한다(코드가 ASGI의 상대 주소를 그대로 쓴다).
4. 쿠키로 인증한 **쓰기 요청**은 자기 페이지에서 온 것만 받는다: 안전한 메서드(GET · HEAD · OPTIONS · TRACE)가 아니면 `Sec-Fetch-Site`가 `same-site` · `cross-site`일 때 403 (`H:hermes_cli/dashboard_auth/middleware.py:84-97`).
5. WebSocket: 브라우저는 업그레이드 요청에 헤더를 못 붙이므로, 인증된 `POST /api/auth/ws-ticket`으로 **한 번만 쓰는 표(ticket)** 를 받아 `?ticket=`으로 넘긴다. 수명 30초 (`H:hermes_cli/dashboard_auth/ws_tickets.py:1-21`).
6. 로그아웃 `POST /auth/logout`: 쿠키를 지우고 공급자에 폐기를 요청한다(실패해도 로그만). 접근 토큰도 폐기한다(수명이 긴 접근 토큰이 갱신 토큰보다 오래 살 수 있어서) (`routes.py:513-528`).
7. 감사 기록: 로그인 실패 · 성공을 파일에 남긴다. 칸은 256자에서 자르고, 파일은 5MiB × 3개로 돌린다 (`H:hermes_cli/dashboard_auth/audit.py:18-28`).

**키를 바꿨을 때**: 루프백 모드의 세션 토큰은 프로세스가 쥔 값 하나라, 값을 바꾸면(다시 켜면) 예전 토큰을 든 화면은 전부 401이 된다 [추정: 비교가 지금 값 하나와만 이뤄지므로]. 게이트 모드는 공급자가 토큰을 검증하므로 공급자 쪽 폐기에 달려 있다.

### 5.2 OpenClaw 게이트웨이

#### 동작 원리 [관찰]

1. **시도 제한** (`O:src/gateway/auth-rate-limit.ts:17-47, 99-107, 192-238`):
   - 기본: 60초 창에 실패 10번 → **300초 잠금**. 추적하는 신원 10,000개까지, 60초마다 정리.
   - 열쇠는 `(범위, 클라이언트 IP)`. 범위가 나뉘어 있어 한 종류의 실패가 다른 종류를 잠그지 않는다: `shared-secret`(토큰 · 비밀번호), `device-token`, `bootstrap-token`, `device-join`, `browser-pair`, `node-pairing`, `hook-auth` 등.
   - 잠겼으면 `{allowed: false, remaining: 0, retryAfterMs}`. 잠금이 풀리는 시각이 지나면 표시를 지운다.
   - **루프백은 기본으로 잠그지 않는다**(`exemptLoopback: true`). 대신 루프백의 실패에는 응답을 늦춘다: 250ms에서 시작해 두 배씩, 5초까지. 프록시 뒤에서 밖의 시도가 루프백으로 보일 수 있다는 위험은 `remote-access-security.md` 7.2에 적혀 있다.
2. **토큰 자동 생성**: 토큰 없이 켜면 무작위 24바이트를 16진수 48자로 만들어 쓴다 (`O:src/gateway/startup-auth.ts:199`).
3. **기기 서명**: 연결할 때 기기가 서명한 시각과 서버 시각의 차이는 ±2분까지 (`O:src/gateway/server/ws-connection/connect-auth.ts:69`).
4. **기기 짝짓기**: 새 기기의 요청은 5분 동안만 대기한다(`DEVICE_PAIRING_PENDING_TTL_MS`). 브라우저 짝짓기 코드는 10분, 동시에 32개까지. 부트스트랩 토큰은 10분 (`O:src/infra/device-pairing-state.kernel.ts:9`, `device-pairing-browser-code.ts:12-17`, `device-bootstrap.worker-types.ts:5`).
5. **공유 비밀을 바꿨을 때** (`O:src/gateway/server-shared-auth-generation.ts`, `server-secrets-reload.ts:160-196`):
   - 게이트웨이는 공유 인증 설정에서 "세대(generation)" 값을 뽑아 둔다. 연결된 클라이언트마다 `usesSharedGatewayAuth`와 연결 당시의 세대를 기억한다.
   - 설정 · 비밀을 다시 읽어 세대가 달라지면 **예전 세대로 들어온 공유 비밀 연결을 끊는다**(`disconnectStaleSharedGatewayAuthClients`).
   - 기기 토큰으로 들어온 연결은 공유 비밀과 무관하므로 이 길로는 끊기지 않는다 [추정: 끊는 대상이 `usesSharedGatewayAuth`인 클라이언트로 한정돼 있다].
   - trusted-proxy 모드끼리의 변경은 허용 사용자 목록과 권한이 같으면 끊지 않는다.

확인하지 못한 것: 핸드셰이크 시간 초과 값, `connect` 프레임 검증의 정확한 순서, 기기 토큰을 해시로 저장하는지, 기기 토큰의 회전 · 폐기 흐름(소스에서 값을 찾지 못했다).

### 5.3 DeskRPG

`remote-access-security.md` 7.1 · 7.2와 `conformance.md` 4절에 있는 것 그대로다: 아이디 · 비밀번호(bcrypt) → HS256 JWT 쿠키 7일, 갱신 없음, 폐기 수단 없음, 로그인 시도 제한은 찾지 못함. 이번에 더 뜯지 않았다.

### 5.4 가격인하에 옮길 때

| 무엇 | 판정 | 까닭 · 닿는 곳 |
|---|---|---|
| 같은 컴퓨터는 켤 때 정한 토큰을 화면에 넣어 주고, 다른 기기는 게이트(쿠키) | 그대로 | F8 · T17. Hermes의 두 모드가 우리 스펙의 "같은 컴퓨터는 바로, 다른 기기는 접속 키"와 같다 |
| 쿠키 속성(`HttpOnly`, `SameSite=Lax`, https일 때 `Secure`, `__Host-` 접두사) | 그대로 | `conformance.md` D2 · D6 |
| 쓰기 요청의 `Sec-Fetch-Site` 검사 | 그대로 | 주문 승인 버튼을 다른 사이트가 누르게 하는 공격을 막는다 |
| WebSocket 한 번 쓰는 표(30초) | 그대로 | 실시간 연결 주소에 긴 수명의 키를 싣지 않는다 |
| 시도 제한: OpenClaw(10번/60초 → **5분 잠금**, 종류별로 따로) | **그대로** | "여러 번 틀리면 잠근다". Hermes는 잠금이 없고 창만 있다. 스펙이 잠금을 말하므로 OpenClaw 쪽 |
| 루프백은 잠그지 않고 늦추기(250ms~5초) | 그대로 | 같은 컴퓨터의 사용자를 자기 서버에서 잠그지 않는다. 단 프록시(Tailscale Serve) 뒤에서는 밖의 요청이 루프백으로 보이므로 **프록시를 거친 요청은 루프백으로 치지 않는 규칙을 새로 정해야 한다** |
| 실패 응답 뭉뚱그리기, 감사 기록(5MiB × 3) | 그대로 | |
| 키를 바꾸면 예전 키로 들어온 연결을 끊는 "세대" | **그대로** | "키를 바꿨을 때 기존 세션". DeskRPG의 7일 JWT에는 폐기 수단이 없어 본받지 않는다 |
| 기기 짝짓기(서명 · 기기 토큰) | 안 가져옴 | 사용자 한 명 · 접속 키 하나로 충분하다(`remote-access-security.md` 4장이 TOTP를 따로 다룬다) |

원본에 없어서 우리가 새로 정해야 하는 것: 잠금이 걸렸을 때 **이미 들어와 있는 세션**을 어떻게 할지(원본은 새 시도만 막는다), 잠금을 사용자가 같은 컴퓨터에서 푸는 길.

---

## 6. 백업 · 옮기기

### 6.1 Hermes

`hermes-agent.md`에는 "나중에" 한 줄뿐이었다(`conformance.md` T19 줄).

#### 동작 원리 [관찰] (`H:hermes_cli/backup.py`)

**만들기 (`hermes backup`)** (`:593-683`)

1. 백업 잠금 `.backup.lock`을 잡는다(0.25초 안에 못 잡으면 그만둔다).
2. 홈 폴더를 훑어 담을 파일을 모은다. 빼는 것 (`:74-153`):
   - 다시 만들 수 있는 것: 코드 저장소(`hermes-agent`), `__pycache__`, `.git`, `node_modules`, venv, 각종 캐시, 체크포인트, 받아 둔 모델 같은 런타임 폴더.
   - 이전 백업(`backups`, `state-snapshots`). 백업이 백업을 품지 않게.
   - 브라우저 프로필(잠긴 SQLite 때문에 백업이 영영 멈추기도 하고, 로그인 정보가 들어 있어 넣으면 안 됨).
   - SQLite 곁 파일(`.db-wal`, `.db-shm`, `.db-journal`). **새로 뜬 스냅샷에 낡은 WAL을 짝지으면 되살릴 때 찢어진다.**
   - 이 기계에서만 뜻이 있는 것: `.backup.lock`, `gateway.pid`, `cron.pid`.
   - `cache/` 아래는 다시 만들 수 없는 것만 담는다: 주고받은 이미지 · 소리 · 영상 · 문서 · 스크린샷, 인용 근거 장부.
3. zip(DEFLATE, 수준 6)으로 묶는다. 임시 이름에 쓰고 끝나면 이름을 바꾼다(원자적). `*.db`는 파일 복사가 아니라 **SQLite 백업 API로 스냅샷**을 떠서 넣는다(실행 중에도 안전). 뜨는 데 실패한 DB는 오류 목록에 넣고 나머지는 계속한다.
4. 못 넣은 파일이 있어도 묶음은 남기고 실패로 보고한다. **오래된 백업 지우기는 온전한 백업 뒤에만** 한다("같은 파일을 매번 못 읽는 타이머가 마지막으로 온전했던 백업을 밀어내면 안 된다").
5. 파일 이름 접두사 `hermes-backup-`.

**비밀의 처리** [관찰]: `.env`, `auth.json`, `state.db`, `vault.key`, `vault.json.enc`를 **그대로(암호화 없이) 담는다.** 이 이름 목록(`_SECRET_FILE_NAMES`, `:166-170`)은 빼려는 것이 아니라, 되살릴 때 권한을 0600으로 조이려는 것이다(zip이 권한 비트를 잃어버려서). 암호화 옵션은 찾지 못했다.

**되살리기 (`hermes import`)** (`:736-911`)

1. zip인지, 백업 모양인지 검사한다. 묶음 안의 맨 위 폴더 이름(접두사)을 알아내 떼어 낸다.
2. 이미 설치가 있으면(`config.yaml` 또는 `.env`) 덮어쓴다고 알리고 묻는다(`--force`면 묻지 않음).
3. **쓰기 전에** 모든 구성원을 풀어 보고 CRC를 본다. 하나라도 깨졌으면 "아무것도 되살리지 않았다"로 끝낸다(반쯤 되살린 상태를 만들지 않는다).
4. 푼다. 경로는 안전한 조각으로 정규화한다(경로 탈출 방어, `H:hermes_cli/archive_safe.py`). 이 기계의 런타임 상태 파일(`gateway_state.json`, `gateway.pid`, `cron.pid`, `gateway.lock`, `processes.json`)은 묶음에 있어도 **덮어쓰지 않는다**(원본 기계의 PID · 상태가 들어오면 게이트웨이가 "시작 중"에서 멈춘다).
5. 세션 DB가 묶음 쪽이 더 작으면 "오래된 백업 내용으로 바뀌었다, 백업 뒤에 기록된 것은 없다"고 경고한다.
6. 프로필 실행 스크립트를 다시 만든다.
7. **게이트웨이를 다시 살린다**(`_revive_gateway_after_import`, `:1071-1093`): 기본 홈에 되살렸고 서비스가 안 돌고 있으면 묻지 않고 서비스를 설치 · 시작한다. "봇 토큰과 cron 작업은 게이트웨이 없이는 죽어 있다"는 까닭이다. 기본 홈이 아닌 곳에 되살렸으면 건드리지 않는다. **곧 되살린 뒤 예약 작업이 바로 다시 돈다.**

**자동 백업** [관찰]

- 빠른 스냅샷(`state-snapshots/`): 상태 파일 몇 개의 사본. 최근 20개 (`:1105-1123`).
- 업데이트 전 전체 백업(`backups/pre-update-*`) 최근 5개, 마이그레이션 전(`pre-migration-*`) 최근 5개. 쓰다 만 것은 `.incomplete.zip`으로 남겨 따로 치운다 (`:1156-1161`).
- 업데이트 뒤 cron 안전망(`H:hermes_cli/backup_cron_prompts.py:1-6`): 업데이트가 작업 수는 그대로 둔 채 작업의 프롬프트를 작업 이름으로 뭉개 버린 경우, 스냅샷과 비교해 프롬프트를 되돌린다.

### 6.2 OpenClaw

#### 동작 원리 [관찰] (`O:docs/cli/backup.md`. 이 절은 문서를 읽었고 구현 소스는 `migrate`의 백업 호출만 확인했다)

**만들기 (`openclaw backup create`)**

1. 담는 것: 상태 폴더, 설정(과 그것이 `$include`로 끌어오는 파일. 상태 폴더 밖에 있어도), 인증 프로필, 채널 · 공급자 자격, 세션, (기본으로) 작업 폴더. `--no-include-workspace`는 작업 폴더만 빼고, `--only-config`는 설정만 (`:21-23, 147, 205`).
2. 형식: `.tar.gz`. 이름은 `<시각>-openclaw-backup.tar.gz`. 안에 판 번호 1의 `manifest.json`(원래 경로, 묶음 안의 배치, 에이전트 id와 폴더, 그때 뜬 SQLite 스냅샷 목록)이 들어간다 (`:44, 143`).
3. 게이트웨이가 돌고 있어도 만든다. DB는 스냅샷으로 뜬다 (`:70-75`).
4. **있는 묶음 파일은 절대 덮어쓰지 않는다. 출력 경로가 원본 상태 · 작업 폴더 안이면 거절한다**(자기 자신을 담지 않게) (`:145`).
5. 설정 사본은 사람이 쓴 그대로(주석 · 환경 변수 자리표시 유지)이고 **풀린 비밀 값은 설정 사본에 쓰지 않는다.** 다만 다른 파일들에 민감한 내용이 있을 수 있으니 묶음을 보호하라고 적는다 (`:147`).
6. 설정을 뜨는 동안 파일이 바뀌면 만들기를 실패시킨다 (`:148-150`).

**암호화와 올리기**: 묶음 자체는 평문이다. `--to <저장 위치>`로 올릴 때 **저장 위치 설정이 암호화와 자격을 맡는다**(내장 `filesystem`, Cloudflare R2 플러그인). 순서: 위치를 열어 확인 → 임시 폴더에 묶음 만들기 → manifest와 내용 검증(올릴 때는 `--verify` 없이도 항상) → 올리기 → 저장된 크기 확인 → 임시 묶음 삭제. `--output`으로 로컬 사본도 남기면 그 사본은 **저장소 암호화가 켜져 있어도 평문 `.tar.gz`** 다. 초기화되지 않은 위치는 만들기 전에 실패하고, 백업이 저장소를 몰래 초기화하지 않는다. "암호화 암호문과 위치 표식을 복구용으로 보관하라" (`:180-197`). 보존: `--keep-daily 7 --keep-weekly 4 --keep-monthly 12`. 예약: `backup enable --to offsite --every 24h` (`:40, 57-58`).

**검증 (`backup verify`)**: 루트 manifest가 정확히 하나인가, 경로 탈출 모양의 경로와 위험한 심볼릭 링크가 없는가, manifest가 말한 내용물이 다 있는가, SQLite 스냅샷이 온전한가 (`:146`).

**되살리기 (`backup restore --target <폴더>`)** (`:298-358`)

1. 대상은 없거나 빈 폴더여야 하고, **지금 쓰는 상태 폴더나 에이전트 폴더 안이면 안 된다.**
2. 묶음과 그 안의 SQLite를 **대상을 만들기 전에** 검증한다. 실패하면 반쯤 푼 것을 지운다.
3. **제자리 되살리기는 없다. `--force`도 없다.**
4. 켜는 것은 따로 하는 단계다: 게이트웨이를 멈추고 → 되살린 상태를 제자리로 옮기거나 `OPENCLAW_STATE_DIR`이 가리키게 하고 → `openclaw doctor`를 돌린 뒤 → 다시 켠다.
5. 경고: "되살리기는 시간 여행이다." 메신저 자격이 어긋날 수 있고, **승인과 전달 · 중복 방지 상태도 되돌아가므로 게이트웨이를 다시 켜기 전에 대기 중인 승인을 살펴보라** (`:339-343`). 원본과 되살린 설치를 같은 자격으로 동시에 돌리지 말라 (`:134`).

**Hermes 가져오기 (`openclaw migrate hermes`)** (`O:docs/install/migrating-hermes.md:20-174`, `O:src/commands/migrate/apply.ts:47-125`)

1. 원본 찾기: Hermes 홈 · 프로필을 알아낸다. `--from <경로>`로 바꾼다.
2. 계획 만들기(`provider.plan`): 가져올 항목 목록. 충돌 · 건너뛸 것 · 민감한 것이 표시되고, 비밀처럼 보이는 키의 값은 출력에서 가린다. `--dry-run`은 여기서 멈춘다(`--json`도 `--yes` 없이는 계획만 찍는다).
3. 고르기 적용 → **충돌 검사**: 계획에 충돌(대상에 이미 파일 · 설정 값이 있음)이 있으면 적용을 거절한다. 고르지 않은 항목의 충돌은 막지 않는다(고르기를 먼저 적용하므로). `--overwrite`로만 넘어간다.
4. **백업**: `backup create`를 `verify: true`로 부른다. 백업할 상태가 아직 없으면(새 설치) 건너뛴다. `--no-backup`으로 끌 수 있다.
5. 보고서 폴더를 만든다(상태 폴더 아래, 공급자별).
6. 적용(`provider.apply`): 도중에 한 항목이 충돌나면 그 항목만 충돌로 적고 **나머지는 계속한다.** 같은 기억 가져오기를 되풀이해도 중복이 생기지 않는다. 일부 실패면 전체 보고서를 내고 종료 코드 1.
7. `doctor` → 다시 켜기.

가져오는 것: 기본 모델 · 공급자 · 사용자 정의 엔드포인트, MCP 서버 정의, `MEMORY.md` · `USER.md`(**덮어쓰지 않고 뒤에 붙인다**), 스킬(`SKILL.md`가 있는 폴더)과 스킬별 설정. 자격(OAuth 항목과 `.env`의 지원되는 키)은 대화형에서는 묻고(기본 예), 무인 실행에서는 `--include-secrets`가 있어야 한다.

**가져오지 않고 보고서 폴더에 복사만 해 두는 것**(사람이 보라고): `sessions/`, `cron/`, `mcp-tokens/`, `plans/`, `workspace/`, `skins/`, `kanban/`, 그리고 `state.db` 등 DB 파일 전부. 곧 **예약 작업과 대화 기록은 자동으로 넘어오지 않는다.**

REA로 본 것: 이전에 `rea-triggers/oc-backup-migrate/`에 파일을 복사해 둔 것까지만 있고 분석 결과는 남아 있지 않았다. 이 절은 문서와 `apply.ts`를 직접 읽었다.

### 6.3 가격인하에 옮길 때

| 무엇 | 판정 | 까닭 · 닿는 곳 |
|---|---|---|
| DB는 파일 복사가 아니라 SQLite 백업 API로, WAL · SHM은 담지 않음 | 그대로 | F9 · T19 · A14. 실행 중(장 마감 뒤에도 팀은 돈다)에 뜬다 |
| 다시 만들 수 있는 것 · 이 기계의 런타임 상태 · 이전 백업은 빼기 | 그대로 | |
| manifest(판 번호 · 경로 · 스냅샷 목록) + 검증(경로 탈출 · 빠진 내용물 · DB 온전함) | 그대로 | 되살리기 전에 검증하고, 깨졌으면 아무것도 쓰지 않는다(두 원본이 같다) |
| 온전한 백업 뒤에만 오래된 것 지우기, 보존 일 7 · 주 4 · 월 12 | 그대로 | |
| 올리기 순서(위치 확인 → 임시로 만들기 → 검증 → 올리기 → 크기 확인 → 임시 삭제) | 그대로 | "매일 장 마감 뒤 암호화해 올린다" |
| **암호화** | **원본과 다르게** | Hermes는 비밀을 평문으로 담고, OpenClaw는 묶음은 평문이고 저장 위치가 암호화한다. 개발 스펙 D7 · A14가 이미 정했다(SQLite 스냅샷을 age로 암호화해 private GitHub 저장소에, 비밀키는 백업하지 않음). 원본에서 가져올 것은 암호화가 아니라 **올리기 전 검증과 "로컬 사본은 평문으로 남는다"는 함정**이다 |
| **되살린 뒤의 상태** | **원본과 다르게** | Hermes는 게이트웨이를 다시 살려 예약 작업이 바로 돈다. OpenClaw는 켜는 것을 사람에게 맡기고 "대기 중인 승인을 살펴보라"고 경고만 한다. 우리 스펙(F9)은 "되살리면 멈춘 상태로 시작"이다. OpenClaw의 경고가 까닭을 준다(승인 · 중복 방지 상태가 과거로 돌아간다). **멈춤 표시를 백업 안에 넣을지, 되살리기가 쓸지는 새로 정해야 한다** |
| 제자리 되살리기 없음(빈 폴더에만) | 그대로 | 지금 도는 팀의 DB를 덮어쓰지 않는다 |
| 이 기계의 런타임 파일은 되살릴 때 덮어쓰지 않기 | 그대로 | |
| 가져오기의 순서(계획 → 미리 보기 → 충돌이면 거절 → 백업 → 적용 → 보고서) | 방식만 | 우리에게 "다른 도구에서 가져오기"는 1차에 없다. 템플릿 적용(T8)과 설정 바꾸기에 같은 순서를 쓴다 |
| 기억 파일은 덮어쓰지 않고 뒤에 붙이기 | 그대로 | 다른 컴퓨터에서 옮겨 올 때 복기 기록을 잃지 않는다 |

---

---

## 바로잡을 곳 (기존 조사 문서)

| 문서 | 적힌 것 | 소스 |
|---|---|---|
| `openclaw.md` 1장 판정 표 (a) | Hermes에 "범용 웹훅 → 에이전트 라우팅이 없다", 바깥 HTTP 입구는 메신저 콜백과 `/api/cron/fire`뿐 | 범용 webhook 어댑터가 있다. 경로별 서명 · 속도 제한 · 중복 표 · 묶어 보내기 · cron 작업 발사 (`H:gateway/platforms/webhook.py:1-10, 693-771`) |
| `openclaw.md` 2.1 "Hermes와 차이" | Hermes에 묶어 보내기 · 상시 이벤트가 없다는 뜻으로 읽힘 | webhook 경로의 `coalesce`(조용한 30초, 최대 300초)가 있다 (`H:gateway/platforms/webhook_coalesce.py:22-23`) |
| `openclaw.md` 2.2 | 승인 저장을 "공유 상태 DB의 단일 행(JSON 문서)"으로만 적음 | 그것은 허용 목록 문서다. 대기 중인 승인은 표 `operator_approvals`에 한 줄씩 있다 (`O:src/state/openclaw-state-schema.sql:370-475`) |
| `openclaw.md` 5.3의 5 | 승인이 다시 켠 뒤 복구되는 방법 미확인 | 복구하지 않는다. 다른 `runtime_epoch`의 대기를 `cancelled / gateway-restart`로 닫는다 (`O:src/gateway/operator-approval-store.transitions.worker.ts:245-276`) |
| `openclaw.md` 2.3 | "키워드 점수는 최고 의미 점수를 넘지 않게 축척"(식 없음), 가중치 미확인 | 식은 이 문서 4.1의 4번, 기본값 0.7 / 0.3 (`O:src/agents/memory-search.ts:163-169`, `O:extensions/memory-core/src/memory/hybrid.ts:47-64`) |
| `hermes-agent.md` 6.1 | 연속 실패 처리 언급 없음 · "못 본 부분" 7(작업 파일을 매 tick 다시 읽는지) | 매 tick 잠금 안에서 다시 읽는다. 연속 실패로 자동으로 끄지 않고 3번째부터 알림에 덧붙인다 (`H:cron/jobs.py:3269-3271`, `H:cron/scheduler.py:137-163`) |
| `hermes-agent.md` "못 본 부분" 4 | `approvals.timeout` 기본값 미확인 | 300초 (`H:tools/approval_context.py:318-328`) |
| `remote-access-security.md` 7.1 "시도 제한" Hermes 칸 | 확인 못 함 | 대시보드 비밀번호 로그인: IP당 60초 10번, 429, 잠금 없음 (`H:hermes_cli/dashboard_auth/routes.py:403-447, 474-476`) |
| `remote-access-security.md` 7.1 "승인 실패" Hermes 칸 | "재시작하면 타임아웃 거부" | 재시작하면 대기 줄 자체가 메모리에서 사라진다(시간 초과로 거부되는 것은 기다리던 프로세스가 살아 있을 때다) (`H:tools/approval.py:150-184`의 줄이 메모리 구조) |
| `conformance.md` "더 뜯어야 할 것" T22 | 웹 푸시는 원본에 없다 | OpenClaw에 승인용 웹 푸시가 있다(조용한 시간 · 종류별 설정 포함) (`O:src/gateway/approval-web-push.ts:1-20`) |

2026-11-14 앞 문서에 반영함 (10건 모두. 못 한 것: 없음. 해당 절이 없는 "6.1 연속 실패 처리 언급 없음"은 `hermes-agent.md` 6.1 tick 목록에 10번 항목을 더해 반영했다).

---

## REA를 쓴 범위와 직접 읽은 범위

| 구분 | 범위 |
|---|---|
| REA (`rea analyze-javascript-application`, v6.7.0) — 이번에 돌린 것 | OpenClaw 스케줄러 묶음 24개 파일(`src/cron/store/{scheduler-actor.worker,dispatch.worker,run-recovery.kernel,run-receipt-store,save.kernel}.ts`, `src/cron/service/{scheduler-actor,timer-trigger,timer-runnable,timer-outcomes,jobs-scheduling,auto-disable,failure-alerts,startup-run-repair,run-recovery,wake,timeout-policy,timer-constants,occurrence-admission}.ts`, `src/cron/{schedule,stagger,active-hours,session-reaper}.ts`, `src/infra/system-events.ts`, `src/auto-reply/tokens.ts`)을 빈 폴더 `…/scratchpad/rea-triggers/oc-cron2/`에 복사해 그 폴더를 루트로 돌렸다. 성공, 파싱 실패 0, 노드 527 · 관계 683(import 524 · exposes 111 · contains 48). 출력 `…/scratchpad/rea-triggers/out/oc-cron/analysis.json`(약 35MB) |
| REA — 이전 실행의 출력을 쓴 것 | DeskRPG 승인 묶음 22개 파일(`…/rea-triggers/dk-approval/`, 출력 `…/rea-triggers/out/dk-approval/`). 타이머 위치와 설정 출처(환경 변수 · 기본값) 목록을 얻었다 |
| REA가 준 것 | 모듈 사이의 import · 내보내기 관계, 타이머 · 설정 출처의 위치. **호출 순서 · 조건 · 숫자는 주지 않았다** |
| 직접 읽음 | 위를 뺀 전부. Hermes(Python) 전체, OpenClaw의 SQL 스키마 · 승인 · 기억 검색 · 인증 · `migrate`, OpenClaw 백업은 문서(`docs/cli/backup.md`, `docs/install/migrating-hermes.md`, `docs/automation/cron-jobs/*.md`), DeskRPG `tool-approvals.ts` · `tool-approval-types.ts` |

쓴 명령:

```bash
# 관련 파일만 경로를 유지해 빈 폴더에 복사한 뒤, 그 폴더에서
rea analyze-javascript-application . --artifact-format directory \
  --source-path <파일> [--source-path <파일> ...] --max-heap-mb 1024 --format json
```

보조 에이전트에게 원본 읽기를 나눠 맡기려 했으나 사용량 한도로 전부 결과 없이 끊겼다. 이 문서의 줄 번호는 전부 이 조사에서 직접 본 것이다. 끝에 숫자가 걸린 줄 17곳(동시 실행 8, 자동 끔 10, 승인 붙들기 15초, 토큰 24바이트, 짝짓기 대기 5분, 발췌 700자, 승인 번호 UUID, 일회성 유예 120초, 보존 7일 · 50개 · 20개, 알림 6시간, 비밀 파일 목록, 표 30초, 묶기 30초, 카드 다시 보내기, 기본 300초)을 원본과 다시 대조했고 전부 맞았다. 나머지 줄 번호는 읽으면서 적은 값이고 다시 대조하지 않았다.

## 확인하지 못한 것

1. OpenClaw에서 깨워진 턴이 받는 입력의 머리말 문구(시스템 이벤트 대기열의 글이 프롬프트에 들어가는 모양).
2. OpenClaw 조각 나누기의 토큰당 글자 수 어림값(`CHARS_PER_TOKEN_ESTIMATE`의 정의 줄), MMR이 쓰는 비슷함 계산의 정확한 식, sqlite-vec를 못 불러올 때 벡터 검색을 무엇으로 대신하는지.
3. OpenClaw 연결 핸드셰이크의 시간 초과 값과 검증 순서, 기기 토큰의 저장 방식(해시인지) · 회전 · 폐기, Control UI가 토큰을 어디에 두는지.
4. OpenClaw `backup create` · `restore`의 구현 소스(문서만 읽었다). 저장 위치 암호화의 방식.
5. OpenClaw 승인의 "답 뒤 쓸 수 있는 시간 창"(redemption window)의 값, plugin 승인이 exec 승인과 다른 점, standing grant의 일치 판정 코드.
6. Hermes `/api/cron/fire`의 인증과 동작, webhook 서명의 헤더 이름 전체, 스크립트 wake gate의 출력 크기 제한.
7. Hermes 사라지는 시각(봄 서머타임)의 실제 동작. 코드에 전용 처리가 없다는 것까지만 봤다.
8. Hermes `session_search` 도구의 요약 단계(보조 모델 · 글자 상한 · 동시 수)와 메모리 도구의 잠금 · 주입 검사. `hermes-agent.md` 3.5 · 3.6에 있는 것 말고 더 읽지 못했다.
9. Hermes 프로필 내보내기 · 가져오기(`profiles` export/import)와 OpenClaw에서 Hermes로 가져오는 길.
10. DeskRPG 로그인 · 세션의 추가 사항(가입 규칙, 비밀번호 해시 비용 인자, 소켓의 만료 검사)과 플러그인의 `approval.blocked` 조건. 기존 문서에 있는 것 말고 더 읽지 못했다.

## 사용자가 정해야 하는 것

1. **답이 없는 승인.** 신호마다 기다리는 시간, 시간이 지난 뒤 팀이 고를 수 있는 동작의 범위(원본은 전부 "거절" 하나).
2. **다시 켠 뒤의 승인.** 원본은 전부 닫는다. 우리는 만료 전이면 다시 물을지, 닫고 팀이 새로 판단하게 할지.
3. **놓친 트리거와 장 시간.** 꺼져 있던 동안의 장 시작 브리핑 · 시세 조건을 언제까지 따라잡을지(원본의 유예는 주기만 본다).
4. **연속 실패한 예약.** Hermes처럼 끄지 않고 알리기만 할지, OpenClaw처럼 10번이면 끌지.
5. **백업에 담을 범위.** 암호화 방식은 개발 스펙 D7이 정했다(age). 남은 것은 원본처럼 "다시 만들 수 있는 것 · 이 기계의 런타임 상태"를 어디까지 뺄지와, 올리기 전에 만든 로컬 임시 묶음을 지우는 규칙이다.
6. **되살린 뒤 멈춤.** 멈춤을 누가 어떻게 표시하고, 풀 때 승인이 필요한지(스펙 F4의 "전체 정지 뒤 다시 시작"과 같은 길로 묶을지).
7. **잠금.** 5분 잠금(OpenClaw 값)을 그대로 쓸지, 잠긴 동안 이미 들어온 세션을 어떻게 할지, 프록시를 거친 요청을 루프백으로 치지 않는 규칙.
8. **승인 뒤 시세 변화.** 승인한 가격에서 얼마나 벗어나면 다시 묻는지(원본은 "같은 내용인가"만 본다).
