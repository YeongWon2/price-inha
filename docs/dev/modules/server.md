# server 모듈 (조립 · 실행 파일)

실행 파일 `price-inha`, 폴더 `crates/server/`. 모든 모듈을 쓸 수 있는 유일한 곳이고, 실행 파일은 이것 하나다.
2026-11-16에 gate 모듈에서 떼어 냈다(사용자 승인, `docs/dev/decisions.md` 2026-11-16 2번). 1~8차의 조립 작업 기록은 `docs/dev/modules/gate.md`의 "price-inha(실행 파일)가 만드는 것"과 2~8차 절에 있다. 잠긴 테스트(`crates/server/tests/`)는 그때 것을 그대로 이어받고, `red/server` 태그로 잠근다.

## 원본 동작 근거

- 대조표: `docs/dev/conformance.md` 4장 "접속 관문 · 실시간 연결 · 설정"(처음 설정 · 실행).
- 설치와 실행, 처음 설정: `docs/research/install-and-cli.md`. 로컬 접속: `docs/research/remote-access-security.md`. CLI 백엔드 조립: `docs/research/cli-backend.md` 5.6 · 5.7.
- 각 차수가 잇는 모듈의 원본 근거는 그 모듈의 지시서를 따른다.

## 앞으로의 차수 (로드맵 W5 — 옛 `docs/dev/roadmap.md`, git 기록)

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
- 끝남: 테스트: crates/server/tests/round12/. "4차 후보"와 "다른 모듈에서 넘어온 것(5차)"을 처리했다(store 방 로그 마지막 번호 A… — 커밋 —(red) · —(green) · 전문: `git show 1a73576:docs/dev/modules/server.md`

## 6차 작업 (마지막 차수 검토 참고)
- 끝남: /keys · POST /api/remote/keys HTTP 경로 시험: 세션 없이 401, 다른 출처 POST 거부, 세션이면 등록되고 답에 값이 없다. — 커밋 f7b1ee0(red) · 68f68f6(green) · 전문: `git show 1a73576:docs/dev/modules/server.md`

## 7차 작업 (6차에서 드러난 빈틈: 접속 키 조립)
- 끝남: gate 9차(T17)가 만든 GateServer::access_keys()(AccessKeys)를 조립한다. 이미 이어진 것은 확인만 하고 빠진 것만 한다. — 커밋 6037e9d(red) · 6868da8(green) · 전문: `git show 1a73576:docs/dev/modules/server.md`

## 8차 작업 (바깥 키 칸 등록 · 접속 키 다듬기)
- 끝남: 사용자 말: "외부에서 가져와야 되는 키값들은 설정에 비밀값으로 넣을 수 있게 기능을 만들면 돼." — 커밋 fe2be6b(red) · —(green) · 전문: `git show 1a73576:docs/dev/modules/server.md`

## 9차 작업 (홈서버 · 미니 PC · VPN — 2026-11-21 결정 10번)
- 끝남: 사용자 말: "홈서버나 미니 PC VPN같은 환경에서 가능하게끔 되어야해 … 오픈클로나 헤르메스처럼". 화면 없는 컴퓨터에서 SSH로 설치 · 설정하고, 서비스… — 커밋 ea1aab5(red) · —(green) · 전문: `git show 1a73576:docs/dev/modules/server.md`

## 10차 작업 (2026-11-21 발표 자료 대조 — 모듈 잇기)
- 끝남: 9차(홈서버) 뒤에 한다. 모듈 차수(market 7 · data 4 · secrets 8 · rules 4 · template 9 · agent 9 · tri… — 커밋 d489c81(red) · f5564a7(green) · 전문: `git show 1a73576:docs/dev/modules/server.md`

## 11차 Vite 이전 (2026-11-23, 결정 1 · 결정 1 자리 조정)
- 끝남: 화면을 손으로 쓴 HTML/JS에서 Vite + TypeScript 다중 페이지로 옮긴다. 동작과 경로는 지금 그대로 — 이 차수는 옮기기이고, 새 화면은 사무… — 커밋 3957cd8(red) · c08b76d(green) · 전문: `git show 1a73576:docs/dev/modules/server.md`

## 12차 옛 화면 시험 옮기기 (2026-11-24, office · notify 옛 화면 정리와 함께)
- 끝남: office · notify 크레이트의 옛 화면 시험을 지우면서 화면 쪽 동작을 보는 시험이 빠진다. 같은 뜻을 server 화면 시험으로 옮긴다(동작은 이미 … — 커밋 e2dc98a(red) · a5ca033(green) · 전문: `git show 1a73576:docs/dev/modules/server.md`

## 13차 흔들리는 시험 넷 (2026-11-24 조정자, 사용자 "알아서 추천대로", QMIT 결정 7)
- 끝남: 원인 조사(부하 아래 재현 · 계측): 셋은 시험 쪽, 하나는 구현 쪽. — 커밋 53b9e02(red) · 9ca441d(green) · 전문: `git show 1a73576:docs/dev/modules/server.md`

## 14차 손실 기준 저장 순서 (2026-11-24 조정자, 13차 N3 참고 — 안전)
- 끝남: books()는 marks 잠금 안에서 저장할 손실 기준을 꺼내고(take_payload) 잠금을 푼 뒤에 저장한다(save_basis). 장부 셈 둘이 동시에… — 커밋 a2fdbfb(red) · 21ee3c4(green) · 전문: `git show 1a73576:docs/dev/modules/server.md`

## 15차 남은 후보 (2026-11-24 조정자, 사용자 "남은 후보 진행해" · "알아서 추천대로")
- 끝남: 7 · 11 · 12 · 13 · 14차 검토 참고에 남은 후보 가운데 server 화면 · 머리 · 시험 보강을 묶는다. 원본에 없는 우리 화면 동작이다(원본… — 커밋 fbd4f2b(red) · c2351a3(green) · 전문: `git show 1a73576:docs/dev/modules/server.md`

## 16차 남은 후보 — 안전 (2026-11-24 조정자, 결정 12 · 사용자 "다음 이어서 작업해")
- 끝남: 10b · 10c 검토 참고에 남은 server 쪽 안전 후보. exec 9차의 Executor::forget_pending_sell을 쓴다. — 커밋 282653a(red) · 66816c0(green) · 전문: `git show 1a73576:docs/dev/modules/server.md`

## 17차 16차 검토 참고 — 안전 (2026-11-24 조정자, 사용자 "알아서 이어서 할 일 진행해")
- 끝남: 새로 내기는 낸 뒤에 지운다: 낡은 손절 · 청산 대기를 새로 낼 때 대기를 먼저 지우지 않는다. 지금 남은 수량으로 같은 목적의 sell을 먼저 내고(실행기는… — 커밋 4cac6a3(red) · ca65400(green) · 전문: `git show 1a73576:docs/dev/modules/server.md`

## 18차 흔들리는 새 시험 (2026-11-24 조정자, 동작 잠금 — 구현은 그대로)
- 끝남: 전체 시험을 돌리면 16 · 17차 round22 시험이 가끔 실패한다. 병렬 12개로 되풀이하면(조정자 측정) hold_off::최대_손실로_전체_정지가_되면… — 커밋 e772fe3(red) · —(green) · 전문: `git show 1a73576:docs/dev/modules/server.md`

## 19차 15 · 17차 검토 참고 (2026-11-24 조정자, 사용자 "푸쉬하고 이어서 진행")
- 끝남: 15차 N3 · 17차 N3가 남긴 참고 가운데 동작이 틀린 것과 시험 빈틈. 돈이 움직이는 판단은 바꾸지 않는다. — 커밋 e618abd(red) · e39312b(green) · 전문: `git show 1a73576:docs/dev/modules/server.md`

## 20차 사무실을 원본 자산으로 (2026-11-24 조정자, 결정 13 · 사용자 "원본 캐릭터 · 사무실 · 배경을 그대로")
- 끝남: 지금 3D 사무실은 캐릭터 · 가구를 코드 도형으로 그린다. 원본 DeskRPG는 외형 50종 GLB 캐릭터(서기 · 걷기 · 앉기 애니메이션)와 배경 · 가구… — 커밋 538f97c(red) · 7e3a02d(green) · 전문: `git show 1a73576:docs/dev/modules/server.md`

## 21차 남은 후보 — 돈 · 안전 (2026-11-24 조정자, 결정 9 · 14, 사용자 "남은 작업들 다 진행해")
- 끝남: 위험을 키우는 템플릿 값 바꾸기는 승인 · 냉각을 거친다(결정 9, R14-1 · E28): adjust_template이 pi_template::plan_ad… — 커밋 89cde16(red) · 2cb7cde(green) · 전문: `git show 1a73576:docs/dev/modules/server.md`

## 22차 사무실에서 팀의 대화가 보인다 (2026-11-24 조정자, 사용자 "서로 대화하는 게 보여야" · "남은 일 한 개도 없이")
- 끝남: 지금 담당들의 논의 글(방 로그 message.member — 종목 논의 방 trade-<번호>, 하루 판단 방 day)은 저장소에만 남고 사무실 화면에 오지 … — 커밋 cab8fda(red) · a2a097b(green) · 전문: `git show 1a73576:docs/dev/modules/server.md`

## 23차 사무실 이름표 · 말풍선을 원본대로 (2026-11-24 조정자, 사용자 "말풍선이 조금 이상한데" · "DeskRPG 레퍼런스 조사해서 디테일을 잡자" · "맞춰")
- 끝남: 22차 화면을 원본 화면(README GIF)과 나란히 보니 다르다. 원본 근거는 docs/research/deskrpg.md 11절(수치) · 10절(동작)이… — 커밋 a5467c3(red) · 8931403(green) · 전문: `git show 1a73576:docs/dev/modules/server.md`

## 24차 시연 모드에서 팀이 실제로 돈다 (2026-11-25 조정자, 결정 17 — e2e 준비 중 발견)
- 끝남: 근거: docs/product/stock-team/additions.md R2-3 · D14 · C33 · E56 · N19 · V54 · T35(차수 표 1번… — 커밋 7b632fb(red) · 3fcf75d(green) · 전문: `git show 1a73576:docs/dev/modules/server.md`

## 25차 사무실 다듬기 — 순한 카메라 · 읽히는 판단 글 · 시연의 속도 (2026-11-25 조정자, 결정 18 · 24차 화면 확인)
- 끝남: 24차를 실제 서버(시연 모드)로 보니 세 가지가 어색하다. — 커밋 c74019e(red) · e7516b3(green) · 전문: `git show 1a73576:docs/dev/modules/server.md`

## 26차 공통 메뉴 · 디자인 토큰 · 관리 화면 틀 (2026-11-25 조정자, 결정 17 — 사용자 "설정값들 볼 수 있는 페이지" · "디자인 잘해줘" · "vite로 웹을 가볍게")
- 끝남: 근거: additions.md R18-1~3 · N21 · V75~V77 · T36(차수 표 2번), F12. — 커밋 c74019e(red) · e7516b3(green) · 전문: `git show 1a73576:docs/dev/modules/server.md`

## 27차 템플릿 화면 (2026-11-25 조정자, 결정 17 — 스펙 R10-1 · R10-2 · R11-2 · F6)
- 끝남: 근거: docs/product/stock-team/additions.md C28 · E58 · V79 · V80 · V81 · T39(차수 표 4번), 스펙 F… — 커밋 0d14d64(red) · 1db5ed7(green) · 전문: `git show 1a73576:docs/dev/modules/server.md`

## 28차 규칙 화면 (2026-11-25 조정자, 결정 17 — 스펙 R7-3 · F5-8)
- 끝남: 근거: additions.md C29 · V82 · T39(차수 표 5번), 스펙 F5(복기 · 개선안), backlog 묶음 F 마지막 줄(승인 카드로 개선안… — 커밋 4a21696(red) · aa3e8ad(green) · 전문: `git show 1a73576:docs/dev/modules/server.md`

## 29차 히스토리 화면 (2026-11-25 조정자, 결정 17 — 스펙 R6-3 · R4-3 · F7-7 · F7-9)
- 끝남: 근거: additions.md C30 · V83 · V84 · T39(차수 표 7번). 아래 조회는 준비돼 있다: store 18차(trade_page · tr… — 커밋 079ac88(red) · dbbd14e(green) · 전문: `git show 1a73576:docs/dev/modules/server.md`

## 30차 계좌 · 운용 화면 (2026-11-25 조정자, 결정 17 — 스펙 R3-1 · R3-3 · R16-5 · F1-6)
- 끝남: 근거: additions.md C31 · V85 · V86 · T39(차수 표 8번). 지금 있는 GET /api/team · POST /api/team/res… — 커밋 ae97502(red) · 4d82aa0(green) · 전문: `git show 1a73576:docs/dev/modules/server.md`

## 31차 설정 화면 보강 (2026-11-25 조정자, 결정 17 — 스펙 R18-4 · R16-4)
- 끝남: 근거: additions.md R18-4 · C32 · V78 · T39(차수 표 9번), server 9차 후보. — 커밋 9ce7332(red) · 67b189d(green) · 전문: `git show 1a73576:docs/dev/modules/server.md`

## 32차 내 캐릭터 · 방향키 이동 · 카메라 따라가기 · 다가가 말 걸기 (2026-11-25 조정자, 결정 17 — 사용자 "내 캐릭터도 있으면 좋을 듯. 키보드로 움직일 수 있게" · "키보드는 방향키로" · "진짜 게임처럼")
- 끝남: 근거: docs/product/stock-team/additions.md R5-4 · F10 기본 흐름 1~7 · 2a · 2b · 2c · 2d · 3a · … — 커밋 0798603(red) · 62b5137(green) · 전문: `git show 1a73576:docs/dev/modules/server.md`

## 33차 멘션 · 전체 공지 · 담당 부르기 (2026-11-25 조정자, 결정 17 — 사용자 "채팅에서 멘션 가능하게" · "팀방에서는 전체 공지사항을 전달할 수 있게")
- 끝남: 근거: docs/product/stock-team/additions.md R5-6~8 · R17 · F10 대안 4a~4e · 7a · F11 · C18 · C… — 커밋 6acdf88(red) · 77104bf(green) · 전문: `git show 1a73576:docs/dev/modules/server.md`

## 34차 승인 ✋ 말풍선 · 알림음 · 끝남 표시 · 아이콘 범례 (2026-11-25 조정자, 결정 17)
- 끝남: 근거: docs/product/stock-team/additions.md R5-10 · C27 · E49 · E50 · N12 · V62 · T28(차수 표 1… — 커밋 4eb2b71(red) · 1d999c1(green) · 전문: `git show 1a73576:docs/dev/modules/server.md`

## 35차 업종표 · 금지 종목을 실제로 잇는다 — 시연에서 주문까지 간다 (2026-11-25 조정자, 34차 검토에서 발견)
- 끝남: 근거: 결정 19번(처음 목록: "P5 · P8 · P12의 자료 — 1차는 설정 파일의 금지 종목과 업종표"), 스펙 R7-3 · F5(규칙 화면), docs… — 커밋 49e6821(red) · 4000cd4(green) · 전문: `git show 1a73576:docs/dev/modules/server.md`

## 36차 하네스: 화면 린트 · 브라우저 e2e 특성 시험 (2026-11-26 조정자, 결정 23 — 사용자 "e2e랑 Lint 정적 툴들 붙여서 기계로 검사")
- 끝남: 동작을 바꾸지 않는 차수다(동작 잠금). 지금 구현에서 모든 시험이 통과해야 한다. — 커밋 dbc9971(red) · —(green) · 전문: `git show 1a73576:docs/dev/modules/server.md`

## 37차 `price-inha setup` 마법사와 `open` (2026-11-26 조정자, 결정 24 — 사용자 "cli로 다른 컴퓨터에서 설치하고 세팅까지 도와주는 것" · "CLI는 러스트로")
- 끝남: 근거: docs/research/onboarding-messenger.md 2.1 · 2.2 · 5장 (가) · (나), docs/research/install… — 커밋 7fd9c27(red) · 57a96a1(green) · 전문: `git show 1a73576:docs/dev/modules/server.md`

## 38차 CLI가 "도는 서버"의 신원을 확인한다 (2026-11-26 조정자, 37차 push 뒤 보안 검토 지적)
- 끝남: 근거: 37차 보안 검토 — price-inha setup(과 access-key · open)은 설정의 포트에서 답하는 것이 POST /api/session에… — 커밋 1761fb1(red) · af18722(green) · 전문: `git show 1a73576:docs/dev/modules/server.md`

## 39차 메신저(텔레그램)를 잇는다 — 등록 · 페어링 · 알림 · 명령 (2026-11-26 조정자, 결정 24 — 사용자 "텔레그램, 슬랙 등등 메신저도 등록 가능하게… 메신저를 연결해서 게이트를 둘 수도")
- 끝남: 근거: 결정 24, docs/research/onboarding-messenger.md 2.4~2.6 · 5장 (다) · (라), docs/dev/modules… — 커밋 630b58a(red) · e4e9cd3(green) · 전문: `git show 1a73576:docs/dev/modules/server.md`

## 40차 사람이 내리는 전체 정지 · 알림 전달처와 조용한 시간 (2026-11-26 조정자, 결정 25 · 24)
- 끝남: 근거: 결정 25(사람이 내리는 전체 정지 — store 20차의 TeamCause::OwnerStop), 결정 24(메신저 — 알림 종류별 전달처 · 조용한 … — 커밋 08e88d9(red) · b3f7f70(green) · 전문: `git show 1a73576:docs/dev/modules/server.md`

## 41차 메신저에서 팀 대화 · 전체 정지 알림 (2026-11-26 조정자, 결정 26 — 사용자 "메신저에서도 팀들끼리 대화 주고받는 것들 되긴 해야 해" · 40차 검토)
- 끝남: 근거: 결정 26 · 24, docs/research/onboarding-messenger.md 2.5(원본의 채널로 흐르는 대화 · 채널에서 온 글을 에이전트… — 커밋 ce253ee(red) · 43d82c9(green) · 전문: `git show 1a73576:docs/dev/modules/server.md`

## 42차 병렬로 만든 기능 다섯을 합쳐 잠근다 (2026-11-26 조정자, 결정 28 — 사용자 "전부 넣어" · "병렬로 돌려. 오케스트레이션이 그걸 합쳐")
- 끝남: 동작 잠금 차수다. 기능 다섯을 별도 작업 폴더에서 red(시험 커밋) → green(구현 커밋) → verify.sh server로 만들고, 조정자가 main… — 커밋 73b706d(red) · d3a2350(green) · 전문: `git show 1a73576:docs/dev/modules/server.md`

## 43차 병렬로 만든 기능 다섯(F~J)을 합쳐 잠근다 (2026-11-27 조정자, 결정 28 — 사용자 "이어서해" · "cli 구독으로 할거야… 윈도우 컴퓨터에서 cli를 설치해서 알아서 다 setup이 되게끔")
- 끝남: 동작 잠금 차수다. 42차와 같은 방식으로 기능 다섯을 별도 작업 폴더에서 red(시험 커밋) → green(구현 커밋)으로 만들고 조정자가 main에 합쳤다.… — 커밋 2ed06b0(red) · 79ae95b(green) · 전문: `git show 1a73576:docs/dev/modules/server.md`

## 44차 별도 작업 폴더의 기능 셋(K 모이기 · L 팀 지침 · M 경량화)을 합쳐 잠근다 (2026-11-27 조정자, 결정 29 · 30 · 31 · 28)
- 끝남: 동작 잠금 차수다. 기능 셋을 별도 작업 폴더에서 red → green으로 만들고 조정자가 main에 합쳤다. 이 차수는 합친 결과가 모든 시험을 통과하는지 보… — 커밋 4979ffe(red) · de94469(green) · 전문: `git show 1a73576:docs/dev/modules/server.md`

## 45차 백로그 다섯 묶음(P · Q · R · S · T)을 합쳐 잠근다 (2026-11-27 조정자, 결정 32 · 34 · 35 · 28 — 사용자 "백로그 다 실행해" · "남은 것들만 다 실행해 key 제외하고")
- 끝남: 동작 잠금 차수다. 백로그를 다섯 묶음으로 나눠 별도 작업 폴더에서 red → green으로 만들고 조정자가 main에 합쳤다(같은 날 store 22차 · a… — 커밋 193ee1d(red) · d0a540b(green) · 전문: `git show aa0b083:docs/dev/modules/server.md`

## 46차 체험판 · 남은 백로그 · 거래량 · 논의 끝 표시를 합쳐 잠근다 (2026-11-28 조정자, 결정 36 · 28 — 사용자 "실제로 동작하는 것처럼 데모버전으로 … 똑같이 실제버전이랑" · "다 처리해" · "푸쉬까지")

동작 잠금 차수다. 별도 작업 폴더 넷에서 red → green으로 만들고 조정자가 main에 합쳤다(같은 날 market 9차 · store 23차 · backup 7차 · trigger 8차를 먼저 잠갔다).

| 묶음 | 무엇 | 근거 | 새 시험 |
|---|---|---|---|
| 체험판 | `price-inha demo [--no-open] [--demo-day-minutes <수>] [설정 인자들]` — 키 · AI · 증권사 없이 시연 서버를 켜고 브라우저로 사무실을 연다. 보관 폴더는 `<보관 폴더>-demo`(`demo.json` 표시), 기본 포트가 차 있으면 뒤 20개 → 운영체제가 고른 빈 포트. 쓰는 템플릿이 없으면 스윙 템플릿(모의중). 체험 시계로 하루 20분(장 시작 전 1분 · 장중 15분 · 마감 뒤 4분, 마감 뒤 회차 · 장 전 브리핑을 30초로 당김, 가짜 공시 2분 · 시세 250ms). 체험 폴더의 키 등록은 확인하지 않고 받아 둔다(E56 — `"trial": true`). 설정 화면에 "체험판에서는 … · 실제판에서 쓸 수 있습니다" 줄. 시연 시세의 0걸음이 오르거나 내리게(쏠림 고침 — `run --demo`에도) | 결정 36 | `round67/` · vitest `setup-trial` |
| 백로그 U | doctor가 읽기 전용 손잡이로 저장소 · 백업 상태를 읽는다(판을 올리지 않음) · 상한으로 버린 공시 · 뉴스를 "본 것"에서 빼서 다시 넘긴다 · macOS 서비스는 `--log-file`로 서버가 기록을 돌려 쓰고 켜지기 전 실패 파일은 64 KiB 상한 | 백로그 D · C | `round68/` |
| V32 | 그날 누적 거래량을 일봉으로 1분마다(시연은 받을 때마다) 읽어 `quotes` 이벤트에 `volume`, 관심 종목 표에 "거래량" 칸("1.2만주") | V32 · R12-1 | `round70/` · vitest `setup-volume` |
| 논의 끝 표시 | 논의중 매매의 마지막 논의 결과가 지켜보기 · 오류면 `watched` · `failed` 칸(`/api/trades` · `/api/trades/{id}` · 근거), `state` 거르기를 셋으로, 히스토리 "지켜봄 — <까닭>" · "논의가 끝나지 못함 — <까닭>", 근거 패널 줄. 논의 · 자료 모으기가 오류로 끝나면 방에 `room.activity {"status":"failed"}`(까닭 `discussion_error` · `gather_failed`). 상태표(M2)는 그대로 | 체험판에서 찾음 · C30 확장 | `round69/` · vitest `discussion-end` |

- 원본 동작 근거: 체험판은 결정 36(우리 것 — 실제판과 같은 조립 `run_assembly`). 백로그 U는 지시서 해당 줄 · F4 7b2. V32는 토스 공식 명세 v1.2.31(`docs/dev/toss-api-notes.md` 거래량 줄). 논의 끝 표시는 agent `watched`의 방 로그 까닭 코드.
- 시험으로 확인할 것: 위 표의 새 시험과 기존 시험이 합친 구현에서 모두 통과한다(동작 잠금).

### 대조

| 시험 · 문서 경로 | 판정 | 근거 |
|---|---|---|
| 새 시험 `round67/` ~ `round70/`, vitest `setup-trial` · `setup-volume` · `discussion-end` | 유지 | 작업 폴더에서 red 커밋으로 먼저 쓰고 green 커밋으로 통과 |
| `round16/logs.rs`의 `운영체제마다_정의가_기록_자리를_가리킨다` · `round16/service.rs`의 `macos_정의의_칸과_dry_run의_명령_순서` | 고침 | 백로그 U 5번(사용자 "다 처리해"): macOS launchd가 직접 쓰던 `StandardOutPath`를 `/dev/null`로 두고 실행 인자에 `--log-file <home>/logs/server.log`를 실어 서버가 돌려 쓴다 — 그 기대만 바꿨다(한 원소 for는 clippy 때문에 바로 단언으로) |
| 그 밖의 server 시험 | 유지 | 동작이 바뀌지 않는다 |

## 47차 시연 모드의 키 등록은 어느 길로 띄워도 바깥에 닿지 않는다 (2026-11-28 조정자, 결정 36 · E56 — 사용자 "어긋난 것들 개선해")

지금은 `price-inha demo`가 만든 체험 폴더(`demo.json`)에서만 키 확인을 가짜로 하고, `run --demo`로 띄운 서버에서는 키를 등록하면 실제 확인 서버(증권사 · 네이버 등)로 요청이 나간다 — E56("시연은 바깥으로 나가는 요청을 만들지 않는다")과 어긋난다.

1. 시연 모드면(띄운 길과 상관없이) 키 등록은 실제로 확인하지 않고 받아 둔다(칸 검사는 그대로). 답 `{"registered": true, "trial": true, "notice"}`, 설정 상태에 `"trial": true`, 설정 화면의 "체험판에서는 …" 줄도 같다. 실제판(시연 아님)은 그대로 확인한다.
2. 결정 36의 "체험 폴더의 키 등록" 문장을 "시연 모드의 키 등록"으로 넓힌다.

- 원본 동작 근거: 해당 없음(우리 것 — E56 · 결정 36).
- 시험으로 확인할 것: `run --demo`로 띄운 서버에서 증권 · 네이버 키를 등록하면 가짜 바깥 서버로 요청 0건이고 저장되며 `trial`이 붙는다 / 실제판은 지금처럼 확인 요청이 나가고 거절이면 저장하지 않는다.

### 대조

| 시험 · 문서 경로 | 판정 | 근거 |
|---|---|---|
| `round18/keys.rs`의 `네이버_키는_검색_한_번으로_확인되고_받아들여지면_저장된다` · `네이버가_키를_거절하면_저장하지_않는다` | 고침 | 사용자 "어긋난 것들 개선해" · E56: 시연에서는 확인 요청을 보내지 않으므로, 이 두 시험이 보는 "확인 요청 · 거절" 동작은 실제판의 것이다 — 발판만 `Plan::demo()` → `Plan::live()`로 바꾸고 단언은 그대로 |
| `round67/trial.rs`의 `run_demo의_폴더에는_trial이_없다` | 고침 | 결정 36 · E56: `run --demo`도 시연이므로 `trial`이 붙는다 — 단언을 "있다"로 뒤집고 이름을 `run_demo의_폴더에도_trial이_붙는다`로 |
| 새 시험 `round71/` | 유지 | 새로 쓴다(red 먼저) |
| 그 밖의 server 시험 | 유지 | 동작이 바뀌지 않는다 |
