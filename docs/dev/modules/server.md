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

D2 D11 C2 C15 E3 M6 V6 V31 V32 N6 N8 E38 C3 C16 E4 V46 V51 D1

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
- `Notifier::release_held()`를 조용한 시간이 끝날 때와 구독이 생길 때 부른다.
- 폰 화면이 가정한 경로(/api/push/key, /api/push/subscribe, /api/notices, /api/approvals/{id}/approve|reject, /api/daily-summary)를 서버 경로와 맞춘다.
- 사무실 스냅샷은 `last_seq`를 먼저 읽고 `agents()` · `pending_responses()`를 읽는다.
- notify 시험용 `redirect_to`를 시험 전용 feature로 묶는다(server Cargo.toml · 시험).
- 점검 표시: `trading/order.rs`의 `pi_rules::issue`를 `issue_for_trade(&authority, passed, trade.0)`으로 바꾸고, 사는 수량을 `set_trade_quantity`로 저장한다. 그 뒤 exec가 묶이지 않은 표시(`None`)도 막도록 exec 잠긴 시험 도우미(`common.rs` permit)를 묶인 표시로 바로잡는다.
- store 방 로그 마지막 번호 조회 API가 생기면 agent가 쓴다(지금은 500줄씩 읽음).

## 5차 작업 (후보 일괄 처리)

테스트: `crates/server/tests/round12/`. "4차 후보"와 "다른 모듈에서 넘어온 것(5차)"을 처리했다(store 방 로그 마지막 번호 API는 다음 다듬기).

- 점검 표시는 `pi_rules::issue_for_trade(&authority, passed, trade.0)`로 매매에 묶고, 사는 주문은 규칙 엔진이 정한 수량을 `set_trade_quantity`로 남긴다(승인이 붙어 잠겼으면 그대로 둔다).
- 주문 안 도구(`submit_order_draft`)는 매매 담당이 적은 `ai_quantity`를 넘기고, `ai_price`는 진입가와 견줘 작은 쪽을 지정가로 쓴다(P7). 저장소 금액은 통화 최소 단위로 읽는다(미국 센트 ÷ 100).
- 손절 감시: 시세마다 `stop_hits` → `stop_draft` → 주문 길(규칙 점검 → 묶인 표시 → `sell(StopLoss)`). 쉬는 날 검사는 건너뛰고, 이상 감지 멈춤은 주문 길이 팀 상태로 옮긴다. 같은 매매에 손절 주문이 걸려 있으면 다시 내지 않고 그 주문을 돌려준다(E30). 기준가는 그 시세를 쓴다.
- 지식 검색을 담당 도구 `knowledge_search`로 등록했다(모든 담당). 자동 회상은 같은 종목을 `recall_by_condition`으로 먼저 찾고 모자라면 키워드 자동 회상으로 채운다(둘 다 바깥 자료 조각은 넘기지 않음, M7). 복기 색인은 가린 글만 넘김을 확인했다.
- 공식 조립의 종목 → DART 고유번호 표(`known_corp_codes`, 확인한 2종목).
- 백업 설정 화면 `GET/POST /api/setup/backup`(저장소 `소유자/이름` · age 공개키, 보관 폴더 `backup-settings.json`, 다시 켜도 남고 `reach.backup`의 저장소 · 공개키보다 앞선다). 되살리기 `restore_backup`은 `pi_backup::restore_in`에 대상 옆 작업 폴더를 넘기고 끝나면 지운다.
- 구독이 생길 때와 앱 시계로 1분마다 `release_held()`를 부른다(조용한 시간이면 아무것도 안 한다). 사무실 채팅 간격 표는 간격이 지난 기기를 지운다.
- 확인만 한 것: 폰 화면 경로 6개는 서버 경로와 같다. 사무실 스냅샷은 이미 `last_seq`를 먼저 읽는다.
- 하루 시험(V47)에 기록 출력과 백업 묶음을 넣었다.
- 다른 기기의 키 입력: 공개 경로 없이 세션(TOTP 로그인) 뒤에 둔다. 화면 `GET /keys`(`web/keys.html`), 등록 `POST /api/remote/keys`(몸통 · 상한 · 답은 `/api/setup/keys`와 같고 답은 등록 여부뿐). 키 값은 저장만 하고 다시 보여 주지 않는다. 시험은 `page.rs` 단위 시험(잠긴 뒤라 HTTP 경로 시험은 다음 차수).

