# 에이전트 실행기와 여러 에이전트 협업: 다시 만들 수 있는 깊이의 동작 원리

**결론: 담당 한 명의 턴은 Hermes 턴 고리를 숫자까지 그대로 옮기고, 팀 논의는 Hermes hosted room의 "로그에서 다음 턴을 계산하는" 방식으로 만들되, 점수와 팀장 판단 단계는 원본에 없어 새로 붙여야 한다.**

1. **뜯은 것**: 한 턴의 순서와 횟수(1절), 프롬프트 조립과 캐시(2절), 압축의 문턱 · 경계 · 요약 틀(3절), 오류 분류표와 자격 풀(4절), 방의 계획기 · 드라이버 · `message_agent`(7절), 사용자 대화의 끝에서 끝까지(8절). 세션 · 메모리(5 · 6절)와 OpenClaw(9절)는 부분만.
2. **가장 큰 발견**: DeskRPG는 Hermes의 방 기능을 쓰지 않고 회의 순서를 스스로 짠다(매 바퀴 전원에게 손들기 질문). 같은 문제의 두 해법이라 하나를 골라야 하고, 이 문서는 Hermes 쪽(재시작에 견디고 폴링 호출이 없다)을 권한다(7.9).
3. **원본에 없는 것**: 담당별 점수, 합의 점수, 팀장의 최종 판단 턴, 팀 전체가 보는 기억. F2 6~8번은 우리가 설계해야 한다(7.6 표 끝줄, 6절).
4. **기존 문서에서 틀린 곳**: 압축 문턱은 200K 창 모델에서 50%가 아니라 75%다. 방 이벤트 종류가 5개 빠져 있다. 아래 "바로잡을 곳".
5. **정해야 할 것**: 턴 시간 제한, 발언 순서 규칙, 점수 단계의 모양 등 8가지. 맨 끝 절.

| 항목 | 깊이 | 절 |
|---|---|---|
| 한 턴의 동작 원리 | 충분 | 1 |
| 시스템 프롬프트 조립 | 충분(스킬 색인 모양만 빠짐) | 2 |
| 컨텍스트 압축 | 충분(호출 전 관문 · DB 표시 · 게이트웨이 85% 위생은 빠짐) | 3 |
| 공급자 호출 실패 처리 · 자격 풀 | 충분 | 4 |
| 세션과 기록 저장 | 부분(자동 리셋 정책 빠짐) | 5 |
| 메모리 | 부분(외부 공급자 · 실패 상한 빠짐) | 6 |
| 여러 에이전트의 협업 | 충분(hosted room · message_agent · DeskRPG 회의), delegate는 덧붙임만 | 7 |
| 사용자와의 대화 | 충분(위험 판정 규칙은 빠짐) | 8 |
| OpenClaw 보완 | 부분(큐 모드와 sessions_send만) | 9 |


표기: `H:` Hermes Agent(Python, MIT), `O:` OpenClaw(TypeScript, MIT), `D:` DeskRPG(TypeScript, Sustainable Use — 방식만 참고), `P:` DeskRPG Hermes 플러그인(Python, LICENSE 없음 — 명세만 참고). 경로는 각 저장소 루트 기준이고 `경로:줄`로 적는다. [관찰]은 소스에서 직접 본 것, [추정]은 본 것에서 끌어낸 것이다. 원본 코드는 옮기지 않았다(구조 · 흐름 · 이름 · 숫자만).

기존 조사 문서에 이미 있는 것은 다시 쓰지 않고 절 번호로 가리킨다: `hermes-agent.md`, `openclaw.md`, `deskrpg.md`, `deskrpg-hermes-plugin.md`.

---

## 1. 한 턴의 동작 원리 (Hermes)

기존 문서 `hermes-agent.md` 3.1(입출력 칸) · 3.2(순서도 · 단계 파일 이름 · 불변식 · 도구 병렬 숫자 · 재시도 표)는 다시 쓰지 않는다. 아래는 그 절에 없는 **분기 조건 · 횟수 · 저장 시점 · 끊겼을 때**다. 전부 직접 읽은 것이다.

### 동작 원리 [관찰]

**가. 턴에 들어가기 전 — 세션 턴 lease** (`H:agent/turn_facade_lease.py`)

1. 세션 하나는 한 번에 한 프로세스만 "읽기 → 실행 → 쓰기"를 한다. 턴을 시작할 때 `session_turn_leases` 행을 잡는다. 쥔 쪽 표식은 `pid=<pid>…:turn=<턴 id>:platform=<플랫폼>` (`:323-326`).
2. 수명 300초, 60초마다 갱신, 남이 쥐고 있으면 최대 1800초 기다린다(`LEASE_TTL_SECONDS`, `LEASE_WAIT_SECONDS`, `refresh_interval`, `:26-27, 52`). 기다리는 동안 "다른 프로세스가 이 세션을 쓰는 중" 상태 문구를 낸다(`:336-344`).
3. 기다렸다가 잡았으면 **대화 기록을 DB에서 다시 읽는다**(앞 사람이 압축했거나 글을 더했을 수 있다). 바로 잡았으면 다시 읽지 않는다(캐시 보호) (`:378-401`).
4. 못 잡았거나(시간 초과) 기다리다 interrupt가 오면 턴을 돌지 않고 `api_calls: 0, completed: false` 결과를 돌려준다. 이때 받은 입력을 잃지 않도록, 다음 턴이 저장할 수 있게 표시를 붙여 기록 끝에 실어 준다(`/stop` 같은 강제 중단이면 버린다) (`:411-449`).
5. 갱신에 실패해 lease를 잃으면 그 턴에 interrupt를 건다(이유 `session turn lease lost`, `:24`).

**나. 턴 본체** (`H:agent/conversation_loop.py:1647-1806`)

1. 턴 준비(`build_turn_context`): 시스템 프롬프트를 되살리거나 만든다(2절), user 메시지를 기록에 붙인다, 필요하면 호출 전 압축을 돈다(3절). 플러그인 `pre_llm_call`이 막으면 모델을 부르지 않고 막는 문구를 답으로 끝낸다(`:1618-1626`).
2. 반복 조건: `(api_call_count < max_iterations 이고 iteration_budget이 남음) 또는 grace 호출 표시` (`:1752-1754`). `max_iterations` 기본 500은 기존 문서 3.1.
3. 반복 한 번의 순서:
   1. `begin_iteration` (`H:agent/turn_iteration_prep.py:352-417`): 대기 중 redirect(턴 도중 사용자 정정) 적용 → **interrupt 확인**(걸려 있으면 `interrupted_by_user` 또는 `interrupted_by_system(<누가>)`로 끝) → 검토 입력 예산 확인 → `api_call_count += 1` → 예산 1 소비(못 하면 `budget_exhausted`로 끝. grace 표시가 있으면 예산 대신 표시를 쓴다).
   2. `prepare_iteration` (`:144-258`): 대기 중 `/steer` 글을 **가장 최근 tool 결과 바로 뒤의 새 user 행**으로 끼운다(tool 행에 붙이지 않는다 — 이미 저장된 행이라 재생 때 달라진다. tool 결과가 없으면 도로 대기열에) → 실행 시간 예산의 80%에서 한 번, 반복 예산 경고를 한 번(선택) **가장 최근 tool 결과 글 끝에 덧붙인다**(가짜 user/system 행을 만들지 않는다) → 깨진 tool_call 인자 고치기 → role 교대 고치기(`tool→user`, `user→user` 꼬리 합치기).
   3. 요청 조립(`assemble_api_request`) → 호출 전 관문(`run_preflight_gate`: 압축 등. 3절).
   4. **API 호출 재시도 고리** (`:1578-1607`): `retry_count < max_retries` 동안 `요청 만들기 → 호출 → 응답 확인`. 예외는 분류기(4절)로. `max_retries`는 `agent.api_max_retries`(기본 3 = 시도 총 횟수).
   5. `apply_retry_restarts` (`turn_iteration_prep.py:437-562`): 고리가 남긴 "다시 시작" 표시를 우선순위대로 처리한다 — ① redirect로 다시(횟수·예산 환불, `max_retries`번 넘으면 `redirect_restart_limit_exceeded`로 끝) ② interrupt면 끝 ③ 압축하고 다시(환불하되 `retry_count += 1`) ④ 폴백 모델로 다시(환불, `max_retries`번 넘으면 `rebuilt_restart_limit_exceeded`) ⑤ 잘린 응답 이어 받기 ⑥ 응답이 끝내 없으면 `all_retries_exhausted_no_response`로 끝.
   6. 응답 정규화 → **tool_calls가 있으면 도구 라운드, 없으면 최종 글 처리**.
4. 고리를 벗어나면 `finalize_turn`이 결과 dict를 만든다(기존 문서 3.1).

**다. 도구 라운드** (`H:agent/turn_tool_round.py:56-231`, `H:agent/turn_tool_validation.py:101-267`)

1. 검증:
   - **모르는 도구 이름만 있는 경우**: assistant 메시지를 붙이고, 호출마다 "그런 도구 없음" tool 결과를 붙여 모델이 고치게 한다. 연속 3번이면 부분 결과로 턴을 끝낸다(`:161-186`).
   - **섞인 경우**(아는 것 + 모르는 것): 모르는 호출에만 오류 결과를 주고 나머지는 실행한다. 3번 세기를 0으로 되돌린다(`:151-160`, `turn_tool_round.py:104-128`).
   - **인자가 JSON이 아닌 경우**: 빈 문자열은 `{}`로 본다. 인자가 `}`나 `]`로 끝나지 않으면 **잘린 것**으로 보고 실행하지 않고 부분 결과로 끝낸다. 그 밖에는 메시지를 더하지 않고 같은 호출을 2번까지 다시 하고, 3번째에는 assistant 메시지 + 호출마다 "JSON 오류" tool 결과를 붙여 계속한다(`:190-265`).
2. 가드: `delegate_task` 호출 수 자르기, 같은 호출 중복 빼기(`turn_tool_round.py:97-99`).
3. assistant(tool_calls) 메시지를 기록에 붙이고 **도구를 돌리기 전에 DB에 쓴다**("persist-before-execute". 도구가 프로세스를 다시 띄워도 재개 때 그 블록이 보이게). 이 쓰기가 실패하면 도구를 돌리지 않고 `session_persistence_failed`로 턴을 끝낸다(`:130-153`).
4. 도구 실행(병렬 숫자는 기존 문서 3.2). 결과 쓰기가 실패해도 같은 이유로 끝낸다(`:176-182`).
5. 도구 가드레일이 멈추라고 하면 설명 글을 답으로 붙이고 `guardrail_halt`로 끝(`:184-198`).
6. 도구가 `execute_code` 하나뿐이면 예산 1을 돌려준다(`:205-208`). 그다음 "도구 결과 뒤 압축" 검사(3절)를 하고 다음 반복으로.

**라. 최종 글 처리** (`H:agent/turn_final_response.py`, `turn_empty_response.py`, `turn_truncation.py`, `turn_stop_gates.py`)

- 정상: 글이 있으면 stop gate를 거친다 — 코드 수정 뒤 검증 훅, 칸반 워커는 끝내는 보드 도구(`kanban_complete` 등)를 부르기 전에는 못 끝냄(`turn_stop_gates.py:54-95, 139`). 통과하면 assistant 메시지를 붙이고 DB에 쓰고 `text_response(finish_reason=…)`로 끝(`turn_final_response.py:406-417`).
- **빈 응답 사다리** (`turn_empty_response.py:277-405`), 위에서부터:
  1. 스트림이 끊기기 전에 보인 글이 있으면 그것을 답으로(`partial_stream_recovery`).
  2. 기계 알림 턴(백그라운드 완료 알림 등)의 빈 답이면 조용히 끝(`notification_no_reply`).
  3. 바로 앞 반복이 "글 + 살림 도구(memory · todo · skill_manage · session_search)만"이었으면 그 글을 답으로(`fallback_prior_turn_content`).
  4. 최근 5개 안에 tool 결과가 있으면 "계속하라"는 힌트를 **한 번** 붙여 다시 호출.
  5. 추론만 있고 글이 없으면 "마무리하라"는 힌트로 2번까지.
  6. 진짜 빈 응답이면 최대 3번 다시(`DEFAULT_EMPTY_RETRY_BUDGET = 3`, `H:agent/empty_response_guard.py:54`. 비싼 요청은 줄어듦), 대기는 5초에서 시작해 60초 상한의 jitter backoff. 같은 빈 응답이 되풀이되면 남은 횟수를 버린다.
  7. 폴백 모델이 있으면 넘어가서 다시(호출 수 환불).
  8. 다 쓰면 `(empty)`라는 표식 글을 assistant 행으로 저장하고 `empty_response_exhausted`로 끝.
- **잘린 응답**(`finish_reason == "length"`): 글이면 조각을 붙이고 "이어서 써라"를 넣어 3번까지 이어 받고(4번째에 그만), 그때마다 출력 상한을 `기본 × 2ⁿ`으로 올린다. 조각들은 겹치는 부분(32자 이상)을 맞춰 하나로 잇는다. 도구 호출이 잘렸으면 같은 호출을 상한을 올려 4번까지 다시 한다(`turn_truncation.py:314-345, 391-415`, `conversation_loop.py:265-296`).
- 응답 처리 중 예외가 밖으로 새면 `min(8, max_iterations)`번까지 봐주고 끝낸다(`H:agent/turn_loop_errors.py:149`).

```mermaid
stateDiagram-v2
  [*] --> LeaseWait
  LeaseWait --> Ended: 1800초 초과 · 기다리다 interrupt
  LeaseWait --> Setup: lease 얻음(기다렸으면 기록 다시 읽기)
  Setup --> Ended: pre_llm_call이 막음 · 호출 전 압축 시간 초과
  Setup --> IterStart
  IterStart --> Ended: interrupt · 예산 없음
  IterStart --> Prepare: 호출 수 +1, 예산 -1
  Prepare --> ApiCall: steer 끼우기 · 고치기 · 조립 · 관문
  ApiCall --> ApiCall: 재시도(4절)
  ApiCall --> IterStart: 압축/폴백/redirect/이어받기로 다시(환불)
  ApiCall --> Ended: 재시도 소진 · interrupt · 분류기가 끝냄
  ApiCall --> ToolRound: tool_calls 있음
  ApiCall --> FinalText: tool_calls 없음
  ToolRound --> IterStart: 결과 붙이고 저장
  ToolRound --> Ended: 이름 오류 3번 · 잘린 인자 · 저장 실패 · 가드레일
  FinalText --> IterStart: 빈 응답 다시 · stop gate가 더 시킴 · 잘림 이어받기
  FinalText --> Ended: 답 확정
  Ended --> [*]: finalize_turn (lease 놓기)
```

### 상태와 저장되는 것 [관찰]

- 저장 시점은 세 곳이다: ① 도구를 돌리기 **전**(assistant tool_calls 행) ② 도구 결과가 나올 때마다 ③ 턴이 끝날 때(최종 답). 턴 도중 실패로 나갈 때도 거의 모든 길에서 `_persist_session`을 부른다(위 근거 줄들). 그래서 DB에는 "tool_calls는 있는데 결과가 아직 없는" 순간이 있을 수 있다.
- 턴 사이에 남는 것: 세션의 메시지 행(5절), 시스템 프롬프트 원문과 그때의 도구 목록(2절), 토큰 사용량, 압축 상태. 에이전트 객체는 게이트웨이가 턴 사이에 캐시하지만 **턴마다 초기화하는 것**이 따로 적혀 있다 — 중간 글 중복 표시, 저장 실패 표시, 자격 갱신 횟수, 마지막 사용량 등(`conversation_loop.py:1726-1739`).
- 한 턴 안의 세기 값: `_invalid_tool_retries`(3), `_invalid_json_retries`(3), `_empty_content_retries`(3), `_thinking_prefill_retries`(2), `length_continue_retries`(4), `truncated_tool_call_retries`(4), `restart_count`(`max_retries`), `compression_attempts`(3).

### 실패했을 때 · 끊겼을 때 [관찰]

| 상황 | 일어나는 일 | 다음 턴 |
|---|---|---|
| **interrupt**(사용자 취소 · 새 메시지) — 반복 사이 | `begin_iteration`에서 끝. `interrupted: true` | 기록은 저장된 데까지. 그대로 이어 간다 |
| interrupt — 모델 응답을 기다리는 중 | 호출을 끊는다. 받은 조각이 있으면 부분 글로 남긴다 [추정: `handle_api_interrupt` 본문은 읽지 않았다] | 같음 |
| interrupt — 도구 실행 중 | 도는 도구는 취소 결과 `[Tool execution cancelled — <이름> was abandoned: …]`, 아직 안 돈 도구는 `… was skipped …` 결과를 **호출마다 하나씩** 붙인다(짝을 깨지 않으려고) | 짝이 맞는 기록이 남는다 (`H:agent/tool_executor.py:977, 1606`) |
| **무활동 시간 초과** | 감시가 15초마다 활동 시각을 본다. 600초(`agent.turn_liveness.timeout_s`) 동안 변화가 없으면 턴을 강제로 끊고 lease 갱신을 멈춰 만료되게 둔다 | lease가 300초 안에 풀린다 (`H:agent/turn_liveness.py:23-28, 143`) |
| **프로세스 종료** | lease는 갱신이 끊겨 최대 300초 뒤 만료. DB에는 마지막으로 쓴 행까지 남는다 | 다음 턴이 lease를 잡고 기록을 읽을 때 role 교대 고치기(`repair_alternation`)와 짝 고치기를 한다(5절). 돌던 턴을 **자동으로 다시 돌리지는 않는다** [추정: api_server run은 메모리의 큐라 사라진다. hosted room만 드라이버 작업 표로 되살린다(7.5)] |
| DB 쓰기 실패 | 도구를 돌리지 않고 턴을 실패로 끝낸다 | 메모리에만 있는 상태로 진행하지 않는다 |

### 근거

위에 줄마다 적었다. 모두 Hermes(`H:agent/…`)다.

### 가격인하에 옮길 때 (T9 · A4)

| 원본 | 판단 | 까닭 |
|---|---|---|
| 반복 고리의 순서(interrupt 확인 → 예산 → 준비 → 호출 → 도구/최종) | **그대로** | 담당 6명 모두 같은 고리를 쓴다 |
| persist-before-execute, 저장 실패면 도구를 안 돌림 | **그대로** | 주문 도구가 여기에 걸린다. "기록 없이 주문"을 막는 바탕(E20 · E33) |
| interrupt 때 호출마다 취소 결과를 채워 짝을 맞춤 | **그대로** | 안 하면 다음 호출이 400으로 죽는다 |
| 이름 오류 3 · JSON 오류 3 · 빈 응답 3 · 잘림 이어받기 4 · 바깥 오류 8 | **그대로** | 숫자까지 옮긴다 |
| 세션 턴 lease(300/60/1800초, 프로세스 사이) | **줄여서** | Rust 한 프로세스(D1)라 세션별 뮤텍스면 된다. 다만 "기다렸으면 기록을 다시 읽는다"는 규칙은 남긴다 |
| `max_iterations` 500 | **줄여서** | 매매 판단 턴은 짧다. 담당별 상한은 사용자가 정한다 |
| `/steer`, redirect, grace 호출, 실행 시간 예산 경고 | **줄여서** | steer만 가져온다(사용자가 사무실 채팅으로 도는 담당에게 말할 때). 넣는 자리 규칙(최근 tool 결과 뒤 user 행)은 그대로 |
| 칸반 stop gate, 코드 검증 훅, Codex · Ollama · MoA 갈래 | **안 가져옴** | 쓰지 않는 기능 |
| 무활동 감시 600초 / 15초 | **그대로**(숫자는 조정) | 담당이 멈추면 E6으로 보내야 한다 |


## 2. 시스템 프롬프트 조립

기존 문서 `hermes-agent.md` 3.3의 세 층 표(stable / context / volatile에 무엇이 들어가는지)는 맞다. 다시 쓰지 않고, 그 표에 없는 **정확한 순서 · 길이 제한 · 다시 조립하는 조건 · 캐시 표식**만 적는다.

### 동작 원리 [관찰]

**가. 조각 순서** (`H:agent/system_prompt.py:751-826`). 조각 사이, 층 사이 모두 빈 줄 하나(`"\n\n"`)로 잇고, 비어 있는 조각은 뺀다(`:746-748, 826`).

