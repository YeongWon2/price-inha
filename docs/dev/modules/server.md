# server 모듈 (조립 · 실행 파일)

실행 파일 `price-inha`, 폴더 `crates/server/`. 모든 모듈을 쓸 수 있는 유일한 곳이고, 실행 파일은 이것 하나다.
2026-11-16에 gate 모듈에서 떼어 냈다(사용자 승인, `docs/dev/decisions.md` 2026-11-16 2번). 1~8차의 조립 작업 기록은 `docs/dev/modules/gate.md`의 "price-inha(실행 파일)가 만드는 것"과 2~8차 절에 있다. 잠긴 테스트(`crates/server/tests/`)는 그때 것을 그대로 이어받고, `red/server` 태그로 잠근다.

## 원본 동작 근거

- 대조표: `docs/dev/conformance.md` 4장 "접속 관문 · 실시간 연결 · 설정"(처음 설정 · 실행).
- 설치와 실행, 처음 설정: `docs/research/install-and-cli.md`. 로컬 접속: `docs/research/remote-access-security.md`. CLI 백엔드 조립: `docs/research/cli-backend.md` 5.6 · 5.7.
- 각 차수가 잇는 모듈의 원본 근거는 그 모듈의 지시서를 따른다.

## 앞으로의 차수 (로드맵 `docs/dev/roadmap.md` W5)

아래를 차수마다 **테스트로 먼저 쓰고** 구현한다. 새 테스트는 새 최상위 테스트 파일에 쓴다(잠긴 파일은 고치지 않는다).

### 1차: 규칙 · 주문 · 템플릿 · 멈춤 · 승인 잇기
- T6 · T7 · T8 · T13 · T14 연결: rules에만 `CheckAuthority`를 건넨다, exec · approval · template를 잇는다, 손실 · 멈춤 판단을 store 전이로.
- T18: tracing 출력에 가리기를 건다.
- C8 · C16 · E9, E19(쉬는 날에는 손절 · 청산만), V5 · V7~V9 · V31.
- gate 8차 검토의 참고: `App::shutdown` 문서 주석에 "도는 AI 고르기의 CLI 확인(최대 30초)만큼 끄기가 늦어질 수 있다"는 상한을 적는다.
- 체크포인트: V5~V9 · V24~V31 · V42~V45.

### 2차: 에이전트 · 자료 · 트리거 · 복기 잇기
- T9 · T10 · T11 · T12 · T15 연결.
- 체크포인트: V1~V4 · V16~V21 · V32~V36.

### 3차: 검색 · 백업 · 사무실 · 폰 잇기
- T16 · T19 · T20 · T21 · T22 연결, 각 크레이트의 `web_assets()`를 경로에 단다.
- V47(하루 동안 비밀이 어디에도 새지 않는 시험), V52.
- 체크포인트: V10~V15 · V22 · V23 · V37~V41 · V46~V52.

## 하지 않는 것

실제 증권사로 주문 보내기, 다른 모듈의 로직을 여기서 다시 만들기(조립만 한다).

## 테스트 주의

서버를 `127.0.0.1:0`에 띄워 실제 HTTP · 웹소켓으로 확인한다. 실행 파일 테스트는 임시 보관 폴더와 `--demo`로 띄운다. 실제 증권사 · AI 서버를 부르지 않는다.

## 필수 번호

D2 D11 C2 C15 E3 M6 V6 V31 V32 N6 N8 E38 C3 C16 E4 V46

## 트리거를 조립할 때 (trigger 3차 검토의 참고)

- `Trigger::settle()`을 `select!` 같은 취소될 수 있는 자리에 두지 않는다. 끝난 결과는 `drain_finished()`로 가져가고, 가져갈 때 tick을 한 번 깨워 실행이 끝난 작업의 기한 회차가 늦지 않게 한다.

## 사무실 화면을 조립할 때 (office 1차 · gate 9차 검토의 참고)

- 경로: `GET /api/office` → `{agents, pending, last_seq}`(마지막 번호를 **먼저** 읽고 `agents()` · `pending_responses()`를 읽는다), `POST /api/office/chat` 몸통 `{room, agent, text, sourceMessageId}`. 글 길이 상한 500자, 같은 기기 2초 간격, `sourceMessageId` 길이 상한(원본 값).
- 채팅 응답 타임아웃(무응답 180초 · 전체 600초)과 취소를 에이전트 조립에서 건다.
- 팀 방의 사용자 글도 이벤트로 낸다(다른 화면 · 새로 고침에서 질문이 보이게).
- 다른 기기에서 키를 넣는 화면: 원격은 세션 없이 401이므로 키 입력만 하는 작은 공개 쪽을 따로 둔다.