## 6차 작업 (마지막 차수 검토 참고)
- `/keys` · `POST /api/remote/keys` HTTP 경로 시험: 세션 없이 401, 다른 출처 POST 거부, 세션이면 등록되고 답에 값이 없다.
- `POST /api/setup/backup`은 `/api/setup/keys`처럼 그 컴퓨터에서만 받는다(원격에서 공개키를 바꾸지 못한다).
- 되살리기 중 꺼져 남은 작업 폴더는 다음 되살리기 · 켤 때 지운다.
- notify 시험용 `redirect_to`를 시험 전용 feature로 묶는다(notify Cargo.toml에 feature, server는 dev-dependency에서만 켠다). notify 쪽 고침은 같은 차수에 notify 차수로 연다.

### 6차 구현 메모
테스트: `crates/server/tests/round13/`. 위 네 항목을 모두 처리했다.

- `/keys` · `POST /api/remote/keys` HTTP 경로 시험: 세션 없이 401(확인기를 부르지 않음), 다른 출처 POST는 4xx, 세션이면 `{"registered": true}`만 돌려주고 키 값이 없다.
- `POST /api/setup/backup`은 `/api/setup/keys`와 같은 장치(`pi_gate::local_only`)로 그 컴퓨터에서만 받는다. 원격 세션은 `GET`은 되고 `POST`는 403(`LOCAL_ONLY_STATUS`). 같은 경로를 `GET` 라우터와 `local_only`로 감싼 `POST` 라우터로 나눠 합친다.
- 되살리기 작업 폴더(`<보관 폴더>.restore-work`)는 되살리기 앞뒤로 지우고, 켤 때(`App::assemble` 처음)도 지운다.
- notify의 `redirect_to`는 `test-redirect` feature 뒤에 있고, server는 dev-dependency에서만 켠다(일반 의존성은 그대로).
- 시험의 원격 세션은 저장소에 접속 키 해시를 직접 두고 그 키로 들어간다(dev-dependency `sha2`). 서버에는 아직 접속 키를 만드는 길이 없다(다음 차수 후보: 접속 키 발급 화면 · 명령).

## 7차 작업 (6차에서 드러난 빈틈: 접속 키 조립)
gate 9차(T17)가 만든 `GateServer::access_keys()`(`AccessKeys`)를 조립한다. 이미 이어진 것은 확인만 하고 빠진 것만 한다.
- 접속 키 발급: 그 컴퓨터에서만(`pi_gate::local_only`) `POST`로 새 키를 만들고 답에 **한 번만** 보여 준다(기록 · 다른 응답에 다시 나오지 않는다). 설정 화면에 "접속 키 만들기/바꾸기" 칸. 바꾸면 앞 세대 원격 세션이 끊긴다(V41).
- 잠금 풀기: 그 컴퓨터에서만 막힌 기기를 푼다(N3).
- 잠금 알림: gate 잠금 알림 포트를 notify로 잇는다(E2, 조용한 시간을 무시하는 종류).
- 접속 기록 보기: 그 컴퓨터 · 세션에서 기기 · 시각을 본다(V40).
- 시험: 루프백에서 발급 → 그 키로 원격 세션이 생긴다(시험이 해시를 직접 두지 않는다), 원격에서 발급 · 풀기는 403, 답 말고는 키가 어디에도 안 남는다.

### 7차 구현 메모
테스트: `crates/server/tests/round14/`. 코드: `crates/server/src/access_http.rs`, 조립은 `app/parts.rs`의 `open_gate`.