| 층 | 순서 | 조각 | 출처 · 조건 |
|---|---|---|---|
| stable | 1 | 정체성 | 그 에이전트 홈의 `SOUL.md` 전문. 없거나 비면 기본 정체성 문장 (`:563-569`, `H:agent/prompt_builder.py:1520-1545`) |
| | 2 | Hermes 도움말 안내 | 스킬 도구가 있고 색인에 `hermes-agent` 스킬이 있으면 스킬을 가리키는 판, 아니면 짧은 판 (`:767-774`) |
| | 3 | 일반 지침 | 도구가 하나라도 있을 때만: 일 마무리 지침, 병렬 도구 호출 지침, 도구별 안내 묶음, steer 통로 안내(한 번짜리 실행은 뺌), 모델 이름에 따라 도구 사용 강제 지침 · 실행 지침 (`:572-600`) |
| | 4 | 고정 스킬 | `skills.auto_load`에 적은 스킬의 본문. 에이전트 수명 동안 **한 번만** 읽는다 (`:323-347`) |
| | 5 | 코딩 지침 앞부분 | 도구가 있을 때 |
| context | 6 | 호출자 `system_message` | 호출 인자 (`:788-789`) |
| | 7 | 프로젝트 컨텍스트 파일 | 작업 폴더의 AGENTS.md 등. `skip_context_files`면 없음 (`:730-743`) |
| | 8 | 워크스페이스 스냅샷 → 코딩 지침 뒷부분 → 환경 점검 한 줄 → **Bot Mode 규약**(제목이 `Bot Chat`인 세션에서만) → 플랫폼 힌트 | 워크스페이스가 없으면 이 묶음은 stable 끝으로 간다 (`:713-727, 791-796`) |
| volatile | 9 | 스킬 색인 | 스킬 도구(`skills_list` · `skill_view` · `skill_manage`)가 하나도 없으면 빈칸 (`:307-320`) |
| | 10 | 메모리(MEMORY.md) → 사용자 프로필(USER.md) → 외부 메모리 공급자 블록 | 6절 (`:535-560`) |
| | 11 | 플러그인 섹션(`after_memory`) | 세션 시작 때 한 번 그려 얼린다 |
| | 12 | 활성 프로필 줄 | 프로필 홈 경로를 이름으로 알려 줌. 경로가 들어가서 volatile에 둔다 (`:804-806`) |
| | 13 | 시각 줄 | 아래 "다" |
| | 14 | 런타임 환경 힌트 | 머리 · 끝 표식으로 감싼다 (`:810-813`) |

**나. 길이 제한** (`H:agent/prompt_builder.py:1040-1061, 1470-1501`)

- 컨텍스트 파일(SOUL.md 포함, `load_soul_md`가 같은 자르기를 쓴다) 상한: 설정 `context_file_max_chars`가 있으면 그 값. 없으면 `모델 창(토큰) × 글자/토큰 × 0.06`을 **2만 자 ~ 50만 자**로 자른 값. 창을 모르면 2만 자.
- 넘으면 **앞 70% + 표식 + 뒤 20%**만 남긴다. 표식에는 원래 길이, 빠진 구간의 Markdown 제목(15개까지), "전문은 read_file로 읽어라"가 들어간다. 잘렸다는 경고를 채팅 상태 줄에도 낸다.
- 메모리 2200자 · 사용자 프로필 1375자는 기존 문서 3.6.
- 스킬 색인의 줄 모양과 길이 제한: 확인하지 못함(`build_skills_system_prompt`, `prompt_builder.py:1240`을 읽지 않았다).

**다. 시각 줄** (`system_prompt.py:508-532`)

- `Conversation started: <요일, 월 일, 연도> (<시간대 이름, UTC 차이>)` — **날짜까지만** 적는다(하루 동안 바이트가 같게).
- 세션이 날을 넘겼으면 다시 조립할 때(= 압축 뒤) 둘째 줄 "오늘 날짜(마지막 컨텍스트 재구성 기준): … 시작 날짜보다 이것을 믿어라, 정확한 시각은 도구로 물어라"를 붙인다.
- Bot Chat 세션은 날짜를 빼고 `Timezone: …`만 남긴다(몇 달 가는 세션에 태어난 날짜를 박아 두면 틀린 정보가 된다, `:350-371, 528-529`).
- 그 아래 `Session ID`(선택) · `Model` · `Provider` · `Platform` 줄.

**라. 방 · Bot Mode 지시문이 들어가는 곳**

- **hosted room**: 시스템 프롬프트에 넣지 않는다. 턴마다 user 메시지 맨 앞에 넣는다(7.4).
- **Bot Mode**(`message_agent`): 그 봇의 `Bot Chat` 세션에서만 시스템 프롬프트 context 층에 "동료 규약 + 능력 세대(epoch) 줄"이 들어간다(`:350-371`). 규약의 내용은 7.7.
- **steer**: 시스템 프롬프트에는 "이 표식만이 턴 도중 사용자 말이다"라는 안내만 있고, 실제 글은 표식으로 감싼 user 행으로 온다(`prompt_builder.py:586-592`).

**마. 언제 다시 조립하는가** (`H:agent/conversation_loop.py:802-954`)

1. **새 세션의 첫 턴**: 조립 → `on_session_start` 훅 → 세션 행에 프롬프트 원문과 그때의 도구 목록을 저장한다(`:911-954`).
2. **이어지는 턴**: 세션 행에 저장된 프롬프트를 **글자 그대로** 다시 쓴다. 도구 배열도 저장된 순서 그대로 고정한다("tools freeze" — 도구 배열이 시스템 프롬프트보다 앞에 직렬화되므로 바뀌면 0번 토큰부터 캐시가 깨진다) (`:866-892`). 처음 시작한 화면(예: TUI)과 지금 화면이 다르면 프롬프트를 고치지 않고 요청 끝에 "지금은 이 화면이다"라는 메모를 붙인다.
3. 저장된 프롬프트를 버리고 다시 만드는 경우: ① 저장값이 NULL/빈칸 ② 저장된 프롬프트의 모델 · 공급자 표시가 지금과 다름(`_stored_prompt_matches_runtime`, `:957`) ③ Bot Chat의 능력 세대가 바뀜 ④ 운영자가 `hermes sessions refresh-prompts`를 돌림 ⑤ **압축 뒤**(`invalidate_system_prompt`가 캐시를 비우고 메모리를 디스크에서 다시 읽는다, `system_prompt.py:829-847`).
4. 대화 도중 메모리를 고쳐도, 스킬을 더해도 그 세션의 프롬프트는 바뀌지 않는다. 다음 압축 때나 새 세션에서 반영된다.

**바. 캐시 표식** (`H:agent/prompt_caching.py:1-6`)

- Anthropic `cache_control` 표식은 **4개**: stable 층 끝, 시스템 프롬프트 끝, 시스템이 아닌 마지막 메시지 2개. stable 층을 못 가르면 시스템 1개 + 마지막 메시지 3개. TTL은 전부 같은 값(5분 또는 1시간).
- stable 층 원문은 저장하지 않는다(전체만 저장). 재개 때 stable 층을 다시 만들어 저장된 전체가 **그것으로 시작할 때만** 앞쪽 표식을 쓴다. 아니면 한 덩어리로 보낸다(`system_prompt.py:850-876`).

### 상태와 저장되는 것

- `sessions.system_prompt`(실제로는 `system_prompts(hash, prompt)` 표를 해시로 가리킨다 — 기존 문서 7.1), 고정된 도구 목록(`tool_names`), 프롬프트 갱신 세대.
- 에이전트 객체 안: `_cached_system_prompt`, `_cached_system_prompt_static`, 얼린 플러그인 섹션, 고정 스킬 결과, 얼린 워크스페이스 스냅샷.

### 실패했을 때

- 세션 행을 못 읽으면 새로 조립하고 경고를 남긴다(그 턴은 캐시가 빗나간다) (`conversation_loop.py:818-823`).
- 프롬프트 저장이 실패하면 턴은 계속하고, 다음 턴이 또 조립한다(`:948-953`).
- 환경 점검 · 워크스페이스 탐색 · 플러그인 섹션이 실패해도 조립을 막지 않는다(그 조각만 빠진다).

### 가격인하에 옮길 때 (T8 · T9)

| 원본 | 판단 | 까닭 |
|---|---|---|
| 세 층 순서(바뀌지 않는 것 → 프로젝트 → 자주 바뀌는 것), 날짜는 일 단위 | **그대로** | 담당 6명이 하루 수십 턴을 돈다. 캐시가 비용을 좌우한다 |
| 세션 행에 프롬프트 원문 저장 → 이어지는 턴은 글자 그대로 재사용, 도구 배열 고정 | **그대로** | 같은 까닭. 우리 도구 목록은 담당별로 고정이라 더 쉽다 |
| 다시 조립은 압축 뒤 · 모델 바뀜 · 저장값 없음일 때만 | **그대로** | |
| 캐시 표식 4개 | **그대로** | Anthropic 어댑터(A11)에 넣는다 |
| 조각 1(SOUL.md) = 담당 역할 템플릿, 조각 4(고정 스킬) = 투자 규칙 템플릿 | **그대로** | 기존 문서 3.3의 매핑을 확정한다. 템플릿을 바꾸면(C4 · C14) 그 담당의 세션을 새로 열어야 반영된다는 점을 T8에 적어야 한다 |
| 파일 자르기(앞 70% · 뒤 20% · 표식) | **줄여서** | 우리 템플릿은 우리가 쓰니 상한을 넘으면 등록 때 거절하면 된다 |
| 코딩 지침 · 워크스페이스 스냅샷 · 환경 점검 · 플랫폼 힌트 · 화면 전환 메모 · 플러그인 섹션 | **안 가져옴** | 코딩 에이전트용 |
| 방 지시문은 시스템 프롬프트가 아니라 user 메시지에 | **그대로** | 7.4 |


## 3. 컨텍스트 압축

기존 문서 `hermes-agent.md` 3.4(두 겹, 다섯 단계 개요, 설정 기본값)는 다시 쓰지 않는다. `conformance.md`가 "같은 동작으로 만들 수 없다"고 적은 **문턱값 식 · 경계 고르기 · 요약 요청 구조 · 끼우는 규칙 · 실패 처리**를 채운다. 본체는 `H:agent/context_compressor.py`(5679줄)이고, 아래는 그 가운데 읽은 구간에서 나온 것이다(읽지 못한 구간은 끝에 적는다).

### 동작 원리 [관찰]

**가. 언제 시작하는가**

1. 문턱값(토큰) 계산(`_compute_threshold_tokens`, `:2765-2797`):
   - 입력 예산 = `컨텍스트 창 − 출력 예약(max_tokens)`. 예약이 없거나 빼면 0 이하가 되면 창 전체.
   - 기본 = `입력 예산 × threshold_percent`(설정 기본 0.50).
   - **창이 512K 미만인 모델은 비율을 0.75 이상으로 올린다**(`_SMALL_CTX_WINDOW_LIMIT = 512_000`, `_SMALL_CTX_THRESHOLD_PERCENT = 0.75`, `:1150-1151, 2751-2755`). 즉 200K 창 모델은 설정이 0.50이어도 실제로는 75%에서 시작한다.
   - 바닥 64,000 토큰(`MINIMUM_CONTEXT_LENGTH`, `H:agent/model_metadata.py:279`). 바닥이 걸려서 입력 예산의 85%(`_MIN_CTX_TRIGGER_RATIO`, `:2714`)를 넘으면 85%로 낮춘다. 문턱이 예산 이상이면 `min(85%, 예산 − 1)`.
2. 판단(`should_compress_info`, `:2994-3003`): `프롬프트 토큰 ≥ 문턱`이고 막혀 있지 않으면 시작. 토큰 수는 **공급자가 준 실제 usage의 prompt 토큰**(`last_prompt_tokens`)이 기본이고, 호출 전 관문에서는 어림값(글자 수 기반)을 쓴다.
3. 검사하는 자리 [관찰 + 일부 추정]: ① 턴 시작 준비 때(호출 전), ② 매 호출 전 관문(`run_preflight_gate`), ③ 도구 결과를 붙인 직후(`compress_after_tool_results`, `H:agent/turn_tool_round.py:210-226`), ④ 공급자가 컨텍스트 초과 오류를 준 뒤(이때는 쿨다운을 무시하고 돈다 — `bypass_cooldown`, `:5391-5393`). ①②의 세부 조건은 `turn_context_compaction.py`를 읽지 않아 확인하지 못함.
4. 막는 조건(`_automatic_compression_blocked_locally`, `:3043-3110`):
   - 요약 실패 쿨다운이 남음 / "구조상 할 것이 없음" 물러서기 300초가 남음.
   - **반-요동 차단기**: 압축한 뒤 공급자가 준 실제 프롬프트 토큰이 여전히 문턱 이상이면 "효과 없음" 1점. 2점이면 자동 압축을 멈춘다(`_tripped`, `:3015-3022, 2923-2935`). 기준은 "메시지가 줄었나"가 아니라 "문턱 아래로 내려갔나"다(시스템 프롬프트 + 도구 스키마가 문턱보다 크면 아무리 줄여도 소용없기 때문).
   - 멈춘 채 300초(`_ANTI_THRASH_RECOVERY_SECONDS`)가 지나면 점수를 1로 낮춰 **한 번** 시도를 허락한다. 마감 시각은 벽시계로 세션 행에 저장한다(에이전트 객체가 새로 만들어져도 같은 창을 이어 가게).
   - 실제 토큰이 문턱 아래로 한 번이라도 찍히면 점수 0.

**나. 순서** (`compress`, `:5376-5491`)

1. 메시지 수가 `보호 머리 + 3 + 1` 이하면 아무것도 안 한다("할 것 없음" → 300초 물러서기).
2. **도구 결과 가지치기**(모델 호출 없음, `_prune_old_tool_results`, `:3274-3312`). 복사본에서 한다(원본은 손대지 않는다 — 중단 · 실패 때 원본을 돌려주려고).
   - 같은 내용의 도구 결과 중복 제거.
   - 꼬리 보호 경계보다 앞에 있는, 200자(`_PRUNE_MIN_CHARS`) 넘는 도구 결과를 **도구별 한 줄 요약**으로 바꾼다(terminal은 명령 · 줄 수, write_file은 경로 · 크기 등 도구마다 요약기가 있다, `:1653-1975`). 요약기가 없으면 `[Old tool output cleared to save context space]`(`:837`).
   - **도구 호출의 인자는 절대 고치지 않는다**(실행 기록이라서).
   - 아직 모델이 읽지 않은 "대기 중 도구 라운드"는 줄이지 않는다(줄이면 모델이 같은 호출을 다시 한다). 그것 하나가 꼬리 몫을 넘을 때만 예외.
   - 방금 불러온 스킬(`skill_view` 결과)은 보호한다. 꼬리 안의 오래된 이미지는 최신 3장만 남기고 뺀다(`_MAX_KEEP_TOOL_IMAGES`).
3. **경계 고르기**(`H:agent/context_compressor_window.py:33-87`):
   - 머리 = 시스템 메시지(있으면 1) + `protect_first_n`(기본 3). 단 **한 번이라도 압축했거나 앞 요약이 있으면 `protect_first_n`은 0으로 본다**(초반 턴이 화석처럼 남지 않게). 위임 자식은 첫 user 턴 1개를 끝까지 지킨다.
   - 시작 경계가 tool 결과 위에 떨어지면 **뒤로**(다음 메시지로) 민다(`_align_boundary_forward`, `:4547-4551`).
   - 꼬리 = 뒤에서부터 토큰 예산만큼. 예산(기본 lean 모드) = `창의 2.5%`를 **1만 ~ 2만5천 토큰**으로 자르고, 문턱의 50%를 넘지 않게, 그리고 창의 20%를 넘지 않게(`H:agent/context_compressor_tail.py:23-40`, `context_compressor.py:924-929, 2219-2229`).
   - 끝 경계가 tool 묶음 중간에 떨어지면 **앞으로**(그 묶음의 부모 assistant 앞까지) 당긴다 — 묶음이 통째로 요약에 들어가게(`_align_boundary_backward`, `:4553-4561`).
   - 마지막 실제 user 메시지는 반드시 꼬리에 남긴다(`_ensure_last_user_message_in_tail`). 요약이 꼬리 첫 행에 합쳐질 수 있으므로, 그 자리에 user의 새 요청이 오지 않게 앞의 assistant/tool 다리 하나를 꼬리에 더 남긴다(`window.py:63-77`).
   - 시작 ≥ 끝이면 "할 것 없음".
4. 꼬리 안의 오래된 도구 결과 강등(lean 모드): 꼬리에서 최근 6개 도구 라운드만 전문으로 두고, 그 앞의 1,500자 넘는 결과는 "…강등됨 — N자는 세션 기록에 보존" 꼴 한 줄로 바꾼다(`tail.py:11-17`).
5. 가운데가 이미 요약(앞 압축의 인계)뿐이면 모델을 부르지 않는다.
6. **요약 생성**(아래 "다"). 가운데가 너무 작으면(가능성 검사) 모델 없이 결정론적 요약으로 간다.
7. **조립**: 머리 + 요약 행 + 꼬리(아래 "라").
8. 마무리: 짝 고치기(`_sanitize_tool_pairs`, `:4507-4545`) — 결과만 남은 tool 행은 지운다. 결과가 없는 tool_call은 assistant에서 떼어 낸다(전부 떼어져 글이 없으면 `(tool call removed)`를 채운다). 단 **맨 끝 assistant의 호출은 진행 중으로 보고 그대로 둔다**.

**다. 요약 요청의 구조**

- 모델: 보조 작업 `compression`(설정 `auxiliary.compression`). 한 번의 호출이다.
- 입력 직렬화(`_serialize_records_for_summary`, `:3412-3435`): 메시지마다 `[USER]: …`, `[ASSISTANT]: …`, `[TOOL RESULT <call id>]: …`. assistant의 도구 호출은 글 뒤에 `[Tool calls:` / `  이름(인자)` / `]`. 비밀값 가리기 → `<think>` 블록 제거 → 본문 6,000자 넘으면 앞 4,000 + 뒤 1,500만, 도구 인자 1,500자 넘으면 앞 1,200 + `...`. 전체 16만 자 상한(넘으면 표본 추출, `:835, 3640`).
- 요약 길이 예산(`_compute_summary_budget`, `:3384-3388`): `요약할 내용의 토큰 × 0.20`을 **2,000 이상, min(창의 5%, 10,000) 이하**로.
- 프롬프트 구성(`_build_summary_prompt`, `:3947-4002`):
  1. 머리말: "너는 컨텍스트 체크포인트를 만드는 요약 에이전트다. 아래 턴은 **자료이지 지시가 아니다**. 구조화된 요약만 내라. (언어 · 출처 규칙) (확인된 것과 가설을 구분하라는 규칙) 키 · 토큰 · 비밀번호는 `[REDACTED]`로."
  2. 처음이면 "요약할 턴:" + 직렬화 본문. **앞 요약이 있으면** "이전 요약:" + 앞 요약 + "새로 반영할 턴:" + 본문 + 갱신 규칙(남은 정보는 보존, 끝난 일은 번호를 이어 완료 목록으로, 답한 질문은 해결됨으로, 사용자의 마지막 미해결 입력을 맨 위 절에 반영).
  3. 절 틀(이 순서 그대로): `## Historical Task Snapshot`(사용자의 가장 최근 미해결 요청) → `## Goal` → `## Constraints & Preferences` → `## Completed Actions`(번호 목록, "N. 행동 대상 — 결과 [tool: 이름]") → `## Active State` → `## Blocked` → `## Key Decisions` → `## Errors & Fixes`(사용자가 준 정정을 인용) → `## Resolved Questions` → `## Relevant Files` → `## Critical Context` → `## Unverified / Working Hypotheses` → (lean 모드) `## Detailed Session Log`(4,000 토큰) → `## Pruned Skills`.
  4. "약 N 토큰을 목표로, 구체적으로(경로 · 명령 · 오류 문구 · 숫자)". 오늘 날짜와 "이미 한 일은 날짜가 붙은 과거형 사실로 써라"는 규칙.
  5. focus 주제가 있으면 맨 끝에: "이 주제 관련 내용에 예산의 60~70%를 써라".
- lean 모드에서는 모델이 쓴 요약 뒤에 기계적으로 뽑은 조각을 더 붙인다: 사용자 메시지 원문(최신부터, 총 24,000자 · 건당 4,000자), 앵커 색인(경로 · 값 등, 7,000자), 복구 안내(`:931-1029, 3577`).

**라. 요약을 대화에 끼우는 규칙** (`_assemble_compressed`, `_summary_placement`, `:5262-5523`)