### 1차 보강 (1차 검토 되돌림)

- 그날 시작 평가액과 계좌 최고 평가액을 저장소에 남기고 열 때 되읽는다. 다시 켜도 하루 손실 한도(P3) · 최대 낙폭 전체 정지(P4)의 기준이 바뀌지 않는다. 최고 평가액은 처음 돈 아래로 내려가지 않는다. 시험: 손실을 낸 뒤 앱을 다시 띄워 남은 한도만큼만 더 잃으면 멈춘다.
- 기록 가리기는 App과 같은 비밀 보관소(`Arc`)를 쓴다. 실행 중에 등록한 키도 표준 오류 · 기록에서 가려진다(E38 · N8). 시험: 띄운 뒤 키를 등록하고 그 값을 tracing으로 찍으면 가려진다.
- 승인 길(`run_approved`)도 쉬는 날(E19) 검사를 한다: 쉬는 날로 정한 뒤에 승인된 사는 주문 · 익절은 내지 않는다.
- 참고(되돌림 아님, 2차 이후): 다시 켠 뒤 기다리던 승인은 막힘으로 정리하고 답은 `content_changed` 대신 막힘 사유로; 점검과 제출 사이 팀 상태 변화(`team_turn`); 쉬기 결정 사유를 "장 마감" 대신 맞는 사유로.
- 1차 보강 검토 참고: `take_payload`가 저장 전에 '남긴 것'으로 표시해 `append_at` 실패 때 다음 변화까지 다시 남기지 않는다(저장 성공 뒤 표시로). `TradingWiring::default`와 `default_wiring` 중복.

### 2차 보강 (2차 검토 되돌림)

- 개선안 승인은 승인 때의 지금 규칙(current)과 개선안의 after로 `pi_journal::increases_risk`를 **다시** 판정하고, 위험하면 냉각으로 보낸다(journal `approve` · `approve_as_risky`와 같은 길, 가능하면 journal의 `approve`를 그대로 부른다). 시험: 같은 규칙에서 낸 개선안 둘 중 한도를 좁히는 것을 먼저 반영한 뒤 다른 것을 승인하면 냉각중이 되고 한도가 넓어지지 않는다.
- 담당 모델 기본값은 지금 모델 `claude-sonnet-5-5`로 바꾼다(조정자 결정, 사용자가 고르는 설정은 다음 차수 후보).
- 참고(되돌림 아님): 거절 경로가 승인 모듈 거절 답의 오류를 버림(`let _`); `proposal_notices`가 이벤트를 매번 처음부터 읽음; 종목 → DART 고유번호 표가 비어 실행 파일은 공시 논의를 안 함; V32 거래량 · V35 뉴스 미덮음; 트리거 단조 시계가 실제 `Instant`.
- 2차 보강 검토 참고: `trading.params()`가 없음 · 오류이면 승인을 쓰지 않은 채 "rejected/not_found"로 답한다(승인은 남음).

### 4차 후보 (모듈 2차 검토에서)

- `team/tools.rs`의 `submit_order_draft`가 `OrderDraft.ai_quantity` · `ai_price`를 rules 요청에 넘긴다(P7 작은 쪽, 지금은 None).
- 실행 파일에 백업 설정(저장소 이름 · 수신자 공개키) 화면, 원격 기기의 키 입력용 작은 공개 쪽, 지식 검색을 에이전트 도구로 등록, 종목 → DART 고유번호 표 채우기.
- V47 하루 시험에 tracing 출력과 백업 묶음도 넣는다. 사무실 채팅 간격 표(`sent`)를 오래된 것부터 지운다.

### 후보 일괄 처리 때 다른 모듈에서 넘어온 것 (5차)
- 손절 감시 조립: 시세마다 `exec.stop_hits` → `stop_draft` → `rules::check` → `issue` → `sell(StopLoss)`. 이상 감지 뒤 팀 상태 바꾸기.
- 되살리기를 `pi_backup::restore_in`으로 옮겨 임시 폴더를 넘긴다.
- journal · agent가 가린 글만 knowledge에 넘기는지 확인(M7). 자동 회상은 `recall_by_condition`.
- 손절 주문 안은 시장가 · 상품 모름(exec 3차 결정). agent가 AI 손절값을 통화 최소 단위(미국 센트)로 넣는지 확인.