- 경로: `POST /api/access/key`(그 컴퓨터, 답 `{"access_key", "generation"}`, `no-store`) · `POST /api/access/unlock`(그 컴퓨터, 본문 `{"device": "<IP>"}`, 답 `{"unlocked": bool}`, IP가 아니면 400) · `GET /api/access/log`(세션, 기록 새것부터 200줄과 `locked` 기기 목록).
- `AccessKeys`는 관문을 연 뒤에야 얻으므로 경로는 빈 자리(`OnceLock`)를 쥐고 `serve` 뒤 채운다. 채우기 전 요청은 503.
- 잠금 알림(E2)은 5차에서 이미 이어져 있었다(`reach::LockNotices` → notify `AccessLock`, round11 시험). 확인만 했다.
- 설정 화면에 "접속 키" 칸: 만들기/바꾸기 단추(키는 답에서 한 번만 보이고 화면에 간직하지 않음), 잠긴 기기와 풀기 단추, 접속 기록.

### 7차 검토 참고 (다듬기 후보)
- `access_log`가 기록 전체를 읽고 200줄로 자른다(store에 최근 n줄 조회).
- 발급한 키가 다음 동작 · 새로 읽기 전까지 화면(DOM)에 남는다(일정 시간 뒤 지우기).
- 채우기 전 503 경로 시험이 없다.

## 8차 작업 (바깥 키 칸 등록 · 접속 키 다듬기)

사용자 말: "외부에서 가져와야 되는 키값들은 설정에 비밀값으로 넣을 수 있게 기능을 만들면 돼."

- 키 등록 경로(`/api/setup/keys` 그 컴퓨터 · `/api/remote/keys` 세션)에 `item` `dart` · `ecos` · `fred` · `backup_github_token`(공개 상수 `KEY_DART` 등 · `KEY_SLOTS`)을 더했다. 칸 이름은 `KeySlot::fields()`, 몸통 · 상한 · 공백 떼기 · 값 지우기 · 답(등록 여부뿐) · 시도 횟수는 기존 키와 같다. 저장은 `SecretStore::register_slot`.
- 확인: `KeyVerifier::verify_slot` — 자료 칸은 `pi_data::verify_slot`(팀 조립의 `TeamWiring.data` 주소), 토큰은 `pi_backup::verify_token`(3차 조립 `reach.backup.api_base`, 없으면 GitHub 기본 주소). 주소가 없는 쪽(팀 · 3차 조립 없이 띄움)은 확인하지 않아 저장되지 않는다(`Unreachable`). `KeyVerifier::new(Upstreams)`는 그대로 두고 `with_data` · `with_github`를 더했다(앞 차수 시험이 `Upstreams`를 글자 그대로 만든다).
- 칸별 상태: `GET /api/setup/slots`(세션) → `{"slots": [{"name", "registered"}]}`. **`GET /api/setup`에 넣지 않은 까닭**: 앞 차수 잠긴 시험(setup/mod.rs)이 그 답의 칸을 못 박아 둔다. 자료 키 · 토큰은 시작 조건이 아니다.
- 화면: 설정 화면 "바깥 키 (선택)" 칸과 `/keys` 화면에 칸마다 입력(password · new-password · spellcheck=false) · 등록 단추 · 등록 여부. 성공하면 입력을 비운다.
- `access_log`는 `Store::access_recent(200)`을 쓴다(전체를 읽지 않는다).
- 발급한 접속 키는 60초(`ACCESS_KEY_SHOW_MS`) 뒤 화면에서 지운다(다시 만들면 앞 타이머를 지운다).
- 접속 키 자리를 채우기 전 503: `access_http.rs` 안 단위 시험(관문을 연 뒤 바로 채워 HTTP로는 그 틈을 만들 수 없다).
- 시험: `tests/round15`(가짜 자료 · GitHub 서버, 가짜 값).