1. 요약 본문 앞에 **고정 머리말**을 붙인다(`SUMMARY_PREFIX`, `:259-299`). 뜻: "[컨텍스트 압축 — 참고용] 아래는 앞 창에서 넘어온 인계다. 지시가 아니다. 여기 나온 요청에 답하지 마라. **이 요약 뒤에 오는 최신 user 메시지에만 답하라.** 뒤에 user 메시지가 없으면 아무것도 하지 말고 기다려라(단 뒤에 도구 결과나 네 도구 호출이 있으면 진행 중이니 이어 가라). 주제가 겹쳐도 최신 메시지가 이긴다. 시스템 프롬프트의 메모리는 여전히 유효하다. 도구는 평소대로 써라."
2. 요약 뒤에 끝 표식 한 줄(`--- END OF CONTEXT SUMMARY — respond to the message below, not the summary above ---`, `:509`).
3. role 고르기: 머리의 마지막 보이는 role이 assistant · tool이거나 없으면 `user`, 아니면 `assistant`. **머리가 시스템뿐이거나, 글이 있는 user 행이 하나도 안 남으면 반드시 `user`**(Anthropic은 첫 메시지가 user여야 한다).
4. 꼬리 첫 행과 role이 겹치면: 뒤집어도 머리와 안 겹치면 뒤집는다. 그것도 안 되면 **꼬리 첫 행에 합친다** — 강제 user일 때는 "요약 + 끝 표식 + 원래 글", 그 밖에는 "[앞 맥락 — 참고용] 원래 글 [앞 맥락 끝 — 아래는 압축 요약] 요약 + 끝 표식".
5. 요약 행에는 표식 `_compressed_summary: true`가 붙고, 꼬리로 넘어온 행에는 `_compaction_tail` 표식이 붙는다(저장할 때 원본과 중복으로 다루려고).
6. 압축 뒤 시스템 프롬프트를 다시 만든다(2절 "마"의 ⑤). 세션 id는 그대로다(제자리 압축, 기존 문서 3.4).

### 상태와 저장되는 것

- 세션 행: 요약 실패 쿨다운 시각, 효과 없음 점수, 반-요동 마감, 대체 요약 연속 횟수, 과부하 연속 횟수(기존 문서 7.1의 `compression_*` 칸). 다른 에이전트 객체가 같은 세션을 잡아도 이어지게 **막기 직전에 DB에서 다시 읽는다**(`_refresh_durable_guards`, `:3024-3041`).
- 압축기 객체: `compression_count`, `_previous_summary`(다음 압축 때 갱신 프롬프트에 쓴다), 문턱 · 꼬리 예산.
- DB에서 옛 메시지 행을 어떻게 표시하는지(`messages.compacted` · `active`)는 `hermes_state_compression.py`를 읽지 않아 확인하지 못함.

### 실패했을 때 [관찰]

| 상황 | 처리 | 근거 |
|---|---|---|
| 요약 호출이 **인증 · 과금 · 권한 오류** | 압축을 그만둔다. 메시지는 그대로 | `:758-765` |
| **네트워크 오류** | 그만둔다(잠깐일 수 있으니) | `:766-772` |
| **요약이 출력 상한에서 잘림** | 그만둔다(반쪽 요약은 맥락을 조용히 잃는다) | `:773-779` |
| **빈 요약** | 그만둔다 | `:780-786` |
| **공급자 과부하** | 그만둔다. 단 한 세션에서 연속 3번이면 그만두지 않고 결정론적 대체 요약으로 넘어간다 | `:787-812` |
| **시간 초과 · 잘림** | 쿨다운 사다리 60 → 300 → 900초(종류별로 따로 센다) | `:796-822` |
| 그 밖의 실패 | 기본 쿨다운 600초(`_SUMMARY_FAILURE_COOLDOWN_SECONDS`, `:1123`), 설정에 따라 그만두거나 대체 요약 | `:4080-4178`(본문은 읽지 않음) |
| **결정론적 대체 요약** | 모델 없이 만든다: 앞 요약 3,000자 + 턴마다 700자 줄임 + 경로 · 도구 앵커, 전체 8,000자 | `:1126-1128, 3502` |
| 줄여도 문턱 위 | 효과 없음 점수 → 2점이면 자동 압축 정지(가의 4) | |
| 한 턴 안에서 압축 다시 시도 | 최대 3번(`max_compression_attempts`). 넘으면 `context_compression_exhausted`로 턴을 끝내고 "새 세션에서 이어 가라"는 결과를 준다 | `H:agent/conversation_loop.py:1743, 1802-1805` |

### 근거

위에 줄마다 적었다. 모두 Hermes(`H:agent/context_compressor*.py`)다.

### 가격인하에 옮길 때 (T9 · T15)

| 원본 | 판단 | 까닭 |
|---|---|---|
| 문턱 식(입력 예산 × 비율, 512K 미만은 0.75, 바닥 64K, 85% 덮개) | **그대로** | Claude 200K 창이면 실제 문턱은 약 (200K − 출력 예약) × 0.75다. 0.50으로 만들면 원본보다 두 배 자주 압축한다 |
| 도구 결과 가지치기(인자는 안 고침, 대기 중 라운드는 안 건드림) | **그대로** | 시세 · 재무 자료 도구 결과가 크다. 요약기는 우리 도구(시세, 재무, 뉴스, 지식 검색)용으로 새로 쓴다 |
| 경계 규칙(시작은 뒤로, 끝은 앞으로, 마지막 user는 꼬리에, 압축 뒤 머리 보호 0) | **그대로** | 이것이 "도구 쌍을 깨지 않는 규칙"의 전부다 |
| 꼬리 예산(창의 2.5%, 1만~2만5천, 문턱의 50% · 창의 20% 이하) | **그대로** | |
| 요약 프롬프트의 절 틀 | **줄여서** | 코딩용 절(Relevant Files, Active State의 브랜치 · 테스트)은 빼고, `Historical Task Snapshot · Goal · Constraints · Completed Actions · Key Decisions · Errors & Fixes · Critical Context · Unverified`를 남긴다. 매매 담당용으로 "보유 · 주문 상태", "자료와 알게 된 시각" 절을 넣을지는 사용자가 정한다 |
| 머리말("참고용, 최신 user 메시지에만 답하라") + 끝 표식 + role 고르기 · 합치기 | **그대로** | 압축 뒤 옛 일을 다시 하는 사고를 막는 장치다. 문구는 우리 말로 새로 쓴다(원문을 옮기지 않는다) |
| 반-요동 차단(실제 토큰 기준 2점, 300초 뒤 한 번) · 쿨다운 사다리 · 연속 3번 과부하 뒤 대체 요약 | **그대로** | |
| 요약 전에 메모리로 흘려보내기 | 원본에 **없다**(6절). OpenClaw 방식(9절)을 T15에서 따로 정한다 | |
| lean 모드의 부록(사용자 메시지 원문 · 앵커 색인), 미리 요약해 두기(`prepare_ahead`), 스킬 표식, 이미지 처리 | **안 가져옴**(나중에) | 담당 세션은 방 논의와 예약 실행이 대부분이라 길어지는 속도가 느리다 |


## 4. 공급자 호출 실패 처리와 자격 증명 풀

기존 문서 `hermes-agent.md` 3.2의 재시도 표와 4.3, `conformance.md`의 바로잡음(첫 간격 2초 · 상한 60초, `api_max_retries`는 시도 총 횟수)은 다시 쓰지 않는다. 아래는 **분류기의 판단 기준 표 · 종류별 처리 순서 · 풀의 고르기와 복구**다.

### 4.1 분류기 [관찰] (`H:agent/error_classifier.py`)

**결과 객체** `ClassifiedError`(`:78-92`): `reason`(아래 종류), `status_code`, `provider`, `model`, `message`, `error_context` + 처리 힌트 네 개 — `retryable`(기본 참), `should_compress`, `should_rotate_credential`, `should_fallback`(기본 거짓).

**판단 순서**(먼저 답을 낸 단계가 이긴다, `:1109-1115`): ① 플러그인 훅 → ② 공급자 프로필 훅 → ③ 공급자별 특수 경우 → ④ **HTTP 상태 코드** → ⑤ MoA 모양 → ⑥ 본문의 구조화된 오류 코드 → ⑦ 메시지 문구 → ⑧ 전송 계층(SSL · 끊김 · 예외 타입) → ⑨ 아무것도 아니면 `unknown`(다시 시도 가능). 상태 코드가 없는데 SDK 예외 이름이 `RateLimitError`면 429, `…AuthenticationError`면 401로 본다(`:1131-1135`). 메시지는 소문자로 바꿔 부분 문자열로 찾는다.

**상태 코드별 판단** (`:1456-1464` + 각 처리 함수)

| 상태 | 판단 | 힌트 | 근거 |
|---|---|---|---|
| 400 | 아래 "400 꼬리 규칙" | | `:1363-1442` |
| 401 | `auth` | 다시 시도 안 함, 자격 바꾸기 + 폴백 | `:555, 1457` |
| 402 | 문구에 사용 한도 낱말(`usage limit, quota, limit exceeded, key limit exceeded`)**과** 일시 신호(`try again, retry, resets at, reset in, resets in, reset after, available in, wait, requests remaining, periodic, window, per minute, per second`)가 **둘 다** 있으면 `rate_limit`, 아니면 `billing` | billing = 다시 시도 안 함 + 자격 바꾸기 + 폴백 | `:197-201, 1297-1312` |
| 403 | 일시 코드면 `overloaded` → 과금 벽 문구면 `billing` → WAF/CDN 차단 문구면 `upstream_blocked`(키는 멀쩡, 폴백만) → 그 밖에 `auth`(폴백만, 자격은 안 바꿈) | | `:1215-1228` |
| 404 | 본문 코드가 과금이면 `billing` → 모델 없음 문구면 `model_not_found`(폴백) → 그 밖에 `unknown` | | `:1231-1242` |
| 408 | `timeout` | | `:1459` |
| 413 | `payload_too_large` | 압축 | `:1459, 567` |
| 422 | 이미지 · 도구 모양 규칙 | | `:1445` |
| 429 | ① 본문 코드가 과금이면 `billing` ② 과부하 문구(`overloaded, at capacity, over capacity` 등)면 `overloaded`(같은 키로 물러섰다 다시) ③ 중계 사업자가 감싼 상류 429면 `upstream_rate_limit`(폴백만) ④ 일시 신호가 없고 하루 · 주 단위 창 문구(`tokens per day, daily limit, daily quota, weekly usage limit, weekly limit, reached your session usage limit`)면 `billing` ⑤ 한도 벽 문구인데 `rate limit`류 문구도 일시 신호도 없으면 `billing` ⑥ 그 밖에 `rate_limit`(자격 바꾸기 + 폴백. 응답에 재개 시각이 있으면 `error_context.reset_at`에 싣는다) | | `:1245-1277` |
| 500 · 502 | 요청 검증 문구면 `format_error`(다시 해도 소용없음) → 초과 문구면 `context_overflow` → `server_error` | | `:1280-1286` |
| 503 · 529 | 초과 문구면 `context_overflow` → `overloaded` | | `:1462-1463` |
| 그 밖의 4xx | `format_error`(다시 시도 안 함, 폴백) | | `:1105` |
| 그 밖의 5xx | `server_error` | | `:1105` |

**400 꼬리 규칙**(위에서부터, `:1363-1442`): 이미지 크기 · 이미지 깨짐 · tool 결과의 목록형 내용 → 계정이 그 모델을 못 씀(`model_entitlement`) → 암호화된 추론 재생 블롭 거절 → 추론 끄기 거절(`reasoning_mandatory`: 그 값만 빼고 한 번 다시) → 서버가 끼워 넣은 파라미터 거절(일시 오류로 보고 그대로 다시) → **깨진 도구 인자**(`format_error`, 다시 안 함 · 폴백 안 함) → **같은 role 연속**(`role_alternation`) → 바이트 크기 상한(`payload_too_large`) → 요청 검증 문구(`format_error`) → 깨진 메시지 배열(`format_error` — 초과로 보면 압축 고리에 빠지므로 일부러 앞에 둔다) → 그 밖의 꼬리 규칙(컨텍스트 초과 문구 등) → **본문 메시지가 30자 미만이거나 그냥 "error"이고 세션이 크면**(토큰 > 창의 40%, 또는 창이 256K 이하일 때 토큰 > 8만 또는 메시지 > 80개) `context_overflow` → 남으면 `format_error`.

**문구 표** (전부 소문자 부분 일치)

| 종류 | 대표 문구 | 근거 |
|---|---|---|
| 과금 | `insufficient credits, insufficient_quota, insufficient balance, credit balance, credits exhausted, payment required, billing hard limit, exceeded your current quota, account is deactivated, plan does not include, out of extra usage, out of funds, insufficient funds, budget limit exceeded, hard billing limit` 등 | `:112-128` |
| 속도 제한 | `rate limit, rate_limit, too many requests, throttled, requests per minute, tokens per minute, requests per day, try again in, please retry after, resource exhausted, too many concurrent requests, throttling` | `:175-180` |
| 컨텍스트 초과 | `context length, context size, maximum context, token limit, too many tokens, reduce the length, exceeds the limit, context window, prompt is too long, max_tokens, maximum number of tokens, input is too long, max input token, maximum allowed input length` 등(중국어 2개 포함) | `:316-329` |
| 인증 | `invalid api key, invalid_api_key, authentication, unauthorized, error code: 401, forbidden, invalid token, token expired, token revoked, access denied` | `:454-460` |

**전송 계층**(`:1063-1097`): 인증서 검증 실패 → `ssl_cert_verification`(바로 실패). 일시적 SSL 경고 → `timeout`. **서버가 끊었는데 상태 코드가 없고 세션이 크면**(토큰 > 창의 60%, 또는 256K 이하 창에서 토큰 > 12만 또는 메시지 > 200) `context_overflow`, 단 추론 모델이면 `timeout`(긴 생각 중 유휴 끊김일 가능성이 높다). 조기 종료 문구 · 연결/시간 초과 예외 타입 → `timeout`.

**Anthropic 전용**(`:991-1001`): 400 생각 블록 서명 무효 → `thinking_signature`(재생한 생각 블록을 떼고 다시). 429 "extra usage … long context" → `long_context_tier`(창을 20만으로 낮추고 압축). 400 "long context beta … not yet available" → `oauth_long_context_beta_forbidden`(OAuth에서 1M 베타 헤더를 끄고 한 번 다시).

### 4.2 종류마다 무엇을 하는가 [관찰] (`H:agent/turn_api_error.py:53-236`, `H:agent/turn_recovery.py`)

오류 하나가 나면 아래 순서로 간다. 앞 단계가 "고쳤다"고 하면 **시도 횟수를 늘리지 않고** 바로 다시 호출한다.

1. 분류 전 복구(유니코드 인코딩 오류 등, `recover_before_classification`).
2. 프로세스가 내려가는 중이면 곧장 턴 실패(`interpreter_shutdown`).
3. **분류**(4.1).
4. 분류 뒤 한 번짜리 복구(`recover_after_classification`, `turn_recovery.py:654-812`), 순서대로: 출력 상한 줄이기(402 잔액 부족형) → **자격 풀로 바꾸기**(4.3) → 이미지 줄이기 → tool 결과의 이미지 떼기 → 추론 끄기 값 빼기 → 깨진 이미지 떼기 → OAuth 1M 베타 끄기 → **401 뒤 자격 갱신**(공급자별) → 형식 오류 고치기(생각 블록 떼기 · 같은 role 합치기 등).
5. 여기까지 못 고치면 `retry_count += 1`. interrupt가 걸려 있으면 그만둔다.
6. `route_classified_error`(`turn_recovery.py:1796-1969`), 순서대로:
   - 초과인데 압축이 꺼져 있으면 턴 실패.
   - `long_context_tier` → 창을 20만으로 낮추고 압축해서 다시.
   - **이른 폴백**: `rate_limit · upstream_rate_limit · billing`이면 **즉시**, `timeout · overloaded`이면 **2번째 실패부터** 폴백 사슬의 다음 모델로 넘어간다. 단 **자격 풀에 쓸 수 있는 항목이 남아 있고 항목이 2개 이상이면 폴백하지 않고 풀에 맡긴다**(`H:run_agent.py:190-197`). 상류 429는 풀과 상관없이 폴백. 요금제 사용 창 한도(402 usage window)는 자동으로 모델을 바꾸지 않는다.
   - 갱신해도 안 되는 401/403 → 폴백 사슬로 **한 번**.
   - 폴백으로 넘어가면 `retry_count`와 `compression_attempts`를 0으로 되돌리고, 시스템 프롬프트를 새 공급자에 맞춘다(`_fallback_break`).
7. 초과 처리(`recover_from_overflow`): 압축하고 다시(최대 3번, 3절).
8. 그래도 남으면: 다시 시도 못 하는 종류는 턴 실패, 할 수 있는 종류는 **물러섰다가 다시**.

**숫자**

| 항목 | 값 | 근거 |
|---|---|---|
| 물러서기 식 | `min(base × 2^(시도−1), max) + [0, 0.5 × delay] 균등 난수`. API 오류는 base 2초 · max 60초 | `H:agent/retry_utils.py:121-134`, `turn_recovery.py:1489` |
| Retry-After | 있으면 그 값을 쓴다(429뿐 아니라 다시 시도 가능한 모든 오류). 상한 600초. 60초를 넘으면 기다린다고 바로 알린다 | `turn_recovery.py:1482-1508`, `retry_utils.py:179-182` |
| 429일 때 | Retry-After가 없으면 적응형 물러서기(`adaptive_rate_limit_backoff`) | `turn_recovery.py:1491-1495` |
| 자동 복구 사다리 | 재시도와 폴백을 다 쓰고 **사용자에게 글이 하나도 안 나갔을 때만**, `timeout · overloaded · server_error`에 한해 `agent.auto_recovery_cycles`번 더: 15 → 30 → 60 → 60 → 60초(난수 20%). Retry-After는 120초까지 | `H:agent/turn_recovery_autorecover.py:23-62`, `error_classifier.py:72` |
| 빈 응답 재시도 | 5초 시작 · 60초 상한 · 3번 | 1절 |

**폴백에서 돌아오기**: 폴백은 **턴 단위**다. 다음 턴을 시작할 때 원래 모델로 되돌린다(`restore_primary_runtime`, `H:agent/agent_runtime_helpers.py:1390-1412`). 단 원래 모델이 "이 계정은 못 씀"으로 판정됐으면 폴백에 머문다. 429로 폴백했으면 재개 시각까지 원래 모델을 쉬게 하는 쿨다운이 따로 있다(`_arm_rate_limit_cooldown`, `H:agent/chat_completion_helpers.py:2036-2043`. 길이는 확인하지 못함).

**스트림 도중 끊김**: 사용자에게 이미 글이 나갔으면 자동 복구 사다리는 돌지 않는다(나간 글을 다시 내보내지 않으려고). 받은 글은 부분 답으로 남는다(1절 빈 응답 사다리의 1번).

### 4.3 자격 증명 풀 [관찰] (`H:agent/credential_pool.py`)

**항목의 칸**(`PooledCredential`, `:209-239`): `provider, id, label, auth_type(oauth|api_key), priority, source, access_token, refresh_token, last_status(ok|exhausted|dead), last_status_at, last_error_code, last_error_reason, last_error_message, last_error_reset_at, status_cleared_at, base_url, expires_at, expires_at_ms, last_refresh, request_count, model_cooldowns{모델: 풀리는 시각}, extra`. 저장은 `auth.json`의 풀 구간(값은 읽지 않았다. 코드의 `_persist`만 봤다).

**고르기**(`_available_entries` + `_select_unlocked`, `:2030-2161`)

1. 쓸 수 있는 항목만 추린다: 키가 빈 항목 제외 → `dead` 제외(수동 추가분은 24시간 뒤 목록에서 지운다. TTL로는 절대 안 돌아온다) → **이 모델에 대한 쿨다운**이 남은 항목 제외 → `exhausted`인데 풀리는 시각 전이면 제외, 지났으면 `ok`로 되돌리고 저장 → OAuth 토큰이 곧 만료면 갱신(실패하면 제외).
2. 전략: `fill_first`(기본. 우선순위 첫 항목) / `round_robin`(고른 항목을 맨 뒤로 보내고 우선순위를 다시 매겨 저장) / `random` / `least_used`(`request_count` 최소).
3. 고를 때마다 `request_count += 1`.
4. 하나도 없으면 `None`(로그는 60초에 한 번).

**실패했을 때 항목 상태**(`mark_exhausted_and_rotate`, `:2255-2331`; `_exhausted_ttl`, `:371-398`)

| 원인 | 쉬는 시간 | 비고 |
|---|---|---|
| 401 | 300초 | 풀 크기와 무관 |
| 429 | 3600초 | 공급자가 준 재개 시각(`reset_at`)이 있으면 그것이 이긴다 |
| 402 · 과금으로 분류된 것 | 3600초 | 항목이 하나뿐이어도 줄이지 않는다 |
| 그 밖(403 · 5xx · 모름) | 3600초 | |
| **항목이 하나뿐**이고 과금이 아닐 때 | 위 값과 60초 가운데 작은 것 | 바꿀 데가 없으니 오래 쉬게 해 봐야 소용없다 |
| 과금인지 확실치 않음 | 60초 | 진짜 402면 3600초 |
| 그 모델만 막힘(모델별 429 · 계정이 그 모델을 못 씀) | **그 모델만** 쿨다운, 항목은 `ok` 유지 | 모델 권한 거절은 365일 (`H:agent/credential_pool_model_cooldowns.py:19`) |
| 갱신 토큰이 죽음 | `dead` | 다시 로그인해야만 풀린다 |

