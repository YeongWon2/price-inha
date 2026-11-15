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