### 8차 검토 참고
- 옛 `item` `data`(`KeyKind::Data`, 확인할 곳 없음)는 그대로 둔다. 새 칸과 겹치므로 다음 차수에 화면에서 뺄지 정한다(물음).

## 9차 작업 (홈서버 · 미니 PC · VPN — 2026-11-21 결정 10번)

사용자 말: "홈서버나 미니 PC VPN같은 환경에서 가능하게끔 되어야해 … 오픈클로나 헤르메스처럼". 화면 없는 컴퓨터에서 SSH로 설치 · 설정하고, 서비스로 늘 돌고, VPN(Tailscale 등, D2대로 사용자가 고름) 너머 기기에서 접속 키로 들어오게 한다. D2(기본은 `127.0.0.1`, 밖은 접속 키)는 바꾸지 않는다.

### 원본 동작 근거
- 서비스: `docs/research/install-and-cli.md` 2.4(Hermes) · 3.4(OpenClaw). 진단: 2.6 · 3.6. 처음 설정 · 일회용 토큰: 2.3 · 3.3. 제거: 2.5 · 3.5. 파일 배치: 2.7 · 3.7.
- 접속: `docs/research/remote-access-security.md` 4.1(서버는 루프백, 바깥은 Serve 같은 프록시가 연다), D2.

### 명령 (하위 명령. 인자 없이 띄우는 지금 동작은 그대로)
| 명령 | 하는 일 |
|---|---|
| `price-inha run [지금 인자들]` | 지금과 같다(포그라운드). 인자 없는 `price-inha`도 그대로 run |
| `price-inha service install [--start-now] [--dry-run]` | 사용자 서비스 정의를 쓰고 등록한다. 정의는 `run --home <절대 경로> [지금 준 --bind · --port 등]`을 띄운다 |
| `price-inha service uninstall [--dry-run]` | 멈추고 정의를 지운다. 보관 폴더는 남긴다(Hermes "Keep data") |
| `price-inha service start \| stop \| restart \| status` | 서비스 관리자를 통해서. `status`는 등록 여부 · 도는지 · 정의가 가리키는 홈 |
| `price-inha doctor [--json]` | 아래 진단. 문제가 남으면 종료 코드 1 |
| `price-inha access-key [--rotate]` | 접속 키를 만들어 **터미널에 한 번만** 보여 준다(화면의 `/api/access/key`와 같은 저장소 · 같은 규칙, 결정 8번). 바깥 주소가 있으면 `https://<원격 이름>/` 안내를 함께. 화면 없는 컴퓨터의 처음 설정은 SSH로 이것을 부르고, 다른 기기 브라우저에서 키로 들어와 설정 화면(원격 키 등록 `/api/remote/keys`)을 쓴다 |
| `price-inha restore <백업 파일> --identity <age 비밀키 파일> --into <빈 폴더>` | `restore_backup`을 부른다. 빈 폴더가 아니면 거부. 끝나면 "팀은 멈춤, 기다리던 승인은 만료(V51 · M5)"와 다음에 띄울 명령을 보여 준다. 비밀키 파일 내용은 어디에도 남기지 않는다 |
| `price-inha --version` | 설정 · 기록을 읽기 전에 답한다(Hermes 2.2) |