- 같은 키를 가리키는 다른 항목(직접 넣은 것 + 설정에서 자동으로 만든 것)도 **함께** 쉬게 한다(안 그러면 같은 키가 계속 뽑힌다).
- 바꾼 결과가 방금 쉬게 한 바로 그 항목이면 "복구 못 함"으로 돌려준다(같은 429를 무한히 다시 하는 것을 막는다).

**언제 무엇을 하는가**(`recover_with_credential_pool`, `H:agent/agent_runtime_helpers.py:972-1095`)

- 풀은 **주 공급자의 것**이다. 폴백 공급자를 쓰는 중에 난 오류로는 풀을 건드리지 않는다.
- `billing` → 즉시 다음 항목으로.
- `rate_limit` → **한 번은 같은 항목으로 다시** 해 보고, 또 429면 바꾼다. 그 항목이 이미 `exhausted`로 적혀 있으면 바로 바꾼다(`:941-959`).
- `auth` → **먼저 갱신**을 해 보고(OAuth), 안 되면 바꾼다(`_recover_auth_failure`. 본문은 읽지 않았다).
- `upstream_rate_limit` → 풀은 가만히 두고 폴백에 맡긴다.
- 우선순위가 높은 항목을 429/402로 쉬게 하고 낮은 항목으로 옮겼으면, 그 창이 다시 열릴 때 **원래 항목으로 돌아가도록** 표시해 둔다(`_credential_pool_revert_id`, `:1043-1058`).
- 동시 사용: 항목마다 동시 1건(`DEFAULT_MAX_CONCURRENT_PER_CREDENTIAL = 1`, `:719`)의 느슨한 lease가 있다(세부는 읽지 않았다).

### 4.4 OAuth 경로의 도구 이름 [관찰]

- 보낼 때: OAuth 토큰으로 Anthropic을 부를 때 도구 이름 앞에 `mcp__`를 붙인다. 이미 `mcp_`로 시작하는 이름은 그 접두를 떼고 `mcp__`를 붙인다(`H:agent/anthropic_adapter.py:319, 549-555`).
- 받을 때: `normalize_response`가 `strip_tool_prefix`가 켜져 있으면 응답의 tool_use 이름에서 그 접두를 뗀다. 뗀 이름이 등록된 도구에 없으면 `이름 그대로 → mcp_ + 맨이름 → 맨이름` 순으로 맞춰 본다(`H:agent/transports/anthropic.py:20, 65-90`).

### 실패했을 때 (이 장치 자체가)

- 풀에 쓸 항목이 없으면 `select`가 `None` → 호출자는 폴백 사슬로 간다. 폴백도 없으면 턴 실패(`all_retries_exhausted_no_response` 또는 종류별 실패 결과).
- 풀 공급자와 지금 공급자가 다르면 풀을 건드리지 않는다(경고만).

### 가격인하에 옮길 때 (T9 · A11, `conformance.md`의 A12 · A14)

| 원본 | 판단 | 까닭 |
|---|---|---|
| 분류 결과의 모양(종류 + 힌트 네 개), 판단 순서(상태 코드 → 본문 코드 → 문구 → 전송) | **그대로** | 호출하는 쪽이 종류가 아니라 힌트만 보고 움직이게 된다 |
| 상태 코드 표 가운데 401 · 402 · 403 · 404 · 408 · 413 · 429 · 500 · 502 · 503 · 529, 400 꼬리 규칙의 뼈대(깨진 도구 인자 → role 연속 → 크기 → 검증 → 초과 추정) | **그대로** | Anthropic과 OpenAI 호환만 쓴다 |
| 문구 표 | **줄여서** | 과금 · 속도 제한 · 초과 · 인증 네 묶음만. 공급자가 늘면 더한다 |
| Nous · Codex · Z.AI · Gemini · llama.cpp · MoA · 이미지 갈래 | **안 가져옴** | 쓰지 않는 공급자 |
| 처리 순서(고치기 → 풀 → 갱신 → 이른 폴백 → 압축 → 물러서기), 폴백은 턴 단위 | **그대로** | |
| 물러서기 2초 · 60초 · 난수 50%, Retry-After 600초 상한 | **그대로** | |
| 자동 복구 사다리 | **줄여서** | 예약 실행(트리거)에는 쓸모 있다. 사용자 채팅에는 끈다. 사용자가 정한다 |
| 풀: 상태 세 가지, 쉬는 시간 표(300 / 3600 / 항목 하나면 60), 모델별 쿨다운, 같은 키 함께 쉬기, 429는 한 번 더 해 보고 바꾸기 | **그대로** | 사용자가 API 키와 구독 OAuth를 함께 넣을 수 있다(D3 · D4) |
| 전략 네 가지 | **줄여서** | `fill_first`만. 담당 6명이 같은 키를 쓰니 돌려 쓰기가 필요해지면 그때 |
| OAuth 도구 이름 접두 붙이기 · 떼기 | **그대로** | OAuth 경로를 넣으면 짝으로 필요하다(`conformance.md` A4) |


## 5. 세션과 기록 저장

기존 문서 `hermes-agent.md` 3.5(파일 묶음), 7.1(표와 칸, 세션 id · 세션 키 형식, 보존)은 다시 쓰지 않는다. 이 절은 **부분만 뜯었다** — 쓰기 · 읽기 · 검색의 동작과 프로필 나눔만 확인했고, 자동 리셋 정책은 확인하지 못했다.

### 동작 원리 [관찰]

**가. 세션의 수명**

- 세션은 **첫 턴이 도는 중에** 행이 생긴다. 클라이언트가 정해 준 세션 id(api_server의 `X-Hermes-Session-Id`, `/v1/runs`의 `session_id`)는 행이 없어도 턴 lease를 먼저 잡는다 — 두 요청이 같은 새 id로 동시에 들어와 서로의 턴에 끼어드는 것을 막으려고(`H:agent/turn_facade_lease.py:318-322`).
- 이어 가기: 같은 세션 id로 다시 부르면 저장된 시스템 프롬프트 · 도구 배열 · 메시지를 그대로 읽어 이어 간다(2절 마).
- 세션 id는 압축으로 바뀔 수 있었다(옛 방식의 회전). 이어 갈 때 `resolve_resume_session_id`가 "이 id의 최신 후손"을 찾아 준다(`H:hermes_state_messages.py:1468`, `turn_facade_lease.py:383-386`). 지금 기본은 제자리 압축이라 id가 유지된다(`hermes-agent.md` 3.4).
- 방 멤버 세션은 방 × 프로필마다 하나를 제목으로 찾아 계속 쓴다(7.4). Bot Chat 세션은 프로필마다 하나를 영원히 쓴다(7.7).
- 끝나는 이유(`end_reason`)로 본 값: `session_reset`(게이트웨이가 세션 키의 세션을 새로 바꿀 때, `H:gateway/session.py:1030, 1229`). 그 밖의 값 목록과 **자동 리셋 정책(유휴 시간 · 날짜 바뀜)의 조건 · 숫자는 확인하지 못함**.

**나. 메시지 쓰기** (`H:hermes_state_messages.py:397-429`)

- 한 메시지 = `messages` 한 행. 칸은 OpenAI 형식 메시지의 칸과 이름이 같다(`role, content, tool_calls, tool_call_id, tool_name, finish_reason, reasoning, reasoning_content, reasoning_details, …`).
- `content`는 화면 · 검색용 깨끗한 글, **`api_content`는 실제로 API에 보낸 글이 `content`와 달랐을 때만** 따로 둔다(바이트 그대로. 다시 보낼 때 캐시가 맞게).
- `message_uid`: 메시지마다 한 번 매기는 영구 id. 도구 호출에도 uid를 매겨 결과 행과 짝을 맞춘다.
- `display_kind` · `display_metadata`: 화면에 어떻게 보일지(숨김, steer, 기계 알림 등). 모델에게 가는 글은 바꾸지 않는다.
- 쓰기 트랜잭션 안에서 **턴 lease와 압축 락을 쥔 쪽이 맞는지 확인**한다(`_check_transcript_write_guards`). lease를 잃은 프로세스의 늦은 쓰기는 거절된다.
- 쓸 때마다 세션의 `message_count` · `tool_call_count`를 올린다.
- 이 쓰기는 "실패하면 턴이 끝나는" 쓰기라서, 다른 쓰기가 락을 몇 초 쥐고 있어도 기다린다(긴 인내 값).
- 쓰는 시점은 1절 "상태와 저장되는 것".

**다. 다시 불러오기** (`get_messages_as_conversation`, `:1522-1539`; `_rows_to_conversation`, `:1576-`)

1. `active = 1`인 행만 id 순서로 읽는다. 압축으로 보관된 행(`compacted`)은 **모델용 복원에서는 빼고** 화면 표시용 읽기에서만 넣는다(압축이 줄인 것을 다시 키우지 않으려고).
2. `api_content`가 있으면 그것을 **그대로**(다듬지 않고) 쓴다.
3. 추론 칸은 assistant 행에만 되살린다.
4. 실시간 재생용 읽기(`repair_alternation=True`)는 같은 role 연속을 고친 목록을 돌려준다. **저장된 기록은 고치지 않는다.**
5. 읽은 메시지마다 "이미 저장됨" 표식을 붙인다(다음 저장 때 또 쓰지 않게).
6. 짝 없는 tool_call · tool 결과는 요청을 만들기 직전에 고친다(1절 나-3-ii, 3절 나-8).

**라. 검색** (`H:hermes_state_search.py:1154-1240`, `H:hermes_state_fts.py:55-60`)

- 입력: `query, source_filter, exclude_sources, role_filter, limit(기본 20), offset, sort(없음 = BM25 순위 | newest | oldest), after_ts, before_ts`.
- 길: ① `role_filter`에 `tool`이 있으면 색인 대신 `LIKE`로 원문을 훑는다(큰 도구 결과는 앞부분만 색인하기 때문) ② FTS 색인이 낡았거나 깨졌으면 `LIKE`로 대신한다 ③ **질의에 한중일 글자가 있으면 CJK 길**(`_search_cjk`) — trigram 색인(`messages_fts_trigram`, `tokenize='trigram'`)을 쓴다 ④ 그 밖에는 `messages_fts`에서 `MATCH`, 기본 정렬은 순위.
- 되감기로 지운 행(`active=0, compacted=0`)은 빼고, 압축으로 보관된 행은 **검색에 포함**한다. 색인을 다시 만드는 중이면 빠진 구간을 `LIKE`로 보충한다.
- trigram은 3글자 단위라 **2글자 한글 낱말은 색인으로 못 찾는다** [추정: trigram의 일반 성질. `_search_cjk`가 짧은 질의를 어떻게 다루는지는 본문을 읽지 않아 확인하지 못함].

**마. 여러 프로필이 저장소를 나누는 방식** [관찰]

| 무엇 | 어디 | 근거 |
|---|---|---|
| 세션 · 메시지 · 시스템 프롬프트 · 턴 lease · 사용량 | **프로필마다** 자기 홈의 `state.db`. 기본 프로필은 루트, 나머지는 `<루트>/profiles/<이름>` | `H:tui_gateway/hosted_room_service.py:812-815`, `hermes-agent.md` 7.1 |
| SOUL.md · 메모리 · 스킬 · 설정 · 자격 | 프로필 홈마다 | `hermes-agent.md` 3.8 |
| 방(hosted rooms) · 방 이벤트 · 드라이버 작업 · 라우터 claim | **루트의 공유 저장소** 하나 | `H:tui_gateway/hosted_room_router_call.py:108`, `hermes-agent.md` 7.1 |
| 방 멤버의 대화 이력 | 각 멤버 프로필의 `state.db` 안 `Group: <room_id>` 세션 | 7.4 |
| 칸반 | 루트 공유 | `hermes-agent.md` 6.2 |

한 게이트웨이 프로세스가 여러 프로필의 DB를 **동시에 연다**. 어느 프로필로 도는지는 "현재 홈" 문맥 변수로 정하고, 방 설정처럼 주인 프로필의 설정을 읽어야 할 때는 잠깐 홈을 바꿔 읽는다(`H:tui_gateway/hosted_room_service.py:93-103`).

### 실패했을 때

- 쓰기 실패 → 턴 실패(1절). 원인을 분류해 결과에 싣는다(`classify_persistence_error`).
- FTS가 깨지면 색인을 떼고 원문 훑기로 답한다(검색은 계속 된다).
- lease를 잃은 쓰기는 거절된다.

### 가격인하에 옮길 때 (T9 · T16 · A12)

| 원본 | 판단 | 까닭 |
|---|---|---|
| 메시지 한 행 = OpenAI 형식 한 메시지, `content`와 `api_content` 분리, `message_uid` | **그대로** | 캐시 보존과 화면 표시를 둘 다 만족한다 |
| 복원은 `active`만, 보관된 행은 표시 · 검색에만 | **그대로** | |
| 재생용 고치기는 읽은 사본에만, 저장본은 안 고침 | **그대로** | |
| 프로필마다 DB 파일 따로 | **줄여서** | SQLite 파일 하나(D6)에 `agent_id` 칸으로 나눈다. 한 프로세스 · 한 writer라 파일을 나눌 까닭이 없다. 방 로그는 원본처럼 공유 표로 |
| 검색(FTS5 + trigram + 원문 훑기 대체) | **줄여서** | 한글이 주라 trigram을 기본으로 하고, 2글자 질의는 원문 훑기로 보낸다. T16에서 정한다 |
| 다중 프로세스 영수증 · 세대 · 수리 계열(약 40개 파일) | **안 가져옴** | `hermes-agent.md` 3.5의 판단 그대로 |


## 6. 메모리

기존 문서 `hermes-agent.md` 3.6(위치, 구분자 `\n§\n`, 동작 종류, 한도 2200 / 1375자, 프롬프트 volatile 층)은 다시 쓰지 않는다.

### 동작 원리 [관찰]

**가. 언제 쓰이는가** — 세 길이 있다.

1. **도구 호출**: 에이전트가 턴 도중 `memory` 도구를 부른다(에이전트 수준 도구라 일반 도구 등록부보다 먼저 가로챈다 — `hermes-agent.md` 3.2).
2. **주기적 검토(nudge)**: user 턴을 셀 때마다 1씩 올리고, `memory.nudge_interval`(기본 **10**)에 닿으면 그 턴에 "메모리 검토" 표시를 세운다(`H:agent/turn_context.py:816-829`, `H:agent/agent_init.py:1160, 1194`). 대화에 **글을 끼워 넣지 않는다**(원래 user 메시지를 그대로 둔다, `turn_context.py:1293-1295`).
3. **턴 뒤 백그라운드 검토**: 표시가 선 턴이 끝나면 데몬 스레드에서 에이전트를 **복제**해 대화 사본을 다시 돌리며 "저장하거나 고칠 메모리 · 스킬이 있는가"를 묻는다. 쓰기는 메모리 · 스킬 저장소로 바로 간다. 본 대화와 그 프롬프트 캐시는 건드리지 않는다. 복제본은 부모의 공급자 · 모델 · 자격 · **캐시된 시스템 프롬프트**를 물려받아 같은 캐시 앞부분을 쓰고, 쓸 수 있는 도구가 제한된다(`H:agent/background_review.py:1-6`).

**압축 전에 메모리로 흘려보내는 단계는 보지 못했다** [관찰: 3절에서 읽은 `compress()` 본문에 없다. 다만 요약 프롬프트에 "외부 메모리 공급자가 준 맥락" 절을 넣는 자리는 있다(`_memory_provider_section`, `H:agent/context_compressor.py:2053, 3952`)].

**나. 도구의 규칙** (`H:tools/memory_tool_store.py`)

- 대상 둘: `memory`(머리말 "MEMORY (your personal notes)") / `user`(머리말 "USER PROFILE (who the user is)") (`:23-24`).
- `add`: 빈 글 거절 → 주입 · 비밀 검사에 걸리면 거절(`scan_blocked`) → **같은 글이 이미 있으면 성공으로 돌려주고 더하지 않음** → 더하면 한도를 넘을 때 **거절**(자르지 않는다). 거절 응답에 지금 항목 전체와 사용량, "낡은 항목을 지우거나 줄이고 이것을 더하는 것을 **한 번의 묶음(operations)** 으로 다시 하라, 한도는 묶음 결과로만 본다"는 안내가 실린다(`:403-419, 332-340`).
- `replace`: `old_text`는 항목을 **찾는 데만** 쓴다(통째 일치 먼저, 그다음 부분 일치). 찾은 항목 **전체**를 새 글로 바꾼다. 둘 이상 걸리면 거절(`:421-`, `_find_unique_match :98`). 찾을 때 따옴표 종류는 같은 것으로 본다(`:76-80`).
- 성공 응답: `{success, done, target, usage: "N% — 현재/한도 chars", entry_count, note: "저장됨, 되풀이하지 마라"}`. **항목 목록은 돌려주지 않는다**(돌려주면 모델이 "더 고칠 것"을 찾아 같은 일을 또 한다, `:660-673`).
- 파일: 쓰기는 파일 락을 잡고 통째로 다시 쓴다(`_file_lock`, `_mutate`). 다른 세션이 그 사이 바꿨으면(drift) 덮어쓰지 않고 오류를 낸다. 백업 실패도 알린다(`:38-65, 262-353`).

**다. 읽는 시점**

- 세션 시작 때 디스크에서 읽어 **얼린 사본**을 만든다. 시스템 프롬프트에는 이 사본이 들어간다. 턴 도중 쓴 것은 파일에는 바로 들어가지만 **그 세션의 프롬프트에는 반영되지 않는다**(`format_for_system_prompt`, `:655-658`).
- 다시 읽는 때: 압축 뒤 프롬프트를 다시 만들 때(`invalidate_system_prompt`가 `load_from_disk`를 부른다, 2절 마).
- 프롬프트 블록 모양: 머리말 + 사용량 표시 + 항목들(`_render_block`, `:675-`).

**라. 공유**

- 메모리 파일은 **프로필 홈마다 따로**다(`<홈>/memories/`). 프로필(에이전트)끼리 공유하지 않는다. 같은 프로필의 여러 세션은 같은 파일을 쓴다(그래서 drift 검사가 있다).
- cron 세션은 메모리를 건너뛴다(`hermes-agent.md` 3.6).
- 외부 메모리 공급자: 있으면 프롬프트에 블록 하나를 더하고 도구를 더 내놓는다(`H:agent/system_prompt.py:545-559`). 인터페이스 세부는 확인하지 못함.

### 실패했을 때

- 한도 초과 · 중복 일치 · 검사 걸림 · drift는 모두 **구조화된 오류**로 모델에게 돌아간다. 한 턴 안의 연속 실패에는 상한이 있다(`_consolidation_failures`. 숫자는 확인하지 못함).
- 백그라운드 검토가 실패해도 본 대화에는 영향이 없다.

### 가격인하에 옮길 때 (T15 · T9)

| 원본 | 판단 | 까닭 |
|---|---|---|
| 담당마다 메모리 따로, 세션 시작 때 얼린 사본, 압축 뒤에만 다시 읽기 | **그대로** | 캐시 불변식과 한 묶음이다 |
| 한도 넘으면 자르지 않고 거절 + "묶음으로 다시" 안내, 성공 응답에 항목을 안 실음 | **그대로** | 모델이 메모리를 스스로 정리하게 만드는 장치다 |
| 10턴마다 턴 뒤 백그라운드 검토(복제본, 캐시 앞부분 공유) | **줄여서** | 우리는 **복기 담당**(F5 · T15)이 장 마감 뒤 하는 일이 이것이다. 턴마다 돌리지 않고 복기 때 한 번 돌린다. "본 대화를 건드리지 않고 사본으로 돈다"는 방식은 그대로 |
| 팀 전체가 보는 기억 | 원본에 **없다** | 방의 공유 노트(7.1, 4KB)가 가장 가깝다. 실수 기록 · 규칙 개선안(M4)은 메모리가 아니라 우리 DB 표와 지식 검색(M7)에 둔다 |
| 외부 메모리 공급자 | **안 가져옴** | |


## 7. 여러 에이전트의 협업

기존 문서 `hermes-agent.md` 6.5(이벤트 로그 칸, 라우팅 세 모드 개요, 상한 숫자, JSON-RPC 이름)는 다시 쓰지 않는다. 아래는 그 절에 없는 **계획기의 알고리즘 · 멤버 턴의 입력 조립 · 드라이버 상태 기계 · 숫자**다. 전부 직접 읽은 것이다.

### 7.1 hosted rooms: 방과 로그 [관찰]

**저장되는 것**

- `hosted_rooms` 한 행 = 방 하나: `room_id, name, members_json, authority_gateway_id, authority_epoch(≥1), next_seq, event_bytes, revision, created_at, updated_at, disbanded_at` (`H:gateway/hosted_rooms.py:89-101`).
- 멤버 한 명의 모양: `member_id, profile, handle, display_name(선택), target`(`{"kind":"local","profile":…}` 또는 원격 `peer`) (`H:gateway/hosted_room_discussion.py:113-119`, `H:tui_gateway/hosted_room_service.py:921-926`). 논의 멤버는 2명 이상 6명 이하다(`MIN/MAX_DISCUSSION_MEMBERS`, `discussion.py:29-30`).
- `hosted_room_events`: `(room_id, seq)`가 기본 키, `(room_id, event_id)`가 유일 — `event_id`가 멱등 키다(`hosted_rooms.py:102-114`). 같은 `event_id`로 다른 내용을 쓰면 `EventConflictError`.
- 누가 어떤 종류를 쓸 수 있는지가 actor 종류로 고정돼 있다(`hosted_rooms.py:50-58`):

| actor.kind | 쓸 수 있는 이벤트 |
|---|---|
| `user` | `message.user` |
| `member` | `message.member` |
| `gateway` | `member.unavailable, room.activity, room.assignment, room.continued, room.routed, room.stop_requested, turn.started, turn.settled, turn.failed, turn.cancelled, turn.deferred, turn.reassigned` |
| `system` | `authority.claimed, authority.lost, room.context_set, room.created, room.disbanded, room.members_changed, room.renamed` |

- actor 칸: `kind, id` + 선택 `display_name, profile, connection_id` (`hosted_rooms.py:59-61`).
- 방 설정(목적 · 공유 노트 · 라우팅 · 리드)은 따로 표가 없고 `room.context_set` 이벤트의 payload `{purpose, notes, routing, lead_member_id}`다. 목적 500자, 노트 4KB 상한. 라우팅은 `everyone | lead | auto`. `lead`는 리드 멤버가 있어야 하고 그 멤버가 방에 앉아 있어야 한다(`H:gateway/hosted_room_context.py:26-54, 117-118`).
- 설정은 **논의를 연 메시지 바로 앞까지의 값**으로 읽는다(`room_context(before_seq=그 메시지 seq)`, `hosted_room_context.py:64-80`, `service.py:780-781`). 논의 도중 바꿔도 이미 시작한 논의는 그대로다.
- 크기 상한: 이벤트 하나 256KB, 방 하나 5만 이벤트 · 256MB, 살아 있는 방 256개, 로그 한 쪽 500개 · 2MB, 사용자 글 64KB, 멤버 글 64KB(넘으면 잘림 안내 문구를 붙여 자른다) (`hosted_rooms.py:30-40`, `discussion.py:34-36`).

**이벤트별 payload 칸** (`discussion.py:70-88`, `hosted_room_router.py:26-27`, `hosted_room_lead.py:32-33`)

| 이벤트 | payload |
|---|---|
| `message.user` | `text, thread_id`(+ 첨부) |
| `message.member` | 턴 좌표 7개 + `text` |
| `turn.settled` | 턴 좌표 + `seen_through_seq, message_event_id, passed` |
| `turn.failed` | 턴 좌표 + `seen_through_seq, error`(+ 선택 `reason_code`) |
| `turn.cancelled` | 턴 좌표 + `seen_through_seq, reason` |
| `turn.deferred` | 턴 좌표 + `seen_through_seq, execution_generation, reason` |
| `room.activity` | `status(settled|bounded), reason_code, thread_id, discussion_event_id` |
| `room.stop_requested` | `cancel_id` |
| `room.routed` | `discussion_event_id, thread_id, member_ids, everyone, fallback` |
| `room.assignment` | `discussion_event_id, thread_id, task_id, lead_member_id, member_ids, fallback` |
| `room.continued` | `discussion_event_id, thread_id, grant(1~5)` |

턴 좌표 = `discussion_event_id, member_id, member_index, round_index, task_id, thread_id, turn_id`. `turn_id`는 `d<논의 seq>.r<라운드>.p<자리 0~5>.s<본 곳 seq>.m<멤버 지문 24자>` 모양이고, `task_id`는 `dtask:` + (논의 · 멤버 · 라운드 · 본 곳 · 프롬프트 해시 등을 묶은 JSON의 sha256 앞 48자)다(`discussion.py:53-56, 661-677`). 즉 **같은 로그면 같은 id가 나온다**. 발행 이벤트 id도 여기서 파생한다: `dmessage:<digest>`, `dterminal:<digest>`, `ddeferred:<digest>:g<세대>`, `dassign:<digest>`, `droute:<논의 id>`, `dactivity:<논의 id>:<reason>[:c<이어가기 수>]`, `dcontinue:<논의 id>:<grant>` (`discussion.py:1042-1048`, `service.py:749, 886`, `router.py:36-37`).

### 7.2 누가 말할 차례인가: 계획기 `plan_next_task` [관찰]

계획기는 I/O가 없는 순수 함수다. **입력**: 방(멤버 명단) + 전체 로그 + 논의를 연 시점의 방 설정 + 상한. **출력**: `idle | task | settled | bounded | route` 가운데 하나와, `task`일 때 **다음 턴 딱 하나**(`discussion.py:1-7, 803-879`). 한 방에서 턴은 한 번에 하나만 돈다(순차). 아래 순서 그대로다.

1. **논의 고르기** (`_pending_discussion`, `:691-704`). "논의"는 `message.user` 이벤트 하나가 연다. 스레드마다 **가장 최근** user 메시지만 후보다(같은 스레드에 새 user 메시지가 오면 앞 논의는 버려진다). 후보 가운데 ① 마지막 `room.stop_requested`보다 뒤에 있고 ② 아직 `room.activity`(settled · bounded)로 끝나지 않은 것(끝났어도 그 뒤에 `room.continued`가 있으면 다시 열림) 중 **seq가 가장 작은 것**. 없으면 `idle`.
2. **글 수 상한**. 이 논의에서 나온 멤버 글 수 ≥ `max_messages × (이어가기 수 + 1)`이면 `bounded / max_messages` (`:824-830`). 기본 `max_rounds 3`, `max_messages 10`, 설정 키 `hosted_rooms.discussion.max_rounds / max_messages`(방 주인의 config. 값이 이상하면 기본값으로) (`service.py:93-116`).
3. **auto 라우팅 대기**. 방이 `auto`이고 이 메시지가 "라우팅 대상"(아래 4의 조건과 같다)인데 `room.routed`가 아직 없으면 `route / router_pending`을 돌려준다 → 서비스가 라우터 호출을 띄운다(7.3) (`router.py:45-59`, `discussion.py:831-834`).
4. **lead 라우팅 턴**. 방이 `lead`이고 리드가 앉아 있고, 메시지가 ① 예약 게시가 아니고 ② 명단의 핸들이나 `@all`/`@everyone`을 하나도 멘션하지 않고 ③ 그 스레드의 첫 메시지이면(`routes_through_lead`, `lead.py:48-61`) 리드의 **라우팅 턴**을 먼저 낸다(라운드 0 · 자리 0 · `seen_through_seq = 메시지 seq − 1` · payload `lead_routing: true`). 이미 끝났으면 그 결과(7.3)로 0라운드 응답자를 정한다.
5. **라운드 돌기** — `round_index = 0 .. max_rounds×(이어가기+1) − 1`:
   - **응답자 정하기**. 0라운드: (리드 배정 결과) 또는 (auto 라우터 결과) 또는 `_first_round_responders` — 사람 메시지는 멘션한 멤버만, **멘션이 없으면 전원**, `@all`/`@everyone`이면 전원. 예약 게시(cron)는 멘션한 멤버만(없으면 아무도), 실패 알림(`cron-notice:`)은 아무도 (`:770-776`). 1라운드부터: `_unaddressed_member_mentions` — 이 논의의 멤버 글에서 **다른 멤버가 @핸들로 부른 멤버 가운데, 불린 뒤로 아직 글을 내지 않은 멤버만**(`:365-380`). 자기 멘션과 명단에 없는 핸들은 무시한다(`:346-362`).
   - 이어가기 직후의 라운드(`resume_round` = 이어가기 전 마지막 글의 라운드 + 1)에 부른 사람이 아무도 없으면 **전원**이 다시 받는다(`:855-857`, `router.py:67-74`).
   - **순서**. 응답자 목록을 `round_index`만큼 왼쪽으로 돌린다(`_rotate`, `:562-564`). 명단 순서가 기본이고 라운드마다 시작 사람이 한 칸씩 밀린다.
   - 순서대로 각 멤버에 대해: 그 멤버의 **워터마크**(이 스레드에서 그 멤버가 마지막으로 본 seq) 뒤의 스레드 글을 모은다(`_bounded_task_delta`). 새 글이 없거나, 이 라운드에서 이미 끝 이벤트(`turn.*`)를 낸 멤버면 건너뛴다. 아니면 **그 멤버의 턴 하나를 돌려주고 끝**(`:858-874`).
   - 이 라운드에 낼 턴이 없을 때: 이 라운드에서 나온 멤버 글이 하나도 없으면 `settled / silent_round`(논의 끝). 마지막 라운드였으면 `bounded / max_rounds`. 아니면 다음 라운드로.
6. `settled` · `bounded`가 나오면 서비스가 `room.activity`를 한 번 적는다(`service.py:740-755, 807-808`). 그 뒤로 이 논의는 1에서 걸러진다.

핵심 성질 세 가지:

- **뒤 라운드는 "불린 사람만"이다.** 누가 누구를 @멘션하지 않으면 1라운드에서 조용한 라운드가 되어 끝난다. 그래서 everyone 방의 보통 논의는 "전원이 한 번씩 → 서로 부른 사람만 한두 번 더"로 끝난다.
- **침묵 표식**: 멤버의 답이 비었거나 `pass` · `(pass)` · `(pass).`(대소문자 무시)면 "넘김"이다. `message.member`를 내지 않고 `turn.settled`에 `passed: true`만 남는다(`is_pass_text`, `:340-343, 975-985`).
- **직접 불린 멤버는 넘길 수 없다.** 사람이 0라운드에서 직접 멘션한 멤버가 넘김으로 답하면 `turn.failed`("직접 불렸는데 보이는 답이 없다")로 바꿔 발행한다(`:647-653, 1036-1041`).

워터마크 계산(`_derive_member_watermarks`, `:525-554`): `(thread_id, member_id)`마다, 그 멤버의 끝 이벤트의 `seen_through_seq` 최댓값. 넘기지 않고 글을 낸 `turn.settled`면 자기 글의 seq까지 올린다(자기 글을 다시 받지 않게). 한 작업에 끝 이벤트는 하나뿐이어야 하고(`turn.deferred`만 세대가 오르며 여러 번 가능) 어기면 로그가 깨진 것으로 본다.

### 7.3 lead · auto 라우팅의 세부 [관찰]

**lead** (`H:gateway/hosted_room_lead.py`)

1. 라우팅 턴의 프롬프트 구조: 머리줄(방 이름, "너는 @리드, 이 방의 리드") → 방 목적 · 노트 → 멤버 목록(`- @handle (표시 이름) (you)`) → 사용자 메시지 원문(+ 첨부 파일 이름 · 종류) → 지시 두 줄("이름 없는 메시지가 왔으니 누가 맡을지 골라라, 여기서 답하지 마라" / "JSON 객체 하나로만 답하라, 예: `{"assign": ["handle"]}`") (`:34-38, 68-77`). 리드 자신의 방 세션에서 보통 멤버 턴처럼 돈다.
2. 답 해석: 답 글에서 **`assign` 키가 있는 첫 번째 균형 잡힌 JSON 객체**를 찾는다(앞뒤에 산문이 있어도 된다). 값이 문자열 목록이면 `@`를 떼고 소문자로 맞춰 명단 순서로 고른다(`:88-111`).
3. 결과를 `room.assignment`로 적는다. 아무도 못 고르거나(JSON 아님 · 명단 밖) 라우팅 턴이 실패하면 **리드 혼자**(`fallback: true`) — 절대 전원 방송으로 번지지 않는다. 라우팅 턴이 취소(버림 · 대체)로 끝나도 리드가 받는다(`:119-132, 149-167`).
4. 라우팅 답 자체는 방에 글로 나가지 않는다(넘김으로 처리, `discussion.py:1045-1051`). 라우팅 턴의 영수증은 리드의 워터마크를 메시지 앞까지만 올리므로, 리드가 자기를 고르면 0라운드에서 보통 턴으로 다시 답한다.
5. 고른 멤버의 프롬프트에는 "방 리드 @X가 사용자 메시지를 너에게 줬다"가 들어가고 넘김이 금지된다(`discussion.py:615, 622`).

**auto** (`H:gateway/hosted_room_router.py`, `H:tui_gateway/hosted_room_router_call.py`)

1. 호출: 보조 모델 작업 이름 `group_router`(설정 `auxiliary.group_router`), `temperature 0`, `max_tokens 200`, 도구 없음, 메시지 두 개(system + user). 방 주인 프로필의 설정 · 자격으로 돈다(`router_call.py:25-26, 53-72`). 기본 시간 제한 20초(주석 `router_call.py:88`. 설정값을 읽는 곳은 확인하지 못함).
2. system 지시의 뜻: "그룹 채팅의 메시지를 라우팅한다. 요청이 닿는 역할의 멤버만 골라라. 전체용일 때만 everyone. 남의 말을 되풀이하거나 맞장구치게 하려고 고르지 마라. `{"members": ["handle", …]}` 또는 `{"members": "everyone"}` JSON 하나로만 답하라"(`router.py:155-160`).
3. user 입력 구조: `Room:` 이름 → `Purpose:` → `Notes:`(2000자까지) → `Members:`(멤버마다 `- @handle (이름): 역할` — 역할은 그 프로필 `profile.yaml`의 `description` 300자까지) → `Recent messages`(방의 최근 5개, 각 600자로 한 줄) → 사용자 새 메시지(20000자까지) → 첨부 파일 이름(`router.py:31-32, 163-189`, `router_call.py:29-50`).
4. 답 해석(엄격): 코드펜스는 벗긴다. 키가 정확히 `members` 하나인 객체여야 한다. `"everyone"`이면 전원. 목록이면 비어 있지 않고 전부 문자열이고 중복 없고 전부 명단에 있어야 한다. 하나라도 어긋나면 **무효 → 전원**(`fallback: true`) (`router.py:192-213, 147-150`).
5. 호출 실패(예외)도 전원으로 대체한다(`router_call.py:74-76`).
6. 답은 `room.routed`로 **먼저** 적고 그다음 계획한다. 적을 때 "지금도 이 메시지의 라우팅을 기다리는 중인가"를 다시 계획해 확인하고, 로그 끝 번호가 그대로일 때만 적는다(`expected_latest_seq`, 3번까지 재시도). 호출 동안 Stop이나 새 메시지가 왔으면 답을 버린다(`service.py:828-858`).
7. 같은 (방, 메시지)에 라우터 호출은 하나만: `hosted_room_router_claims` 표에 주인 · 만료를 적는 claim, 수명 120초(`router_call.py:85-121`).
8. auto로 골라진 멤버(전원이 아닐 때)의 프롬프트에는 "방이 너를 골랐다"가 들어가고 넘김이 금지된다(`discussion.py:617, 622, 869`).

### 7.4 한 사람의 말이 다른 참가자에게 전달되는 방식 [관찰]

- **멤버마다 자기 세션이 있다.** 방 하나 × 프로필 하나 = 그 프로필의 저장소 안에 제목이 `Group: <room_id>`이고 source가 `bot_room`인 숨김 세션 하나. 없으면 만들고 있으면 이어 쓴다(`H:tui_gateway/hosted_room_driver.py:30, 1211-1220, 1238-1240`). 즉 멤버는 자기 SOUL · 메모리 · 도구로, 자기 방 세션의 이력 위에서 턴을 돈다.
- **다른 사람의 말은 그 멤버 세션의 `user` 메시지로 들어간다**(시스템 프롬프트가 아니다 — 캐시 보호. `discussion.py:594-595` 주석). 턴 프롬프트의 구조(`_build_prompt`, `:605-644`):
  1. 머리줄: `[Discussion: "<방 이름>"]` 너는 @핸들, 다른 참가자 @a, @b … 와 사용자와 함께 있다.
  2. 방 목적 한 줄, 공유 노트(들여쓴 줄들).
  3. "네 마지막 턴 뒤로 이 스레드에 새로 온 메시지(오래된 것부터)": 워터마크 뒤 ~ `seen_through_seq`까지의 스레드 글 가운데 **마지막 24줄**(`MAX_DISCUSSION_DELTA_LINES`). 줄 모양은 사람 글 `User (user): …`, 예약 게시 `Scheduled job "<이름>" (automated post, not the user): …`, 멤버 글 `@handle: …` (`:574-582`).
  4. 규칙: (가) 불린 경우 — "사용자가 직접 불렀다 / 리드가 줬다 / 방이 골랐다 / 예약 작업이 불렀다. 답 · 결과 · 구체적 막힘을 답하라, `(pass)` 금지". (나) 그 밖 — "새로 보탤 것이 있을 때만 한 번 말하라, 없으면 정확히 `(pass)`". 공통 — "동료를 핸들로 멘션하면 다음 라운드에 끌어온다, 이미 나온 말을 되풀이하지 마라, 사적인 대화 내용을 드러내지 마라, 네 답은 그대로 게시된다".
- 프롬프트 전체 상한 128KB(`MAX_PROMPT_BYTES`, `H:gateway/hosted_room_driver.py:30`). 넘치면 **오래된 줄부터** 빼고 `[Earlier content omitted to fit this turn.]`을 넣는다(`discussion.py:629-641`).
- **주입 방어**: 멤버 글과 예약 게시 글 안에 Hermes 자신의 제어 프레임 머리(steer 표식, 압축 인계, 런타임 노트 등 `CONTROL_FRAME_OPENERS`)가 있으면 `[member-quoted `로 바꿔 넣는다. 사람 글은 손대지 않는다(`:42-52, 574-582`).
- 멤버가 본 것만 워터마크가 오르므로, 한참 뒤에 불린 멤버도 그동안의 글을 한 번에 받는다(24줄 · 128KB 안에서).
- 멤버의 답 → 방: 멤버 세션의 턴이 끝나면 그 **최종 답 글 전체**(64KB 상한)가 드라이버 작업의 결과로 저장되고, `prepare_room`이 그것을 `message.member`(actor = 그 멤버) + `turn.settled` 두 이벤트로 발행한다. 순서는 **글 먼저, 끝 이벤트 나중**이고, 그 사이에 죽으면 "끝 이벤트 없는 글"은 계획 입력에서 빼서 같은 좌표가 다시 나오게 한다(`discussion.py:712-726`, `service.py:710-738`). 스트리밍 중간 글은 로그에 들어가지 않는다 — 진행 표시는 따로 메모리의 `task_progress`로 나간다(`service.py:82-90`. 클라이언트로 가는 통지의 모양은 확인하지 못함).
- 늦게 온 결과: 턴이 도는 동안 같은 스레드에 새 user 메시지가 오면, 그 턴의 결과는 글로 발행되지 않고 `turn.cancelled`(`reason: superseded_by_newer_user_event`)만 남는다(`discussion.py:1031-1034, 997-1000`).

### 7.5 드라이버: 작업 상태 기계 · lease · 동시성 [관찰]

작업 표 `hosted_room_driver_tasks`: `room_id, task_id, thread_id, turn_id, source_event_seq, payload_json(target_profile, prompt, source_event_seq + 선택 target_member_id, input_context, attachments, lead_routing), status, execution_generation, cancel_generation, run_gateway_id, run_process_generation, run_lease_generation, cancel_id, settlement_id, settlement_status, result_json, created_at, updated_at, started_at, terminal_at, indeterminate_at` (`H:gateway/hosted_room_driver.py:40-41, 240-254`). 결과 JSON 상한 256KB, 끝난 작업은 30일 · 2048개까지 보관(`:31-33`).

```mermaid
stateDiagram-v2
  [*] --> queued: admit_task (계획기가 낸 턴)
  queued --> running: start_task (lease 쥔 드라이버, execution_generation +1)
  running --> settled: 멤버 답 도착
  running --> failed: 턴 실패 · 제출 전 거절
  running --> queued: 제출이 받아들여지지 않음(재시도 1→30초 배로)
  running --> indeterminate: 제출 뒤 관찰 실패가 lease 길이만큼 이어짐 · 프로세스가 바뀜
  running --> stopping: Stop · 턴 시간 초과(cancel_id "deadline:<세대>")
  queued --> cancelled: Stop
  deferred --> cancelled: Stop
  stopping --> cancelled: 중단 확인
  stopping --> failed: 시간 초과로 멈춘 것(reason_code turn_deadline_exceeded)
  stopping --> settled: 멈추기 전에 답이 먼저 끝남
  stopping --> deferred: 중단을 확인하지 못함
  indeterminate --> settled: 세션 이력에서 답을 찾음
  indeterminate --> queued: 돌지 않았음이 확인됨
  indeterminate --> deferred: 60초 안에 판가름 못 함
  deferred --> queued: 사용자가 Retry
  deferred --> indeterminate: 다시 살펴보기
  settled --> [*]
  failed --> [*]
  cancelled --> [*]
```