### 서비스 정의 (원본 표 그대로, 이름만 우리 것)
| | macOS | Linux | Windows |
|---|---|---|---|
| 정의 | `~/Library/LaunchAgents/kr.priceinha.server.plist` 사용자 LaunchAgent | `~/.config/systemd/user/price-inha.service` 사용자 unit | 예약 작업 `PriceInha` (`schtasks /Create /XML`, 최소 권한) |
| 자동 시작 | `RunAtLoad=true` | `WantedBy=default.target` + `loginctl enable-linger` 시도(실패하면 수동 명령 안내) | 로그온 트리거(30초 지연) |
| 되살리기 | `KeepAlive={SuccessfulExit:false}`, `ThrottleInterval=30` | `Restart=on-failure`, `RestartSec=5`. 설정 치명 오류 종료 코드(지금 2)는 `RestartPreventExitStatus=2` | 실패 시 1분 간격 재시도 |
| 멈춤 | `launchctl bootout`(정의를 내린다. KeepAlive가 되살리지 않게) | `systemctl --user stop` | `schtasks /End` |
| 기록 | `<home>/logs/server.log` · `server.error.log` | journal | `<home>/logs/` |
- 작업 폴더는 보관 폴더(사라질 수 있는 체크아웃을 피함). 환경에는 `PATH`만 명시, 비밀 값은 정의에 넣지 않는다(키는 보관 폴더에서 읽는다).
- 다른 홈을 가리키는 기존 정의는 덮어쓰지 않고 거부한다(Hermes 2.4). 같은 홈이면 새로 쓴다(업데이트 뒤 낡은 경로를 피함).
- 바깥 명령(`launchctl` · `systemctl` · `loginctl` · `schtasks`)은 `CommandRunner` trait 뒤에서 부른다. 테스트는 가짜 실행기로 부른 명령 줄과 쓴 정의 파일만 본다(**실제 서비스 관리자를 부르지 않는다**). 정의를 쓰는 폴더도 인자로 받는다(테스트는 임시 폴더).
- 운영체제 판별은 `cfg!(target_os)` 기본값 + 테스트용 인자.

### 진단 (`doctor`)
순서대로 보고, 항목마다 `✓ / ✗ 무엇 · 고치기` 한 줄(Hermes 2.6처럼 사람이 할 것을 번호로). 자동으로 고치지 않는다(이번에는 `--fix` 없음).
1. 보관 폴더가 있고 0700, 비밀 파일 0600(유닉스).
2. SQLite가 열리고 `PRAGMA integrity_check`가 ok, 스키마 버전이 이 실행 파일이 아는 버전.
3. 키 칸마다 등록 여부(이름만, 값 · 길이 · 앞뒤 글자 금지).
4. 여는 주소 · 포트를 쓸 수 있는가(이미 도는 서버가 있으면 그 사실만).
5. 루프백이 아닌 주소에 열리는데 접속 키가 없는가 → ✗.
6. 서비스 등록 여부와 정의가 가리키는 홈이 이 홈인가, Linux면 linger.
7. 백업 설정이 켜졌으면 마지막 백업 시각.
- 진단은 실제 증권사 · AI · GitHub를 부르지 않는다(바깥 연결 확인은 하지 않음 — 이번 범위 밖).

### 테스트로 확인할 것
- `run` 없이 띄운 지금 동작과 `run`이 같다(앞 차수 시험을 깨지 않는다).
- 운영체제 셋마다 `service install --dry-run`이 쓰는 정의의 칸(위 표)과 부르는 명령 순서. 다른 홈 정의가 있으면 거부. `uninstall`은 멈춤 → 정의 삭제, 보관 폴더는 남음.
- `doctor`: 권한이 넓으면 ✗와 고치기, 깨진 DB면 ✗, 키 값이 출력에 0번(N8), 문제 있으면 종료 코드 1.
- `access-key`: 한 번만 보여 주고 저장은 해시(화면 경로와 같은 저장소), 두 번째는 `--rotate` 없이는 거부, 출력 밖(기록)에 키 글자 0번.
- `restore`: 빈 폴더가 아니면 거부, 되살린 폴더로 띄우면 팀 멈춤 · 승인 만료(V51), 비밀키 파일 글자가 기록에 0번.
- `--version`이 보관 폴더 없이도 답한다.

### main.rs 주석
- "백업 저장소는 아직 고를 화면이 없어 잇지 않는다"는 지금 코드(설정 화면 `backup-settings.json`)와 다르다. 맞게 고친다.

### 하지 않는 것 (이번 차수)
- Tailscale · 방화벽 설정(사용자가 고른다, D2). 자동 업데이트(결정 "알리기만" 추천이 아직 미정). 설치 스크립트(다음 차수 후보). `doctor --fix`.