(상태 이름 `:35-38, 246-247`, 전이 `:76-111, 660-905`. `indeterminate → deferred`의 60초는 `indeterminate_defer_seconds`, `H:tui_gateway/hosted_room_driver.py:125`.)

**실행 순서** (`H:tui_gateway/hosted_room_driver.py`)

1. 감독 스레드가 5초마다(또는 깨움 신호 즉시) 방 목록을 돈다. 한 번에 **최대 4개 방**을 방마다 스레드 하나로 처리하고, 시작 방을 한 칸씩 돌려 공평하게 한다(`poll_interval_seconds 5`, `max_concurrent_rooms 4`, `:123-125, 646-679`).
2. 방 하나 처리(`_process_room`, `:700-739`): `stopping` 작업 먼저 마무리 → `prepare_room`(끝난 작업을 방 이벤트로 발행 → 살아 있는 작업이 없으면 계획기를 불러 다음 턴을 `queued`로 넣음, `service.py:757-810`) → 버려진 시도 살펴보기 → **방 lease** 얻기 → `queued` 작업을 차례로 `start_task` → 실행.
3. 방 lease(`hosted_room_driver_leases`): 수명 30초, 남은 시간이 절반 아래면 갱신, 턴이 도는 동안은 별도 스레드가 10초(수명/3)마다 갱신한다. 다른 프로세스가 살아 있는 lease를 쥐고 있으면 그 방은 건너뛴다. lease를 새로 얻으면 한 번 `recover_room`(앞 세대가 `running`으로 남긴 작업을 `indeterminate`로)을 돌린다(`:123, 751-794, 827-829`).
4. 실행(`_execute_attempt`, `:807-884`): **프로필 턴 락**을 잡는다(같은 프로필은 한 번에 한 턴) → 방 세션을 찾거나 만든다 → 프롬프트를 제출한다 → 끝 영수증을 기다린다(0.25초 간격으로 작업 상태 · 세션 이력 · 세션 정보 확인).
5. 턴 시간 제한: `HERMES_AGENT_TIMEOUT`(기본 1800초) + 30초 = **1830초**. 넘으면 그 시도를 멈추고 `turn.failed`(`turn_deadline_exceeded`)로 발행한다(`service.py:119-124`, `driver.py:969-971, 997-1028`).
6. 끝 영수증이 오면 `settle_task` → `prepare_room`이 다시 돌아 발행하고 다음 턴을 계획한다 → 방을 다시 깨운다.

**동시에 여러 방**: 방끼리는 최대 4개가 병렬이지만, 같은 프로필(멤버)이 두 방에 걸리면 프로필 턴 락 때문에 한쪽이 기다린다. 기다리는 동안 lease는 계속 갱신한다(`:824-829`). 방 안에서는 항상 한 턴씩이다(`prepare_room`은 살아 있는 작업이 있으면 다음 턴을 넣지 않는다, `service.py:772-773`).

**실패했을 때**

| 상황 | 처리 | 근거 |
|---|---|---|
| 멤버 턴 실패 | `turn.failed`(`error`, `reason_code`) 발행. 그 멤버만 빠지고 **같은 라운드의 나머지는 계속** 돈다(끝 이벤트가 있으면 건너뛸 뿐) | `discussion.py:988-994, 858-862` |
| 멤버가 답을 못 받을 상태(원격 · 미확인) | `turn.deferred` — 끝이 아니다. 화면에 Retry / Discard가 뜬다. Retry면 다시 `queued` | `lead.py:123-125`, `service.py:44-45, 1082` |
| 프로세스가 죽음 | lease 만료 → 새 lease → `recover_room` → 그 프로필의 방 세션 이력에서 `task_id + execution_generation`이 붙은 assistant 글을 찾아 있으면 그것으로 정산, 없으면 다시 큐 또는 deferred | `driver.py:1031-1090, 1293-1310` |
| 계획 · 발행이 계속 실패 | 방을 "막힘"으로 세고 다음 Send나 Retry까지 건너뛴다. 방에 한 번 알린다 | `service.py:896-908` |
| 로그가 그 사이 바뀜 | `expected_latest_seq`가 어긋나면 `EventCursorConflictError` → 다음 순회에서 다시 계획 | `service.py:763-765` |

### 7.6 끝나는 조건과 Stop · Continue [관찰]

- 논의가 끝나는 길: ① 조용한 라운드(`settled / silent_round`) ② 라운드 상한(`bounded / max_rounds`) ③ 글 수 상한(`bounded / max_messages`) ④ Stop ⑤ 같은 스레드에 새 user 메시지(앞 논의는 후보에서 빠지고 도는 턴의 결과는 `turn.cancelled`) ⑥ 방 해산.
- **Stop** (`service.py:971-1016`): `room.stop_requested`를 적는다(그 seq가 "멈춤 울타리"). 울타리보다 앞선 메시지에서 나온 작업 가운데 `queued, running, indeterminate, deferred, stopping`을 전부 취소한다. `running`은 `stopping`으로 바꾸고 그 멤버 세션에 interrupt를 보낸 뒤 확인되면 `cancelled`. 울타리 뒤에 온 메시지는 새 논의로 정상 진행한다.
- **Continue** (`service.py:860-894`, `router.py:118-144`): 마지막 결과가 `bounded`이고 그 뒤로 Stop도 같은 스레드의 새 메시지도 없을 때만 된다. `room.continued`(`grant` = 몇 번째인지)를 적으면 같은 논의에 라운드 · 글 예산이 한 번 더 붙는다(누적: `상한 × (이어가기 수 + 1)`). 논의 하나에 최대 5번.
- 방 해산은 모든 작업의 Stop이 확인된 뒤에만 된다(`require_acknowledged`).

**근거(7.1~7.6)**: 위에 줄마다 적었다. 모두 Hermes(`H:`)다.

**가격인하에 옮길 때** (T9 · A4 · F2)

| 원본 | 판단 | 까닭 |
|---|---|---|
| 방 = append-only 이벤트 로그, 계획기 = 로그의 순수 함수, id를 로그에서 파생 | **그대로** | 꺼졌다 켜져도(E16) 같은 턴이 다시 나오고 두 번 발행되지 않는다. F2 9번(기록이 남는다)의 바탕 |
| 멤버 2~6명, 한 방 한 턴씩, 방 여러 개 병렬(4), 프로필 턴 락 | **그대로** | 담당이 딱 6명이다. 종목 여러 개가 동시에 논의될 때 같은 담당이 겹치지 않게 하는 장치가 필요하다 |
| 0라운드 = 불린 사람(없으면 전원), 뒤 라운드 = @멘션으로 불린 사람만, 라운드마다 순서 회전 | **그대로** | F2 3번("관련 담당에게 넘긴다") = 예약 게시가 @전략 @리스크를 멘션하는 것과 같다 |
| `(pass)` 침묵, 직접 불린 멤버는 넘김 금지 | **그대로** | E6(담당 응답 실패)를 "넘김"과 구분해 준다 |
| 다른 멤버의 말은 user 메시지로, 워터마크 뒤 24줄 · 128KB | **그대로** | 시스템 프롬프트를 안 건드려 캐시가 유지된다 |
| 멤버 글 속 제어 프레임 바꿔 넣기 | **그대로** | 담당 하나가 뉴스 원문을 인용하다 다른 담당을 조종하는 일을 막는다 |
| 상한 3라운드 · 10글, Continue 5번 | **줄여서** | 숫자는 그대로 쓰되 Continue는 사람이 누르는 것이라 자동 논의에는 없다. 상한에 닿으면 "합의 못 함 → 지켜보기"(E27)로 떨어뜨린다 |
| lead 모드(리드가 JSON으로 배정) | **줄여서** | 우리는 계기를 알아차린 담당(소식)이 넘길 사람을 고르면 되므로 라우팅 턴은 사용자 채팅(사람이 팀 방에 말할 때)에만 쓴다 |
| auto 모드(보조 모델 라우터) | **안 가져옴**(나중에) | 담당 6명의 역할이 고정이라 멘션과 lead로 충분하다. 싼 모델 호출 하나가 더 붙는다 |
| 작업 상태 `indeterminate` · `deferred`, 세대 울타리, 원격 peer · 첨부 · 권한 이전(`authority_epoch`) | **줄여서** | Rust 한 프로세스(D1)라 "다른 게이트웨이가 쥐고 있다"가 없다. `queued → running → settled / failed / cancelled` + 재시작 때 `running`을 다시 큐로 돌리는 것만 둔다. 다만 `execution_generation`은 둔다(늦게 온 결과 버리기) |
| 턴 시간 제한 1830초 | **줄여서** | 매매 판단은 그렇게 기다릴 수 없다. 숫자는 사용자가 정한다(끝의 "정해야 하는 것") |
| **없는 것**: 담당별 점수 · 합의 점수 · 팀장의 최종 판단 턴 | **새로 만든다** | 원본의 논의는 "조용해지면 끝"이고 결론을 내는 단계가 없다. F2 6~8번은 원본에 없는 우리 것이다. 붙일 자리: 논의가 `settled`/`bounded`로 끝난 직후, 계획기 바깥에서 ① 참가 담당마다 "점수 + 근거"를 구조화된 출력으로 받는 턴 ② 팀장의 판단 턴을 차례로 넣는다 [추정] |

### 7.7 `message_agent` (Bot Mode) [관찰]

기존 문서 `hermes-agent.md` 6.4(개요, 사슬 규칙 요약)는 다시 쓰지 않는다. 도구 스키마 · 흐름 · 반환 모양 · 숫자를 채운다.

**도구 스키마** (`H:tools/bot_mode_dm.py:70-124`). 이름 `message_agent`. 필수는 `target`뿐.

| 인자 | 타입 | 뜻 · 제약 |
|---|---|---|
| `target` | string | 명단의 이름(`researcher`), 원격 `<peer>/<agent>` 또는 `<peer>`, 여러 기기에 같은 핸들이 있으면 `<handle>@<connection>`. **쉼표로 최대 8명**(`FAN_OUT_MAX`) — 여러 명이면 답이 하나로 묶여 온다 |
| `message` | string | 보내는 쪽이 직접 쓴 글. 봉투 전체 16,000자 상한(`MESSAGE_MAX_CHARS`). "Message from …" 머리는 넣지 않는다(서버가 붙인다) |
| `constraints` | string | 답이 지켜야 할 한도(범위 · 길이 · 형식) |
| `expecting` | string | 돌려받고 싶은 것. 받는 쪽에 "네가 줘야 할 답"으로 보인다 |
| `expects_reply` | boolean | `false`면 알림만 — 답이 안 오고 보낸 쪽을 깨우지도 않는다 |
| `cancel` | string | 자기 ack의 `delivery_id` — 대기 중이면 거두고, 이미 턴을 시작했으면 멈춘다 |

**도구가 보이는 조건**: 도구 등록부에 없다. 관리되는 Bot Mode 설치에서, 그 봇의 **제목이 정확히 `Bot Chat`인 세션**에만 스키마를 끼워 넣는다. 조건이 세션 수명 동안 변하지 않으므로 도구 배열이 턴마다 같다(캐시 보호). 실행할 때도 같은 조건을 다시 본다(`:127-171, 251-256`).

**흐름**

1. 조건 확인 → 보내는 쪽 정보(`Sender`: 내 프로필, 명단, peer 목록)를 한 번 만든다(`:262-266`). 명단은 설치 루트의 프로필 목록에서 온다(`tools/bot_mode_probe._roster`. 본문은 읽지 않았다).
2. 받는 쪽이 읽는 본문 = `message` + 빈 줄 + `Constraints: …` / `Reply needed: …` 줄(`_envelope`, `:226-231`).
3. 대상 나누기 → 8명 넘으면 거절.
4. 서명 머리 `Message from 🤖 <표시 이름> (@<핸들>): `을 본문 앞에 붙인다. 알림만이면 그 뒤에 `[no reply needed]` 표식이 붙는다(`:288-293`).
5. **사슬 관문**(`H:agent/bot_chain.py:139-178`) — 전송 전에:
   - 사람 메시지가 사슬을 시작한다. 사람의 턴에서 보내면 hop 1. 봇이 쓴 턴 안에서 보내면 `보낸 수 + 1`. 여러 대상 한 번 호출은 한 hop.
   - hop이 `gateway.bot_loop_guard.max_hops`(기본 **4**)를 넘으면 호출 전체 거절("이미 한도에 닿았다, 다시 하지 말고 사용자에게 상황을 말하라").
   - 같은 사슬에서 같은 수신자에게 **같은 글**(공백을 정리한 뒤 sha256)을 또 보내면 그 수신자만 거절.
   - 이 턴을 시작시킨 봇에게 보내는 것은 "답장"이다. 기본(`bot_replies_expect_reply: false`)에서는 알림만으로 바뀐다. 상대가 "답 필요 없음"으로 보낸 글에서 시작된 턴이면 그 상대에게 다시 못 보낸다.
   - hop 수는 런타임만 쓰는 작성자 봉투 `{"id": "bot:<프로필>", "name": 핸들, "is_bot": true, "hops": n}`에 실린다(hop 1이면 생략). 모델은 못 바꾼다.
   - 사슬 상태(`hops, sent, asks, no_reply_from`)는 **메모리에만** 있고 세션 id로 찾는다. 600초(`cooldown_seconds`) 안 쓰면 지운다. 사람이 말하면 그 세션의 사슬을 지운다(`:29, 71-105`, `H:gateway/bot_loop_guard.py:33-35`).
   - 그 턴을 시작시킨 봇에게 이 턴 안에서 또 DM을 보내면 거절한다 — 이 턴의 최종 답이 어차피 그 봇에게 돌아가기 때문(`bot_mode_dm.py:356-360`).
6. **전달**(`_dispatch`, `:296-364`): 로컬 동료면 그 프로필의 `Bot Chat` 세션으로 넣는다. 그 세션을 쥔 살아 있는 프로세스가 있으면 거기에 넘기고(최대 300초 기다림, 상한 30분 — `_LIVE_WAIT_SECONDS`, `_LIVE_WAIT_MAX_SECONDS`), 없으면 그 프로필로 새 실행을 띄운다(모듈 설명 `:1-14`). 본문은 임시 폴더 `hermes-dm`의 파일로 넘기고, 24시간 지난 파일은 치운다(`:50-51`).
7. 전달은 **백그라운드 프로세스**로 돈다(`terminal_tool(background=True, notify_on_complete=…)`, `:843-847`). 도구는 바로 ack를 돌려준다.
8. 받는 쪽은 그 메시지를 user 턴으로 받아 평소처럼 한 턴을 돈다(작성자 봉투가 `turn_author`로 들어간다). **그 턴의 최종 답이 곧 답장**이다.
9. 전달 프로세스가 끝나면 보낸 쪽 세션에 "백그라운드 프로세스 완료 알림"이 들어가 **보낸 쪽을 새 턴으로 깨운다**. 알림 본문 맨 앞에 "누가 답했다 / 왜 답이 없다" 한 줄이 온다(`H:tools/bot_mode_dm_outcome.py`). 답 글은 최대 18,000자(`REPLY_COMPLETION_CHARS`)가 실린다.
10. 보낸 쪽이 완료 알림을 못 받는 세션(api_server 등)이면 ack에 `reply_delivery: "poll"`이 오고, 턴을 끝내기 전에 `process(action="wait")`로 직접 기다리라고 안내한다(`:868-880`).

**반환 JSON** (`:840, 866-895, 196-204`)

| 경우 | 모양 |
|---|---|
| 접수 | `{"status": "queued", "delivery_id", "to", "reply_delivery": "notification"|"poll", "detail", "process_id", "queued_at"}` |
| 알림만 | `{"status": "queued", "delivery_id", "to", "expects_reply": false, "detail"}` |
| 거절 · 실패 | `{"error": 문구, "reason": 분류 코드, "teammates": [명단], "peers": [...]}` (명단 · peer는 대상 문제일 때만) |

`queued`는 **백그라운드에 넘겼다는 뜻이지 배달 영수증이 아니다**. 설명 문구가 "기다리거나 조회하지 말고 턴을 끝내라"고 못 박는다.

**실패했을 때**: 대상이 명단에 없으면 명단을 실어 거절. 자기 자신이면 거절. 받는 쪽 세션에 닿을 수 없으면 "보내지 않음"으로 확정하고 사슬의 hop과 중복 기록을 되돌린다(`Admission.release`, `bot_chain.py:126-136`). 여러 대상 호출의 전체 대기 상한 32분(`FAN_OUT_WAIT_SECONDS`, `H:tools/bot_mode_dm_coordination.py:33`).

**영속성**: 대기 중 요청은 임시 파일 + 백그라운드 프로세스다. 프로세스가 죽으면 사슬 상태(메모리)는 사라진다. 재시작 뒤 대기 중 전달이 이어지는지는 확인하지 못함.

**가격인하에 옮길 때**

| 원본 | 판단 | 까닭 |
|---|---|---|
| 스키마(`target`, `message`, `constraints`, `expecting`, `expects_reply`), 서명 머리, 봉투 모양 | **그대로** | F2 3번("계기와 자료를 관련 담당에게 넘긴다")과 사용자가 한 담당에게 물었을 때 그 담당이 다른 담당에게 묻는 길 |
| 사슬 관문(hop 4, 같은 글 중복 거절, 답장은 알림만, 작성자 봉투는 런타임만 씀) | **그대로** | 담당끼리 끝없이 주고받는 것을 막는다. 사슬 상태는 우리는 DB에 둔다(재시작 대비) |
| 즉시 ack + 나중에 완료 알림으로 깨우기 | **줄여서** | 같은 프로세스 안이라 백그라운드 프로세스 · 임시 파일 · live owner 찾기가 필요 없다. "받는 담당의 세션 큐에 넣고, 끝나면 보낸 담당의 세션 큐에 완료 알림을 넣는다"로 줄인다 |
| peer · relay · 여러 기기 | **안 가져옴** | 한 컴퓨터(D2) |
| 팀 논의가 필요한 일 | `message_agent`가 아니라 **방**(7.1~7.6)으로 | 1:1은 다른 담당이 못 본다. 점수를 내려면 모두가 같은 글을 봐야 한다 |

### 7.8 `delegate_task` [관찰]

기존 문서 `hermes-agent.md` 6.3(입력 · 출력 칸, 상한, leaf 제한, background, output_schema)은 맞고 충분하다. 덧붙일 것만 적는다.

1. 자식은 **부모 스레드에서 만들고**(`_build_child_agent`, `H:tools/delegate_tool.py:204-273`) 따로 돌린다. 자식 id는 `sa-<작업 번호>-<8자>`.
2. 자식의 역할은 인자가 아니라 **깊이로 정한다**: `자식 깊이 < max_spawn_depth`이고 orchestrator 기능이 켜져 있으면 orchestrator, 아니면 leaf. `role` 인자는 무시된다(`:247-251`).
3. 자식의 시스템 프롬프트는 부모 것을 물려받지 않고 위임용으로 따로 만든다(`_build_child_system_prompt`. 본문은 읽지 않았다). 자식은 컨텍스트 파일과 고정 스킬을 읽지 않는다(2절 조각 4의 조건).
4. 물려받는 것: 부모의 공급자 · 키(설정 `delegation`에 override가 있으면 그것). `profile`을 주면 그 프로필 범위에서 만들고 **부모의 자격 풀을 쓰지 않는다**(`:239-241`).
5. 자식의 첫 user 턴(= 일 설명서)은 압축 때 끝까지 보호된다(3절 나-3). 자식의 압축 문턱 상한을 따로 줄 수 있고 16,000 토큰 아래면 설정 오류로 본다(`:163-166`).
6. 부모에게 30초마다 살아 있음을 알린다(`_HEARTBEAT_INTERVAL`). 자식이 턴 사이에서 450초, 같은 도구 안에서 1200초 동안 변화가 없으면 멈춘 것으로 보고 세운다 → `status: "timeout"`(`:107-113`, `H:tools/delegate_tool_child_run.py:658-662`).
7. 결과 항목의 `status` 정하기(`child_run.py:636-695`): interrupt면 `interrupted`(이때 `summary`는 자식이 마지막으로 쓴 진짜 글) → 실패 · 오류면 `failed` → 그 밖에 요약이 있으면 `completed`(예산을 다 써도 요약이 있으면 `completed` + `truncated: true`, `exit_reason: "max_iterations"`), 요약이 없거나 `(empty)`면 `failed`.
8. 결과 항목에는 기존 문서의 칸 말고 `api_calls, model, exit_reason, truncated, tokens{input, output}, tool_trace`(자식이 부른 도구 목록)도 있다.

**가격인하에 옮길 때**: **안 가져옴**(나중에). 담당 6명은 고정된 역할이고, 담당 안에서 일을 쪼갤 필요가 생기면 그때 넣는다. 넣는다면 "자식 역할은 깊이로", "자식 첫 턴은 압축에서 보호", "예산을 다 써도 요약이 있으면 버리지 않는다" 세 가지를 지킨다.

### 7.9 DeskRPG가 회의를 화면에 보여 주는 방식, 플러그인이 하는 일

기존 문서 `deskrpg.md` 3-2(회의 이벤트 이름), 6(회의 모델 개요)은 다시 쓰지 않는다. **가장 중요한 사실 하나와 회의 고리의 알고리즘**을 적는다.

**DeskRPG는 Hermes의 hosted rooms(`groups.*`)를 쓰지 않는다** [관찰]. 회의 순서는 DeskRPG 서버가 스스로 짠다. 근거: 회의 고리의 머리말이 "어댑터만 안다 — 게이트웨이도 소켓도 DB도 모른다"이고(`D:src/lib/conversation/channel-runtime.ts:1-9`), NPC에게 가는 호출은 `NpcAdapter.execute()`(= Hermes `/v1/runs`, `deskrpg.md` 4-6) 하나뿐이다(`D:src/lib/conversation/npc-runtime.ts:137-144, 217-219`). DeskRPG `src/` 전체와 플러그인 저장소 전체를 `groups.send · groups.create · hosted_room · room-members`로 찾아도 Hermes 방 호출은 없다(DeskRPG에서 걸리는 것은 접근 그룹 DB 표 `groups`와 자기 소켓 서버의 내부 경로 `/_internal/room-members`뿐이고, 플러그인은 0건). 플러그인 문서(`deskrpg-hermes-plugin.md` 2 · 3)의 경로 표에도 방 관련 경로가 없다. 즉 **같은 문제(여러 에이전트가 차례로 말하기)를 Hermes와 DeskRPG가 서로 다른 방식으로 풀었다.**

**DeskRPG 회의 고리** [관찰] (`D:src/lib/conversation/channel-runtime.ts:352-531`, `floor-controller.ts:109-224`, `npc-runtime.ts:123-186`, `turn-policy.ts:26-39`)

1. 한 바퀴의 순서: (첨부 대기) → 대기 명령 처리(모드 바꾸기) → **사람 발언 대기열을 기록에 붙임**(이때 연속 PASS 수를 0으로) → 다음 발언자 정하기 → 발언 → 쉬기.
2. 다음 발언자(`MeetingFloorController.next`):
   1. **지명(grant)이 먼저**다. 지명은 사람이 준 것(`direct-speak`)과 앞 발언이 멘션으로 넘긴 것 두 가지. 사람 지명은 검사 없이 통과하고, 멘션 지명은 "남은 발언 수 > 0이고 연속 실패로 빠지지 않았을 때"만 통과한다. 못 통과하면 `meeting:mention-skipped`(이유 `quota_exhausted` 또는 `backend_failing`)로 알린다.
   2. 지명이 없고 `directed` 모드면 아무도 안 부르고 사람 입력을 기다린다.
   3. 후보 = 앉아 있고 남은 발언 수가 있고 빠지지 않은 NPC. 없으면 끝(`no_candidates`. 전원이 연속 실패면 `consecutive_failures`).
   4. **손들기 폴링**: 후보 전원에게 **병렬로**(한 번에 4명씩 — Hermes `max_concurrent_runs`에 맞춘 값) 짧은 질문을 보낸다. 질문에는 주제 · **최근 3턴** · 지금 몇 턴째/전체 · 남은 발언 수가 들어간다. 답은 `SPEAK: 이유` 또는 `PASS`. 폴링은 NPC의 본 세션이 아니라 **`<세션 키>-poll`이라는 별도 세션**으로 보낸다(본 세션에 SPEAK/PASS 주고받기가 쌓이지 않게).
   5. 손든 NPC가 없으면 "전원 PASS". 있으면 그 가운데 **가장 오래 말하지 않은 NPC**(`lastSpokeAt` 최소)가 발언자다.
3. 발언(`takeTurn`): 프롬프트에 주제 · 참가자 목록(이름 · 역할) · **최근 10턴**(`historyLimit`) · 턴 번호 · 남은 발언 수를 글로 넣는다. 즉 다른 NPC의 말은 **직전 10턴의 전문**으로 전달된다. 응답은 조각마다 화면으로 흘린다.
4. 턴 시간 제한: 무응답 180초 / 전체 600초. 넘으면 그 턴만 끊고 회의는 계속한다. 실패한 턴은 기록에 남기지 않는다. 한 NPC가 **연속 3번** 실패하면 후보에서 뺀다(`MAX_CONSECUTIVE_FAILURES`, `npc-runtime.ts:36-53`).
5. 끝나는 조건(`EngineEndReason`): `max_turns`(기본 50, 회의 시작 때 `settings.maxTotalTurns`로 바꿈) / `consecutive_passes`(전원 PASS가 **2번** 연속) / `consecutive_failures`(아무에게도 닿지 못한 폴링이 3번 연속, 또는 전원 빠짐) / `no_candidates` / `stopped`. NPC 한 명당 최대 20턴. 턴 사이 1초 쉼(`D:src/server/meeting-discussion.ts:443-446, 726`).
6. "아무에게도 닿지 못한 폴링"은 "전원 PASS"와 따로 센다 — 게이트웨이가 죽은 것을 조용한 회의로 오해하지 않으려고(`channel-runtime.ts:462-480`).
7. 모드: `auto`(쉬고 계속) / `manual`(매 바퀴 뒤 사람의 `next-turn`을 기다림. 선택에 따라 일정 시간 뒤 auto로 복귀) / `directed`(폴링 없이 사람의 지명만).

**화면으로 가는 이벤트 순서** [관찰: 이름은 `deskrpg.md` 3-2, 위치는 이번 REA 출력]

`meeting:start-discussion`(C→S) → 참가 NPC를 회의 공간 자리로 모음(`meeting:spatial-state`, 전원 도착해야 시작) → 바퀴마다 `meeting:poll-status`(폴링 시작 / 결과: 손든 사람 · PASS · 실패) → `meeting:npc-turn-start{npcId, 이름}` → `meeting:npc-stream`(조각, 끝에 `done: true` — 성공 · 멘션만 남김 · 빈 응답 · 오류 · 시간 초과 **다섯 갈래 모두**에서 보낸다. 안 그러면 말풍선이 안 닫힌다, `channel-runtime.ts:49-60`) → `meeting:message`(확정된 발언) → (manual · directed면 `meeting:waiting-input`) → … → `meeting:end{이유}` → 회의록 요약 → 저장. 사람의 개입: `meeting:user-speak`, `next-turn`, `direct-speak`, `abort-turn`(`meeting:turn-aborted`), `set-mode`(`meeting:mode-changed`), `stop`.

**방 채팅에서 NPC끼리 이어 말하기**: `deskrpg.md` 6에 있다(예산 6, NPC→NPC마다 1 소모, 사람이 말하면 다시 6 — `DEFAULT_CHAT_BUDGET = 6`, `D:src/lib/conversation/chat-quota.ts:8`).

**플러그인이 하는 일**: `deskrpg-hermes-plugin.md` 2 · 3에 충분히 있다. 회의 · 대화의 **순서 결정에는 끼지 않는다**. 플러그인은 프로필 · SOUL · 스킬 · cron · 칸반 관리 경로, NPC 질문(`ask-user`) 중계, 사람 없는 실행의 승인 막힘 이벤트(`approval.blocked`), 산출물 저장을 맡는다. 대화 자체는 DeskRPG가 Hermes `api_server`를 바로 부른다.

**두 방식 비교와 가격인하에 옮길 때** (T9 · T20 · T21)

| | Hermes hosted room (7.2) | DeskRPG 회의 |
|---|---|---|
| 다음 발언자 | 로그에서 결정적으로 계산(0라운드 전원/멘션, 뒤는 멘션된 사람만) | 매 바퀴 전원에게 "말할래?" 모델 호출(폴링) |
| 비용 | 발언 턴만 | 바퀴마다 후보 수만큼 폴링 호출 + 발언 턴 |
| 침묵 | 답으로 `(pass)` | 폴링에서 `PASS` |
| 남의 말 전달 | 자기 세션에 "마지막으로 본 뒤의 글"(워터마크) | 매번 최근 10턴 전문을 프롬프트에 |
| 끝 | 조용한 라운드 · 3라운드 · 10글 | 전원 PASS 2번 · 50턴 |
| 재시작 | 로그에서 그대로 이어짐 | 메모리. 사라진다 [추정: 회의 상태를 DB에서 되살리는 코드를 보지 못했다] |
| 사람 개입 | Stop · Continue · 새 메시지 | 모드 3개 · 지명 · 다음 턴 · 턴 끊기 |

- **순서 정하기는 Hermes 방식을 따른다**(7.6의 표). 까닭: 재시작에 견디고(E16), 폴링 호출이 없어 싸고, 기록이 그대로 감사 자료가 된다.
- **화면 이벤트는 DeskRPG 방식을 따른다**(방식만 참고. Sustainable Use 라이선스라 코드 · 이름을 옮기지 않는다): "턴 시작 → 조각 → 끝(모든 갈래에서) → 확정 발언" 네 단계와, "닿지 못함을 침묵과 구분해 보여 주기". 우리 방 로그의 `turn.started / message.member / turn.settled · failed`를 이 네 단계에 맞추면 된다. 논의 장면(T20)에서 담당이 회의 자리로 모이는 것은 방에 턴이 생길 때 시작한다.
- **가져올 만한 DeskRPG 규칙 두 가지**: ① 발언자 고를 때 "가장 오래 말하지 않은 사람 먼저"(Hermes는 명단 순서를 라운드마다 한 칸 돌린다 — 둘 중 하나를 사용자가 고른다) ② 한 담당이 연속 3번 실패하면 그 논의에서 빼고 "응답 실패"로 표시(E6).


## 8. 사용자와의 대화

기존 문서에 이미 충분한 것: 소켓 이벤트와 payload(`deskrpg.md` 3-2), Hermes 호출 경로 · 세션 키 · 수명주기 상태 기계 · 큐 · 타임아웃 · 재연결(`deskrpg.md` 4-6), 승인 · 질문(`deskrpg.md` 4-7), 지시문 세 층(`deskrpg.md` 4-5), `POST /v1/runs` 6단계와 SSE 봉투(`hermes-agent.md` 5.2), 스트리밍 네 규칙(`hermes-agent.md` 5.3). 여기서는 그것들을 **한 줄로 이은 전체 흐름**과, 기존 문서에 없는 **Hermes 쪽 이벤트 순서 · 취소 · 승인 대기**만 적는다.

### 동작 원리: 화면 → 서버 → Hermes → 화면

```mermaid
sequenceDiagram
  participant U as 화면(브라우저)
  participant D as DeskRPG 서버
  participant H as Hermes api_server
  participant A as 에이전트 턴(1절)
  U->>D: npc:chat {npcId, message ≤500자}
  D->>D: 2초 간격 검사 · NPC×사용자 큐(대기 8) · requestId 발급
  D-->>U: npc:response-state (queued)
  D->>D: 사용자 줄 DB 저장 → thinking
  D->>H: 세션 없으면 POST /api/sessions {title: 세션 키}
  D->>H: POST /api/sessions/{id}/chat/stream {message, system_message}
  H->>A: run_conversation (세션 턴 lease)
  A-->>H: 글 조각
  H-->>D: assistant.delta {delta}
  D-->>U: npc:response-state (streaming, content = 누적)
  A-->>H: 도구 시작 / 진행
  H-->>D: tool.started / tool.progress {tool_name}
  D-->>U: npc:activity {activityKey}
  A-->>H: (위험한 도구) 승인 요청
  H-->>D: approval.request {command(가려짐), choices}
  D-->>U: tool-approval:request (승인자의 모든 탭)
  U->>D: tool-approval:decide {key, choice}
  D->>H: POST /v1/runs/{id}/approval {choice}
  A-->>H: 최종 답
  H-->>D: assistant.completed {content}
  D->>D: NPC 줄 DB 저장 (messageId)
  D-->>U: npc:response-state (complete, messageId) → npc:response-complete
```

(DeskRPG 쪽 단계와 이름은 `deskrpg.md` 3-2 · 4-6 · 4-7에서 가져왔다. 이 절에서 새로 확인한 것은 아래 Hermes 쪽이다.)

**Hermes run의 이벤트와 순서** [관찰] (`H:gateway/platforms/api_server_runs.py`)

1. 접수: 상태 `queued`(동시 실행 상한에 걸려 기다리면 `run.queued` 이벤트, `:1180-1182`) → 실행 시작 `run.started`.
2. 글: `message.delta {delta}` — 모델 스트림의 글 조각 그대로(`:1077`). 도구 라운드 사이에 이미 보낸 중간 글은 `message.interim {text, already_streamed}`로 한 번 더 확정해 준다(`:1087`. 화면이 초안을 문단으로 굳힐 때 쓴다).
3. 추론: `reasoning.available {text}`(`:145`).
4. 도구: `tool.started {tool, preview, tool_call_id}` → `tool.completed {tool, …, tool_call_id, is_error}`(`:141-144, 1124-1129`). `preview`는 도구 인자에서 만든 한 줄 미리보기다.
5. 승인: `approval.request` — 위험 명령 원문은 **가려서** 보낸다. payload에는 `command`, `description`(둘 다 가려진 것), 돌려보낼 수 있는 `choices`가 들어간다. 선택지는 요청에 따라 "이번 세션 동안 허용" · "항상 허용"이 빠질 수 있다(`allow_session`, `allow_permanent`, `smart_denied`, `H:gateway/platforms/api_server.py:90-102`). 이때 run 상태가 **`waiting_for_approval`**로 바뀐다(`api_server_runs.py:1009`). 답하면 `approval.responded {choice, request_id?, resolved}`(`:1410`).
6. 끝: `run.completed`(최종 글 · 사용량) / `run.failed` / `run.cancelled` 또는 `run.interrupted`.
7. 번호 · 재생 · 버퍼는 `conformance.md`의 바로잡음(0부터, 기본 `last_seq` −1, 1000개, 300초)을 따른다.

**도중에 취소** [관찰]

- 화면: `npc:cancel-response`(DM) / `room:cancel-response`(방. 보낸 사람만) → DeskRPG가 `AbortController.abort` → Hermes `POST /v1/runs/{id}/stop`(`deskrpg.md` 4-6).
- Hermes `stop`(`api_server_runs.py:1496-1530`): 이미 끝난 run이면 그 상태를 그대로 돌려준다 → 이 프로세스에 살아 있지 않으면 409 `run_not_active` → 상태를 `stopping`으로 바꾸고(`run.stopping`) 에이전트에 **강제 interrupt**를 건다 → 이 run이 띄운 백그라운드 프로세스만 정리한다 → `{run_id, status: "stopping"}`을 바로 돌려준다. 실제 멈춤은 턴 고리가 다음 확인 지점(1절: 반복 시작 · 모델 대기 중 · 도구 실행 중)에서 한다.
- 멈춘 턴의 기록: 도구 실행 중이었으면 호출마다 취소 결과가 채워져 저장된다(1절 표). 받은 글 조각이 있었으면 부분 답으로 남는다.
- 사용자가 자리를 뜨거나 소켓이 끊겨도 **run은 계속 돈다**. 다시 들어오면 `npc:response-snapshot`으로 진행 중 응답을 받는다(`deskrpg.md` 4-6). Hermes 쪽은 듣는 쪽이 없고 300초가 지나면 이벤트 버퍼를 지운다(`conformance.md`).
- 시간 제한: DeskRPG가 무응답 180초 / 전체 600초에 끊는다. **승인 대기 중에는 무응답 타이머를 멈춘다**(`deskrpg.md` 4-6).

**도구 승인 질문이 끼어들 때** [관찰 + 기존 문서]

1. 에이전트가 승인이 필요한 도구를 부르면 그 턴은 도구 실행 직전에 멈춰 기다린다. 승인 관문에는 락이 있어 병렬 도구 묶음 안에서도 한 번에 하나만 묻는다(승인 관문 락 360초 — `hermes-agent.md` 3.2).
2. `approval.request`가 스트림에 나온다 → DeskRPG가 승인자(일을 시킨 사람)의 모든 탭에 카드를 띄운다. 방 · 회의에는 "승인 대기 중"만 알린다(`tool-approval:pending`).
3. 답 `once | session | deny`가 `POST /v1/runs/{id}/approval`로 간다 → `approval.responded` → 도구가 실행되거나, 거절이면 "거절됨" tool 결과가 모델에게 간다.
4. 답이 없으면 Hermes `approvals.timeout`(DeskRPG가 플러그인으로 읽는 값, 기본 300초 — `deskrpg.md` 4-7) 뒤 거절로 처리된다. DeskRPG가 재시작되면 대기 목록(메모리)은 사라지고 Hermes가 자기 타임아웃으로 거절한다.
5. 사람이 없는 실행(cron · 칸반)은 묻지 않고 정책으로 막는다 → 플러그인 이벤트 `approval.blocked` → 시킨 사람에게만 알림(`deskrpg.md` 4-7).
6. Hermes의 위험 판정 규칙(무엇이 승인을 부르는가) 자체는 이번에 읽지 않았다. `conformance.md`의 T14 "더 뜯어야 할 것"에 그대로 남는다.

### 상태와 저장되는 것

- DeskRPG: `ChatResponse` 객체(상태가 바뀔 때마다 통째로 보냄), NPC×사용자 큐, `npc_sessions(npc_id, user_id, context_key)`에 Hermes 세션 id — 전부 `deskrpg.md` 4-6 · 5-1.
- Hermes: run 상태(`queued → running → waiting_for_approval ↔ running → stopping → completed / failed / cancelled / interrupted`. 값 목록 전체는 확인하지 못함), 이벤트 버퍼(메모리), 세션 메시지(5절).

### 실패했을 때

- Hermes가 HTTP 200으로 `run.failed`/`error`를 주면 DeskRPG는 실패로 처리하고 `ChatResponse.status = failed`(+ 오류 코드)를 보낸다(`deskrpg.md` 4-6).
- 빈 응답은 실패다. Hermes 쪽에서는 `(empty)` 표식 글이 온다(1절).
- 큐가 차면(대기 8) 받지 않고 "잠시 뒤 보내라"를 돌려준다.

### 가격인하에 옮길 때 (T21 · C5 · C2 · T14)

| 원본 | 판단 | 까닭 |
|---|---|---|
| 흐름의 뼈대: 접수 즉시 `queued` 응답 → 사용자 줄 저장 → `thinking` → 조각마다 누적 글을 통째로 보냄 → 저장한 id를 실어 `complete` | **그대로**(방식만) | 화면이 초안을 저장본으로 바꾸는 규칙까지 포함. DeskRPG 코드 · 이벤트 이름은 옮기지 않는다 |
| 담당별 큐(FIFO, 대기 8), 연속 전송 간격 | **그대로** | 사용자가 돌고 있는 담당에게 또 말할 때의 동작이 정해진다. 숫자는 그대로 |
| Hermes 이벤트 종류(시작 · 글 조각 · 중간 글 확정 · 도구 시작/끝 · 승인 요청/응답 · 끝) | **줄여서** | 서버와 실행기가 한 프로세스(D1)라 HTTP · SSE가 없다. 같은 종류의 이벤트를 내부 채널로 흘리고, WebSocket(D11 · C2)에 번호를 붙여 내보낸다 |
| 취소 = 강제 interrupt + 호출마다 취소 결과 채우기 + 부분 글 보존 | **그대로** | |
| 끊겨도 실행은 계속, 다시 들어오면 진행 중 응답을 받음 | **그대로** | 폰과 PC를 오가는 사용(F7 · F8) |
| 승인: 원문을 가려서 보냄, 선택지는 요청이 정함, 대기 중 무응답 타이머 정지, 답 없으면 거절 | **그대로** | T14와 맞물린다. 우리 승인은 6자리 재확인과 내용 지문(M5 · FN2)이 더 붙는다 |
| 도구 승인 대기 중 run 상태를 따로 둠(`waiting_for_approval`) | **그대로** | 화면이 "생각 중"과 "내 답을 기다리는 중"을 구분해야 한다 |
| 무응답 180초 / 전체 600초 | **그대로**(숫자는 조정) | E6의 판정 기준이 된다 |


## 9. OpenClaw에서 보완할 것

이 절은 **부분만 뜯었다**. 기존 문서 `openclaw.md`는 트리거 · 승인 · 기억 · 인증을 다뤘고(2.1~2.4), 턴 실행 · 에이전트 간 대화 · 세션은 비어 있다. 이번에 확인한 것은 **메시지가 겹칠 때의 큐**와 **`sessions_send`(에이전트 간 대화)의 입출력** 두 가지다. 턴 실행기 본체(embedded runner), 바인딩 라우팅, 세션 저장 모양 · 리셋 정책 · 가지치기, 압축 전 기억 저장의 발동 조건은 **이번에 읽지 못했다**(끝의 "확인하지 못한 것").

### 9.1 한 세션에 메시지가 겹쳐 올 때: 큐 모드 [관찰]

- 모드 네 가지: `steer | followup | collect | interrupt` (`O:packages/gateway-protocol/src/schema/logs-chat.ts:368`). 이름으로 본 뜻 [추정]: steer = 도는 턴에 끼워 넣기, followup = 턴이 끝난 뒤 다음 턴으로, collect = 모아서 한 번에, interrupt = 도는 턴을 끊고 새로.
- 고르는 우선순위: 메시지에 붙은 지시 → 그 세션에 저장된 모드 → 채널별 설정(`messages.queue.byChannel`) → 전역 설정(`messages.queue.mode`) → **기본 `steer`** (`O:src/auto-reply/reply/queue/settings.ts:14-19`).
- 숫자: 모으는 지연 **500ms**(`debounceMs`), 대기 상한 **20건**(`cap`), 넘칠 때 정책 기본 **`summarize`**(넘친 것을 요약해 남긴다 [추정: 이름과 export된 `trimSummaryElisionsToCap`로 본 것. 본문은 읽지 않았다]) (`O:src/auto-reply/reply/queue/state.ts:25-27`). 세션 · 채널 · 메시지별로 덮어쓸 수 있다(`settings.ts:20-35`).
- **Hermes와의 차이**: Hermes는 같은 세션의 두 번째 턴을 lease로 최대 1800초 **기다리게** 하거나(1절 가), 사용자가 명시적으로 `/steer`를 보낸다. "겹친 메시지를 어떻게 다룰지"를 설정으로 고르는 층이 없다. DeskRPG는 NPC별 FIFO 큐(대기 8)만 있다(`deskrpg.md` 4-6).
- **가져올 만한가**: **가져온다(줄여서)**. 담당이 예약 실행으로 도는 중에 사용자가 말을 걸거나 트리거가 또 오는 일이 잦다. `steer`(사용자 채팅 기본)와 `followup`(트리거 기본) 두 모드, 500ms 모으기, 상한 20을 둔다. Hermes의 steer 끼우는 자리 규칙(1절 나-3-ii)과 충돌하지 않는다 — OpenClaw가 "언제 끼울지"를, Hermes가 "어디에 끼울지"를 정한다.

### 9.2 에이전트 간 대화: `sessions_send` [관찰]

**입력** (`O:src/agents/tools/sessions-send-tool.schema.ts:5-21`): 대상은 `sessionKey` · `label` · `agentId` 가운데 하나, `message`(필수), `timeoutSeconds`(0 이상 정수), `watch`, `mode`(`notify | steer | followup | resume`).

**출력** — 상태별로 모양이 다른 합집합이다(`:30-141`).

| `status` | 뜻 | 함께 오는 칸 |
|---|---|---|
| `ok` | 상대가 답했다 | `runId, sessionKey, reply`(답 글), `replyTruncated?`, `delivery{status: pending|skipped}` |
| `no_reply` | 상대 턴은 끝났는데 답이 없다 | `runId, sessionKey, message` |
| `timeout` | 기다리다 시간 초과. **보내기는 됐다**(`sentBeforeError: true`) | `runId, sessionKey, error` |
| `accepted` | 받아들여졌고 기다리지 않는다 | `targetDisposition: queued|steered`(상대가 도는 중이면 끼워졌는지 대기열인지) |
| `queued` | 알림으로만 넣었다(턴을 시작하지 않음) | `notificationId, durability: "process", runStarted: false` |
| `error` · `forbidden` | 실패 · 금지 | `error`, `sentBeforeError?` |

- 허용 규칙: 다른 에이전트로 보내려면 `tools.agentToAgent.enabled`가 켜져 있고 `tools.agentToAgent.allow`가 (보내는 쪽, 받는 쪽)을 허용해야 한다. 아니면 `forbidden`(`O:src/agents/tools/sessions-send-executor.ts:154-164`). 세션 가시성 규칙도 따로 본다(`:257-266`).
- **Hermes와의 차이**: Hermes `message_agent`는 **항상 바로 돌아오고** 답은 나중에 새 턴으로 온다(7.7). OpenClaw는 `timeoutSeconds` 동안 **같은 도구 호출 안에서 답을 기다려** `reply`로 돌려줄 수 있고(`ok`), 0이면 Hermes처럼 넘기기만 한다. 또 상대가 도는 중일 때 "끼움 / 대기열" 가운데 무엇이 됐는지를 알려 준다.
- hop 상한 · 중복 거절 같은 고리 방지는 이번에 OpenClaw에서 찾지 못했다(옛 문서에 있던 `maxPingPongTurns`, `REPLY_SKIP`은 이 버전 소스에서 설정 이전 코드 한 곳에만 남아 있다 — `O:src/commands/doctor/shared/legacy-config-migrations.runtime.retired-media.ts`. 지금 무엇이 그 일을 하는지는 확인하지 못함).
- **가져올 만한가**: **출력 모양만 가져온다**. ① "보냈는데 답을 못 받음"(`timeout` + `sentBeforeError`)과 "못 보냄"(`error`)을 구분하는 것 ② `no_reply`를 실패와 따로 두는 것 — E6(담당 응답 실패)을 정확히 가르는 데 쓴다. 기다리는 방식(동기)은 가져오지 않는다 — 한 담당의 턴이 다른 담당을 기다리며 서 있으면 턴 시간 제한과 얽힌다. 고리 방지는 Hermes 것(hop 4 · 중복 거절)을 쓴다.

### 9.3 기존 문서에서 이어지는 것

- 인증 프로필 회전 순서와 쿨다운은 `openclaw.md` 2.4에 있다. 4.3의 Hermes 풀과 비교해 우리가 고를 것은 `conformance.md` A12 줄에 이미 있다.
- 압축 전 기억 저장(memory flush)은 `openclaw.md` 2.3에 있다. Hermes에는 이 단계가 없으므로(6절) T15에서 "압축 직전에 복기 담당에게 넘길 메모를 남길지"를 정할 때 그 절을 본다.

### 가격인하에 옮길 때 (요약)

| 무엇 | 판단 | 닿는 곳 |
|---|---|---|
| 큐 모드 `steer` / `followup`, 모으기 500ms, 상한 20 | 가져옴(줄여서) | T9 · T21 · T11 |
| `sessions_send`의 상태 구분(`ok / no_reply / timeout+sentBeforeError / accepted / error / forbidden`) | 모양만 가져옴 | T9(담당 간 넘기기) · E6 |
| 에이전트 간 허용 목록 | 가져옴(줄여서) — 담당 6명 사이는 전부 허용, 주문 도구를 가진 매매 담당에게 보내는 것만 제한할지는 사용자가 정한다 | T9 |
| 동기 대기 | 안 가져옴 | |


---

## 바로잡을 곳 (기존 조사 문서)

| 문서 · 절 | 적힌 것 | 소스 |
|---|---|---|
| `hermes-agent.md` 3.4 | 에이전트 압축 문턱 50% | 창이 512K 미만인 모델은 **75% 이상으로 올린다**. 기준도 창 전체가 아니라 `창 − 출력 예약`이고, 바닥 64K · 덮개 85%가 있다 (`H:agent/context_compressor.py:1150-1151, 2751-2797`) |
| `hermes-agent.md` 3.4 | 경계는 앞 `protect_first_n` 3개, 뒤 `protect_last_n` 20개 | 앞 보호는 **첫 압축 뒤 0으로** 줄어든다. 뒤는 메시지 수가 아니라 **토큰 예산**(창의 2.5%, 1만~2만5천)이 정한다 (`H:agent/context_compressor_window.py:33-48`, `H:agent/context_compressor_tail.py:26-40`) |
| `hermes-agent.md` 3.4 | 요약이 멈추면 한 번 재시도 뒤 결정론적 대체 요약 | 실패 종류에 따라 다르다. 인증 · 네트워크 · 잘림 · 빈 요약 · 과부하는 **압축을 그만두고 메시지를 그대로 둔다**. 과부하만 연속 3번째에 대체 요약으로 간다 (`context_compressor.py:758-812`) |
| `hermes-agent.md` 6.5 | 이벤트 종류 15개 | `turn.cancelled, turn.started, turn.reassigned, member.unavailable, authority.claimed, authority.lost`가 빠졌다. `turn.cancelled`는 "새 메시지에 밀린 턴"에 쓰이는 핵심 종류다 (`H:gateway/hosted_rooms.py:49-58`, `H:gateway/hosted_room_discussion.py:74-77`) |
| `hermes-agent.md` 6.5 | 논의 상한 3라운드 · 10글, Continue마다 한 번 더 | 맞다. 덧붙임: Continue는 논의 하나에 **최대 5번**, 멤버는 **2~6명** (`H:gateway/hosted_room_router.py:29`, `hosted_room_discussion.py:29-30`) |
| `hermes-agent.md` 3.3 | 프롬프트는 `_cached_system_prompt`에 두고 압축 뒤에만 다시 만든다 | 메모리 캐시뿐 아니라 **세션 행에 원문을 저장해 턴 사이에도 그대로 쓴다**. 다시 만드는 경우는 압축 말고도 넷 더 있다(저장값 없음, 모델 · 공급자 바뀜, Bot Chat 능력 변경, 운영자 갱신 명령) (`H:agent/conversation_loop.py:802-954`) |
| `hermes-agent.md` 3.2 | 턴 무활동 타임아웃 600초 | 맞다. 덧붙임: 감시 주기 15초, 설정 키 `agent.turn_liveness.timeout_s`, 세션 턴 lease는 300초 수명 · 60초 갱신 · 1800초 대기 (`H:agent/turn_liveness.py:23-28`, `H:agent/turn_facade_lease.py:26-27, 52`) |
| `hermes-agent.md` 6.3 | `role(leaf|orchestrator)` 인자 | 인자는 받기만 하고 **무시한다**. 역할은 깊이로 정한다 (`H:tools/delegate_tool.py:247-251`) |
| `hermes-agent.md` 6.5의 [추정] | DeskRPG 사무실 채팅은 방 로그를 그대로 보여 주면 된다 | DeskRPG는 Hermes 방을 **쓰지 않는다**. 회의 순서를 스스로 짠다(7.9). 원본 사이에 이 연결은 없으므로 우리가 새로 잇는 부분이다 |
| `openclaw.md`(해당 절 없음) | — | 에이전트 간 대화 · 큐 모드가 빠져 있다. 9절이 채운다 |

## REA를 쓴 범위와 직접 읽은 범위

| 구분 | 범위 | 얻은 것 |
|---|---|---|
| REA `analyze-javascript-application` (v6.7.0) — DeskRPG | 회의 · 채팅 파일 10개만 빈 폴더에 복사: `src/server/{meeting-discussion, meeting-socket, dm-response-runtime, room-runtime, hermes-dispatch}.ts`, `src/lib/conversation/{channel-runtime, floor-controller, open-chat-runtime, chat-quota}.ts`, `src/lib/meeting-protocol.ts` | 파싱 10/10. export 32개(회의 고리의 진입 이름 `ChannelRuntime`, `MeetingFloorController`, `registerMeetingDiscussionHandlers`, `runTrackedDm` 등), 소켓 리스너 위치(`meeting-discussion.ts:581` start-discussion, `:1028` user-speak, `:1093` stop …)와 내보내는 이벤트 이름 목록(`meeting:poll-status, npc-turn-start, waiting-input, turn-aborted, mention-skipped, mode-changed, message, end, error`). 7.9의 이벤트 순서는 이것에 기댔다 |
| REA — OpenClaw | 11개 파일: `src/agents/tools/sessions-send-{tool, tool.schema, tool.a2a, executor, reply-flow, tool.delivery, tool.steering}.ts`, `src/auto-reply/reply/queue/{settings, state, enqueue, drain}.ts` | 파싱 11/11. export 39개(`executeSessionsSend`, `runSessionsSendA2AFlow`, `enqueueFollowupRun`, `scheduleFollowupDrain`, `resolveQueueSettingsCore`, 기본값 상수 세 개). 9절에서 어느 파일을 읽을지 고르는 데 썼다 |
| REA의 한계 | 두 번 모두 `@/…` · 패키지 경로 import는 풀리지 않았고, 함수 사이 호출(`calls`)은 85건 · 72건만 풀렸다(미해결 3142 · 2999건). **숫자 · 조건 · 순서는 REA에서 나오지 않아 전부 직접 읽었다** | |
| 직접 읽음 — Hermes(Python) | REA가 호출 관계를 못 내므로 전부 직접. 읽은 파일: `agent/{conversation_loop(일부), turn_iteration_prep, turn_tool_round, turn_tool_validation(일부), turn_empty_response(일부), turn_truncation(일부), turn_facade_lease(일부), turn_api_error, turn_recovery(일부), turn_recovery_autorecover, retry_utils(일부), system_prompt(일부), prompt_builder(일부), prompt_caching(머리말), context_compressor(약 900줄), context_compressor_window, context_compressor_tail(일부), error_classifier(약 600줄), credential_pool(약 400줄), agent_runtime_helpers(일부), bot_chain}.py`, `gateway/{hosted_room_discussion, hosted_room_router, hosted_room_lead, hosted_room_context, hosted_rooms(스키마), hosted_room_driver(스키마 · 전이)}.py`, `tui_gateway/{hosted_room_driver(약 600줄), hosted_room_service(약 450줄), hosted_room_router_call}.py`, `tools/{bot_mode_dm(약 400줄), delegate_tool(일부), delegate_tool_child_run(일부), memory_tool_store(일부)}.py`, `hermes_state_messages.py(일부)`, `hermes_state_search.py(일부)`, `gateway/platforms/api_server_runs.py(일부)` | |
| 직접 읽음 — DeskRPG · OpenClaw | 위 REA 대상 가운데 `channel-runtime.ts`(약 330줄), `floor-controller.ts`, `npc-runtime.ts`(일부), `turn-policy.ts`(일부), `sessions-send-tool.schema.ts`, `queue/settings.ts`, `queue/state.ts`(상수) | |
| 직접 읽음 — 플러그인 | 이번에 새로 읽지 않았다. 방 호출이 없다는 것만 저장소 전체 검색으로 확인했다 | |

쓴 명령(두 번 모두 같은 꼴. 작업 폴더 `…/scratchpad/rea-agent-runtime/{deskrpg-chat, openclaw-runtime}/`, 출력 `out/analysis.json`):

```sh
cd <빈 폴더>/app   # 고른 파일만 원래 경로대로 복사해 둔 곳
bash -c 'ARGS=$(find src -name "*.ts" | sed "s/^/--source-path /" | tr "\n" " ");
  rea analyze-javascript-application . --artifact-format directory $ARGS --max-heap-mb 1024 --format json'
```

(zsh에서는 `$ARGS`가 낱말로 쪼개지지 않아 "Unknown flag"로 실패한다. `bash -c`로 감싸야 한다.)

조사 방법에 대한 메모: 처음에 원본 읽기를 보조 에이전트 여덟에게 나눠 맡겼으나 사용량 한도로 모두 결과 없이 끝났다. 이 문서의 내용은 전부 그 뒤에 원본을 직접 읽어 쓴 것이고, 줄 번호도 모두 직접 확인한 것이다.

## 확인하지 못한 것

1. **Hermes 턴**: 모델 응답을 기다리는 중 interrupt가 왔을 때의 정확한 처리(`handle_api_interrupt`), `run_preflight_gate`와 턴 시작 압축의 조건(`turn_context_compaction.py`), `turn_overflow.py`의 본문, stop gate의 세부.
2. **프롬프트**: 스킬 색인 조각의 줄 모양과 길이 제한(`build_skills_system_prompt`), Bot Mode 규약 절의 내용(`tools/bot_mode_probe.py`), 자식 에이전트 프롬프트(`_build_child_system_prompt`).
3. **압축**: 압축 뒤 DB에서 옛 행을 표시하는 방법(`hermes_state_compression.py`), 게이트웨이 세션 위생(85%)이 언제 무엇을 하는지, 가능성 검사(feasibility skip)의 조건, `_on_summary_failure`의 본문, 꼬리 자르기 함수(`_find_tail_cut_by_tokens`)의 세부, 요약 호출의 시간 제한.
4. **실패 처리**: `_recover_auth_failure`의 본문(401 뒤 갱신 → 바꾸기의 세부), 적응형 429 물러서기의 식, 폴백 뒤 원래 모델을 쉬게 하는 쿨다운 길이, 자격 lease의 세부, 연결 · 읽기 · 스트림 정지 타임아웃 값.
5. **세션**: 자동 리셋 정책(유휴 · 날짜)의 조건과 숫자, `end_reason` 값 전체, CJK 검색이 2글자 질의를 다루는 방법, 검색 결과 조각(snippet) 만들기.
6. **메모리**: 한 턴 안 연속 실패 상한의 숫자, 외부 메모리 공급자 인터페이스, 쓰기 전 사람 확인(기존 문서 3.6의 "쓰기 게이트")이 실제로 있는지.
7. **hosted room**: 클라이언트로 가는 실시간 통지(`groups.changed`, 진행 표시)의 payload, `groups.state` · `groups.log` 응답의 칸 전체, 방에서 멤버가 쓸 수 있는 도구 제한(`hosted_room_execution_policy.py`), 방 턴의 도구 승인(`groups.approve`)이 끼는 순서, 정책 체크포인트가 캐시하는 것, auto 라우터 시간 제한을 읽는 곳.
8. **message_agent**: 명단(`_roster`)이 만들어지는 방법, 살아 있는 세션 주인에게 넘기는 절차(`bot_live_delivery`), 재시작 뒤 대기 중 전달이 이어지는지.
9. **api_server**: run 상태 값 전체와 전이, 같은 세션에 run이 겹칠 때의 처리, `/steer` · `/clarify`의 본문, **위험 판정 규칙**(무엇이 승인을 부르는가).
10. **DeskRPG**: 회의가 서버 재시작 뒤 이어지는지(코드를 찾지 못했다), 폴링 프롬프트와 발언 프롬프트의 정확한 문구 구조(`formatPollMessage`, `formatSpeakMessage`), 회의 공간 모이기의 세부.
11. **OpenClaw**: 턴 실행기 본체, 바인딩 라우팅, 세션 저장 모양 · 리셋 · 가지치기, 압축 전 기억 저장의 발동 숫자, 하위 에이전트(`sessions_spawn`), 지금의 고리 방지 장치, 큐 넘침 정책 `summarize`의 본문.
12. **실행해 보지 않았다.** 이 문서는 전부 소스를 읽은 결과다. 원본을 돌려 실제 요청 · 이벤트를 관찰하지 않았다.

## 사용자가 정해야 하는 것

1. **팀 논의의 순서 규칙을 Hermes 방식으로 할지**(로그에서 계산: 0라운드 불린 사람, 뒤는 멘션된 사람만) **DeskRPG 방식으로 할지**(매 바퀴 손들기 폴링). 이 문서는 Hermes 방식을 권한다(7.9).
2. **점수와 판단 단계의 모양**(F2 6~8번). 원본에 없다. 제안: 논의가 끝난 직후 ① 참가 담당마다 "점수 + 근거"를 정해진 틀로 받는 턴 ② 팀장 판단 턴. 정할 것: 점수 턴을 방 로그에 넣을지 따로 둘지, 담당 하나가 실패했을 때 합의 점수를 어떻게 셀지(스펙은 "지켜보기"), 팀장이 lead 라우팅도 맡을지.
3. **턴 시간 제한.** 원본은 방 턴 1830초, DeskRPG는 무응답 180초 / 전체 600초, 턴 무활동 600초다. 매매 판단에는 길다. 담당별 · 상황별(트리거 / 사용자 채팅) 값이 필요하다.
4. **담당별 반복 상한.** 원본 기본은 500(자식 250). 담당마다 줄일 값.
5. **논의 상한을 그대로 쓸지**(3라운드 · 10글 · 6명). 상한에 닿았을 때 "합의 못 함 → 지켜보기"로 떨어뜨리는 것이 맞는지.
6. **발언 순서**: 명단 순서를 라운드마다 한 칸 돌리기(Hermes) / 가장 오래 말하지 않은 담당 먼저(DeskRPG).
7. **압축 요약의 절 틀.** 코딩용 절을 빼고 매매용 절("보유 · 주문 상태", "자료와 알게 된 시각")을 넣을지. 요약 모델을 주 모델과 다르게 둘지.
8. **겹친 입력의 기본 동작**(9.1): 도는 담당에게 사용자가 말하면 끼워 넣기(steer), 트리거가 또 오면 다음 턴으로(followup)가 맞는지. 자동 복구 사다리(4.2)를 예약 실행에만 켤지.

